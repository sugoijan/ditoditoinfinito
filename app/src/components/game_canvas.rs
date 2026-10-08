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
use std::rc::Rc;

use ddi_chart::Song;
use ddi_engine::player::Results;
use ddi_platform::DeviceProfile;
use gloo::events::{EventListener, EventListenerOptions};
use gloo::render::{AnimationFrame, request_animation_frame};
use wasm_bindgen::JsCast;
use wasm_bindgen_futures::spawn_local;
use web_sys::{AudioBuffer, HtmlCanvasElement, HtmlElement, KeyboardEvent};
use yew::prelude::*;

use crate::calibration::{self, CalMode, Outcome};
use crate::game_loop::GameLoop;
use crate::mods::mods_summary;
use crate::play::{PlaySession, SessionConfig, SessionEvent};
use crate::router::Route;
use crate::settings::Settings;
use crate::songs::{load_background, load_song};
use crate::web::audio::WebAudio;
use crate::web::gamepad::Gamepads;
use crate::web::gfx::{BackendPreference, Gfx};
use ddi_library::backgrounds::{self, BgImage};

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
    /// The song has a background image.
    pub(crate) has_background: bool,
}

pub(crate) enum Msg {
    GfxReady(Result<Box<GameLoop>, String>),
    /// Escape/Backspace outside of play: leave the screen.
    BackRequested,
    SongLoaded(Result<Rc<Loaded>, String>),
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
    /// On the results: set this song's offset (seconds, notes later).
    SetSongOffset(f64),
    /// A background image decoded: the song's background or a background
    /// change's (`None`: no such image, or it failed to load).
    BackgroundLoaded(BgImage, Option<web_sys::ImageBitmap>),
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
}

pub(crate) struct GameCanvas {
    canvas: NodeRef,
    debug: NodeRef,
    gfx: Status<&'static str>,
    song: Status<Rc<Loaded>>,
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
    _keys: Option<EventListener>,
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
            SongSource::Song { id, chart: _ } => {
                let link = ctx.link().clone();
                let show_background = Settings::load().bg_brightness > 0.0;
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
                            for what in images {
                                let entry = loaded.entry.clone();
                                let shared = shared.clone();
                                link.send_future(async move {
                                    let bitmap = match load_background(&entry, &what, &shared).await
                                    {
                                        Ok(Some(blob)) => {
                                            let side = match what {
                                                BgImage::Song => MAX_BACKGROUND_SIDE,
                                                BgImage::File(_) => MAX_CHANGE_SIDE,
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
                            has_background: loaded.entry.background.is_some(),
                            song: loaded.song,
                            music_bytes: Some(loaded.music_bytes),
                        })
                    });
                    Msg::SongLoaded(r)
                });
            }
            SongSource::Calibration(mode) => {
                let loaded = Loaded {
                    song: calibration::song(mode),
                    title: mode.title().into(),
                    subtitle: mode.hint().into(),
                    music_bytes: None,
                    bg_images: Vec::new(),
                    has_background: false,
                };
                ctx.link()
                    .send_message(Msg::SongLoaded(Ok(Rc::new(loaded))));
            }
        }
        let keys = web_sys::window().map(|w| {
            let link = ctx.link().clone();
            EventListener::new_with_options(
                &w,
                "keydown",
                EventListenerOptions::enable_prevent_default(),
                move |event| {
                    let Some(e) = event.dyn_ref::<KeyboardEvent>() else {
                        return;
                    };
                    if e.repeat() {
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
            g.set_listener(Some(Box::new(move |edge| {
                if let (ddi_platform::DeviceId::Gamepad(id), true) = (&edge.device, edge.pressed) {
                    link.send_message(Msg::PadPress(id.clone(), edge.control.clone()));
                }
            })));
        }
        GameCanvas {
            canvas: NodeRef::default(),
            debug: NodeRef::default(),
            gfx: Status::Pending,
            song: Status::Pending,
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
            _keys: keys,
        }
    }

    fn rendered(&mut self, ctx: &Context<Self>, first_render: bool) {
        if !first_render {
            // The debug element is re-created on re-render; keep the loop pointed at it.
            if let Some(game) = self.game.borrow_mut().as_mut() {
                game.set_debug_element(self.debug.cast::<HtmlElement>());
            }
            return;
        }
        let Some(canvas) = self.canvas.cast::<HtmlCanvasElement>() else {
            ctx.link()
                .send_message(Msg::GfxReady(Err("canvas element missing".into())));
            return;
        };
        let dpr = web_sys::window()
            .map(|w| w.device_pixel_ratio())
            .unwrap_or(1.0);
        let rect = canvas.get_bounding_client_rect();
        canvas.set_width((rect.width() * dpr).round().max(1.0) as u32);
        canvas.set_height((rect.height() * dpr).round().max(1.0) as u32);
        let link = ctx.link().clone();
        let on_event = ctx.link().callback(Msg::Session);
        let preference = ctx.props().backend;
        spawn_local(async move {
            let result = Gfx::new(canvas, preference)
                .await
                .map(|(gfx, canvas)| Box::new(GameLoop::new(gfx, canvas, on_event)));
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
            Msg::SongLoaded(Err(e)) => {
                self.song = Status::Failed(e);
                true
            }
            Msg::BackRequested => {
                // During play the session owns the cancel gesture.
                if matches!(self.stage, Stage::Playing | Stage::Decoding) {
                    return false;
                }
                if let Some(game) = self.game.borrow_mut().as_mut() {
                    game.clear_session();
                }
                back_route(&ctx.props().source).navigate();
                false
            }
            Msg::StartRequested => {
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
                self.stage = Stage::Finished {
                    results: *results,
                    calibration,
                    device_changed,
                };
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
                if let Some(game) = self.game.borrow_mut().as_mut() {
                    game.clear_session();
                }
                self.stage = Stage::Idle;
                true
            }
            Msg::SetSongOffset(seconds) => {
                let SongSource::Song { id, .. } = &ctx.props().source else {
                    return false;
                };
                let mut settings = Settings::load();
                settings.set_song_offset(id, seconds);
                settings.save();
                self.settings = settings;
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
                html! { <div class="canvas-overlay error">{ format!("renderer failed: {e}") }</div> }
            }
            (_, Status::Failed(e), _) => {
                html! { <div class="canvas-overlay error">{ format!("song failed to load: {e}") }</div> }
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
                (SongSource::Song { id, .. }, _) => results_view(
                    results,
                    self.names.as_ref(),
                    self.layout.as_ref(),
                    *device_changed,
                    Some((self.played_offset, self.settings.song_offset(id))),
                    link,
                ),
                (SongSource::Calibration(_), None) => results_view(
                    results,
                    self.names.as_ref(),
                    self.layout.as_ref(),
                    *device_changed,
                    None,
                    link,
                ),
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
                <canvas ref={self.canvas.clone()} class="game-canvas"></canvas>
                { overlay }
                { debug }
                if playing && touch {
                    // The session's touch source handles it (see `web::touch`).
                    <button class="touch-quit" aria-label="quit: hold or double-tap" title="hold or double-tap to quit">{ "✕" }</button>
                } else {
                    <a class="back-link" href={back_route(source).to_hash()}>{ "← back" }</a>
                }
            </div>
        }
    }

    fn destroy(&mut self, _ctx: &Context<Self>) {
        for (_, image) in self.backgrounds.drain(..) {
            image.close();
        }
        self.raf.borrow_mut().take();
        self.game.borrow_mut().take();
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
        config.touch_canvas = self.canvas.cast::<HtmlCanvasElement>();
        self.names = Some(config.ruleset.names.clone());
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
            SongSource::Song { .. } => {
                backgrounds::schedule(play_song, &song.bg_images, song.has_background)
            }
            SongSource::Calibration(_) => Vec::new(),
        };
        match PlaySession::start(play_song, chart, &self.settings, config, audio, buffer) {
            Ok(session) => {
                self.layout = Some(session.layout.clone());
                if let Some(game) = self.game.borrow_mut().as_mut() {
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

fn back_route(source: &SongSource) -> Route {
    match source {
        SongSource::Song { .. } => Route::Home,
        SongSource::Calibration(_) => Route::Calibrate,
    }
}

fn results_view(
    r: &Results,
    names: Option<&ddi_engine::rules::JudgeNames>,
    layout: Option<&ddi_chart::Layout>,
    device_changed: bool,
    song_offset: Option<(f64, f64)>,
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
    let score = match (r.score.money, r.score.percent) {
        (Some(m), _) => format!("{m}"),
        (None, Some(p)) => format!("{:.2}%", p * 100.0),
        _ => "—".into(),
    };
    let ex = match (r.score.ex, r.score.max_ex) {
        (Some(ex), Some(max)) => format!("EX {ex} / {max}"),
        _ => String::new(),
    };
    let fc = match r.full_combo {
        ddi_engine::rules::FullCombo::None => "",
        ddi_engine::rules::FullCombo::FC => "FULL COMBO",
        ddi_engine::rules::FullCombo::GreatFC => "GREAT FULL COMBO",
        ddi_engine::rules::FullCombo::PerfectFC => "PERFECT FULL COMBO",
        ddi_engine::rules::FullCombo::MarvelousFC => "MARVELOUS FULL COMBO",
    };
    // An assisted play (notes removed or simplified) says so with its lamp.
    let (fc, fc_class) = match (fc.is_empty(), r.assist) {
        (true, true) => ("ASSIST".to_string(), "results-fc results-assist"),
        (false, true) => (format!("{fc} · ASSIST"), "results-fc"),
        _ => (fc.to_string(), "results-fc"),
    };
    let mods = mods_summary(
        &r.transform,
        r.appearance,
        r.scroll_action,
        layout.map(|l| (r.lane_map.as_slice(), l)),
    )
    .join(" · ");
    html! {
        <div class="canvas-overlay results">
            <div class="results-card">
                <div class="results-grade">{ &r.grade }</div>
                <div class="results-score">{ score }</div>
                <div class="muted">{ ex }</div>
                { if mods.is_empty() { html!{} } else { html!{ <div class="muted">{ mods }</div> } } }
                { if r.failed { html!{ <div class="error">{ "FAILED" }</div> } } else { html!{} } }
                { if device_changed { html!{ <div class="muted">{ "an audio device or the display changed during play" }</div> } } else { html!{} } }
                { if fc.is_empty() { html!{} } else { html!{ <div class={fc_class}>{ fc }</div> } } }
                <table class="results-table">
                    { for (0..6).filter(|i| !tier_name(*i).is_empty()).map(|i| html!{ <tr><td class="muted">{ tier_name(i) }</td><td>{ r.tally.taps[i] }</td></tr> }) }
                    <tr><td class="muted">{ held_name }</td><td>{ format!("{} / {}", r.tally.held, r.tally.let_go) }</td></tr>
                    <tr><td class="muted">{ "mines hit" }</td><td>{ r.tally.mine_hit }</td></tr>
                    <tr><td class="muted">{ "max combo" }</td><td>{ r.max_combo }</td></tr>
                    <tr><td class="muted">{ "fast / slow" }</td><td>{ format!("{} / {}", r.fast, r.slow) }</td></tr>
                    <tr><td class="muted">{ "mean offset" }</td><td>{ format!("{:+.1} ms (σ {:.1})", r.mean_delta * 1000.0, r.stddev_delta * 1000.0) }</td></tr>
                </table>
                { for song_offset.map(|(played, current)| song_offset_editor(r, played, current, link)) }
                <div class="results-actions">
                    <button onclick={link.callback(|_| Msg::Retry)}>{ "retry" }</button>
                    <a href={Route::Home.to_hash()}>{ "song list" }</a>
                </div>
            </div>
        </div>
    }
}

/// Hits needed before the mean error is offered as this song's offset.
const MIN_HITS_FOR_SONG_OFFSET: u32 = 20;

/// Per-song offset on the results: nudge by a millisecond, adopt the mean
/// error of this play, or reset. Takes effect from the next play. The
/// suggestion builds on the offset the play ran with, not the current one,
/// so adopting it twice does not add the error twice.
fn song_offset_editor(
    r: &Results,
    played: f64,
    current: f64,
    link: &html::Scope<GameCanvas>,
) -> Html {
    let set = |v: f64| link.callback(move |_| Msg::SetSongOffset(v));
    let ms = |s: f64| (s * 1000.0).round() / 1000.0;
    let hits = r.fast + r.slow;
    let suggested = ms(played + r.mean_delta);
    let suggest = (hits >= MIN_HITS_FOR_SONG_OFFSET
        && (suggested - played).abs() >= 0.002
        && (suggested - current).abs() >= 0.001)
        .then(|| {
            html! {
                <button onclick={set(suggested)} title="Shift the notes by this play's mean error. If every song feels off, calibrate instead.">
                    { format!("use {:+.0} ms", suggested * 1000.0) }
                </button>
            }
        });
    html! {
        <div class="song-offset">
            <span class="muted">{ "song offset" }</span>
            <button class="small" onclick={set(ms(current - 0.001))} title="notes 1 ms earlier">{ "−1" }</button>
            <span class="song-offset-value">{ format!("{:+.0} ms", current * 1000.0) }</span>
            <button class="small" onclick={set(ms(current + 0.001))} title="notes 1 ms later">{ "+1" }</button>
            { for suggest }
            { if current != 0.0 { html!{ <button class="small" onclick={set(0.0)}>{ "reset" }</button> } } else { html!{} } }
        </div>
    }
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
