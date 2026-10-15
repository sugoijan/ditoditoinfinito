//! Gameplay screen: loads the song (or generates the calibration one), waits
//! for a user gesture (the audio context must be created inside one), starts
//! the play session and shows the results when it ends.
//!
//! A controller's Start button starts the song too: Chrome and Firefox treat
//! a gamepad press as a user gesture, so the audio context is created right
//! after the poll that saw the press (Yew handles the message in a microtask
//! of the same task), as a keydown does. Safari does not; when the context
//! does not start, the prompt asks for a tap, click or key press instead, and
//! one made while the screen still waits resumes the waiting context.
//!
//! On touch screens (a coarse pointer, or once a finger has touched the
//! screen) the play screen shows a quit button instead of the back link,
//! since there is no Escape key and a stray tap on a link would leave the
//! song at once; the button runs the same hold or double-tap gesture.

use std::cell::RefCell;
use std::rc::{Rc, Weak};

use ddi_chart::Song;
use ddi_engine::lyrics::LyricTrack;
use ddi_engine::player::Results;
use ddi_engine::sprites::{SpriteSource, SpriteTrack};
use ddi_platform::DeviceProfile;
use gloo::events::{EventListener, EventListenerOptions};
use gloo::render::{AnimationFrame, request_animation_frame};
use wasm_bindgen::{JsCast, JsValue};
use wasm_bindgen_futures::spawn_local;
use web_sys::{AudioBuffer, HtmlElement, KeyboardEvent};
use yew::prelude::*;

use crate::calibration::{self, CalMode, Outcome};
use crate::game_loop::GameLoop;
use crate::lyrics::LyricsView;
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

/// How long after the results' Retry a start is not taken (ms).
const PROMPT_GUARD_MS: f64 = 500.0;

/// How long a context created from a pad press may take to start before the
/// screen asks for a real gesture, ms.
const PAD_START_WAIT_MS: u32 = 1500;
/// The song background is scaled down to this side at most: the texture
/// size every WebGL2 device supports, and more than the field needs.
const MAX_BACKGROUND_SIDE: u32 = 2048;
/// Background-change images are scaled down further: a song may have
/// dozens, all kept on the GPU during play.
const MAX_CHANGE_SIDE: u32 = 1280;

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
        crate::web::textures::keep(&old, None);
    }
}

pub(crate) enum Msg {
    GfxReady(Result<Box<GameLoop>, String>),
    /// The app's device was lost and made again.
    DeviceLost,
    /// Escape/Backspace outside of play: leave the screen.
    BackRequested,
    SongLoaded(Result<Rc<Loaded>, LoadError>),
    /// The answer to asking for a linked song's folder.
    Access(bool),
    StartRequested,
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
}

#[derive(Properties, PartialEq)]
pub(crate) struct Props {
    pub(crate) source: SongSource,
    #[prop_or_default]
    pub(crate) backend: BackendPreference,
    #[prop_or_default]
    pub(crate) auto: bool,
    /// Autoplay through the headless tests' fake gamepad.
    #[prop_or_default]
    pub(crate) auto_pad: bool,
    /// Autoplay bias in seconds (testing aid).
    #[prop_or_default]
    pub(crate) auto_bias: f64,
    /// Frames take longer while a movie is drawn (testing aid for the
    /// automatic video setting).
    #[prop_or_default]
    pub(crate) slow: bool,
    /// Asks the app to mount the screen again (a lost device).
    #[prop_or_default]
    pub(crate) restart: Callback<()>,
}

pub(crate) struct GameCanvas {
    /// Where the app's canvas is put (`web::screen`).
    slot: NodeRef,
    /// The app's canvas, while this screen holds it.
    attachment: Option<crate::web::screen::Attachment>,
    /// The device was lost while the results showed: they stay, and the
    /// screen starts over when the player goes on.
    device_lost: bool,
    _lost: crate::web::screen::Listener,
    debug: NodeRef,
    /// Lyrics overlay (filled by the game loop).
    lyrics: NodeRef,
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
    /// The context a pad press created, while the screen waits to see
    /// whether it starts; a tap or key press meanwhile resumes it.
    pad_start_ctx: Option<web_sys::AudioContext>,
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
    /// The noteskin readied before the game loop was there.
    noteskin: Option<Box<crate::noteskins::Prepared>>,
    _video_tick: gloo::timers::callback::Interval,
    _keys: Option<EventListener>,
    /// A song's results, the theme's screen on the canvas (step 6.1).
    results: Option<crate::components::ui_dev::Embedded>,
    /// While they show: their host, which takes the keys and the pads.
    results_host: Rc<RefCell<Option<Weak<RefCell<crate::components::ui_dev::Host>>>>>,
    /// The ruleset played by and the music's length, for the results.
    rules: Option<ddi_engine::Ruleset>,
    song_length: f64,
    /// When the results' Retry brought the start prompt back (ms, the
    /// page's clock): a click right after it is the tap's, not a start.
    retried_at: f64,
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
        results: Results,
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
        match ctx.props().source.clone() {
            SongSource::Song { id, chart } => load(ctx, id, chart),
            SongSource::Calibration(mode) => {
                let song = calibration::song(mode);
                // Drawn with the chosen arrows, as a song is, readied
                // before the start prompt shows (the test starts at once).
                // The audio test holds the skin's beat still
                // (`GameLoop::frame`).
                let skin = noteskin_ready(&Settings::load(), &song, 0);
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
                    match e.code().as_str() {
                        "Enter" | "Space" => link.send_message(Msg::StartRequested),
                        "Escape" | "Backspace" => link.send_message(Msg::BackRequested),
                        _ => {}
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
        GameCanvas {
            slot: NodeRef::default(),
            attachment: None,
            device_lost: false,
            _lost: {
                let link = ctx.link().clone();
                crate::web::screen::on_lost(move || link.send_message(Msg::DeviceLost))
            },
            debug: NodeRef::default(),
            lyrics: NodeRef::default(),
            gfx: Status::Pending,
            song: Status::Pending,
            access: None,
            stage: Stage::Idle,
            raf: Rc::new(RefCell::new(None)),
            game: Rc::new(RefCell::new(None)),
            names: None,
            layout: None,
            settings: Settings::load(),
            devices: None,
            played_offset: 0.0,
            gamepads,
            needs_gesture: false,
            pad_start_ctx: None,
            touch: coarse_pointer(),
            backgrounds: Vec::new(),
            stage_contents: Vec::new(),
            stage_pixels: 0,
            movies: Vec::new(),
            noteskin: None,
            video_line: None,
            _video_tick: video_tick,
            _keys: keys,
            results: None,
            results_host,
            rules: None,
            song_length: 0.0,
            retried_at: f64::NEG_INFINITY,
        }
    }

    fn rendered(&mut self, ctx: &Context<Self>, first_render: bool) {
        // The folder prompt's button takes Enter (a gesture, as the ask
        // needs); `autofocus` is ignored on inserted elements.
        if self.access.is_some()
            && let Some(button) = web_sys::window()
                .and_then(|w| w.document())
                .and_then(|d| d.query_selector(".access-prompt button").ok().flatten())
                .and_then(|b| b.dyn_into::<HtmlElement>().ok())
        {
            let _ = button.focus();
        }
        if !first_render {
            // The debug element is re-created on re-render; keep the loop pointed at it.
            if let Some(game) = self.game.borrow_mut().as_mut() {
                game.set_debug_element(self.debug.cast::<HtmlElement>());
            }
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
        spawn_local(async move {
            let result = crate::web::screen::gfx().await.map(|gfx| {
                gfx.set_mode(SurfaceMode::Scene);
                // The canvas made with the device (another, if it was made
                // again since).
                let canvas = crate::web::screen::canvas().unwrap_or(canvas);
                gfx.resize(canvas.width(), canvas.height());
                Box::new(GameLoop::new(gfx, canvas, on_event, skins))
            });
            link.send_message(Msg::GfxReady(result));
        });
    }

    fn update(&mut self, ctx: &Context<Self>, msg: Self::Message) -> bool {
        match msg {
            Msg::GfxReady(Ok(mut game)) => {
                self.gfx = Status::Ready(game.backend_name());
                game.set_debug_element(self.debug.cast::<HtmlElement>());
                *self.game.borrow_mut() = Some(*game);
                self.apply_background();
                schedule(self.raf.clone(), self.game.clone());
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
                self.song = Status::Ready(song);
                true
            }
            Msg::BackgroundLoaded(what, image) => {
                if let Some(image) = image {
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
                if let SongSource::Song { id, chart } = ctx.props().source.clone() {
                    load(ctx, id, chart);
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
            Msg::StartRequested => {
                // The click a touch makes after a tap on the results'
                // Retry lands on the prompt that replaced them.
                if js_sys::Date::now() - self.retried_at < PROMPT_GUARD_MS {
                    return false;
                }
                if let (Stage::Decoding, Some(audio)) = (&self.stage, &self.pad_start_ctx) {
                    let _ = audio.resume();
                    return false;
                }
                self.start(ctx, false)
            }
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
                match &self.stage {
                    Stage::Idle if is_start => self.start(ctx, true),
                    // A player on a pad alone must get past the new-device
                    // prompt too: Start plays with the default offsets.
                    Stage::NewDevice { .. } if is_start => <Self as Component>::update(
                        self,
                        ctx,
                        Msg::DeviceContinue {
                            copy_audio_from: None,
                            copy_display_from: None,
                        },
                    ),
                    Stage::Finished { .. } | Stage::Interrupted if is_start => {
                        <Self as Component>::update(self, ctx, Msg::Retry)
                    }
                    _ if is_back => <Self as Component>::update(self, ctx, Msg::BackRequested),
                    _ => false,
                }
            }
            Msg::TouchSeen => {
                let changed = !self.touch;
                self.touch = true;
                changed
            }
            Msg::NeedsGesture => {
                self.pad_start_ctx = None;
                self.stage = Stage::Idle;
                self.needs_gesture = true;
                true
            }
            Msg::Decoded(Ok((audio, buffer))) => {
                self.pad_start_ctx = None;
                // Probe the devices now that the context is running.
                let refresh = self
                    .game
                    .borrow()
                    .as_ref()
                    .map(|g| g.refresh_hz())
                    .unwrap_or(60.0);
                let profile = DeviceProfile {
                    audio: audio.fingerprint(),
                    display: crate::web::display::fingerprint(refresh),
                };
                self.settings = Settings::load();
                let new_audio = self.settings.audio_profile(&profile.audio.id).is_none();
                let new_display = self.settings.display_profile(&profile.display.id).is_none();
                self.devices = Some(profile);
                let is_calibration = matches!(ctx.props().source, SongSource::Calibration(_));
                if (new_audio || new_display) && !is_calibration && !ctx.props().auto {
                    self.stage = Stage::NewDevice {
                        audio,
                        buffer,
                        new_audio,
                        new_display,
                    };
                    return true;
                }
                self.register_devices(None, None);
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
                self.pad_start_ctx = None;
                self.stage = Stage::Error(e);
                true
            }
            Msg::Session(SessionEvent::Finished {
                results,
                calibration,
                device_changed,
            }) => {
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
                    results: *results,
                    calibration,
                    device_changed,
                    video_stopped,
                    video_trouble,
                };
                if matches!(ctx.props().source, SongSource::Song { .. }) {
                    self.show_results(ctx);
                }
                true
            }
            Msg::Session(SessionEvent::Aborted) => {
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
                self.stage = Stage::Interrupted;
                if let Some(game) = self.game.borrow_mut().as_mut() {
                    game.clear_session();
                }
                true
            }
            Msg::Retry => {
                if self.device_lost {
                    ctx.props().restart.emit(());
                    return false;
                }
                let had_results = self.close_results();
                if let Some(game) = self.game.borrow_mut().as_mut() {
                    game.clear_session();
                    game.scene_mode();
                }
                if had_results {
                    // The play's frames again (the results drew their own).
                    schedule(self.raf.clone(), self.game.clone());
                    self.retried_at = js_sys::Date::now();
                }
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

    fn view(&self, ctx: &Context<Self>) -> Html {
        let link = ctx.link();
        let source = &ctx.props().source;
        let overlay = match (&self.gfx, &self.song, &self.stage) {
            (Status::Failed(e), _, _) => {
                html! { <div class="canvas-overlay error">{ crate::components::ui_test::no_gpu(e) }</div> }
            }
            (_, Status::Failed(e), _) => {
                html! { <div class="canvas-overlay error">{ format!("song failed to load: {e}") }</div> }
            }
            (_, Status::Pending, _) if self.access.is_some() => {
                let (handle, name, refused) = self.access.clone().expect("checked");
                // The ask happens in the click itself (a user gesture).
                let onclick = {
                    let link = link.clone();
                    Callback::from(move |_: MouseEvent| {
                        let answer = crate::web::folders::access(&handle);
                        link.send_future(async move { Msg::Access(answer.await) });
                    })
                };
                html! {
                    <div class="canvas-overlay access-prompt">
                        <div>{ format!("This song plays from the folder “{name}”, which the browser asks about again.") }</div>
                        <button class="button" {onclick} autofocus=true>{ "allow reading the folder" }</button>
                        { for refused.map(|r| html! { <div class="error">{ r }</div> }) }
                    </div>
                }
            }
            (Status::Pending, _, _) | (_, Status::Pending, _) => {
                html! { <div class="canvas-overlay muted">{ "loading…" }</div> }
            }
            (Status::Ready(_), Status::Ready(song), Stage::Idle) => html! {
                <div class="canvas-overlay start-prompt" onclick={link.callback(|_| Msg::StartRequested)}>
                    <div class="start-title">{ &song.title }</div>
                    <div class="muted">{ &song.subtitle }</div>
                    if self.needs_gesture {
                        <div class="start-hint">{ "This browser needs a tap, click or key press to start the audio." }</div>
                    }
                    if let Some(line) = &self.video_line {
                        <div class="muted">{ line }</div>
                    }
                    if self.touch {
                        <div class="start-hint">{ "tap to start · during play, hold or double-tap the ✕ button to quit" }</div>
                    } else {
                        <div class="start-hint">{ "press Enter (or Start on a controller) or click to start · Esc goes back · during play, hold or double-tap Esc (or Back) to quit" }</div>
                    }
                </div>
            },
            (
                _,
                _,
                Stage::NewDevice {
                    new_audio,
                    new_display,
                    ..
                },
            ) => self.new_device_view(ctx, *new_audio, *new_display),
            (_, _, Stage::Decoding) => {
                html! { <div class="canvas-overlay muted">{ "preparing audio…" }</div> }
            }
            (_, _, Stage::Playing) => html! {},
            (_, _, Stage::Aborted) => html! {},
            (_, _, Stage::Interrupted) => html! {
                <div class="canvas-overlay results">
                    <div class="results-card">
                        <div class="results-score">{ "interrupted" }</div>
                        <p class="muted">{ "The tab was hidden or audio was suspended." }</p>
                        <div class="results-actions">
                            <button onclick={link.callback(|_| Msg::Retry)}>{ "retry" }</button>
                            <a href={back_route(source).to_hash()}>{ "back" }</a>
                        </div>
                    </div>
                </div>
            },
            (_, _, Stage::Error(e)) => html! { <div class="canvas-overlay error">{ e }</div> },
            (
                _,
                _,
                Stage::Finished {
                    results,
                    calibration,
                    device_changed,
                    video_stopped,
                    video_trouble,
                },
            ) => match (source, calibration) {
                (SongSource::Calibration(mode), Some(o)) => calibration_view(
                    o,
                    *mode,
                    &self.settings,
                    self.devices.as_ref(),
                    self.refresh_hz(),
                    false,
                    *device_changed,
                    link,
                ),
                // The theme's results on the canvas; a plain card only
                // should it not start.
                (SongSource::Song { .. }, _) if self.results.is_some() => html! {},
                (SongSource::Song { .. }, _) => html! {
                    <div class="canvas-overlay results">
                        <div class="results-card">
                            <div class="results-score">{ score_text(results) }</div>
                            <div class="results-actions">
                                <button onclick={link.callback(|_| Msg::Retry)}>{ "retry" }</button>
                                <a href={Route::Home.to_hash()}>{ "song list" }</a>
                            </div>
                        </div>
                    </div>
                },
                (SongSource::Calibration(_), None) => {
                    let _ = (video_stopped, video_trouble);
                    results_view(
                        results,
                        self.names.as_ref(),
                        self.layout.as_ref(),
                        *device_changed,
                        link,
                    )
                }
            },
            (_, _, Stage::Saved(o)) => match source {
                SongSource::Calibration(mode) => calibration_view(
                    o,
                    *mode,
                    &self.settings,
                    self.devices.as_ref(),
                    self.refresh_hz(),
                    true,
                    false,
                    link,
                ),
                SongSource::Song { .. } => html! {},
            },
        };
        let debug = if self.settings.debug {
            html! { <pre class="debug-hud" ref={self.debug.clone()}></pre> }
        } else {
            html! {}
        };
        let playing = matches!(self.stage, Stage::Playing);
        let touch = self.touch;
        let onpointerdown = link.batch_callback(move |e: PointerEvent| {
            (!touch && matches!(e.pointer_type().as_str(), "touch" | "pen"))
                .then_some(Msg::TouchSeen)
        });
        html! {
            <div class={classes!("game-canvas-host", playing.then_some("playing"))} {onpointerdown}>
                <div class="game-canvas-slot" ref={self.slot.clone()}></div>
                <div class="lyrics" ref={self.lyrics.clone()} aria-hidden="true"></div>
                { overlay }
                { debug }
                if playing && touch {
                    // The session's touch source handles it (see `web::touch`).
                    <button class="touch-quit" aria-label="quit: hold or double-tap" title="hold or double-tap to quit">{ "✕" }</button>
                } else if self.results.is_some() {
                    // The results have their own way back.
                } else {
                    <a class="back-link" href={back_route(source).to_hash()}>{ "← back" }</a>
                }
            </div>
        }
    }

    fn destroy(&mut self, _ctx: &Context<Self>) {
        self.close_results();
        release_play_background();
        for (_, image) in self.backgrounds.drain(..) {
            image.close();
        }
        if let Some(p) = self.noteskin.take() {
            p.close();
        }
        self.raf.borrow_mut().take();
        self.game.borrow_mut().take();
        self.attachment = None;
    }
}

impl GameCanvas {
    /// Creates the audio context (synchronously: this runs inside the key,
    /// click or pad press that asked for it) and decodes the song.
    fn start(&mut self, ctx: &Context<Self>, from_pad: bool) -> bool {
        if !matches!(self.stage, Stage::Idle) {
            return false;
        }
        let (Status::Ready(song), Status::Ready(_)) = (&self.song, &self.gfx) else {
            return false;
        };
        let audio = match WebAudio::new() {
            Ok(a) => a,
            Err(e) => {
                self.stage = Stage::Error(e);
                return true;
            }
        };
        self.stage = Stage::Decoding;
        let audio_ctx = audio.context().clone();
        let song = song.clone();
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
                Some(bytes) => audio.decode(bytes).await,
                None => calibration::click_track(&audio, &song.song)
                    .ok_or_else(|| "could not synthesize the click track".to_string()),
            };
            Msg::Decoded(r.map(|b| (audio, b)))
        });
        self.needs_gesture = false;
        self.pad_start_ctx = from_pad.then(|| audio_ctx.clone());
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
        self.settings = settings;
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
        let (chart, mut config) = match &ctx.props().source {
            SongSource::Song { chart, .. } => (
                *chart,
                SessionConfig::from_settings(
                    &self.settings,
                    ctx.props().auto,
                    self.devices.clone(),
                ),
            ),
            SongSource::Calibration(mode) => {
                let mut c = SessionConfig::from_settings(
                    &self.settings,
                    ctx.props().auto,
                    self.devices.clone(),
                );
                c.ruleset = ddi_engine::rules::presets::calibration();
                // The test measures the plain chart: no turn, no removed
                // notes, every note drawn at a steady speed.
                c.options.transform = Default::default();
                c.options.appearance = Default::default();
                c.options.scroll.scroll_action = Default::default();
                c.render.background = 0.0;
                c.render.field_filter = 0.0;
                c.render.hide_notes = mode.hide_notes();
                c.render.show_deltas = true;
                c.calibration = Some(*mode);
                if mode.muted() {
                    c.volume = 0.0;
                }
                (0, c)
            }
        };
        config.auto_bias = ctx.props().auto_bias;
        config.auto_pad = ctx.props().auto_pad;
        config.gamepads = self.gamepads.clone();
        config.touch_canvas = crate::web::screen::canvas();
        let shift = match &ctx.props().source {
            SongSource::Song { id, .. } => self.settings.song_offset(id),
            SongSource::Calibration(_) => 0.0,
        };
        self.played_offset = shift;
        let shifted;
        let play_song = if shift == 0.0 {
            &song.song
        } else {
            let mut s = song.song.clone();
            s.shift_notes(shift);
            shifted = s;
            &shifted
        };
        // On the song's timing as played, so the song offset moves them too.
        let schedule = match &ctx.props().source {
            SongSource::Song { .. } => backgrounds::schedule(
                play_song,
                &song.bg_images,
                &song.bg_videos,
                song.has_background,
            ),
            SongSource::Calibration(_) => Vec::new(),
        };
        match PlaySession::start(play_song, chart, &self.settings, config, audio, buffer) {
            Ok(session) => {
                self.layout = Some(session.layout.clone());
                // The ruleset is chosen per chart when the session starts.
                self.names = Some(session.names.clone());
                self.rules = Some(session.rules.fresh());
                self.song_length = session.length;
                let lyrics = self
                    .settings
                    .lyrics
                    .then(|| {
                        let single_row = session.layout.lanes.iter().all(|l| l.row == 0);
                        LyricTrack::new(play_song, chart, self.settings.reverse, single_row)
                    })
                    .filter(|t| !t.is_empty())
                    .zip(self.lyrics.cast::<HtmlElement>())
                    .map(|(track, el)| LyricsView::new(track, el));
                if let Some(game) = self.game.borrow_mut().as_mut() {
                    game.set_video(
                        self.settings.video,
                        self.settings.background_frame,
                        ctx.props().slow,
                    );
                    game.set_lyrics(lyrics);
                    // A work's back and mask layers, with the backgrounds.
                    game.set_stage_track(
                        (self.settings.bg_brightness > 0.0)
                            .then(|| SpriteTrack::new(play_song, chart, self.settings.reverse)),
                    );
                    game.set_background_schedule(schedule);
                    game.set_session(session);
                }
                self.stage = Stage::Playing;
            }
            Err(e) => self.stage = Stage::Error(e),
        }
    }

    fn new_device_view(&self, ctx: &Context<Self>, new_audio: bool, new_display: bool) -> Html {
        let link = ctx.link();
        let Some(d) = &self.devices else {
            return html! {};
        };
        let s = &self.settings;
        let row = |what: &str,
                   fp: &ddi_platform::DeviceFingerprint,
                   is_new: bool,
                   known: Vec<(String, String, f64)>,
                   kind: &'static str|
         -> Html {
            if !is_new {
                return html! { <li>{ format!("{what}: {} (known)", fp.label) }</li> };
            }
            html! {
                <li>
                    { format!("new {what}: {}", fp.label) }
                    { if known.is_empty() { html!{} } else { html! {
                        <ul class="copy-list">
                            { for known.into_iter().map(|(id, label, off)| {
                                let onclick = if kind == "audio" {
                                    link.callback(move |_| Msg::DeviceContinue { copy_audio_from: Some(id.clone()), copy_display_from: None })
                                } else {
                                    link.callback(move |_| Msg::DeviceContinue { copy_audio_from: None, copy_display_from: Some(id.clone()) })
                                };
                                html!{ <li><button class="link-button" {onclick}>{ format!("use {:+.0} ms from {label}", off * 1000.0) }</button></li> }
                            }) }
                        </ul>
                    } } }
                </li>
            }
        };
        let known_audio: Vec<_> = s
            .audio_profiles
            .iter()
            .filter(|p| p.calibrated)
            .map(|p| (p.id.clone(), p.label.clone(), p.offset))
            .collect();
        let known_display: Vec<_> = s
            .display_profiles
            .iter()
            .filter(|p| p.calibrated)
            .map(|p| (p.id.clone(), p.label.clone(), p.offset))
            .collect();
        html! {
            <div class="canvas-overlay results">
                <div class="results-card">
                    <div class="results-score">{ "new device" }</div>
                    <p class="muted">{ "Offsets are kept per audio path and per display. These have not been calibrated yet:" }</p>
                    <ul class="device-list">
                        { row("audio path", &d.audio, new_audio, known_audio, "audio") }
                        { row("display", &d.display, new_display, known_display, "display") }
                    </ul>
                    <div class="results-actions">
                        <button onclick={link.callback(|_| Msg::DeviceCalibrate)}>{ "calibrate now" }</button>
                        <button onclick={link.callback(|_| Msg::DeviceContinue { copy_audio_from: None, copy_display_from: None })}>{ "play with defaults" }</button>
                    </div>
                </div>
            </div>
        }
    }
}

impl GameCanvas {
    /// The theme's results for the song just played, on the canvas
    /// (`ddi_ui::screens::results`): the play's frames stop, the screen
    /// takes the canvas, the keys and the pads until Retry or Song list.
    fn show_results(&mut self, ctx: &Context<Self>) {
        use ddi_ui::screens::data::SongEntry;
        use ddi_ui::screens::play::{PlayInfo, results_data};
        use ddi_ui::screens::results::{
            ActionFn, OffsetFn, PanelsFn, ResultsActions, ResultsScreen,
        };

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
            Some(c) => format!(
                "{} {} · {}",
                ddi_ui::screens::data::difficulty_name(c.difficulty),
                c.meter,
                layout.name
            ),
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
        };
        let mut video_notes = Vec::new();
        if let Some(reason) = video_stopped {
            let why = match reason {
                ddi_platform::video::StopReason::DroppedFrames => "frames were being dropped",
                ddi_platform::video::StopReason::DecoderBehind => "the decoder could not keep up",
            };
            video_notes.push(format!(
                "Video was turned off for this song: {why} (Options)."
            ));
        }
        if let Some(why) = video_trouble {
            video_notes.push(format!("{why}; the song background showed."));
        }
        let data = results_data(&PlayInfo {
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
        let offset: OffsetFn = {
            let id = id.clone();
            Rc::new(move |seconds| {
                let mut settings = Settings::load();
                settings.set_song_offset(&id, seconds);
                settings.save();
            })
        };
        let panels: PanelsFn = Rc::new(|name| {
            let mut settings = Settings::load();
            settings.results_panels = name.to_string();
            settings.save();
        });
        let theme = crate::components::ui_dev::theme_host(Rc::new(crate::components::ui_dev::nfkd));
        let env = theme.env().clone();
        let mut ui = ddi_ui::Ui::new(ddi_ui::screens::DESIGN, ddi_ui::TextMode::Mirror, true);
        ui.set_focus_ring(false);
        ui.push(ResultsScreen::new(
            env.clone(),
            data.clone(),
            ResultsActions {
                retry: Some(retry),
                song_list: Some(song_list),
                offset: Some(offset),
                panels: Some(panels),
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
                schedule(self.raf.clone(), self.game.clone());
            }
        }
    }

    /// Takes the results' screen down (and what it said in
    /// `__DDI_DEBUG`); whether it was up.
    fn close_results(&mut self) -> bool {
        self.results_host.borrow_mut().take();
        let was = self.results.take().is_some();
        if was
            && let Some(d) = web_sys::window()
                .and_then(|w| js_sys::Reflect::get(&w, &"__DDI_DEBUG".into()).ok())
                .filter(|d| d.is_object())
        {
            let d = js_sys::Object::from(d);
            let _ = js_sys::Reflect::delete_property(&d, &"results".into());
            let _ = js_sys::Reflect::delete_property(&d, &"ui".into());
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

/// A calibration play's summary when it gave no outcome, as the old
/// results card showed a play (songs show the theme's results,
/// `ddi_ui::screens::results`; calibration's move with step 6.4). A
/// calibration plays no background videos, so it has no video notes.
fn results_view(
    r: &Results,
    names: Option<&ddi_engine::rules::JudgeNames>,
    layout: Option<&ddi_chart::Layout>,
    device_changed: bool,
    link: &html::Scope<GameCanvas>,
) -> Html {
    let tier_name = |i: usize| -> String {
        names
            .map(|n| n.tiers[i].clone())
            .unwrap_or_else(|| format!("tier {}", i + 1))
    };
    let held_name = names
        .map(|n| format!("{} / {}", n.held, n.let_go))
        .unwrap_or_else(|| "held / let go".into());
    let mods = mods_summary(
        &r.transform,
        r.appearance,
        r.scroll_action,
        layout.map(|l| (r.lane_map.as_slice(), l)),
    )
    .join(" · ");
    let ex = match (r.score.ex, r.score.max_ex) {
        (Some(ex), Some(max)) => format!("EX {ex} / {max}"),
        _ => String::new(),
    };
    let lamp = ddi_ui::screens::play::lamp(r).unwrap_or_default();
    let t = &r.tally;
    html! {
        <div class="canvas-overlay results">
            <div class="results-card">
                <div class="results-grade">{ &r.grade }</div>
                <div class="results-score">{ score_text(r) }</div>
                <div class="muted">{ ex }</div>
                { if mods.is_empty() { html!{} } else { html!{ <div class="muted">{ mods }</div> } } }
                { if r.failed { html!{ <div class="error">{ "FAILED" }</div> } } else { html!{} } }
                { if device_changed { html!{ <div class="muted">{ "an audio device or the display changed during play" }</div> } } else { html!{} } }
                { if lamp.is_empty() { html!{} } else { html!{ <div class="results-fc">{ lamp }</div> } } }
                <table class="results-table">
                    { for (0..6).filter(|i| !tier_name(*i).is_empty()).map(|i| html!{ <tr><td class="muted">{ tier_name(i) }</td><td>{ t.taps[i] }</td></tr> }) }
                    <tr><td class="muted">{ held_name }</td><td>{ format!("{} / {}", t.held, t.let_go) }</td></tr>
                    { if t.checkpoint_hit + t.checkpoint_miss > 0 { html!{ <tr><td class="muted">{ "hold ticks hit / missed" }</td><td>{ format!("{} / {}", t.checkpoint_hit, t.checkpoint_miss) }</td></tr> } } else { html!{} } }
                    <tr><td class="muted">{ "mines hit" }</td><td>{ t.mine_hit }</td></tr>
                    { if t.boo > 0 { html!{ <tr><td class="muted">{ "stray presses" }</td><td>{ t.boo }</td></tr> } } else { html!{} } }
                    <tr><td class="muted">{ "max combo" }</td><td>{ r.max_combo }</td></tr>
                    <tr><td class="muted">{ "fast / slow" }</td><td>{ format!("{} / {}", r.fast, r.slow) }</td></tr>
                    <tr><td class="muted">{ "mean offset" }</td><td>{ format!("{:+.1} ms (σ {:.1})", r.mean_delta * 1000.0, r.stddev_delta * 1000.0) }</td></tr>
                </table>
                <div class="results-actions">
                    <button onclick={link.callback(|_| Msg::Retry)}>{ "retry" }</button>
                    <a href={Route::Home.to_hash()}>{ "song list" }</a>
                </div>
            </div>
        </div>
    }
}

fn score_text(r: &Results) -> String {
    ddi_ui::screens::play::score_text(r)
}

#[allow(clippy::too_many_arguments)]
fn calibration_view(
    o: &Outcome,
    mode: CalMode,
    settings: &Settings,
    devices: Option<&DeviceProfile>,
    refresh_hz: f64,
    saved: bool,
    device_changed: bool,
    link: &html::Scope<GameCanvas>,
) -> Html {
    let total_ms = o.total() * 1000.0;
    let frames = if mode == CalMode::Visual {
        format!(
            " ≈ {:.2} frames at {refresh_hz:.0} Hz",
            o.total() * refresh_hz
        )
    } else {
        String::new()
    };
    let (audio_now, visual_now) = match devices {
        Some(d) => (
            settings
                .audio_profile(&d.audio.id)
                .map(|p| p.offset)
                .unwrap_or(settings.audio_offset),
            settings
                .display_profile(&d.display.id)
                .map(|p| p.offset)
                .unwrap_or(settings.visual_offset),
        ),
        None => (settings.audio_offset, settings.visual_offset),
    };
    let device_line = match devices {
        Some(d) => format!("audio: {} · display: {}", d.audio.label, d.display.label),
        None => String::new(),
    };
    let ci_ms = o.confidence() * 1000.0;
    let enough = o.hits >= calibration::MIN_RESIDUAL as u32;
    let unusable = o.out_of_range || o.unreliable();
    let meaningful = enough
        && !unusable
        && o.total().abs() >= calibration::MIN_STEP
        && (o.rounds > 0 || o.residual_significant());
    let status = if o.out_of_range {
        format!(
            "The measured offset is beyond ±{:.0} ms, which this test cannot tell apart from hitting the neighbouring note. If you really hear the clicks that late (some Bluetooth headphones do), set the audio offset by hand in the options and run the test again from there.",
            calibration::MAX_OFFSET * 1000.0
        )
    } else if o.high_miss_rate() {
        format!(
            "{} of {} notes were missed, so the result is unreliable. Presses more than {:.0} ms off do not count, and a press stream one note late misses after every rest. If you hear the clicks later than that, set the offset by hand first; otherwise try again and hit every note.",
            o.misses,
            o.hits + o.misses,
            calibration::MAX_OFFSET * 1000.0
        )
    } else if !enough {
        "Too few hits to measure; try again and hit every note.".to_string()
    } else if o.unreliable() {
        format!(
            "Spread is too high (σ {:.0} ms) to trust the mean. Try again, keeping a steady rhythm.",
            o.residual_sd * 1000.0
        )
    } else if o.converged {
        format!(
            "Converged after {} adjustment{}: the remaining error ({:+.1} ms ± {:.1}) is within noise.",
            o.rounds,
            if o.rounds == 1 { "" } else { "s" },
            o.residual_mean * 1000.0,
            ci_ms
        )
    } else if o.residual_significant() {
        format!(
            "Ran out of notes before converging; the remaining error ({:+.1} ms ± {:.1}) is still significant. Try again, keeping a steady rhythm.",
            o.residual_mean * 1000.0,
            ci_ms
        )
    } else {
        "Finished; the remaining error is within noise.".to_string()
    };
    let verdict = if !meaningful {
        "Nothing to change.".to_string()
    } else {
        format!(
            "Saving adds {:+.1} ms to the {}, so these hits count as on time.",
            total_ms,
            mode.target()
        )
    };
    // The last step of the guided flow ends it: back to the songs. Earlier
    // steps leave the flow, so they return to the calibration page.
    let done = if mode.next().is_none() {
        Route::Home
    } else {
        Route::Calibrate
    };
    let next = mode.next().map(|m| {
        let label = match m {
            CalMode::Audio => "next: sound only",
            CalMode::Combined => "next: check with both",
            CalMode::Visual => "next",
        };
        html! { <a class="button" href={Route::CalibrateRun { mode: m.id().into(), auto: false, bias_ms: 0 }.to_hash()}>{ label }</a> }
    });
    html! {
        <div class="canvas-overlay results">
            <div class="results-card">
                <div class="results-score">{ if unusable { "unreliable".to_string() } else { format!("{total_ms:+.1} ms{frames}") } }</div>
                <div class="muted">{ format!("{} hits ({} warm-up/adaptation ignored, {} extremes trimmed) · {} missed · residual σ {:.1} ms · 95% CI ±{:.1} ms", o.hits, o.ignored, o.trimmed, o.misses, o.residual_sd * 1000.0, ci_ms) }</div>
                <p class="cal-verdict">{ status }{ " " }{ verdict }</p>
                <p class="muted">{ format!("offsets in effect: audio {:+.0} ms · visual {:+.0} ms", audio_now * 1000.0, visual_now * 1000.0) }</p>
                { if device_line.is_empty() { html!{} } else { html!{ <p class="muted device-line">{ device_line }</p> } } }
                { if device_changed { html!{ <p class="error">{ "a device changed during the test; discard this result" }</p> } } else { html!{} } }
                <div class="results-actions">
                    { if saved {
                        html!{ <span class="results-fc">{ "saved" }</span> }
                    } else if meaningful {
                        html!{ <button onclick={link.callback(|_| Msg::SaveCalibration)}>{ format!("save to {}", mode.target()) }</button> }
                    } else { html!{} } }
                    <button onclick={link.callback(|_| Msg::Retry)}>{ "again" }</button>
                    { if saved || !meaningful { next.unwrap_or_default() } else { html!{} } }
                    <a href={done.to_hash()}>{ "done" }</a>
                </div>
            </div>
        </div>
    }
}

/// Whether the primary pointer is a finger (phones, tablets).
fn coarse_pointer() -> bool {
    web_sys::window()
        .and_then(|w| w.match_media("(pointer: coarse)").ok().flatten())
        .is_some_and(|q| q.matches())
}

fn schedule(raf: Rc<RefCell<Option<AnimationFrame>>>, game: Rc<RefCell<Option<GameLoop>>>) {
    let raf2 = raf.clone();
    let game2 = game.clone();
    let handle = request_animation_frame(move |time_ms| {
        if let Some(g) = game2.borrow_mut().as_mut() {
            g.frame(time_ms);
        } else {
            return;
        }
        schedule(raf2, game2);
    });
    *raf.borrow_mut() = Some(handle);
}

/// Longest side of a back or mask image kept (they show at the stage's
/// size, about the screen's height at most).
const MAX_LAYER_SIDE: u32 = 2048;

/// Texts of a work's layers drawn, at most.
const MAX_LAYER_TEXTS: usize = 64;

/// Pixels of a work's layers kept decoded, in all (each is kept twice: the
/// bitmap, for a skin's reinstall, and its texture).
const MAX_LAYER_PIXELS: u64 = 48 << 20;

/// How much larger than its stage size layer text is drawn, to stay
/// sharp on large screens.
const TEXT_SCALE: f64 = 3.0;

/// Loads the song and its background images, and readies the chosen
/// noteskin for the chart's layout; the song arrives as [`Msg::SongLoaded`].
fn load(ctx: &Context<GameCanvas>, id: String, chart: usize) {
    let link = ctx.link().clone();
    let background_name = new_play_background(&id);
    let settings = Settings::load();
    let show_background = settings.bg_brightness > 0.0;
    // Off: no movie file is read and the decoder is never fetched.
    let play_videos = show_background && settings.video != VideoMode::Off;
    ctx.link().send_future(async move {
        let r = load_song(&id).await.map(|loaded| {
            if show_background {
                // Loads alongside the start prompt; the play
                // never waits for it.
                let shared = Rc::new(loaded.shared_bg.clone());
                let images = std::iter::once(BgImage::Song).chain(
                    loaded
                        .entry
                        .bg_images
                        .iter()
                        .chain(loaded.shared_bg.iter().map(|(name, _)| name))
                        .cloned()
                        .map(BgImage::File),
                );
                // A work's back and mask layers: their images (those the
                // import kept) and text, under both scroll directions'
                // data, within the budgets.
                let kept: std::collections::HashSet<&str> = loaded
                    .entry
                    .layer_images
                    .iter()
                    .map(String::as_str)
                    .collect();
                let mut sources = SpriteTrack::new(&loaded.song, chart, false).sources();
                for s in SpriteTrack::new(&loaded.song, chart, true).sources() {
                    if !sources.contains(&s) {
                        sources.push(s);
                    }
                }
                let mut texts = 0;
                sources.retain(|s| match s {
                    SpriteSource::Image(name) => kept.contains(&**name),
                    SpriteSource::Text { .. } => {
                        texts += 1;
                        texts <= MAX_LAYER_TEXTS
                    }
                });
                for source in sources {
                    let entry = loaded.entry.clone();
                    link.send_future(async move {
                        let content = match &source {
                            SpriteSource::Image(name) => match load_layer_image(&entry, name).await
                            {
                                Ok(blob) => crate::web::image::decode_sized(&blob, MAX_LAYER_SIDE)
                                    .await
                                    .map(|(b, (w, h))| (b, (w as f32, h as f32))),
                                Err(e) => Err(e),
                            },
                            SpriteSource::Text { text, size, color } => {
                                crate::web::image::text_bitmap(text, *size, color, TEXT_SCALE).await
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
                    let shared = shared.clone();
                    let background_name = background_name.clone();
                    link.send_future(async move {
                        let bitmap = match load_background(&entry, &what, &shared).await {
                            Ok(Some(blob)) => {
                                // Kept for the results' backdrop (read
                                // once).
                                if what == BgImage::Song {
                                    crate::web::textures::keep(
                                        &background_name,
                                        Some(blob.clone()),
                                    );
                                }
                                let side = match what {
                                    BgImage::Song => MAX_BACKGROUND_SIDE,
                                    // Movies are not loaded here (only images
                                    // are listed above).
                                    BgImage::File(_) | BgImage::Movie(_) => MAX_CHANGE_SIDE,
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
                    .map(|v| (v.name.clone(), v.format.clone()))
                    .chain(
                        loaded
                            .shared_videos
                            .iter()
                            .map(|(name, _)| (name.clone(), unknown.clone())),
                    )
                    .filter(|_| play_videos)
                    .collect();
                // The movie the song shows first sets the background frame.
                let first = backgrounds::schedule(
                    &loaded.song,
                    &[] as &[String],
                    &movies.iter().map(|(n, _)| n.clone()).collect::<Vec<_>>(),
                    false,
                )
                .into_iter()
                .find_map(|s| match s.image {
                    BgImage::Movie(name) => Some(name),
                    _ => None,
                });
                for (name, format) in movies {
                    let probe = first.as_ref() == Some(&name);
                    let entry = loaded.entry.clone();
                    let shared = shared_videos.clone();
                    link.send_future_batch(async move {
                        let what = BgImage::Movie(name.clone());
                        match load_background(&entry, &what, &shared).await {
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
                // The song's own images, then those found in
                // shared folders, under the names it uses.
                bg_images: loaded
                    .entry
                    .bg_images
                    .iter()
                    .chain(loaded.shared_bg.iter().map(|(name, _)| name))
                    .cloned()
                    .collect(),
                // Until movies are drawn (docs/plans/video-backgrounds.md,
                // step 4) a movie segment shows the song background.
                bg_videos: loaded
                    .entry
                    .bg_videos
                    .iter()
                    .filter(|v| v.plays())
                    .map(|v| v.name.clone())
                    .chain(loaded.shared_videos.iter().map(|(name, _)| name.clone()))
                    .collect(),
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
            && let Some(skin) = noteskin_ready(&settings, &loaded.song, chart)
        {
            link.send_future(async move { Msg::NoteskinLoaded(skin.await) });
        }
        Msg::SongLoaded(r)
    });
}

/// Readies the chosen noteskin for `song`'s chart `chart` (its layout,
/// colours and scroll direction), for [`Msg::NoteskinLoaded`]. `None` for
/// the built-in arrows.
fn noteskin_ready(
    settings: &Settings,
    song: &Song,
    chart: usize,
) -> Option<impl std::future::Future<Output = Option<Box<crate::noteskins::Prepared>>> + use<>> {
    let c = song.charts.get(chart)?;
    let key = crate::noteskins::play_skin(
        settings.noteskin.as_deref(),
        settings.note_colors,
        settings.chart_colors && c.notes.iter().any(|n| n.color.is_some()),
    )?;
    let layout = song.layout_of(c)?;
    let env = crate::noteskins::lua_env(song, c, &layout, settings.reverse);
    Some(async move {
        crate::noteskins::prepare(&key, &layout, env)
            .await
            .map(Box::new)
    })
}
