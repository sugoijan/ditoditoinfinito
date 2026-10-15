//! The web shell of the UI core (`docs/plans/ui-core.md`, step 3), at
//! `#/ui-dev`, not linked from the menus: the developer screen
//! (`ddi_ui::dev`) on the app's one canvas over the whole window.
//!
//! The shell turns DOM events into `UiEvent`s and calls `Ui::handle`
//! inside each handler, carrying out the requests it returns there and
//! then (P2): the audio context is made and a sound started, the hidden
//! file input clicked, the hidden textarea focused, a window opened, all
//! still inside the gesture. `preventDefault` follows `Outcome::consumed`
//! (the canvas's own presses and wheels always).
//!
//! - **Pointers** on the canvas (`touch-action: none`, no context menu
//!   or iOS callout; captured on press), the **wheel** (ctrl+wheel and
//!   pinch kept from zooming the page), **keys** on the window,
//!   **focus**: `blur` and a hidden page release held input.
//! - **Typing** (P5) through a hidden `<textarea>` placed at the caret
//!   (step 0's recipe: opacity 0, 16 px, autocomplete, autocapitalize and
//!   autocorrect off, no spellcheck, `inputmode`, `enterkeyhint`,
//!   focused with `preventScroll` inside the gesture). Its value,
//!   selection and composition are read again on every composition,
//!   input, key and selection event; the composition's range is what
//!   changed since `compositionstart`; a key is the IME's while
//!   `isComposing` or with key code 229 (Safari's committing Enter); the
//!   textarea's `blur` ends typing.
//! - **Controllers** through the app's poller, by the player's menu
//!   bindings; a press counts as a gesture in Chrome and Firefox, not in
//!   WebKit (P2).
//! - **Back**: a dialog opened pushes a history entry, so the browser's
//!   Back (and Android's back gesture) closes it.
//! - **Size**: the visual viewport (a phone's keyboard shrinks it) less
//!   the safe-area insets, at the device pixel ratio; orientation flips
//!   are resizes.
//! - **Frames on demand** (P13): `requestAnimationFrame` only while
//!   `Ui::needs_redraw` (the poller keeps its own frames while a pad is
//!   connected and wakes this one on an edge).
//!
//! `window.__DDI_DEBUG.ui` publishes the screen stack, focus, the field,
//! the last requests, frames drawn, sounds played and where the named
//! actors are (CSS pixels), for the headless tests.

use std::cell::RefCell;
use std::collections::VecDeque;
use std::rc::{Rc, Weak};

use ddi_engine::ControlBindings;
use ddi_platform::DeviceId;
use ddi_platform::storage::{self, Area};
use ddi_render::DrawList;
use ddi_render::texture_cache::TextureKey;
use ddi_ui::dev::{self, DevScreen, Shared};
use ddi_ui::dev_widgets::{FoldFn, LIBRARY_MESSAGE, LibrarySong, SharedWidgets, WidgetScreen};
use ddi_ui::event::{
    DeltaMode, KeyEvent, MenuEvent, Modifiers, PointerEvent, PointerKind, PointerPhase, TextEvent,
    TextSync, UiEvent, WheelEvent, wheel_pixels,
};
use ddi_ui::request::{Cursor, EnterKey, InputMode};
use ddi_ui::state::library::Libraries;
use ddi_ui::text_field::composition_range;
use ddi_ui::{Message, TextMode, Ui, UiRequest};
use gloo::events::{EventListener, EventListenerOptions};
use gloo::render::{AnimationFrame, request_animation_frame};
use wasm_bindgen::{JsCast, JsValue};
use web_sys::{HtmlCanvasElement, HtmlElement, HtmlInputElement, HtmlTextAreaElement};
use yew::prelude::*;

use crate::web::audio::WebAudio;
use crate::web::gamepad::Gamepads;
use crate::web::gfx::{Gfx, SurfaceMode};
use crate::web::screen;

/// Requests kept for the debug view.
const KEPT_REQUESTS: usize = 24;
/// The actors whose places the debug view gives.
const NAMED: [&str; 31] = [
    "Beep",
    "Link",
    "Files",
    "Dialog",
    "Popup",
    "Count",
    "Field",
    "FieldText",
    "Strip",
    "Row0",
    "Row3",
    "Yes",
    "No",
    "PopA",
    "PopB",
    "Status",
    "Title",
    "Widgets",
    "Songs",
    "Search",
    "Search.Item0",
    "Search.Item1",
    "Style",
    "Speed",
    "Speed.Choice0",
    "Panels",
    "Panels.Choice1",
    "Reverse",
    "Volume",
    "Panel.Timing",
    "Songs.Bar",
];

#[derive(Properties, PartialEq)]
pub(crate) struct Props {
    /// `widgets`: the widget screen at once.
    #[prop_or_default]
    pub(crate) screen: String,
}

pub(crate) struct UiDev {
    host: Option<Rc<RefCell<Host>>>,
    listeners: Vec<EventListener>,
    gamepads: Option<Gamepads>,
    root: NodeRef,
    failure: Option<String>,
}

pub(crate) enum Msg {
    Failed(String),
}

/// What the composition started from.
struct Composition {
    before: String,
    data: String,
}

struct Host {
    ui: Ui,
    dev: Shared,
    gfx: Option<Rc<Gfx>>,
    attachment: Option<screen::Attachment>,
    canvas: HtmlCanvasElement,
    frame_el: HtmlElement,
    textarea: HtmlTextAreaElement,
    files: HtmlInputElement,
    raf: Option<AnimationFrame>,
    last_ms: Option<f64>,
    frames: u64,
    requests: VecDeque<String>,
    audio: Option<WebAudio>,
    beep: Option<web_sys::AudioBuffer>,
    sounds: u32,
    composition: Option<Composition>,
    controls: ControlBindings,
    /// Whether a pad press is a user gesture here (not in WebKit).
    pad_gesture: bool,
    /// History entries pushed for the layers over the first, and pops of
    /// ours to ignore.
    depth: usize,
    ignore_pops: u32,
    files_picked: u32,
    last_consumed: bool,
    list: DrawList,
    closed: bool,
    /// Where the caret was last put (CSS pixels of the canvas).
    caret: Option<[f32; 4]>,
    widgets: SharedWidgets,
    /// Textures were being loaded at the last frame: frames go on until
    /// they are in.
    loading: bool,
    /// The last frame's work and the slowest since the last report, ms.
    cpu_ms: f64,
    worst_ms: f64,
    /// Work of every frame drawn so far, ms (with `frames`, the mean).
    total_ms: f64,
    texture_stats: Option<ddi_render::texture_cache::TextureStats>,
}

impl Component for UiDev {
    type Message = Msg;
    type Properties = Props;

    fn create(_ctx: &Context<Self>) -> Self {
        UiDev {
            host: None,
            listeners: Vec::new(),
            gamepads: None,
            root: NodeRef::default(),
            failure: None,
        }
    }

    fn update(&mut self, _ctx: &Context<Self>, msg: Msg) -> bool {
        match msg {
            Msg::Failed(e) => {
                // The no-GPU page is a plain page: its keys and its Back are
                // the browser's.
                self.failure = Some(e);
                self.listeners.clear();
                if let Some(pads) = &self.gamepads {
                    pads.set_listener(None);
                }
                if let Some(host) = &self.host
                    && let Ok(mut h) = host.try_borrow_mut()
                {
                    h.closed = true;
                    h.raf = None;
                }
            }
        }
        true
    }

    fn rendered(&mut self, ctx: &Context<Self>, first_render: bool) {
        if !first_render {
            return;
        }
        let Some(el) = self.root.cast::<HtmlElement>() else {
            return;
        };
        match start(&el, &ctx.props().screen) {
            Ok((host, listeners, pads)) => {
                self.host = Some(host.clone());
                self.listeners = listeners;
                self.gamepads = pads;
                let link = ctx.link().clone();
                wasm_bindgen_futures::spawn_local(async move {
                    match screen::gfx().await {
                        Ok(gfx) => {
                            let Ok(mut h) = host.try_borrow_mut() else {
                                return;
                            };
                            if h.closed {
                                return;
                            }
                            gfx.set_mode(SurfaceMode::Plain);
                            h.gfx = Some(gfx);
                            drop(h);
                            schedule(&host);
                        }
                        Err(e) => link.send_message(Msg::Failed(e)),
                    }
                });
                load_library(Rc::downgrade(self.host.as_ref().expect("host")));
            }
            Err(e) => ctx.link().send_message(Msg::Failed(e)),
        }
    }

    fn destroy(&mut self, _ctx: &Context<Self>) {
        self.listeners.clear();
        if let Some(pads) = &self.gamepads {
            pads.set_listener(None);
        }
        if let Some(host) = &self.host
            && let Ok(mut h) = host.try_borrow_mut()
        {
            h.closed = true;
            h.raf = None;
            h.attachment = None;
            let _ = h.textarea.blur();
            if let Some(gfx) = h.gfx.take() {
                gfx.set_mode(SurfaceMode::Scene);
            }
        }
    }

    fn view(&self, _ctx: &Context<Self>) -> Html {
        if let Some(e) = &self.failure {
            return super::ui_test::no_gpu(e);
        }
        html! { <main class="ui-dev" ref={self.root.clone()}></main> }
    }
}

fn document() -> web_sys::Document {
    web_sys::window()
        .and_then(|w| w.document())
        .expect("document")
}

type Started = (Rc<RefCell<Host>>, Vec<EventListener>, Option<Gamepads>);

/// Makes the host: the canvas in its frame, the hidden textarea and file
/// input, the listeners.
fn start(el: &HtmlElement, first: &str) -> Result<Started, String> {
    let doc = document();
    let make = |tag: &str| doc.create_element(tag).map_err(|e| format!("{e:?}"));
    let frame_el: HtmlElement = make("div")?.unchecked_into();
    frame_el.set_class_name("ui-dev-frame");
    el.append_child(&frame_el).map_err(|e| format!("{e:?}"))?;
    let attachment = screen::attach(&frame_el, "ui-dev-canvas").ok_or("no canvas")?;
    let canvas = screen::canvas().ok_or("no canvas")?;
    let textarea: HtmlTextAreaElement = make("textarea")?.unchecked_into();
    textarea.set_class_name("ui-dev-input");
    for (k, v) in [
        ("autocomplete", "off"),
        ("autocapitalize", "off"),
        ("autocorrect", "off"),
        ("spellcheck", "false"),
        ("rows", "1"),
        ("aria-hidden", "true"),
        ("tabindex", "-1"),
    ] {
        let _ = textarea.set_attribute(k, v);
    }
    el.append_child(&textarea).map_err(|e| format!("{e:?}"))?;
    let files: HtmlInputElement = make("input")?.unchecked_into();
    files.set_type("file");
    files.set_multiple(true);
    files.set_class_name("ui-dev-files");
    // Out of the Tab order, as the textarea.
    let _ = files.set_attribute("tabindex", "-1");
    el.append_child(&files).map_err(|e| format!("{e:?}"))?;

    let dev: Shared = Shared::default();
    let widgets = SharedWidgets::default();
    let fold: FoldFn = Rc::new(nfkd);
    let mut ui = Ui::new(dev::DESIGN, TextMode::Mirror, true);
    ui.push(DevScreen::with_widgets(
        dev.clone(),
        widgets.clone(),
        fold.clone(),
    ));
    if first == "widgets" {
        ui.push(WidgetScreen::new(widgets.clone(), fold));
    }
    let settings = crate::settings::Settings::load();
    let host = Rc::new(RefCell::new(Host {
        ui,
        dev,
        widgets,
        gfx: None,
        attachment: Some(attachment),
        canvas: canvas.clone(),
        frame_el,
        textarea: textarea.clone(),
        files: files.clone(),
        raf: None,
        last_ms: None,
        frames: 0,
        requests: VecDeque::new(),
        audio: None,
        beep: None,
        sounds: 0,
        composition: None,
        controls: settings.controls,
        pad_gesture: !is_webkit(),
        depth: 0,
        ignore_pops: 0,
        files_picked: 0,
        last_consumed: false,
        list: DrawList::new(),
        closed: false,
        caret: None,
        loading: false,
        cpu_ms: 0.0,
        worst_ms: 0.0,
        total_ms: 0.0,
        texture_stats: None,
    }));
    resize(&host);
    // Back into an entry pushed for a dialog of an earlier visit (a reload,
    // the route left with a dialog open): that dialog is gone, so step back
    // to the route's own entry.
    if let Some(n) = pushed_depth()
        && let Some(history) = web_sys::window().and_then(|w| w.history().ok())
    {
        host.borrow_mut().ignore_pops += 1;
        let _ = history.go_with_delta(-(n as i32));
    }
    let listeners = listen(&host, &canvas, &textarea, &files);
    let pads = Gamepads::new();
    if let Some(p) = &pads {
        // Edges go to the listener alone: nothing drains a queue here.
        p.set_queue(false);
        let weak = Rc::downgrade(&host);
        p.set_listener(Some(Box::new(move |edge| pad_edge(&weak, edge))));
    }
    publish(&host.borrow());
    Ok((host, listeners, pads))
}

/// The browser's NFKD (`String.prototype.normalize`), which the search
/// folds with on the web.
fn nfkd(text: &str) -> String {
    js_sys::JsString::from(text)
        .normalize("NFKD")
        .as_string()
        .unwrap_or_else(|| text.to_string())
}

/// WebKit (Safari, and every browser on iOS) does not count a pad press
/// as a user gesture.
fn is_webkit() -> bool {
    let ua = web_sys::window()
        .and_then(|w| w.navigator().user_agent().ok())
        .unwrap_or_default();
    ua.contains("AppleWebKit") && !ua.contains("Chrome/") && !ua.contains("Chromium/")
}

fn modifiers(e: &web_sys::MouseEvent) -> Modifiers {
    Modifiers {
        shift: e.shift_key(),
        ctrl: e.ctrl_key(),
        alt: e.alt_key(),
        meta: e.meta_key(),
    }
}

fn num(v: &JsValue, key: &str) -> f64 {
    js_sys::Reflect::get(v, &key.into())
        .ok()
        .and_then(|v| v.as_f64())
        .unwrap_or(0.0)
}

fn listen(
    host: &Rc<RefCell<Host>>,
    canvas: &HtmlCanvasElement,
    textarea: &HtmlTextAreaElement,
    files: &HtmlInputElement,
) -> Vec<EventListener> {
    let window = web_sys::window().expect("window");
    let doc = document();
    let active = EventListenerOptions::enable_prevent_default();
    let mut out = Vec::new();
    let weak = Rc::downgrade(host);

    for (name, phase) in [
        ("pointerdown", PointerPhase::Down),
        ("pointermove", PointerPhase::Move),
        ("pointerup", PointerPhase::Up),
        ("pointercancel", PointerPhase::Cancel),
        ("pointerleave", PointerPhase::Leave),
    ] {
        let weak = weak.clone();
        let canvas2 = canvas.clone();
        out.push(EventListener::new_with_options(
            canvas,
            name,
            active,
            move |e| {
                let Some(e) = e.dyn_ref::<web_sys::PointerEvent>() else {
                    return;
                };
                let rect = canvas2.get_bounding_client_rect();
                let kind = match e.pointer_type().as_str() {
                    "touch" => PointerKind::Touch,
                    "pen" => PointerKind::Pen,
                    _ => PointerKind::Mouse,
                };
                if phase == PointerPhase::Down {
                    let _ = canvas2.set_pointer_capture(e.pointer_id());
                }
                let event = UiEvent::Pointer(PointerEvent {
                    id: e.pointer_id() as u64,
                    kind,
                    phase,
                    pos: [
                        (num(e, "clientX") - rect.left()) as f32,
                        (num(e, "clientY") - rect.top()) as f32,
                    ],
                    buttons: u32::from(e.buttons()),
                    modifiers: modifiers(e),
                    time: e.time_stamp(),
                });
                let consumed = dispatch(&weak, event);
                // A press on the canvas is the game's: no focus change (the
                // textarea keeps it while typing), no compatibility mouse
                // events, no text selection.
                if consumed || phase == PointerPhase::Down {
                    e.prevent_default();
                }
            },
        ));
    }
    out.push(EventListener::new_with_options(
        canvas,
        "contextmenu",
        active,
        |e| e.prevent_default(),
    ));
    // Safari's pinch.
    out.push(EventListener::new_with_options(
        canvas,
        "gesturestart",
        active,
        |e| e.prevent_default(),
    ));
    {
        let weak = weak.clone();
        let canvas2 = canvas.clone();
        out.push(EventListener::new_with_options(
            canvas,
            "wheel",
            active,
            move |e| {
                let Some(e) = e.dyn_ref::<web_sys::WheelEvent>() else {
                    return;
                };
                // Never the page's zoom or scroll.
                e.prevent_default();
                let rect = canvas2.get_bounding_client_rect();
                let mode = match e.delta_mode() {
                    1 => DeltaMode::Line,
                    2 => DeltaMode::Page,
                    _ => DeltaMode::Pixel,
                };
                let delta = wheel_pixels(
                    [e.delta_x() as f32, e.delta_y() as f32],
                    mode,
                    [rect.width() as f32, rect.height() as f32],
                );
                dispatch(
                    &weak,
                    UiEvent::Wheel(WheelEvent {
                        pos: [
                            (num(e, "clientX") - rect.left()) as f32,
                            (num(e, "clientY") - rect.top()) as f32,
                        ],
                        delta,
                        modifiers: modifiers(e),
                        time: e.time_stamp(),
                    }),
                );
            },
        ));
    }
    for name in ["keydown", "keyup"] {
        let weak = weak.clone();
        out.push(EventListener::new_with_options(
            &window,
            name,
            active,
            move |e| {
                let Some(e) = e.dyn_ref::<web_sys::KeyboardEvent>() else {
                    return;
                };
                let event = UiEvent::Key(KeyEvent {
                    code: e.code(),
                    key: e.key(),
                    down: e.type_() == "keydown",
                    repeat: e.repeat(),
                    modifiers: Modifiers {
                        shift: e.shift_key(),
                        ctrl: e.ctrl_key(),
                        alt: e.alt_key(),
                        meta: e.meta_key(),
                    },
                    // Safari sends the Enter that commits after
                    // `compositionend` with `isComposing` false, key code 229.
                    composing: e.is_composing() || e.key_code() == 229,
                    time: e.time_stamp(),
                });
                if dispatch(&weak, event) {
                    e.prevent_default();
                }
                sync_text(&weak);
                // And once the key's default action has moved the caret
                // (Chrome fires no `selectionchange` on arrows).
                let weak = weak.clone();
                gloo::timers::callback::Timeout::new(0, move || sync_text(&weak)).forget();
            },
        ));
    }
    {
        let weak = weak.clone();
        let ta = textarea.clone();
        out.push(EventListener::new(
            textarea,
            "compositionstart",
            move |_| {
                if let Some(h) = weak.upgrade()
                    && let Ok(mut h) = h.try_borrow_mut()
                {
                    h.composition = Some(Composition {
                        before: ta.value(),
                        data: String::new(),
                    });
                }
                sync_text(&weak);
            },
        ));
    }
    {
        let weak = weak.clone();
        out.push(EventListener::new(
            textarea,
            "compositionupdate",
            move |e| {
                let data = e
                    .dyn_ref::<web_sys::CompositionEvent>()
                    .and_then(|c| c.data())
                    .unwrap_or_default();
                if let Some(h) = weak.upgrade()
                    && let Ok(mut h) = h.try_borrow_mut()
                    && let Some(c) = &mut h.composition
                {
                    c.data = data;
                }
                sync_text(&weak);
            },
        ));
    }
    {
        let weak = weak.clone();
        out.push(EventListener::new(textarea, "compositionend", move |_| {
            if let Some(h) = weak.upgrade()
                && let Ok(mut h) = h.try_borrow_mut()
            {
                h.composition = None;
            }
            sync_text(&weak);
        }));
    }
    for name in ["input", "select", "keyup", "selectionchange"] {
        let weak = weak.clone();
        out.push(EventListener::new(textarea, name, move |_| {
            sync_text(&weak)
        }));
    }
    {
        // Chrome fires no `selectionchange` on arrows, but this catches the
        // rest (a tap moving the caret).
        let weak = weak.clone();
        out.push(EventListener::new(&doc, "selectionchange", move |_| {
            sync_text(&weak)
        }));
    }
    {
        let weak = weak.clone();
        out.push(EventListener::new(textarea, "blur", move |_| {
            if let Some(h) = weak.upgrade()
                && let Ok(mut h) = h.try_borrow_mut()
            {
                h.composition = None;
            }
            dispatch(&weak, UiEvent::Text(TextEvent::Blurred));
        }));
    }
    {
        let weak = weak.clone();
        let input = files.clone();
        out.push(EventListener::new(files, "change", move |_| {
            let n = input.files().map_or(0, |f| f.length());
            input.set_value("");
            // The picker may answer while the host is busy (Firefox under
            // automation, inside the click): handled after this task.
            let weak = weak.clone();
            wasm_bindgen_futures::spawn_local(async move {
                if let Some(h) = weak.upgrade()
                    && let Ok(mut h) = h.try_borrow_mut()
                {
                    h.files_picked += n;
                    h.dev.borrow_mut().status = format!("{n} file(s) picked");
                    h.ui.broadcast(Message::new("FilesPicked"));
                    publish(&h);
                }
                schedule_weak(&weak);
            });
        }));
    }
    for name in ["blur", "pagehide"] {
        let weak = weak.clone();
        out.push(EventListener::new(&window, name, move |_| {
            dispatch(&weak, UiEvent::Focus(false));
        }));
    }
    {
        let weak = weak.clone();
        out.push(EventListener::new(&window, "focus", move |_| {
            dispatch(&weak, UiEvent::Focus(true));
        }));
    }
    {
        let weak = weak.clone();
        out.push(EventListener::new(&doc, "visibilitychange", move |_| {
            let hidden = document().visibility_state() == web_sys::VisibilityState::Hidden;
            dispatch(&weak, UiEvent::Focus(!hidden));
        }));
    }
    {
        let weak = weak.clone();
        out.push(EventListener::new(&window, "popstate", move |_| {
            let Some(h) = weak.upgrade() else { return };
            {
                let Ok(mut h) = h.try_borrow_mut() else {
                    return;
                };
                if h.ignore_pops > 0 {
                    h.ignore_pops -= 1;
                    return;
                }
                h.depth = h.depth.saturating_sub(1);
            }
            // The browser's Back: the menu's Back (no gesture).
            for down in [true, false] {
                dispatch(
                    &weak,
                    UiEvent::Menu(MenuEvent {
                        action: ddi_ui::MenuAction::Back,
                        down,
                        source: u32::MAX,
                        gesture: false,
                        time: 0.0,
                    }),
                );
            }
        }));
    }
    let resize_targets: Vec<web_sys::EventTarget> = {
        let mut t: Vec<web_sys::EventTarget> = vec![window.clone().into()];
        if let Some(vv) = window.visual_viewport() {
            t.push(vv.into());
        }
        t
    };
    for target in resize_targets {
        for name in ["resize", "scroll", "orientationchange"] {
            let weak = weak.clone();
            out.push(EventListener::new(&target, name, move |_| {
                if let Some(h) = weak.upgrade() {
                    resize(&h);
                    schedule(&h);
                }
            }));
        }
    }
    out
}

/// The safe-area insets (top, right, bottom, left), CSS pixels.
fn safe_insets() -> [f64; 4] {
    let doc = document();
    let Some(body) = doc.body() else {
        return [0.0; 4];
    };
    let Ok(probe) = doc.create_element("div") else {
        return [0.0; 4];
    };
    let _ = probe.set_attribute(
        "style",
        "position:fixed;visibility:hidden;padding:env(safe-area-inset-top) env(safe-area-inset-right) env(safe-area-inset-bottom) env(safe-area-inset-left)",
    );
    let _ = body.append_child(&probe);
    let out = web_sys::window()
        .and_then(|w| w.get_computed_style(&probe).ok().flatten())
        .map(|s| {
            let px = |k: &str| {
                s.get_property_value(k)
                    .ok()
                    .and_then(|v| v.trim_end_matches("px").parse().ok())
                    .unwrap_or(0.0)
            };
            [
                px("padding-top"),
                px("padding-right"),
                px("padding-bottom"),
                px("padding-left"),
            ]
        })
        .unwrap_or([0.0; 4]);
    probe.remove();
    out
}

/// Places the canvas over the visible area less the safe-area insets and
/// tells the UI its size.
fn resize(host: &Rc<RefCell<Host>>) {
    let Some(window) = web_sys::window() else {
        return;
    };
    let (x, y, w, h) = match window.visual_viewport() {
        Some(vv) => (vv.offset_left(), vv.offset_top(), vv.width(), vv.height()),
        None => (
            0.0,
            0.0,
            window
                .inner_width()
                .ok()
                .and_then(|v| v.as_f64())
                .unwrap_or(1.0),
            window
                .inner_height()
                .ok()
                .and_then(|v| v.as_f64())
                .unwrap_or(1.0),
        ),
    };
    let [t, r, b, l] = safe_insets();
    let (x, y, w, h) = (x + l, y + t, (w - l - r).max(1.0), (h - t - b).max(1.0));
    let ratio = window.device_pixel_ratio() as f32;
    let Ok(mut host) = host.try_borrow_mut() else {
        return;
    };
    let style = host.frame_el.style();
    let _ = style.set_property("left", &format!("{x}px"));
    let _ = style.set_property("top", &format!("{y}px"));
    let _ = style.set_property("width", &format!("{w}px"));
    let _ = style.set_property("height", &format!("{h}px"));
    let out = host.ui.handle(UiEvent::Resize {
        size: [w as f32, h as f32],
        ratio,
    });
    host.run(out.requests);
    // The visual viewport may have moved under a caret that did not.
    if host.ui.text_field().is_some()
        && let Some(caret) = host.caret
    {
        host.place_textarea(caret);
    }
}

/// Hands an event to the UI inside the platform's handler; whether it
/// was consumed.
fn dispatch(weak: &Weak<RefCell<Host>>, event: UiEvent) -> bool {
    let Some(rc) = weak.upgrade() else {
        return false;
    };
    let consumed = {
        // Busy: an event the shell's own request caused (the textarea's
        // blur when typing stops), already accounted for.
        let Ok(mut h) = rc.try_borrow_mut() else {
            return false;
        };
        if h.closed {
            return false;
        }
        let out = h.ui.handle(event);
        h.last_consumed = out.consumed;
        h.run(out.requests);
        h.sync_history();
        publish(&h);
        out.consumed
    };
    schedule(&rc);
    consumed
}

/// The textarea's state, read again.
fn sync_text(weak: &Weak<RefCell<Host>>) {
    let Some(rc) = weak.upgrade() else { return };
    let sync = {
        let Ok(h) = rc.try_borrow() else { return };
        if h.ui.text_field().is_none() {
            return;
        }
        let ta = &h.textarea;
        let value = ta.value();
        let start = ta.selection_start().ok().flatten().unwrap_or(0) as usize;
        let end = ta.selection_end().ok().flatten().unwrap_or(0) as usize;
        let backward = ta.selection_direction().ok().flatten().as_deref() == Some("backward");
        let selection = if backward { [end, start] } else { [start, end] };
        let composition = h
            .composition
            .as_ref()
            .and_then(|c| composition_range(&c.before, &value, &c.data, end));
        TextSync {
            text: value,
            selection,
            composition,
        }
    };
    dispatch(weak, UiEvent::Text(TextEvent::Sync(sync)));
}

fn pad_edge(weak: &Weak<RefCell<Host>>, edge: &ddi_platform::RawInput) {
    let DeviceId::Gamepad(id) = &edge.device else {
        return;
    };
    let (action, gesture) = {
        let Some(rc) = weak.upgrade() else { return };
        let Ok(h) = rc.try_borrow() else { return };
        let Some(action) = h
            .controls
            .menu_input(id, pad_standard(edge.slot), &edge.control)
        else {
            return;
        };
        (action, h.pad_gesture)
    };
    dispatch(
        weak,
        UiEvent::Menu(MenuEvent {
            action: action.into(),
            down: edge.pressed,
            source: edge.slot,
            gesture,
            time: edge.host_time.0 * 1000.0,
        }),
    );
}

/// Whether the pad in `slot` has the standard mapping (read from the
/// browser: the poller is busy calling us).
fn pad_standard(slot: u32) -> bool {
    let Some(nav) = web_sys::window().map(|w| w.navigator()) else {
        return false;
    };
    let Ok(get) = js_sys::Reflect::get(&nav, &"getGamepads".into()) else {
        return false;
    };
    let Ok(get) = get.dyn_into::<js_sys::Function>() else {
        return false;
    };
    let Ok(list) = get.call0(&nav) else {
        return false;
    };
    let pad = js_sys::Reflect::get(&list, &JsValue::from(slot)).unwrap_or(JsValue::NULL);
    js_sys::Reflect::get(&pad, &"mapping".into())
        .ok()
        .and_then(|m| m.as_string())
        .is_some_and(|m| m == "standard")
}

fn schedule_weak(weak: &Weak<RefCell<Host>>) {
    if let Some(rc) = weak.upgrade() {
        schedule(&rc);
    }
}

/// A frame, if the UI needs one and none is coming (P13).
fn schedule(rc: &Rc<RefCell<Host>>) {
    let Ok(mut h) = rc.try_borrow_mut() else {
        return;
    };
    if h.closed || h.raf.is_some() || h.gfx.is_none() {
        return;
    }
    if !h.ui.needs_redraw() && !h.loading {
        // Idle: the next frame starts its clock afresh.
        h.last_ms = None;
        return;
    }
    let weak = Rc::downgrade(rc);
    h.raf = Some(request_animation_frame(move |time_ms| {
        let Some(rc) = weak.upgrade() else { return };
        {
            let Ok(mut h) = rc.try_borrow_mut() else {
                return;
            };
            h.raf = None;
            h.frame(time_ms);
            publish(&h);
        }
        schedule(&rc);
    }));
}

impl Host {
    /// Carries out requests (inside the platform's handler when they come
    /// from an event).
    fn run(&mut self, requests: Vec<UiRequest>) {
        for r in requests {
            self.requests.push_back(format!("{r:?}"));
            while self.requests.len() > KEPT_REQUESTS {
                self.requests.pop_front();
            }
            match r {
                UiRequest::StartTextInput {
                    caret,
                    text,
                    selection,
                    hints,
                } => {
                    let ta = self.textarea.clone();
                    ta.set_value(&text);
                    let (a, b) = (selection[0] as u32, selection[1] as u32);
                    let dir = if a > b { "backward" } else { "forward" };
                    let _ = ta.set_selection_range_with_direction(a.min(b), a.max(b), dir);
                    let mode = match hints.mode {
                        InputMode::Text => "text",
                        InputMode::Search => "search",
                        InputMode::Numeric => "numeric",
                        InputMode::Decimal => "decimal",
                        InputMode::Url => "url",
                        InputMode::Email => "email",
                    };
                    let enter = match hints.enter {
                        EnterKey::Enter => "enter",
                        EnterKey::Done => "done",
                        EnterKey::Go => "go",
                        EnterKey::Next => "next",
                        EnterKey::Search => "search",
                        EnterKey::Send => "send",
                    };
                    let _ = ta.set_attribute("inputmode", mode);
                    let _ = ta.set_attribute("enterkeyhint", enter);
                    self.place_textarea(caret);
                    let opts = web_sys::FocusOptions::new();
                    opts.set_prevent_scroll(true);
                    let _ = ta.focus_with_options(&opts);
                }
                UiRequest::StopTextInput => {
                    self.composition = None;
                    let _ = self.textarea.blur();
                }
                UiRequest::SetImeCursorArea(rect) => self.place_textarea(rect),
                UiRequest::SetText { text, selection } => {
                    self.textarea.set_value(&text);
                    let (a, b) = (selection[0] as u32, selection[1] as u32);
                    let _ = self.textarea.set_selection_range(a.min(b), a.max(b));
                }
                UiRequest::PickFiles { folder } => {
                    let _ = if folder {
                        self.files.set_attribute("webkitdirectory", "")
                    } else {
                        self.files.remove_attribute("webkitdirectory")
                    };
                    self.files.click();
                }
                UiRequest::OpenUrl(url) => {
                    if let Some(w) = web_sys::window() {
                        let _ = w.open_with_url_and_target_and_features(&url, "_blank", "noopener");
                    }
                }
                UiRequest::Navigate(route) => {
                    if let Some(w) = web_sys::window() {
                        let _ = w.location().set_hash(&route);
                    }
                }
                UiRequest::StartAudio => self.beep(),
                UiRequest::PlayPreview { .. } | UiRequest::StopPreview => {}
                UiRequest::Redraw => {}
                UiRequest::SetCursor(c) => {
                    let css = match c {
                        Cursor::Default => "default",
                        Cursor::Pointer => "pointer",
                        Cursor::Text => "text",
                        Cursor::Grab => "grab",
                        Cursor::Grabbing => "grabbing",
                    };
                    let _ = self.canvas.style().set_property("cursor", css);
                }
                UiRequest::Persist { key, value } => {
                    let _ = storage::set(Area::Local, &key, &value);
                }
                UiRequest::CopyText(text) => {
                    if let Some(w) = web_sys::window() {
                        let nav = w.navigator();
                        if let Ok(cb) = js_sys::Reflect::get(&nav, &"clipboard".into())
                            && let Ok(f) = js_sys::Reflect::get(&cb, &"writeText".into())
                            && let Ok(f) = f.dyn_into::<js_sys::Function>()
                        {
                            let _ = f.call1(&cb, &text.into());
                        }
                    }
                }
                // The textarea pastes by itself.
                UiRequest::Paste => {}
            }
        }
    }

    /// Plays a short tone: the context is made (or resumed) here, inside
    /// the gesture, with no `await` before it (`AGENTS.md`).
    fn beep(&mut self) {
        if self.audio.is_none() {
            match WebAudio::new() {
                Ok(a) => self.audio = Some(a),
                Err(e) => {
                    self.dev.borrow_mut().status = e;
                    return;
                }
            }
        }
        let Some(audio) = self.audio.as_mut() else {
            return;
        };
        audio.resume_now();
        if self.beep.is_none() {
            let ctx = audio.context();
            let rate = ctx.sample_rate();
            // 0.2 s of 880 Hz: 5 ms in, then fading out, peaking at −14 dBFS.
            let n = (rate * 0.2) as u32;
            if let Ok(buf) = ctx.create_buffer(1, n, rate) {
                let samples: Vec<f32> = (0..n)
                    .map(|i| {
                        let t = i as f32 / rate;
                        let envelope = (t / 0.005).min(1.0) * (-t * 18.0).exp();
                        (t * 880.0 * std::f32::consts::TAU).sin() * 0.2 * envelope
                    })
                    .collect();
                let _ = buf.copy_to_channel(&samples, 0);
                self.beep = Some(buf);
            }
        }
        // Straight to the output, at a modest level: a click of a tone.
        let ctx = audio.context();
        if let Some(buf) = &self.beep
            && let Ok(source) = ctx.create_buffer_source()
        {
            source.set_buffer(Some(buf));
            let scheduled: &web_sys::AudioScheduledSourceNode = source.as_ref();
            if source.connect_with_audio_node(&ctx.destination()).is_ok()
                && scheduled.start().is_ok()
            {
                self.sounds += 1;
            }
        }
    }

    /// The textarea at the caret (CSS pixels of the canvas's frame).
    fn place_textarea(&mut self, [x, y, w, h]: [f32; 4]) {
        self.caret = Some([x, y, w, h]);
        let rect = self.canvas.get_bounding_client_rect();
        let style = self.textarea.style();
        let _ = style.set_property("left", &format!("{}px", rect.left() + f64::from(x)));
        let _ = style.set_property("top", &format!("{}px", rect.top() + f64::from(y)));
        let _ = style.set_property("width", &format!("{}px", w.max(2.0)));
        let _ = style.set_property("height", &format!("{}px", h.max(16.0)));
    }

    /// A history entry for each layer over the first, so Back closes it;
    /// entries of layers closed otherwise go back quietly.
    fn sync_history(&mut self) {
        let Some(history) = web_sys::window().and_then(|w| w.history().ok()) else {
            return;
        };
        // Screens and dialogs shown (Back does nothing for a popup; one
        // leaving has had its Back).
        let depth = self
            .ui
            .layers()
            .iter()
            .filter(|l| (l.modal || !l.overlay) && l.phase != "out")
            .count()
            .saturating_sub(1);
        while self.depth < depth {
            let marker = js_sys::Object::new();
            let _ = js_sys::Reflect::set(
                &marker,
                &HISTORY_MARK.into(),
                &JsValue::from(self.depth as f64 + 1.0),
            );
            let _ = history.push_state(&marker, "");
            self.depth += 1;
        }
        if self.depth > depth {
            let n = self.depth - depth;
            self.ignore_pops += n as u32;
            let _ = history.go_with_delta(-(n as i32));
            self.depth = depth;
        }
    }

    fn frame(&mut self, time_ms: f64) {
        let began = web_sys::window()
            .and_then(|w| w.performance())
            .map_or(0.0, |p| p.now());
        self.draw_frame(time_ms);
        let ms = web_sys::window()
            .and_then(|w| w.performance())
            .map_or(0.0, |p| p.now())
            - began;
        self.cpu_ms = ms;
        self.worst_ms = self.worst_ms.max(ms);
        self.total_ms += ms;
    }

    fn draw_frame(&mut self, time_ms: f64) {
        let Some(gfx) = self.gfx.clone() else {
            return;
        };
        if gfx.is_lost() || !self.attachment.as_ref().is_some_and(|a| a.holds()) {
            return;
        }
        let dt = match self.last_ms {
            Some(last) => ((time_ms - last) / 1000.0).clamp(0.0, 0.25) as f32,
            None => 1.0 / 60.0,
        };
        self.last_ms = Some(time_ms);
        self.ui.update(dt);
        let frame_requests = self.ui.take_requests();
        self.run(frame_requests);
        self.sync_history();

        let canvas = &self.canvas;
        let vp = *self.ui.viewport();
        let (w, h) = (
            vp.physical[0].round().max(1.0) as u32,
            vp.physical[1].round().max(1.0) as u32,
        );
        if canvas.width() != w || canvas.height() != h {
            canvas.set_width(w);
            canvas.set_height(h);
        }
        gfx.set_mode(SurfaceMode::Plain);
        gfx.resize(w, h);
        screen::prepare_textures(&gfx);
        let Some(format) = gfx.menu_format() else {
            return;
        };
        let scale = vp.scale;
        let shift = [
            1.0,
            0.0,
            0.0,
            1.0,
            vp.offset[0] / scale,
            vp.offset[1] / scale,
        ];
        let ui = &mut self.ui;
        let list = &mut self.list;
        let dpr = vp.scale;
        let prepared = screen::with_painter(&gfx, |p| {
            let drawn = {
                let mut measure = p.measure(scale);
                ui.draw_in(&mut measure, shift)
            };
            list.clear();
            // Actor textures to the cache's, by source and the size drawn
            // (only what is drawn is asked for, P9).
            ddi_render::actors2d::append(&drawn, list, &mut |t| {
                let (source, side) = ui.texture_source(t)?;
                Some(p.texture(TextureKey::drawn(source, side * dpr)))
            });
            p.stats().textures
        });
        let Some(stats) = prepared else {
            return;
        };
        self.loading = stats.loading > 0;
        self.texture_stats = Some(stats);
        let draw_requests = self.ui.take_requests();
        self.run(draw_requests);
        let Some(surface) = gfx.frame() else {
            return;
        };
        let view = surface.texture.create_view(&wgpu::TextureViewDescriptor {
            format: Some(format),
            ..Default::default()
        });
        let mut encoder = gfx
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("ui-dev"),
            });
        let list = &self.list;
        screen::with_painter(&gfx, |p| {
            p.draw(
                &gfx.device,
                &gfx.queue,
                &mut encoder,
                &view,
                [w, h],
                scale,
                Some(wgpu::Color {
                    r: 12.0 / 255.0,
                    g: 12.0 / 255.0,
                    b: 18.0 / 255.0,
                    a: 1.0,
                }),
                list,
                &mut |_slot, _encoder| {},
            );
        });
        gfx.queue.submit(Some(encoder.finish()));
        gfx.queue.present(surface);
        screen::after_submit();
        self.frames += 1;
    }
}

/// The key of the history entries pushed for dialogs.
const HISTORY_MARK: &str = "ddiUiDepth";

/// The depth marked on the current history entry, if it is one pushed for
/// a dialog ([`Host::sync_history`]).
fn pushed_depth() -> Option<u32> {
    let state = web_sys::window()?.history().ok()?.state().ok()?;
    let n = js_sys::Reflect::get(&state, &HISTORY_MARK.into())
        .ok()?
        .as_f64()?;
    (n >= 1.0).then_some(n as u32)
}

/// The library's titles for the strip (P12's view).
fn load_library(weak: Weak<RefCell<Host>>) {
    wasm_bindgen_futures::spawn_local(async move {
        let Ok(library) = crate::songs::Library::load().await else {
            return;
        };
        let view = library.view();
        let titles: Vec<String> = view
            .songs
            .iter()
            .take(300)
            .map(|s| match s.pack() {
                Some(p) => format!("{} ({p})", s.entry.title),
                None => s.entry.title.clone(),
            })
            .collect();
        let skins = crate::noteskins::choices()
            .await
            .map(|c| c.views().len())
            .unwrap_or(0);
        let Some(rc) = weak.upgrade() else { return };
        {
            let Ok(mut h) = rc.try_borrow_mut() else {
                return;
            };
            if titles.is_empty() {
                return;
            }
            let songs = titles.len();
            h.dev.borrow_mut().songs = titles;
            h.widgets.borrow_mut().library = view
                .songs
                .iter()
                .map(|s| LibrarySong {
                    title: s.entry.title.clone(),
                    artist: s.entry.artist.clone(),
                    pack: s.pack().map(str::to_string),
                    banner: s.banner.clone(),
                })
                .collect();
            h.ui.broadcast(Message::new(LIBRARY_MESSAGE));
            h.ui.broadcast(Message::new(dev::SONGS_MESSAGE));
            h.dev.borrow_mut().status = format!("{songs} songs, {skins} noteskins");
            publish(&h);
        }
        schedule(&rc);
    });
}

fn publish(h: &Host) {
    let Some(window) = web_sys::window() else {
        return;
    };
    let set = |o: &js_sys::Object, k: &str, v: JsValue| {
        let _ = js_sys::Reflect::set(o, &k.into(), &v);
    };
    let obj = js_sys::Object::new();
    set(&obj, "frames", (h.frames as f64).into());
    if let Some(gfx) = &h.gfx {
        set(&obj, "backend", gfx.backend_name().into());
    }
    let ui = js_sys::Object::new();
    let layers = js_sys::Array::new();
    for l in h.ui.layers() {
        let o = js_sys::Object::new();
        set(&o, "name", l.name.into());
        set(&o, "phase", l.phase.into());
        set(&o, "overlay", l.overlay.into());
        set(&o, "modal", l.modal.into());
        set(&o, "focused", l.focused.map_or(JsValue::NULL, |f| f.into()));
        layers.push(&o);
    }
    set(&ui, "layers", layers.into());
    set(
        &ui,
        "focused",
        h.ui.focused().map_or(JsValue::NULL, |f| f.into()),
    );
    set(&ui, "ring", h.ui.focus_ring_shown().into());
    if let Some((name, field)) = h.ui.text_field() {
        let t = js_sys::Object::new();
        set(&t, "name", name.into());
        set(&t, "text", field.text().into());
        let [a, b] = field.selection_utf16();
        set(
            &t,
            "selection",
            js_sys::Array::of2(&(a as f64).into(), &(b as f64).into()).into(),
        );
        set(&t, "composing", field.is_composing().into());
        let d = field.display();
        if let Some(p) = d.preedit {
            set(&t, "preedit", d.text[p].into());
        }
        set(&ui, "text", t.into());
    } else {
        set(&ui, "text", JsValue::NULL);
    }
    let requests = js_sys::Array::new();
    for r in &h.requests {
        requests.push(&r.as_str().into());
    }
    set(&ui, "requests", requests.into());
    set(&ui, "frames", (h.frames as f64).into());
    set(&ui, "sounds", (h.sounds as f64).into());
    set(
        &ui,
        "audio",
        h.audio.as_ref().map_or(JsValue::NULL, |a| {
            format!("{:?}", a.context().state()).into()
        }),
    );
    set(&ui, "files", (h.files_picked as f64).into());
    set(&ui, "consumed", h.last_consumed.into());
    set(&ui, "needsRedraw", h.ui.needs_redraw().into());
    set(&ui, "padGesture", h.pad_gesture.into());
    {
        let d = h.dev.borrow();
        set(&ui, "status", d.status.as_str().into());
        set(&ui, "count", d.count.into());
        set(&ui, "beeps", d.beeps.into());
        set(&ui, "refused", d.refused.into());
        set(&ui, "field", d.field.as_str().into());
        let sub = js_sys::Array::new();
        for s in &d.submitted {
            sub.push(&s.as_str().into());
        }
        set(&ui, "submitted", sub.into());
        set(&ui, "strip", d.strip.into());
        set(&ui, "velocity", d.velocity.into());
        set(&ui, "songs", (d.songs.len() as f64).into());
    }
    // Where the named actors are, CSS pixels `[x, y, w, h]` in the window.
    let rects = js_sys::Object::new();
    let vp = h.ui.viewport();
    let origin = h.canvas.get_bounding_client_rect();
    for name in NAMED {
        if let Some(b) = h.ui.bounds(name) {
            let a = vp.to_physical([b[0], b[1]]);
            let c = vp.to_physical([b[2], b[3]]);
            let r = f64::from(vp.ratio);
            let arr = js_sys::Array::new();
            for v in [
                origin.left() + f64::from(a[0]) / r,
                origin.top() + f64::from(a[1]) / r,
                f64::from(c[0] - a[0]) / r,
                f64::from(c[1] - a[1]) / r,
            ] {
                arr.push(&v.into());
            }
            set(&rects, name, arr.into());
        }
    }
    set(&ui, "rects", rects.into());
    {
        let w = h.widgets.borrow();
        let o = js_sys::Object::new();
        set(&o, "status", w.status.as_str().into());
        set(&o, "selected", (w.selected as f64).into());
        set(&o, "scroll", w.scroll.into());
        set(&o, "items", (w.items as f64).into());
        set(&o, "pool", (w.pool as f64).into());
        set(&o, "style", (w.style as f64).into());
        set(&o, "speed", (w.speed as f64).into());
        set(&o, "panels", (w.panels as f64).into());
        set(&o, "reverse", w.reverse.into());
        set(&o, "volume", w.volume.into());
        set(&o, "search", w.search.as_str().into());
        set(&o, "popup", w.popup.into());
        let a = js_sys::Array::new();
        for x in &w.suggestions {
            a.push(&x.as_str().into());
        }
        set(&o, "suggestions", a.into());
        let a = js_sys::Array::new();
        for x in &w.submitted {
            a.push(&x.as_str().into());
        }
        set(&o, "submitted", a.into());
        set(&ui, "widgets", o.into());
    }
    set(&ui, "cpuMs", h.cpu_ms.into());
    set(&ui, "worstMs", h.worst_ms.into());
    set(&ui, "totalMs", h.total_ms.into());
    set(&ui, "imagesAsked", (h.ui.texture_count() as f64).into());
    if let Some(t) = &h.texture_stats {
        set(&ui, "texturesLoaded", (t.loaded as f64).into());
        set(&ui, "texturesLoading", (t.loading as f64).into());
        set(&ui, "textureEntries", (t.entries as f64).into());
    }
    set(&obj, "ui", ui.into());
    screen::debug_into(&obj);
    let _ = js_sys::Reflect::set(&window, &"__DDI_DEBUG".into(), &obj);
}
