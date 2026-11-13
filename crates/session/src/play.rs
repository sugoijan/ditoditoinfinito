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
use ddi_engine::rules::Ruleset;
use ddi_engine::{ConnectedPad, ControlBindings, FieldPlace, PadBindings};
use ddi_platform::{AudioBackend, DeviceId, DeviceProfile, HostTime, RawInput};
use ddi_ui::state::history::{
    Ghost, GhostDelta, GhostRun, InputDevice, PlayLog, Replay, Replaying, RulesetKey,
};
use ddi_ui::state::settings::Settings;

use crate::announcer::battle::{BATTLE_VOICE_V0, Told, VoicePolicy};
use crate::announcer::{Announcer, Ending, Moment, Prior, SharedVoice, Timing};
use crate::battle::{Battle, OpponentView, StageOutcome, StageSetup};
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
/// The control the touch lanes' quit button sends.
pub const TOUCH_CANCEL_CONTROL: &str = "cancel";
/// The control a tap or a click on a stage's attack policy label sends
/// (`ddi_shell`'s label by the power bar): it cycles the policy.
pub const TOUCH_POLICY_CONTROL: &str = "policy";
/// The keys that cycle a stage's attack policy (D8; Shift since the
/// maintainer's decision of 2026-11-13, Tab kept), each unless a lane is
/// bound to it: either Shift by physical code, and Tab.
pub use ddi_ui::controller_hints::POLICY_KEYS;

/// How the attack policy is changed during play, as the label beside it
/// says (`ddi_shell::battle_cue::draw_policy`): a key (its code, one of
/// [`POLICY_KEYS`]), or a pad's control (its Back) by its name.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PolicyKey {
    Key(&'static str),
    Pad(&'static str),
}
/// How long a pad's Back is held in a stage with attacks before its
/// quit's hint and bar show: a shorter press is the policy's (D8), which
/// shows nothing of the quit.
const POLICY_TAP: f64 = 0.25;
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
    /// `layout` (drawn reversed or not, at `place` across the screen), as
    /// the play starts.
    fn begin(&mut self, capture: Vec<String>, layout: &Layout, reverse: bool, place: FieldPlace);
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
    /// Prepared CPU presses, already bound to the projected chart. Automated,
    /// with human lane input disabled; the usual cancel gesture still works.
    pub cpu: Option<Vec<Press>>,
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
    /// Where the field is drawn across the screen (the touch lanes
    /// follow).
    pub place: FieldPlace,
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
    /// The announcer's gain (`Settings::voice_volume`;
    /// `docs/plans/announcers.md`).
    pub voice_volume: f32,
    /// What the history says of the chart before this play, for the
    /// results' first clear and new record (`None`: nothing to tell them
    /// from; never for autoplay or a replay).
    pub prior: Option<Prior>,
    /// A quick restart's play (decision 79): the announcer opens it with
    /// `gameplay restart`.
    pub restart: bool,
    /// A play staged for the announcer studio's scenes ([`Staging`]):
    /// autoplayed from part-way with what came before given; never kept.
    pub staging: Option<Staging>,
    /// A multiplayer stage against CPU opponents (`crate::battle`;
    /// `docs/plans/multiplayer-prototype-progress.md`, M3): they are judged
    /// beside the play; its gauge's fail is immediate (out of the stage),
    /// turned off once the player has won it; no turns, removals or rate;
    /// no quick restart; a quit is a forfeit. The solo CPU demonstration
    /// ([`Self::cpu`]) is another thing and is not combined with it.
    pub opponents: Option<StageSetup>,
    /// TESTS ONLY: a stage's ruleset as built here, in place of the match's
    /// (`crate::battle::stage_ruleset`): a gauge of one life, say, so that
    /// CPUs go out. Never set by a shell.
    pub stage_rules: Option<Ruleset>,
    /// A stage an online room plays live ([`Self::opponents`]' `live`;
    /// `crate::battle::live`): its room's client, through which the play
    /// starts at the room's instant, its moments are streamed and the
    /// others' come.
    pub live: Option<crate::battle::live::LiveHandle>,
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
            cpu: None,
            calibration: None,
            auto_bias: 0.0,
            devices,
            auto_pad: false,
            seed: 0,
            reverse: settings.reverse,
            place: settings.field_place,
            hide_notes: false,
            replay: None,
            ghost: None,
            start: None,
            voice_volume: settings.voice_volume(),
            prior: None,
            restart: false,
            staging: None,
            opponents: None,
            stage_rules: None,
            live: None,
        }
    }

    /// Resolve the exact effective chart before starting audio or reading inputs.
    /// Hosts use this same specification to preload chart accompaniment.
    pub fn player_spec(
        &self,
        song: &Song,
        chart_index: usize,
        settings: &Settings,
    ) -> Result<(Ruleset, PlayOptions), String> {
        let chart = song.charts.get(chart_index).ok_or("chart missing")?;
        song.layout_of(chart).ok_or("layout not playable")?;
        let rules = match (&self.replay, self.calibration, &self.opponents) {
            (Some(r), ..) => r.ruleset.build(chart),
            (None, Some(_), _) => self.ruleset.fresh(),
            // A stage: the match's ruleset, everyone's (D0), never the
            // player's own.
            (None, None, Some(stage)) => self.stage_rules.as_ref().map_or_else(
                || crate::battle::stage_ruleset(&stage.rules),
                Ruleset::fresh,
            ),
            (None, None, None) => settings.ruleset_for(chart),
        };
        let mut options = self.replay.as_ref().map_or(self.options, |r| r.options);
        let mut rules = rules;
        if self.opponents.is_some() {
            // A stage: out when the gauge empties; the chart as the CPUs
            // have it, at its own rate.
            rules.fail = ddi_engine::FailPolicy::Immediate;
            options.transform = Default::default();
            options.clock.rate = 1.0;
        }
        if self.replay.is_none() {
            options.seed = self.seed;
            options.scroll.speed = ddi_engine::scroll::speed_for_chart(
                song,
                chart_index,
                options.scroll.speed,
                settings.speed_source,
            );
        }
        // A stage with attacks: an extreme slow speed raised so that the
        // attacks' warning comes in time (D5; its prompt says so).
        if let Some(stage) = &self.opponents
            && stage.battle.attacks()
            && (!stage.remote() || stage.live.is_some())
            && self.replay.is_none()
            && let Some(speed) = crate::battle::raised_speed(song, chart_index, &options)
        {
            options.scroll.speed = speed;
        }
        Ok((rules, options))
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
/// (the sound-only test, the end of a calibration), the quit or restart
/// gesture's progress and hint (the one pressed last).
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct SessionView {
    pub hide_notes: bool,
    pub cancel_progress: f32,
    pub cancel_hint: &'static str,
    /// The gesture shown is the restart's.
    pub restarting: bool,
}

/// A gesture made by holding a control or pressing it twice (the quit's,
/// the restart's).
#[derive(Clone, Copy, Debug, Default)]
struct Gesture {
    /// Host time of the last press.
    pressed_at: Option<f64>,
    held: bool,
}

impl Gesture {
    /// A press or a release at `now`: whether it completes the gesture
    /// (a second press soon after the first).
    fn edge(&mut self, pressed: bool, now: f64) -> bool {
        if !pressed {
            self.held = false;
            return false;
        }
        let twice = self
            .pressed_at
            .is_some_and(|t| now - t <= CANCEL_DOUBLE_TAP);
        self.pressed_at = Some(now);
        self.held = true;
        twice
    }

    /// Its progress at `now`, 1 once held long enough; a sliver while
    /// the hint shows after a tap; 0 otherwise.
    fn progress(&self, now: f64) -> f64 {
        match self.pressed_at {
            Some(t) if self.held => ((now - t) / CANCEL_HOLD).clamp(0.0, 1.0),
            Some(t) if now - t <= CANCEL_HINT => 0.001,
            _ => 0.0,
        }
    }
}

/// The play's sound, stopped when the session goes, unless a restart
/// took it back ([`PlaySession::into_restart`]).
struct Sound<A: AudioBackend>(Option<A>);

impl<A: AudioBackend> std::ops::Deref for Sound<A> {
    type Target = A;
    fn deref(&self) -> &A {
        self.0.as_ref().expect("the session's sound")
    }
}

impl<A: AudioBackend> std::ops::DerefMut for Sound<A> {
    fn deref_mut(&mut self) -> &mut A {
        self.0.as_mut().expect("the session's sound")
    }
}

impl<A: AudioBackend> Drop for Sound<A> {
    fn drop(&mut self) {
        if let Some(a) = self.0.as_mut() {
            a.stop_all();
        }
    }
}

/// The shell's inputs, whose fast pad loop stops when the session goes.
struct Running<I: Inputs>(I);

impl<I: Inputs> std::ops::Deref for Running<I> {
    type Target = I;
    fn deref(&self) -> &I {
        &self.0
    }
}

impl<I: Inputs> std::ops::DerefMut for Running<I> {
    fn deref_mut(&mut self) -> &mut I {
        &mut self.0
    }
}

impl<I: Inputs> Drop for Running<I> {
    fn drop(&mut self) {
        self.0.pads_running(None);
    }
}

pub struct PlaySession<A: AudioBackend, I: Inputs> {
    pub player: Player,
    pub layout: Layout,
    /// The ruleset played by, fresh (its windows and gauge for the
    /// results).
    pub rules: Ruleset,
    /// The music's length, seconds.
    pub length: f64,
    pub view: SessionView,
    audio: Sound<A>,
    /// The music, for a restart ([`PlaySession::into_restart`]).
    buffer: A::Buffer,
    /// Music volume the session plays at (0 in the muted calibration).
    volume: f32,
    inputs: Running<I>,
    input: LaneInput,
    /// Pad controls that act like Escape: `(Gamepad.id, control)`.
    pad_cancel: Vec<(String, String)>,
    /// The device of the last press (what the policy's label names).
    last_device: Option<DeviceId>,
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
    cpu: bool,
    aborted: bool,
    last_frame_host: Option<f64>,
    frame_interval: f64,
    /// Host time at which an immediate fail was noticed.
    failed_at: Option<f64>,
    /// Signed error of the last judged tap, seconds.
    pub last_delta: Option<f64>,
    calibration: Option<(CalMode, Calibrator)>,
    /// The quit gesture (Escape, Backspace, a pad's Back, the touch quit
    /// button).
    cancel: Gesture,
    /// The restart gesture (R or `\`; none in a replay or a
    /// calibration), and whether it was made.
    restart: Option<Gesture>,
    restarted: bool,
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
    /// Where the music's song seconds play on the audio clock.
    timing: Timing,
    /// The announcer: its lines over the play and on its results (off in
    /// calibration; silent until the shell hands it a voice).
    announcer: Announcer<A::Buffer>,
    /// A staged play's end and restart, song seconds ([`Staging`]).
    scene_until: Option<f64>,
    scene_restart: Option<f64>,
    /// A multiplayer stage's CPU opponents, and its outcome once the play
    /// is over ([`SessionConfig::opponents`]).
    battle: Option<Battle>,
    stage_outcome: Option<StageOutcome>,
    /// The battle's moments as told (`crate::announcer::battle`): said by
    /// the announcer, or shown beside the field where not; whether the
    /// stage's opening has been told yet.
    told: Vec<Told>,
    opening_told: bool,
    /// The local player's attack moments as told (an attack against
    /// them, one of their own): shown in the status line.
    attack_told: Vec<crate::announcer::battle::AttackTold>,
    /// The stage's end as its results tell it, and the last frame's host
    /// seconds (when the results' moments are told).
    stage_end: Option<ddi_library::announcer::Battle>,
    last_host: f64,
    /// A pad's Back down in a stage with attacks, since when (host
    /// seconds): released soon, it cycles the attack policy (D8); held,
    /// it quits.
    policy_pad: Option<f64>,
    /// The stage is watched ([`watch`]): by a spectator from its start, by
    /// a player from when they are out of it (their life gone, or left).
    watch: Option<watch::Watch>,
    /// The session's event was given: the song goes on for nothing more.
    over: bool,
    /// The player's steps down so far as told: each one's rung and the
    /// host second it was noticed at (the status line's cue).
    steps_told: Vec<(u8, bool, f64)>,
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
    /// The bias its script was made with (a step down's new chart is
    /// scripted with it too).
    bias: f64,
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
    /// The player restarted: nothing kept, the same play to start again
    /// at once on the same sound and music ([`PlaySession::into_restart`]).
    Restart,
    /// A staged play's scene is over ([`Staging::until`]): nothing kept,
    /// the shell goes back to where it came from.
    SceneOver,
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

    /// After [`SessionEvent::Restart`]: the sound, silent and at full
    /// volume again (after a fail's fade, a work's fade out), and the
    /// music, for the next play's [`PlaySession::start`].
    pub fn into_restart(mut self) -> (A, A::Buffer) {
        let mut audio = self.audio.0.take().expect("the session's sound");
        audio.stop_all();
        audio.master_ramp(None, 1.0, None, 0.0);
        (audio, self.buffer)
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

    /// The announcer's sounds, decoded as the shell goes
    /// (`crate::announcer::wanted`): it speaks those there when each line
    /// is due.
    pub fn set_voice(&mut self, voice: SharedVoice<A::Buffer>) {
        self.announcer.set_voice(voice);
    }

    /// What the announcer did.
    pub fn announcer_report(&self) -> &crate::announcer::Report {
        &self.announcer.report
    }

    /// The sound, still playing what it plays (the results' line), for
    /// the shell to keep while the results show; dropped, it stops.
    pub fn into_sound(mut self) -> A {
        self.audio.0.take().expect("the session's sound")
    }

    /// Builds the session and schedules the start on the audio clock.
    pub fn start(
        song: &Song,
        chart_index: usize,
        settings: &Settings,
        mut config: SessionConfig,
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
        let (mut ruleset, options) = config.player_spec(song, chart_index, settings)?;
        let cpu = config.cpu.is_some();
        if config.opponents.is_some()
            && (cpu
                || config.replay.is_some()
                || config.calibration.is_some()
                || config.staging.is_some()
                || config.start.is_some())
        {
            return Err("a stage is played from its start, live".into());
        }
        if cpu {
            // A CPU demonstration completes the song even if its gauge empties.
            // Timing windows, note matching, scoring and the gauge stay unchanged.
            ruleset.fail = ddi_engine::FailPolicy::Off;
            if config.replay.is_some()
                || config.calibration.is_some()
                || config.staging.is_some()
                || config.start.is_some()
                || config.auto_pad
                || layout.lanes.len() != 4
                || config.options.transform != Default::default()
                || config.options.clock.rate != 1.0
                || chart
                    .notes
                    .iter()
                    .any(|n| n.kind != ddi_chart::NoteKind::Tap)
            {
                return Err("CPU requires an untransformed tap-only 4K chart at 1×".into());
            }
            ddi_engine::cpu::validate_script(config.cpu.as_ref().unwrap())?;
        }
        let staging = config.staging.clone().filter(|_| config.replay.is_none());
        // (A watched stage has no play of the viewer's own to autoplay.)
        let watched = config.opponents.as_ref().is_some_and(|s| s.watching);
        let auto_play =
            (config.auto || cpu || staging.is_some()) && config.replay.is_none() && !watched;
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
        // A stage with attacks: Shift and Tab cycle the policy, never
        // the page's focus.
        if config
            .opponents
            .as_ref()
            .is_some_and(|s| {
                (s.battle.attacks() || s.watching) && (!s.remote() || s.live.is_some())
            })
        {
            capture.extend(POLICY_KEYS.iter().map(|k| k.to_string()));
        }
        // A watched stage: nothing of the session's own player is played
        // or judged (its clock alone runs).
        let spectating = config.opponents.as_ref().is_some_and(|s| s.watching);
        inputs.begin(capture, &layout, config.reverse, config.place);

        let mut player = Player::new(song, chart_index, ruleset, options);
        // A stage whose zero life steps down: its ladder, the player's
        // rungs (a watcher has none of its own to play).
        let ladder = config
            .opponents
            .as_ref()
            .map_or_else(Vec::new, |s| crate::battle::stage_ladder(song, chart_index, s));
        if !spectating {
            for r in &ladder {
                if r.up {
                    player.add_rung_up(song, r.chart)?;
                } else {
                    player.add_rung(song, r.chart)?;
                }
            }
        }
        player.set_autoplay(auto_play);
        // A replay of an autoplayed record is graded as the play was.
        if config.replay.as_ref().is_some_and(|r| r.autoplayed) {
            player.set_bot(true);
        }
        // A locked lane keeps its mine's effect up, unless motion is
        // reduced (the hit's one effect then).
        player.set_lock_effect(!settings.reduced_motion.reduced(false));
        player.judge_all_in(any_lane);
        // The autoplayer presses the transformed chart's lanes (a staged
        // play's as its touches say).
        let mut presses = config.cpu.take().unwrap_or_else(|| {
            ddi_engine::autoplay::script(player.judge().notes(), config.auto_bias)
        });
        if let Some(s) = &staging {
            presses = staging::touched(presses, &s.touches);
        }
        let auto = auto_play.then(|| AutoPlay {
            bias: config.auto_bias,
            events: presses.clone(),
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
        // Reserve enough lead-in for the CPU's earliest press, including early
        // errors at the first note. The audio still begins at song time zero.
        if cpu
            && auto
                .as_ref()
                .is_some_and(|a| a.events.iter().any(|p| p.release > player.song_end_time()))
        {
            return Err("CPU motor queue extends past the chart's end; this chart is outside the prototype limits".into());
        }
        let cpu_lead = if cpu {
            auto.as_ref()
                .and_then(|a| a.events.first())
                .map_or(0., |p| {
                    (-p.press - config.options.clock.audio_offset).max(0.)
                })
        } else {
            0.
        };
        // A stage an online room plays live: song time zero heard at the
        // room's instant (`crate::battle::live::live_start`), joined late
        // where this device could not make it, not at all past the late
        // join (a forfeit, watched).
        let live_start = match (&config.live, &config.opponents) {
            (Some(h), Some(stage)) if stage.live.is_some() => {
                // On the room's client's clock as of now (nothing may have
                // ticked it since its screen went: an audience's seat
                // between its page and this play).
                h.room.borrow_mut().tick((h.wall_ms)());
                let zero_in = h.room.borrow().song_zero_in(stage.stage);
                zero_in.map(|zero_in| {
                    crate::battle::live::live_start(
                        zero_in,
                        sample.output_latency,
                        &ddi_room::live::LIVE_V1,
                    )
                })
            }
            _ => None,
        };
        let start_ctx = match live_start {
            // The context time its first second is heard at.
            Some(l) => sample.context_time + l.start + l.from,
            None => sample.context_time + START_LOOKAHEAD + cpu_lead,
        };
        // A work may start part-way into its music (`startFrame`) and end
        // or fade out before the music does.
        let danoni = chart
            .danoni
            .as_ref()
            .filter(|_| config.calibration.is_none());
        let song_offset = match (&config.replay, &staging) {
            _ if live_start.is_some() => live_start.map_or(0.0, |l| l.from),
            // Where the play started.
            (Some(r), _) => r.log.start,
            (None, Some(s)) => Some(s.from)
                .filter(|s| s.is_finite() && *s > 0.0 && *s < duration)
                .unwrap_or(0.0),
            (None, None) => config
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
        // A staged play: the notes before its start judged at once as
        // the autoplayer presses them, the presses from it on its own (a
        // hold pressed before it let go on time).
        let mut auto = auto;
        match (&staging, auto.as_mut()) {
            (Some(s), Some(a)) => {
                let mut past = ddi_engine::autoplay::edges(presses.iter().copied());
                ddi_engine::autoplay::sort_edges(&mut past);
                player.start_staged(start_ctx, song_offset, &past, s.combo);
                a.next = a.events.partition_point(|p| p.press < song_offset);
                a.pending_release = a.events[..a.next]
                    .iter()
                    .filter(|p| p.release >= song_offset)
                    .map(|p| (p.release, p.lane))
                    .collect();
            }
            // A late join: the notes it missed judged as misses.
            _ if live_start.is_some_and(|l| l.from > 0.0) => {
                player.start_staged(start_ctx, song_offset, &[], 0)
            }
            _ => player.start(start_ctx, song_offset),
        }
        // A stage's CPU opponents, on the same song and rules, from the
        // same second.
        let battle = match config.opponents.take() {
            Some(setup) => {
                // How far ahead the player reads: the attacks' leads
                // (only a battle with attacks needs it).
                let horizon = if setup.battle.attacks() {
                    player.reading_horizon()
                } else {
                    0.0
                };
                let b = Battle::new(
                    setup,
                    song,
                    chart_index,
                    player.ruleset(),
                    song_offset,
                    horizon,
                    &ladder,
                )?;
                Some(match config.live.take() {
                    Some(handle) => b.with_live(handle, player.ruleset().judge.lock),
                    None => b,
                })
            }
            None => None,
        };
        // DEVELOPMENT ONLY: a section of battle shocks in the stage's
        // play and each CPU's (`--mp-dev-mines`, `mpdevmines=`).
        let mut battle = battle;
        // Later than a live stage's late join: forfeited at once, the
        // stage watched from where the room has it.
        let late_watch = live_start.is_some_and(|l| l.late)
            && battle
                .as_mut()
                .is_some_and(|b| b.forfeit_now(song_offset, &player));
        if late_watch {
            player.quit_at(song_offset);
        }
        if let Some(b) = battle.as_mut()
            && let Some(beat) = b.setup().dev_mines_at_beat
        {
            let seed = b.setup().stage.0;
            // None fit from there: none (the debug state says so).
            if let Some(section) =
                crate::battle::dev_shocks(&mut player, song, chart_index, beat, seed)
            {
                b.dev_shocks(&section, song_offset);
            }
        }
        // The announcer, on the music's timeline: its first note is the
        // first judged one the play reaches.
        let first_note = player
            .judge()
            .notes()
            .iter()
            .filter(|n| n.judgeable && n.seconds >= song_offset)
            .map(|n| n.seconds)
            .reduce(f64::min);
        let timing = Timing {
            start: start_ctx,
            offset: song_offset,
            rate: player.clock().options().rate,
        };
        let announcer = Announcer::new(
            config.calibration.is_none(),
            config.voice_volume,
            config
                .prior
                .filter(|_| config.replay.is_none() && !auto_play),
            timing,
            first_note,
        );
        let announcer = if config.restart {
            announcer.restarted()
        } else {
            announcer
        };
        // A stage's voice: opened by the battle, its routine left out.
        let announcer = match &battle {
            Some(b) => announcer.in_stage(b.setup().opening(), BATTLE_VOICE_V0),
            None => announcer,
        };
        // A staged play has danced already where a note came before its
        // start (no ready, no here we go), its combo and comment gap given.
        let announcer = match &staging {
            Some(s) => {
                let danced = player
                    .judge()
                    .notes()
                    .iter()
                    .any(|n| n.judgeable && n.seconds < song_offset);
                announcer
                    .staged(danced, player.combo(), s.since)
                    .for_example(s.example)
            }
            None => announcer,
        };
        // Nothing may cue the player in calibration; a watcher has no notes
        // of their own to clap or step on.
        let sounds = if config.calibration.is_some() || spectating {
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
            rules,
            length: duration,
            view: SessionView {
                hide_notes: config.hide_notes,
                cancel_progress: 0.0,
                cancel_hint: "",
                restarting: false,
            },
            audio: Sound(Some(audio)),
            buffer,
            inputs: Running(inputs),
            input: LaneInput::new(bindings),
            pad_cancel,
            last_device: None,
            pad_start,
            cover: CoverControl::new(config.reverse),
            keep_cover: config.replay.is_none() && config.calibration.is_none(),
            known_pads,
            styles: settings.controller_styles.clone(),
            controls,
            held: vec![false; lanes],
            raw: Vec::new(),
            auto,
            cpu,
            aborted: false,
            last_frame_host: None,
            frame_interval: 1.0 / 60.0,
            failed_at: None,
            last_delta: None,
            calibration: config.calibration.map(|m| (m, Calibrator::default())),
            cancel: Gesture::default(),
            restart: (config.replay.is_none() && config.calibration.is_none() && battle.is_none())
                .then(Gesture::default),
            restarted: false,
            devices: config.devices,
            played_with: Vec::new(),
            latency,
            fail_report: None,
            replay: config.replay.map(|r| (r.log, 0, 0)),
            ghost: config.ghost.as_ref().filter(|_| !spectating).map(Ghost::run),
            ghost_delta: None,
            sounds,
            timing,
            announcer,
            scene_until: staging.as_ref().and_then(|s| s.until),
            scene_restart: staging.as_ref().and_then(|s| s.restart),
            battle,
            stage_outcome: None,
            told: Vec::new(),
            opening_told: false,
            stage_end: None,
            last_host: 0.0,
            attack_told: Vec::new(),
            policy_pad: None,
            // The watching begins with the first frame's host time.
            watch: if spectating {
                Some(watch::Watch::new(Watching::Spectator, f64::NAN))
            } else {
                late_watch.then(|| watch::Watch::new(Watching::Left, f64::NAN))
            },
            over: false,
            steps_told: Vec::new(),
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

    /// [`ddi_engine::calibration::stop_early`] on what
    /// [`Self::calibration_outcome`] counts, without working out the
    /// residuals every frame.
    fn calibration_stops_early(&self) -> bool {
        self.calibration.is_some() && {
            let t = self.player.tally();
            ddi_engine::calibration::stop_early(t.taps[0], t.taps[5])
        }
    }

    /// Advance to the current host time. Returns judge events for effects.
    pub fn tick(&mut self, host_now: HostTime) -> (Vec<JudgeEvent>, Option<SessionEvent>) {
        let mut events = Vec::new();
        let mut frame_dt = 0.0;
        if let Some(last) = self.last_frame_host {
            let dt = host_now.0 - last;
            if dt > 0.0 && dt < 0.25 {
                self.frame_interval = self.frame_interval * 0.9 + dt * 0.1;
                frame_dt = dt;
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
            if raw.pressed {
                self.last_device = Some(raw.device.clone());
            }
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
            // A stage with attacks: a pad's Back pressed briefly cycles
            // the attack policy (D8); held, it quits as ever. Pressing it
            // twice does not quit there: two quick presses are two steps
            // of the cycle, never a forfeit.
            if self.is_cancel(&raw)
                && matches!(raw.device, DeviceId::Gamepad(_))
                && (self.policy_pad.is_some() || self.policy_cycles())
            {
                if raw.pressed {
                    self.cancel.edge(true, raw.host_time.0);
                    self.policy_pad = Some(raw.host_time.0);
                    self.view.restarting = false;
                    self.view.cancel_hint = ddi_ui::controller_hints::quit_hold_hint(
                        &raw,
                        &self.known_pads,
                        &self.styles,
                    );
                } else {
                    self.cancel.edge(false, raw.host_time.0);
                    // Nothing of the quit lingers after a short press.
                    self.cancel.pressed_at = None;
                    if self
                        .policy_pad
                        .take()
                        .is_some_and(|at| raw.host_time.0 - at < CANCEL_HOLD)
                    {
                        self.cycle_policy(raw.host_time);
                    }
                }
                continue;
            }
            if self.is_cancel(&raw) {
                // A spectator's Escape goes back to every field; held or
                // pressed twice it still leaves, as ever.
                if raw.pressed
                    && raw.device == DeviceId::Keyboard
                    && let Some(w) = self.watch.as_mut()
                {
                    w.focus = None;
                }
                self.aborted |= self.cancel.edge(raw.pressed, raw.host_time.0);
                if raw.pressed {
                    self.view.restarting = false;
                    self.view.cancel_hint = match raw.device {
                        // No Esc key on a phone.
                        DeviceId::Touch => ddi_ui::controller_hints::quit_button_hint(),
                        DeviceId::Gamepad(_) => ddi_ui::controller_hints::quit_hint(
                            &raw,
                            &self.known_pads,
                            &self.styles,
                        ),
                        _ => "",
                    };
                }
                continue;
            }
            if let Some(hint) = self.restart_hint(&raw)
                && let Some(restart) = self.restart.as_mut()
            {
                self.restarted |= restart.edge(raw.pressed, raw.host_time.0);
                if raw.pressed {
                    self.view.restarting = true;
                    self.view.cancel_hint = hint;
                }
                continue;
            }
            // A replay's lanes and covers are its log's.
            if self.replay.is_some() || self.cpu {
                continue;
            }
            // Watching: no lane is played; the edges choose who is
            // followed, or skip to the results.
            if let Some(w) = self.watch.as_mut() {
                let menu = match &raw.device {
                    DeviceId::Gamepad(id) => {
                        let standard = self
                            .known_pads
                            .iter()
                            .any(|p| p.id == *id && p.index == raw.slot && p.standard);
                        self.controls.menu_input(id, standard, &raw.control)
                    }
                    _ => None,
                };
                if let Some(ask) = w.read(&raw, menu) {
                    let seats = self.battle.as_ref().map_or(0, |b| b.feeds().count());
                    w.apply(ask, seats);
                }
                continue;
            }
            // A stage's attack policy: Tab (unless a lane has it) or a
            // tap on its label.
            if self.is_policy_control(&raw) {
                if raw.pressed {
                    self.cycle_policy(raw.host_time);
                }
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

        // Hold-to-quit (or restart) progress and the hint after a single
        // tap.
        let quit = self.cancel.progress(host_now.0);
        let restart = self.restart.map_or(0.0, |r| r.progress(host_now.0));
        self.aborted |= quit >= 1.0;
        self.restarted |= restart >= 1.0;
        // A pad's Back in a stage with attacks: nothing of the quit shows
        // until it is held past a press of the policy's.
        let quit_shown = match self.policy_pad {
            Some(at) if host_now.0 - at < POLICY_TAP => 0.0,
            _ => quit,
        };
        self.view.cancel_progress = if self.view.restarting {
            restart
        } else {
            quit_shown
        } as f32;

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
                if self.cpu {
                    // CPU times are already in the song's timeline. A host-time
                    // round trip would apply different slewed clock drift at
                    // now and at the earlier input (especially after a stall).
                    self.held[usize::from(lane)] = pressed;
                    events.extend(self.player.input_at(
                        lane,
                        pressed,
                        song_t - self.rules.judge.judge_offset,
                    ));
                    continue;
                }
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

        // Up to the time the update judges to (a watcher's own player
        // is never judged: its clock alone runs).
        let spectating = self
            .watch
            .as_ref()
            .is_some_and(|w| w.why == Watching::Spectator);
        if !spectating {
            self.feed_replay(HostTime(judge_now), &mut events);
            events.extend(self.player.update(HostTime(judge_now)));
            self.follow_steps(host_now.0);
        }
        if let Some(w) = self.watch.as_mut()
            && w.since.is_nan()
        {
            w.since = host_now.0;
        }
        // The ghost judged to the same point (a failed play's stops).
        if let Some(g) = self.ghost.as_mut()
            && !self.player.finished()
        {
            let t = self.player.judge_time(HostTime(judge_now));
            self.ghost_delta = Some(g.delta(&self.player.ruleset().score.view(), t));
        }
        // A stage's opponents judged to the same point; won, the player
        // finishes the song whatever the gauge does.
        let mut moments = Vec::new();
        let mut attack_moments = Vec::new();
        if let Some(b) = self.battle.as_mut() {
            let t = self.player.judge_time(HostTime(judge_now));
            if b.advance(t, &mut self.player, &events) == Some(true) {
                self.player.set_fail(ddi_engine::FailPolicy::Off);
            }
            moments = b.take_events();
            attack_moments = b.take_attack_moments();
            // What someone watching is told of it (dropped while the
            // player plays: they are told their own).
            let seen = b.take_watch_moments();
            if let Some(w) = self.watch.as_mut() {
                for m in seen {
                    w.tell(m, host_now.0);
                }
            }
        }
        // The event sounds: a mine stepped on now (the step that failed
        // the play too), the claps due soon while it goes on.
        let now = sample.context_time;
        self.sounds.judged(&mut *self.audio, now, &events);
        if !self.player.finished() {
            let timing = self.timing;
            self.sounds
                .schedule(&mut *self.audio, now, |t| timing.at(t));
        }
        for ev in &events {
            if let JudgeEventKind::Tap(j, delta) = ev.kind
                && j != ddi_engine::rules::Judgement::Miss
            {
                self.last_delta = Some(delta);
                self.calibrate_hit(delta);
            }
        }
        // The announcer's lines due now (the moment read only for one
        // that speaks: it is off by default).
        if self.announcer.speaking() {
            let moment = self.moment(now, frame_dt, HostTime(judge_now));
            self.announcer.tick(&mut *self.audio, &moment);
        }
        if self.battle.is_some() {
            self.last_host = host_now.0;
            self.tell(moments, attack_moments, now, host_now.0);
        }

        // An immediate fail: the engine has stopped judging and the field is
        // blank; fade the music and hold the FAILED banner before the results.
        // (A delayed fail plays on: it comes here once the play ends.)
        if self.player.failed()
            && self.player.finished()
            && self.failed_at.is_none()
            && self.watch.is_none()
        {
            // Final now: kept even if the hold is cut short (a replay's is
            // not kept).
            if self.calibration.is_none() && self.replay.is_none() {
                self.fail_report = Some(SessionEvent::Failed {
                    results: Box::new(self.player.results()),
                    facts: Box::new(self.facts()),
                });
            }
            if self.battle.as_ref().is_some_and(Battle::goes_on) {
                // Out of a stage that goes on between the others: it is
                // watched to its end from the player's own dimmed field
                // (`watch-v0`), the music playing on; their play ended
                // here, and so did its results.
                self.watch = Some(watch::Watch::new(Watching::Out, host_now.0));
                self.held.iter_mut().for_each(|h| *h = false);
                self.cover.release();
            } else {
                self.failed_at = Some(host_now.0);
                self.audio.master_ramp(None, 0.0, None, FAIL_FADE);
                self.inputs.release();
            }
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
        // A staged play's restart (R twice) and its end.
        let song_now = self.player.judge_time(host_now);
        if self
            .scene_restart
            .is_some_and(|at| song_now >= at && !self.player.finished())
        {
            self.restarted = true;
            self.view.restarting = true;
        }
        let over = self.scene_until.is_some_and(|at| song_now >= at);
        // Leaving a stage's play while still in it forfeits the stage and
        // goes on to watch the rest of it (the maintainer, 2026-11-12):
        // out by quitting is out by life's path. Where nothing is left to
        // watch (the stage decided, an online room's: nobody else's play
        // is here) the play ends as ever.
        if self.aborted
            && self.watch.is_none()
            && let Some(b) = self.battle.as_mut()
            && b.goes_on()
            && b.forfeit_now(song_now, &self.player)
        {
            self.player.quit_at(song_now);
            self.aborted = false;
            self.cancel = Gesture::default();
            self.policy_pad = None;
            self.view.cancel_progress = 0.0;
            self.watch = Some(watch::Watch::new(Watching::Left, host_now.0));
            self.held.iter_mut().for_each(|h| *h = false);
            self.cover.release();
        }
        // A watched stage's end: the player's skip or quit, the song's
        // end, or a while after the last one still playing it ended (the
        // winner finishing, `watch-v1`; the music fading).
        let mut watch_ended = None;
        if let (Some(w), Some(b)) = (self.watch.as_mut(), self.battle.as_ref()) {
            // The seat it is watched through ended (an audience's): the
            // view stops (the music fading), says why and waits for the
            // way back (or the song's end).
            if !w.ended && b.seat_end().is_some() {
                w.ended = true;
                self.audio
                    .master_ramp(None, 0.0, None, watch::WATCH_END_FADE);
            }
            if w.over_at.is_none() && !w.ended && !b.goes_on() && !b.still_playing() {
                w.over_at = Some(host_now.0);
                self.audio
                    .master_ramp(None, 0.0, None, watch::WATCH_END_FADE);
            }
            let held = w
                .over_at
                .is_some_and(|at| host_now.0 - at >= WATCH_END_HOLD);
            let song_over = song_now >= self.player.end_time();
            if w.skip || self.aborted || held || song_over {
                watch_ended = Some((w.why, self.aborted));
            }
        }
        let outcome = if let Some((why, quit)) = watch_ended {
            let left = why == Watching::Left || (why == Watching::Spectator && quit);
            if left {
                self.audio.stop_all();
            } else {
                self.audio.stop_music();
            }
            let results = self.player.results();
            self.announce_results(&results);
            let (results, facts) = (Box::new(results), Box::new(self.facts()));
            Some(if left {
                // A play left (its results graded QUIT), or a watcher
                // leaving the stage.
                SessionEvent::Aborted { results, facts }
            } else {
                SessionEvent::Finished {
                    results,
                    facts,
                    calibration: None,
                    device_changed: self.inputs.device_changed(),
                }
            })
        } else if self.aborted {
            self.audio.stop_all();
            // The play ends here, judged to now (`Player::quit_at`).
            self.player.quit_at(self.player.judge_time(host_now));
            let results = self.player.results();
            self.announce_results(&results);
            Some(SessionEvent::Aborted {
                results: Box::new(results),
                facts: Box::new(self.facts()),
            })
        } else if self.restarted {
            self.audio.stop_all();
            Some(SessionEvent::Restart)
        } else if over {
            // The voice says its last line out; the music stops.
            self.audio.stop_music();
            Some(SessionEvent::SceneOver)
        } else if self.inputs.interrupted() || !self.audio.running() {
            self.audio.stop_all();
            Some(SessionEvent::Interrupted)
        } else if let Some(at) = self.failed_at {
            (host_now.0 - at >= FAIL_HOLD).then(|| {
                // The music (faded) and the event sounds; the voice's
                // `gameplay failed` goes on into the results.
                self.audio.stop_music();
                let results = self.player.results();
                self.announce_results(&results);
                SessionEvent::Finished {
                    results: Box::new(results),
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
        } else if self.player.finished() && self.watch.is_none() {
            Some(self.finished_event())
        } else {
            None
        };
        // A stage's outcome as the play ends: a quit or an interruption
        // is a forfeit, unless the stage was decided already.
        if let (Some(b), Some(event)) = (self.battle.as_mut(), outcome.as_ref()) {
            let quit = matches!(
                event,
                SessionEvent::Aborted { .. } | SessionEvent::Interrupted
            );
            let o = b.outcome(&self.player, quit);
            self.stage_end = b.end_event(&o, quit);
            self.announcer.stage_end(self.stage_end);
            self.stage_outcome = Some(o);
        }
        if outcome.is_some() {
            self.over = true;
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

    /// A stage's CPU opponents as the play shows them, in their slots
    /// (none: not a stage).
    pub fn opponents(&self) -> Vec<OpponentView> {
        self.battle.as_ref().map_or_else(Vec::new, Battle::views)
    }

    /// A stage's opponents, while one is played.
    pub fn battle(&self) -> Option<&Battle> {
        self.battle.as_ref()
    }

    /// A stage's attack commitments since the last call, worded for a
    /// log (English; `Battle::take_log`).
    pub fn take_battle_log(&mut self) -> Vec<String> {
        self.battle
            .as_mut()
            .map_or_else(Vec::new, crate::battle::Battle::take_log)
    }

    /// The battle's moments this frame (`crate::announcer::battle`), at
    /// audio time `now` and host time `host`: the stage's opening once
    /// the announcer has said it or could not, then each moment, the
    /// local player's attack moments among them, the one that matters
    /// most first, said where the policy lets it; each told, shown where
    /// its own line was not said. An attack against the player (`attacks`:
    /// each with its first shock's song second) is said before that shock
    /// can be seen or not at all, and one within
    /// [`VoicePolicy::incoming_again`] seconds of the one told before it
    /// (a chained attack right behind it) is not told again: the warning
    /// by the receptors says it.
    fn tell(
        &mut self,
        moments: Vec<ddi_library::announcer::Battle>,
        attacks: Vec<(crate::battle::AttackMoment, f64)>,
        now: f64,
        host: f64,
    ) {
        use crate::announcer::battle::AttackTold;
        use crate::battle::AttackMoment;
        if !self.opening_told
            && let Some(said) = self.announcer.opened_with()
            && let Some(b) = self.battle.as_ref()
        {
            self.opening_told = true;
            self.told.push(Told {
                event: b.setup().opening(),
                said,
                at: host,
            });
        }
        let policy = BATTLE_VOICE_V0;
        let mut due: Vec<_> = moments.into_iter().map(|e| (e, None)).collect();
        due.extend(attacks.into_iter().map(|(m, first)| (m.event(), Some((m, first)))));
        due.sort_by_key(|(e, _)| std::cmp::Reverse(policy.priority(*e)));
        for (event, attack) in due {
            let Some((moment, first)) = attack else {
                let said = self.announcer.battle(&mut *self.audio, event, now);
                self.told.push(Told {
                    event,
                    said,
                    at: host,
                });
                continue;
            };
            let incoming = moment == AttackMoment::Incoming;
            let again = incoming
                && self
                    .attack_told
                    .iter()
                    .rev()
                    .find(|t| t.moment == AttackMoment::Incoming)
                    .is_some_and(|t| host - t.at < policy.incoming_again);
            if again {
                continue;
            }
            // An attack against the player: its line's deadline is its
            // first shock coming into view.
            let by = incoming
                .then(|| self.timing.at(first - self.player.reading_horizon()));
            let said = self
                .announcer
                .battle_by(&mut *self.audio, event, now, by);
            self.attack_told.push(AttackTold {
                moment,
                said,
                at: host,
            });
        }
    }

    /// The local player's attack moments told so far (tests, the debug
    /// state).
    pub fn attacks_told(&self) -> &[crate::announcer::battle::AttackTold] {
        &self.attack_told
    }

    /// The player stepped to another chart (a match's "Charts:
    /// Adaptive"): told once (the status line's cue), and the
    /// autoplayer's presses from the boundary on made for the new chart
    /// (after a step up, the old chart's before it kept).
    fn follow_steps(&mut self, host: f64) {
        let steps = self.player.steps();
        if steps.len() <= self.steps_told.len() {
            return;
        }
        for st in &steps[self.steps_told.len()..] {
            self.steps_told.push((st.rung, st.up, host));
        }
        let Some(step) = steps.last().copied() else {
            return;
        };
        self.sounds.step(self.player.judge().notes(), step.at);
        if let Some(a) = self.auto.as_mut() {
            let early = self.player.ruleset().judge.max_early();
            let from = step.boundary - early;
            let keep = if step.up {
                a.next + a.events[a.next..].partition_point(|p| p.press < from)
            } else {
                a.next
            };
            let fed_until = a.events[..keep].last().map_or(f64::NEG_INFINITY, |p| p.release);
            let fresh: Vec<Press> = ddi_engine::autoplay::script(self.player.judge().notes(), a.bias)
                .into_iter()
                .filter(|p| p.press >= from && p.press > fed_until)
                .collect();
            a.events.truncate(keep);
            a.events.extend(fresh);
        }
    }

    /// The player's steps as told: each one's rung, whether it was up and
    /// the host second it was noticed at.
    pub fn steps_told(&self) -> &[(u8, bool, f64)] {
        &self.steps_told
    }

    /// What the status line shows at host time `host`, and how old it
    /// is: of the battle's moment no line said ([`PlaySession::battle_cue`])
    /// and the attack moments no line said, each while it lasts, the one
    /// that matters most (the policy's priorities), the newest of
    /// equals.
    pub fn stage_cue(&self, host: f64) -> Option<(crate::announcer::battle::StageCue, f64)> {
        use crate::announcer::battle::StageCue;
        let policy = self.battle_policy()?;
        let event = self
            .battle_cue(host)
            .map(|(e, age)| (policy.priority(e), StageCue::Event(e), age));
        let attacks = self
            .attack_told
            .iter()
            .filter(|t| !t.spoken())
            .map(|t| (t, host - t.at))
            .filter(|(_, age)| *age < policy.cue_seconds)
            .map(|(t, age)| (policy.attack_priority(t.moment), StageCue::Attack(t.moment), age));
        // A step of the player's own, as their going out would be.
        let stepped = self
            .steps_told
            .last()
            .map(|&(rung, up, at)| (rung, up, host - at))
            .filter(|(_, _, age)| *age < policy.cue_seconds)
            .map(|(rung, up, age)| {
                (
                    policy.priority(ddi_library::announcer::Battle::Eliminated),
                    StageCue::Step { rung, up },
                    age,
                )
            });
        event
            .into_iter()
            .chain(attacks)
            .chain(stepped)
            .max_by(|a, b| a.0.cmp(&b.0).then(b.2.total_cmp(&a.2)))
            .map(|(_, cue, age)| (cue, age))
    }

    /// The room confirmed this stage's outcome (the shell reported it),
    /// the match `ending` with it where it was the last: the results'
    /// battle line, the match's over the stage's.
    pub fn stage_reported(&mut self, ending: Option<ddi_ui::state::room::MatchEnd>) {
        if self.battle.is_none() {
            return;
        }
        let ending = ending.map(crate::announcer::battle::match_event);
        let now = self.audio.now().context_time;
        let said = self.announcer.stage_results(&mut *self.audio, now, ending);
        for event in ending.into_iter().chain(self.stage_end.take()) {
            // Said by its own chain, or not (the match's took its place).
            let line = ddi_library::announcer::Line::Battle(event);
            self.told.push(Told {
                event,
                said: said.filter(|l| line.chain().contains(l)),
                at: self.last_host,
            });
        }
    }

    /// The battle's moments told so far (`__DDI_DEBUG.battle`, tests).
    pub fn battle_told(&self) -> &[Told] {
        &self.told
    }

    /// The battle voice's policy, for the cues' look.
    pub fn battle_policy(&self) -> Option<VoicePolicy> {
        self.battle.as_ref().map(|_| BATTLE_VOICE_V0)
    }

    /// The moment shown beside the field at host time `host`, and how old
    /// it is: the latest the policy shows, while it lasts.
    pub fn battle_cue(&self, host: f64) -> Option<(ddi_library::announcer::Battle, f64)> {
        let policy = self.battle_policy()?;
        let t = self.told.iter().rev().find(|t| t.shown(&policy))?;
        let age = host - t.at;
        (age < policy.cue_seconds).then_some((t.event, age))
    }

    /// A stage's outcome, once, after the play's end
    /// ([`SessionEvent::Finished`], `Aborted` or `Interrupted`), for the
    /// shell to report to the room.
    pub fn take_stage_outcome(&mut self) -> Option<StageOutcome> {
        self.stage_outcome.take()
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

    fn finished_event(&mut self) -> SessionEvent {
        let results = self.player.results();
        self.announce_results(&results);
        SessionEvent::Finished {
            results: Box::new(results),
            facts: Box::new(self.facts()),
            calibration: self.calibration_outcome(),
            device_changed: self.inputs.device_changed(),
        }
    }

    /// Whether the play shows StepMania's danger to a song's scripts
    /// (`BackgroundImpl::IsDangerAllVisible`): the gauge in danger, failing
    /// not off, not failed (it is too late then).
    pub fn danger_shown(&self) -> bool {
        use ddi_engine::rules::FailPolicy;
        let rules = self.player.ruleset();
        rules.gauge.danger() && !matches!(rules.fail, FailPolicy::Off) && !self.player.failed()
    }

    /// The play as the announcer reads it now: at context time `now`,
    /// `dt` after the last frame, judged to `judged`.
    fn moment(&self, now: f64, dt: f64, judged: HostTime) -> Moment {
        use ddi_engine::rules::FailPolicy;
        let rules = self.player.ruleset();
        let fail = &rules.fail;
        // Failed at once; else the last note judged; a play failing at
        // its end once that end is known.
        let ended = self.player.finished()
            || (self.player.judge().all_resolved()
                && !matches!(fail, FailPolicy::EndOfSong { .. }));
        Moment {
            now,
            dt,
            song_time: self.player.judge_time(judged),
            combo: self.player.combo(),
            life: rules.gauge.life(),
            danger: rules.gauge.danger() && !matches!(fail, FailPolicy::Off),
            lives: rules.gauge.lives(),
            failed: self.player.failed(),
            ended,
            // Worked out once, as the end comes.
            full_combo: if ended && !self.announcer.ended() {
                self.player.results_brief().full_combo
            } else {
                ddi_engine::rules::FullCombo::None
            },
        }
    }

    /// The results' line, on the play's sound as they open.
    fn announce_results(&mut self, results: &Results) {
        let now = self.audio.now().context_time;
        let rules = self.player.ruleset();
        let combo_w4 = rules.combo.continue_min == ddi_engine::rules::Judgement::W4;
        self.announcer.results(
            &mut *self.audio,
            now,
            &Ending::of(results),
            &rules.grades,
            combo_w4,
        );
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

    /// The restart's hint when `raw` is a restart key's edge: R or `\`,
    /// held or pressed twice as the quit's (`docs/PLAN.md` decision 79),
    /// unless a lane is bound to it (Dancing☆Onigiri 9A's R, a player's
    /// binding), which it then plays.
    fn restart_hint(&self, raw: &RawInput) -> Option<&'static str> {
        if raw.device != DeviceId::Keyboard
            || self
                .input
                .bindings
                .lane_of_slot(&raw.device, raw.slot, &raw.control)
                .is_some()
        {
            return None;
        }
        let key = match raw.control.as_str() {
            "KeyR" => "R",
            "Backslash" => "\\",
            _ => return None,
        };
        Some(ddi_ui::controller_hints::restart_hint(key))
    }

    /// Whether the attack policy can be cycled now: a stage with attacks
    /// played live, the player still in and the stage not won.
    fn policy_cycles(&self) -> bool {
        self.replay.is_none()
            && !self.cpu
            && self
                .battle
                .as_ref()
                .is_some_and(|b| b.policy(&self.player).is_some())
    }

    /// The attack policy's own controls (D8): either Shift and Tab
    /// ([`POLICY_KEYS`]), each unless the layout or the player's bindings
    /// give it to a lane (it then plays the lane), and a tap or a click on
    /// its label. (A pad's Back is told apart in
    /// the quit's place: pressed briefly it cycles, held it quits.)
    fn is_policy_control(&self, raw: &RawInput) -> bool {
        if !self.policy_cycles() {
            return false;
        }
        match &raw.device {
            DeviceId::Keyboard => {
                POLICY_KEYS.contains(&raw.control.as_str())
                    && self
                        .input
                        .bindings
                        .lane_of_slot(&raw.device, raw.slot, &raw.control)
                        .is_none()
            }
            DeviceId::Touch => raw.control == TOUCH_POLICY_CONTROL,
            _ => false,
        }
    }

    /// How the attack policy is changed now, for the label beside it:
    /// the pad's Back where a pad was pressed last (named as its cap
    /// is), nothing after a tap (the label is tapped), else the first
    /// policy key no lane has; none where the policy does not cycle.
    pub fn policy_key(&self) -> Option<PolicyKey> {
        if !self.policy_cycles() {
            return None;
        }
        match &self.last_device {
            Some(DeviceId::Touch) => None,
            Some(DeviceId::Gamepad(id)) => {
                let control = self.pad_cancel.iter().find(|(p, _)| p == id)?;
                ddi_ui::controller_hints::pad_control_name(
                    &self.known_pads,
                    &self.styles,
                    id,
                    &control.1,
                )
                .map(PolicyKey::Pad)
            }
            _ => ddi_ui::controller_hints::policy_key(|k| {
                self.input
                    .bindings
                    .lane_of_slot(&DeviceId::Keyboard, 0, k)
                    .is_some()
            })
            .map(PolicyKey::Key),
        }
    }

    /// The next attack policy, from the judged song second of host time
    /// `host`: kept in the play's log and given to the ledger, which
    /// launches by it from its next checkpoint.
    fn cycle_policy(&mut self, host: HostTime) {
        let Some(b) = self.battle.as_mut() else {
            return;
        };
        let Some(now) = b.policy(&self.player) else {
            return;
        };
        let next = crate::battle::next_policy(now);
        let t = self.player.judge_time(host);
        if let Some(at) = self
            .player
            .set_policy_at(crate::battle::policy_code(next), t)
        {
            b.retarget(next, at);
        }
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
    ///
    /// A watched stage's ([`PlaySession::watch`]): a spectator's is the
    /// field of the competitor they follow as its stream of judged
    /// moments says it (`crate::battle::feed::FieldFeed::frame_large`:
    /// its notes, its judgment's word, its life, combo and score), or an
    /// empty field while every field shows; a player's out of the stage
    /// is their own field, blank (the shell dims it and says why), or the
    /// competitor they follow, as a spectator's.
    pub fn frame(&self, present: HostTime) -> Frame {
        let mut frame = self.player.frame(present);
        if self.view.hide_notes {
            frame.beat = 0.0;
        }
        let Some(w) = &self.watch else {
            return frame;
        };
        // (A player out of the stage who follows one: theirs too.)
        let feed = self
            .battle
            .as_ref()
            .and_then(|b| Some((b, b.feeds().nth(w.focus?)?)));
        if let Some((b, feed)) = feed {
            let options = self.player.options();
            let mut large = feed.frame_large(
                b.track_of(w.focus.unwrap_or(usize::MAX)),
                options.scroll,
                frame.song_time,
                options.visible_range,
            );
            // Out is said over the field, as a miniature's.
            large.failed = false;
            return large;
        }
        frame.notes.clear();
        frame.receptors.fill(ddi_engine::frame::ReceptorState {
            pressed: false,
            mine: None,
            flash: None,
            holding: None,
        });
        frame.judgement = None;
        frame.holds = Default::default();
        frame.failed = false;
        frame.danger = false;
        if w.why == Watching::Spectator {
            frame.combo = 0;
            frame.score = Default::default();
            frame.life = 0.0;
        }
        frame
    }

    /// The player leaves the play from outside it (the window asked to
    /// close, the browser's Back): as their quit gesture made whole. A
    /// stage's is a forfeit: where its others play on here (CPU
    /// practice) the rest of it is watched on, else (an online room's:
    /// nobody else's play is here yet) the play ends at the next frame
    /// and its results follow, where the room's question can be asked.
    pub fn leave(&mut self) {
        self.aborted = true;
    }

    /// The stage as it is watched, at host second `host`: by a spectator
    /// from its start, by a player from when they are out of it; none
    /// while the player plays (and outside a stage).
    pub fn watch(&self, host: f64) -> Option<WatchView> {
        self.watch.as_ref().map(|w| w.view(host))
    }

    /// Where the dancing hand rests (judged song second): a player out of
    /// the stage (their gauge emptied) or who left its play stops there
    /// rather than dancing on behind their dimmed field; none while they
    /// play (and for a spectator, who has no hand).
    pub fn hand_rests_at(&self) -> Option<f64> {
        let w = self.watch.as_ref()?;
        if w.why == Watching::Spectator {
            return None;
        }
        self.player
            .gauge_failed_at()
            .or_else(|| self.battle.as_ref().and_then(Battle::forfeited_at))
            .or_else(|| self.player.judge().last_time())
    }

    /// The song goes on for what is drawn: the play's own, or a stage
    /// watched on after the player's play ended; not once the session
    /// has given its event.
    pub fn song_going(&self) -> bool {
        self.player.started()
            && !self.over
            && (self.watch.is_some() || !self.player.finished())
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
pub mod staging;
pub mod watch;
pub use staging::{How, Staging, Touch};
pub use watch::{
    GRID_PAGE, TOUCH_PAGE_SEAT, TOUCH_SKIP_CONTROL, TOUCH_WATCH_ALL, TOUCH_WATCH_CONTROL,
    WATCH_END_HOLD, WATCH_HINT_SECONDS, WatchView, Watching,
};
#[cfg(test)]
mod tests;
