//! One play of one chart: owns the engine `Player`, the audio backend, the
//! keyboard and the bindings, and advances them once per animation frame.

use std::cell::Cell;
use std::rc::Rc;

use ddi_chart::{Layout, NoteKind, Song};
use ddi_engine::input::{Bindings, InputEvent};
use ddi_engine::judge::{JudgeEvent, JudgeEventKind};
use ddi_engine::player::{PlayOptions, Player, Results};
use ddi_engine::rules::{JudgeNames, Ruleset};
use ddi_platform::{AudioBackend, DeviceId, DeviceProfile, HostTime, InputSource};
use ddi_render::RenderOptions;
use gloo::events::EventListener;
use wasm_bindgen::JsCast;
use web_sys::AudioBuffer;

use crate::calibration::{self, CalMode, Calibrator, Outcome};
use crate::settings::Settings;
use crate::web::audio::WebAudio;
use crate::web::keyboard::WebKeyboard;

/// Lead time between scheduling the start and the first audio sample, so the
/// buffer source starts exactly on the audio clock.
const START_LOOKAHEAD: f64 = 0.8;
/// Music fade after an immediate fail, and the pause before the results.
const FAIL_FADE: f64 = 1.2;
const FAIL_HOLD: f64 = 1.8;
/// Cancel: a second press within this many seconds, or one press held this
/// long. A single tap only shows the hint.
const CANCEL_DOUBLE_TAP: f64 = 0.5;
const CANCEL_HOLD: f64 = 0.7;
/// How long the hint stays after a single tap.
const CANCEL_HINT: f64 = 1.5;

fn is_cancel_key(code: &str) -> bool {
    code == "Escape" || code == "Backspace"
}

/// Everything about a session that is not the song.
pub(crate) struct SessionConfig {
    pub(crate) ruleset: Ruleset,
    pub(crate) options: PlayOptions,
    pub(crate) volume: f32,
    pub(crate) auto: bool,
    pub(crate) render: RenderOptions,
    /// Converging calibration test, if this session is one.
    pub(crate) calibration: Option<CalMode>,
    /// Autoplay presses this many seconds late (negative = early); testing aid.
    pub(crate) auto_bias: f64,
    /// Devices the session runs on (selects the offsets).
    pub(crate) devices: Option<DeviceProfile>,
}

impl SessionConfig {
    pub(crate) fn from_settings(
        settings: &Settings,
        auto: bool,
        devices: Option<DeviceProfile>,
    ) -> SessionConfig {
        SessionConfig {
            ruleset: settings.ruleset(),
            options: settings.play_options(devices.as_ref()),
            volume: settings.volume,
            auto,
            render: RenderOptions {
                reverse: settings.reverse,
                hide_notes: false,
                show_deltas: settings.show_deltas,
                cancel_progress: 0.0,
            },
            calibration: None,
            auto_bias: 0.0,
            devices,
        }
    }
}

pub(crate) struct PlaySession {
    pub(crate) player: Player,
    pub(crate) layout: Layout,
    pub(crate) names: JudgeNames,
    pub(crate) render: RenderOptions,
    audio: WebAudio,
    keyboard: Option<WebKeyboard>,
    bindings: Bindings,
    held: Vec<bool>,
    raw: Vec<ddi_platform::RawInput>,
    /// Autoplay: scripted presses at each note's judged time.
    auto: Option<AutoPlay>,
    aborted: bool,
    interrupted: Rc<Cell<bool>>,
    _interrupt_listeners: Vec<EventListener>,
    last_frame_host: Option<f64>,
    frame_interval: f64,
    /// Host time at which an immediate fail was noticed.
    failed_at: Option<f64>,
    /// Signed error of the last judged tap, seconds.
    pub(crate) last_delta: Option<f64>,
    calibration: Option<(CalMode, Calibrator)>,
    /// Host time of the last cancel-key press, and when it was released.
    cancel_pressed_at: Option<f64>,
    cancel_held: bool,
    /// Set when the audio device list or the display scale changed mid-play.
    device_changed: Rc<Cell<bool>>,
    _device_listeners: Vec<EventListener>,
    _dpr_query: Option<web_sys::MediaQueryList>,
    pub(crate) devices: Option<DeviceProfile>,
}

/// Which cue a simulated player follows. A human reacts to what they hear
/// or see, not to the judged timeline, so corrections to the offsets must
/// change when the autoplayer presses the way they change a human's timing.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Cue {
    /// The physical audio: judged time plus the audio offset.
    Audio,
    /// The drawn arrows: judged time plus the visual offset.
    Visual,
}

struct AutoPlay {
    /// (press time, release time, lane), sorted by press time.
    events: Vec<(f64, f64, u8)>,
    next: usize,
    pending_release: Vec<(f64, u8)>,
    cue: Cue,
}

pub(crate) enum SessionEvent {
    Finished {
        results: Box<Results>,
        calibration: Option<Outcome>,
        /// An audio device or the display changed during the play.
        device_changed: bool,
    },
    /// The player pressed Escape.
    Aborted,
    /// The tab was hidden or the audio context was suspended mid-play.
    Interrupted,
}

impl PlaySession {
    /// Builds the session and schedules the start on the audio clock.
    /// `audio` must have been created inside a user gesture.
    pub(crate) fn start(
        song: &Song,
        chart_index: usize,
        settings: &Settings,
        config: SessionConfig,
        mut audio: WebAudio,
        buffer: AudioBuffer,
    ) -> Result<PlaySession, String> {
        let chart = song
            .charts
            .get(chart_index)
            .ok_or_else(|| format!("chart {chart_index} missing"))?;
        let layout = Layout::builtin(&chart.layout)
            .ok_or_else(|| format!("layout {} not playable", chart.layout))?;
        let names = config.ruleset.names.clone();
        let any_key = config.calibration.is_some_and(|m| m.any_key());
        let bindings = if layout.id == "dance-single" {
            let mut b = Bindings::default();
            for (lane, codes) in settings.keys_single.iter().enumerate() {
                for code in codes {
                    let lane = if any_key {
                        calibration::AUDIO_LANE
                    } else {
                        lane as u8
                    };
                    b.bind(&DeviceId::Keyboard, code, lane);
                }
            }
            b
        } else {
            Bindings::from_layout(&layout)
        };
        let mut capture: Vec<String> = bindings
            .bindings
            .iter()
            .map(|b| b.control.clone())
            .collect();
        capture.extend(
            [
                "ArrowLeft",
                "ArrowRight",
                "ArrowUp",
                "ArrowDown",
                "Space",
                "Backspace",
            ]
            .iter()
            .map(|s| s.to_string()),
        );
        let keyboard = WebKeyboard::new(capture);
        let interrupted = Rc::new(Cell::new(false));
        let mut interrupt_listeners = Vec::new();
        if let Some(document) = web_sys::window().and_then(|w| w.document()) {
            let flag = interrupted.clone();
            let doc = document.clone();
            interrupt_listeners.push(EventListener::new(
                &document,
                "visibilitychange",
                move |_| {
                    if doc.hidden() {
                        flag.set(true);
                    }
                },
            ));
        }

        let timing = chart.timing(song).clone();
        let auto = config.auto.then(|| {
            let mut events: Vec<(f64, f64, u8)> = chart
                .notes
                .iter()
                .filter(|n| timing.judgeable(n.tick))
                .filter_map(|n| match n.kind {
                    NoteKind::Tap | NoteKind::Lift => {
                        let t = timing.seconds_at(n.tick) + config.auto_bias;
                        Some((t, t + 0.06, n.lane))
                    }
                    NoteKind::HoldHead { end } | NoteKind::RollHead { end } => Some((
                        timing.seconds_at(n.tick),
                        timing.seconds_at(end) + 0.02,
                        n.lane,
                    )),
                    _ => None,
                })
                .collect();
            events.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap());
            AutoPlay {
                events,
                next: 0,
                pending_release: Vec::new(),
                cue: if config.calibration.is_some_and(|m| m.muted()) {
                    Cue::Visual
                } else {
                    Cue::Audio
                },
            }
        });

        let mut player = Player::new(song, chart_index, config.ruleset, config.options);
        let sample = audio.now();
        player.clock_sample(sample);
        let start_ctx = sample.context_time + START_LOOKAHEAD;
        audio.play(&buffer, start_ctx, 0.0, config.volume)?;
        player.start(start_ctx, 0.0);
        {
            let flag = interrupted.clone();
            interrupt_listeners.push(EventListener::new(
                audio.context(),
                "statechange",
                move |event| {
                    if let Some(ctx) = event
                        .target()
                        .and_then(|t| t.dyn_into::<web_sys::AudioContext>().ok())
                        && ctx.state() != web_sys::AudioContextState::Running
                    {
                        flag.set(true);
                    }
                },
            ));
        }
        // Device changes mid-play: audio device list (connect/disconnect,
        // no permission needed) and display scale (window moved screens).
        let device_changed = Rc::new(Cell::new(false));
        let mut device_listeners = Vec::new();
        if let Some(md) = web_sys::window().and_then(|w| w.navigator().media_devices().ok()) {
            let flag = device_changed.clone();
            device_listeners.push(EventListener::new(&md, "devicechange", move |_| {
                flag.set(true)
            }));
        }
        let dpr_query = {
            let flag = device_changed.clone();
            crate::web::display::dpr_change_listener(move || flag.set(true)).map(
                |(query, listener)| {
                    device_listeners.push(listener);
                    query
                },
            )
        };
        let lanes = layout.lanes.len();
        Ok(PlaySession {
            player,
            layout,
            names,
            render: config.render,
            audio,
            keyboard,
            bindings,
            held: vec![false; lanes],
            raw: Vec::new(),
            auto,
            aborted: false,
            interrupted,
            _interrupt_listeners: interrupt_listeners,
            last_frame_host: None,
            frame_interval: 1.0 / 60.0,
            failed_at: None,
            last_delta: None,
            calibration: config.calibration.map(|m| (m, Calibrator::default())),
            cancel_pressed_at: None,
            cancel_held: false,
            device_changed,
            _device_listeners: device_listeners,
            _dpr_query: dpr_query,
            devices: config.devices,
        })
    }

    /// Feeds a hit to the calibrator and applies any correction it asks for.
    fn calibrate_hit(&mut self, delta: f64) {
        let Some((mode, cal)) = self.calibration.as_mut() else {
            return;
        };
        if let Some(step) = cal.record(delta) {
            let mut opts = *self.player.clock().options();
            match mode {
                CalMode::Combined | CalMode::Audio => opts.audio_offset += step,
                CalMode::Visual => opts.visual_offset += step,
            }
            self.player.set_clock_options(opts);
        }
    }

    pub(crate) fn calibration_outcome(&self) -> Option<Outcome> {
        let (_, cal) = self.calibration.as_ref()?;
        let t = self.player.tally();
        Some(cal.outcome(t.taps[0], t.taps[5]))
    }

    /// Advance to the current host time. Returns judge events for effects.
    pub(crate) fn tick(&mut self, host_now: HostTime) -> (Vec<JudgeEvent>, Option<SessionEvent>) {
        let mut events = Vec::new();
        if let Some(last) = self.last_frame_host {
            let dt = host_now.0 - last;
            if dt > 0.0 && dt < 0.25 {
                self.frame_interval = self.frame_interval * 0.9 + dt * 0.1;
            }
        }
        self.last_frame_host = Some(host_now.0);
        self.player.set_frame_interval(self.frame_interval);

        self.player.clock_sample(self.audio.now());

        self.raw.clear();
        if let Some(kb) = self.keyboard.as_mut() {
            kb.poll(&mut self.raw);
        }
        let mut raw_inputs = std::mem::take(&mut self.raw);
        for raw in raw_inputs.drain(..) {
            if raw.device == DeviceId::Keyboard && is_cancel_key(&raw.control) {
                if raw.pressed {
                    let now = raw.host_time.0;
                    if self
                        .cancel_pressed_at
                        .is_some_and(|t| now - t <= CANCEL_DOUBLE_TAP)
                    {
                        self.aborted = true;
                    }
                    self.cancel_pressed_at = Some(now);
                    self.cancel_held = true;
                } else {
                    self.cancel_held = false;
                }
                continue;
            }
            if let Some(ev) = self.bindings.resolve(&raw) {
                self.apply(ev, &mut events);
            }
        }
        self.raw = raw_inputs;

        // Hold-to-quit progress and the hint after a single tap.
        self.render.cancel_progress = match self.cancel_pressed_at {
            Some(t) if self.cancel_held => {
                let p = (host_now.0 - t) / CANCEL_HOLD;
                if p >= 1.0 {
                    self.aborted = true;
                }
                p.clamp(0.0, 1.0) as f32
            }
            Some(t) if host_now.0 - t <= CANCEL_HINT => 0.001,
            _ => 0.0,
        };

        if let Some(auto) = self.auto.as_mut() {
            let opts = *self.player.clock().options();
            // Song time of the cue the simulated player follows, right now.
            let heard = self.player.clock().heard_now(host_now)
                + match auto.cue {
                    Cue::Audio => opts.audio_offset,
                    Cue::Visual => opts.visual_offset,
                };
            let mut pending = Vec::new();
            while auto.next < auto.events.len() && auto.events[auto.next].0 <= heard {
                let (t, rel, lane) = auto.events[auto.next];
                auto.next += 1;
                pending.push((t, lane, true));
                auto.pending_release.push((rel, lane));
            }
            auto.pending_release.retain(|&(rel, lane)| {
                if rel <= heard {
                    pending.push((rel, lane, false));
                    false
                } else {
                    true
                }
            });
            for (song_t, lane, pressed) in pending {
                // Convert the song time back to a host time by offsetting from now.
                let host = HostTime(host_now.0 - (heard - song_t));
                self.apply(
                    InputEvent {
                        lane,
                        pressed,
                        host_time: host,
                    },
                    &mut events,
                );
            }
        }

        events.extend(self.player.update(host_now));
        let hits: Vec<f64> = events
            .iter()
            .filter_map(|ev| match ev.kind {
                JudgeEventKind::Tap(j, delta) if j != ddi_engine::rules::Judgement::Miss => {
                    Some(delta)
                }
                _ => None,
            })
            .collect();
        for delta in hits {
            self.last_delta = Some(delta);
            self.calibrate_hit(delta);
        }

        // An immediate fail: the engine has stopped judging and the field is
        // blank; fade the music and hold the FAILED banner before the results.
        if self.player.failed() && self.failed_at.is_none() {
            self.failed_at = Some(host_now.0);
            self.audio.fade_out(FAIL_FADE);
            self.keyboard = None;
        }

        let outcome = if self.aborted {
            self.audio_stop();
            Some(SessionEvent::Aborted)
        } else if self.interrupted.get() || !self.audio.running() {
            self.audio_stop();
            Some(SessionEvent::Interrupted)
        } else if let Some(at) = self.failed_at {
            (host_now.0 - at >= FAIL_HOLD).then(|| {
                self.audio_stop();
                SessionEvent::Finished {
                    results: Box::new(self.player.results()),
                    calibration: None,
                    device_changed: self.device_changed.get(),
                }
            })
        } else if self.calibration.is_some()
            && (self.calibration.as_ref().is_some_and(|(_, c)| c.done())
                || self
                    .calibration_outcome()
                    .is_some_and(|o| o.should_stop_early()))
        {
            self.audio.fade_out(0.3);
            // The test is over: clear the field instead of letting the rest of
            // the chart scroll on behind the result card.
            self.render.hide_notes = true;
            Some(self.finished_event())
        } else if self.player.finished() {
            Some(self.finished_event())
        } else {
            None
        };
        if outcome.is_some() {
            self.keyboard = None;
        }
        (events, outcome)
    }

    fn finished_event(&self) -> SessionEvent {
        SessionEvent::Finished {
            results: Box::new(self.player.results()),
            calibration: self.calibration_outcome(),
            device_changed: self.device_changed.get(),
        }
    }

    fn apply(&mut self, ev: InputEvent, out: &mut Vec<JudgeEvent>) {
        if let Some(h) = self.held.get_mut(ev.lane as usize) {
            *h = ev.pressed;
        }
        out.extend(self.player.input(ev));
    }

    fn audio_stop(&mut self) {
        self.audio.stop_all();
    }

    pub(crate) fn frame_interval(&self) -> f64 {
        self.frame_interval
    }

    pub(crate) fn predicted_present(&self, host_now: HostTime) -> HostTime {
        HostTime(host_now.0 + self.frame_interval)
    }
}

impl Drop for PlaySession {
    fn drop(&mut self) {
        self.audio_stop();
    }
}
