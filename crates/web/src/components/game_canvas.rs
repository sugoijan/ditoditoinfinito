//! Gameplay screen: loads the song (or generates the calibration one), waits
//! for a user gesture (the audio context must be created inside one), starts
//! the play session and shows the results when it ends.
//!
//! Until the play starts the canvas shows the start prompt, the UI core's
//! screen (`ddi_ui::screens::prompt`, step 6.4) on the page's canvas as the
//! results are: the song over its picture, what still loads, the linked
//! folder to allow, the devices not calibrated yet, an interruption. Its
//! choices come back inside the key, click, tap or pad press that made them
//! ([`PromptShell::choose`]): the audio context is made and resumed there,
//! and a linked folder asked for there, with no `await` before.
//!
//! A controller's Start button starts the song too: Chrome and Firefox treat
//! a gamepad press as a user gesture, so the audio context is created in
//! the poll that saw the press, as a keydown does. Safari does not; when the
//! context does not start, the prompt asks for a tap, click or key press
//! instead, and one made while the screen still waits resumes the waiting
//! context.
//!
//! On touch screens (a coarse pointer, or once a finger has touched the
//! screen) the play screen shows a quit button instead of the back link,
//! since there is no Escape key and a stray tap on a link would leave the
//! song at once; the button runs the same hold or double-tap gesture.

use std::cell::RefCell;
use std::rc::{Rc, Weak};

use crate::component::{Callback, Component, Context, NodeRef};
use ddi_chart::Song;
use ddi_engine::player::Results;
use ddi_engine::sprites::SpriteSource;
use ddi_platform::DeviceProfile;
use ddi_session::backgrounds::{MAX_BACKGROUND_SIDE, MAX_CHANGE_SIDE};
use ddi_session::stage::{MAX_LAYER_SIDE, TEXT_SCALE};
use gloo::events::{EventListener, EventListenerOptions};
use gloo::render::{AnimationFrame, request_animation_frame};
use wasm_bindgen::{JsCast, JsValue};
use wasm_bindgen_futures::spawn_local;
use web_sys::{AudioBuffer, HtmlElement, KeyboardEvent, PointerEvent};

use crate::calibration::{self, CalMode, Outcome};
use crate::game_loop::GameLoop;
use crate::mods::mods_summary;
use crate::play::{PlaySession, SessionConfig, SessionEvent};
use crate::router::Route;
use crate::settings::{Settings, VideoMode};
use crate::songs::{LoadError, load_background, load_layer_image, load_song};
use crate::web::audio::WebAudio;
use crate::web::gamepad::Gamepads;
use crate::web::gfx::{BackendPreference, SurfaceMode};
use crate::web::video::ModuleStatus;
use ddi_library::backgrounds::{self, BgImage};
use ddi_ui::screens::prompt::{
    PromptChoice, PromptFn, PromptInfo, PromptPicture, PromptState, PromptStep, SharedPrompt,
    chart_line, mods_line, new_device_step,
};

/// How long after the results' Retry a start is not taken (ms).
const PROMPT_GUARD_MS: f64 = 500.0;

/// How long a context created from a pad press may take to start before the
/// screen asks for a real gesture, ms.
const PAD_START_WAIT_MS: u32 = 1500;

/// What the screen plays.
#[derive(Clone, Debug, PartialEq)]
pub(crate) enum SongSource {
    /// A bundled or imported song by id and chart index.
    Song { id: String, chart: usize },
    /// The generated calibration chart.
    Calibration(CalMode),
}

/// A song ready to play: parsed chart plus either encoded audio bytes (to be
/// decoded) or nothing (the calibration click track is synthesized).
pub(crate) struct Loaded {
    pub(crate) song: Song,
    pub(crate) title: String,
    pub(crate) subtitle: String,
    pub(crate) music_bytes: Option<Vec<u8>>,
    /// Images the background changes show (manifest `bg_images`).
    pub(crate) bg_images: Vec<String>,
    /// Movies they play (manifest `bg_videos` that play, and shared ones).
    pub(crate) bg_videos: Vec<String>,
    /// The song has a background image.
    pub(crate) has_background: bool,
    /// Its manifest entry (the results' song).
    pub(crate) entry: Option<ddi_library::manifest::ManifestEntry>,
    /// Its banner as a texture source (`crate::web::textures`).
    pub(crate) banner: Option<String>,
}

/// What the start prompt shows behind it: the opening still's or the
/// background's kept file (`crate::web::textures`), under the play's
/// background name and a suffix, and the image's width over its height.
fn prompt_source(kind: &str) -> Option<String> {
    PLAY_BACKGROUND.with(|b| b.borrow().as_ref().map(|n| format!("{n}:{kind}")))
}

thread_local! {
    /// The texture source the current play's background is kept under for
    /// its results (`crate::web::textures::keep`): a name of its own for
    /// every song loaded, since the texture cache keeps what a name showed
    /// (and a name that failed to load).
    static PLAY_BACKGROUND: RefCell<Option<String>> = const { RefCell::new(None) };
    static PLAY_LOADS: std::cell::Cell<u64> = const { std::cell::Cell::new(0) };
}

/// A new name for the background of song `id`, the last one let go.
fn new_play_background(id: &str) -> String {
    release_play_background();
    let n = PLAY_LOADS.with(|c| {
        c.set(c.get() + 1);
        c.get()
    });
    let name = format!("play:{id}:{n}");
    PLAY_BACKGROUND.with(|b| *b.borrow_mut() = Some(name.clone()));
    name
}

/// The current play's background name, while its file is kept.
fn play_background() -> Option<String> {
    PLAY_BACKGROUND
        .with(|b| b.borrow().clone())
        .filter(|n| crate::web::textures::is_kept(n))
}

fn release_play_background() {
    if let Some(old) = PLAY_BACKGROUND.with(|b| b.borrow_mut().take()) {
        // The textures made of them go too (each load has names of its
        // own: kept, they would fill the cache visit after visit).
        crate::web::screen::forget_textures(&old);
        crate::web::textures::keep(&old, None);
        for kind in ["still", "song"] {
            crate::web::textures::keep(&format!("{old}:{kind}"), None);
        }
    }
}

pub(crate) enum Msg {
    GfxReady(Result<Box<GameLoop>, String>),
    /// The history kept the play (its id), or why it could not.
    PlaySaved(Result<String, String>),
    /// The results waited long enough for the play to be kept.
    SaveWaited,
    /// The app's device was lost and made again.
    DeviceLost,
    /// Escape/Backspace outside of play: leave the screen.
    BackRequested,
    SongLoaded(Result<Rc<Loaded>, LoadError>),
    /// The answer to asking for a linked song's folder.
    Access(bool),
    /// The start prompt's Start: the audio context made inside the gesture
    /// (or a pad's press), and whether it came from a pad the browser may
    /// not count as a gesture.
    Begin(Box<Result<WebAudio, String>>, bool),
    /// A controller button went down: `(Gamepad.id, control)`.
    PadPress(String, String),
    /// Started from a pad, but the browser kept the audio context
    /// suspended (no gamepad gestures): ask for a tap or key.
    NeedsGesture,
    /// A finger or pen touched the screen.
    TouchSeen,
    Decoded(Result<(WebAudio, AudioBuffer), String>),
    /// On the new-device prompt: start with the (possibly copied) offsets.
    DeviceContinue {
        copy_audio_from: Option<String>,
        copy_display_from: Option<String>,
    },
    /// On the new-device prompt: register the devices and go calibrate.
    DeviceCalibrate,
    /// A card's action (a calibration's outcome; a song's score).
    CardAction(String),
    Session(SessionEvent),
    Retry,
    SaveCalibration,
    /// A background image decoded: the song's background or a background
    /// change's (`None`: no such image, or it failed to load).
    BackgroundLoaded(BgImage, Option<web_sys::ImageBitmap>),
    /// An image or text of a work's back and mask layers, decoded, with
    /// its size in the stage's px.
    StageContentLoaded(SpriteSource, Option<(web_sys::ImageBitmap, (f32, f32))>),
    /// A background movie's file, for the game loop to play; whether it
    /// is the song's first movie (probed for the background frame).
    MovieLoaded(
        String,
        web_sys::Blob,
        ddi_platform::video::VideoFormat,
        bool,
    ),
    /// Time to look at the video decoder's download again (start prompt).
    VideoTick,
    /// The chosen noteskin, readied for the chart's layout (`None`: the
    /// built-in arrows).
    NoteskinLoaded(Option<Box<crate::noteskins::Prepared>>),
    /// The chart's best play, read for its ghost (`Settings::ghost`).
    GhostLoaded(Result<ddi_platform::history::PlayEntry, String>),
    /// A replay's skin looked for once its song was read (its folder
    /// waited for access): why it cannot be drawn, if it cannot.
    ReplaySkin(Option<String>),
}

#[derive(Clone, PartialEq)]
pub(crate) struct Props {
    pub(crate) source: SongSource,
    pub(crate) backend: BackendPreference,
    pub(crate) auto: bool,
    /// Autoplay through the headless tests' fake gamepad.
    pub(crate) auto_pad: bool,
    /// Autoplay bias in seconds (testing aid).
    pub(crate) auto_bias: f64,
    /// Frames take longer while a movie is drawn (testing aid for the
    /// automatic video setting).
    pub(crate) slow: bool,
    /// Asks the app to mount the screen again (a lost device).
    pub(crate) restart: Callback<()>,
    /// A kept play replayed, or its results shown (`crate::replay`).
    pub(crate) replay: Option<Rc<crate::replay::ReplayProp>>,
}

/// The page's few DOM elements around the canvas.
struct PlayDom {
    overlay: HtmlElement,
    back: HtmlElement,
    _pointer: EventListener,
}

pub(crate) struct GameCanvas {
    /// The page's DOM, made at the first view.
    dom: RefCell<Option<PlayDom>>,
    /// A finger or a pen was seen (the pointer listener's own copy).
    touch_seen: Rc<std::cell::Cell<bool>>,
    /// Where the app's canvas is put (`web::screen`).
    slot: NodeRef,
    /// The app's canvas, while this screen holds it.
    attachment: Option<crate::web::screen::Attachment>,
    /// The device was lost while the results showed: they stay, and the
    /// screen starts over when the player goes on.
    device_lost: bool,
    _lost: crate::web::screen::Listener,
    debug: NodeRef,
    gfx: Status<&'static str>,
    song: Status<Rc<Loaded>>,
    /// A linked song whose folder needs the permission asked again: the
    /// handle, the folder's name, and why the last ask failed.
    access: Option<(JsValue, String, Option<String>)>,
    stage: Stage,
    raf: Rc<RefCell<Option<AnimationFrame>>>,
    game: Rc<RefCell<Option<GameLoop>>>,
    names: Option<ddi_engine::rules::JudgeNames>,
    /// Layout of the chart played, for the results.
    layout: Option<ddi_chart::Layout>,
    settings: Settings,
    devices: Option<DeviceProfile>,
    /// Song offset (seconds) the current or last play ran with.
    played_offset: f64,
    gamepads: Option<Gamepads>,
    /// A pad start left the audio suspended; the prompt asks for a gesture.
    needs_gesture: bool,
    /// Whether the page is still shown: a decode for it stops when not
    /// (an Opus worker ends with it).
    alive: Rc<std::cell::Cell<bool>>,
    /// Touch screen: show the quit button and touch hints.
    touch: bool,
    /// Decoded background images until they are copied to the GPU (which
    /// waits for the renderer, and never happens mid-play).
    backgrounds: Vec<(BgImage, web_sys::ImageBitmap)>,
    /// Back and mask contents decoded before the game was there.
    stage_contents: Vec<(SpriteSource, web_sys::ImageBitmap, (f32, f32))>,
    /// Pixels of the back and mask contents decoded so far.
    stage_pixels: u64,
    /// Movie files loaded before the game loop was there.
    movies: Vec<(
        String,
        web_sys::Blob,
        ddi_platform::video::VideoFormat,
        bool,
    )>,
    /// The video decoder's download, for the start prompt.
    video_line: Option<String>,
    /// The start prompt on the canvas while the play has not started (or
    /// was interrupted), what it shows, and what its choices reach.
    prompt: Option<crate::components::ui_dev::Embedded>,
    prompt_state: SharedPrompt,
    shell: Rc<PromptShell>,
    /// The theme's results could not start: a card says the score.
    results_failed: bool,
    /// The prompt's picture: the still's and the background's kept
    /// sources with their shapes, once loaded.
    still: Option<(String, f32)>,
    song_picture: Option<(String, f32)>,
    /// The noteskin readied before the game loop was there.
    noteskin: Option<Box<crate::noteskins::Prepared>>,
    _video_tick: gloo::timers::callback::Interval,
    _keys: Option<EventListener>,
    /// A song's results, the theme's screen on the canvas (step 6.1).
    results: Option<crate::components::ui_dev::Embedded>,
    /// While a screen of the page's shows on the canvas (the prompt, the
    /// results): its host, which takes the keys and the pads.
    results_host: Rc<RefCell<Option<Weak<RefCell<crate::components::ui_dev::Host>>>>>,
    /// The ruleset played by and the music's length, for the results.
    rules: Option<ddi_engine::Ruleset>,
    song_length: f64,
    /// The display's refresh rate as the play started (for the history).
    start_refresh: f64,
    /// Keeping the play: not asked, under way (the results wait a moment
    /// for it), or done (why it failed, if it did).
    save: Save,
    _save_wait: Option<gloo::timers::callback::Timeout>,
    /// The play just kept (its replay).
    play_id: Option<String>,
    /// A replay (`crate::replay`): what it replays; why it cannot be.
    replay: Option<Rc<crate::replay::ReplayProp>>,
    refused: Option<String>,
    /// A replay's results compared with the play's.
    replay_check: Option<ddi_ui::state::history::ReplayCheck>,
    /// The chart's best play's ghost, once read (`Settings::ghost`).
    ghost: Option<ddi_ui::state::history::Ghost>,
}

/// Where keeping the play in the history is.
#[derive(Clone, Debug, PartialEq)]
enum Save {
    NotAsked,
    Pending,
    Done(Option<String>),
}

/// How long the results wait for the play to be kept, ms (it takes a few;
/// a library waiting for another tab would take longer).
const SAVE_WAIT_MS: u32 = 1000;

/// What the start prompt's choices reach, inside the input that made
/// them (`ddi_ui::screens::prompt::PromptFn`).
pub(crate) struct PromptShell {
    link: crate::component::Scope<GameCanvas>,
    state: SharedPrompt,
    /// The linked folder's handle while the prompt asks for it.
    access: RefCell<Option<JsValue>>,
    /// The context a pad press made, while the screen waits to see
    /// whether it starts; a tap or key press meanwhile resumes it.
    pad_ctx: RefCell<Option<web_sys::AudioContext>>,
    /// When the results' Retry brought the start prompt back (ms, the
    /// page's clock): a tap right after it is the Retry's, not a start.
    retried_at: std::cell::Cell<f64>,
}

impl PromptShell {
    fn choose(&self, choice: PromptChoice, cx: &mut ddi_ui::Cx) {
        let send = |m: Msg| self.link.send_message(m);
        match choice {
            PromptChoice::Start => {
                if js_sys::Date::now() - self.retried_at.get() < PROMPT_GUARD_MS {
                    return;
                }
                // A pad's start is waiting: this gesture lets it play.
                if let Some(ctx) = self.pad_ctx.borrow().as_ref() {
                    if cx.in_gesture() {
                        let _ = ctx.resume();
                    }
                    return;
                }
                if self.state.borrow().step != PromptStep::Ready {
                    return;
                }
                // A pad's press: the context is watched until it starts
                // (it may stay suspended), not awaited.
                let from_pad = !cx.in_gesture() || cx.menu_from_pad();
                // A press the browser does not count as an activation, on a
                // page that has had none: a context made now could not
                // start (Firefox and Chrome would say so), so the prompt
                // asks for a gesture at once, as when one stays suspended.
                if from_pad && !crate::web::autoplay::may_start() {
                    send(Msg::NeedsGesture);
                    return;
                }
                // Made and asked to start here, inside the gesture (no
                // `await` before; `AGENTS.md`).
                let audio = WebAudio::new();
                if let Ok(a) = &audio {
                    a.resume_now();
                    if from_pad {
                        *self.pad_ctx.borrow_mut() = Some(a.context().clone());
                    }
                }
                self.state
                    .borrow_mut()
                    .set_step(PromptStep::Loading("preparing audio…".into()));
                send(Msg::Begin(Box::new(audio), from_pad));
            }
            PromptChoice::Back => send(Msg::BackRequested),
            PromptChoice::Allow => {
                // The ask happens here, in the gesture.
                if let Some(handle) = self.access.borrow().as_ref() {
                    let answer = crate::web::folders::access(handle);
                    self.link
                        .send_future(async move { Msg::Access(answer.await) });
                }
            }
            PromptChoice::Retry => send(Msg::Retry),
            PromptChoice::Action(id) => send(Msg::CardAction(id)),
            PromptChoice::Device(id) => send(match id.as_str() {
                "calibrate" => Msg::DeviceCalibrate,
                other => Msg::DeviceContinue {
                    copy_audio_from: other.strip_prefix("copy-audio:").map(str::to_string),
                    copy_display_from: other.strip_prefix("copy-display:").map(str::to_string),
                },
            }),
        }
    }
}

enum Status<T> {
    Pending,
    Ready(T),
    Failed(String),
}

enum Stage {
    /// Waiting for the user gesture.
    Idle,
    Decoding,
    /// Audio decoded, but at least one device has no calibration profile.
    NewDevice {
        audio: WebAudio,
        buffer: AudioBuffer,
        new_audio: bool,
        new_display: bool,
    },
    Playing,
    Finished {
        results: Box<Results>,
        calibration: Option<Outcome>,
        device_changed: bool,
        /// Why the automatic setting turned the background videos off.
        video_stopped: Option<ddi_platform::video::StopReason>,
        /// What kept background videos from playing.
        video_trouble: Option<String>,
    },
    /// Calibration result saved to the settings.
    Saved(Outcome),
    Aborted,
    /// Tab hidden or audio context suspended mid-play.
    Interrupted,
    Error(String),
}

impl Component for GameCanvas {
    type Message = Msg;
    type Properties = Props;

    fn create(ctx: &Context<Self>) -> Self {
        let replay = ctx.props().replay.clone();
        let refused = replay.as_ref().and_then(|r| r.refused.clone());
        // A replay is drawn with its own settings (`Replay::settings`).
        let play_settings = play_settings(replay.as_deref());
        match ctx.props().source.clone() {
            // Refused: nothing to load, the prompt says why.
            SongSource::Song { .. } if refused.is_some() => {}
            SongSource::Song { id, chart } => {
                crate::history::set_current(Some(&id));
                load(ctx, id, chart, play_settings.clone())
            }
            SongSource::Calibration(mode) => {
                let song = calibration::song(mode);
                // Drawn with the chosen arrows, as a song is, readied
                // before the start prompt shows (the test starts at once).
                // The audio test holds the skin's beat still
                // (`GameLoop::frame`).
                let skin = noteskin_ready(&Settings::load(), &song, 0, 0.0);
                let loaded = Loaded {
                    song,
                    title: mode.title().into(),
                    subtitle: mode.hint().into(),
                    music_bytes: None,
                    bg_images: Vec::new(),
                    bg_videos: Vec::new(),
                    has_background: false,
                    entry: None,
                    banner: None,
                };
                let link = ctx.link().clone();
                ctx.link().send_future(async move {
                    if let Some(skin) = skin {
                        link.send_message(Msg::NoteskinLoaded(skin.await));
                    }
                    Msg::SongLoaded(Ok(Rc::new(loaded)))
                });
            }
        }
        let results_host: Rc<RefCell<Option<Weak<RefCell<crate::components::ui_dev::Host>>>>> =
            Rc::default();
        let keys = web_sys::window().map(|w| {
            let link = ctx.link().clone();
            let results = results_host.clone();
            EventListener::new_with_options(
                &w,
                "keydown",
                EventListenerOptions::enable_prevent_default(),
                move |event| {
                    let Some(e) = event.dyn_ref::<KeyboardEvent>() else {
                        return;
                    };
                    // The results take their own keys.
                    if e.repeat() || results.borrow().is_some() {
                        return;
                    }
                    // The prompt and the results take their own keys; in
                    // play the session has Escape; on calibration's card
                    // Escape and Backspace leave.
                    if matches!(e.code().as_str(), "Escape" | "Backspace") {
                        link.send_message(Msg::BackRequested);
                    }
                },
            )
        });
        let gamepads = Gamepads::new();
        if let Some(g) = &gamepads {
            let link = ctx.link().clone();
            let results = results_host.clone();
            g.set_listener(Some(Box::new(move |edge| {
                // The results take the pads while they show.
                if let Some(host) = results.borrow().as_ref() {
                    crate::components::ui_dev::pad_edge(host, edge);
                    return;
                }
                if let (ddi_platform::DeviceId::Gamepad(id), true) = (&edge.device, edge.pressed) {
                    link.send_message(Msg::PadPress(id.clone(), edge.control.clone()));
                }
            })));
        }
        let video_tick = {
            let link = ctx.link().clone();
            gloo::timers::callback::Interval::new(250, move || link.send_message(Msg::VideoTick))
        };
        let prompt_state = SharedPrompt::default();
        if let SongSource::Song { id, .. } = &ctx.props().source {
            prompt_state
                .borrow_mut()
                .set_base(crate::web::screen::song_backdrop(id));
        }
        prompt_state
            .borrow_mut()
            .set_step(PromptStep::Loading("loading…".into()));
        let shell = Rc::new(PromptShell {
            link: ctx.link().clone(),
            state: prompt_state.clone(),
            access: RefCell::new(None),
            pad_ctx: RefCell::new(None),
            retried_at: std::cell::Cell::new(f64::NEG_INFINITY),
        });
        GameCanvas {
            dom: RefCell::new(None),
            touch_seen: Rc::new(std::cell::Cell::new(false)),
            slot: NodeRef::default(),
            attachment: None,
            device_lost: false,
            _lost: {
                let link = ctx.link().clone();
                crate::web::screen::on_lost(move || link.send_message(Msg::DeviceLost))
            },
            debug: NodeRef::default(),
            gfx: Status::Pending,
            song: Status::Pending,
            access: None,
            stage: Stage::Idle,
            raf: Rc::new(RefCell::new(None)),
            game: Rc::new(RefCell::new(None)),
            names: None,
            layout: None,
            settings: play_settings,
            devices: None,
            played_offset: 0.0,
            gamepads,
            needs_gesture: false,
            alive: Rc::new(std::cell::Cell::new(true)),
            touch: coarse_pointer(),
            backgrounds: Vec::new(),
            stage_contents: Vec::new(),
            stage_pixels: 0,
            movies: Vec::new(),
            noteskin: None,
            video_line: None,
            prompt: None,
            prompt_state,
            shell,
            results_failed: false,
            still: None,
            song_picture: None,
            _video_tick: video_tick,
            _keys: keys,
            results: None,
            results_host,
            rules: None,
            song_length: 0.0,
            start_refresh: 0.0,
            save: Save::NotAsked,
            _save_wait: None,
            play_id: None,
            replay,
            refused,
            replay_check: None,
            ghost: None,
        }
    }

    fn rendered(&mut self, ctx: &Context<Self>, first_render: bool) {
        if !first_render {
            return;
        }
        let attached = self
            .slot
            .cast::<web_sys::Element>()
            .and_then(|slot| crate::web::screen::attach(&slot, "game-canvas"));
        let Some(canvas) = attached
            .is_some()
            .then(crate::web::screen::canvas)
            .flatten()
        else {
            ctx.link()
                .send_message(Msg::GfxReady(Err("canvas element missing".into())));
            return;
        };
        self.attachment = attached;
        let dpr = web_sys::window()
            .map(|w| w.device_pixel_ratio())
            .unwrap_or(1.0);
        let rect = canvas.get_bounding_client_rect();
        canvas.set_width((rect.width() * dpr).round().max(1.0) as u32);
        canvas.set_height((rect.height() * dpr).round().max(1.0) as u32);
        let link = ctx.link().clone();
        let on_event = ctx.link().callback(Msg::Session);
        // `gfx=gl`: the app's device on WebGL2 from now on.
        if ctx.props().backend == BackendPreference::WebGl2 {
            crate::web::screen::require_gl();
        }
        // The built-in arrows draw no skin: on WebGL2 frames then go
        // straight to the canvas (`Gfx::skin_blending`).
        let skins = self.settings.noteskin.as_deref() != Some(crate::noteskins::BUILT_IN);
        let video = self.settings.video;
        let frame = self.settings.background_frame;
        let slow = ctx.props().slow;
        spawn_local(async move {
            let result = crate::web::screen::gfx().await.map(|gfx| {
                gfx.set_mode(SurfaceMode::Scene);
                // The canvas made with the device (another, if it was made
                // again since).
                let canvas = crate::web::screen::canvas().unwrap_or(canvas);
                gfx.resize(canvas.width(), canvas.height());
                let mut game = Box::new(GameLoop::new(gfx, canvas, on_event, skins));
                game.set_video(video, frame, slow);
                game
            });
            link.send_message(Msg::GfxReady(result));
        });
    }

    fn update(&mut self, ctx: &Context<Self>, msg: Self::Message) -> bool {
        let render = self.handle(ctx, msg);
        self.sync_prompt(ctx);
        // The quit button over a touch screen's play (drawn in the scene).
        crate::web::touch::set_quit_shown(matches!(self.stage, Stage::Playing) && self.touch);
        render
    }

    fn view(&self, ctx: &Context<Self>) {
        self.page(ctx)
    }

    fn destroy(&mut self, _ctx: &Context<Self>) {
        self.alive.set(false);
        self.teardown();
    }
}

impl GameCanvas {
    fn handle(&mut self, ctx: &Context<Self>, msg: Msg) -> bool {
        match msg {
            Msg::GfxReady(Ok(mut game)) => {
                self.gfx = Status::Ready(game.backend_name());
                game.set_debug_element(self.debug.cast::<HtmlElement>());
                *self.game.borrow_mut() = Some(*game);
                self.apply_background();
                // The prompt draws (`sync_prompt`); the play's frames start
                // with the play.
                true
            }
            Msg::DeviceLost => {
                // The results stay (a phone loses contexts under memory
                // pressure); anything else starts over on the new device.
                if matches!(self.stage, Stage::Finished { .. } | Stage::Saved(_)) {
                    self.device_lost = true;
                    self.raf.borrow_mut().take();
                    self.game.borrow_mut().take();
                    // The results' screen draws on the new device, as it was.
                    if let Some(r) = &self.results {
                        r.device_lost();
                    }
                } else {
                    ctx.props().restart.emit(());
                }
                false
            }
            Msg::GfxReady(Err(e)) => {
                self.gfx = Status::Failed(e);
                true
            }
            Msg::SongLoaded(Ok(song)) => {
                // The chart is at hand: its stale plays are judged again.
                if let SongSource::Song { id, .. } = &ctx.props().source {
                    crate::history::chart_loaded(id, &song.song);
                }
                // A replay's chart is the one played (by its content).
                if let (Some(r), SongSource::Song { chart, .. }) =
                    (self.kept(), &ctx.props().source)
                    && r.chart(&song.song) != Ok(*chart)
                {
                    self.refused = Some(r.chart(&song.song).err().unwrap_or_else(|| {
                        "This chart changed since the play: its replay would be wrong.".into()
                    }));
                }
                self.song = Status::Ready(song);
                if self.refused.is_none() && self.replay.as_ref().is_some_and(|r| r.view_results) {
                    self.show_kept_results(ctx);
                }
                self.read_ghost(ctx);
                // Its skin, when the loader could not look (the folder
                // waited for access).
                if let (Some(r), Status::Ready(song), SongSource::Song { chart, .. }) = (
                    self.replay
                        .as_ref()
                        .filter(|r| !r.skin_checked && r.refused.is_none()),
                    &self.song,
                    &ctx.props().source,
                ) && self.refused.is_none()
                    && let (Some(replay), Some(c)) =
                        (r.replay.clone(), song.song.charts.get(*chart))
                {
                    let layout = c.layout.clone();
                    ctx.link().send_future(async move {
                        Msg::ReplaySkin(crate::replay::missing_skin(&replay, &layout).await)
                    });
                }
                true
            }
            Msg::ReplaySkin(why) => {
                // Refused before it plays (never mid-play).
                if why.is_some() && matches!(self.stage, Stage::Idle) {
                    self.refused = why;
                    return true;
                }
                false
            }
            Msg::GhostLoaded(entry) => {
                let Status::Ready(song) = &self.song else {
                    return false;
                };
                self.ghost = entry
                    .and_then(|e| ddi_ui::state::history::Replay::from_entry(&e))
                    .and_then(|r| ddi_ui::state::history::Ghost::of(&r, &song.song))
                    .map_err(|e| web_sys::console::warn_1(&format!("ghost: {e}").into()))
                    .ok();
                false
            }
            Msg::BackgroundLoaded(what, image) => {
                if let Some(image) = image {
                    // The prompt's picture: the still, else the background
                    // (as the play opens on them), kept under its own name.
                    let shape = image.width() as f32 / image.height().max(1) as f32;
                    match what {
                        BgImage::Still => self.still = prompt_source("still").map(|s| (s, shape)),
                        BgImage::Song => {
                            self.song_picture = prompt_source("song").map(|s| (s, shape))
                        }
                        _ => {}
                    }
                    self.backgrounds.push((what, image));
                    self.apply_background();
                }
                false
            }
            Msg::StageContentLoaded(source, content) => {
                if let Some((bitmap, size)) = content {
                    let pixels = u64::from(bitmap.width()) * u64::from(bitmap.height());
                    if self.stage_pixels + pixels > MAX_LAYER_PIXELS {
                        bitmap.close();
                        web_sys::console::warn_1(
                            &"back and mask images past the memory budget are not shown".into(),
                        );
                        return false;
                    }
                    self.stage_pixels += pixels;
                    self.stage_contents.push((source, bitmap, size));
                    self.apply_background();
                }
                false
            }
            Msg::MovieLoaded(name, blob, format, probe) => {
                self.movies.push((name, blob, format, probe));
                self.apply_background();
                false
            }
            Msg::NoteskinLoaded(prepared) => {
                // A second load (after asking for a folder) replaces one
                // not handed over yet.
                if let Some(old) = std::mem::replace(&mut self.noteskin, prepared) {
                    old.close();
                }
                self.apply_background();
                false
            }
            Msg::VideoTick => {
                if !matches!(self.stage, Stage::Idle) {
                    return false;
                }
                let status = self
                    .game
                    .try_borrow()
                    .ok()
                    .and_then(|g| g.as_ref()?.video_status());
                let line = status.and_then(|s| match s {
                    ModuleStatus::Idle => Some("loading the video decoder…".to_string()),
                    ModuleStatus::Loading { loaded, total } if total > 0.0 => Some(format!(
                        "loading the video decoder… {:.0} %",
                        (loaded / total * 100.0).min(100.0)
                    )),
                    ModuleStatus::Loading { .. } => Some("loading the video decoder…".to_string()),
                    ModuleStatus::Ready { .. } => None,
                    ModuleStatus::Failed(e) => Some(format!("background videos cannot play: {e}")),
                });
                let changed = line != self.video_line;
                self.video_line = line;
                changed
            }
            Msg::SongLoaded(Err(LoadError::NeedsAccess { handle, name })) => {
                *self.shell.access.borrow_mut() = Some(handle.clone());
                self.access = Some((handle, name, None));
                true
            }
            Msg::SongLoaded(Err(e)) => {
                self.song = Status::Failed(e.message());
                true
            }
            Msg::Access(granted) => {
                if !granted {
                    if let Some((_, _, refused)) = self.access.as_mut() {
                        *refused = Some("Reading the folder was not allowed.".into());
                    }
                    return true;
                }
                self.access = None;
                self.shell.access.borrow_mut().take();
                if let SongSource::Song { id, chart } = ctx.props().source.clone() {
                    load(ctx, id, chart, self.settings.clone());
                }
                true
            }
            Msg::BackRequested => {
                // During play the session owns the cancel gesture; the
                // results their own keys.
                if matches!(self.stage, Stage::Playing | Stage::Decoding) || self.results.is_some()
                {
                    return false;
                }
                if let Some(game) = self.game.borrow_mut().as_mut() {
                    game.clear_session();
                }
                back_route(&ctx.props().source).navigate();
                false
            }
            Msg::Begin(audio, from_pad) => self.start(ctx, *audio, from_pad),
            Msg::PadPress(id, control) => {
                // During play the session owns the pad (Back included).
                if matches!(self.stage, Stage::Playing | Stage::Decoding) {
                    return false;
                }
                let standard = self
                    .gamepads
                    .as_ref()
                    .and_then(|g| g.pads().into_iter().find(|p| p.id == id))
                    .is_some_and(|p| p.standard);
                let Some(pad) = self.settings.pad_bindings(&id, standard) else {
                    return false;
                };
                let is_start = pad.start.as_deref() == Some(control.as_str());
                let is_back = pad.back.as_deref() == Some(control.as_str());
                // The prompt takes the pads while it shows (its new-device
                // step plays on with a pad's Start); here what is left: a
                // calibration's card.
                match &self.stage {
                    Stage::Finished { .. } | Stage::Interrupted if is_start => {
                        self.handle(ctx, Msg::Retry)
                    }
                    _ if is_back => self.handle(ctx, Msg::BackRequested),
                    _ => false,
                }
            }
            Msg::TouchSeen => {
                let changed = !self.touch;
                self.touch = true;
                changed
            }
            Msg::NeedsGesture => {
                self.shell.pad_ctx.borrow_mut().take();
                self.stage = Stage::Idle;
                self.needs_gesture = true;
                true
            }
            Msg::Decoded(Ok((audio, buffer))) => {
                self.shell.pad_ctx.borrow_mut().take();
                // Probe the devices now that the context is running.
                let refresh = self.refresh_hz();
                let profile = DeviceProfile {
                    audio: audio.fingerprint(),
                    display: crate::web::display::fingerprint(refresh),
                };
                self.settings = self.play_settings();
                let new_audio = self.settings.audio_profile(&profile.audio.id).is_none();
                let new_display = self.settings.display_profile(&profile.display.id).is_none();
                self.devices = Some(profile);
                let is_calibration = matches!(ctx.props().source, SongSource::Calibration(_));
                if (new_audio || new_display)
                    && !is_calibration
                    && !ctx.props().auto
                    && self.replay.is_none()
                {
                    self.stage = Stage::NewDevice {
                        audio,
                        buffer,
                        new_audio,
                        new_display,
                    };
                    return true;
                }
                // A replay changes no setting (no profile made or touched).
                if self.replay.is_none() {
                    self.register_devices(None, None);
                }
                self.begin_session(ctx, audio, buffer);
                true
            }
            Msg::DeviceContinue {
                copy_audio_from,
                copy_display_from,
            } => {
                let Stage::NewDevice { .. } = &self.stage else {
                    return false;
                };
                let Stage::NewDevice { audio, buffer, .. } =
                    std::mem::replace(&mut self.stage, Stage::Decoding)
                else {
                    return false;
                };
                self.register_devices(copy_audio_from.as_deref(), copy_display_from.as_deref());
                self.begin_session(ctx, audio, buffer);
                true
            }
            Msg::DeviceCalibrate => {
                if !matches!(self.stage, Stage::NewDevice { .. }) {
                    return false;
                }
                self.register_devices(None, None);
                self.stage = Stage::Idle;
                Route::Calibrate.navigate();
                true
            }
            Msg::Decoded(Err(e)) => {
                self.shell.pad_ctx.borrow_mut().take();
                self.stage = Stage::Error(e);
                true
            }
            Msg::Session(SessionEvent::Finished {
                results,
                facts,
                calibration,
                device_changed,
            }) => {
                crate::history::set_playing(false);
                // A song's play is kept (a failed one already was, when it
                // failed); a calibration's is not, nor a replay.
                if let SongSource::Song { id, chart } = &ctx.props().source
                    && self.save == Save::NotAsked
                    && self.replay.is_none()
                {
                    self.record_play(ctx, id, *chart, &results, *facts);
                }
                self.check_replay(&results);
                let (video_stopped, video_trouble) = self
                    .game
                    .try_borrow()
                    .ok()
                    .and_then(|g| {
                        let g = g.as_ref()?;
                        Some((g.video_stopped(), g.video_trouble()))
                    })
                    .unwrap_or_default();
                self.stage = Stage::Finished {
                    results,
                    calibration,
                    device_changed,
                    video_stopped,
                    video_trouble,
                };
                if matches!(ctx.props().source, SongSource::Song { .. }) {
                    if self.save == Save::Pending {
                        // A moment for the play to be kept, so the results
                        // can say when it could not.
                        let link = ctx.link().clone();
                        self._save_wait = Some(gloo::timers::callback::Timeout::new(
                            SAVE_WAIT_MS,
                            move || link.send_message(Msg::SaveWaited),
                        ));
                    } else {
                        self.show_results(ctx);
                    }
                }
                true
            }
            Msg::Session(SessionEvent::Failed { results, facts }) => {
                // Final now: kept even if a quit or an interruption cuts
                // the fail's hold short (never a replay's).
                if let SongSource::Song { id, chart } = &ctx.props().source
                    && self.save == Save::NotAsked
                    && self.replay.is_none()
                {
                    self.record_play(ctx, id, *chart, &results, *facts);
                }
                false
            }
            Msg::PlaySaved(r) => {
                self.play_id = r.as_ref().ok().cloned();
                self.save = Save::Done(r.err());
                self.show_waiting_results(ctx)
            }
            Msg::SaveWaited => self.show_waiting_results(ctx),
            Msg::Session(SessionEvent::Aborted) => {
                // A quit is not a play (not kept).
                crate::history::set_playing(false);
                self.stage = Stage::Aborted;
                if let Some(game) = self.game.borrow_mut().as_mut() {
                    game.clear_session();
                }
                match &ctx.props().source {
                    SongSource::Calibration(_) => Route::Calibrate.navigate(),
                    SongSource::Song { .. } => Route::Home.navigate(),
                }
                true
            }
            Msg::Session(SessionEvent::Interrupted) => {
                crate::history::set_playing(false);
                self.stage = Stage::Interrupted;
                if let Some(game) = self.game.borrow_mut().as_mut() {
                    game.clear_session();
                }
                true
            }
            Msg::CardAction(id) => match id.as_str() {
                "save" => self.handle(ctx, Msg::SaveCalibration),
                "again" => self.handle(ctx, Msg::Retry),
                "songs" => {
                    Route::Home.navigate();
                    false
                }
                "next" | "done" => {
                    if let SongSource::Calibration(mode) = &ctx.props().source {
                        match (id.as_str(), mode.next()) {
                            ("next", Some(m)) => Route::CalibrateRun {
                                mode: m.id().into(),
                                auto: false,
                                bias_ms: 0,
                            }
                            .navigate(),
                            // The last step of the guided flow ends it: back
                            // to the songs; earlier ones leave the flow, to
                            // the calibration page.
                            (_, None) => Route::Home.navigate(),
                            _ => Route::Calibrate.navigate(),
                        }
                    }
                    false
                }
                _ => false,
            },
            Msg::Retry => {
                self.results_failed = false;
                if self.device_lost {
                    ctx.props().restart.emit(());
                    return false;
                }
                let had_results = self.close_results();
                if let Some(game) = self.game.borrow_mut().as_mut() {
                    game.clear_session();
                }
                if had_results {
                    self.shell.retried_at.set(js_sys::Date::now());
                }
                // The prompt again (`sync_prompt`).
                self.stage = Stage::Idle;
                true
            }
            Msg::SaveCalibration => {
                let Stage::Finished {
                    calibration: Some(outcome),
                    ..
                } = &self.stage
                else {
                    return false;
                };
                let SongSource::Calibration(mode) = &ctx.props().source else {
                    return false;
                };
                let mut settings = Settings::load();
                mode.apply(&mut settings, self.devices.as_ref(), outcome.total());
                settings.save();
                self.settings = settings;
                let outcome = outcome.clone();
                self.stage = Stage::Saved(outcome);
                true
            }
        }
    }

    /// The page around the canvas: what is not on it yet (a GPU that
    /// failed, calibration's card, results that could not start), the
    /// debug text, the lyrics, the touch quit button, the way back.
    fn page(&self, ctx: &Context<Self>) {
        use crate::component::element;
        let root = ctx.root();
        let source = &ctx.props().source;
        let mut dom = self.dom.borrow_mut();
        let dom = dom.get_or_insert_with(|| {
            let slot = element(root, "div", "game-canvas-slot");
            self.slot.set(&slot);
            // A GPU that failed: plain DOM (nothing draws on the canvas).
            let overlay = element(root, "div", "canvas-overlay error");
            if self.settings.debug {
                let debug = element(root, "pre", "debug-hud");
                self.debug.set(&debug);
            }
            let back = element(root, "a", "back-link");
            let _ = back.set_attribute("href", &back_route(source).to_hash());
            back.set_text_content(Some("← back"));
            let link = ctx.link().clone();
            let touch = self.touch_seen.clone();
            let pointer = EventListener::new(root, "pointerdown", move |e| {
                if let Some(e) = e.dyn_ref::<PointerEvent>()
                    && !touch.get()
                    && matches!(e.pointer_type().as_str(), "touch" | "pen")
                {
                    touch.set(true);
                    link.send_message(Msg::TouchSeen);
                }
            });
            PlayDom {
                overlay,
                back,
                _pointer: pointer,
            }
        });
        let playing = matches!(self.stage, Stage::Playing);
        root.set_class_name(if playing {
            "game-canvas-host playing"
        } else {
            "game-canvas-host"
        });
        match &self.gfx {
            Status::Failed(e) => {
                dom.overlay.set_hidden(false);
                if dom.overlay.child_element_count() == 0 {
                    let page = element(&dom.overlay, "div", "");
                    crate::component::no_gpu(&page, e);
                }
            }
            _ => dom.overlay.set_hidden(true),
        }
        // The way back: the prompt and the results have their own, and a
        // touch screen's play the scene's quit button (`web::touch`).
        dom.back
            .set_hidden((playing && self.touch) || self.results.is_some() || self.prompt.is_some());
    }

    fn teardown(&mut self) {
        crate::history::set_playing(false);
        crate::history::set_current(None);
        self.close_prompt();
        self.close_results();
        release_play_background();
        for (_, image) in self.backgrounds.drain(..) {
            image.close();
        }
        if let Some(p) = self.noteskin.take() {
            p.close();
        }
        // The route can change inside a frame (the game loop or its
        // animation frame borrowed): what is borrowed goes once that
        // returns.
        take_later(&self.raf);
        take_later(&self.game);
        self.attachment = None;
    }
}

/// Empties `cell` now, or right after whoever borrows it returns.
/// [`GameCanvas::play_settings`] for `replay`.
fn play_settings(replay: Option<&crate::replay::ReplayProp>) -> Settings {
    match replay {
        Some(r) => r.settings(),
        None => Settings::load(),
    }
}

fn take_later<T: 'static>(cell: &Rc<RefCell<Option<T>>>) {
    match cell.try_borrow_mut() {
        Ok(mut c) => drop(c.take()),
        Err(_) => {
            let cell = cell.clone();
            gloo::timers::callback::Timeout::new(0, move || {
                if let Ok(mut c) = cell.try_borrow_mut() {
                    drop(c.take());
                }
            })
            .forget();
        }
    }
}

impl GameCanvas {
    /// Decodes the song with the audio context the prompt's Start made
    /// inside its gesture (`PromptShell::choose`), waiting for a pad's to
    /// start.
    fn start(
        &mut self,
        ctx: &Context<Self>,
        audio: Result<WebAudio, String>,
        from_pad: bool,
    ) -> bool {
        let ready = matches!(
            (&self.stage, &self.song, &self.gfx),
            (Stage::Idle, Status::Ready(_), Status::Ready(_))
        );
        let audio = match audio {
            Ok(a) if ready => a,
            Ok(a) => {
                // Not waiting for a start (a second press): let it go.
                let _ = a.context().close();
                self.shell.pad_ctx.borrow_mut().take();
                return false;
            }
            Err(e) => {
                self.stage = Stage::Error(e);
                return true;
            }
        };
        let Status::Ready(song) = &self.song else {
            return false;
        };
        self.stage = Stage::Decoding;
        let song = song.clone();
        let alive = Rc::clone(&self.alive);
        ctx.link().send_future(async move {
            if from_pad {
                // `resume()` may never settle without a gesture; poll the
                // state instead and give up after a while.
                let mut waited = 0;
                while !audio.running() && waited < PAD_START_WAIT_MS {
                    gloo::timers::future::TimeoutFuture::new(50).await;
                    waited += 50;
                }
                if !audio.running() {
                    return Msg::NeedsGesture;
                }
            } else {
                audio.resume().await;
            }
            audio.wait_for_latency().await;
            let r = match &song.music_bytes {
                Some(bytes) => {
                    audio
                        .decoder()
                        .decode_while(bytes, ddi_session::music_target(&song.song), || alive.get())
                        .await
                }
                None => calibration::click_track(&audio, &song.song)
                    .ok_or_else(|| "could not synthesize the click track".to_string()),
            };
            Msg::Decoded(r.map(|b| (audio, b)))
        });
        self.needs_gesture = false;
        true
    }

    fn refresh_hz(&self) -> f64 {
        self.game
            .borrow()
            .as_ref()
            .map(|g| g.refresh_hz())
            .unwrap_or(60.0)
    }

    /// Create/refresh the profiles for the probed devices, optionally copying
    /// another profile's offset for a new one.
    fn register_devices(&mut self, copy_audio_from: Option<&str>, copy_display_from: Option<&str>) {
        let Some(devices) = self.devices.clone() else {
            return;
        };
        let mut settings = Settings::load();
        let audio_initial = copy_audio_from
            .and_then(|id| settings.audio_profile(id).map(|p| p.offset))
            .unwrap_or(settings.audio_offset);
        let display_initial = copy_display_from
            .and_then(|id| settings.display_profile(id).map(|p| p.offset))
            .unwrap_or(settings.visual_offset);
        {
            let p = Settings::touch_profile(
                &mut settings.audio_profiles,
                &devices.audio,
                audio_initial,
            );
            if copy_audio_from.is_some() && !p.calibrated {
                p.offset = audio_initial;
            }
        }
        {
            let p = Settings::touch_profile(
                &mut settings.display_profiles,
                &devices.display,
                display_initial,
            );
            if copy_display_from.is_some() && !p.calibrated {
                p.offset = display_initial;
            }
        }
        settings.save();
        self.settings = self.play_settings();
    }

    /// The settings the play runs with: the stored ones, or a replay's
    /// over them (`Replay::settings`).
    fn play_settings(&self) -> Settings {
        play_settings(self.replay.as_deref())
    }

    /// The chart's best play under the rules it is played by, read for
    /// its ghost when the setting asks (not for a replay or a test).
    fn read_ghost(&mut self, ctx: &Context<Self>) {
        let (Status::Ready(song), SongSource::Song { chart, .. }) =
            (&self.song, &ctx.props().source)
        else {
            return;
        };
        if !self.settings.ghost || self.replay.is_some() {
            return;
        }
        let best =
            crate::history::plays()
                .history()
                .best_of_play(&song.song, *chart, &self.settings);
        if let Some(b) = best {
            let link = ctx.link().clone();
            crate::history::entry(&b.score_play, move |e| {
                link.send_message(Msg::GhostLoaded(e))
            });
        }
    }

    /// The kept play replayed, unless refused.
    fn kept(&self) -> Option<&ddi_ui::state::history::Replay> {
        self.replay.as_ref()?.replay.as_ref()
    }

    /// A replay ended: its results against the log judged at once and the
    /// play's kept ones (`Replay::check`); said on the results and in
    /// `__DDI_DEBUG.replay`.
    fn check_replay(&mut self, results: &Results) {
        let (Some(r), Status::Ready(song)) = (self.kept(), &self.song) else {
            return;
        };
        let check = r.check_on(&song.song, results);
        if !check.matches {
            web_sys::console::error_1(&format!("replay {}: {:?}", r.id, check.problems).into());
        }
        crate::replay::publish(&r.id, "ended", None, Some(&check));
        self.replay_check = Some(check);
    }

    /// Hands decoded backgrounds to the game loop once it exists; it
    /// decides when to copy them to the GPU.
    fn apply_background(&mut self) {
        let mut game = self.game.borrow_mut();
        if let Some(game) = game.as_mut() {
            for (what, image) in self.backgrounds.drain(..) {
                game.add_background(what, image);
            }
            for (source, bitmap, size) in self.stage_contents.drain(..) {
                game.add_stage_content(source, bitmap, size);
            }
            for (name, blob, format, probe) in self.movies.drain(..) {
                game.add_movie(name, blob, format, probe);
            }
            if let Some(p) = self.noteskin.take() {
                game.set_noteskin(Some(p.play), p.images, p.key);
            }
        }
    }

    fn begin_session(&mut self, ctx: &Context<Self>, audio: WebAudio, buffer: AudioBuffer) {
        self.apply_background();
        let Status::Ready(song) = &self.song else {
            return;
        };
        let mut config =
            SessionConfig::from_settings(&self.settings, ctx.props().auto, self.devices.clone());
        let chart = match &ctx.props().source {
            SongSource::Song { chart, .. } => *chart,
            SongSource::Calibration(mode) => {
                config.session.calibrate(*mode);
                config.render.for_calibration(mode.hide_notes());
                0
            }
        };
        config.session.auto_bias = ctx.props().auto_bias;
        config.session.auto_pad = ctx.props().auto_pad;
        config.gamepads = self.gamepads.clone();
        config.touch_canvas = crate::web::screen::canvas();
        // A replay: its log on the lanes, its options and rules, the
        // offsets this device needs (`Replay::replaying`).
        if let Some(r) = self.kept() {
            config.session.set_replay(r);
            crate::replay::publish(&r.id, "playing", None, None);
        }
        let shift = match &ctx.props().source {
            SongSource::Song { id, .. } => {
                ddi_session::play::song_shift(&self.settings, id, self.kept(), false)
            }
            SongSource::Calibration(_) => 0.0,
        };
        // The best play's ghost beside it, lined up by the song offsets.
        if self.settings.ghost && self.replay.is_none() {
            config.session.ghost = self.ghost.clone().map(|g| g.for_play(shift));
        }
        self.played_offset = shift;
        let play_song = song.song.shifted(shift);
        // On the song's timing as played, so the song offset moves them too.
        let schedule = match &ctx.props().source {
            SongSource::Song { .. } => {
                // Movies the import found but that do not play, scripted
                // animations that exist: the second layer shows nothing for
                // them.
                let unshowable: Vec<&str> = song
                    .entry
                    .iter()
                    .flat_map(|e| {
                        e.bg_videos
                            .iter()
                            .filter(|v| !v.plays())
                            .map(|v| v.name.as_str())
                            .chain(e.bg_scripted.iter().map(|p| p.as_str()))
                    })
                    .collect();
                [
                    backgrounds::schedule(
                        &play_song,
                        &song.bg_images,
                        &song.bg_videos,
                        song.has_background,
                    ),
                    backgrounds::second_layer(
                        &play_song,
                        &song.bg_images,
                        &song.bg_videos,
                        &unshowable,
                        song.has_background,
                    ),
                ]
            }
            SongSource::Calibration(_) => Default::default(),
        };
        match PlaySession::start(&play_song, chart, &self.settings, config, audio, buffer) {
            Ok(session) => {
                self.layout = Some(session.layout.clone());
                // The ruleset is chosen per chart when the session starts.
                self.names = Some(session.names.clone());
                self.rules = Some(session.rules.fresh());
                self.song_length = session.length;
                // The lyrics drawn over the play by the 2D pass
                // (`GameLoop`), a work's back and mask layers with the
                // backgrounds.
                let (stage_track, lyrics) = ddi_session::stage::play_layers(
                    &play_song,
                    chart,
                    &session.layout,
                    &self.settings,
                );
                self.start_refresh = self.refresh_hz();
                self.save = Save::NotAsked;
                self.replay_check = None;
                crate::history::set_playing(true);
                if let Some(game) = self.game.borrow_mut().as_mut() {
                    game.set_video(
                        self.settings.video,
                        self.settings.background_frame,
                        ctx.props().slow,
                    );
                    game.set_lyrics(lyrics);
                    game.set_stage_track(stage_track);
                    game.set_background_schedule(schedule);
                    game.set_session(session);
                    game.enter_background(
                        self.prompt_state.borrow().reveal,
                        self.settings
                            .reduced_motion
                            .reduced(crate::components::ui_dev::system_reduced()),
                    );
                }
                self.stage = Stage::Playing;
            }
            Err(e) => self.stage = Stage::Error(e),
        }
    }
}

/// The start prompt on the canvas (`ddi_ui::screens::prompt`).
impl GameCanvas {
    /// The prompt as the page is now: shown while the play has not started
    /// (or was interrupted), with the step, lines and picture of the
    /// moment; gone, the play's frames, once it plays.
    fn sync_prompt(&mut self, ctx: &Context<Self>) {
        // A calibration's outcome, or a song's should its results not
        // start, show on the prompt's screen as a card.
        let card = match (&self.stage, &ctx.props().source) {
            (Stage::Finished { .. } | Stage::Saved(_), SongSource::Calibration(_)) => true,
            (Stage::Finished { .. }, SongSource::Song { .. }) => self.results_failed,
            _ => false,
        };
        let waiting = (card
            || matches!(
                self.stage,
                Stage::Idle
                    | Stage::Decoding
                    | Stage::NewDevice { .. }
                    | Stage::Interrupted
                    | Stage::Error(_)
            ))
            && self.results.is_none()
            && matches!(self.gfx, Status::Ready(_));
        if !waiting {
            if self.prompt.is_some() {
                self.close_prompt();
                if matches!(self.stage, Stage::Playing) {
                    if let Some(game) = self.game.borrow_mut().as_mut() {
                        game.scene_mode();
                    }
                    schedule(self.raf.clone(), self.game.clone(), GameLoop::frame);
                }
            }
            return;
        }
        let step = self.prompt_step(ctx);
        let mut notes = Vec::new();
        if self.needs_gesture {
            notes.push("This browser needs a tap, click or key press to start the audio.".into());
        }
        if matches!(self.stage, Stage::Idle)
            && let Some(line) = &self.video_line
        {
            notes.push(line.clone());
        }
        let picture = self.prompt_picture(ctx);
        let info = self.prompt_info(ctx);
        let changed = {
            let mut st = self.prompt_state.borrow_mut();
            let before = st.version;
            st.set_info(info);
            st.set_step(step);
            st.set_notes(notes);
            st.set_touch(self.touch);
            st.set_picture(picture);
            st.version != before
        };
        if self.prompt.is_none() {
            self.open_prompt();
        } else if changed && let Some(p) = &self.prompt {
            crate::components::ui_dev::wake(&p.host);
        }
    }

    /// What the prompt is at, from the page's state.
    fn prompt_step(&self, ctx: &Context<Self>) -> PromptStep {
        if let Some((_, name, refused)) = &self.access {
            return PromptStep::Access {
                folder: name.clone(),
                refused: refused.clone(),
            };
        }
        if let Some(why) = &self.refused {
            return PromptStep::Error(format!("This replay cannot be watched. {why}"));
        }
        match (&self.song, &self.stage) {
            (Status::Failed(e), _) => PromptStep::Error(format!("The song failed to load: {e}")),
            // A chart the address names that this game does not play (a
            // style it leaves out, a chart past the song's): said at once.
            (Status::Ready(song), _)
                if !matches!(self.stage, Stage::Finished { .. } | Stage::Saved(_))
                    && let SongSource::Song { chart, .. } = &ctx.props().source
                    && song
                        .song
                        .charts
                        .get(*chart)
                        .and_then(|c| song.song.layout_of(c))
                        .is_none() =>
            {
                PromptStep::Error(match song.song.charts.get(*chart) {
                    Some(c) => format!(
                        "This chart is for {}, a style this game does not play.",
                        c.layout
                    ),
                    None => format!("This song has no chart {chart}."),
                })
            }
            (Status::Pending, _) => PromptStep::Loading("loading…".into()),
            (_, Stage::Decoding) => PromptStep::Loading("preparing audio…".into()),
            (
                _,
                Stage::NewDevice {
                    new_audio,
                    new_display,
                    ..
                },
            ) => match &self.devices {
                Some(d) => new_device_step(&self.settings, d, *new_audio, *new_display),
                None => PromptStep::Loading("preparing audio…".into()),
            },
            (_, Stage::Interrupted) => PromptStep::Interrupted,
            (_, Stage::Error(e)) => PromptStep::Error(e.clone()),
            (
                _,
                Stage::Finished {
                    results,
                    calibration,
                    device_changed,
                    ..
                },
            ) => match (&ctx.props().source, calibration) {
                (SongSource::Calibration(mode), Some(o)) => ddi_session::calibration::card(
                    o,
                    *mode,
                    &self.settings,
                    self.devices.as_ref(),
                    self.refresh_hz(),
                    false,
                    *device_changed,
                ),
                _ => results_card(
                    results,
                    self.names.as_ref(),
                    self.layout.as_ref(),
                    *device_changed,
                ),
            },
            (_, Stage::Saved(o)) => match &ctx.props().source {
                SongSource::Calibration(mode) => ddi_session::calibration::card(
                    o,
                    *mode,
                    &self.settings,
                    self.devices.as_ref(),
                    self.refresh_hz(),
                    true,
                    false,
                ),
                SongSource::Song { .. } => PromptStep::Ready,
            },
            _ => PromptStep::Ready,
        }
    }

    /// The song the prompt names (once it is read), its chart and the
    /// mods it will play with.
    fn prompt_info(&self, ctx: &Context<Self>) -> PromptInfo {
        let props = ctx.props();
        match (&self.song, &props.source) {
            (Status::Ready(song), SongSource::Song { chart, .. }) => {
                let c = song.song.charts.get(*chart);
                let style = c
                    .and_then(|c| song.song.layout_of(c))
                    .map(|l| l.name.clone())
                    .unwrap_or_default();
                PromptInfo {
                    title: song.title.clone(),
                    artist: song.subtitle.clone(),
                    chart: c
                        .map(|c| chart_line(c.difficulty, c.meter, &style))
                        .unwrap_or_default(),
                    difficulty: c
                        .map(|c| ddi_ui::screens::data::difficulty_name(c.difficulty).to_string())
                        .unwrap_or_default(),
                    mods: mods_line(&self.settings),
                    badge: if self.replay.is_some() {
                        "REPLAY".into()
                    } else if props.auto {
                        "AUTOPLAY".into()
                    } else {
                        String::new()
                    },
                }
            }
            (Status::Ready(song), SongSource::Calibration(_)) => PromptInfo {
                title: song.title.clone(),
                artist: song.subtitle.clone(),
                badge: if props.auto {
                    "AUTOPLAY".into()
                } else {
                    String::new()
                },
                ..PromptInfo::default()
            },
            // Its name comes once it is read (`SongLoaded`).
            _ => PromptInfo {
                title: match &props.source {
                    SongSource::Calibration(mode) => mode.title().into(),
                    SongSource::Song { .. } => String::new(),
                },
                ..PromptInfo::default()
            },
        }
    }

    /// The song's picture as the play opens on it: its opening still,
    /// else its background, in the background frame, dimmed, with the
    /// field's filter where the chart's lanes will be.
    fn prompt_picture(&self, ctx: &Context<Self>) -> Option<PromptPicture> {
        let (source, aspect) = self.still.clone().or_else(|| self.song_picture.clone())?;
        // The canvas's size in CSS pixels (the field's geometry is its).
        let (cw, ch) = crate::web::screen::canvas()
            .map(|c| (c.client_width() as f32, c.client_height() as f32))
            .filter(|(w, h)| *w > 0.0 && *h > 0.0)
            .unwrap_or((1280.0, 720.0));
        let filter = match (&self.song, &ctx.props().source) {
            (Status::Ready(song), SongSource::Song { chart, .. })
                if self.settings.field_filter > 0.0 =>
            {
                song.song
                    .charts
                    .get(*chart)
                    .and_then(|c| song.song.layout_of(c))
                    .filter(|l| !l.lanes.is_empty())
                    .map(|layout| {
                        let band = ddi_render::scene::filter_fraction(
                            &layout,
                            cw,
                            ch,
                            self.settings.reverse,
                        );
                        (band, self.settings.field_filter)
                    })
            }
            _ => None,
        };
        let frame = self
            .game
            .try_borrow_mut()
            .ok()
            .and_then(|mut g| g.as_mut().map(|g| g.prompt_frame()))
            .flatten();
        Some(PromptPicture {
            source,
            aspect,
            frame,
            brightness: self.settings.bg_brightness,
            filter,
        })
    }

    /// Puts the prompt on the canvas: the play's frames give way to the
    /// screen's (the play's loop keeps measuring the display and readying
    /// the backgrounds, drawing nothing).
    fn open_prompt(&mut self) {
        use ddi_ui::screens::prompt::PromptScreen;
        let Some(container) = self.slot.cast::<HtmlElement>() else {
            return;
        };
        let shell = self.shell.clone();
        let act: PromptFn = Rc::new(move |choice, cx| shell.choose(choice, cx));
        let theme = crate::components::ui_dev::theme_host(Rc::new(crate::components::ui_dev::nfkd));
        let env = theme.env().clone();
        let mut ui = ddi_ui::Ui::new(ddi_ui::screens::DESIGN, ddi_ui::TextMode::Mirror, true);
        ui.set_focus_ring(false);
        ui.push(PromptScreen::new(env, self.prompt_state.clone(), act));
        // What tests read: `__DDI_DEBUG.prompt` and `prompt_backdrop`.
        let state = self.prompt_state.clone();
        let extra = Box::new(move |obj: &js_sys::Object| prompt_debug(obj, &state.borrow()));
        // The play's frames stop; the screen draws, the loop idles.
        self.raf.borrow_mut().take();
        match crate::components::ui_dev::embed(&container, ui, theme, extra) {
            Ok(e) => {
                *self.results_host.borrow_mut() = Some(Rc::downgrade(&e.host));
                self.prompt = Some(e);
                schedule(self.raf.clone(), self.game.clone(), GameLoop::idle);
            }
            Err(e) => web_sys::console::warn_1(&format!("start prompt: {e}").into()),
        }
    }

    /// Takes the prompt down (and what it said in `__DDI_DEBUG`).
    fn close_prompt(&mut self) {
        if self.prompt.take().is_none() {
            return;
        }
        self.results_host.borrow_mut().take();
        self.raf.borrow_mut().take();
        crate::web::forget_debug(&["ui", "prompt", "prompt_backdrop"]);
    }
}

/// The prompt as tests read it: `__DDI_DEBUG.prompt` (`step`, `notes`,
/// `picture`), and `prompt_backdrop` (`still`, `background` or `none`).
fn prompt_debug(obj: &js_sys::Object, st: &PromptState) {
    let set = |o: &js_sys::Object, k: &str, v: JsValue| {
        let _ = js_sys::Reflect::set(o, &k.into(), &v);
    };
    let p = js_sys::Object::new();
    let step = match &st.step {
        PromptStep::Loading(s) => s.as_str(),
        PromptStep::Ready => "ready",
        PromptStep::Access { .. } => "access",
        PromptStep::NewDevice { .. } => "new device",
        PromptStep::Interrupted => "interrupted",
        PromptStep::Error(_) => "error",
        PromptStep::Card { .. } => "card",
    };
    set(&p, "step", step.into());
    set(&p, "reveal", st.reveal.into());
    if let PromptStep::Error(e) = &st.step {
        set(&p, "error", e.as_str().into());
    }
    // A card's heading, lines and actions' ids.
    if let PromptStep::Card {
        heading,
        lines,
        actions,
    } = &st.step
    {
        set(&p, "heading", heading.as_str().into());
        let l = js_sys::Array::new();
        for (t, _) in lines {
            l.push(&t.as_str().into());
        }
        set(&p, "lines", l.into());
        let a = js_sys::Array::new();
        for (id, _) in actions {
            a.push(&id.as_str().into());
        }
        set(&p, "actions", a.into());
    }
    let notes = js_sys::Array::new();
    for n in &st.notes {
        notes.push(&n.as_str().into());
    }
    set(&p, "notes", notes.into());
    set(
        &p,
        "picture",
        st.picture
            .as_ref()
            .map_or(JsValue::NULL, |p| p.source.as_str().into()),
    );
    if let Some(pic) = &st.picture {
        set(&p, "frame", pic.frame.map_or(JsValue::NULL, |f| f.into()));
    }
    set(obj, "prompt", p.into());
    let backdrop = match &st.picture {
        None => "none",
        Some(p) if p.source.ends_with(":still") => "still",
        Some(_) => "background",
    };
    set(obj, "prompt_backdrop", backdrop.into());
}

/// A setting's name as stored (`"quantized"`).
impl GameCanvas {
    /// Keeps the play just ended in the history (`crate::history`): what
    /// it was of, every parameter it ran with, its log and its timing.
    fn record_play(
        &mut self,
        ctx: &Context<Self>,
        id: &str,
        chart: usize,
        results: &Results,
        facts: crate::play::PlayFacts,
    ) {
        use ddi_ui::state::history::Autoplay;
        let Status::Ready(song) = &self.song else {
            return;
        };
        let props = ctx.props();
        let played = ddi_session::record::Played {
            song_id: id,
            song: &song.song,
            chart,
            settings: &self.settings,
            // As stored, so fields this version does not know are kept.
            stored_settings: ddi_platform::storage::get(
                ddi_platform::storage::Area::Local,
                ddi_ui::state::settings::KEY,
            )
            .and_then(|s| serde_json::from_str(&s).ok()),
            devices: self.devices.as_ref(),
            refresh_hz: self.start_refresh,
            graphics: crate::web::screen::current()
                .map(|g| g.backend_name().to_string())
                .unwrap_or_default(),
            song_offset: self.played_offset,
            autoplay: props.auto.then_some(Autoplay {
                pad: props.auto_pad,
                bias: props.auto_bias,
            }),
            time: js_sys::Date::now(),
            app_version: env!("CARGO_PKG_VERSION"),
        };
        let Some(record) = ddi_session::record::play_record(&played, results, &facts) else {
            return;
        };
        self.save = Save::Pending;
        self.play_id = None;
        let link = ctx.link().clone();
        crate::history::record(&record, &facts.log, results, move |r| {
            link.send_message(Msg::PlaySaved(r))
        });
    }

    /// The results that waited for the play to be kept, if they did.
    fn show_waiting_results(&mut self, ctx: &Context<Self>) -> bool {
        if self._save_wait.take().is_none() || self.results.is_some() {
            return false;
        }
        if !matches!(self.stage, Stage::Finished { .. }) {
            return false;
        }
        self.show_results(ctx);
        // The page drops what the play showed over the canvas.
        true
    }

    /// The theme's results for the song just played, on the canvas
    /// (`ddi_ui::screens::results`): the play's frames stop, the screen
    /// takes the canvas, the keys and the pads until Retry or Song list.
    fn show_results(&mut self, ctx: &Context<Self>) {
        use ddi_ui::screens::data::SongEntry;
        use ddi_ui::screens::play::{PlayInfo, results_data};
        use ddi_ui::screens::results::{ActionFn, ResultsActions, ResultsScreen, stored_panels};

        let Stage::Finished {
            results,
            device_changed,
            video_stopped,
            video_trouble,
            ..
        } = &self.stage
        else {
            return;
        };
        let SongSource::Song { id, chart } = ctx.props().source.clone() else {
            return;
        };
        let (Some(rules), Some(layout), Status::Ready(song)) =
            (&self.rules, &self.layout, &self.song)
        else {
            return;
        };
        let Some(container) = self.slot.cast::<HtmlElement>() else {
            return;
        };
        let settings = Settings::load();
        let entry = song.entry.clone();
        let bpm_text = entry.as_ref().map(|e| e.bpm.clone()).unwrap_or_default();
        let played = song.song.charts.get(chart);
        let chart_line = match played {
            Some(c) => ddi_ui::screens::prompt::chart_line(c.difficulty, c.meter, &layout.name),
            None => layout.name.clone(),
        };
        let song_entry = SongEntry {
            id: id.clone(),
            title: song.title.clone(),
            subtitle: String::new(),
            artist: song.subtitle.clone(),
            pack: String::new(),
            bpm: bpm_text.parse().unwrap_or(0.0),
            bpm_text,
            length: self.song_length as f32,
            banner: song.banner.clone(),
            // The background the play read, kept; else the banner.
            background: play_background(),
            charts: Vec::new(),
            ..Default::default()
        };
        let save_error = match &self.save {
            Save::Done(Some(e)) => Some(e.as_str()),
            _ => None,
        };
        let video_notes = ddi_ui::screens::play::results_notes(
            *video_stopped,
            video_trouble.as_deref(),
            save_error,
            self.replay_check.as_ref(),
            self.kept(),
        );
        let kept = self.kept().cloned();
        let mut data = results_data(&PlayInfo {
            results,
            ruleset: rules,
            layout,
            song: song_entry,
            chart: chart_line,
            mods: Vec::new(),
            device_changed: *device_changed,
            video_notes,
            offset: Some((self.played_offset, settings.song_offset(&id))),
            panels: settings.results_panels.clone(),
            autoplay: match &kept {
                Some(r) => r.record.autoplay.is_some(),
                None => ctx.props().auto,
            },
        });
        // A replay's, or a kept play's: marked, when it was played.
        let view_results = self.replay.as_ref().is_some_and(|r| r.view_results);
        if let Some(r) = &kept {
            let kind = if view_results {
                ddi_ui::screens::data::ResultsKind::Kept
            } else {
                ddi_ui::screens::data::ResultsKind::Replay
            };
            ddi_ui::screens::play::mark_replay(
                &mut data,
                kind,
                &r.when_line(&crate::history::tz_minutes),
            );
        }
        // Its replay: the play just kept's, or the kept play's.
        let replay_id = match &kept {
            Some(r) if view_results => Some(r.id.clone()),
            Some(_) => None,
            None => self.play_id.clone(),
        };
        let replay: Option<ActionFn> = replay_id.map(|play| {
            let route = Route::Play {
                song: String::new(),
                chart: 0,
                force_gl: ctx.props().backend == BackendPreference::WebGl2,
                auto: false,
                auto_pad: false,
                bias_ms: 0,
                slow: ctx.props().slow,
                replay: Some(play),
                view_results: false,
            };
            Rc::new(move |_: &ddi_ui::screens::Env, _: &mut ddi_ui::Cx| {
                let route = route.clone();
                spawn_local(async move { route.navigate() });
            }) as ActionFn
        });
        // What the buttons do: after the screen's call returns (they
        // change this page).
        let retry: ActionFn = {
            let link = ctx.link().clone();
            Rc::new(move |_, _| {
                let link = link.clone();
                spawn_local(async move { link.send_message(Msg::Retry) });
            })
        };
        let song_list: ActionFn = Rc::new(|_, _| {
            spawn_local(async { Route::Home.navigate() });
        });
        let offset = ddi_ui::screens::play::song_offset_saver(id.clone());
        let theme = crate::components::ui_dev::theme_host(Rc::new(crate::components::ui_dev::nfkd));
        let env = theme.env().clone();
        let mut ui = ddi_ui::Ui::new(ddi_ui::screens::DESIGN, ddi_ui::TextMode::Mirror, true);
        ui.set_focus_ring(false);
        ui.push(ResultsScreen::new(
            env.clone(),
            data.clone(),
            ResultsActions {
                retry: (!view_results).then_some(retry),
                song_list: Some(song_list),
                offset: Some(offset),
                panels: Some(stored_panels()),
                replay,
            },
        ));
        // What tests read: `__DDI_DEBUG.results`.
        let extra = Box::new(move |obj: &js_sys::Object| {
            let st = env.state.borrow();
            js_debug(obj, &data, &st);
        });
        // The play's frames stop; the screen draws.
        self.raf.borrow_mut().take();
        match crate::components::ui_dev::embed(&container, ui, theme, extra) {
            Ok(e) => {
                *self.results_host.borrow_mut() = Some(Rc::downgrade(&e.host));
                self.results = Some(e);
            }
            Err(e) => {
                web_sys::console::warn_1(&format!("results: {e}").into());
                // A card on the prompt's screen instead (`sync_prompt`).
                self.results_failed = true;
            }
        }
    }

    /// A kept play's results (`view=results`): judged again from its log
    /// on the chart just read, shown as a play's are, with its replay.
    fn show_kept_results(&mut self, ctx: &Context<Self>) {
        let (Some(r), Status::Ready(song), SongSource::Song { chart, .. }) =
            (self.kept().cloned(), &self.song, &ctx.props().source)
        else {
            return;
        };
        let chart = *chart;
        let results = match r.results(&song.song) {
            Ok(results) => results,
            Err(e) => {
                self.refused = Some(e);
                return;
            }
        };
        let Some(c) = song.song.charts.get(chart) else {
            return;
        };
        let ruleset = r.record.ruleset.build(c);
        self.names = Some(ruleset.names.clone());
        self.layout = song.song.layout_of(c);
        self.rules = Some(ruleset);
        self.song_length = song.entry.as_ref().map_or(0.0, |e| e.length);
        self.played_offset = r.record.song_offset;
        crate::replay::publish(&r.id, "results", None, None);
        self.stage = Stage::Finished {
            results: Box::new(results),
            calibration: None,
            device_changed: false,
            video_stopped: None,
            video_trouble: None,
        };
        self.show_results(ctx);
    }

    /// Takes the results' screen down (and what it said in
    /// `__DDI_DEBUG`); whether it was up.
    fn close_results(&mut self) -> bool {
        self.results_host.borrow_mut().take();
        let was = self.results.take().is_some();
        if was {
            crate::web::forget_debug(&["results", "ui"]);
        }
        was
    }
}

/// The results as tests read them (`__DDI_DEBUG.results`).
fn js_debug(
    obj: &js_sys::Object,
    d: &ddi_ui::screens::data::ResultsData,
    st: &ddi_ui::screens::ThemeState,
) {
    let set = |o: &js_sys::Object, k: &str, v: JsValue| {
        let _ = js_sys::Reflect::set(o, &k.into(), &v);
    };
    let r = js_sys::Object::new();
    set(&r, "grade", d.grade.as_str().into());
    set(&r, "score", d.score.as_str().into());
    set(
        &r,
        "ex",
        d.ex.as_deref().map_or(JsValue::NULL, |s| s.into()),
    );
    set(
        &r,
        "lamp",
        d.lamp.as_deref().map_or(JsValue::NULL, |s| s.into()),
    );
    set(&r, "failed", d.failed.into());
    set(&r, "line", d.line.as_str().into());
    set(&r, "mods", d.mods.as_str().into());
    let tiers = js_sys::Array::new();
    for j in &d.judgements {
        let t = js_sys::Object::new();
        set(&t, "name", j.name.as_str().into());
        set(&t, "count", j.count.into());
        tiers.push(&t);
    }
    set(&r, "tiers", tiers.into());
    let stats = js_sys::Object::new();
    for (k, v) in &d.stats {
        set(&stats, k, v.as_str().into());
    }
    set(&r, "stats", stats.into());
    let notes = js_sys::Array::new();
    for n in &d.notes {
        notes.push(&n.as_str().into());
    }
    set(&r, "notes", notes.into());
    let lanes = js_sys::Array::new();
    for l in &d.lanes {
        lanes.push(&format!("{} {}", l.symbol, l.label).into());
    }
    set(&r, "lanes", lanes.into());
    set(&r, "hits", (d.hits.len() as f64).into());
    set(&r, "lifePoints", (d.life.len() as f64).into());
    set(&r, "windows", (d.windows.len() as f64).into());
    set(&r, "danger", d.danger.map_or(JsValue::NULL, |v| v.into()));
    set(&r, "panels", st.panels.as_str().into());
    let sets = js_sys::Array::new();
    for s in &d.panel_sets {
        sets.push(&s.name.as_str().into());
    }
    set(&r, "panelSets", sets.into());
    set(&r, "offsetMs", st.song_offset.into());
    set(
        &r,
        "suggestedMs",
        d.offset
            .and_then(|o| o.suggested)
            .map_or(JsValue::NULL, |s| (s * 1000.0).round().into()),
    );
    set(&r, "status", st.status.as_str().into());
    set(
        &r,
        "backdrop",
        st.backdrop.as_deref().map_or(JsValue::NULL, |s| s.into()),
    );
    set(&r, "song", d.song.title.as_str().into());
    set(obj, "results", r.into());
}

fn back_route(source: &SongSource) -> Route {
    match source {
        SongSource::Song { .. } => Route::Home,
        SongSource::Calibration(_) => Route::Calibrate,
    }
}

/// A calibration play's summary when it gave no outcome (or a song's,
/// should the theme's results not start), as the old results card had
/// it: a card on the start prompt's screen.
fn results_card(
    r: &Results,
    names: Option<&ddi_engine::rules::JudgeNames>,
    layout: Option<&ddi_chart::Layout>,
    device_changed: bool,
) -> PromptStep {
    let mut lines: Vec<(String, String)> = Vec::new();
    let mut push = |s: String, color: &str| lines.push((s, color.to_string()));
    if let (Some(ex), Some(max)) = (r.score.ex, r.score.max_ex) {
        push(format!("EX {ex} / {max}"), "Muted");
    }
    let mods = mods_summary(
        &r.transform,
        r.appearance,
        r.scroll_action,
        layout.map(|l| (r.lane_map.as_slice(), l)),
    )
    .join(" · ");
    if !mods.is_empty() {
        push(mods, "Muted");
    }
    if r.failed {
        push("FAILED".into(), "Danger");
    }
    if device_changed {
        push(
            "an audio device or the display changed during play".into(),
            "Muted",
        );
    }
    if let Some(lamp) = ddi_ui::screens::play::lamp(r) {
        push(lamp, "Accent");
    }
    let t = &r.tally;
    let tiers: Vec<String> = (0..6)
        .filter_map(|i| {
            let name = names.map_or_else(|| format!("tier {}", i + 1), |n| n.tiers[i].clone());
            (!name.is_empty()).then(|| format!("{name} {}", t.taps[i]))
        })
        .collect();
    push(tiers.join(" · "), "Secondary");
    let held = names.map_or_else(
        || "held / let go".into(),
        |n| format!("{} / {}", n.held, n.let_go),
    );
    push(format!("{held} {} / {}", t.held, t.let_go), "Secondary");
    if t.checkpoint_hit + t.checkpoint_miss > 0 {
        push(
            format!(
                "hold ticks hit / missed {} / {}",
                t.checkpoint_hit, t.checkpoint_miss
            ),
            "Secondary",
        );
    }
    let mut more = format!("mines hit {} · max combo {}", t.mine_hit, r.max_combo);
    if t.boo > 0 {
        more.push_str(&format!(" · stray presses {}", t.boo));
    }
    push(more, "Secondary");
    push(
        format!(
            "fast / slow {} / {} · mean offset {:+.1} ms (σ {:.1})",
            r.fast,
            r.slow,
            r.mean_delta * 1000.0,
            r.stddev_delta * 1000.0
        ),
        "Secondary",
    );
    PromptStep::Card {
        heading: format!("{} {}", r.grade, score_text(r)),
        lines,
        actions: vec![
            ("again".into(), "Retry".into()),
            ("songs".into(), "Song list".into()),
        ],
    }
}

fn score_text(r: &Results) -> String {
    ddi_ui::screens::play::score_text(r)
}

/// Whether the primary pointer is a finger (phones, tablets).
fn coarse_pointer() -> bool {
    web_sys::window()
        .and_then(|w| w.match_media("(pointer: coarse)").ok().flatten())
        .is_some_and(|q| q.matches())
}

/// The play's loop: `step` on each animation frame while the game lives
/// (`GameLoop::frame`; `GameLoop::idle` while the start prompt draws, which
/// measures and readies, drawing nothing).
fn schedule(
    raf: Rc<RefCell<Option<AnimationFrame>>>,
    game: Rc<RefCell<Option<GameLoop>>>,
    step: fn(&mut GameLoop, f64),
) {
    let raf2 = raf.clone();
    let game2 = game.clone();
    let handle = request_animation_frame(move |time_ms| {
        if let Some(g) = game2.borrow_mut().as_mut() {
            step(g, time_ms);
        } else {
            return;
        }
        schedule(raf2, game2, step);
    });
    *raf.borrow_mut() = Some(handle);
}

/// Pixels of a work's layers kept decoded, in all (each is kept twice: the
/// bitmap, for a skin's reinstall, and its texture).
const MAX_LAYER_PIXELS: u64 = 48 << 20;

/// Loads the song and its background images, and readies the chosen
/// noteskin for the chart's layout; the song arrives as [`Msg::SongLoaded`].
/// `settings`: those the play runs with (a replay's own).
fn load(ctx: &Context<GameCanvas>, id: String, chart: usize, settings: Settings) {
    let link = ctx.link().clone();
    // The song offset the play will shift the notes by (`begin_session`),
    // for the skin's Lua.
    let kept = ctx.props().replay.as_ref().and_then(|r| r.replay.as_ref());
    let shift = ddi_session::play::song_shift(&settings, &id, kept, false);
    let background_name = new_play_background(&id);
    let show_background = settings.bg_brightness > 0.0;
    // Off: no movie file is read and the decoder is never fetched.
    let play_videos = show_background && settings.video != VideoMode::Off;
    ctx.link().send_future(async move {
        let r = load_song(&id).await.map(|loaded| {
            // The song's own images, then those found in shared folders,
            // under the names it uses; the movies that play.
            let bg_images: Vec<String> = loaded
                .entry
                .bg_images
                .iter()
                .map(|p| p.to_string())
                .chain(loaded.shared_bg.iter().map(|(name, _)| name.clone()))
                .collect();
            let bg_videos: Vec<String> = loaded
                .entry
                .bg_videos
                .iter()
                .filter(|v| v.plays())
                .map(|v| v.name.to_string())
                .chain(loaded.shared_videos.iter().map(|(name, _)| name.clone()))
                .collect();
            // The opening movie's still (`crate::stills`): the one taken,
            // when it is still this schedule's opening, or taken now for a
            // song imported before stills were kept (never with videos
            // off: no decoder then).
            let opening = backgrounds::opening_movie(
                &loaded.song,
                &backgrounds::schedule(
                    &loaded.song,
                    &bg_images,
                    &bg_videos,
                    loaded.entry.background.is_some(),
                ),
            );
            // With videos off the movie never plays: the prompt and the
            // field show the `#BACKGROUND`, as the cut then goes to it.
            let still_taken = play_videos
                && matches!(
                    &loaded.still,
                    Some(crate::songs::Still::Taken { movie, .. })
                        if opening.as_ref().is_some_and(|o| o.movie == *movie)
                );
            // A blank still (a black or white opening frame) stays out of
            // the results' backdrop, which shows the song's background.
            let mut blank = match &loaded.still {
                Some(crate::songs::Still::Taken { blank, .. }) => *blank,
                _ => None,
            };
            // A song imported before its shared folder recorded no movie:
            // looked at again when its opening movie is found now.
            let still_now = matches!(loaded.still, None | Some(crate::songs::Still::NoMovie))
                && opening.is_some()
                && play_videos
                && crate::songs::is_imported_id(&loaded.entry.id);
            // The results' backdrop is kept by the still's load when there
            // is one (the still, or the song's background when blank), else
            // by the song background's: one keeper, so the order is fixed.
            let still_keeps = show_background && (still_taken || still_now);
            if still_keeps {
                let id = loaded.entry.id.clone();
                let entry = loaded.entry.clone();
                let record = loaded.record.clone();
                let background_name = background_name.clone();
                link.send_future(async move {
                    let keep_song = || async {
                        if let Ok(Some(blob)) =
                            load_background(&entry, &BgImage::Song, &[], record.as_deref()).await
                        {
                            crate::web::textures::keep(&background_name, Some(blob));
                        }
                    };
                    if still_now {
                        let filled =
                            crate::stills::fill(&id, &mut crate::stills::Taker::default()).await;
                        web_sys::console::info_1(
                            &format!("song picture from its video: {filled:?}").into(),
                        );
                        let crate::stills::Filled::Taken { blank: b } = filled else {
                            keep_song().await;
                            return Msg::BackgroundLoaded(BgImage::Still, None);
                        };
                        blank = Some(b);
                    }
                    let bitmap = match crate::stills::load(&id).await {
                        Ok(blob) => {
                            // Taken before blankness was kept: checked now.
                            if blank.is_none() {
                                blank = crate::stills::check_blank(&id, &blob)
                                    .await
                                    .map_err(|e| web_sys::console::warn_1(&e.into()))
                                    .ok();
                            }
                            // The prompt's picture, blank or not (the play
                            // opens on it); the results' backdrop, unless
                            // blank.
                            crate::web::textures::keep(
                                &format!("{background_name}:still"),
                                Some(blob.clone()),
                            );
                            if blank == Some(false) {
                                crate::web::textures::keep(&background_name, Some(blob.clone()));
                            } else {
                                keep_song().await;
                            }
                            crate::web::image::decode(&blob, MAX_BACKGROUND_SIDE)
                                .await
                                .map_err(|e| web_sys::console::warn_1(&e.into()))
                                .ok()
                        }
                        Err(e) => {
                            web_sys::console::warn_1(&e.into());
                            keep_song().await;
                            None
                        }
                    };
                    Msg::BackgroundLoaded(BgImage::Still, bitmap)
                });
            }
            if show_background {
                // Loads alongside the start prompt; the play
                // never waits for it.
                let shared = Rc::new(loaded.shared_bg.clone());
                let images = std::iter::once(BgImage::Song)
                    .chain(bg_images.iter().cloned().map(BgImage::File));
                // A work's back and mask layers: their images (those the
                // import kept) and text, under both scroll directions'
                // data, within the budgets.
                let kept: std::collections::HashSet<&str> = loaded
                    .entry
                    .layer_images
                    .iter()
                    .map(|p| p.as_str())
                    .collect();
                let mut sources = ddi_session::stage::layer_sources(&loaded.song, chart);
                sources.retain(|s| match s {
                    SpriteSource::Image(name) => kept.contains(&**name),
                    SpriteSource::Text { .. } => true,
                });
                for source in sources {
                    let entry = loaded.entry.clone();
                    let record = loaded.record.clone();
                    link.send_future(async move {
                        let content = match &source {
                            SpriteSource::Image(name) => {
                                match load_layer_image(&entry, name, record.as_deref()).await {
                                    Ok(blob) => {
                                        crate::web::image::decode_sized(&blob, MAX_LAYER_SIDE)
                                            .await
                                            .map(|(b, (w, h))| (b, (w as f32, h as f32)))
                                    }
                                    Err(e) => Err(e),
                                }
                            }
                            SpriteSource::Text { text, size, color } => {
                                crate::web::image::text_bitmap(
                                    text,
                                    *size,
                                    color,
                                    f64::from(TEXT_SCALE),
                                )
                                .await
                            }
                        };
                        let content = content
                            .map_err(|e| web_sys::console::warn_1(&e.into()))
                            .ok();
                        Msg::StageContentLoaded(source, content)
                    });
                }
                for what in images {
                    let entry = loaded.entry.clone();
                    let record = loaded.record.clone();
                    let shared = shared.clone();
                    let background_name = background_name.clone();
                    link.send_future(async move {
                        let bitmap = match load_background(
                            &entry,
                            &what,
                            &shared,
                            record.as_deref(),
                        )
                        .await
                        {
                            Ok(Some(blob)) => {
                                // Kept for the prompt's picture and the
                                // results' backdrop (read once).
                                if what == BgImage::Song {
                                    crate::web::textures::keep(
                                        &format!("{background_name}:song"),
                                        Some(blob.clone()),
                                    );
                                    if !still_keeps {
                                        crate::web::textures::keep(
                                            &background_name,
                                            Some(blob.clone()),
                                        );
                                    }
                                }
                                let side = match what {
                                    BgImage::Song => MAX_BACKGROUND_SIDE,
                                    // Movies are not loaded here (only images
                                    // are listed above).
                                    BgImage::File(_)
                                    | BgImage::Movie(_)
                                    | BgImage::Still
                                    | BgImage::Clear
                                    | BgImage::Plain => MAX_CHANGE_SIDE,
                                };
                                crate::web::image::decode(&blob, side)
                                    .await
                                    .map_err(|e| {
                                        web_sys::console::warn_1(&e.into());
                                    })
                                    .ok()
                            }
                            Ok(None) => None,
                            Err(e) => {
                                web_sys::console::warn_1(&e.into());
                                None
                            }
                        };
                        Msg::BackgroundLoaded(what, bitmap)
                    });
                }
                // Movies: their files (stored, linked or in a shared
                // folder), opened by the game loop as their changes come.
                // Shared ones were not sniffed; the decoder probes them.
                let shared_videos = Rc::new(loaded.shared_videos.clone());
                let unknown = ddi_platform::video::VideoFormat {
                    container: ddi_platform::video::VideoContainer::Unknown,
                    codec: ddi_platform::video::VideoCodec::Unknown,
                };
                let movies: Vec<_> = loaded
                    .entry
                    .bg_videos
                    .iter()
                    .filter(|v| v.plays())
                    .map(|v| (v.name.to_string(), v.format.clone()))
                    .chain(
                        loaded
                            .shared_videos
                            .iter()
                            .map(|(name, _)| (name.clone(), unknown.clone())),
                    )
                    .filter(|_| play_videos)
                    .collect();
                // The movie the song shows first sets the background frame.
                let first =
                    backgrounds::schedule(&loaded.song, &[] as &[String], &bg_videos, false)
                        .into_iter()
                        .find_map(|s| match s.image {
                            BgImage::Movie(name) => Some(name),
                            _ => None,
                        });
                for (name, format) in movies {
                    let probe = first.as_ref() == Some(&name);
                    let entry = loaded.entry.clone();
                    let record = loaded.record.clone();
                    let shared = shared_videos.clone();
                    link.send_future_batch(async move {
                        let what = BgImage::Movie(name.clone());
                        match load_background(&entry, &what, &shared, record.as_deref()).await {
                            Ok(Some(blob)) => vec![Msg::MovieLoaded(name, blob, format, probe)],
                            Ok(None) => Vec::new(),
                            Err(e) => {
                                web_sys::console::warn_1(&e.into());
                                Vec::new()
                            }
                        }
                    });
                }
            }
            Rc::new(Loaded {
                title: loaded.entry.title.clone(),
                subtitle: loaded.entry.artist.clone(),
                bg_images,
                bg_videos,
                has_background: loaded.entry.background.is_some(),
                banner: loaded.entry.banner.as_ref().map(|b| {
                    if crate::songs::is_imported_id(&loaded.entry.id) {
                        format!("files:{}", crate::songs::file_key(&loaded.entry.id, b))
                    } else {
                        format!("songs/{}/{b}", loaded.entry.id)
                    }
                }),
                entry: Some(loaded.entry.clone()),
                song: loaded.song,
                music_bytes: Some(loaded.music_bytes),
            })
        });
        if let Ok(loaded) = &r
            && let Some(skin) = noteskin_ready(&settings, &loaded.song, chart, shift)
        {
            link.send_future(async move { Msg::NoteskinLoaded(skin.await) });
        }
        Msg::SongLoaded(r)
    });
}

/// Readies the chosen noteskin for `song`'s chart `chart` (its layout,
/// colors and scroll direction), for [`Msg::NoteskinLoaded`]; its Lua
/// sees the timing as the play shifts it by `shift` (the song offset).
/// `None` for the built-in arrows.
fn noteskin_ready(
    settings: &Settings,
    song: &Song,
    chart: usize,
    shift: f64,
) -> Option<impl std::future::Future<Output = Option<Box<crate::noteskins::Prepared>>> + use<>> {
    let c = song.charts.get(chart)?;
    let layout = song.layout_of(c)?;
    let key = settings.play_skin(&layout.id, Some(c))?;
    let env = ddi_noteskin::lua::chart_env(song, c, &layout, settings.reverse).shifted(shift);
    Some(async move {
        crate::noteskins::prepare(&key, &layout, env)
            .await
            .map(Box::new)
    })
}
