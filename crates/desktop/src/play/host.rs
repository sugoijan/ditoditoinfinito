//! The shell's side of a play: song select's play call starts one, the
//! start prompt (the UI core's, `ddi_ui::screens::prompt`, as the web's)
//! waits for the song (read on a thread), the sound to run and Start, the
//! play runs with the keyboard and pads its own, and its results come up
//! on the theme's screen with retry and the song list. The window, the
//! surface and the UI stay the shell's; this says what to do with them.

use std::cell::{Cell, RefCell};
use std::rc::Rc;
use std::sync::Arc;
use std::sync::mpsc::{self, Receiver};
use std::time::{Duration, Instant};

use ddi_library::backgrounds::BgImage;
use ddi_platform::{DeviceFingerprint, DeviceProfile, HostTime};
use ddi_render::RenderOptions;
use ddi_session::announcer::scene::director::{Heard, SaidWords};
use ddi_session::announcer::scene::in_play::{self, PlayScene};
use ddi_ui::Ui;
use ddi_ui::screens::Env;
use ddi_ui::screens::data::SongEntry;
use ddi_ui::screens::play::{PlayInfo, results_data};
use ddi_ui::screens::prompt::{
    Badge, PromptChart, PromptChoice, PromptFn, PromptInfo, PromptScreen, PromptState, PromptStep,
    SharedPrompt, chart_line,
};
use ddi_ui::screens::prompt_options::{PlayOptions, PlayOverrides};
use ddi_ui::screens::results::{ActionFn, ResultsActions, ResultsScreen};
use ddi_ui::screens::stage::{self, Picture as PictureScreen, StageFrame};
use ddi_ui::state::history::Plays;
use ddi_ui::state::settings::Settings;

use super::load::{Loaded, Loading};
use super::view::{Formats, Rgba, View};
use super::{Ended, Feed, Play, Request};
use crate::audio::Audio;

/// How long the prompt waits for the play's sound to run before it
/// offers Start anyway (a device that never calls back: the play then
/// stops as interrupted).
const SOUND_WAIT: Duration = Duration::from_secs(3);
/// How long Start waits for the song's pictures (a slow disk, a large
/// image): then it plays with those read so far.
const PICTURES_WAIT: Duration = Duration::from_secs(5);

/// What a play needs from the device.
pub struct Device<'a> {
    pub device: &'a wgpu::Device,
    pub queue: &'a wgpu::Queue,
    pub formats: Formats,
}

/// A song readied for its plays: loaded, its view (renderer, skin,
/// backgrounds) made, its prompt, its sound opened.
struct Readied {
    request: Request,
    prompt: SharedPrompt,
    loading: Option<Loading>,
    /// When the song was asked for, and the output's rate it is read at.
    asked: Instant,
    rate: u32,
    /// A bundled song (known at once).
    bundled: bool,
    loaded: Option<Rc<Loaded>>,
    images: Option<Receiver<(BgImage, Rgba)>>,
    view: Option<View>,
    /// The song could not be read: why.
    error: Option<ddi_ui::screens::failures::LoadFailure>,
    /// The play's sound, opened while the prompt is up so the play
    /// starts on a running output (its clock, its latency and its
    /// device known), and when it was opened.
    audio: Option<(Audio, Instant)>,
    /// The song's background as a texture source, and its shape, for
    /// the prompt's picture.
    picture: Option<(String, f32)>,
    /// For a song without one, the generated background's first frame
    /// held still in its place (the painter's under `still_source`,
    /// [`ddi_shell::generative::Stills`]), and the play's first beat it
    /// is drawn at.
    still: Option<ddi_shell::generative::Still>,
    still_source: String,
    first_beat: Option<f64>,
    /// The skin being read on its thread, for its layout.
    skin: Option<(super::skins::Reading, ddi_chart::Layout)>,
    /// When its drawing was made (the pictures are waited for a while).
    view_made: Option<Instant>,
    /// A work's layers' contents, read on a thread.
    stage: Option<Receiver<super::stage::Read>>,
    /// The song's scripted backgrounds, readied while the prompt is up.
    scripts: SongScripts,
    /// Start was pressed (or a new device's choice made): the play
    /// begins once everything is ready, the loading screen up meanwhile
    /// (`docs/plans/start-prompt.md`).
    started: bool,
    /// Everything the play waits for is ready (as of the last poll).
    ready: bool,
    play_seed: u64,
    /// The play options on the prompt (`ddi_ui::screens::prompt_options`;
    /// none for a test or a replay), the ones its drawing was readied
    /// with, and the song folders' skins for its skin row, listed on a
    /// thread.
    options: Option<Rc<RefCell<PlayOptions>>>,
    applied: PlayOverrides,
    skins: Option<Receiver<Vec<(String, String)>>>,
    /// The announcer's sounds for the play, decoded on a thread
    /// (`super::voice`; none without an announcer, or in calibration).
    voice: Option<super::voice::VoiceLoad>,
    /// The ended play's sound, kept while its results show: their line
    /// speaks on it.
    lingering: Option<Audio>,
    /// The announcer studio's scene in play, if this is one.
    scene: Option<SceneRun>,
}

/// The announcer studio's scene in play under way: how it is staged and
/// what it said so far.
struct SceneRun {
    order: Rc<super::SceneOrder>,
    /// Its play, staged once the song is read (`None` after: no play
    /// gives the line).
    staged: Option<Option<PlayScene>>,
    /// How long it lasts from its play's start, seconds (the frame's bar).
    seconds: f64,
    /// The frame's bar was told.
    bar_told: bool,
    /// The frame is told what happens at the next poll.
    tell: bool,
    /// The lines said so far (the play's, then its results').
    spoken: Vec<String>,
    /// The words of what the play says, as each line starts.
    words: SaidWords,
    /// Back to the studio then, once its results' line is said.
    leave_at: Option<Instant>,
    /// Where the play shows what can be ringed.
    spots: stage::SharedSpots,
}

impl SceneRun {
    fn new(order: Rc<super::SceneOrder>) -> SceneRun {
        let words = SaidWords::new(order.line, order.words.clone(), order.lengths.clone());
        SceneRun {
            order,
            staged: None,
            seconds: 0.0,
            bar_told: false,
            tell: false,
            spoken: Vec::new(),
            words,
            leave_at: None,
            spots: Default::default(),
        }
    }

    /// What it said in the play just ended.
    fn keep_said(&mut self, play: &Play) {
        let report = play.session.announcer_report();
        self.spoken
            .extend(report.spoken.iter().map(|(f, _)| f.clone()));
    }
}

/// The texture a scene's play is drawn into, shown in the frame.
struct Picture {
    texture: wgpu::Texture,
    view: wgpu::TextureView,
    /// The painter has it.
    shown: bool,
}

/// The picture's view at `size` (made again at another).
fn picture_view<'a>(
    picture: &'a mut Option<Picture>,
    device: &wgpu::Device,
    size: [u32; 2],
) -> &'a wgpu::TextureView {
    let stale = picture
        .as_ref()
        .is_none_or(|p| [p.texture.width(), p.texture.height()] != size);
    if stale {
        let texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("ddi-staged-play"),
            size: wgpu::Extent3d {
                width: size[0].max(1),
                height: size[1].max(1),
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: ddi_render::field_preview::FORMAT,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING,
            view_formats: &[],
        });
        let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
        *picture = Some(Picture {
            texture,
            view,
            shown: false,
        });
    }
    &picture.as_ref().expect("made above").view
}

/// A song's scripted backgrounds (`super::scripts`): readied once for
/// all its plays, kept here while no view draws them (a view made again
/// after a lost device takes them as they were).
enum SongScripts {
    /// Not started: the song is not read yet.
    Unread,
    Loading(Box<super::scripts::ScriptLoad>),
    /// Ready for the next view (`None`: none to show).
    Held(Option<Box<super::scripts::Ready>>),
    /// The view draws them.
    Given,
}

impl Readied {
    /// The settings the play runs with: the request's, the prompt's play
    /// options over them.
    fn settings(&self) -> Settings {
        let mut s = self.request.settings();
        if let Some(o) = &self.options {
            o.borrow().over(&mut s);
        }
        // A scene's play: the gauge and the rules its line wants (never
        // stored).
        if let Some(pick) = self
            .scene
            .as_ref()
            .and_then(|r| in_play::pick(r.order.line))
        {
            s = pick.settings(&s);
        }
        s
    }

    /// The song's drawing let go (a lost device): its scripts are kept.
    fn drop_view(&mut self) {
        if let Some(mut v) = self.view.take()
            && matches!(self.scripts, SongScripts::Given)
        {
            self.scripts = SongScripts::Held(v.take_scripts().map(Box::new));
        }
    }
}

enum Stage {
    Idle,
    /// The start prompt is up.
    Prompt(Box<Readied>),
    Playing(Box<Readied>, Box<Play>, Box<RenderOptions>),
    /// The results are up: retry or leave.
    Results(Box<Readied>),
}

/// What the results' buttons asked for.
#[derive(Clone, Debug, PartialEq, Eq)]
enum After {
    Retry,
    Leave,
    /// Watch kept play `id`'s replay.
    Watch(String),
}

/// Opens calibration's menu (the shell's: its buttons reach the
/// library's play call and the options).
pub type MenuFn = Rc<dyn Fn(&Env, &mut Ui)>;

/// Hears a song's id and chart once read ([`Host::chart_loaded`]).
pub type ChartLoadedFn = Rc<dyn Fn(&str, &ddi_chart::Song)>;

/// What covers the screens under a play (song select, calibration's
/// menu) while it runs ([`Host::start`]): it draws nothing and takes
/// nothing (the play has the keys and the pointer), and goes once the
/// play has, whatever came up over it first (its results, the prompt
/// again), so the screens under it are told they show only then.
struct PlayScreen {
    in_play: Rc<Cell<bool>>,
    /// A scene's play: its picture, shown in the frame.
    picture: Option<PictureScreen>,
}

impl ddi_ui::Screen for PlayScreen {
    fn name(&self) -> &str {
        "play"
    }

    fn build(&mut self, cx: &mut ddi_ui::Cx) -> ddi_actor::Actor {
        match &mut self.picture {
            Some(p) => p.build(cx),
            None => ddi_actor::Actor::new(ddi_actor::Kind::Frame(Vec::new())),
        }
    }

    fn update(&mut self, dt: f32, root: &mut ddi_actor::Actor, cx: &mut ddi_ui::Cx) {
        if let Some(p) = &mut self.picture {
            p.update(dt, root, cx);
        }
        if !self.in_play.get() {
            cx.close(false);
        }
    }

    fn spot(&self, what: ddi_ui::Mark) -> Option<ddi_ui::Spot> {
        self.picture.as_ref()?.spot(what)
    }
}

pub struct Host {
    /// Song select's play call puts what to play here.
    pub requested: Rc<RefCell<Option<Request>>>,
    pub feed: Rc<RefCell<Feed>>,
    stage: Stage,
    /// A play runs: its [`PlayScreen`] stays while it does.
    in_play: Rc<Cell<bool>>,
    /// Wakes the event loop (a song read on its thread).
    pub wake: Option<Arc<dyn Fn() + Send + Sync>>,
    /// The play beginning is a quick restart's ([`Request::restart`]).
    restarting: bool,
    pub plays: Option<Rc<Plays>>,
    /// Told a song's id and chart once read for its prompt (the history
    /// works its stale plays out first, `Songs::chart_loaded`).
    pub chart_loaded: Option<ChartLoadedFn>,
    after: Rc<Cell<Option<After>>>,
    /// What the prompt's buttons chose, for the next poll.
    choice: Rc<RefCell<Option<PromptChoice>>>,
    /// The devices as the prompt's Start sees them (new ones are offered
    /// their offsets first).
    devices: Rc<RefCell<Option<DeviceProfile>>>,
    /// The prompt's state as last drawn (a change asks for a frame).
    prompt_seen: u64,
    /// The window's display, for the offsets' profile.
    pub display: Option<DeviceFingerprint>,
    /// The display's refresh rate as the OS says it, until frames have
    /// measured it.
    pub refresh_hint: Option<f64>,
    /// The devices as last seen, and the notices about them.
    pub watcher: crate::devices::Watcher,
    /// Calibration's menu, opened again when a test leaves.
    pub menu: Option<MenuFn>,
    /// What each play ended with (a timed run prints them).
    pub log: Vec<String>,
    /// The graphics backend's name, for the records.
    pub graphics: String,
    /// The refresh the next frame is for, when the shell's pacing knows
    /// it (`crate::pacing`, late pacing); else the play predicts it.
    pub present_at: Option<HostTime>,
    /// Plays on no device with this clock (tests): `None` opens the
    /// output (or none under `--silent`).
    pub test_clock: Option<crate::audio::HostClock>,
    /// The master volume over the music's (`--volume`): 1 unless given.
    pub master: Option<f32>,
    /// Start the next prompt by itself once the song is read (`--start`).
    pub start_when_ready: bool,
    /// The window's scale factor (device pixels per point), for sizes
    /// given in points, such as the lyrics' smallest.
    pub scale: f32,
    /// A scene's way back was asked for (its frame's button, Escape).
    scene_back: Rc<Cell<bool>>,
    /// What a scene's play is drawn into.
    picture: Option<Picture>,
    /// How long a scene's results show (tests shorten it).
    pub scene_results: Duration,
}

impl Default for Host {
    fn default() -> Host {
        Host {
            requested: Rc::default(),
            feed: Rc::default(),
            stage: Stage::Idle,
            in_play: Rc::default(),
            wake: None,
            restarting: false,
            plays: None,
            chart_loaded: None,
            after: Rc::default(),
            choice: Rc::default(),
            devices: Rc::default(),
            prompt_seen: 0,
            display: None,
            refresh_hint: None,
            watcher: crate::devices::Watcher::default(),
            menu: None,
            log: Vec::new(),
            graphics: String::new(),
            present_at: None,
            test_clock: None,
            master: None,
            start_when_ready: false,
            scale: 1.0,
            scene_back: Rc::default(),
            picture: None,
            scene_results: Duration::from_secs_f64(in_play::RESULTS_SECONDS),
        }
    }
}

impl Host {
    /// Plays what song select puts in `requested`, kept in `plays`.
    pub fn new(requested: Rc<RefCell<Option<Request>>>, plays: Option<Rc<Plays>>) -> Host {
        Host {
            requested,
            plays,
            ..Host::default()
        }
    }
}

/// What the prompt names before the song is read: a test's title and
/// hint, or the song's title and artist from the library.
fn first_info(request: &Request) -> PromptInfo {
    let badge = Badge::new(
        request.replay.is_some() || request.refused.is_some(),
        request.auto,
    );
    if let Some(mode) = request.calibration {
        return PromptInfo::for_test(mode.title(), mode.hint(), badge);
    }
    let entry = crate::library::song(&request.song)
        .map(|s| s.entry)
        .or_else(|| {
            crate::library::bundled()
                .into_iter()
                .find(|e| e.id == request.song)
        });
    PromptInfo {
        title: entry.as_ref().map_or(String::new(), |e| e.title.clone()),
        artist: entry.map_or(String::new(), |e| e.artist),
        badge,
        ..PromptInfo::default()
    }
}

/// What the prompt names once the song is read: its chart and the mods
/// it will play with too (the web's `prompt_info`).
fn full_info(request: &Request, loaded: &Loaded, settings: &Settings) -> PromptInfo {
    let info = first_info(request);
    if request.calibration.is_some() {
        return info;
    }
    let mut info = PromptInfo::for_chart(
        loaded.entry.title.clone(),
        loaded.entry.artist.clone(),
        PromptChart::of(&loaded.song, request.chart),
        settings,
        info.badge,
    );
    if let Some(cpu) = &request.cpu {
        info.set_cpu_with_skills(cpu.level, cpu.seed, cpu.skills);
    }
    if let Some(order) = &request.stage
        && order.watching
    {
        // Watched (a member's Watch, or with the room's audience).
        info.set_watched_stage(
            order.number,
            order.of,
            order.cpus.len() + usize::from(order.others),
            &order.rules,
            order.attacks_rule(),
        );
    } else if let Some(order) = &request.stage {
        info.set_stage(
            order.number,
            order.of,
            order.cpus.len(),
            order.others,
            &order.rules,
            order.attacks_rule(),
        );
        if order.battle.attacks()
            && order.others == 0
            && ddi_session::battle::stage_speed_raised(&loaded.song, request.chart, settings)
        {
            info.set_speed_raised();
        }
    }
    info
}

impl Host {
    /// The output a play runs on, while one runs (the devices' watcher
    /// looks at it then, not the previews').
    pub fn play_fingerprint(&self) -> Option<DeviceFingerprint> {
        match &self.stage {
            Stage::Playing(_, play, _) => play.session.audio().fingerprint(),
            _ => None,
        }
    }

    /// A scene's play runs: drawn into a picture in its frame, the
    /// pointer the frame's.
    pub fn framed(&self) -> bool {
        matches!(&self.stage, Stage::Playing(r, ..) if r.scene.is_some())
    }

    /// A scene's way back was asked for: it leaves at the next poll.
    pub fn back_from_scene(&self) {
        self.scene_back.set(true);
    }

    /// There is a scene's picture: the UI draws it, in the frame.
    pub fn has_picture(&self) -> bool {
        self.picture.is_some()
    }

    /// The scene's picture, given to `painter` where it has not it yet.
    pub fn show_picture(&mut self, painter: &mut ddi_render::Painter, device: &wgpu::Device) {
        if let Some(p) = &mut self.picture
            && !p.shown
        {
            painter.set_external_image(device, stage::PICTURE, Some(&p.texture));
            p.shown = true;
        }
    }

    /// The scene's picture let go of, once its play is over.
    pub fn release_picture(&mut self, painter: &mut ddi_render::Painter, device: &wgpu::Device) {
        if !self.framed() && self.picture.take().is_some() {
            painter.set_external_image(device, stage::PICTURE, None);
        }
    }

    /// No play, prompt or results.
    pub fn idle(&self) -> bool {
        matches!(self.stage, Stage::Idle)
    }

    /// A play runs: keys and pads are the play's.
    pub fn playing(&self) -> bool {
        matches!(self.stage, Stage::Playing(..))
    }

    /// The play running, if one is.
    pub fn running(&self) -> Option<&Play> {
        match &self.stage {
            Stage::Playing(_, play, _) => Some(play),
            _ => None,
        }
    }

    /// Where the play's last frame drew a stage's opponents' miniature
    /// fields, if it did (tests).
    pub fn opponent_fields(&self) -> Option<ddi_shell::opponents::Fields> {
        let Stage::Playing(r, ..) = &self.stage else {
            return None;
        };
        r.view.as_ref()?.opponent_fields().cloned()
    }

    /// A large field's ladder as the play's last frame drew it: its words
    /// (tests; `ddi_shell::rivals`).
    pub fn ladder_drawn(&self) -> Option<String> {
        let Stage::Playing(r, ..) = &self.stage else {
            return None;
        };
        r.view.as_ref()?.ladder_drawn().map(|(words, _)| words.clone())
    }

    /// Who attacks whom as the play's last frame drew it, for the
    /// player's own attacks (tests; `ddi_shell::attack_links`).
    pub fn attack_links(&self) -> Vec<ddi_shell::attack_links::Shown> {
        let Stage::Playing(r, ..) = &self.stage else {
            return Vec::new();
        };
        r.view
            .as_ref()
            .map(|v| v.attack_links().to_vec())
            .unwrap_or_default()
    }

    /// A room's stage is being played (not a solo play, a replay or a
    /// test).
    pub fn stage_playing(&self) -> bool {
        matches!(&self.stage, Stage::Playing(r, ..) if r.request.stage.is_some())
    }

    /// The player leaves the stage's play from outside it (the window
    /// asked to close): the play's quit (`PlaySession::leave`: a
    /// forfeit), its results at the next frame. Whether a stage was
    /// being played.
    pub fn leave_stage_play(&mut self) -> bool {
        match &mut self.stage {
            Stage::Playing(r, play, _) if r.request.stage.is_some() => {
                play.session.leave();
                true
            }
            _ => false,
        }
    }

    /// A watched stage as the play's last frame drew it (tests;
    /// `ddi_shell::play_view::WatchDrawn`): the grid or the competitor
    /// followed, or a player's own field after they are out of the
    /// stage; none while the player plays.
    pub fn watch(&self) -> Option<ddi_shell::play_view::WatchDrawn> {
        let Stage::Playing(r, ..) = &self.stage else {
            return None;
        };
        r.view.as_ref()?.watch_drawn().cloned()
    }

    /// Gives the running play lyrics and says what its last frame drew
    /// of them (tests: a stage's miniatures with lyrics over the play).
    pub fn set_lyrics(&mut self, lyrics: Option<ddi_engine::lyrics::LyricTrack>) {
        if let Stage::Playing(r, ..) = &mut self.stage
            && let Some(v) = r.view.as_mut()
        {
            v.set_lyrics(lyrics);
        }
    }

    pub fn lyrics_shown(&self) -> Vec<String> {
        let Stage::Playing(r, ..) = &self.stage else {
            return Vec::new();
        };
        r.view
            .as_ref()
            .map(|v| v.lyrics_shown().to_vec())
            .unwrap_or_default()
    }

    /// Bounded hand/runtime diagnostics for off-screen checks and the host.
    pub fn hand_debug(&self) -> Option<serde_json::Value> {
        let Stage::Playing(r, ..) = &self.stage else {
            return None;
        };
        r.view.as_ref().map(|v| v.dance.debug())
    }

    /// The prompt is up.
    pub fn prompting(&self) -> bool {
        matches!(self.stage, Stage::Prompt(_))
    }

    /// The GPU device was lost: what the song drew with goes (made
    /// again on the new device when the prompt readies it), and a play
    /// stops as interrupted (results stay: they are the UI's).
    pub fn device_lost(&mut self) {
        self.picture = None;
        match &mut self.stage {
            Stage::Idle => {}
            // The play waits for the song's drawing again (Start stays).
            Stage::Prompt(r) => {
                r.drop_view();
                r.ready = false;
            }
            Stage::Results(r) => r.drop_view(),
            Stage::Playing(r, ..) => {
                r.drop_view();
                self.feed.borrow_mut().interrupted = true;
            }
        }
    }

    /// The prompt waits for something that wakes no one (the play's
    /// sound to start running, the scripts' Lua to run its next slices,
    /// the pictures' wait): the loop looks again shortly.
    pub fn wants_poll(&self) -> bool {
        match &self.stage {
            Stage::Prompt(r) => {
                (!r.ready && r.error.is_none() && r.request.refused.is_none())
                    || matches!(&r.scripts, SongScripts::Loading(l) if l.running())
            }
            // A scene's results, until they leave by themselves.
            Stage::Results(r) => r.scene.as_ref().is_some_and(|s| s.leave_at.is_some()),
            _ => false,
        }
    }

    /// The song's pictures on the device now (tests).
    pub fn backgrounds(&self) -> usize {
        match &self.stage {
            Stage::Prompt(r) | Stage::Results(r) | Stage::Playing(r, ..) => {
                r.view.as_ref().map_or(0, View::backgrounds)
            }
            Stage::Idle => 0,
        }
    }

    /// What the prompt is at (tests).
    pub fn prompt_step(&self) -> Option<PromptStep> {
        match &self.stage {
            Stage::Prompt(r) => Some(r.prompt.borrow().step.clone()),
            _ => None,
        }
    }

    /// The prompt offers Start and everything the play waits for is
    /// ready: Start plays at once (tests).
    pub fn prompt_ready(&self) -> bool {
        matches!(&self.stage, Stage::Prompt(r)
            if r.ready && r.prompt.borrow().step == PromptStep::Ready)
    }

    /// What the prompt still loads (tests).
    pub fn prompt_waiting(&self) -> Vec<String> {
        match &self.stage {
            Stage::Prompt(r) => r.prompt.borrow().waiting.clone(),
            _ => Vec::new(),
        }
    }

    /// The prompt changed since this was last asked: a frame shows it.
    pub fn prompt_changed(&mut self) -> bool {
        let Stage::Prompt(r) = &self.stage else {
            return false;
        };
        let v = r.prompt.borrow().version;
        std::mem::replace(&mut self.prompt_seen, v) != v
    }

    /// Puts `r`'s prompt up; its buttons choose for the next poll. Start
    /// is asked for again (after an interruption, a test's outcome).
    fn push_prompt(&self, ui: &mut Ui, env: &Env, r: &mut Readied) {
        r.play_seed = super::random_seed();

        if let Some(v) = r.view.as_mut() {
            v.dance.reset_preload();
        }
        *self.choice.borrow_mut() = None;
        r.started = false;
        r.prompt.borrow_mut().done = false;
        let (choice, state) = (self.choice.clone(), r.prompt.clone());
        let options = r.options.clone();
        let act: PromptFn = Rc::new(move |c, cx| {
            let step = state.borrow().step.clone();
            match &c {
                // Start: the play begins at the next poll, or the loading
                // screen shows until everything is ready (the screen stays
                // until then, `PromptState::finish`).
                PromptChoice::Start => {
                    if step != PromptStep::Ready {
                        return;
                    }
                    state.borrow_mut().set_step(PromptStep::Loading);
                }
                // After an interruption: the prompt readies the play
                // again (Start then plays).
                PromptChoice::Retry => {}
                PromptChoice::Back => cx.close(true),
                // The quiet line's link: calibration's menu (the poll).
                PromptChoice::Calibrate => cx.close(true),
                // A card's action (none on desktop: the theme's results).
                PromptChoice::Action(_) => cx.close(true),
                PromptChoice::Allow => return,
                // A room's Ready: nothing to ready on desktop (its audio
                // starts without a gesture); the room starts the stage.
                PromptChoice::Ready => return,
                // This play's options, at once; the poll readies what
                // they change (`Host::ready`).
                PromptChoice::Option { id, index } => {
                    if let Some(o) = &options {
                        o.borrow_mut().change(id, *index);
                        o.borrow().show(&mut state.borrow_mut());
                    }
                    return;
                }
                PromptChoice::SaveOptions => {
                    if let Some(o) = &options {
                        o.borrow_mut().save();
                        o.borrow().show(&mut state.borrow_mut());
                    }
                    return;
                }
            }
            *choice.borrow_mut() = Some(c);
        });
        let mut prompt = PromptScreen::new(env.clone(), r.prompt.clone(), act).over_select();
        // A room's stage (M7b): its prompt is the readiness check (a
        // stage watched with the audience has none: it counts down).
        if let Some(order) = &r.request.stage
            && crate::room::audience_stage(order).is_none()
        {
            let room = crate::room::stage_room();
            let shell = crate::room::prompt_shell(&room);
            prompt = prompt.with_room(room, order.stage, shell);
        }
        ui.push(prompt);
    }

    /// The play's news between frames: a play asked for, the song read,
    /// the prompt's or the results' choice. `rate` is the output's.
    /// The start prompt's generated still and the source the painter
    /// draws it under, while the prompt is up
    /// ([`ddi_shell::generative::Stills::set`]).
    pub fn prompt_still(&self) -> (Option<String>, Option<ddi_shell::generative::Still>) {
        match &self.stage {
            Stage::Prompt(r) => (Some(r.still_source.clone()), r.still.clone()),
            _ => (None, None),
        }
    }

    pub fn poll(&mut self, ui: &mut Ui, env: &Env, dev: Option<Device<'_>>, rate: u32) {
        *self.devices.borrow_mut() = self.watcher.last.clone();
        let requested = self.requested.borrow_mut().take();
        if let Some(r) = requested
            && matches!(self.stage, Stage::Idle)
        {
            // Song select's: counting down to Start; a scene's, a moment,
            // or at once, no card, where it is of a line said long after
            // one.
            let no_card = r
                .scene
                .as_ref()
                .is_some_and(|o| !in_play::opens_on_prompt(o.line));
            let countdown = if no_card {
                None
            } else if r.scene.is_some() {
                Some(1.0)
            } else {
                r.countdown
                    .then(|| r.settings().start_countdown)
                    .filter(|s| *s > 0.0)
            };
            let mut info = first_info(&r);
            info.timing = self.timing_hint(&r);
            let prompt: SharedPrompt = Rc::new(RefCell::new(PromptState {
                info,
                base: r.backdrop.clone(),
                countdown,
                no_card,
                // Start at once: what the play needs loads behind it.
                step: match &r.refused {
                    Some(why) => {
                        PromptStep::Error(ddi_l10n::msg::prompt::replay_refused(why.text()).into())
                    }
                    None => PromptStep::Ready,
                },
                ..PromptState::default()
            }));
            let mut readied = Readied {
                // Started once the library knows the song (a folder's
                // songs come with its scan).
                loading: None,
                asked: Instant::now(),
                rate,
                bundled: r.calibration.is_some()
                    || crate::library::bundled().iter().any(|e| e.id == r.song),
                request: r,
                prompt,
                loaded: None,
                images: None,
                view: None,
                error: None,
                audio: None,
                picture: None,
                still: None,
                still_source: ddi_shell::generative::still_source(),
                first_beat: None,
                skin: None,
                stage: None,
                view_made: None,
                scripts: SongScripts::Unread,
                started: false,
                ready: false,
                play_seed: super::random_seed(),
                options: None,
                applied: PlayOverrides::default(),
                skins: None,
                voice: None,
                lingering: None,
                scene: None,
            };
            // A scene's frame, over the prompt, the play and the results.
            if let Some(order) = readied.request.scene.clone() {
                self.scene_back.set(false);
                let flag = self.scene_back.clone();
                ui.push_chrome(StageFrame::new(
                    env.clone(),
                    &order.name,
                    &order.voice_label,
                    "",
                    Rc::new(move |_| flag.set(true)),
                ));
                readied.scene = Some(SceneRun::new(order));
            }
            // A song's prompt offers this play's options (not a test's, a
            // replay's: it is drawn with its own); the skins the song
            // folders hold come from a thread.
            if readied.request.calibration.is_none()
                && readied.request.replay.is_none()
                && readied.request.refused.is_none()
                && readied.request.cpu.is_none()
                && readied.request.stage.is_none()
            {
                let mut o = PlayOptions::new(readied.request.settings(), "", env.platform);
                let bundled = crate::library::noteskins::bundled();
                o.set_noteskins(crate::library::noteskins::choices(&bundled), false);
                o.show(&mut readied.prompt.borrow_mut());
                readied.options = Some(Rc::new(RefCell::new(o)));
                let (tx, rx) = mpsc::channel();
                let folders = crate::library::all_folders();
                let wake = self.wake.clone();
                let _ = std::thread::Builder::new()
                    .name("ddi-prompt-skins".into())
                    .spawn(move || {
                        let skins = crate::library::noteskins::all(&folders);
                        let _ = tx.send(crate::library::noteskins::choices(&skins));
                        if let Some(w) = wake {
                            w();
                        }
                    });
                readied.skins = Some(rx);
            }
            // A kept play's results: at once, judged again from its log.
            if readied.request.view_results && readied.request.refused.is_none() {
                self.stage = self.show_kept_results(readied, ui, env);
            } else {
                self.push_prompt(ui, env, &mut readied);
                self.stage = Stage::Prompt(Box::new(readied));
            }
        }
        let stage = std::mem::replace(&mut self.stage, Stage::Idle);
        self.stage = match stage {
            Stage::Prompt(mut r) => {
                self.ready(&mut r, dev, ui, env.reduced());
                if self.scene_back.take() {
                    press_back(ui);
                }
                tell_frame(&mut r, ui);
                // `--start`: Start pressed for the player once it is offered.
                if self.start_when_ready && r.prompt.borrow().step == PromptStep::Ready {
                    self.start_when_ready = false;
                    // The screen shows the step first (Start focused).
                    ui.update(0.0);
                    for down in [true, false] {
                        ui.handle(ddi_ui::event::UiEvent::Menu(ddi_ui::event::MenuEvent {
                            action: ddi_ui::nav::MenuAction::Start,
                            down,
                            source: 0,
                            gesture: true,
                            time: crate::clock::now().0 * 1000.0,
                        }));
                    }
                }
                let choice = self.choice.borrow_mut().take();
                match choice {
                    Some(PromptChoice::Back) if r.scene.is_some() => self.leave_scene(r, ui),
                    // A stage left from its prompt: a forfeit (the room
                    // goes on), or the match stopped if its song could not
                    // be read.
                    Some(PromptChoice::Back) if r.request.stage.is_some() => {
                        leave_stage(&r);
                        Stage::Idle
                    }
                    Some(PromptChoice::Back) => {
                        // Leaving a test: the menu.
                        if r.request.calibration.is_some() {
                            self.open_menu(ui, env);
                        }
                        Stage::Idle
                    }
                    Some(PromptChoice::Calibrate) => {
                        self.open_menu(ui, env);
                        Stage::Idle
                    }
                    Some(PromptChoice::Start) => {
                        r.started = true;
                        self.start_if_ready(r, ui, env)
                    }
                    Some(PromptChoice::Retry) => {
                        r.started = false;
                        r.prompt.borrow_mut().set_step(PromptStep::Ready);
                        if let Some(v) = r.view.as_mut() {
                            v.start();
                        }
                        Stage::Prompt(r)
                    }
                    Some(PromptChoice::Action(_)) => Stage::Idle,
                    // (The options are applied in the prompt's call.)
                    Some(
                        PromptChoice::Allow
                        | PromptChoice::Ready
                        | PromptChoice::Option { .. }
                        | PromptChoice::SaveOptions,
                    )
                    | None
                        if r.started =>
                    {
                        self.start_if_ready(r, ui, env)
                    }
                    Some(
                        PromptChoice::Allow
                        | PromptChoice::Ready
                        | PromptChoice::Option { .. }
                        | PromptChoice::SaveOptions,
                    )
                    | None => Stage::Prompt(r),
                }
            }
            Stage::Playing(mut r, play, _) if self.scene_back.take() => {
                if let Some(run) = r.scene.as_mut() {
                    run.keep_said(&play);
                }
                self.leave_scene(r, ui)
            }
            Stage::Results(mut r)
                if self.scene_back.take()
                    || r.scene
                        .as_ref()
                        .and_then(|s| s.leave_at)
                        .is_some_and(|at| Instant::now() >= at) =>
            {
                // Their way out, as a player's Back.
                if let Some(s) = r.scene.as_mut() {
                    s.leave_at = None;
                }
                press_back(ui);
                Stage::Results(r)
            }
            Stage::Results(r) => match self.after.take() {
                Some(After::Leave) if r.scene.is_some() => self.leave_scene(r, ui),
                Some(After::Watch(id)) => {
                    if let Some(plays) = &self.plays {
                        *self.requested.borrow_mut() =
                            Some(super::replays::request(plays, &id, false));
                    }
                    Stage::Idle
                }
                Some(After::Retry) => {
                    let mut r = r;
                    // The results' line stops with them.
                    r.lingering = None;
                    // A skin whose Lua stopped is readied again (its Lua
                    // tried again, or off after a second stop).
                    if let (Some(v), Some(loaded)) = (r.view.as_ref(), r.loaded.clone())
                        && v.noteskin().is_some_and(|n| n.lua_failure().is_some())
                    {
                        let settings = r.settings();
                        r.skin = read_skin(&loaded, &r.request, &settings, self.wake.clone());
                    }
                    // The sound opens again behind the prompt.
                    r.started = false;
                    r.ready = false;
                    {
                        let mut p = r.prompt.borrow_mut();
                        p.done = false;
                        p.set_step(PromptStep::Ready);
                        // Counting down to Start again, as from song select.
                        let seconds = r.settings().start_countdown;
                        p.set_countdown((seconds > 0.0).then_some(seconds));
                    }
                    self.push_prompt(ui, env, &mut r);
                    if let Some(v) = r.view.as_mut() {
                        v.start();
                    }
                    Stage::Prompt(r)
                }
                Some(After::Leave) => {
                    // A test's "done" goes back to calibration's menu,
                    // but the guided flow's last ends it.
                    if r.request.calibration.is_some_and(|m| m.next().is_some()) {
                        self.open_menu(ui, env);
                    }
                    Stage::Idle
                }
                None => Stage::Results(r),
            },
            s => s,
        };
        // The next test asked for: its prompt at once.
        if matches!(self.stage, Stage::Idle) && self.requested.borrow().is_some() {
            self.poll(ui, env, None, rate);
        }
    }

    /// The prompt's quiet suggestion to calibrate (`PromptInfo::timing`,
    /// `docs/plans/quiet-calibration.md`; the web's `timing_hint`): none
    /// on a test, a replay or an autoplay; else what the kept plays on
    /// the devices say and whether both are calibrated, in a line.
    fn timing_hint(&self, request: &Request) -> Option<String> {
        if request.calibration.is_some()
            || request.replay.is_some()
            || request.auto
            || request.scene.is_some()
            || request.stage.is_some()
        {
            return None;
        }
        // The devices watched.
        let ids = self
            .watcher
            .last
            .as_ref()
            .map(|d| (d.audio.id.clone(), d.display.id.clone()));
        let history = self.plays.as_ref().map(|p| p.history());
        Some(ddi_ui::screens::prompt::timing_hint_for(
            &Settings::load(),
            ids,
            history.as_deref(),
        ))
    }

    fn open_menu(&self, ui: &mut Ui, env: &Env) {
        if let Some(menu) = &self.menu {
            menu(env, ui);
        }
    }

    /// The play's sound, opened.
    fn open_audio(&self) -> Audio {
        // The music plays at the settings' volume (the session's), under
        // a master at 1 (as the web's), or at `--volume`.
        let master = self.master.unwrap_or(1.0);
        match &self.test_clock {
            Some(c) => Audio::headless(master, Some(c.clone())),
            None => Audio::new(master),
        }
    }

    /// The song read, its view made, its skin readied, its pictures
    /// taken in as they come, its sound opened; the prompt says where it
    /// is, and offers Start once all is ready.
    fn ready(&mut self, r: &mut Readied, dev: Option<Device<'_>>, ui: &Ui, reduced: bool) {
        r.ready = false;
        // A replay refused: the prompt says why, nothing loads.
        if r.request.refused.is_some() {
            return;
        }
        if r.loading.is_none() && r.loaded.is_none() && r.error.is_none() {
            let known = r.bundled || crate::library::song(&r.request.song).is_some();
            if let Some(mode) = r.request.calibration {
                r.loaded = Some(Rc::new(super::load::calibration(mode, r.rate)));
            } else if let Some(source) = r
                .request
                .cpu
                .as_ref()
                .and_then(|cpu| cpu.source_file.as_ref())
            {
                r.loading = Some(Loading::start_cpu(
                    source.clone(),
                    r.rate,
                    self.wake.clone(),
                ));
            } else if known || r.asked.elapsed().as_secs() >= 30 {
                // A song never listed fails its load after a while.
                let s = r.settings();
                let movies = cfg!(feature = "video")
                    && s.bg_brightness > 0.0
                    && s.video != ddi_ui::state::settings::VideoMode::Off;
                r.loading = Some(Loading::start(
                    &r.request.song,
                    Some(r.request.chart),
                    r.rate,
                    movies,
                    movies && s.random_movies,
                    r.request.edited.clone(),
                    self.wake.clone(),
                ));
            }
        }
        if let Some(l) = &r.loading
            && let Some(result) = l.poll()
        {
            r.loading = None;
            match result {
                Ok(loaded) => {
                    // The library learns the file's charts, not the
                    // editor's.
                    if let Some(told) = self
                        .chart_loaded
                        .as_ref()
                        .filter(|_| r.request.edited.is_none())
                    {
                        told(&r.request.song, &loaded.song);
                    }
                    // The announcer's sounds, decoded behind the prompt.
                    if let Some(key) = r.settings().announcer.filter(|_| r.scene.is_none()) {
                        let notes = loaded
                            .song
                            .charts
                            .get(r.request.chart)
                            .map_or(0, |c| c.notes.len() as u32);
                        // A practice stage's: the battle's lines, not a
                        // play's routine.
                        let of = if r.request.stage.is_some() {
                            super::voice::For::Stage
                        } else {
                            super::voice::For::Play { notes }
                        };
                        r.voice = Some(super::voice::VoiceLoad::start(
                            key,
                            crate::library::all_folders(),
                            of,
                            r.rate,
                            self.wake.clone(),
                        ));
                    }
                    r.loaded = Some(Rc::new(loaded))
                }
                Err(e) => {
                    r.prompt
                        .borrow_mut()
                        .set_step(PromptStep::Error(e.sentence().into()));
                    r.error = Some(e);
                }
            }
        }
        if r.error.is_some() {
            return;
        }
        if let Some(v) = r.voice.as_mut() {
            v.poll();
        }
        // The song folders' skins, for the options' skin row.
        if let (Some(rx), Some(o)) = (&r.skins, &r.options)
            && let Ok(skins) = rx.try_recv()
        {
            o.borrow_mut().set_noteskins(skins, true);
            o.borrow().show(&mut r.prompt.borrow_mut());
            r.skins = None;
        }
        // The sound opens while the rest is readied.
        if r.audio.is_none() {
            r.audio = Some((self.open_audio(), Instant::now()));
        }
        // The song's scripts: their files read on threads, their Lua run
        // here a turn a poll (the prompt keeps drawing).
        if let Some(loaded) = &r.loaded {
            if matches!(r.scripts, SongScripts::Unread) {
                let settings = r.settings();
                r.scripts =
                    match super::scripts::ScriptLoad::start(loaded, &settings, self.wake.clone()) {
                        Some(l) => SongScripts::Loading(Box::new(l)),
                        None => SongScripts::Held(None),
                    };
            }
            if let SongScripts::Loading(l) = &mut r.scripts
                && let Some(ready) = l.poll(super::scripts::LUA_TURN)
            {
                r.scripts = SongScripts::Held(ready.map(Box::new));
            }
        }
        let (Some(loaded), Some(dev)) = (r.loaded.clone(), dev) else {
            if r.loaded.is_none() {
                let mut p = r.prompt.borrow_mut();
                p.set_waiting(vec!["song".into()]);
                // A shared song's files coming down: said quietly.
                p.set_notes(crate::share::download_line().into_iter().collect());
            }
            return;
        };
        // A scene's play staged, once the song is read.
        stage_scene(r, &loaded, &self.watcher);
        let settings = r.settings();
        // A scene's play is drawn into a picture of its own.
        let formats = if r.scene.is_some() {
            Formats::picture()
        } else {
            dev.formats
        };
        let layout = if r.request.calibration.is_none() && r.request.replay.is_none() {
            loaded
                .song
                .charts
                .get(r.request.chart)
                .and_then(|c| loaded.song.layout_of(c))
        } else {
            None
        };
        r.prompt
            .borrow_mut()
            .set_bindings(layout.as_ref(), &settings);
        if r.view.is_none() {
            let mut view = View::new(dev.device, dev.queue, formats);
            view.set_song(&loaded, &settings);
            let calibration = r.request.calibration.is_some();
            view.set_generated(&loaded, &settings, calibration);
            view.set_noteskin(dev.device, dev.queue, None);
            view.start();
            // A stage's opponents' names are text: its fonts read now.
            if r.request.stage.is_some() {
                view.read_text();
            }
            r.view = Some(view);
            // Its pictures are read on threads (again for a view made
            // again, after a lost device); Start waits for them.
            r.images = Some(read_images(
                &loaded,
                &settings,
                calibration,
                self.wake.clone(),
            ));
            r.view_made = Some(Instant::now());
            // The skin is read on a thread (its images decoded there);
            // until it comes the prompt says it loads.
            r.skin = read_skin(&loaded, &r.request, &settings, self.wake.clone());
            // A work's layers' images and texts, on theirs.
            if settings.bg_brightness > 0.0 && r.request.calibration.is_none() {
                let contents =
                    super::stage::contents(&loaded.song, r.request.chart, &loaded.layers);
                if !contents.is_empty() {
                    r.stage = Some(super::stage::read(contents, self.wake.clone()));
                }
            }
            let mut info = full_info(&r.request, &loaded, &settings);
            info.timing = self.timing_hint(&r.request);
            r.prompt.borrow_mut().set_info(info);
            // The options' skin row is the chart's layout's.
            if let Some(o) = &r.options {
                if let Some(layout) = loaded
                    .song
                    .charts
                    .get(r.request.chart)
                    .and_then(|c| loaded.song.layout_of(c))
                {
                    o.borrow_mut().set_layout(&layout.id);
                }
                o.borrow().show(&mut r.prompt.borrow_mut());
                r.applied = o.borrow().overrides();
            }
        }
        // The play options changed on the prompt: the skin read again
        // when they change it (its key, or the direction its Lua is
        // told), the summary as it now is.
        let mut changed = false;
        if let Some(o) = &r.options {
            let now = o.borrow().overrides();
            if now != r.applied {
                changed = true;
                let before = std::mem::replace(&mut r.applied, now.clone());
                let skin = before.noteskin != now.noteskin
                    || before.pump_noteskin != now.pump_noteskin
                    || before.reverse != now.reverse;
                if skin {
                    r.skin = read_skin(&loaded, &r.request, &settings, self.wake.clone());
                }
                let mut info = full_info(&r.request, &loaded, &settings);
                info.timing = self.timing_hint(&r.request);
                r.prompt.borrow_mut().set_info(info);
            }
        }
        let Some(view) = r.view.as_mut() else {
            return;
        };
        if matches!(r.scripts, SongScripts::Held(_))
            && let SongScripts::Held(ready) = std::mem::replace(&mut r.scripts, SongScripts::Given)
        {
            view.set_scripts(ready.map(|r| *r));
        }
        if let Some(rx) = &r.stage {
            for read in rx.try_iter() {
                view.add_stage_content(read);
            }
        }
        if let Some((reading, layout)) = &r.skin
            && let Some(read) = reading.poll()
        {
            let made = read
                .and_then(|read| super::skins::build(read, layout))
                .map(|p| (p.play, p.images, p.strike));
            view.set_noteskin(dev.device, dev.queue, made);
            view.start();
            r.skin = None;
        }
        // The song's pictures as they come; all read (or not to wait
        // for longer) once their thread is done.
        let mut images_done = r.images.is_none();
        if let Some(rx) = &r.images {
            loop {
                match rx.try_recv() {
                    Ok((what, image)) => {
                        if what == BgImage::Song
                            && let Some(bg) = &loaded.entry.background
                            && image.size[1] > 0
                        {
                            let aspect = image.size[0] as f32 / image.size[1] as f32;
                            r.picture = Some((loaded.files.source(bg), aspect));
                        }
                        view.add_background(what, image);
                    }
                    Err(mpsc::TryRecvError::Empty) => break,
                    Err(mpsc::TryRecvError::Disconnected) => {
                        images_done = true;
                        break;
                    }
                }
            }
        }
        let images_done = images_done || r.view_made.is_some_and(|t| t.elapsed() >= PICTURES_WAIT);
        // On the device now: the frame's shape knows the picture.
        view.prepare(dev.device, dev.queue);
        // The prompt's picture, as the play opens on it (a test shows
        // none): the song's, else its generated background's first
        // frame held still (self-lit, as the play draws it).
        let frame = view.prompt_frame();
        r.still = None;
        if loaded.entry.background.is_none() && view.has_generated() {
            let first = *r
                .first_beat
                .get_or_insert_with(|| first_beat(&loaded, &r.request, &settings));
            let v = ui.viewport();
            let aspect = frame.unwrap_or(v.physical[0] / v.physical[1].max(1.0));
            r.still = view.generated_still(
                (ddi_library::backgrounds::PLAY_START, first),
                settings.bg_brightness,
                reduced,
                aspect,
            );
        }
        let picture = ddi_shell::generative::prompt_picture(
            r.picture
                .clone()
                .filter(|_| r.request.calibration.is_none()),
            r.still.as_ref().map(|s| (r.still_source.clone(), s.aspect)),
            settings.bg_brightness,
            frame,
            filter(&loaded, r.request.chart, &settings, ui),
        );
        // The dancing hand readied for the play as it now is: again when
        // the play's options change, or after a reset (a new view, the
        // prompt up again with a new seed).
        if changed || view.dance.needs_preload() {
            let config = r
                .request
                .session_config(&settings, self.watcher.for_play(), r.play_seed);
            view.dance.preload(
                &loaded.song,
                r.request.song_shift(&settings),
                r.request.chart,
                &settings,
                &config,
            );
        }
        let at = Instant::now();
        view.dance.preload_work(
            dev.device,
            dev.queue,
            formats.plain.unwrap_or(formats.view),
            ui.viewport().physical.map(|v| v.max(1.) as u32),
            &|| at.elapsed().as_secs_f64(),
        );
        let sound = r
            .audio
            .as_ref()
            .is_some_and(|(a, at)| a.running() || at.elapsed() >= SOUND_WAIT);
        // What the play waits for (the scripts too: they never load
        // during a play); Start never does.
        let scripts = matches!(r.scripts, SongScripts::Unread | SongScripts::Loading(_));
        let waiting: Vec<String> = [
            (r.skin.is_some(), "noteskin"),
            (!images_done, "pictures"),
            (scripts, "scripts"),
            (!sound, "audio"),
            (view.dance.preload_pending(), "dancing hand"),
        ]
        .into_iter()
        .filter(|(w, _)| *w)
        .map(|(_, n)| n.to_string())
        .collect();
        r.ready = waiting.is_empty();
        let mut p = r.prompt.borrow_mut();
        p.set_picture(picture);
        p.set_waiting(waiting);
    }

    /// Start was pressed: the play begins if everything is ready, else
    /// the loading screen shows until it is (looked at again each poll).
    fn start_if_ready(&mut self, mut r: Box<Readied>, ui: &mut Ui, env: &Env) -> Stage {
        let step = r.prompt.borrow().step.clone();
        if !matches!(step, PromptStep::Ready | PromptStep::Loading) {
            // An error came meanwhile: it shows, nothing starts.
            r.started = false;
            return Stage::Prompt(r);
        }
        if r.ready && r.loaded.is_some() {
            return self.start(r, ui, env);
        }
        if step == PromptStep::Ready {
            r.prompt.borrow_mut().set_step(PromptStep::Loading);
        }
        Stage::Prompt(r)
    }

    /// Starts the play, the prompt taken down.
    fn start(&mut self, mut r: Box<Readied>, ui: &mut Ui, env: &Env) -> Stage {
        let Some(loaded) = r.loaded.clone() else {
            return Stage::Idle;
        };
        // The backdrop's entrance where the prompt left it, then the
        // screen goes before the play's first frame. Its next showing
        // (a retry, an interruption) waits for Start.
        let entrance = r.prompt.borrow().entrance();
        r.prompt.borrow_mut().countdown = None;
        // The play covers what was under the prompt until it ends.
        self.in_play.set(true);
        ui.push(PlayScreen {
            in_play: self.in_play.clone(),
            picture: r
                .scene
                .as_ref()
                .map(|s| PictureScreen::new(stage::PICTURE).with_spots(s.spots.clone())),
        });
        r.prompt.borrow_mut().finish();
        ui.update(0.0);
        let settings = r.settings();
        let audio = match r.audio.take() {
            Some((a, _)) => a,
            None => self.open_audio(),
        };
        self.begin(r, loaded, &settings, audio, entrance, ui, env)
    }

    /// The play started on `audio`, the prompt down: from Start, or
    /// again at once after a restart (the backdrop then as it was).
    #[allow(clippy::too_many_arguments)]
    fn begin(
        &mut self,
        mut r: Box<Readied>,
        loaded: Rc<Loaded>,
        settings: &Settings,
        audio: Audio,
        entrance: ddi_actor::background::Reveal,
        ui: &mut Ui,
        env: &Env,
    ) -> Stage {
        {
            let mut f = self.feed.borrow_mut();
            f.interrupted = false;
            f.device_changed = false;
            f.edges.clear();
        }
        let refresh = r
            .view
            .as_ref()
            .and_then(|v| v.fps.refresh_estimate())
            .or(self.refresh_hint)
            .unwrap_or(60.0);
        let mut request = r.request.clone();
        request.restart = std::mem::take(&mut self.restarting);
        if settings.ghost
            && request.replay.is_none()
            && request.calibration.is_none()
            && request.scene.is_none()
            && request.cpu.is_none()
            && request.stage.is_none()
        {
            request.ghost = self.ghost(&loaded, &request, settings).map(Rc::new);
        }
        // What the history says of the chart before this play, for the
        // results' first clear and new record (a play that is kept).
        if request.replay.is_none()
            && request.calibration.is_none()
            && request.edited.is_none()
            && request.scene.is_none()
            && request.cpu.is_none()
            && request.stage.is_none()
        {
            request.prior = Some(self.plays.as_ref().map_or_else(Default::default, |p| {
                ddi_session::announcer::Prior::before(
                    &p.history(),
                    &loaded.song,
                    request.chart,
                    settings,
                )
            }));
        }
        match Play::start_seeded(
            request,
            loaded,
            settings,
            audio,
            self.feed.clone(),
            self.watcher.for_play(),
            refresh,
            r.play_seed,
        ) {
            Ok(mut play) => {
                play.graphics = self.graphics.clone();
                if let Some(v) = &r.voice {
                    play.session.set_voice(v.voice.clone());
                }
                // A scene's: its voice, and how long it lasts from here.
                if let Some(run) = &mut r.scene {
                    play.session.set_voice(run.order.voice.clone());
                    // Its bar counts through a restart: told once.
                    if !std::mem::replace(&mut run.bar_told, true) {
                        ui.broadcast(
                            ddi_ui::Message::new(stage::UNTIL)
                                .with("Seconds", ddi_ui::Param::Num(run.seconds)),
                        );
                    }
                    ui.mark(in_play::mark(run.order.line));
                }
                // Kept with the play's options as they were.
                play.overrides = r
                    .options
                    .as_ref()
                    .map(|o| o.borrow().overrides())
                    .unwrap_or_default();
                if let Some(v) = r.view.as_mut() {
                    v.refresh_hint = self.refresh_hint;
                    // The background changes on the song as played: its
                    // offset moves them too.
                    v.set_played_song(play.song(), &play.loaded);

                    v.dance.start_preloaded();
                    v.start();
                    v.enter_background(entrance, env.reduced());
                }
                // A work's back and mask layers, on the song as played
                // (its offset moves them too), with the backgrounds.
                if let Some(v) = r.view.as_mut()
                    && r.request.calibration.is_none()
                {
                    // Its lyrics, over the field (the web's rule: one row
                    // of lanes or two).
                    let (track, lyrics) = ddi_session::stage::play_layers(
                        play.song(),
                        r.request.chart,
                        &play.session.layout,
                        settings,
                    );
                    v.set_stage_track(track);
                    v.set_lyrics(lyrics);
                }
                let mut render = ddi_shell::render_options(settings);
                // A test shows its plain field and each hit's error.
                if let Some(mode) = r.request.calibration {
                    render.for_calibration(mode.hide_notes());
                }
                Stage::Playing(r, Box::new(play), Box::new(render))
            }
            Err(e) => {
                self.in_play.set(false);
                r.prompt.borrow_mut().set_step(PromptStep::Error(
                    ddi_ui::screens::failures::LoadFailure::Other(e)
                        .sentence()
                        .into(),
                ));
                self.push_prompt(ui, env, &mut r);
                Stage::Prompt(r)
            }
        }
    }

    /// One frame of the play at `now`: judged, then drawn into `view`.
    /// When it ends, its results come up on `ui` (or song select again,
    /// or the prompt after an interruption).
    #[allow(clippy::too_many_arguments)]
    pub fn draw_play(
        &mut self,
        dev: &Device<'_>,
        encoder: &mut wgpu::CommandEncoder,
        view: &wgpu::TextureView,
        plain: Option<&wgpu::TextureView>,
        size: [u32; 2],
        now: HostTime,
        ui: &mut Ui,
        env: &Env,
    ) {
        let Stage::Playing(r, play, render) = &mut self.stage else {
            return;
        };
        if let Some(v) = r.voice.as_mut() {
            v.poll();
        }
        let ended = play.tick(now, self.plays.as_ref());
        ddi_shell::apply_view(render, play.session.view, play.session.ghost_delta());
        // A scene's play goes into its picture; the words of what it says
        // into the frame as each line starts.
        let (view, plain) = if let Some(run) = r.scene.as_mut() {
            let at = play.session.audio().context_time();
            for m in run.words.next(&play.session.announcer_report().spoken, at) {
                ui.broadcast(m);
            }
            (picture_view(&mut self.picture, dev.device, size), None)
        } else {
            (view, plain)
        };
        if let Some(v) = r.view.as_mut() {
            v.scale = self.scale;
            v.present_at = self.present_at.take();
            let since = self
                .feed
                .borrow()
                .pointer_at
                .map(|at| at.elapsed().as_secs_f64());
            v.quit_shown = ddi_session::play::quit_shown(false, since);
            v.draw(
                dev.device, dev.queue, encoder, view, plain, size, play, **render, now,
            );
            self.feed.borrow_mut().quit_at = v.quit_at;
            self.feed.borrow_mut().policy_at = v.policy_at;
            self.feed.borrow_mut().watch_at.clone_from(&v.watch_at);
            self.feed.borrow_mut().skip_at = v.skip_at;
            // Where the play showed what its scene can ring.
            if let Some(run) = &r.scene {
                run.spots.set(ddi_shell::play_spots(v.renderer.hud_spots()));
            }
            // Each attack's commitment, to the log (M8b, development).
            for line in play.session.take_battle_log() {
                eprintln!("{line}");
                self.log.push(line);
            }
            // The scripts' sounds, at once on the play's sound.
            for sound in v.take_script_sounds() {
                if let Err(e) = play.session.play_now(&sound) {
                    eprintln!("scripted backgrounds: a sound did not play: {e}");
                }
            }
        }
        let Some(ended) = ended else {
            return;
        };
        let stage = std::mem::replace(&mut self.stage, Stage::Idle);
        let Stage::Playing(r, mut play, _) = stage else {
            return;
        };
        // A stage's outcome, to the room before its results (a quit or an
        // interruption: a forfeit).
        if let Some(o) = play.session.take_stage_outcome() {
            self.log.push(format!("stage outcome: {:?}", o.ranking));
            let ending = crate::room::report(o);
            // The results' battle line, now the room has confirmed it.
            play.session.stage_reported(ending);
        }
        // A scene is over.
        if matches!(ended, Ended::SceneOver | Ended::Interrupted) && r.scene.is_some() {
            let mut r = r;
            if let Some(run) = r.scene.as_mut() {
                run.keep_said(&play);
            }
            self.log_said(&play);
            self.stage = self.leave_scene(r, ui);
            return;
        }
        // Restarted: the same play again at once, nothing kept, the
        // backdrop as it was (`docs/PLAN.md` decision 79).
        if matches!(ended, Ended::Restart) {
            self.log.push(format!("restarted {}", play.request.song));
            // A scene's restart: its play again from the same place,
            // opened by its line.
            let mut r = r;
            if let Some(run) = r.scene.as_mut() {
                run.keep_said(&play);
                run.words.restarted();
                if let Some(Some(p)) = &run.staged {
                    r.request.staging = p.restarted.clone().or(r.request.staging.take());
                }
            }
            let loaded = play.loaded.clone();
            let audio = play.into_restart();
            let settings = r.settings();
            self.restarting = true;
            if settings.dancing_hand_for(&loaded.song.charts[r.request.chart].layout) {
                self.in_play.set(false);
                ui.update(0.);
                r.audio = Some((audio, Instant::now()));
                r.ready = false;
                self.push_prompt(ui, env, &mut r);
                r.started = true;
                r.prompt.borrow_mut().countdown = None;
                r.prompt.borrow_mut().set_step(PromptStep::Loading);
                self.stage = Stage::Prompt(r);
            } else {
                self.stage = self.begin(r, loaded, &settings, audio, Default::default(), ui, env);
            }
            return;
        }
        self.in_play.set(false);
        self.log_said(&play);
        let mut r = r;
        // A scene's results show a moment, in its frame.
        let results_for = self.scene_results;
        if let Some(run) = r.scene.as_mut() {
            run.keep_said(&play);
            run.leave_at = Some(Instant::now() + results_for);
        }
        #[cfg(feature = "video")]
        if let Some(v) = r.view.as_ref()
            && (v.movies.shown_any() || v.movies.trouble().is_some())
        {
            self.log.push(format!(
                "movies: {:?}{}",
                v.movies.stats(),
                v.movies
                    .trouble()
                    .map_or(String::new(), |t| format!(" · {t}"))
            ));
        }
        self.stage = match ended {
            Ended::Finished {
                results,
                device_changed,
                calibration,
            } => {
                self.log.push(format!(
                    "played {} chart {}: {:?} · {} · failed {}",
                    play.request.song,
                    play.request.chart,
                    results.tally.taps,
                    ddi_ui::screens::play::score_text(&results),
                    results.failed,
                ));
                match (play.request.calibration, calibration) {
                    (Some(mode), Some(o)) => {
                        self.log.push(format!(
                            "calibration {}: {:+.1} ms in {} rounds, {} hits, {} missed{}",
                            mode.id(),
                            o.total() * 1000.0,
                            o.rounds,
                            o.hits,
                            o.misses,
                            if o.converged { ", converged" } else { "" }
                        ));
                        // Settled: a meaningful correction applied, a
                        // line for the menu; then the guided flow's next
                        // test, or the menu (`quiet-calibration.md`).
                        let mut s = Settings::load();
                        let settled = ddi_session::calibration::settle(
                            &o,
                            mode,
                            &mut s,
                            play.session.devices.as_ref(),
                            device_changed,
                        );
                        if settled.applied {
                            s.save();
                        }
                        self.log
                            .push(format!("calibration settled: {}", settled.line));
                        ddi_ui::screens::calibration::note_last_run(settled.line);
                        match mode.next() {
                            Some(m) => {
                                let mut next = Request::calibration(m, r.request.auto);
                                next.bias = r.request.bias;
                                *self.requested.borrow_mut() = Some(next);
                            }
                            None => self.open_menu(ui, env),
                        }
                        Stage::Idle
                    }
                    // Watched with the audience: back to its page, which
                    // shows the stage's result.
                    _ if play
                        .request
                        .stage
                        .as_deref()
                        .is_some_and(|o| crate::room::audience_stage(o).is_some()) =>
                    {
                        Stage::Idle
                    }
                    _ => {
                        self.show_results(&r, &play, &results, device_changed, ui, env);
                        r.lingering = Some(play.into_sound());
                        Stage::Results(r)
                    }
                }
            }
            Ended::Aborted { results } => {
                self.log.push(format!("quit {}", play.request.song));
                // A replay the viewer quit goes back too: nothing to show
                // (a kept quit's replay, which quits where the play did,
                // ends on its results as the play's).
                let viewer_quit = play
                    .request
                    .replay
                    .as_ref()
                    .is_some_and(|rp| rp.quit_by_viewer(&results));
                let audience = play
                    .request
                    .stage
                    .as_deref()
                    .is_some_and(|o| crate::room::audience_stage(o).is_some());
                if audience {
                    Stage::Idle
                } else if play.request.calibration.is_some() || viewer_quit {
                    self.open_menu(ui, env);
                    Stage::Idle
                } else {
                    // A quit's results, graded QUIT, as a play's.
                    self.show_results(&r, &play, &results, false, ui, env);
                    r.lingering = Some(play.into_sound());
                    Stage::Results(r)
                }
            }
            // (Started again, or left, above.)
            Ended::Restart | Ended::SceneOver => Stage::Idle,
            // A stage is played once: back to the room (its forfeit
            // reported above).
            Ended::Interrupted if play.request.stage.is_some() => {
                self.log
                    .push(format!("interrupted stage {}", play.request.song));
                Stage::Idle
            }
            Ended::Interrupted => {
                self.log.push(format!("interrupted {}", play.request.song));
                let mut r = r;
                r.prompt.borrow_mut().set_step(PromptStep::Interrupted);
                self.push_prompt(ui, env, &mut r);
                Stage::Prompt(r)
            }
        };
    }

    /// What the play said, for a timed run's lines.
    fn log_said(&mut self, play: &Play) {
        // A stage's battle moments: said by a line, or shown.
        let told = play.session.battle_told();
        if !told.is_empty() {
            let policy = ddi_session::announcer::battle::BATTLE_VOICE_V0;
            let moments: Vec<String> = told
                .iter()
                .map(|t| {
                    let line = ddi_library::announcer::Line::Battle(t.event).folder();
                    match (t.said, t.shown(&policy)) {
                        (_, true) => format!("{line} (shown)"),
                        (Some(l), _) => format!("{line} (said: {})", l.folder()),
                        (None, _) => format!("{line} (results, room)"),
                    }
                })
                .collect();
            self.log.push(format!("battle: {}", moments.join(", ")));
        }
        // The player's attack moments: said by their line, or shown.
        let attacks = play.session.attacks_told();
        if !attacks.is_empty() {
            let moments: Vec<String> = attacks
                .iter()
                .map(|t| {
                    let line = ddi_library::announcer::Line::Battle(t.moment.event()).folder();
                    let how = if t.spoken() { "said" } else { "shown" };
                    format!("{line} ({how})")
                })
                .collect();
            self.log.push(format!("attacks: {}", moments.join(", ")));
        }
        let a = play.session.announcer_report();
        if a == &Default::default() {
            return;
        }
        let spoken: Vec<&str> = a.spoken.iter().map(|(f, _)| f.as_str()).collect();
        self.log.push(format!(
            "announcer: spoke {} · dropped {} · not decoded {:?} · not in the pack {:?}",
            spoken.join(", "),
            a.dropped,
            a.missing,
            a.absent
        ));
    }

    /// The scene is over (its play, its prompt or its results): the frame
    /// goes, and the studio is told what it said.
    fn leave_scene(&mut self, r: Box<Readied>, ui: &mut Ui) -> Stage {
        let Some(run) = r.scene else {
            return Stage::Idle;
        };
        self.in_play.set(false);
        ui.mark(None);
        ui.broadcast(ddi_ui::Message::new(stage::END));
        (run.order.heard)(Heard {
            line: run.order.line,
            said: run.order.said,
            spoken: run.spoken,
        });
        Stage::Idle
    }

    /// The theme's results for the play just ended.
    fn show_results(
        &mut self,
        r: &Readied,
        play: &Play,
        results: &ddi_engine::player::Results,
        device_changed: bool,
        ui: &mut Ui,
        env: &Env,
    ) {
        let settings = Settings::load();
        let session = &play.session;
        let loaded = &play.loaded;
        let e = &loaded.entry;
        let id = play.request.song.clone();
        let song = SongEntry {
            title: e.title.clone(),
            artist: e.artist.clone(),
            length: session.length as f32,
            banner: e.banner.as_deref().map(|b| loaded.files.source(b)),
            background: e.background.as_deref().map(|b| loaded.files.source(b)),
            ..SongEntry::default()
        };
        let mut notes = Vec::new();
        let xruns = play.xruns();
        if xruns > 0 {
            notes.push(ddi_l10n::msg::results::note_sound_dropped(xruns).into());
        }
        if let Some(why) = r.view.as_ref().and_then(|v| v.scripts_stopped()) {
            notes.push(ddi_session::scripted::stop_note(why));
        }
        #[cfg(feature = "video")]
        let (video_off, video_trouble) = r
            .view
            .as_ref()
            .map_or((None, None), |v| (v.movies.off(), v.movies.trouble()));
        #[cfg(not(feature = "video"))]
        let (video_off, video_trouble) = (None, None::<ddi_platform::video::VideoTrouble>);
        // A replay: its results against the log judged at once and the
        // play's kept ones (`Replay::check`); not one the viewer quit,
        // which has nothing to compare.
        let replay = play.request.replay.clone();
        let check = replay
            .as_ref()
            .filter(|rp| !rp.quit_by_viewer(results))
            .map(|rp| {
                let check = rp.check_on(&play.loaded.song, results);
                self.log.push(format!(
                    "replay {}: {}",
                    rp.id,
                    if check.matches {
                        "the play's results".to_string()
                    } else {
                        format!("differs: {}", check.summary())
                    }
                ));
                for note in check.notes() {
                    eprintln!("{note}");
                }
                check
            });
        let save_error = match play.saved.borrow().as_ref() {
            Some(Err(e)) => Some(e.clone()),
            _ => None,
        };
        notes.extend(ddi_ui::screens::play::results_notes(
            video_off,
            video_trouble.as_ref(),
            save_error.as_deref(),
            check.as_ref(),
            replay.as_deref(),
        ));
        if let Some(cpu) = &play.request.cpu {
            notes.push(ddi_ui::screens::prompt::cpu_description_with_skills(
                cpu.level, cpu.seed, cpu.skills,
            ));
        }
        // A stage's: how it went for the player, as the room confirmed it.
        if let Some(order) = &play.request.stage
            && let Some(mark) = crate::room::stage_room().borrow().mark_of(order.stage)
        {
            notes.insert(0, ddi_ui::screens::room::mark_text(&mark));
        }
        let mut data = results_data(&PlayInfo {
            results,
            ruleset: &session.rules,
            layout: &session.layout,
            song,
            chart: r.prompt.borrow().info.chart.clone(),
            mods: Vec::new(),
            device_changed,
            video_notes: notes,
            offset: (play.request.cpu.is_none() && play.request.stage.is_none())
                .then(|| (play.played_offset, settings.song_offset(&id))),
            autoplay: match &replay {
                Some(rp) => rp.record.autoplay.is_some(),
                None => play.request.auto,
            },
        });
        if let Some(rp) = &replay {
            ddi_ui::screens::play::mark_replay(
                &mut data,
                ddi_ui::screens::data::ResultsKind::Replay,
                &rp.when_line(&super::replays::tz_minutes),
            );
        }
        if play.request.stage.is_some() {
            data.kind = ddi_ui::screens::data::ResultsKind::Stage;
            // A final attack's knockout drew it (`SETTLEMENT_V0`).
            if play
                .session
                .battle()
                .is_some_and(|b| b.drawn_by_final_attack())
            {
                data.notes
                    .insert(0, ddi_l10n::msg::room::results_drawn_final_attack().into());
            }
        }
        // The play just kept: its replay.
        let watch: Option<ActionFn> = play
            .play_id
            .clone()
            // Kept: its replay can be read.
            .filter(|_| replay.is_none() && matches!(*play.saved.borrow(), Some(Ok(()))))
            .map(|play_id| {
                results_action(&self.after, move || After::Watch(play_id.clone()), true)
            });
        // A scene's: both go back to the studio.
        let retry = if r.scene.is_some() {
            results_action(&self.after, || After::Leave, true)
        } else {
            results_action(&self.after, || After::Retry, false)
        };
        let song_list = results_action(&self.after, || After::Leave, true);
        let stage = play.request.stage.is_some();
        let offset = (play.request.cpu.is_none() && !stage)
            .then(|| ddi_ui::screens::play::song_offset_saver(id));
        let results = ResultsScreen::new(
            env.clone(),
            data,
            ResultsActions {
                // A stage is played once.
                retry: (!stage).then_some(retry),
                song_list: Some(song_list),
                offset,
                replay: watch,
            },
        );
        // A stage's: its room's standings where the graphs are.
        ui.push(match &play.request.stage {
            // A stage the player watched: no play of their own.
            Some(order) if order.watching => results
                .with_room(crate::room::stage_room(), order.stage)
                .watched(),
            Some(order) => results.with_room(crate::room::stage_room(), order.stage),
            None => results,
        });
    }

    /// The chart's best play's ghost under the rules it is played by
    /// (`Settings::ghost`), when there is one.
    fn ghost(
        &self,
        loaded: &Loaded,
        request: &Request,
        settings: &Settings,
    ) -> Option<ddi_ui::state::history::Ghost> {
        let plays = self.plays.as_ref()?;
        let best = plays
            .history()
            .best_of_play(&loaded.song, request.chart, settings)?;
        let got: Rc<RefCell<Option<_>>> = Rc::default();
        let g = got.clone();
        plays.entry(&best.score_play, move |e| *g.borrow_mut() = Some(e));
        let entry = got.borrow_mut().take()?.ok()?;
        let replay = ddi_ui::state::history::Replay::from_entry(&entry).ok()?;
        ddi_ui::state::history::Ghost::of(&replay, &loaded.song).ok()
    }

    /// A kept play's results (`--replay ID --results`, song select's
    /// plays list): judged again from its log on its chart, read now; its
    /// replay offered.
    fn show_kept_results(&mut self, mut r: Readied, ui: &mut Ui, env: &Env) -> Stage {
        let Some(rp) = r.request.replay.clone() else {
            return Stage::Idle;
        };
        let read = super::load::read_chart(&r.request.song)
            .map_err(ddi_ui::state::history::Refusal::Song)
            .and_then(|song| {
                let results = rp.results(&song)?;
                Ok((song, results))
            });
        let (song, results) = match read {
            Ok(x) => x,
            Err(e) => {
                r.prompt.borrow_mut().set_step(PromptStep::Error(
                    ddi_l10n::msg::prompt::results_refused(e.text()).into(),
                ));
                self.push_prompt(ui, env, &mut r);
                return Stage::Prompt(Box::new(r));
            }
        };
        let chart = r.request.chart;
        let (Some(c), Some(layout)) = (
            song.charts.get(chart),
            song.charts.get(chart).and_then(|c| song.layout_of(c)),
        ) else {
            return Stage::Idle;
        };
        let rules = rp.ruleset().build(c);
        let entry = crate::library::song(&r.request.song)
            .map(|s| s.entry)
            .or_else(|| {
                crate::library::bundled()
                    .into_iter()
                    .find(|e| e.id == r.request.song)
            });
        let files = super::load::Files::of(&r.request.song).map(|(f, _)| f);
        let song_entry = SongEntry {
            title: entry.as_ref().map_or(String::new(), |e| e.title.clone()),
            artist: entry.as_ref().map_or(String::new(), |e| e.artist.clone()),
            length: entry.as_ref().map_or(0.0, |e| e.length as f32),
            banner: entry
                .as_ref()
                .and_then(|e| e.banner.as_deref())
                .zip(files.as_ref())
                .map(|(b, f)| f.source(b)),
            background: entry
                .as_ref()
                .and_then(|e| e.background.as_deref())
                .zip(files.as_ref())
                .map(|(b, f)| f.source(b)),
            ..SongEntry::default()
        };
        let mut data = results_data(&PlayInfo {
            results: &results,
            ruleset: &rules,
            layout: &layout,
            song: song_entry,
            chart: chart_line(
                c.difficulty,
                c.meter,
                &ddi_ui::screens::data::style_name(&layout.id, &layout.name),
            ),
            mods: Vec::new(),
            device_changed: false,
            video_notes: rp
                .rejudged_note()
                .into_iter()
                .chain(rp.older_judge_note())
                .collect(),
            offset: None,
            autoplay: rp.record.autoplay.is_some(),
        });
        ddi_ui::screens::play::mark_replay(
            &mut data,
            ddi_ui::screens::data::ResultsKind::Kept,
            &rp.when_line(&super::replays::tz_minutes),
        );
        self.log.push(format!(
            "kept play {}: {:?} · {}",
            rp.id,
            results.tally.taps,
            ddi_ui::screens::play::score_text(&results)
        ));
        let song_list = results_action(&self.after, || After::Leave, true);
        let id = rp.id.clone();
        let watch = results_action(&self.after, move || After::Watch(id.clone()), true);
        ui.push(ResultsScreen::new(
            env.clone(),
            data,
            ResultsActions {
                retry: None,
                song_list: Some(song_list),
                offset: None,
                replay: Some(watch),
            },
        ));
        Stage::Results(Box::new(r))
    }
}

/// A scene's play staged, once its song is read, under the rules and
/// options it is played with; how long the scene lasts, from it.
fn stage_scene(r: &mut Readied, loaded: &Loaded, watcher: &crate::devices::Watcher) {
    if r.scene.as_ref().is_none_or(|s| s.staged.is_some()) {
        return;
    }
    let settings = r.settings();
    let chart = r.request.chart;
    let shift = r.request.song_shift(&settings);
    let song = loaded.song.shifted(shift);
    let spec = r
        .request
        .session_config(&settings, watcher.for_play(), r.play_seed)
        .player_spec(&song, chart, &settings);
    let Some(run) = r.scene.as_mut() else {
        return;
    };
    let (line, lengths) = (run.order.line, &run.order.lengths);
    let (staged, seconds) = match &spec {
        Ok((rules, options)) => in_play::staged(line, &song, chart, rules, *options, lengths),
        Err(_) => (None, 0.0),
    };
    r.request.staging = staged.as_ref().map(|p| p.staging.clone());
    run.staged = Some(staged);
    run.seconds = seconds;
    run.tell = true;
}

/// A scene's frame told what happens, once its play is staged.
fn tell_frame(r: &mut Readied, ui: &mut Ui) {
    let Some(run) = r.scene.as_mut().filter(|s| s.tell) else {
        return;
    };
    run.tell = false;
    if let Some(Some(p)) = &run.staged {
        let caption = p.caption.text().to_string();
        run.words.set_caption(&caption);
        ui.broadcast(
            ddi_ui::Message::new(stage::CAPTION).with("Text", ddi_ui::Param::Str(caption)),
        );
    }
}

/// Back, as a player presses it.
fn press_back(ui: &mut Ui) {
    for down in [true, false] {
        ui.handle(ddi_ui::event::UiEvent::Menu(ddi_ui::event::MenuEvent {
            action: ddi_ui::nav::MenuAction::Back,
            down,
            source: u32::MAX,
            gesture: false,
            time: crate::clock::now().0 * 1000.0,
        }));
    }
}

/// The field's filter for the prompt's picture
/// ([`ddi_shell::prompt_filter`] on the viewport's logical size; none for
/// an empty one).
/// The beat at the play's first frame (`ddi_library::backgrounds::PLAY_START`),
/// on the chart's timing as the play shifts it.
fn first_beat(loaded: &Loaded, request: &Request, settings: &Settings) -> f64 {
    let song = loaded.song.shifted(request.song_shift(settings));
    song.charts.get(request.chart).map_or(0.0, |c| {
        c.timing(&song)
            .beat_at(ddi_library::backgrounds::PLAY_START)
    })
}

fn filter(loaded: &Loaded, chart: usize, settings: &Settings, ui: &Ui) -> Option<([f32; 2], f32)> {
    let v = ui.viewport();
    let ratio = v.ratio.max(0.1);
    let (w, h) = (v.physical[0] / ratio, v.physical[1] / ratio);
    if w <= 0.0 || h <= 0.0 {
        return None;
    }
    ddi_shell::prompt_filter(&loaded.song, chart, settings, [w, h])
}

/// The song's chart's noteskin, readied (`None`: the built-in arrows;
/// a chosen skin no longer there drawn as Options shows it,
/// [`ddi_shell::play_skins`]).
pub fn noteskin(
    loaded: &Loaded,
    chart: usize,
    settings: &Settings,
) -> Option<(
    ddi_render::noteskin::NoteskinPlay,
    Vec<Option<Rgba>>,
    String,
)> {
    let shift = settings.song_offset(&loaded.entry.id);
    let (wants, layout) = skin_wants(loaded, chart, settings, shift, None)?;
    let skins = crate::library::noteskins::all(&crate::library::all_folders());
    let (key, env) = ddi_shell::first_there(wants, &crate::library::noteskins::choices(&skins));
    let p = super::skins::prepare(&key, &layout, env, &skins)?;
    Some((p.play, p.images, p.strike))
}

/// The skins the song's chart may be drawn with, best first, and its
/// layout ([`ddi_shell::play_skins`]: what their Lua sees, the timing as
/// the play shifts it by `shift`, the song offset); `None` when the chart
/// or its layout is not there.
fn skin_wants(
    loaded: &Loaded,
    chart: usize,
    settings: &Settings,
    shift: f64,
    stage: Option<bool>,
) -> Option<(ddi_shell::SkinWants, ddi_chart::Layout)> {
    let c = loaded.song.charts.get(chart)?;
    let layout = loaded.song.layout_of(c)?;
    let mut wants = ddi_shell::play_skins(settings, &loaded.song, c, &layout, shift);
    // A stage: played under DDI whatever the settings say, and its skin
    // told whether it may be given a battle's shocks
    // (`StageOrder::may_have_shocks`).
    if let Some(shocks) = stage {
        ddi_shell::stage_skin_env(&mut wants.env, shocks);
    }
    Some((wants, layout))
}

/// The song's chart's skin read on a thread (`None`: the built-in
/// arrows, nothing to read).
fn read_skin(
    loaded: &Loaded,
    request: &Request,
    settings: &Settings,
    wake: Option<Arc<dyn Fn() + Send + Sync>>,
) -> Option<(super::skins::Reading, ddi_chart::Layout)> {
    let shift = request.song_shift(settings);
    let stage = request.stage.as_ref().map(|o| o.may_have_shocks());
    let (wants, layout) = skin_wants(loaded, request.chart, settings, shift, stage)?;
    Some((super::skins::Reading::first(wants, &layout, wake), layout))
}

/// Reads the song's pictures on threads; they come through the receiver.
fn read_images(
    loaded: &Loaded,
    settings: &Settings,
    calibration: bool,
    wake: Option<Arc<dyn Fn() + Send + Sync>>,
) -> Receiver<(BgImage, Rgba)> {
    let (tx, rx) = mpsc::channel();
    let jobs = View::images_to_read(loaded, settings, calibration);
    let _ = std::thread::Builder::new()
        .name("ddi-play-images".into())
        .spawn(move || {
            for (what, source, side) in jobs {
                if let Some((size, data)) = crate::images::load(&source, side) {
                    if tx.send((what, Rgba { size, data })).is_err() {
                        return;
                    }
                    if let Some(w) = &wake {
                        w();
                    }
                }
            }
        });
    rx
}

/// A stage left from its prompt: a forfeit reported to the room, the CPUs
/// played to their end; where its song could not be read, the match
/// stopped instead.
fn leave_stage(r: &Readied) {
    let Some(order) = &r.request.stage else {
        return;
    };
    // Before its start: withdrawn; an online room's forfeit is the
    // player's own report.
    if crate::room::left_stage(order.stage) {
        return;
    }
    match &r.loaded {
        Some(loaded) => {
            // The match's rules, everyone's (D0).
            let ruleset = ddi_session::battle::stage_ruleset(&order.rules);
            match ddi_session::battle::forfeit(
                ddi_session::battle::StageSetup::of(order),
                &loaded.song,
                r.request.chart,
                &ruleset,
            ) {
                // Left from the prompt: no play to speak over.
                Ok(o) => {
                    crate::room::report(o);
                }
                Err(e) => {
                    eprintln!("room: stage {}: {e}", order.number);
                    crate::room::current().borrow_mut().stage_lost();
                }
            }
        }
        None => crate::room::current().borrow_mut().stage_lost(),
    }
}

/// A results screen's button: what follows the screen, then leave it.
fn results_action(
    after: &Rc<Cell<Option<After>>>,
    what: impl Fn() -> After + 'static,
    cancel: bool,
) -> ActionFn {
    let after = after.clone();
    Rc::new(move |env: &Env, cx: &mut ddi_ui::Cx| {
        after.set(Some(what()));
        ddi_ui::screens::leave(env, cx, cancel);
    })
}
