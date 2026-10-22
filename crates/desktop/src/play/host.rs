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
use ddi_session::calibration::{CalMode, Outcome};
use ddi_ui::Ui;
use ddi_ui::screens::Env;
use ddi_ui::screens::data::SongEntry;
use ddi_ui::screens::play::{PlayInfo, results_data};
use ddi_ui::screens::prompt::{
    PromptChoice, PromptFn, PromptInfo, PromptPicture, PromptScreen, PromptState, PromptStep,
    SharedPrompt, chart_line, mods_line, new_device_step,
};
use ddi_ui::screens::results::{ActionFn, ResultsActions, ResultsScreen, stored_panels};
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
    error: Option<String>,
    /// The play's sound, opened while the prompt is up so the play
    /// starts on a running output (its clock, its latency and its
    /// device known), and when it was opened.
    audio: Option<(Audio, Instant)>,
    /// The song's background as a texture source, and its shape, for
    /// the prompt's picture.
    picture: Option<(String, f32)>,
    /// A test's outcome, shown on the prompt.
    card: Option<Card>,
    /// The skin being read on its thread, for its layout.
    skin: Option<(super::skins::Reading, ddi_chart::Layout)>,
    /// When its drawing was made (the pictures are waited for a while).
    view_made: Option<Instant>,
    /// A work's layers' contents, read on a thread.
    stage: Option<Receiver<super::stage::Read>>,
    /// The song's scripted backgrounds, readied while the prompt is up.
    scripts: SongScripts,
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

/// A calibration test's outcome on the prompt's card, until it is left.
struct Card {
    mode: CalMode,
    outcome: Outcome,
    devices: Option<DeviceProfile>,
    refresh_hz: f64,
    device_changed: bool,
    saved: bool,
}

impl Card {
    fn step(&self) -> PromptStep {
        ddi_session::calibration::card(
            &self.outcome,
            self.mode,
            &Settings::load(),
            self.devices.as_ref(),
            self.refresh_hz,
            self.saved,
            self.device_changed,
        )
    }
}

/// Opens calibration's menu (the shell's: its buttons reach the
/// library's play call and the options).
pub type MenuFn = Rc<dyn Fn(&Env, &mut Ui)>;

/// Hears a song's id and chart once read ([`Host::chart_loaded`]).
pub type ChartLoadedFn = Rc<dyn Fn(&str, &ddi_chart::Song)>;

pub struct Host {
    /// Song select's play call puts what to play here.
    pub requested: Rc<RefCell<Option<Request>>>,
    pub feed: Rc<RefCell<Feed>>,
    stage: Stage,
    /// Wakes the event loop (a song read on its thread).
    pub wake: Option<Arc<dyn Fn() + Send + Sync>>,
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
}

impl Default for Host {
    fn default() -> Host {
        Host {
            requested: Rc::default(),
            feed: Rc::default(),
            stage: Stage::Idle,
            wake: None,
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
    let badge = if request.replay.is_some() || request.refused.is_some() {
        "REPLAY".to_string()
    } else if request.auto {
        "AUTOPLAY".to_string()
    } else {
        String::new()
    };
    if let Some(mode) = request.calibration {
        return PromptInfo {
            title: mode.title().into(),
            artist: mode.hint().into(),
            badge,
            ..PromptInfo::default()
        };
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
        artist: entry.map_or(String::new(), |e| e.artist.clone()),
        badge,
        ..PromptInfo::default()
    }
}

/// What the prompt names once the song is read: its chart and the mods
/// it will play with too (the web's `prompt_info`).
fn full_info(request: &Request, loaded: &Loaded, settings: &Settings) -> PromptInfo {
    let mut info = first_info(request);
    if request.calibration.is_some() {
        return info;
    }
    info.title = loaded.entry.title.clone();
    info.artist = loaded.entry.artist.clone();
    if let Some(c) = loaded.song.charts.get(request.chart) {
        let style = loaded
            .song
            .layout_of(c)
            .map(|l| l.name.clone())
            .unwrap_or_default();
        info.chart = chart_line(c.difficulty, c.meter, &style);
        info.difficulty = ddi_ui::screens::data::difficulty_name(c.difficulty).into();
    }
    info.mods = mods_line(settings);
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

    /// No play, prompt or results.
    pub fn idle(&self) -> bool {
        matches!(self.stage, Stage::Idle)
    }

    /// A play runs: keys and pads are the play's.
    pub fn playing(&self) -> bool {
        matches!(self.stage, Stage::Playing(..))
    }

    /// The prompt is up.
    pub fn prompting(&self) -> bool {
        matches!(self.stage, Stage::Prompt(_))
    }

    /// The GPU device was lost: what the song drew with goes (made
    /// again on the new device when the prompt readies it), and a play
    /// stops as interrupted (results stay: they are the UI's).
    pub fn device_lost(&mut self) {
        match &mut self.stage {
            Stage::Idle => {}
            Stage::Prompt(r) => {
                r.drop_view();
                // Start waits for the song's drawing again.
                let mut p = r.prompt.borrow_mut();
                if matches!(p.step, PromptStep::Ready) {
                    p.set_step(PromptStep::Loading("loading…".into()));
                }
            }
            Stage::Results(r) => r.drop_view(),
            Stage::Playing(r, ..) => {
                r.drop_view();
                self.feed.borrow_mut().interrupted = true;
            }
        }
    }

    /// The prompt waits for something that wakes no one (the play's
    /// sound to start running, the scripts' Lua to run its next slices):
    /// the loop looks again shortly.
    pub fn wants_poll(&self) -> bool {
        matches!(&self.stage, Stage::Prompt(r)
            if matches!(&r.prompt.borrow().step, PromptStep::Loading(what) if what == "preparing audio…")
                || matches!(&r.scripts, SongScripts::Loading(l) if l.running()))
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

    /// The prompt changed since this was last asked: a frame shows it.
    pub fn prompt_changed(&mut self) -> bool {
        let Stage::Prompt(r) = &self.stage else {
            return false;
        };
        let v = r.prompt.borrow().version;
        std::mem::replace(&mut self.prompt_seen, v) != v
    }

    /// Puts `r`'s prompt up; its buttons choose for the next poll.
    fn push_prompt(&self, ui: &mut Ui, env: &Env, r: &Readied) {
        *self.choice.borrow_mut() = None;
        let (choice, state, devices) =
            (self.choice.clone(), r.prompt.clone(), self.devices.clone());
        // A test, an autoplay or a replay starts without asking about
        // devices.
        let ask_devices =
            r.request.calibration.is_none() && !r.request.auto && r.request.replay.is_none();
        let act: PromptFn = Rc::new(move |c, cx| {
            let step = state.borrow().step.clone();
            match &c {
                PromptChoice::Start => {
                    if step != PromptStep::Ready {
                        return;
                    }
                    // New devices: their offsets are offered first.
                    if ask_devices && let Some(p) = devices.borrow().as_ref() {
                        let s = Settings::load();
                        let new_audio = s.audio_profile(&p.audio.id).is_none();
                        let new_display = s.display_profile(&p.display.id).is_none();
                        if new_audio || new_display {
                            let next = new_device_step(&s, p, new_audio, new_display);
                            state.borrow_mut().set_step(next);
                            return;
                        }
                    }
                    cx.close(false);
                }
                // After an interruption: the prompt readies the play
                // again (Start then plays, new devices asked about).
                PromptChoice::Retry => {}
                PromptChoice::Back => cx.close(true),
                PromptChoice::Device(id) => cx.close(id == "calibrate"),
                // A test's card: saving and again stay on it.
                PromptChoice::Action(id) => match id.as_str() {
                    "save" | "again" => {}
                    "next" => cx.close(false),
                    _ => cx.close(true),
                },
                PromptChoice::Allow => return,
            }
            *choice.borrow_mut() = Some(c);
        });
        ui.push(PromptScreen::new(env.clone(), r.prompt.clone(), act).over_select());
    }

    /// The play's news between frames: a play asked for, the song read,
    /// the prompt's or the results' choice. `rate` is the output's.
    pub fn poll(&mut self, ui: &mut Ui, env: &Env, dev: Option<Device<'_>>, rate: u32) {
        *self.devices.borrow_mut() = self.watcher.last.clone();
        let requested = self.requested.borrow_mut().take();
        if let Some(r) = requested
            && matches!(self.stage, Stage::Idle)
        {
            let prompt: SharedPrompt = Rc::new(RefCell::new(PromptState {
                info: first_info(&r),
                base: r.backdrop.clone(),
                step: match &r.refused {
                    Some(why) => PromptStep::Error(format!("This replay cannot be watched. {why}")),
                    None => PromptStep::Loading("loading…".into()),
                },
                ..PromptState::default()
            }));
            let readied = Readied {
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
                card: None,
                skin: None,
                stage: None,
                view_made: None,
                scripts: SongScripts::Unread,
            };
            // A kept play's results: at once, judged again from its log.
            if readied.request.view_results && readied.request.refused.is_none() {
                self.stage = self.show_kept_results(readied, ui, env);
            } else {
                self.push_prompt(ui, env, &readied);
                self.stage = Stage::Prompt(Box::new(readied));
            }
        }
        let stage = std::mem::replace(&mut self.stage, Stage::Idle);
        self.stage = match stage {
            Stage::Prompt(mut r) => {
                self.ready(&mut r, dev, ui);
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
                    Some(PromptChoice::Back) => {
                        // Leaving a test: the menu, but not after the
                        // guided flow's last one's outcome.
                        let last = r.card.as_ref().is_some_and(|c| c.mode.next().is_none());
                        if r.request.calibration.is_some() && !last {
                            self.open_menu(ui, env);
                        }
                        Stage::Idle
                    }
                    Some(PromptChoice::Start) => self.start(r, ui, env),
                    Some(PromptChoice::Retry) => {
                        r.prompt
                            .borrow_mut()
                            .set_step(PromptStep::Loading("preparing audio…".into()));
                        if let Some(v) = r.view.as_mut() {
                            v.start();
                        }
                        Stage::Prompt(r)
                    }
                    Some(PromptChoice::Device(id)) => {
                        // Kept with the offsets chosen (calibrating: the
                        // defaults until the tests say otherwise).
                        if let Some(p) = self.watcher.last.clone() {
                            let mut s = Settings::load();
                            let how = if id == "calibrate" { "defaults" } else { &id };
                            ddi_ui::screens::notices::register_devices(&mut s, &p, how);
                            s.save();
                        }
                        if id == "calibrate" {
                            self.open_menu(ui, env);
                            Stage::Idle
                        } else {
                            self.start(r, ui, env)
                        }
                    }
                    Some(PromptChoice::Action(id)) => self.card_action(r, &id, ui, env),
                    Some(PromptChoice::Allow) | None => Stage::Prompt(r),
                }
            }
            Stage::Results(r) => match self.after.take() {
                Some(After::Watch(id)) => {
                    if let Some(plays) = &self.plays {
                        *self.requested.borrow_mut() =
                            Some(super::replays::request(plays, &id, false));
                    }
                    Stage::Idle
                }
                Some(After::Retry) => {
                    let mut r = r;
                    // A skin whose Lua stopped is readied again (its Lua
                    // tried again, or off after a second stop).
                    if let (Some(v), Some(loaded)) = (r.view.as_ref(), r.loaded.clone())
                        && v.noteskin().is_some_and(|n| n.lua_failure().is_some())
                    {
                        let settings = r.request.settings();
                        r.skin = read_skin(&loaded, &r.request, &settings, self.wake.clone());
                    }
                    // The sound opens again; Start once it runs.
                    r.prompt
                        .borrow_mut()
                        .set_step(PromptStep::Loading("preparing audio…".into()));
                    self.push_prompt(ui, env, &r);
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

    /// A test's card's button: save (the card says it was), again (the
    /// prompt readies the test again), the guided flow's next test, done
    /// (calibration's menu again; the guided flow's last ends it).
    fn card_action(&mut self, mut r: Box<Readied>, id: &str, ui: &mut Ui, env: &Env) -> Stage {
        let Some(card) = r.card.as_mut() else {
            return Stage::Prompt(r);
        };
        match id {
            "save" => {
                if !card.saved {
                    let mut s = Settings::load();
                    card.mode
                        .apply(&mut s, card.devices.as_ref(), card.outcome.total());
                    s.save();
                    card.saved = true;
                }
                let step = card.step();
                r.prompt.borrow_mut().set_step(step);
                Stage::Prompt(r)
            }
            "again" => {
                r.card = None;
                r.prompt
                    .borrow_mut()
                    .set_step(PromptStep::Loading("preparing audio…".into()));
                if let Some(v) = r.view.as_mut() {
                    v.start();
                }
                Stage::Prompt(r)
            }
            "next" => {
                if let Some(m) = card.mode.next() {
                    let mut next = Request::calibration(m, r.request.auto);
                    next.bias = r.request.bias;
                    *self.requested.borrow_mut() = Some(next);
                }
                Stage::Idle
            }
            _ => {
                if card.mode.next().is_some() {
                    self.open_menu(ui, env);
                }
                Stage::Idle
            }
        }
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
    fn ready(&mut self, r: &mut Readied, dev: Option<Device<'_>>, ui: &Ui) {
        // A replay refused: the prompt says why, nothing loads.
        if r.request.refused.is_some() {
            return;
        }
        if r.loading.is_none() && r.loaded.is_none() && r.error.is_none() {
            let known = r.bundled || crate::library::song(&r.request.song).is_some();
            if let Some(mode) = r.request.calibration {
                r.loaded = Some(Rc::new(super::load::calibration(mode, r.rate)));
            } else if known || r.asked.elapsed().as_secs() >= 30 {
                // A song never listed fails its load after a while.
                let s = r.request.settings();
                let movies = cfg!(feature = "video")
                    && s.bg_brightness > 0.0
                    && s.video != ddi_ui::state::settings::VideoMode::Off;
                r.loading = Some(Loading::start(
                    &r.request.song,
                    Some(r.request.chart),
                    r.rate,
                    movies,
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
                    r.loaded = Some(Rc::new(loaded))
                }
                Err(e) => {
                    r.prompt
                        .borrow_mut()
                        .set_step(PromptStep::Error(format!("The song failed to load: {e}")));
                    r.error = Some(e);
                }
            }
        }
        if r.error.is_some() {
            return;
        }
        // The sound opens while the rest is readied.
        if r.audio.is_none() {
            r.audio = Some((self.open_audio(), Instant::now()));
        }
        // The song's scripts: their files read on threads, their Lua run
        // here a turn a poll (the prompt keeps drawing).
        if let Some(loaded) = &r.loaded {
            if matches!(r.scripts, SongScripts::Unread) {
                let settings = r.request.settings();
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
            return;
        };
        let settings = r.request.settings();
        if r.view.is_none() {
            let mut view = View::new(dev.device, dev.queue, dev.formats);
            view.set_song(&loaded, &settings);
            view.set_noteskin(dev.device, dev.queue, None);
            view.start();
            r.view = Some(view);
            // Its pictures are read on threads (again for a view made
            // again, after a lost device); Start waits for them.
            r.images = Some(read_images(&loaded, &settings, self.wake.clone()));
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
            r.prompt
                .borrow_mut()
                .set_info(full_info(&r.request, &loaded, &settings));
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
        // none).
        let picture = r
            .picture
            .clone()
            .filter(|_| r.request.calibration.is_none() && settings.bg_brightness > 0.0)
            .map(|(source, aspect)| PromptPicture {
                source,
                aspect,
                frame: view.prompt_frame(),
                brightness: settings.bg_brightness,
                filter: filter(&loaded, r.request.chart, &settings, ui),
            });
        let sound = r
            .audio
            .as_ref()
            .is_some_and(|(a, at)| a.running() || at.elapsed() >= SOUND_WAIT);
        let mut p = r.prompt.borrow_mut();
        p.set_picture(picture);
        if matches!(p.step, PromptStep::Loading(_)) {
            // Start waits for the scripts as it did when they loaded
            // with the view: they never load during a play.
            let scripts = matches!(r.scripts, SongScripts::Unread | SongScripts::Loading(_));
            p.set_step(if r.skin.is_some() || !images_done || scripts {
                PromptStep::Loading("loading…".into())
            } else if sound {
                PromptStep::Ready
            } else {
                PromptStep::Loading("preparing audio…".into())
            });
        }
    }

    /// Starts the play (the prompt took itself down).
    fn start(&mut self, mut r: Box<Readied>, ui: &mut Ui, env: &Env) -> Stage {
        let Some(loaded) = r.loaded.clone() else {
            return Stage::Idle;
        };
        let settings = r.request.settings();
        let audio = match r.audio.take() {
            Some((a, _)) => a,
            None => self.open_audio(),
        };
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
        if settings.ghost && request.replay.is_none() && request.calibration.is_none() {
            request.ghost = self.ghost(&loaded, &request, &settings).map(Rc::new);
        }
        match Play::start(
            request,
            loaded,
            &settings,
            audio,
            self.feed.clone(),
            self.watcher.for_play(),
            refresh,
        ) {
            Ok(mut play) => {
                play.graphics = self.graphics.clone();
                if let Some(v) = r.view.as_mut() {
                    v.refresh_hint = self.refresh_hint;
                    // The background changes on the song as played: its
                    // offset moves them too.
                    v.set_played_song(play.song(), &play.loaded);
                    v.start();
                    v.enter_background(r.prompt.borrow().reveal, env.reduced());
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
                        &settings,
                    );
                    v.set_stage_track(track);
                    v.set_lyrics(lyrics);
                }
                let mut render = ddi_shell::render_options(&settings);
                // A test shows its plain field and each hit's error.
                if let Some(mode) = r.request.calibration {
                    render.for_calibration(mode.hide_notes());
                }
                Stage::Playing(r, Box::new(play), Box::new(render))
            }
            Err(e) => {
                r.prompt.borrow_mut().set_step(PromptStep::Error(e));
                self.push_prompt(ui, env, &r);
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
        let ended = play.tick(now, self.plays.as_ref());
        render.ghost = play.session.ghost_delta().map(ddi_shell::score_delta);
        if let Some(v) = r.view.as_mut() {
            v.scale = self.scale;
            v.present_at = self.present_at.take();
            v.draw(
                dev.device, dev.queue, encoder, view, plain, size, play, **render, now,
            );
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
        let Stage::Playing(r, play, _) = stage else {
            return;
        };
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
                        let card = Card {
                            mode,
                            outcome: o,
                            devices: play.session.devices.clone(),
                            refresh_hz: play.refresh_hz,
                            device_changed,
                            saved: false,
                        };
                        let mut r = r;
                        r.prompt.borrow_mut().set_step(card.step());
                        r.card = Some(card);
                        self.push_prompt(ui, env, &r);
                        Stage::Prompt(r)
                    }
                    _ => {
                        self.show_results(&r, &play, &results, device_changed, ui, env);
                        Stage::Results(r)
                    }
                }
            }
            Ended::Aborted => {
                self.log.push(format!("quit {}", play.request.song));
                if play.request.calibration.is_some() {
                    self.open_menu(ui, env);
                }
                Stage::Idle
            }
            Ended::Interrupted => {
                self.log.push(format!("interrupted {}", play.request.song));
                r.prompt.borrow_mut().set_step(PromptStep::Interrupted);
                self.push_prompt(ui, env, &r);
                Stage::Prompt(r)
            }
        };
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
            id: id.clone(),
            title: e.title.clone(),
            artist: e.artist.clone(),
            bpm: e.bpm.parse().unwrap_or(0.0),
            bpm_text: e.bpm.clone(),
            length: session.length as f32,
            banner: e.banner.as_deref().map(|b| loaded.files.source(b)),
            background: e.background.as_deref().map(|b| loaded.files.source(b)),
            ..SongEntry::default()
        };
        let mut notes = Vec::new();
        let xruns = play.xruns();
        if xruns > 0 {
            notes.push(format!(
                "The sound dropped out {xruns} time{} during this play: its timing may be off.",
                if xruns == 1 { "" } else { "s" }
            ));
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
        let (video_off, video_trouble) = (None, None::<String>);
        // A replay: its results against the log judged at once and the
        // play's kept ones (`Replay::check`).
        let replay = play.request.replay.clone();
        let check = replay.as_ref().map(|rp| {
            let check = rp.check_on(&play.loaded.song, results);
            self.log.push(format!(
                "replay {}: {}",
                rp.id,
                if check.matches {
                    "the play's results".to_string()
                } else {
                    format!("differs: {}", check.problems.join("; "))
                }
            ));
            if let Some(note) = check.note() {
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
            video_trouble.as_deref(),
            save_error.as_deref(),
            check.as_ref(),
            replay.as_deref(),
        ));
        let mut data = results_data(&PlayInfo {
            results,
            ruleset: &session.rules,
            layout: &session.layout,
            song,
            chart: r.prompt.borrow().info.chart.clone(),
            mods: Vec::new(),
            device_changed,
            video_notes: notes,
            offset: Some((play.played_offset, settings.song_offset(&id))),
            panels: settings.results_panels.clone(),
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
        // The play just kept: its replay.
        let watch: Option<ActionFn> = play
            .play_id
            .clone()
            // Kept: its replay can be read.
            .filter(|_| replay.is_none() && matches!(*play.saved.borrow(), Some(Ok(()))))
            .map(|play_id| {
                results_action(&self.after, move || After::Watch(play_id.clone()), true)
            });
        let retry = results_action(&self.after, || After::Retry, false);
        let song_list = results_action(&self.after, || After::Leave, true);
        let offset = ddi_ui::screens::play::song_offset_saver(id);
        ui.push(ResultsScreen::new(
            env.clone(),
            data,
            ResultsActions {
                retry: Some(retry),
                song_list: Some(song_list),
                offset: Some(offset),
                panels: Some(stored_panels()),
                replay: watch,
            },
        ));
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
    fn show_kept_results(&mut self, r: Readied, ui: &mut Ui, env: &Env) -> Stage {
        let Some(rp) = r.request.replay.clone() else {
            return Stage::Idle;
        };
        let read = super::load::read_chart(&r.request.song).and_then(|song| {
            let results = rp.results(&song)?;
            Ok((song, results))
        });
        let (song, results) = match read {
            Ok(x) => x,
            Err(e) => {
                r.prompt.borrow_mut().set_step(PromptStep::Error(format!(
                    "This play's results cannot be shown. {e}"
                )));
                self.push_prompt(ui, env, &r);
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
        let rules = rp.record.ruleset.build(c);
        let settings = Settings::load();
        let entry = crate::library::song(&r.request.song)
            .map(|s| s.entry)
            .or_else(|| {
                crate::library::bundled()
                    .into_iter()
                    .find(|e| e.id == r.request.song)
            });
        let files = super::load::Files::of(&r.request.song).map(|(f, _)| f);
        let song_entry = SongEntry {
            id: r.request.song.clone(),
            title: entry.as_ref().map_or(String::new(), |e| e.title.clone()),
            artist: entry.as_ref().map_or(String::new(), |e| e.artist.clone()),
            bpm: entry
                .as_ref()
                .and_then(|e| e.bpm.parse().ok())
                .unwrap_or(0.0),
            bpm_text: entry.as_ref().map_or(String::new(), |e| e.bpm.clone()),
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
            chart: chart_line(c.difficulty, c.meter, &layout.name),
            mods: Vec::new(),
            device_changed: false,
            video_notes: rp.older_judge_note().into_iter().collect(),
            offset: None,
            panels: settings.results_panels.clone(),
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
                panels: Some(stored_panels()),
                replay: Some(watch),
            },
        ));
        Stage::Results(Box::new(r))
    }
}

/// The field's filter for the prompt's picture: the band the chart's
/// lanes will take, as fractions of the screen's width, and how dark.
fn filter(loaded: &Loaded, chart: usize, settings: &Settings, ui: &Ui) -> Option<([f32; 2], f32)> {
    if settings.field_filter <= 0.0 {
        return None;
    }
    let layout = loaded
        .song
        .charts
        .get(chart)
        .and_then(|c| loaded.song.layout_of(c))
        .filter(|l| !l.lanes.is_empty())?;
    let v = ui.viewport();
    let ratio = v.ratio.max(0.1);
    let (w, h) = (v.physical[0] / ratio, v.physical[1] / ratio);
    if w <= 0.0 || h <= 0.0 {
        return None;
    }
    let band = ddi_render::scene::filter_fraction(&layout, w, h, settings.reverse);
    Some((band, settings.field_filter))
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
    let (wants, layout) = skin_wants(loaded, chart, settings, shift)?;
    let skins = crate::library::noteskins::all(&crate::library::all_folders());
    let (key, env) = ddi_shell::first_there(wants, &crate::library::noteskins::choices(&skins))?;
    let p = super::skins::prepare(&key, &layout, env, &skins)?;
    Some((p.play, p.images, p.strike))
}

/// The skins the song's chart may be drawn with, best first, and its
/// layout ([`ddi_shell::play_skins`]: what their Lua sees, the timing as
/// the play shifts it by `shift`, the song offset); `None` when it draws
/// the built-in arrows.
fn skin_wants(
    loaded: &Loaded,
    chart: usize,
    settings: &Settings,
    shift: f64,
) -> Option<(ddi_shell::SkinWants, ddi_chart::Layout)> {
    let c = loaded.song.charts.get(chart)?;
    let layout = loaded.song.layout_of(c)?;
    let wants = ddi_shell::play_skins(settings, &loaded.song, c, &layout, shift)?;
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
    let (wants, layout) = skin_wants(loaded, request.chart, settings, shift)?;
    Some((super::skins::Reading::first(wants, &layout, wake), layout))
}

/// Reads the song's pictures on threads; they come through the receiver.
fn read_images(
    loaded: &Loaded,
    settings: &Settings,
    wake: Option<Arc<dyn Fn() + Send + Sync>>,
) -> Receiver<(BgImage, Rgba)> {
    let (tx, rx) = mpsc::channel();
    let jobs = View::images_to_read(loaded, settings);
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
