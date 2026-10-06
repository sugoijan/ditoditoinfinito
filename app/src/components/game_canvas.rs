//! Gameplay screen: loads the song (or generates the calibration one), waits
//! for a user gesture (the audio context must be created inside one), starts
//! the play session and shows the results when it ends.

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
use crate::play::{PlaySession, SessionConfig, SessionEvent};
use crate::router::Route;
use crate::settings::Settings;
use crate::songs::{load_manifest, load_song};
use crate::web::audio::WebAudio;
use crate::web::gfx::{BackendPreference, Gfx};

/// What the screen plays.
#[derive(Clone, Debug, PartialEq)]
pub(crate) enum SongSource {
    /// A bundled song by manifest id and chart index.
    Bundled { id: String, chart: usize },
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
}

pub(crate) enum Msg {
    GfxReady(Result<Box<GameLoop>, String>),
    /// Escape/Backspace outside of play: leave the screen.
    BackRequested,
    SongLoaded(Result<Rc<Loaded>, String>),
    StartRequested,
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
}

#[derive(Properties, PartialEq)]
pub(crate) struct Props {
    pub(crate) source: SongSource,
    #[prop_or_default]
    pub(crate) backend: BackendPreference,
    #[prop_or_default]
    pub(crate) auto: bool,
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
    settings: Settings,
    devices: Option<DeviceProfile>,
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
            SongSource::Bundled { id, chart: _ } => {
                ctx.link().send_future(async move {
                    let r = async {
                        let manifest = load_manifest().await?;
                        let entry = manifest
                            .songs
                            .iter()
                            .find(|s| s.id == id)
                            .cloned()
                            .ok_or_else(|| format!("unknown song `{id}`"))?;
                        let loaded = load_song(&entry).await?;
                        Ok(Rc::new(Loaded {
                            title: loaded.entry.title.clone(),
                            subtitle: loaded.entry.artist.clone(),
                            song: loaded.song,
                            music_bytes: Some(loaded.music_bytes),
                        }))
                    }
                    .await;
                    Msg::SongLoaded(r)
                });
            }
            SongSource::Calibration(mode) => {
                let loaded = Loaded {
                    song: calibration::song(mode),
                    title: mode.title().into(),
                    subtitle: mode.hint().into(),
                    music_bytes: None,
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
        GameCanvas {
            canvas: NodeRef::default(),
            debug: NodeRef::default(),
            gfx: Status::Pending,
            song: Status::Pending,
            stage: Stage::Idle,
            raf: Rc::new(RefCell::new(None)),
            game: Rc::new(RefCell::new(None)),
            names: None,
            settings: Settings::load(),
            devices: None,
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
                if !matches!(self.stage, Stage::Idle) {
                    return false;
                }
                let (Status::Ready(song), Status::Ready(_)) = (&self.song, &self.gfx) else {
                    return false;
                };
                // Must be created synchronously inside the gesture handler.
                let audio = match WebAudio::new() {
                    Ok(a) => a,
                    Err(e) => {
                        self.stage = Stage::Error(e);
                        return true;
                    }
                };
                self.stage = Stage::Decoding;
                let song = song.clone();
                ctx.link().send_future(async move {
                    audio.resume().await;
                    audio.wait_for_latency().await;
                    let r = match &song.music_bytes {
                        Some(bytes) => audio.decode(bytes).await,
                        None => calibration::click_track(&audio, &song.song)
                            .ok_or_else(|| "could not synthesize the click track".to_string()),
                    };
                    Msg::Decoded(r.map(|b| (audio, b)))
                });
                true
            }
            Msg::Decoded(Ok((audio, buffer))) => {
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
                    SongSource::Bundled { .. } => Route::Home.navigate(),
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
                    <div class="start-hint">{ "press Enter or click to start · Esc goes back · during play, hold or double-tap Esc to quit" }</div>
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
                _ => results_view(results, self.names.as_ref(), *device_changed, link),
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
                SongSource::Bundled { .. } => html! {},
            },
        };
        let debug = if self.settings.debug {
            html! { <pre class="debug-hud" ref={self.debug.clone()}></pre> }
        } else {
            html! {}
        };
        html! {
            <div class="game-canvas-host">
                <canvas ref={self.canvas.clone()} class="game-canvas"></canvas>
                { overlay }
                { debug }
                <a class="back-link" href={back_route(source).to_hash()}>{ "← back" }</a>
            </div>
        }
    }

    fn destroy(&mut self, _ctx: &Context<Self>) {
        self.raf.borrow_mut().take();
        self.game.borrow_mut().take();
    }
}

impl GameCanvas {
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

    fn begin_session(&mut self, ctx: &Context<Self>, audio: WebAudio, buffer: AudioBuffer) {
        let Status::Ready(song) = &self.song else {
            return;
        };
        let (chart, mut config) = match &ctx.props().source {
            SongSource::Bundled { chart, .. } => (
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
        self.names = Some(config.ruleset.names.clone());
        match PlaySession::start(&song.song, chart, &self.settings, config, audio, buffer) {
            Ok(session) => {
                if let Some(game) = self.game.borrow_mut().as_mut() {
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
        SongSource::Bundled { .. } => Route::Home,
        SongSource::Calibration(_) => Route::Calibrate,
    }
}

fn results_view(
    r: &Results,
    names: Option<&ddi_engine::rules::JudgeNames>,
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
    html! {
        <div class="canvas-overlay results">
            <div class="results-card">
                <div class="results-grade">{ &r.grade }</div>
                <div class="results-score">{ score }</div>
                <div class="muted">{ ex }</div>
                { if r.failed { html!{ <div class="error">{ "FAILED" }</div> } } else { html!{} } }
                { if device_changed { html!{ <div class="muted">{ "an audio device or the display changed during play" }</div> } } else { html!{} } }
                { if fc.is_empty() { html!{} } else { html!{ <div class="results-fc">{ fc }</div> } } }
                <table class="results-table">
                    { for (0..6).filter(|i| !tier_name(*i).is_empty()).map(|i| html!{ <tr><td class="muted">{ tier_name(i) }</td><td>{ r.tally.taps[i] }</td></tr> }) }
                    <tr><td class="muted">{ held_name }</td><td>{ format!("{} / {}", r.tally.held, r.tally.let_go) }</td></tr>
                    <tr><td class="muted">{ "mines hit" }</td><td>{ r.tally.mine_hit }</td></tr>
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
                    <a href={Route::Calibrate.to_hash()}>{ "done" }</a>
                </div>
            </div>
        </div>
    }
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
