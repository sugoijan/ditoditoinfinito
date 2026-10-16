//! One play of one chart on the web: the shared session
//! (`ddi_session::play`, `docs/plans/desktop.md` step 7.1) on Web Audio,
//! with the page's inputs (keyboard, gamepads, touch lanes over the
//! canvas), its interruptions (the tab hidden, the audio context
//! suspended) and device changes, advanced once per animation frame.

use std::cell::Cell;
use std::ops::{Deref, DerefMut};
use std::rc::Rc;

use ddi_chart::{Layout, Song};
use ddi_engine::ConnectedPad;
use ddi_engine::judge::JudgeEvent;
use ddi_platform::{DeviceProfile, HostTime, InputSource, RawInput};
use ddi_render::RenderOptions;
use gloo::events::EventListener;
use wasm_bindgen::JsCast;
use web_sys::{AudioBuffer, HtmlCanvasElement};

pub(crate) use ddi_session::play::{PlayFacts, SessionEvent};

use crate::settings::Settings;
use crate::web::audio::WebAudio;
use crate::web::gamepad::Gamepads;
use crate::web::keyboard::WebKeyboard;
use crate::web::touch::TouchLanes;

/// Everything about a session that is not the song: the shared
/// session's configuration as it is, and what only the web has.
pub(crate) struct SessionConfig {
    pub(crate) session: ddi_session::play::SessionConfig,
    pub(crate) render: RenderOptions,
    /// Gamepad poller shared with the screen (start button), if supported.
    pub(crate) gamepads: Option<Gamepads>,
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
            session: ddi_session::play::SessionConfig::from_settings(settings, auto, devices),
            render: RenderOptions {
                reverse: settings.reverse,
                hide_notes: false,
                show_deltas: settings.show_deltas,
                cancel_progress: 0.0,
                cancel_hint: "",
                note_colors: crate::settings::note_scheme(settings.note_colors),
                chart_colors: settings.chart_colors,
                background: settings.bg_brightness,
                // Chosen each frame by the game loop.
                backdrop: Default::default(),
                frame_aspect: None,
                field_filter: settings.field_filter,
                preview: None,
            },
            gamepads: None,
            touch_canvas: None,
        }
    }
}

type Core = ddi_session::play::PlaySession<WebAudio, WebInputs>;

/// The play: the shared session (its fields and methods through `Deref`)
/// and what the field draws.
pub(crate) struct PlaySession {
    core: Core,
    pub(crate) render: RenderOptions,
}

impl Deref for PlaySession {
    type Target = Core;
    fn deref(&self) -> &Core {
        &self.core
    }
}

impl DerefMut for PlaySession {
    fn deref_mut(&mut self) -> &mut Core {
        &mut self.core
    }
}

/// A random 64-bit seed for the Shuffle turn.
fn random_seed() -> u64 {
    let hi = (js_sys::Math::random() * 4_294_967_296.0) as u64;
    let lo = (js_sys::Math::random() * 4_294_967_296.0) as u64;
    (hi << 32) | lo
}

impl PlaySession {
    /// Audio state for the debug overlay (see [`WebAudio::status_line`]).
    pub(crate) fn audio_status(&self) -> String {
        format!(
            "{} · volume {:.2}",
            self.core.audio().status_line(),
            self.core.volume()
        )
    }

    /// Builds the session and schedules the start on the audio clock.
    /// `audio` must have been created inside a user gesture.
    pub(crate) fn start(
        song: &Song,
        chart_index: usize,
        settings: &Settings,
        config: SessionConfig,
        audio: WebAudio,
        buffer: AudioBuffer,
    ) -> Result<PlaySession, String> {
        let inputs = WebInputs::new(config.gamepads, config.touch_canvas, &audio);
        let render = config.render;
        let core = ddi_session::play::SessionConfig {
            seed: random_seed(),
            reverse: render.reverse,
            hide_notes: render.hide_notes,
            ..config.session
        };
        let core = Core::start(song, chart_index, settings, core, audio, buffer, inputs)?;
        Ok(PlaySession { core, render })
    }

    /// Advance to the current host time. Returns judge events for effects.
    pub(crate) fn tick(&mut self, host_now: HostTime) -> (Vec<JudgeEvent>, Option<SessionEvent>) {
        let out = self.core.tick(host_now);
        let view = self.core.view;
        self.render.hide_notes = view.hide_notes;
        self.render.cancel_progress = view.cancel_progress;
        self.render.cancel_hint = view.cancel_hint;
        out
    }

    pub(crate) fn gamepads(&self) -> Option<&Gamepads> {
        self.core.inputs().gamepads.as_ref()
    }
}

/// The page's side of a play: its inputs, interruptions and device
/// changes.
pub(crate) struct WebInputs {
    keyboard: Option<WebKeyboard>,
    gamepads: Option<Gamepads>,
    touch: Option<TouchLanes>,
    touch_canvas: Option<HtmlCanvasElement>,
    interrupted: Rc<Cell<bool>>,
    _interrupt_listeners: Vec<EventListener>,
    /// Set when the audio device list or the display scale changed mid-play.
    device_changed: Rc<Cell<bool>>,
    _device_listeners: Vec<EventListener>,
    _dpr_query: Option<web_sys::MediaQueryList>,
}

impl WebInputs {
    fn new(
        gamepads: Option<Gamepads>,
        touch_canvas: Option<HtmlCanvasElement>,
        audio: &WebAudio,
    ) -> WebInputs {
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
        WebInputs {
            keyboard: None,
            gamepads,
            touch: None,
            touch_canvas,
            interrupted,
            _interrupt_listeners: interrupt_listeners,
            device_changed,
            _device_listeners: device_listeners,
            _dpr_query: dpr_query,
        }
    }
}

impl ddi_session::play::Inputs for WebInputs {
    fn begin(&mut self, capture: Vec<String>, layout: &Layout, reverse: bool) {
        self.keyboard = WebKeyboard::new(capture);
        self.touch = self.touch_canvas.take().and_then(|canvas| {
            TouchLanes::new(
                canvas,
                layout
                    .lanes
                    .iter()
                    .map(|l| (l.column, l.scroll_sign))
                    .collect(),
                reverse,
            )
        });
    }

    fn poll(&mut self, out: &mut Vec<RawInput>) {
        if let Some(kb) = self.keyboard.as_mut() {
            kb.poll(out);
        }
        if let Some(t) = self.touch.as_mut() {
            t.poll(out);
        }
        // The poller's own loops poll the pads (never from inside this
        // tick: its edge listener may re-enter the screen component).
        if let Some(g) = self.gamepads.as_mut() {
            g.poll(out);
        }
    }

    fn connected_pads(&self) -> Vec<ConnectedPad> {
        self.gamepads
            .as_ref()
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

    fn pads_running(&mut self, fast_poll: Option<bool>) {
        if let Some(g) = &self.gamepads {
            match fast_poll {
                Some(fast) => {
                    g.clear();
                    g.set_fast(fast);
                }
                None => g.set_fast(false),
            }
        }
    }

    fn release(&mut self) {
        self.keyboard = None;
        self.touch = None;
    }

    fn interrupted(&self) -> bool {
        self.interrupted.get()
    }

    fn device_changed(&self) -> bool {
        self.device_changed.get()
    }

    /// Schedules an edge on the test pad installed by the headless tests
    /// (`window.__DDI_FAKE_PAD.schedule(hostMs, control, pressed)`).
    fn press_test_pad(&mut self, host: HostTime, control: &str, pressed: bool) {
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
}
