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
//!
//! `?screen=select|results|options` runs the theme's screens instead
//! (`ddi_ui::screens` in `ddi_theme::Infinito`, step 5) on their sample
//! data, at the theme's design size (1280×720; a window under
//! `Compact.Below` CSS pixels high gets the compact design), with the
//! theme's text face registered from its files, reduced motion from the
//! settings and `prefers-reduced-motion`, and what they did in
//! `__DDI_DEBUG.ui.theme`.

use std::cell::RefCell;
use std::collections::VecDeque;
use std::rc::{Rc, Weak};

use crate::component::{Component, Context, NodeRef};
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
use ddi_ui::screens::{self as theme_screens, Env as ThemeEnv, sample};
use ddi_ui::text_field::composition_range;
use ddi_ui::{Message, TextMode, Ui, UiRequest};
use gloo::events::{EventListener, EventListenerOptions};
use gloo::render::{AnimationFrame, request_animation_frame};
use wasm_bindgen::{JsCast, JsValue};
use web_sys::{HtmlCanvasElement, HtmlElement, HtmlInputElement, HtmlTextAreaElement};

use crate::web::audio::WebAudio;
use crate::web::gamepad::{Gamepads, PadNews};
use crate::web::gfx::{Gfx, SurfaceMode};
use crate::web::screen;

/// Requests kept for the debug view.
const KEPT_REQUESTS: usize = 24;
/// The theme screens' actors whose places the debug view gives.
const THEME_NAMED: &[&str] = &[
    // Replays: the results' button, song select's plays list.
    "WatchReplay",
    "Watch",
    "Open",
    "Remove",
    "Newer",
    "Older",
    "Link.Manage",
    "Link.Calibrate",
    "Notice.Link0",
    "Notice.Link1",
    "Mod0",
    "PickFolder",
    "PickFiles",
    "NewSong",
    "Option.copy",
    "Option.videos",
    "Option.details",
    "Close",
    "Confirm",
    "Cancel",
    "RemovePack",
    "RemoveSong",
    "Reread",
    "AddIcon",
    "Ladder.Row1",
    "Ladder.Row2",
    "Ladder.Row4",
    "Tab3",
    "Tab4",
    "Chart1",
    "OffsetDown",
    "UseMean",
    "ResetOffset",
    "PanelSet2",
    "Wheel",
    "Search",
    "Info",
    "Tab0",
    "Tab1",
    "Tab2",
    "Ladder.Row0",
    "Ladder.Row3",
    "Ladder.Empty",
    "Link.Options",
    "Link.Add",
    "Play",
    "Chart0",
    "Chart2",
    "StyleButton",
    "SearchIcon",
    "SongList",
    "Retry",
    "OffsetUp",
    "PanelSet1",
    "Section1",
    "Option0",
    "Option2",
    "Link.Back",
    "OptionsIcon",
    "Back",
    "ResultsClip",
    "PanelSet0",
    "Extra",
];

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

#[derive(Clone, Default, PartialEq)]
pub(crate) struct Props {
    /// `widgets`: the widget screen at once.
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

pub(crate) struct Host {
    pub(crate) ui: Ui,
    dev: Shared,
    gfx: Option<Rc<Gfx>>,
    attachment: Option<screen::Attachment>,
    canvas: HtmlCanvasElement,
    /// The frame the canvas is placed in (the route's own); `None` when
    /// embedded in another screen's canvas ([`embed`]).
    frame_el: Option<HtmlElement>,
    textarea: HtmlTextAreaElement,
    files: HtmlInputElement,
    raf: Option<AnimationFrame>,
    last_ms: Option<f64>,
    /// Frames stopped (idle) since, seconds on the performance timeline,
    /// and the timer that wakes them for a screen's deadline
    /// ([`ddi_ui::Ui::idle_wake`]).
    idle_since: Option<f64>,
    wake: Option<gloo::timers::callback::Timeout>,
    frames: u64,
    requests: VecDeque<String>,
    audio: Option<WebAudio>,
    beep: Option<web_sys::AudioBuffer>,
    sounds: u32,
    composition: Option<Composition>,
    /// The input the hints followed in the host before, a pad its hub has
    /// not listed yet: taken with its first list ([`hand_over`]).
    pending_source: Option<ddi_ui::controller_hints::Source>,
    /// History entries pushed for the layers over the first, and pops of
    /// ours to ignore.
    depth: usize,
    ignore_pops: u32,
    /// A navigation asked for while our own entries are popped (a dialog
    /// closing as it navigates): done once they are, or the pop, which
    /// Firefox carries out later, would undo it.
    navigate_after_pops: Option<String>,
    files_picked: u32,
    last_consumed: bool,
    /// The pointer's shape the screen asked for (`screen::set_shape`,
    /// written again when the canvas comes back).
    cursor: &'static str,
    /// A mouse button is held (the pointer is not hidden then).
    mouse_held: bool,
    /// A page's host: drawn on the app's new device when the old one is
    /// lost, its screens as they were ([`device_lost`]).
    lost: Option<screen::Listener>,
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
    /// Options' field preview (`crate::field_preview`), made when first
    /// shown ([`Host::field_preview`] says where; frames go on while it
    /// shows).
    field: Rc<RefCell<Option<crate::field_preview::FieldPreviewHost>>>,
    /// The theme's screens, when the route asked for them.
    theme: Option<ThemeHost>,
    /// Over another screen's canvas ([`embed`]): its size is the canvas's,
    /// history is the route's, the debug state is added to the page's.
    embedded: bool,
    /// What the embedding screen adds to `__DDI_DEBUG`.
    extra: Option<DebugFn>,
    /// Embedded: where the canvas goes after a lost device, and the
    /// listeners on the canvas (made again with it).
    slot: Option<HtmlElement>,
    listeners: Vec<EventListener>,
    /// Where the options screen shows the field's preview, physical
    /// pixels of the canvas ([`UiRequest::FieldPreview`]).
    pub(crate) field_preview: Option<[f32; 4]>,
    receptor_preview: Option<ddi_ui::screens::prompt_controls::Picture>,
    /// The chart song select shows ([`warm_cell`]), its prompt's
    /// diagram's skin to ready ahead on the app's preview at the next
    /// frame (which has the device).
    warm: WarmCell,
    /// The settings the options page is editing, handed over as they
    /// change (taken by the next frame); the preview reads the stored
    /// ones on the developer routes, where nothing hands them over.
    field_settings: Rc<RefCell<Option<crate::settings::Settings>>>,
    field_last: Option<crate::settings::Settings>,
    /// Under reduced motion the preview is one still frame: frames are
    /// drawn only this many more times (after a change, while a skin
    /// loads), then none.
    field_busy: u32,
    field_size: [u32; 2],
    /// A page's own drawing before the 2D pass ([`set_frame_hook`]), and
    /// whether it wanted frames to go on at the last frame.
    frame_hook: Option<FrameHook>,
    hook_busy: bool,
}

/// A page's drawing in each frame, before the 2D pass (song select's
/// preview movie, `crate::preview_movie`): given the device, the frame's
/// encoder, the UI and the frame's time (ms); whether frames are to go on.
pub(crate) type FrameHook = Box<dyn FnMut(&Gfx, &mut wgpu::CommandEncoder, &Ui, f64) -> bool>;

/// The demo's moment the still preview shows (ms).
const FIELD_STILL_MS: f64 = 2500.0;

/// Adds an embedding screen's state to `__DDI_DEBUG`.
pub(crate) type DebugFn = Box<dyn Fn(&js_sys::Object)>;

/// What the theme's screens run with.
pub(crate) struct ThemeHost {
    env: ThemeEnv,
    look: Rc<ddi_theme::Infinito>,
    /// The system asks for reduced motion (`prefers-reduced-motion`).
    system_reduced: Rc<std::cell::Cell<bool>>,
}

/// Whether `screen` is one of the theme's.
fn theme_screen(screen: &str) -> bool {
    matches!(
        screen,
        "start" | "select" | "results" | "options" | "credits" | "calibration" | "prompt"
    )
}

/// The system's `prefers-reduced-motion`, and its query to listen to.
pub(crate) fn reduced_motion_query() -> Option<web_sys::MediaQueryList> {
    web_sys::window()?
        .match_media("(prefers-reduced-motion: reduce)")
        .ok()
        .flatten()
}

/// Whether the system prefers reduced motion now.
pub(crate) fn system_reduced() -> bool {
    reduced_motion_query().is_some_and(|q| q.matches())
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
            keep_source(h.ui.hints().source().clone());
            h.closed = true;
            h.raf = None;
            h.attachment = None;
            let _ = h.textarea.blur();
            drop_field(&h);
            if let Some(gfx) = h.gfx.take() {
                gfx.set_mode(SurfaceMode::Scene);
            }
        }
    }

    fn view(&self, ctx: &Context<Self>) {
        if let Some(e) = &self.failure {
            crate::component::no_gpu(ctx.root(), e);
            return;
        }
        ctx.root().set_class_name("ui-dev");
        self.root.set(ctx.root());
    }
}

fn document() -> web_sys::Document {
    web_sys::window()
        .and_then(|w| w.document())
        .expect("document")
}

pub(crate) type Started = (Rc<RefCell<Host>>, Vec<EventListener>, Option<Gamepads>);

/// A page's UI on the theme's design: under the boot shell its screen
/// comes in as the shell fades; the theme draws its own focus.
pub(crate) fn theme_ui() -> Ui {
    let mut ui = Ui::new(theme_screens::DESIGN, TextMode::Mirror, true);
    ui.hold_entrances(crate::boot::shown());
    ui.set_focus_ring(false);
    ui
}

/// Gives the host the app's device once it is there; `failed` hears why it
/// is not.
pub(crate) fn attach_gfx(host: Rc<RefCell<Host>>, failed: impl FnOnce(String) + 'static) {
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
            Err(e) => failed(e),
        }
    });
}

/// Stops a host [`start_ui`] made (the route left).
pub(crate) fn stop_ui(host: &Rc<RefCell<Host>>, pads: Option<&Gamepads>) {
    if let Some(pads) = pads {
        pads.set_listener(None);
    }
    if let Ok(mut h) = host.try_borrow_mut() {
        keep_source(h.ui.hints().source().clone());
        h.closed = true;
        h.raf = None;
        h.attachment = None;
        let _ = h.textarea.blur();
        drop_field(&h);
        h.frame_hook = None;
        if let Some(gfx) = h.gfx.take() {
            gfx.set_mode(SurfaceMode::Scene);
        }
    }
}

/// The field's preview outlives the page (its skin stays in for the
/// next screen that draws it: Options, the start prompt), but the 2D
/// pass forgets its image and its textures go.
fn drop_field(h: &Host) {
    let mut slot = h.field.borrow_mut();
    if let (Some(f), Some(gfx)) = (slot.as_mut(), h.gfx.as_ref())
        && f.on(gfx)
    {
        f.release(gfx);
    }
}

/// Where song select puts the controls of the chart it shows (set from
/// its calls, inside the host's own frame): the start prompt's
/// diagram's skin is readied for them ahead on the app's field preview
/// at the host's next frame, so the prompt shows its receptors at once.
pub(crate) type WarmCell = Rc<RefCell<Option<Rc<ddi_ui::screens::prompt_controls::Controls>>>>;

/// Gives the host song select's [`WarmCell`] (the page's own, filled
/// from the screen's first calls, before the host was handed out).
pub(crate) fn use_warm_cell(host: &Rc<RefCell<Host>>, cell: WarmCell) {
    host.borrow_mut().warm = cell;
}

/// Where a page editing the settings hands them to the field's preview
/// ([`Host::field_settings`]): set it whenever they change.
pub(crate) fn field_settings(
    host: &Rc<RefCell<Host>>,
) -> Rc<RefCell<Option<crate::settings::Settings>>> {
    host.borrow().field_settings.clone()
}

/// Draws `hook` in each frame from now on (`None`: no more), before the
/// 2D pass.
pub(crate) fn set_frame_hook(host: &Rc<RefCell<Host>>, hook: Option<FrameHook>) {
    if let Ok(mut h) = host.try_borrow_mut() {
        h.frame_hook = hook;
        h.hook_busy = false;
    }
    schedule(host);
}

/// Broadcasts message `name` to the host's screens and wakes it, as soon
/// as the host is free (after the handler that asked returns); nothing
/// once it is gone.
pub(crate) fn broadcast_soon(host: Weak<RefCell<Host>>, name: &'static str) {
    broadcast_message_soon(host, Message::new(name));
}

/// [`broadcast_soon`] with a message's parameters.
pub(crate) fn broadcast_message_soon(host: Weak<RefCell<Host>>, message: Message) {
    gloo::timers::callback::Timeout::new(0, move || {
        let Some(h) = host.upgrade() else { return };
        if let Ok(mut hm) = h.try_borrow_mut() {
            hm.ui.broadcast(message);
        }
        wake(&h);
    })
    .forget();
}

/// Draws a frame soon (the UI changed outside its events: a message
/// broadcast by the shell).
pub(crate) fn wake(host: &Rc<RefCell<Host>>) {
    schedule(host);
}

/// Makes the host: the canvas in its frame, the hidden textarea and file
/// input, the listeners.
fn start(el: &HtmlElement, first: &str) -> Result<Started, String> {
    let dev: Shared = Shared::default();
    let widgets = SharedWidgets::default();
    let fold: FoldFn = Rc::new(nfkd);
    let mut theme = None;
    let ui = if theme_screen(first) {
        let t = theme_host(fold.clone());
        let env = &t.env;
        let mut ui = crate::components::ui_dev::theme_ui();
        ui.push(sample::start_screen(env, false));
        if first != "start" {
            ui.push(sample::select_screen(
                env,
                Some(sample::motion_change(env, system_reduced)),
            ));
        }
        match first {
            "results" => ui.push(sample::results_screen(env)),
            "options" => ui.push(sample::options_screen(
                env,
                Some(sample::motion_change(env, system_reduced)),
            )),
            "credits" => ui.push(sample::credits_screen(
                env,
                &theme_screens::credits::sample(),
            )),
            "calibration" => ui.push(sample::calibration_screen(env)),
            "prompt" => ui.push(sample::prompt_screen(
                env,
                theme_screens::prompt::PromptStep::Ready,
            )),
            _ => {}
        }
        theme = Some(t);
        ui
    } else {
        let mut ui = Ui::new(dev::DESIGN, TextMode::Mirror, true);
        // Under the boot shell, the screen comes in as the shell fades.
        ui.hold_entrances(crate::boot::shown());
        ui.push(DevScreen::with_widgets(
            dev.clone(),
            widgets.clone(),
            fold.clone(),
        ));
        if first == "widgets" {
            ui.push(WidgetScreen::new(widgets.clone(), fold));
        }
        ui
    };
    let started = start_ui(el, ui, theme, None)?;
    started.0.borrow_mut().dev = dev;
    started.0.borrow_mut().widgets = widgets;
    Ok(started)
}

/// Runs `ui` (with `theme`'s env, when given) on the app's canvas over the
/// whole window in `el`, a route of its own: its frame, the hidden inputs,
/// the listeners, the pads, history for its dialogs; `extra` adds to
/// `__DDI_DEBUG`. The device comes later ([`attach_gfx`]).
pub(crate) fn start_ui(
    el: &HtmlElement,
    ui: Ui,
    theme: Option<ThemeHost>,
    extra: Option<DebugFn>,
) -> Result<Started, String> {
    let doc = document();
    let make = |tag: &str| doc.create_element(tag).map_err(|e| format!("{e:?}"));
    let frame_el: HtmlElement = make("div")?.unchecked_into();
    frame_el.set_class_name("ui-dev-frame");
    el.append_child(&frame_el).map_err(|e| format!("{e:?}"))?;
    let attachment = screen::attach(&frame_el, "ui-dev-canvas").ok_or("no canvas")?;
    let canvas = screen::canvas().ok_or("no canvas")?;
    let (textarea, files) = host_inputs(el)?;
    let host = new_host(
        ui,
        canvas.clone(),
        Some(attachment),
        Some(frame_el),
        textarea.clone(),
        files.clone(),
        theme,
    );
    host.borrow_mut().extra = extra;
    {
        let weak = Rc::downgrade(&host);
        let lost = screen::on_lost(move || {
            if let Some(h) = weak.upgrade() {
                device_lost(&h);
            }
        });
        host.borrow_mut().lost = Some(lost);
    }
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
    let mut listeners = listen(&host, &canvas, &textarea, &files);
    theme_listeners(&host, &mut listeners);
    let pads = Gamepads::new();
    if let Some(p) = &pads {
        p.for_page();
        // Edges go to the listener alone: nothing drains a queue here.
        p.set_queue(false);
        let weak = Rc::downgrade(&host);
        p.set_listener(Some(Box::new(move |news| pad_news(&weak, news))));
        hand_over(&host, p.connected());
    } else {
        hand_over(&host, Vec::new());
    }
    publish(&host.borrow());
    Ok((host, listeners, pads))
}

/// The hidden textarea (typing, P5) and file input, in `el`.
fn host_inputs(el: &HtmlElement) -> Result<(HtmlTextAreaElement, HtmlInputElement), String> {
    let doc = document();
    let make = |tag: &str| doc.create_element(tag).map_err(|e| format!("{e:?}"));
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
    Ok((textarea, files))
}

/// The theme, motion as the settings and the system say (its faces are
/// asked for at start, `crate::start`).
pub(crate) fn theme_host(fold: FoldFn) -> ThemeHost {
    let settings = crate::settings::Settings::load();
    let look = Rc::new(ddi_theme::Infinito::new());
    let env = ThemeEnv::new(look.clone(), fold);
    let system = system_reduced();
    env.follow_motion(settings.reduced_motion, system);
    env.follow_cursor(&settings);
    ThemeHost {
        env,
        look,
        system_reduced: Rc::new(std::cell::Cell::new(system)),
    }
}

impl ThemeHost {
    pub(crate) fn env(&self) -> &ThemeEnv {
        &self.env
    }
}

fn new_host(
    ui: Ui,
    canvas: HtmlCanvasElement,
    attachment: Option<screen::Attachment>,
    frame_el: Option<HtmlElement>,
    textarea: HtmlTextAreaElement,
    files: HtmlInputElement,
    theme: Option<ThemeHost>,
) -> Rc<RefCell<Host>> {
    HOSTS.with(|h| h.set(h.get() + 1));
    let settings = crate::settings::Settings::load();
    let embedded = frame_el.is_none();
    let mut ui = ui;
    // The menus' controller bindings (Options gives the UI its edits),
    // and whether a pad's press is a user gesture here (not in WebKit).
    ui.set_controls(
        settings.controls.clone(),
        settings.controller_styles.clone(),
    );
    ui.set_pad_gesture(!is_webkit());
    let host = Rc::new(RefCell::new(Host {
        ui,
        dev: Shared::default(),
        widgets: SharedWidgets::default(),
        gfx: None,
        attachment,
        canvas,
        frame_el,
        textarea,
        files,
        raf: None,
        last_ms: None,
        idle_since: None,
        wake: None,
        frames: 0,
        requests: VecDeque::new(),
        audio: None,
        beep: None,
        sounds: 0,
        composition: None,
        pending_source: None,
        depth: 0,
        ignore_pops: 0,
        navigate_after_pops: None,
        files_picked: 0,
        last_consumed: false,
        cursor: "default",
        mouse_held: false,
        lost: None,
        list: DrawList::new(),
        closed: false,
        caret: None,
        loading: false,
        field: crate::field_preview::slot(),
        cpu_ms: 0.0,
        worst_ms: 0.0,
        total_ms: 0.0,
        texture_stats: None,
        theme,
        embedded,
        extra: None,
        slot: None,
        listeners: Vec::new(),
        field_preview: None,
        receptor_preview: None,
        warm: Rc::default(),
        field_settings: Rc::default(),
        field_last: None,
        field_busy: 0,
        field_size: [0, 0],
        frame_hook: None,
        hook_busy: false,
    }));
    // A pointer a host before this one hid (a key's press on the start
    // prompt) stays hidden until the mouse moves.
    screen::set_shape("default");
    UP.with_borrow_mut(|up| {
        up.retain(|w| w.strong_count() > 0);
        up.push(Rc::downgrade(&host));
    });
    host
}

/// The theme's listeners: faces loading, the system's reduced motion.
fn theme_listeners(host: &Rc<RefCell<Host>>, listeners: &mut Vec<EventListener>) {
    if host.borrow().theme.is_none() {
        return;
    }
    // A face loaded: the runs drawn meanwhile are drawn again.
    let weak = Rc::downgrade(host);
    crate::web::text::on_fonts_loaded(move || {
        let Some(rc) = weak.upgrade() else {
            // The page is gone: no more calls.
            return false;
        };
        if let Ok(mut h) = rc.try_borrow_mut() {
            h.ui.broadcast(Message::new("FontsLoaded"));
        }
        schedule(&rc);
        true
    });
    // The system's reduced motion changed.
    if let Some(q) = reduced_motion_query() {
        let weak = Rc::downgrade(host);
        listeners.push(EventListener::new(&q, "change", move |e| {
            let reduce = js_sys::Reflect::get(e, &"matches".into())
                .ok()
                .and_then(|v| v.as_bool())
                .unwrap_or(false);
            let Some(rc) = weak.upgrade() else { return };
            if let Ok(mut h) = rc.try_borrow_mut()
                && let Some(t) = &h.theme
            {
                t.system_reduced.set(reduce);
                let setting = crate::settings::Settings::load().reduced_motion;
                t.env.reduced_motion.set(setting.reduced(reduce));
                h.ui.broadcast(Message::new("ReducedMotion"));
            }
            schedule(&rc);
        }));
    }
}

/// A theme screen drawn on another screen's canvas (the play route's
/// results): its own input, frames on demand, the theme; the canvas, its
/// place and the route stay the embedding screen's. Dropping it stops
/// it.
pub(crate) struct Embedded {
    pub(crate) host: Rc<RefCell<Host>>,
    /// The theme's (fonts, reduced motion).
    _listeners: Vec<EventListener>,
}

/// Runs `ui` (built with `theme`'s env) on the app's canvas, already in
/// the page; the hidden inputs go in `container`; `extra` adds to
/// `__DDI_DEBUG` what the embedding screen shows.
pub(crate) fn embed(
    container: &HtmlElement,
    ui: Ui,
    theme: ThemeHost,
    extra: DebugFn,
) -> Result<Embedded, String> {
    let canvas = screen::canvas().ok_or("no canvas")?;
    let (textarea, files) = host_inputs(container)?;
    let host = new_host(
        ui,
        canvas.clone(),
        None,
        None,
        textarea.clone(),
        files.clone(),
        Some(theme),
    );
    host.borrow_mut().extra = Some(extra);
    host.borrow_mut().slot = Some(container.clone());
    resize(&host);
    let canvas_listeners = listen(&host, &canvas, &textarea, &files);
    host.borrow_mut().listeners = canvas_listeners;
    let mut listeners = Vec::new();
    theme_listeners(&host, &mut listeners);
    let weak = Rc::downgrade(&host);
    wasm_bindgen_futures::spawn_local(async move {
        if let Ok(gfx) = screen::gfx().await {
            embedded_gfx(&weak, gfx, false);
        }
    });
    publish(&host.borrow());
    Ok(Embedded {
        host,
        _listeners: listeners,
    })
}

fn embedded_gfx(weak: &Weak<RefCell<Host>>, gfx: Rc<Gfx>, again: bool) {
    let Some(rc) = weak.upgrade() else { return };
    if again {
        // The device came with a new canvas: into the slot (an embedded
        // screen's) or the page's frame, listened to.
        let parts = rc.try_borrow().ok().and_then(|h| {
            let (place, id) = match (&h.slot, &h.frame_el) {
                (Some(s), _) => (s.clone(), "game-canvas"),
                (None, Some(f)) => (f.clone(), "ui-dev-canvas"),
                _ => return None,
            };
            Some((place, id, h.textarea.clone(), h.files.clone(), h.closed))
        });
        let Some((slot, id, textarea, files, closed)) = parts else {
            return;
        };
        if closed {
            return;
        }
        let attachment = screen::attach(&slot, id);
        let Some(canvas) = screen::canvas() else {
            return;
        };
        if let Ok(mut h) = rc.try_borrow_mut() {
            h.attachment = attachment;
            h.canvas = canvas.clone();
            screen::set_shape(h.cursor);
            h.listeners.clear();
        }
        let listeners = listen(&rc, &canvas, &textarea, &files);
        if let Ok(mut h) = rc.try_borrow_mut() {
            h.listeners = listeners;
        }
        resize(&rc);
    }
    {
        let Ok(mut h) = rc.try_borrow_mut() else {
            return;
        };
        if h.closed {
            return;
        }
        gfx.set_mode(SurfaceMode::Plain);
        h.gfx = Some(gfx);
        // Everything again on the new device (the cache reloads what is
        // drawn).
        h.ui.broadcast(Message::new("DeviceChanged"));
    }
    schedule(&rc);
}

impl Embedded {
    /// The app's device was made again: drawn on the new one.
    pub(crate) fn device_lost(&self) {
        device_lost(&self.host);
    }
}

/// The app's device was made again: the host's screens, as they are (the
/// section, the row, a dialog open), drawn on the new one.
pub(crate) fn device_lost(host: &Rc<RefCell<Host>>) {
    if let Ok(mut h) = host.try_borrow_mut() {
        h.gfx = None;
        h.raf = None;
    }
    let weak = Rc::downgrade(host);
    wasm_bindgen_futures::spawn_local(async move {
        // The new device, once it is made (the lost one may still be
        // the current one for a moment).
        for _ in 0..100 {
            match screen::gfx().await {
                Ok(gfx) if !gfx.is_lost() => {
                    embedded_gfx(&weak, gfx, true);
                    return;
                }
                Ok(_) => gloo::timers::future::TimeoutFuture::new(100).await,
                Err(_) => return,
            }
            if weak.upgrade().is_none_or(|h| h.borrow().closed) {
                return;
            }
        }
    });
}

thread_local! {
    /// The hosts alive: whether a UI screen draws on the canvas
    /// (`crate::boot::ready_when_settled` waits for its screen).
    static HOSTS: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
    /// The input the hints followed when the last host closed (or in the
    /// play since): the next host's UI starts from it ([`hand_over`]).
    static LAST_SOURCE: RefCell<ddi_ui::controller_hints::Source> = RefCell::default();
    /// The hosts up, to wake for the shell's fade ([`wake_all`]).
    static UP: RefCell<Vec<Weak<RefCell<Host>>>> = const { RefCell::new(Vec::new()) };
}

/// How many UI hosts are alive.
pub(crate) fn hosts() -> usize {
    HOSTS.with(std::cell::Cell::get)
}

/// The input the hints follow, kept for the next host (a host closing,
/// a play's key or pad press).
pub(crate) fn keep_source(source: ddi_ui::controller_hints::Source) {
    LAST_SOURCE.with_borrow_mut(|s| *s = source);
}

/// A new host's UI: the pads connected (`pads`, its hub's) and the input
/// the hints followed last.
pub(crate) fn hand_over(host: &Rc<RefCell<Host>>, pads: Vec<ddi_engine::ConnectedPad>) {
    if let Ok(mut h) = host.try_borrow_mut() {
        h.ui.set_pads(pads);
        let source = LAST_SOURCE.with_borrow(Clone::clone);
        h.ui.set_source(source.clone());
        // A hub just made has read no pads yet.
        if h.ui.hints().source() != &source {
            h.pending_source = Some(source);
        }
    }
}

/// A frame for every host up (the shell's fade begins: the screens
/// held under it come in, `Ui::reveal`).
pub(crate) fn wake_all() {
    let up: Vec<Rc<RefCell<Host>>> =
        UP.with_borrow_mut(|up| up.iter().filter_map(Weak::upgrade).collect());
    for host in &up {
        schedule(host);
    }
}

impl Drop for Host {
    fn drop(&mut self) {
        HOSTS.with(|h| h.set(h.get().saturating_sub(1)));
    }
}

impl Drop for Embedded {
    fn drop(&mut self) {
        if let Ok(mut h) = self.host.try_borrow_mut() {
            keep_source(h.ui.hints().source().clone());
            h.closed = true;
            h.raf = None;
            h.listeners.clear();
            let _ = h.textarea.blur();
            // The canvas is the app's one: a pointer this host's screen
            // asked for (over the prompt's Start, pressed) would stay on
            // whatever draws next (a play) with no host to change it. A
            // hidden pointer stays hidden until the mouse moves.
            screen::set_shape("default");
            drop_field(&h);
            h.textarea.remove();
            h.files.remove();
        }
    }
}

/// The browser's NFKD (`String.prototype.normalize`), which the search
/// folds with on the web.
pub(crate) fn nfkd(text: &str) -> String {
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
    // A touch's end makes no click: the press was the game's, and a click
    // would land on whatever the press's action put in its place (the
    // play's start prompt after Retry).
    out.push(EventListener::new_with_options(
        canvas,
        "touchend",
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
                // With Ctrl or ⌘ (a trackpad's pinch), the page zooms; the
                // UI leaves such wheels. Never the page's scroll.
                if e.ctrl_key() || e.meta_key() {
                    return;
                }
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
                        source: ddi_ui::event::WheelSource::of(mode),
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
    if !host.borrow().embedded {
        let weak = weak.clone();
        // The route this host runs on: a pop to another one is a
        // navigation away (Firefox fires `popstate` at once when the hash
        // is set), not the browser's Back out of a dialog.
        let route = route_path();
        out.push(EventListener::new(&window, "popstate", move |_| {
            if route_path() != route {
                return;
            }
            let Some(h) = weak.upgrade() else { return };
            {
                let Ok(mut h) = h.try_borrow_mut() else {
                    return;
                };
                if h.ignore_pops > 0 {
                    h.ignore_pops -= 1;
                    h.navigate_now();
                    return;
                }
                // Back only when the pop leaves a dialog's own entry (the
                // entry reached is below it); Forward into an entry of a
                // dialog already closed goes back again, quietly.
                let reached = pushed_depth().unwrap_or(0) as usize;
                if reached >= h.depth {
                    if reached > h.depth
                        && let Some(history) = web_sys::window().and_then(|w| w.history().ok())
                    {
                        let n = reached - h.depth;
                        // One `go`, one `popstate`.
                        h.ignore_pops += 1;
                        let _ = history.go_with_delta(-(n as i32));
                    }
                    return;
                }
                h.depth = reached;
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
pub(crate) fn safe_insets() -> [f64; 4] {
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
    let Ok(mut host) = host.try_borrow_mut() else {
        return;
    };
    let (x, y, mut w, mut h) = (x + l, y + t, (w - l - r).max(1.0), (h - t - b).max(1.0));
    let ratio = window.device_pixel_ratio() as f32;
    // Embedded: the canvas is where its screen put it.
    if host.embedded {
        let rect = host.canvas.get_bounding_client_rect();
        w = rect.width().max(1.0);
        h = rect.height().max(1.0);
    }
    // The theme's design for this size: the compact one for a small
    // window.
    if let Some(t) = &host.theme {
        let design = theme_screens::design_for(&*t.look, [w as f32, h as f32]);
        host.ui.set_design(design);
    }
    if let Some(frame_el) = &host.frame_el {
        let style = frame_el.style();
        let _ = style.set_property("left", &format!("{x}px"));
        let _ = style.set_property("top", &format!("{y}px"));
        let _ = style.set_property("width", &format!("{w}px"));
        let _ = style.set_property("height", &format!("{h}px"));
    }
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
        // Keys and pads hide the mouse pointer (not while a mouse button
        // is held: a drag goes on); the mouse moving shows it.
        if let UiEvent::Pointer(p) = &event
            && p.kind == ddi_ui::PointerKind::Mouse
        {
            h.mouse_held = p.buttons != 0;
        }
        let held = h.mouse_held;
        let hide = match &event {
            UiEvent::Key(k) => (k.down && !held).then_some(true),
            UiEvent::Menu(m) => (m.down && !held).then_some(true),
            UiEvent::Pointer(p) if p.kind == ddi_ui::PointerKind::Mouse => Some(false),
            UiEvent::Wheel(_) => Some(false),
            _ => None,
        };
        match hide {
            Some(true) => screen::hide_until_move(),
            Some(false) => screen::mouse_moved(),
            None => {}
        }
        let out = h.ui.handle(event);
        h.last_consumed = out.consumed;
        h.run(out.requests);
        h.sync_history();
        h.navigate_now();
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

/// A gamepad hub's news for the UI of `weak`, as it is: the pads
/// connected, or an edge (the UI resolves it to a menu action, or hands it
/// on to a screen binding controls).
pub(crate) fn pad_news(weak: &Weak<RefCell<Host>>, news: PadNews<'_>) {
    match news {
        PadNews::Pads(pads) => {
            if let Some(rc) = weak.upgrade()
                && let Ok(mut h) = rc.try_borrow_mut()
            {
                h.ui.set_pads(pads.to_vec());
                if let Some(source) = h.pending_source.take() {
                    h.ui.set_source(source);
                }
            }
            schedule_weak(weak);
        }
        PadNews::Edge(edge) => {
            dispatch(weak, UiEvent::Pad(edge.clone()));
        }
    }
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
    // The field preview moves, unless motion is reduced (then a still
    // frame, drawn again only when it changes).
    let field = h.field_preview.is_some() && (!h.field_still() || h.field_busy > 0);
    let field = field || h.hook_busy;
    // While the shell waits for the first screen, frames poll what is
    // still coming (`crate::boot::settling`): a still screen under
    // reduced motion asks for none; and one comes for the held
    // entrances once the shell fades (`Ui::reveal`).
    let reveal = h.ui.holding() && crate::boot::revealed();
    if !h.ui.needs_redraw() && !h.loading && !field && !crate::boot::waiting() && !reveal {
        // Idle: the next frame starts its clock afresh, and a screen's
        // deadline wakes frames (the UI needs one once it is met).
        h.last_ms = None;
        let now = crate::web::clock::PerformanceClock::now_ms() / 1000.0;
        let mut since = h.idle_since;
        let at = h.ui.idle_wake(&mut since, now);
        h.idle_since = since;
        h.wake = at.map(|at| {
            let weak = Rc::downgrade(rc);
            let ms = ((at - now) * 1000.0).ceil().clamp(0.0, f64::from(u32::MAX));
            gloo::timers::callback::Timeout::new(ms as u32, move || {
                if let Some(rc) = weak.upgrade() {
                    if let Ok(mut h) = rc.try_borrow_mut() {
                        h.wake = None;
                    }
                    schedule(&rc);
                }
            })
        });
        if !h.ui.needs_redraw() {
            return;
        }
    }
    h.idle_since = None;
    h.wake = None;
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
    /// Whether the field preview is a still frame (reduced motion).
    fn field_still(&self) -> bool {
        self.receptor_preview.is_some()
            || self
                .theme
                .as_ref()
                .is_some_and(|t| t.env.reduced_motion.get())
    }

    /// Carries out requests (inside the platform's handler when they come
    /// from an event).
    fn run(&mut self, requests: Vec<UiRequest>) {
        for r in requests {
            self.requests.push_back(match &r {
                UiRequest::ReceptorPreview(p) => {
                    format!("ReceptorPreview({:?})", p.as_ref().map(|p| p.active))
                }
                _ => format!("{r:?}"),
            });
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
                // After our own entries' pops (`navigate_now`).
                UiRequest::Navigate(route) => {
                    if let Some(theme) = &self.theme {
                        crate::router::save_view(
                            self.ui.focused(),
                            Some(&theme.env.state.borrow()),
                        );
                    }
                    if matches!(crate::router::Route::current(), crate::router::Route::Songs) {
                        crate::select_preview::close();
                    }
                    self.navigate_after_pops = Some(route);
                }
                UiRequest::Back => {
                    if matches!(crate::router::Route::current(), crate::router::Route::Songs) {
                        crate::select_preview::close();
                    }
                    self.navigate_after_pops = Some("back".into());
                }
                UiRequest::Quit => {}
                UiRequest::StartAudio => self.beep(),
                UiRequest::PlayPreview { .. } | UiRequest::StopPreview => {}
                UiRequest::Redraw => {}
                UiRequest::SetCursor(c) => {
                    let css = match c {
                        Cursor::Hidden => "none",
                        Cursor::Default => "default",
                        Cursor::Pointer => "pointer",
                        Cursor::Text => "text",
                        Cursor::Grab => "grab",
                        Cursor::Grabbing => "grabbing",
                    };
                    self.cursor = css;
                    screen::set_shape(css);
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
                // Where the options screen shows the field (the preview
                // is drawn by the play's renderer: kept for it and the
                // debug state).
                UiRequest::FieldPreview(rect) => {
                    self.field_preview = rect;
                    self.receptor_preview = None;
                }
                UiRequest::ReceptorPreview(picture) => {
                    self.field_preview = picture.as_ref().map(|p| [0.0, 0.0, p.size[0], p.size[1]]);
                    if self.receptor_preview != picture {
                        self.field_busy = 2;
                    }
                    self.receptor_preview = picture;
                }
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

    /// The navigation asked for, once no pop of ours is under way: out of
    /// the handler that asked (setting the hash fires `popstate` at once
    /// in Firefox, into this host's listener).
    fn navigate_now(&mut self) {
        if self.ignore_pops > 0 {
            return;
        }
        if let Some(route) = self.navigate_after_pops.take() {
            gloo::timers::callback::Timeout::new(0, move || {
                if route == "back" {
                    crate::router::back();
                } else {
                    crate::router::navigate(&route);
                }
            })
            .forget();
        }
    }

    /// A history entry for each layer over the first, so Back closes it;
    /// entries of layers closed otherwise go back quietly.
    fn sync_history(&mut self) {
        // Embedded: history is the route's.
        if self.embedded {
            return;
        }
        let Some(history) = web_sys::window().and_then(|w| w.history().ok()) else {
            return;
        };
        // Screens and dialogs shown (`ddi_ui::back_stack`).
        let depth = ddi_ui::back_stack::depth(&self.ui.layers());
        let step = ddi_ui::back_stack::step(self.depth, depth);
        for mark in step.push {
            let marker = crate::router::history_state();
            let _ =
                js_sys::Reflect::set(&marker, &HISTORY_MARK.into(), &JsValue::from(mark as f64));
            let _ = history.push_state(&marker, "");
            crate::router::history_changed();
        }
        if let Some(n) = step.back {
            // One `go`, one `popstate`, however far.
            self.ignore_pops += step.ignore_pops;
            let _ = history.go_with_delta(-(n as i32));
        }
        self.depth = depth;
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
        if gfx.is_lost() || !(self.embedded || self.attachment.as_ref().is_some_and(|a| a.holds()))
        {
            return;
        }
        let dt = match self.last_ms {
            Some(last) => ((time_ms - last) / 1000.0).clamp(0.0, 0.25) as f32,
            None => 1.0 / 60.0,
        };
        self.last_ms = Some(time_ms);
        // The wall clock, seconds since the epoch: the page's time origin
        // plus the frame's timestamp, what the page's loader runs its art
        // on (site/boot-art.js), and a reload does not start the art
        // over from the same state.
        let origin = web_sys::window()
            .and_then(|w| w.performance())
            .map_or(0.0, |p| p.time_origin());
        self.ui.set_clock((origin + time_ms) / 1000.0);
        self.ui.update(dt);
        // What the first screens need, loaded under the shell from the
        // first frame on (`preload`); the loader's bar runs its last
        // stretch on it and the loader fades once the screen is at rest
        // with it all in, so the menu shows whole under it, its rain
        // where the loader's stood.
        if let Some(theme) = &self.theme {
            crate::preload::start(theme.env(), self.ui.viewport().screen, &gfx, !self.embedded);
        }
        if crate::boot::waiting() {
            crate::preload::poll();
            if crate::boot::settling(self.ui.settled()) {
                crate::boot::ready();
            }
        }
        if crate::boot::revealed() {
            self.ui.reveal();
            // Its entrance played out: a play's skin may load now.
            if self.ui.settled() {
                crate::boot::entrance_done();
            }
        }
        if !self.embedded
            && let Some(theme) = &self.theme
        {
            crate::router::save_view(self.ui.focused(), Some(&theme.env.state.borrow()));
        }
        let frame_requests = self.ui.take_requests();
        self.run(frame_requests);
        self.sync_history();
        self.navigate_now();

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
                let image = ui.texture_source(t)?;
                let key = if image.blurred {
                    TextureKey::blurred(image.source, image.side)
                } else {
                    TextureKey::drawn(image.source, image.side * dpr)
                };
                Some(p.texture(key))
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
        // The chart song select shows: its prompt's diagram's skin
        // readied ahead on the app's preview ([`warm_cell`]).
        let warm = self.warm.borrow_mut().take();
        if let Some(c) = warm {
            let mut slot = self.field.borrow_mut();
            if slot.as_ref().is_none_or(|f| !f.on(&gfx)) {
                *slot = Some(crate::field_preview::FieldPreviewHost::new(&gfx));
            }
            if let Some(f) = slot.as_mut() {
                f.warm(&gfx, &Rc::downgrade(&self.field), &c);
            }
        }
        // Options' field preview: its frame first, then the 2D pass that
        // shows it.
        if let Some([_, _, pw, ph]) = self.field_preview {
            let size = [pw.round().max(1.0) as u32, ph.round().max(1.0) as u32];
            let mut slot = self.field.borrow_mut();
            let mut fresh = false;
            if let Some(s) = self.field_settings.borrow_mut().take() {
                self.field_last = Some(s);
                fresh = true;
            }
            if slot.as_ref().is_none_or(|f| !f.on(&gfx)) {
                *slot = Some(crate::field_preview::FieldPreviewHost::new(&gfx));
                fresh = true;
            }
            let still = self.field_still();
            if fresh || size != self.field_size {
                // A change: draw it (and the skin it may load).
                self.field_busy = 30;
                self.field_size = size;
            }
            if let Some(f) = slot.as_mut() {
                if let Some(picture) = &self.receptor_preview {
                    f.set_receptors(picture);
                } else {
                    match &self.field_last {
                        Some(s) if fresh => f.set_settings(s),
                        Some(_) => {}
                        None => f.poll_settings(time_ms),
                    }
                }
                if still && self.receptor_preview.is_none() {
                    f.seek(FIELD_STILL_MS / 1000.0);
                }
                f.draw(
                    &gfx,
                    &mut encoder,
                    size,
                    if still { FIELD_STILL_MS } else { time_ms },
                    &Rc::downgrade(&self.field),
                );
                if let Some(picture) = &self.receptor_preview {
                    picture.ready.set(f.ready());
                }
                if f.status.contains("loading") {
                    self.field_busy = self.field_busy.max(2);
                }
            }
            self.field_busy = self.field_busy.saturating_sub(1);
        } else if let Some(f) = self.field.borrow_mut().as_mut()
            && f.on(&gfx)
        {
            f.release(&gfx);
        }
        if let Some(hook) = self.frame_hook.as_mut() {
            self.hook_busy = hook(&gfx, &mut encoder, &self.ui, time_ms);
        }
        let list = &self.list;
        let after = screen::with_painter(&gfx, |p| {
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
            p.stats().textures
        });
        gfx.queue.submit(Some(encoder.finish()));
        gfx.queue.present(surface);
        screen::after_submit();
        self.frames += 1;
        // The draw asks for the images it lacks: frames go on until they
        // are in (a first display, a new device, a new size).
        if let Some(stats) = after {
            self.loading = stats.loading > 0;
            self.texture_stats = Some(stats);
        }
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

/// The library's titles for the strip: bundled first, then the imported
/// songs by pack; banners as texture sources (a site path, or
/// `files:<key>` for an imported one, `crate::web::textures`).
fn load_library(weak: Weak<RefCell<Host>>) {
    wasm_bindgen_futures::spawn_local(async move {
        let Ok(library) = crate::songs::Library::load().await else {
            return;
        };
        let bundled = library.bundled.iter().map(|e| LibrarySong {
            title: e.title.clone(),
            artist: e.artist.clone(),
            pack: None,
            banner: e.banner.as_ref().map(|b| format!("songs/{}/{b}", e.id)),
        });
        let imported = library.imported.iter().map(|s| LibrarySong {
            title: s.entry.title.clone(),
            artist: s.entry.artist.clone(),
            pack: Some(s.pack.clone()),
            banner: s
                .entry
                .banner
                .as_ref()
                .map(|b| format!("files:{}", crate::songs::file_key(&s.entry.id, b))),
        });
        let library: Vec<LibrarySong> = bundled.chain(imported).collect();
        let titles: Vec<String> = library
            .iter()
            .take(300)
            .map(|s| match &s.pack {
                Some(p) => format!("{} ({p})", s.title),
                None => s.title.clone(),
            })
            .collect();
        let skins = crate::noteskins::choices()
            .await
            .map(|c| c.skins.len())
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
            h.widgets.borrow_mut().library = library;
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
    // Embedded: added to the page's own debug state (the play's).
    let page = js_sys::Reflect::get(&window, &"__DDI_DEBUG".into())
        .ok()
        .filter(|v| v.is_object())
        .map(js_sys::Object::from);
    let obj = match (h.embedded, page) {
        (true, Some(page)) => page,
        _ => js_sys::Object::new(),
    };
    if !h.embedded {
        set(&obj, "frames", (h.frames as f64).into());
        if let Some(gfx) = &h.gfx {
            set(&obj, "backend", gfx.backend_name().into());
        }
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
    set(
        &ui,
        "chromeFocus",
        h.ui.chrome_focused().map_or(JsValue::NULL, |s| s.into()),
    );
    set(
        &ui,
        "fieldPreview",
        h.field_preview.map_or(JsValue::NULL, |r| {
            let a = js_sys::Array::new();
            for v in r {
                a.push(&v.into());
            }
            a.into()
        }),
    );
    // What draws it: `{frames, status, skin, layout, demoTime, texture}`.
    {
        let f = js_sys::Object::new();
        if let Ok(slot) = h.field.try_borrow()
            && let Some(fp) = slot.as_ref()
        {
            let p = fp.preview();
            set(&f, "frames", (p.frames() as f64).into());
            set(&f, "status", fp.status.as_str().into());
            set(
                &f,
                "skin",
                p.noteskin()
                    .map_or(JsValue::NULL, |s| s.name.as_str().into()),
            );
            // The values its Lua reads for the skin's parameters.
            set(
                &f,
                "params",
                p.noteskin()
                    .map_or(JsValue::NULL, |s| crate::web::json(&s.parameters())),
            );
            set(&f, "layout", p.options().layout.as_str().into());
            set(&f, "demoTime", p.demo_time().into());
            set(&f, "textShaped", (p.text_shaped() as f64).into());
            // Skin loads finished and given up (stepping through skins).
            let (done, given_up) = crate::noteskins::loads();
            set(&f, "loads", (done as f64).into());
            set(&f, "loadsGivenUp", (given_up as f64).into());
            set(&f, "wants", fp.debug_wants().as_str().into());
            if let Some(t) = p.texture() {
                let a = js_sys::Array::new();
                a.push(&f64::from(t.width()).into());
                a.push(&f64::from(t.height()).into());
                set(&f, "texture", a.into());
            }
        }
        set(&ui, "fieldPreviewDrawn", f.into());
    }
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
    set(&ui, "padGesture", (!is_webkit()).into());
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
    // Where the named actors are on view, CSS pixels `[x, y, w, h]` in
    // the window.
    let rects = js_sys::Object::new();
    let vp = h.ui.viewport();
    let origin = h.canvas.get_bounding_client_rect();
    let names: &[&str] = if h.theme.is_some() {
        THEME_NAMED
    } else {
        &NAMED
    };
    let mut names: Vec<String> = names.iter().map(|n| n.to_string()).collect();
    // Options: its rows and what they hold, its sections; a page's items;
    // the notices' buttons.
    if let Some(t) = &h.theme {
        let st = t.env.state.borrow();
        names.extend(st.page_items.iter().cloned());
        names.extend(
            [
                "More",
                "PreviousPage",
                "NextPage",
                "Capture",
                "Number0",
                "Number1",
                "Units",
                "Step",
                "Type",
                "Reset",
                "Remove",
                "Tap",
                "Apply",
                "ApplyOffset",
            ]
            .into_iter()
            .map(str::to_string),
        );
        names.push("AppNotice".into());
        // The plays list's rows (by their place in the list).
        names.extend((0..st.plays.len()).map(|i| format!("Row{i}")));
        names.push("ClearSearch".into());
        // Song select's quick volume and its slider.
        names.extend(["Volume", "Volume.Popup"].map(str::to_string));
        if st.screen == "start" {
            names.extend(st.page_items.iter().cloned());
            names.push("Multiplayer".into());
        } else if st.screen == "prompt" {
            // The start prompt's buttons, by name (`Start`, `Device.defaults`),
            // its play options' rows (`Opt.<id>`, their `.Down` and `.Up`)
            // and `SaveOptions`.
            names.extend(st.page_items.iter().cloned());
            names.push("Card".into());
            names.push("SaveOptions".into());
            names.push("Receptors0".into());
            names.push("OptionsHeading".into());
            names.push("TimingHint0".into());
            names.push("Calibrate".into());
            if let Some(p) = &st.prompt {
                names.extend((0..p.bindings.len()).map(|i| format!("BindingKey{i}")));
                for o in &p.options {
                    let id = o.split('=').next().unwrap_or_default();
                    for part in ["", ".Down", ".Up"] {
                        names.push(format!("Opt.{id}{part}"));
                    }
                }
            }
        } else if st.screen == "editor" {
            // The chart editor's buttons, field and dialogs (a list's
            // `Item.<id>` in `page_items`).
            names.extend(
                [
                    "Button.Menu",
                    "Button.Save",
                    "Button.Play",
                    "FieldLayer",
                    "Cursor",
                    "OverviewPlate",
                    "AudioColumn",
                    "AudioPicture",
                    "Button.Audio",
                    "Panel.Sounds",
                    "Panel.Tap",
                    "Value",
                    "Ok",
                    "Cancel",
                    "Confirm",
                ]
                .map(String::from),
            );
            names.extend(st.page_items.iter().cloned());
        } else if !st.page_items.is_empty() {
            names.extend(["Column".to_string(), "Back".into()]);
            names.extend((0..st.page_items.len()).map(|i| format!("Item{i}")));
        }
        if st.screen == "options" {
            names.extend(
                [
                    "Back",
                    "RowsClip",
                    "Preview",
                    "Description",
                    "PreviewLayout",
                    "PreviewLayout.Down",
                    "PreviewLayout.Up",
                ]
                .map(String::from),
            );
            names.extend((0..12).map(|i| format!("Section{i}")));
            for i in 0..st.option_rows.len() {
                names.push(format!("Option{i}"));
                names.push(format!("Option{i}.Down"));
                names.push(format!("Option{i}.Up"));
                names.push(format!("Option{i}.Track"));
                for part in ["Choice", "Button"] {
                    for j in 0.. {
                        let n = format!("Option{i}.{part}{j}");
                        if h.ui.bounds(&n).is_none() {
                            break;
                        }
                        names.push(n);
                    }
                }
            }
        }
    }
    // Only what is on view: shown, and cut by the clips around it (a
    // row scrolled under a header is not there to tap).
    for name in &names {
        if let Some(b) = h.ui.visible_bounds(name) {
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
            set(&rects, name.as_str(), arr.into());
        }
    }
    set(&ui, "rects", rects.into());
    // The entrances held under the boot shell (`Ui::hold_entrances`):
    // whether they still are, and a part's alpha, for the verify skill.
    set(&ui, "holding", h.ui.holding().into());
    set(&ui, "revealed", crate::boot::revealed().into());
    let alpha = |name: &str| {
        h.ui.actor(name).map_or(JsValue::NULL, |a| {
            JsValue::from_f64(f64::from(a.current().diffuse()[3]))
        })
    };
    set(&ui, "choicesAlpha", alpha("MenuChoices"));
    set(&ui, "cardAlpha", alpha("Prompt"));
    {
        let w = h.widgets.borrow();
        let o = js_sys::Object::new();
        set(&o, "status", w.status.as_str().into());
        set(&o, "selected", (w.selected as f64).into());
        set(&o, "scroll", w.scroll.into());
        set(&o, "blur", w.blur.into());
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
    if let Some(t) = &h.theme {
        let st = t.env.state.borrow();
        let o = js_sys::Object::new();
        let strings = |v: &[String]| {
            let a = js_sys::Array::new();
            for x in v {
                a.push(&x.as_str().into());
            }
            JsValue::from(a)
        };
        set(&o, "screen", st.screen.as_str().into());
        set(&o, "compact", st.compact.into());
        set(&o, "reducedMotion", st.reduced_motion.into());
        set(&o, "song", st.song.as_str().into());
        set(&o, "songId", st.song_id.as_str().into());
        set(
            &o,
            "sharedPack",
            st.shared_pack
                .as_deref()
                .map_or(JsValue::NULL, |s| s.into()),
        );
        set(&o, "toShare", strings(&st.to_share));
        set(
            &o,
            "header",
            st.header.as_deref().map_or(JsValue::NULL, |s| s.into()),
        );
        set(&o, "greyed", st.greyed.into());
        set(&o, "style", st.style.as_str().into());
        set(
            &o,
            "chart",
            st.chart.as_deref().map_or(JsValue::NULL, |s| s.into()),
        );
        set(&o, "preferred", st.preferred.as_str().into());
        set(&o, "ladder", strings(&st.ladder));
        let counts = js_sys::Object::new();
        for (name, n) in &st.style_counts {
            set(&counts, name, (*n as f64).into());
        }
        set(&o, "styleCounts", counts.into());
        set(&o, "items", (st.items as f64).into());
        set(&o, "closed", strings(&st.closed));
        set(&o, "greyedRows", (st.greyed_rows as f64).into());
        set(&o, "search", st.search.as_str().into());
        set(&o, "popup", st.popup.into());
        set(&o, "suggestions", strings(&st.suggestions));
        set(&o, "matches", (st.matches as f64).into());
        set(&o, "hints", strings(&st.hints));
        set(
            &o,
            "backdrop",
            st.backdrop.as_deref().map_or(JsValue::NULL, |s| s.into()),
        );
        set(&o, "panels", st.panels.as_str().into());
        set(&o, "songOffset", st.song_offset.into());
        set(&o, "section", st.section.as_str().into());
        set(&o, "option", st.option.as_str().into());
        set(&o, "optionId", st.option_id.as_str().into());
        set(&o, "value", st.value.as_str().into());
        set(&o, "rows", strings(&st.option_rows));
        set(&o, "scroll", st.scroll.into());
        set(&o, "notice", st.notice.as_str().into());
        set(&o, "pageItems", strings(&st.page_items));
        // The start prompt (`docs/plans/start-prompt.md`).
        set(
            &o,
            "prompt",
            st.prompt.as_ref().map_or(JsValue::NULL, |p| {
                let v = js_sys::Object::new();
                set(&v, "loading", p.loading.into());
                set(&v, "label", p.label.into());
                set(
                    &v,
                    "countdown",
                    p.countdown.map_or(JsValue::NULL, |s| s.into()),
                );
                set(&v, "countdownState", p.countdown_state.as_str().into());
                set(&v, "options", strings(&p.options));
                set(&v, "optionsDiffer", p.options_differ.into());
                set(&v, "bindings", strings(&p.bindings));
                set(
                    &v,
                    "activeLane",
                    p.active_lane.map_or(JsValue::NULL, |i| (i as f64).into()),
                );
                set(&v, "saved", p.options_saved.into());
                set(&v, "waiting", strings(&p.waiting));
                v.into()
            }),
        );
        set(&o, "autoplay", st.autoplay.into());
        set(&o, "pageFocus", st.page_focus.into());
        // Song select's plays list: its rows and the chosen one.
        set(&o, "plays", strings(&st.plays));
        set(
            &o,
            "playsSelected",
            st.plays_selected
                .as_deref()
                .map_or(JsValue::NULL, |s| s.into()),
        );
        set(&o, "pageHeight", st.page_height.into());
        set(
            &o,
            "preview",
            st.preview.map_or(JsValue::NULL, |r| {
                let a = js_sys::Array::new();
                for v in r {
                    a.push(&v.into());
                }
                a.into()
            }),
        );
        set(&o, "status", st.status.as_str().into());
        // The chart editor's view (`docs/plans/editor.md`).
        set(
            &o,
            "editor",
            st.editor.as_ref().map_or(JsValue::NULL, |e| {
                let v = js_sys::Object::new();
                set(&v, "song", e.song.as_str().into());
                set(&v, "chart", (e.chart as f64).into());
                set(&v, "charts", (e.charts as f64).into());
                set(&v, "layout", e.layout.as_str().into());
                set(&v, "cursor", (e.cursor as f64).into());
                set(&v, "snap", (e.snap as f64).into());
                set(&v, "tool", e.tool.as_str().into());
                set(&v, "notes", (e.notes as f64).into());
                set(&v, "dirty", e.dirty.into());
                set(&v, "canUndo", e.can_undo.into());
                set(&v, "listening", e.listening.into());
                set(&v, "shown", e.shown.into());
                set(&v, "segments", strings(&e.segments));
                set(&v, "metronome", e.metronome.into());
                set(&v, "clap", e.clap.into());
                set(&v, "taps", (e.taps as f64).into());
                set(&v, "tapped", e.tapped.map_or(JsValue::NULL, JsValue::from));
                set(&v, "overview", (e.overview as f64).into());
                set(&v, "view", e.view.as_str().into());
                set(&v, "plates", e.plates.into());
                set(&v, "beatHeight", e.beat_h.into());
                set(
                    &v,
                    "record",
                    e.record.as_ref().map_or(JsValue::NULL, |r| {
                        let o = js_sys::Object::new();
                        set(&o, "active", r.active.into());
                        set(&o, "mode", r.mode.as_str().into());
                        set(&o, "start", (r.start as f64).into());
                        set(&o, "notes", (r.notes as f64).into());
                        set(&o, "presses", (r.presses as f64).into());
                        let ms = |v: Option<f64>| v.map_or(JsValue::NULL, JsValue::from);
                        set(&o, "meanMs", ms(r.mean_ms));
                        set(&o, "spreadMs", ms(r.spread_ms));
                        let rows = js_sys::Array::new();
                        for &(tick, lane, end) in &r.rows {
                            let row = js_sys::Array::of3(
                                &(tick as f64).into(),
                                &f64::from(lane).into(),
                                &end.map_or(JsValue::NULL, |e| (e as f64).into()),
                            );
                            rows.push(&row);
                        }
                        set(&o, "rows", rows.into());
                        o.into()
                    }),
                );
                let (skin, frames) = crate::editor_field::status();
                set(&v, "skin", skin.map_or(JsValue::NULL, |s| s.into()));
                set(&v, "fieldFrames", (frames as f64).into());
                let a = &e.audio;
                let audio = js_sys::Object::new();
                set(&audio, "ready", a.ready.into());
                set(&audio, "coming", a.coming.into());
                set(&audio, "shown", a.shown.into());
                set(&audio, "mode", a.mode.as_str().into());
                set(&audio, "layout", a.layout.as_str().into());
                let numbers = |v: &[f32]| {
                    v.iter()
                        .map(|x| JsValue::from(*x))
                        .collect::<js_sys::Array>()
                        .into()
                };
                set(&audio, "rect", numbers(&a.rect));
                set(&audio, "onsets", numbers(&a.onsets));
                set(&audio, "painted", (a.painted as f64).into());
                set(&audio, "micros", a.micros.into());
                set(&v, "audioView", audio.into());
                v.into()
            }),
        );
        set(
            &o,
            "best",
            st.best.as_deref().map_or(JsValue::NULL, |s| s.into()),
        );
        set(&o, "notices", strings(&st.notices));
        set(
            &o,
            "selectedRow",
            st.selected_row
                .as_deref()
                .map_or(JsValue::NULL, |s| s.into()),
        );
        set(&o, "over", (st.over as f64).into());
        set(
            &o,
            "fontsVersion",
            (crate::web::text::fonts_version() as f64).into(),
        );
        let d = h.ui.design();
        set(
            &o,
            "design",
            js_sys::Array::of2(&d[0].into(), &d[1].into()).into(),
        );
        set(&ui, "theme", o.into());
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
    if let Some(extra) = &h.extra {
        extra(&obj);
    }
    let _ = js_sys::Reflect::set(&window, &"__DDI_DEBUG".into(), &obj);
}

/// The address's route without its parameters (`#/options`).
fn route_path() -> String {
    let hash = web_sys::window()
        .and_then(|w| w.location().hash().ok())
        .unwrap_or_default();
    hash.split('?')
        .next()
        .unwrap_or("")
        .trim_end_matches('/')
        .to_string()
}
