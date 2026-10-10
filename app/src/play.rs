//! One play of one chart: owns the engine `Player`, the audio backend, the
//! input sources (keyboard, gamepads, touch lanes) and the bindings, and
//! advances them once per animation frame.

use std::cell::Cell;
use std::rc::Rc;

use ddi_chart::{Layout, Song};
use ddi_engine::input::{BindingDevice, Bindings, InputEvent, LaneInput};
use ddi_engine::judge::{JudgeEvent, JudgeEventKind};
use ddi_engine::player::{PlayOptions, Player, Results};
use ddi_engine::rules::{JudgeNames, Ruleset};
use ddi_engine::{ConnectedPad, ControlBindings};
use ddi_platform::{AudioBackend, DeviceId, DeviceProfile, HostTime, InputSource, RawInput};
use ddi_render::RenderOptions;
use gloo::events::EventListener;
use wasm_bindgen::JsCast;
use web_sys::{AudioBuffer, HtmlCanvasElement};

use crate::calibration::{self, CalMode, Calibrator, Outcome};
use crate::settings::Settings;
use crate::web::audio::WebAudio;
use crate::web::gamepad::Gamepads;
use crate::web::keyboard::WebKeyboard;
use crate::web::touch::{self, TouchLanes};

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
/// The hint when the quit button was touched (no Esc key on a phone).
const TOUCH_CANCEL_HINT: &str = "hold or double-tap the quit button";
/// The hint when a controller's Back button was pressed.
const PAD_CANCEL_HINT: &str = "hold or double-tap Back to quit";

/// Seconds over which the music stops at a work's `endFrame` (a ramp, not
/// a click).
const MUSIC_STOP_FADE: f64 = 0.1;

/// Seconds a work's music fades in over after a `startFrame` start
/// (danoniplus raises the volume by 3/1000 a frame).
const MUSIC_FADE_IN: f64 = 1000.0 / 3.0 / 60.0;

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
    /// Gamepad poller shared with the screen (start button), if supported.
    pub(crate) gamepads: Option<Gamepads>,
    /// Autoplay through a test gamepad (`auto=pad`): presses are scheduled
    /// on `window.__DDI_FAKE_PAD` instead of fed to the engine directly.
    pub(crate) auto_pad: bool,
    /// The play canvas, for touch lanes over it.
    pub(crate) touch_canvas: Option<HtmlCanvasElement>,
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
                cancel_hint: "",
                note_colors: settings.note_colors.scheme(),
                chart_colors: settings.chart_colors,
                background: settings.bg_brightness,
                // Chosen each frame by the game loop.
                backdrop: Default::default(),
                frame_aspect: None,
                field_filter: settings.field_filter,
            },
            calibration: None,
            auto_bias: 0.0,
            devices,
            gamepads: None,
            auto_pad: false,
            touch_canvas: None,
        }
    }
}

pub(crate) struct PlaySession {
    pub(crate) player: Player,
    pub(crate) layout: Layout,
    pub(crate) names: JudgeNames,
    pub(crate) render: RenderOptions,
    audio: WebAudio,
    /// Music volume the session plays at (0 in the muted calibration).
    volume: f32,
    keyboard: Option<WebKeyboard>,
    gamepads: Option<Gamepads>,
    touch: Option<TouchLanes>,
    input: LaneInput,
    /// Pad controls that act like Escape: `(Gamepad.id, control)`.
    pad_cancel: Vec<(String, String)>,
    /// Pads `(Gamepad.id, Gamepad.index)` the bindings were resolved with.
    /// A pad first seen during play (browsers reveal a pad only once a
    /// button is pressed) makes the session resolve them again.
    known_pads: Vec<(String, u32)>,
    /// Lane that every control maps to (any-key calibration), if any.
    any_lane: Option<u8>,
    /// The saved bindings the play started with.
    controls: ControlBindings,
    held: Vec<bool>,
    raw: Vec<RawInput>,
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
    /// `auto=pad`: the pad control per lane that the test pad presses.
    via_pad: Option<Vec<Option<String>>>,
}

/// A random 64-bit seed for the Shuffle turn.
fn random_seed() -> u64 {
    let hi = (js_sys::Math::random() * 4_294_967_296.0) as u64;
    let lo = (js_sys::Math::random() * 4_294_967_296.0) as u64;
    (hi << 32) | lo
}

/// How far ahead `auto=pad` schedules presses on the test pad, seconds: a
/// few frames, and no more, because the song-to-host conversion uses the
/// clock as it is now and later clock corrections would shift the edge.
const AUTO_PAD_LOOKAHEAD: f64 = 0.05;

/// Schedules an edge on the test pad installed by the headless tests
/// (`window.__DDI_FAKE_PAD.schedule(hostMs, control, pressed)`).
fn schedule_fake_pad(host: HostTime, control: &str, pressed: bool) {
    let Some(window) = web_sys::window() else {
        return;
    };
    let Ok(pad) = js_sys::Reflect::get(&window, &"__DDI_FAKE_PAD".into()) else {
        return;
    };
    if let Ok(f) = js_sys::Reflect::get(&pad, &"schedule".into())
        .and_then(|f| f.dyn_into::<js_sys::Function>())
    {
        let _ = f.call3(
            &pad,
            &(host.0 * 1000.0).into(),
            &control.into(),
            &pressed.into(),
        );
    }
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
    /// Audio state for the debug overlay (see [`WebAudio::status_line`]).
    pub(crate) fn audio_status(&self) -> String {
        format!("{} · volume {:.2}", self.audio.status_line(), self.volume)
    }

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
        let layout = song
            .layout_of(chart)
            .ok_or_else(|| format!("layout {} not playable", chart.layout))?;
        // Calibration keeps its own ruleset; songs follow the ruleset mode.
        let ruleset = match config.calibration {
            Some(_) => config.ruleset,
            None => settings.ruleset_for(chart),
        };
        let names = ruleset.names.clone();
        let any_key = config.calibration.is_some_and(|m| m.any_key());
        let any_lane = any_key.then_some(calibration::AUDIO_LANE);
        let gamepads = config.gamepads.clone();
        let connected = connected_pads(gamepads.as_ref());
        let controls = settings.controls.clone();
        let bindings = play_bindings(&controls, &layout, &connected, any_lane);
        let known_pads: Vec<(String, u32)> =
            connected.iter().map(|c| (c.id.clone(), c.index)).collect();
        let mut pad_cancel: Vec<(String, String)> = settings
            .controls
            .pads
            .iter()
            .map(|p| p.id.clone())
            .chain(connected.iter().map(|c| c.id.clone()))
            .filter_map(|id| {
                let standard = connected.iter().any(|c| c.id == id && c.standard);
                let back = settings.pad_bindings(&id, standard)?.back?;
                Some((id, back))
            })
            .collect();
        pad_cancel.sort();
        pad_cancel.dedup();
        let via_pad = (config.auto && config.auto_pad).then(|| {
            (0..layout.lanes.len() as u8)
                .map(|lane| {
                    bindings
                        .controls_of(lane)
                        .find(|(d, _)| matches!(d, ddi_engine::BindingDevice::Gamepad(_)))
                        .map(|(_, c)| c.to_string())
                })
                .collect()
        });
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
        let touch = config.touch_canvas.and_then(|canvas| {
            TouchLanes::new(
                canvas,
                layout
                    .lanes
                    .iter()
                    .map(|l| (l.column, l.scroll_sign))
                    .collect(),
                config.render.reverse,
            )
        });
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

        let mut options = config.options;
        // A fresh shuffle every play; the seed is kept in the results.
        options.seed = random_seed();
        // Frame-based works: the x-mod means the same as at their tempo.
        options.scroll.speed = ddi_engine::scroll::speed_for_chart(
            song,
            chart_index,
            options.scroll.speed,
            settings.speed_source,
        );
        let mut player = Player::new(song, chart_index, ruleset, options);
        // The autoplayer presses the transformed chart's lanes.
        let auto = config.auto.then(|| AutoPlay {
            events: ddi_engine::autoplay::script(player.judge().notes(), config.auto_bias)
                .into_iter()
                .map(|p| (p.press, p.release, p.lane))
                .collect(),
            next: 0,
            pending_release: Vec::new(),
            cue: if config.calibration.is_some_and(|m| m.muted()) {
                Cue::Visual
            } else {
                Cue::Audio
            },
            via_pad,
        });
        let sample = audio.now();
        player.clock_sample(sample);
        let start_ctx = sample.context_time + START_LOOKAHEAD;
        // A work may start part-way into its music (`startFrame`) and end
        // or fade out before the music does.
        let danoni = chart
            .danoni
            .as_ref()
            .filter(|_| config.calibration.is_none());
        let song_offset = danoni
            .and_then(|d| d.start)
            .filter(|s| s.is_finite() && *s > 0.0 && *s < buffer.duration())
            .unwrap_or(0.0);
        let music_end = danoni.and_then(|d| {
            let stop = d.end.map(|at| (at, MUSIC_STOP_FADE));
            [stop, d.fade]
                .into_iter()
                .flatten()
                .filter(|(at, len)| at.is_finite() && len.is_finite())
                .min_by(|a, b| a.0.total_cmp(&b.0))
        });
        // After a `startFrame` the music joins in a little later and fades
        // in (danoniplus). Fades run on the audio clock, so what is heard
        // follows the music exactly.
        let music_offset = danoni
            .and_then(|d| d.music_start)
            .filter(|m| m.is_finite() && *m >= song_offset && *m < buffer.duration())
            .unwrap_or(song_offset);
        let music_ctx = start_ctx + (music_offset - song_offset);
        audio.play(&buffer, music_ctx, music_offset, config.volume)?;
        if song_offset > 0.0 {
            audio.fade_in_at(music_ctx, MUSIC_FADE_IN);
        }
        if let Some((at, fade)) = music_end.filter(|(at, _)| *at > music_offset) {
            audio.fade_out_at(music_ctx + (at - music_offset), fade);
        }
        player.start(start_ctx, song_offset);
        // After the last fallible step: only a session (whose `Drop` turns
        // it off again) may run the fast loop.
        if let Some(g) = &gamepads {
            g.clear();
            g.set_fast(settings.fast_pad_poll);
        }
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
        if let Some(md) = crate::web::devices::media_devices() {
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
            volume: config.volume,
            player,
            layout,
            names,
            render: config.render,
            audio,
            keyboard,
            gamepads,
            touch,
            input: LaneInput::new(bindings),
            pad_cancel,
            known_pads,
            any_lane,
            controls,
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
        if let Some(t) = self.touch.as_mut() {
            t.poll(&mut self.raw);
        }
        // The poller's own loops poll the pads (never from inside this
        // tick: its edge listener may re-enter the screen component).
        if let Some(g) = self.gamepads.as_mut() {
            g.poll(&mut self.raw);
        }
        // Sources are drained one after the other; the engine wants each
        // lane's edges in time order.
        self.raw
            .sort_by(|a, b| a.host_time.0.total_cmp(&b.host_time.0));
        // Pad edges are stamped when polled, which can be after this frame's
        // timestamp; judge up to the latest edge so a press is never ahead
        // of the judged time (a mine in between would count as stepped on).
        let judge_now = self
            .raw
            .iter()
            .fold(host_now.0, |t, r| t.max(r.host_time.0));
        let mut raw_inputs = std::mem::take(&mut self.raw);
        for raw in raw_inputs.drain(..) {
            // Only a press reveals a pad: releases of a pad that went away
            // (`release_all`) must not re-resolve the sides mid-song.
            if let DeviceId::Gamepad(id) = &raw.device
                && raw.pressed
                && !self
                    .known_pads
                    .iter()
                    .any(|(k, slot)| k == id && *slot == raw.slot)
            {
                self.adopt_pads();
            }
            if self.is_cancel(&raw) {
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
                    self.render.cancel_hint = match raw.device {
                        DeviceId::Touch => TOUCH_CANCEL_HINT,
                        DeviceId::Gamepad(_) => PAD_CANCEL_HINT,
                        _ => "",
                    };
                } else {
                    self.cancel_held = false;
                }
                continue;
            }
            if let Some(ev) = self.input.feed(&raw) {
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
            if let Some(controls) = &auto.via_pad {
                // Schedule ahead on the test pad; the edges come back through
                // the poller like a real pad's.
                while auto.next < auto.events.len()
                    && auto.events[auto.next].0 <= heard + AUTO_PAD_LOOKAHEAD
                {
                    let (t, rel, lane) = auto.events[auto.next];
                    auto.next += 1;
                    if let Some(Some(control)) = controls.get(lane as usize) {
                        schedule_fake_pad(HostTime(host_now.0 + (t - heard)), control, true);
                        schedule_fake_pad(HostTime(host_now.0 + (rel - heard)), control, false);
                    }
                }
            }
            let mut pending = Vec::new();
            while auto.via_pad.is_none()
                && auto.next < auto.events.len()
                && auto.events[auto.next].0 <= heard
            {
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
            // In time order: after a long frame a release can precede a
            // later press on the same lane.
            pending.sort_by(|a, b| a.0.total_cmp(&b.0));
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

        events.extend(self.player.update(HostTime(judge_now)));
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
            self.touch = None;
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
            self.touch = None;
            if let Some(g) = &self.gamepads {
                g.set_fast(false);
            }
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

    /// Resolves the bindings again with the pads connected now, after a
    /// pad was first seen during play (saved tables were bound from the
    /// start; this adds standard defaults and two-pad sides).
    fn adopt_pads(&mut self) {
        let connected = connected_pads(self.gamepads.as_ref());
        self.input.bindings =
            play_bindings(&self.controls, &self.layout, &connected, self.any_lane);
        for c in &connected {
            if let Some(back) = self.controls.pad(&c.id, c.standard).and_then(|p| p.back)
                && !self.pad_cancel.iter().any(|(id, _)| *id == c.id)
            {
                self.pad_cancel.push((c.id.clone(), back));
            }
        }
        self.known_pads = connected.into_iter().map(|c| (c.id, c.index)).collect();
    }

    /// Escape/Backspace, a pad's Back control, or the touch quit button.
    fn is_cancel(&self, raw: &RawInput) -> bool {
        match &raw.device {
            DeviceId::Keyboard => is_cancel_key(&raw.control),
            DeviceId::Touch => raw.control == touch::CANCEL_CONTROL,
            DeviceId::Gamepad(id) => self
                .pad_cancel
                .iter()
                .any(|(p, c)| p == id && *c == raw.control),
            _ => false,
        }
    }

    /// Which lanes are down, for the debug state.
    pub(crate) fn held(&self) -> &[bool] {
        &self.held
    }

    pub(crate) fn gamepads(&self) -> Option<&Gamepads> {
        self.gamepads.as_ref()
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
        if let Some(g) = &self.gamepads {
            g.set_fast(false);
        }
    }
}

/// The controllers connected now.
fn connected_pads(gamepads: Option<&Gamepads>) -> Vec<ConnectedPad> {
    gamepads
        .map(|g| {
            g.pads()
                .into_iter()
                .map(|p| ConnectedPad {
                    id: p.id,
                    index: p.index,
                    standard: p.standard,
                })
                .collect()
        })
        .unwrap_or_default()
}

/// A play's bindings: the saved ones for the layout and the touch columns,
/// which follow the layout whatever its lane count.
fn play_bindings(
    controls: &ControlBindings,
    layout: &Layout,
    connected: &[ConnectedPad],
    any_lane: Option<u8>,
) -> Bindings {
    let mut b = controls.resolve(layout, connected, any_lane);
    for lane in 0..layout.lanes.len() as u8 {
        b.bind(
            BindingDevice::Touch,
            &format!("lane:{lane}"),
            any_lane.unwrap_or(lane),
        );
    }
    b
}
