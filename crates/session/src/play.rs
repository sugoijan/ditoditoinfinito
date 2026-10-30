//! One play of one chart (moved from `crates/web/src/play.rs` in
//! `docs/plans/desktop.md` step 7.1): owns the engine `Player`, the audio
//! backend and the bindings, takes the shell's input edges and advances
//! once per frame. What only a shell has (its keyboard, touch lanes and
//! pads, whether the page or the audio was interrupted, a test pad) comes
//! through [`Inputs`]; the audio through `ddi_platform::AudioBackend`.

use ddi_chart::{Layout, Song};
use ddi_engine::autoplay::Press;
use ddi_engine::frame::Frame;
use ddi_engine::input::{BindingDevice, Bindings, InputEvent, LaneInput};
use ddi_engine::judge::{JudgeEvent, JudgeEventKind};
use ddi_engine::player::{PlayOptions, Player, Results};
use ddi_engine::rules::{JudgeNames, Ruleset};
use ddi_engine::{ConnectedPad, ControlBindings, PadBindings};
use ddi_platform::{AudioBackend, DeviceId, DeviceProfile, HostTime, RawInput};
use ddi_ui::state::history::{
    Ghost, GhostDelta, GhostRun, InputDevice, PlayLog, Replay, Replaying, RulesetKey,
};
use ddi_ui::state::settings::Settings;

use crate::calibration::{self, CalMode, Calibrator, Outcome};
use crate::sounds::EventSounds;
pub use cover::{COVER_DOWN_KEY, COVER_UP_KEY, CoverControl, START_KEYS};

/// Lead time between scheduling the start and the first audio sample, so the
/// buffer source starts exactly on the audio clock. The play's first frame
/// is drawn at song second `−START_LOOKAHEAD`, where the opening still is
/// taken (`ddi_library::backgrounds::PLAY_START`, the one constant).
pub const START_LOOKAHEAD: f64 = -ddi_library::backgrounds::PLAY_START;
/// Music fade after an immediate fail, and the pause before the results.
pub const FAIL_FADE: f64 = 1.2;
pub const FAIL_HOLD: f64 = 1.8;
/// Cancel: a second press within this many seconds, or one press held this
/// long. A single tap only shows the hint.
pub const CANCEL_DOUBLE_TAP: f64 = 0.5;
pub const CANCEL_HOLD: f64 = 0.7;
/// How long the hint stays after a single tap.
pub const CANCEL_HINT: f64 = 1.5;
/// The hint when the quit button was touched (no Esc key on a phone).
pub const TOUCH_CANCEL_HINT: &str = "hold or double-tap the quit button";
/// The control the touch lanes' quit button sends.
pub const TOUCH_CANCEL_CONTROL: &str = "cancel";
/// How long a play's quit button stays after the pointer moved over the
/// play, seconds ([`quit_shown`]).
pub const QUIT_LINGER: f64 = 2.0;

/// Whether a play's quit button shows, on both shells: throughout on a
/// touch screen (`touch`), else for [`QUIT_LINGER`] after the pointer
/// moved over the play (`since_pointer` seconds ago; `None`: it has
/// not). Keys and pads leave it as it is.
pub fn quit_shown(touch: bool, since_pointer: Option<f64>) -> bool {
    touch || since_pointer.is_some_and(|s| s < QUIT_LINGER)
}

/// Seconds over which the music stops at a work's `endFrame` (a ramp, not
/// a click).
const MUSIC_STOP_FADE: f64 = 0.1;

/// Seconds a work's music fades in over after a `startFrame` start
/// (danoniplus raises the volume by 3/1000 a frame).
const MUSIC_FADE_IN: f64 = 1000.0 / 3.0 / 60.0;

/// Keys that act as the cancel gesture.
pub fn is_cancel_key(code: &str) -> bool {
    code == "Escape" || code == "Backspace"
}

/// What a shell gives a play besides its audio.
pub trait Inputs {
    /// Readies the keyboard (capturing `capture`, the bound keys and the
    /// keys the page would otherwise scroll with) and the touch lanes of
    /// `layout` (drawn reversed or not), as the play starts.
    fn begin(&mut self, capture: Vec<String>, layout: &Layout, reverse: bool);
    /// The edges since the last call, the keyboard's, then the touch
    /// lanes', then the pads'.
    fn poll(&mut self, out: &mut Vec<RawInput>);
    /// The controllers connected now.
    fn connected_pads(&self) -> Vec<ConnectedPad>;
    /// The play runs (pads polled fast if `fast_poll`, their edges so far
    /// forgotten) or ended (`None`).
    fn pads_running(&mut self, fast_poll: Option<bool>);
    /// The keyboard and the touch lanes are let go (a fail, the end).
    fn release(&mut self);
    /// The page was hidden or the audio interrupted.
    fn interrupted(&self) -> bool;
    /// An audio device or the display changed during the play.
    fn device_changed(&self) -> bool;
    /// `auto=pad`: an edge scheduled on the test pad at host time `host`.
    fn press_test_pad(&mut self, host: HostTime, control: &str, pressed: bool);
}

/// Everything about a session that is not the song.
pub struct SessionConfig {
    pub ruleset: Ruleset,
    pub options: PlayOptions,
    pub volume: f32,
    pub auto: bool,
    /// Converging calibration test, if this session is one.
    pub calibration: Option<CalMode>,
    /// Autoplay presses this many seconds late (negative = early); testing aid.
    pub auto_bias: f64,
    /// Devices the session runs on (selects the offsets).
    pub devices: Option<DeviceProfile>,
    /// Autoplay through a test gamepad (`auto=pad`): presses are scheduled
    /// on it instead of fed to the engine directly.
    pub auto_pad: bool,
    /// The Shuffle turn's seed for this play.
    pub seed: u64,
    /// The field is drawn reversed (the touch lanes follow).
    pub reverse: bool,
    /// The notes are hidden from the start (the sound-only test).
    pub hide_notes: bool,
    /// A kept play replayed (`docs/plans/play-history.md`, Replays): its
    /// options and rules instead of the settings', its log instead of
    /// live presses; never autoplayed, never reported to the history.
    pub replay: Option<Replaying>,
    /// The chart's best play's score over the song, to show how far
    /// ahead of it the play is (`Settings::ghost`).
    pub ghost: Option<Ghost>,
    /// Song second to start at (the chart editor's playtest from its
    /// cursor): earlier notes are skipped and the music fades in, as
    /// after a work's `startFrame`.
    pub start: Option<f64>,
}

impl SessionConfig {
    pub fn from_settings(
        settings: &Settings,
        auto: bool,
        devices: Option<DeviceProfile>,
    ) -> SessionConfig {
        SessionConfig {
            ruleset: settings.ruleset(),
            options: settings.play_options(devices.as_ref()),
            volume: settings.volume,
            auto,
            calibration: None,
            auto_bias: 0.0,
            devices,
            auto_pad: false,
            seed: 0,
            reverse: settings.reverse,
            hide_notes: false,
            replay: None,
            ghost: None,
            start: None,
        }
    }

    /// Calibration test `mode` instead of a song: the plain chart is
    /// measured (its ruleset, no turn, no removed notes, a steady speed);
    /// the sound-only test hides the notes, the arrows-only one is muted.
    pub fn calibrate(&mut self, mode: CalMode) {
        self.ruleset = ddi_engine::rules::presets::calibration();
        self.options.transform = Default::default();
        self.options.appearance = Default::default();
        self.options.cover = Default::default();
        self.options.scroll.scroll_action = Default::default();
        self.hide_notes = mode.hide_notes();
        self.calibration = Some(mode);
        if mode.muted() {
            self.volume = 0.0;
        }
    }

    /// Replays the kept play `r`: its log on the lanes, its options and
    /// rules, the offsets this device needs ([`Replay::replaying`]).
    /// Never autoplayed.
    pub fn set_replay(&mut self, r: &Replay) {
        self.replay = Some(r.replaying(self.devices.as_ref(), self.options.clock));
    }
}

/// Seconds the notes of song `song_id` are moved later for a play
/// ([`ddi_chart::Song::shift_notes`]): a replay's as it was played, else
/// the song's offset in `settings`; none for a calibration test.
pub fn song_shift(
    settings: &Settings,
    song_id: &str,
    replay: Option<&Replay>,
    calibration: bool,
) -> f64 {
    match replay {
        _ if calibration => 0.0,
        Some(r) => r.record.song_offset,
        None => settings.song_offset(song_id),
    }
}

/// What the field shows of the session beside the chart: the notes hidden
/// (the sound-only test, the end of a calibration), the quit gesture's
/// progress and hint.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct SessionView {
    pub hide_notes: bool,
    pub cancel_progress: f32,
    pub cancel_hint: &'static str,
}

pub struct PlaySession<A: AudioBackend, I: Inputs> {
    pub player: Player,
    pub layout: Layout,
    pub names: JudgeNames,
    /// The ruleset played by, fresh (its windows and gauge for the
    /// results).
    pub rules: Ruleset,
    /// The music's length, seconds.
    pub length: f64,
    pub view: SessionView,
    audio: A,
    /// Music volume the session plays at (0 in the muted calibration).
    volume: f32,
    inputs: I,
    input: LaneInput,
    /// Pad controls that act like Escape: `(Gamepad.id, control)`.
    pad_cancel: Vec<(String, String)>,
    /// Pads' Start controls, which hide and move the lane covers.
    pad_start: Vec<(String, String)>,
    /// What the edges do to the lane covers.
    cover: CoverControl,
    /// The lane covers' heights to keep in the settings once the play
    /// ends ([`Settings::keep_cover_heights`]): none for a replay or a
    /// calibration.
    keep_cover: bool,
    /// The pads the bindings were resolved with. A pad first seen during
    /// play (browsers reveal a pad only once a button is pressed) makes
    /// the session resolve them again.
    known_pads: Vec<ConnectedPad>,
    /// The controller styles the player chose, for the quit hint.
    styles: std::collections::BTreeMap<String, ddi_ui::controller_hints::Style>,
    /// The saved bindings the play started with.
    controls: ControlBindings,
    held: Vec<bool>,
    raw: Vec<RawInput>,
    /// Autoplay: scripted presses at each note's judged time.
    auto: Option<AutoPlay>,
    aborted: bool,
    last_frame_host: Option<f64>,
    frame_interval: f64,
    /// Host time at which an immediate fail was noticed.
    failed_at: Option<f64>,
    /// Signed error of the last judged tap, seconds.
    pub last_delta: Option<f64>,
    calibration: Option<(CalMode, Calibrator)>,
    /// Host time of the last cancel-key press, and when it was released.
    cancel_pressed_at: Option<f64>,
    cancel_held: bool,
    pub devices: Option<DeviceProfile>,
    /// The devices whose presses reached a lane.
    played_with: Vec<InputDevice>,
    /// The output latency at the start, and its range over the play,
    /// from samples of a running output only (a stream with no
    /// callbacks yet knows none).
    latency: Option<(f64, f64, f64)>,
    /// The fail's report, until the shell takes it.
    fail_report: Option<SessionEvent>,
    /// A replay: the log's edges, and how many were fed, and its
    /// covers' changes likewise.
    replay: Option<(PlayLog, usize, usize)>,
    /// The best play's ghost played beside it, and how far ahead of it
    /// the play was at its last update.
    ghost: Option<GhostRun>,
    ghost_delta: Option<GhostDelta>,
    /// The mine sound and the assist clap (none in calibration).
    sounds: EventSounds<A::Buffer>,
    /// Where the music's song second `t` plays on the audio clock:
    /// `start_ctx + (t − song_offset) / rate`.
    start_ctx: f64,
    song_offset: f64,
}

/// Which cue a simulated player follows. In calibration it plays as a
/// human would: a human reacts to what they hear or see, not to the judged
/// timeline, so corrections to the offsets must change when the autoplayer
/// presses the way they change a human's timing. In play it watches the
/// chart for the player, so it presses on the judged timeline: following
/// the heard audio, a calibrated offset moved every press by its whole
/// amount (at 100 ms, a hard chart fails on Greats and misses).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Cue {
    /// The judged timeline.
    Judged,
    /// The physical audio: judged time plus the audio offset.
    Audio,
    /// The drawn arrows: judged time plus the visual offset.
    Visual,
}

struct AutoPlay {
    /// Sorted by press time.
    events: Vec<Press>,
    next: usize,
    pending_release: Vec<(f64, u8)>,
    cue: Cue,
    /// `auto=pad`: the pad control per lane that the test pad presses.
    via_pad: Option<Vec<Option<String>>>,
}

/// How far ahead `auto=pad` schedules presses on the test pad, seconds: a
/// few frames, and no more, because the song-to-host conversion uses the
/// clock as it is now and later clock corrections would shift the edge.
const AUTO_PAD_LOOKAHEAD: f64 = 0.05;

/// What the play history keeps of a play beyond its results.
#[derive(Clone, Debug)]
pub struct PlayFacts {
    /// Every press and release as judged.
    pub log: PlayLog,
    /// The options as played (the seed and the speed included).
    pub options: PlayOptions,
    pub ruleset: RulesetKey,
    /// The devices whose presses reached a lane.
    pub inputs: Vec<InputDevice>,
    /// The output latency as the play started, and its range over the
    /// play, seconds.
    pub output_latency: f64,
    pub latency_min: f64,
    pub latency_max: f64,
    pub sample_rate: f64,
}

pub enum SessionEvent {
    Finished {
        results: Box<Results>,
        facts: Box<PlayFacts>,
        calibration: Option<Outcome>,
        /// An audio device or the display changed during the play.
        device_changed: bool,
    },
    /// The play failed (immediate fail): its results are final, the
    /// session goes on to show it (`Finished` follows after the hold,
    /// unless a quit or an interruption comes first). The history keeps
    /// the play now.
    Failed {
        results: Box<Results>,
        facts: Box<PlayFacts>,
    },
    /// The player quit: the play judged to the quit, the notes left
    /// unjudged, its results graded QUIT (`Results::quit`; a quit in a
    /// fail's hold keeps the fail's) and its facts, which the history
    /// keeps as a quit (a calibration's or a replay's are not kept).
    Aborted {
        results: Box<Results>,
        facts: Box<PlayFacts>,
    },
    /// The page was hidden or the audio suspended mid-play.
    Interrupted,
}

impl<A: AudioBackend, I: Inputs> PlaySession<A, I> {
    /// The audio backend.
    pub fn audio(&self) -> &A {
        &self.audio
    }

    /// The audio, to keep (a shell's housekeeping between frames).
    pub fn audio_mut(&mut self) -> &mut A {
        &mut self.audio
    }

    /// The shell's inputs.
    pub fn inputs(&self) -> &I {
        &self.inputs
    }

    /// The music's volume.
    pub fn volume(&self) -> f32 {
        self.volume
    }

    /// Plays `buffer` now on the play's sound, at the music's volume (a
    /// song's scripts' sounds).
    pub fn play_now(&mut self, buffer: &A::Buffer) -> Result<ddi_platform::SoundId, A::Error> {
        let at = self.audio.now().context_time;
        self.audio.play(buffer, at, 0.0, self.volume)
    }

    /// The claps scheduled and the mine sounds played so far.
    pub fn event_sounds(&self) -> (u32, u32) {
        (self.sounds.claps, self.sounds.mines)
    }

    /// Builds the session and schedules the start on the audio clock.
    pub fn start(
        song: &Song,
        chart_index: usize,
        settings: &Settings,
        config: SessionConfig,
        mut audio: A,
        buffer: A::Buffer,
        mut inputs: I,
    ) -> Result<PlaySession<A, I>, String>
    where
        A::Error: std::fmt::Display,
    {
        let chart = song
            .charts
            .get(chart_index)
            .ok_or_else(|| format!("chart {chart_index} missing"))?;
        let layout = song
            .layout_of(chart)
            .ok_or_else(|| format!("layout {} not playable", chart.layout))?;
        // Calibration keeps its own ruleset; songs follow the ruleset mode;
        // a replay plays by the rules it was played by.
        let ruleset = match (&config.replay, config.calibration) {
            (Some(r), _) => r.ruleset.build(chart),
            (None, Some(_)) => config.ruleset,
            (None, None) => settings.ruleset_for(chart),
        };
        // A replay is never autoplayed.
        let auto_play = config.auto && config.replay.is_none();
        let names = ruleset.names.clone();
        let rules = ruleset.fresh();
        let duration = audio.duration(&buffer);
        // Any key for the one lane (calibration by ear): judged there, shown
        // where pressed.
        let any_key = config.calibration.is_some_and(|m| m.any_key());
        let any_lane = any_key.then_some(calibration::AUDIO_LANE);
        let connected = inputs.connected_pads();
        let controls = settings.controls.clone();
        let bindings = play_bindings(&controls, &layout, &connected);
        let known_pads = connected.clone();
        let (pad_cancel, pad_start) = pad_controls(&controls, &connected);
        let via_pad = (auto_play && config.auto_pad).then(|| {
            (0..layout.lanes.len() as u8)
                .map(|lane| {
                    bindings
                        .controls_of(lane)
                        .find(|(d, _)| matches!(d, BindingDevice::Gamepad(_)))
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
                COVER_UP_KEY,
                COVER_DOWN_KEY,
            ]
            .iter()
            .map(|s| s.to_string()),
        );
        inputs.begin(capture, &layout, config.reverse);

        let options = match &config.replay {
            // As played: its seed, its speed as it was worked out.
            Some(r) => r.options,
            None => {
                let mut options = config.options;
                // A fresh shuffle every play; the seed is kept in the results.
                options.seed = config.seed;
                // Frame-based works: the x-mod means the same as at their tempo.
                options.scroll.speed = ddi_engine::scroll::speed_for_chart(
                    song,
                    chart_index,
                    options.scroll.speed,
                    settings.speed_source,
                );
                options
            }
        };
        let mut player = Player::new(song, chart_index, ruleset, options);
        player.set_autoplay(auto_play);
        player.judge_all_in(any_lane);
        // The autoplayer presses the transformed chart's lanes.
        let auto = auto_play.then(|| AutoPlay {
            events: ddi_engine::autoplay::script(player.judge().notes(), config.auto_bias),
            next: 0,
            pending_release: Vec::new(),
            cue: match config.calibration {
                None => Cue::Judged,
                Some(m) if m.muted() => Cue::Visual,
                Some(_) => Cue::Audio,
            },
            via_pad,
        });
        let sample = audio.now();
        let latency = audio.running().then_some((
            sample.output_latency,
            sample.output_latency,
            sample.output_latency,
        ));
        player.clock_sample(sample);
        let start_ctx = sample.context_time + START_LOOKAHEAD;
        // A work may start part-way into its music (`startFrame`) and end
        // or fade out before the music does.
        let danoni = chart
            .danoni
            .as_ref()
            .filter(|_| config.calibration.is_none());
        let song_offset = match &config.replay {
            // Where the play started.
            Some(r) => r.log.start,
            None => config
                .start
                .or_else(|| danoni.and_then(|d| d.start))
                .filter(|s| s.is_finite() && *s > 0.0 && *s < duration)
                .unwrap_or(0.0),
        };
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
            .filter(|m| m.is_finite() && *m >= song_offset && *m < duration)
            .unwrap_or(song_offset);
        let music_ctx = start_ctx + (music_offset - song_offset);
        audio
            .play(&buffer, music_ctx, music_offset, config.volume)
            .map_err(|e| e.to_string())?;
        if song_offset > 0.0 {
            audio.master_ramp(Some(0.0), 1.0, Some(music_ctx), MUSIC_FADE_IN);
        }
        if let Some((at, fade)) = music_end.filter(|(at, _)| *at > music_offset) {
            audio.master_ramp(Some(1.0), 0.0, Some(music_ctx + (at - music_offset)), fade);
        }
        player.start(start_ctx, song_offset);
        // Nothing may cue the player in calibration.
        let sounds = if config.calibration.is_some() {
            EventSounds::none()
        } else {
            EventSounds::new(
                &audio,
                settings.assist_clap,
                settings.mine_sound,
                config.volume,
                player.judge().notes(),
                song_offset,
            )
        };
        // After the last fallible step: only a session (whose drop turns it
        // off again) may run the fast loop.
        inputs.pads_running(Some(settings.fast_pad_poll));
        let lanes = layout.lanes.len();
        Ok(PlaySession {
            volume: config.volume,
            player,
            layout,
            names,
            rules,
            length: duration,
            view: SessionView {
                hide_notes: config.hide_notes,
                cancel_progress: 0.0,
                cancel_hint: "",
            },
            audio,
            inputs,
            input: LaneInput::new(bindings),
            pad_cancel,
            pad_start,
            cover: CoverControl::new(config.reverse),
            keep_cover: config.replay.is_none() && config.calibration.is_none(),
            known_pads,
            styles: settings.controller_styles.clone(),
            controls,
            held: vec![false; lanes],
            raw: Vec::new(),
            auto,
            aborted: false,
            last_frame_host: None,
            frame_interval: 1.0 / 60.0,
            failed_at: None,
            last_delta: None,
            calibration: config.calibration.map(|m| (m, Calibrator::default())),
            cancel_pressed_at: None,
            cancel_held: false,
            devices: config.devices,
            played_with: Vec::new(),
            latency,
            fail_report: None,
            replay: config.replay.map(|r| (r.log, 0, 0)),
            ghost: config.ghost.as_ref().map(Ghost::run),
            ghost_delta: None,
            sounds,
            start_ctx,
            song_offset,
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

    pub fn calibration_outcome(&self) -> Option<Outcome> {
        let (_, cal) = self.calibration.as_ref()?;
        let t = self.player.tally();
        Some(cal.outcome(t.taps[0], t.taps[5]))
    }

    /// [`Outcome::should_stop_early`] of [`Self::calibration_outcome`],
    /// without working out the residuals every frame.
    fn calibration_stops_early(&self) -> bool {
        self.calibration.is_some() && {
            let t = self.player.tally();
            ddi_engine::calibration::stop_early(t.taps[0], t.taps[5])
        }
    }

    /// Advance to the current host time. Returns judge events for effects.
    pub fn tick(&mut self, host_now: HostTime) -> (Vec<JudgeEvent>, Option<SessionEvent>) {
        let mut events = Vec::new();
        if let Some(last) = self.last_frame_host {
            let dt = host_now.0 - last;
            if dt > 0.0 && dt < 0.25 {
                self.frame_interval = self.frame_interval * 0.9 + dt * 0.1;
            }
        }
        self.last_frame_host = Some(host_now.0);
        self.player.set_frame_interval(self.frame_interval);

        let sample = self.audio.now();
        if self.audio.running() {
            let l = sample.output_latency;
            let (_, lo, hi) = self.latency.get_or_insert((l, l, l));
            *lo = lo.min(l);
            *hi = hi.max(l);
        }
        self.player.clock_sample(sample);

        self.raw.clear();
        self.inputs.poll(&mut self.raw);
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
                    .any(|p| p.id == *id && p.index == raw.slot)
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
                    self.view.cancel_hint = match raw.device {
                        DeviceId::Touch => TOUCH_CANCEL_HINT,
                        DeviceId::Gamepad(_) => ddi_ui::controller_hints::quit_hint(
                            &raw,
                            &self.known_pads,
                            &self.styles,
                        ),
                        _ => "",
                    };
                } else {
                    self.cancel_held = false;
                }
                continue;
            }
            // A replay's lanes and covers are its log's.
            if self.replay.is_some() {
                continue;
            }
            if self.cover.feed(
                &raw,
                &self.input.bindings,
                &self.layout,
                &self.pad_start,
                &mut self.player,
            ) {
                continue;
            }
            if let Some(ev) = self.input.feed(&raw) {
                self.note_device(&raw.device);
                self.apply(ev, &mut events);
            }
        }
        self.raw = raw_inputs;
        self.cover.tick(host_now, &mut self.player);

        // Hold-to-quit progress and the hint after a single tap.
        self.view.cancel_progress = match self.cancel_pressed_at {
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
                    Cue::Judged => 0.0,
                    Cue::Audio => opts.audio_offset,
                    Cue::Visual => opts.visual_offset,
                };
            if let Some(controls) = &auto.via_pad {
                // Schedule ahead on the test pad; the edges come back through
                // the poller like a real pad's.
                while auto.next < auto.events.len()
                    && auto.events[auto.next].press <= heard + AUTO_PAD_LOOKAHEAD
                {
                    let Press {
                        press: t,
                        release: rel,
                        lane,
                    } = auto.events[auto.next];
                    auto.next += 1;
                    if let Some(Some(control)) = controls.get(lane as usize) {
                        self.inputs.press_test_pad(
                            HostTime(host_now.0 + (t - heard)),
                            control,
                            true,
                        );
                        self.inputs.press_test_pad(
                            HostTime(host_now.0 + (rel - heard)),
                            control,
                            false,
                        );
                    }
                }
            }
            let mut pending = Vec::new();
            while auto.via_pad.is_none()
                && auto.next < auto.events.len()
                && auto.events[auto.next].press <= heard
            {
                let Press {
                    press: t,
                    release: rel,
                    lane,
                } = auto.events[auto.next];
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

        // Up to the time the update judges to.
        self.feed_replay(HostTime(judge_now), &mut events);
        events.extend(self.player.update(HostTime(judge_now)));
        // The ghost judged to the same point (a failed play's stops).
        if let Some(g) = self.ghost.as_mut()
            && !self.player.finished()
        {
            let t = self.player.judge_time(HostTime(judge_now));
            self.ghost_delta = Some(g.delta(&self.player.ruleset().score.view(), t));
        }
        // The event sounds: a mine stepped on now (the step that failed
        // the play too), the claps due soon while it goes on.
        let now = self.audio.now().context_time;
        self.sounds.judged(&mut self.audio, now, &events);
        if !self.player.finished() {
            let (start, offset) = (self.start_ctx, self.song_offset);
            let rate = self.player.clock().options().rate;
            self.sounds
                .schedule(&mut self.audio, now, |t| start + (t - offset) / rate);
        }
        for ev in &events {
            if let JudgeEventKind::Tap(j, delta) = ev.kind
                && j != ddi_engine::rules::Judgement::Miss
            {
                self.last_delta = Some(delta);
                self.calibrate_hit(delta);
            }
        }

        // An immediate fail: the engine has stopped judging and the field is
        // blank; fade the music and hold the FAILED banner before the results.
        // (A delayed fail plays on: it comes here once the play ends.)
        if self.player.failed() && self.player.finished() && self.failed_at.is_none() {
            self.failed_at = Some(host_now.0);
            // Final now: kept even if the hold is cut short (a replay's is
            // not kept).
            if self.calibration.is_none() && self.replay.is_none() {
                self.fail_report = Some(SessionEvent::Failed {
                    results: Box::new(self.player.results()),
                    facts: Box::new(self.facts()),
                });
            }
            self.audio.master_ramp(None, 0.0, None, FAIL_FADE);
            self.inputs.release();
        }

        // A quit play's replay quits where the play did.
        if !self.aborted
            && let Some((log, ..)) = &self.replay
            && let Some(end) = log.end
            && self.player.judge_time(host_now) >= end
        {
            self.player.quit_at(end);
            self.aborted = true;
        }
        let outcome = if self.aborted {
            self.audio.stop_all();
            // The play ends here, judged to now (`Player::quit_at`).
            self.player.quit_at(self.player.judge_time(host_now));
            Some(SessionEvent::Aborted {
                results: Box::new(self.player.results()),
                facts: Box::new(self.facts()),
            })
        } else if self.inputs.interrupted() || !self.audio.running() {
            self.audio.stop_all();
            Some(SessionEvent::Interrupted)
        } else if let Some(at) = self.failed_at {
            (host_now.0 - at >= FAIL_HOLD).then(|| {
                self.audio.stop_all();
                SessionEvent::Finished {
                    results: Box::new(self.player.results()),
                    facts: Box::new(self.facts()),
                    calibration: None,
                    device_changed: self.inputs.device_changed(),
                }
            })
        } else if self.calibration.as_ref().is_some_and(|(_, c)| c.done())
            || self.calibration_stops_early()
        {
            self.audio.master_ramp(None, 0.0, None, 0.3);
            // The test is over: clear the field instead of letting the rest of
            // the chart scroll on behind the result card.
            self.view.hide_notes = true;
            Some(self.finished_event())
        } else if self.player.finished() {
            Some(self.finished_event())
        } else {
            None
        };
        if outcome.is_some() {
            self.inputs.release();
            self.inputs.pads_running(None);
            self.cover.release();
            self.keep_cover_heights();
        }
        (events, outcome)
    }

    /// Keeps the heights the play left the lane covers at in the
    /// settings, once, for the next song (not a replay's or a
    /// calibration's).
    fn keep_cover_heights(&mut self) {
        if !std::mem::take(&mut self.keep_cover) || !self.player.options().cover.is_on() {
            return;
        }
        let mut settings = Settings::load();
        if settings.keep_cover_heights(self.player.cover()) {
            settings.save();
        }
    }

    /// A replay: the log's edges whose judged time has come, each at its
    /// own time, before the frame's update. Every edge not fed yet is
    /// later than any judged time so far (updates judge behind the
    /// clock), so none is judged late, at any frame rate.
    fn feed_replay(&mut self, host_now: HostTime, events: &mut Vec<JudgeEvent>) {
        let Some((log, next, next_cover)) = self.replay.as_mut() else {
            return;
        };
        let now = self.player.judge_time(host_now);
        let mut due = Vec::new();
        while let Some(i) = log.inputs.get(*next).filter(|i| i.time <= now) {
            due.push(*i);
            *next += 1;
        }
        // The covers as the play changed them, up to now.
        let mut covers = Vec::new();
        while let Some(c) = log.covers.get(*next_cover).filter(|c| c.time <= now) {
            covers.push(*c);
            *next_cover += 1;
        }
        for c in covers {
            self.player.set_cover_at(c.cover, c.time);
        }
        for i in due {
            if let Some(h) = self.held.get_mut(usize::from(i.lane)) {
                *h = i.pressed;
            }
            events.extend(self.player.input_at(i.lane, i.pressed, i.time));
        }
    }

    /// How far ahead of the best play's ghost the play is now (behind:
    /// negative); `None` without a ghost or before the play started.
    pub fn ghost_delta(&self) -> Option<GhostDelta> {
        self.ghost_delta
    }

    /// This session replays a kept play.
    pub fn replaying(&self) -> bool {
        self.replay.is_some()
    }

    /// A replay's edges fed so far, and in all.
    pub fn replay_progress(&self) -> Option<(usize, usize)> {
        self.replay
            .as_ref()
            .map(|(log, n, _)| (*n, log.inputs.len()))
    }

    /// The play's fail, once, when it happened ([`SessionEvent::Failed`]).
    pub fn take_fail_report(&mut self) -> Option<SessionEvent> {
        self.fail_report.take()
    }

    /// What the history keeps of the play.
    fn facts(&self) -> PlayFacts {
        PlayFacts {
            log: self.player.log().clone(),
            options: *self.player.options(),
            ruleset: self.rules.key(),
            inputs: self.played_with.clone(),
            output_latency: self.latency.map_or(0.0, |l| l.0),
            latency_min: self.latency.map_or(0.0, |l| l.1),
            latency_max: self.latency.map_or(0.0, |l| l.2),
            sample_rate: self.audio.sample_rate(),
        }
    }

    fn note_device(&mut self, device: &DeviceId) {
        let d = match device {
            DeviceId::Keyboard => InputDevice::Keyboard,
            DeviceId::Touch => InputDevice::Touch,
            DeviceId::Gamepad(model) => InputDevice::Pad {
                model: model.clone(),
            },
            DeviceId::Other(name) => InputDevice::Other { name: name.clone() },
        };
        if !self.played_with.contains(&d) {
            self.played_with.push(d);
        }
    }

    fn finished_event(&self) -> SessionEvent {
        SessionEvent::Finished {
            results: Box::new(self.player.results()),
            facts: Box::new(self.facts()),
            calibration: self.calibration_outcome(),
            device_changed: self.inputs.device_changed(),
        }
    }

    /// Resolves the bindings again with the pads connected now, after a
    /// pad was first seen during play (saved tables were bound from the
    /// start; this adds standard defaults and two-pad sides).
    fn adopt_pads(&mut self) {
        let connected = self.inputs.connected_pads();
        self.input.bindings = play_bindings(&self.controls, &self.layout, &connected);
        let (cancel, start) = pad_controls(&self.controls, &connected);
        for (kept, new) in [(&mut self.pad_cancel, cancel), (&mut self.pad_start, start)] {
            for (id, control) in new {
                if !kept.iter().any(|(k, _)| *k == id) {
                    kept.push((id, control));
                }
            }
        }
        self.known_pads = connected;
    }

    /// Escape/Backspace, a pad's Back control, or the touch quit button.
    fn is_cancel(&self, raw: &RawInput) -> bool {
        match &raw.device {
            DeviceId::Keyboard => is_cancel_key(&raw.control),
            DeviceId::Touch => raw.control == TOUCH_CANCEL_CONTROL,
            DeviceId::Gamepad(id) => self
                .pad_cancel
                .iter()
                .any(|(p, c)| p == id && *c == raw.control),
            _ => false,
        }
    }

    /// Which lanes are down, for the debug state.
    pub fn held(&self) -> &[bool] {
        &self.held
    }

    fn apply(&mut self, ev: InputEvent, out: &mut Vec<JudgeEvent>) {
        if let Some(h) = self.held.get_mut(ev.lane as usize) {
            *h = ev.pressed;
        }
        out.extend(self.player.input(ev));
    }

    pub fn frame_interval(&self) -> f64 {
        self.frame_interval
    }

    pub fn predicted_present(&self, host_now: HostTime) -> HostTime {
        HostTime(host_now.0 + self.frame_interval)
    }

    /// The player's frame presented at `present`. With the notes hidden
    /// (the audio test) its beat is held too, so that a skin pulsing on
    /// the beat gives no cue to follow.
    pub fn frame(&self, present: HostTime) -> Frame {
        let mut frame = self.player.frame(present);
        if self.view.hide_notes {
            frame.beat = 0.0;
        }
        frame
    }
}

impl<A: AudioBackend, I: Inputs> Drop for PlaySession<A, I> {
    fn drop(&mut self) {
        self.audio.stop_all();
        self.inputs.pads_running(None);
    }
}

/// Pads' controls of one kind: `(Gamepad.id, control)`.
type PadControls = Vec<(String, String)>;

/// The Back and Start controls of every pad `controls` keeps and every
/// one `connected` (a standard one without saved bindings: its defaults).
fn pad_controls(
    controls: &ControlBindings,
    connected: &[ConnectedPad],
) -> (PadControls, PadControls) {
    let ids = controls
        .pads
        .iter()
        .map(|p| &p.id)
        .chain(connected.iter().map(|c| &c.id));
    let pick = |pick: fn(PadBindings) -> Option<String>| {
        let mut out: PadControls = ids
            .clone()
            .filter_map(|id| {
                let standard = connected.iter().any(|c| c.id == *id && c.standard);
                Some((id.clone(), pick(controls.pad(id, standard)?)?))
            })
            .collect();
        out.sort();
        out.dedup();
        out
    };
    (pick(|p| p.back), pick(|p| p.start))
}

/// A play's bindings: the saved ones for the layout and the touch columns,
/// which follow the layout whatever its lane count.
pub fn play_bindings(
    controls: &ControlBindings,
    layout: &Layout,
    connected: &[ConnectedPad],
) -> Bindings {
    let mut b = controls.resolve(layout, connected);
    for lane in 0..layout.lanes.len() as u8 {
        b.bind(BindingDevice::Touch, &format!("lane:{lane}"), lane);
    }
    b
}

mod cover;
#[cfg(test)]
mod tests;
