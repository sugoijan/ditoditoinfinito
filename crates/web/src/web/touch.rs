//! Touch lanes for phones and tablets. The touch controls the player
//! chose (`docs/plans/touch-controls.md`): buttons under the thumbs or the
//! dance mat's pad, placed by `ddi_ui::touch_controls` as the play's view
//! drew them this frame ([`set_controls`]), a finger pressing the lane of
//! the target it lands on and a slide keeping its lane until it is well
//! inside another's (`Controls::lane_at`); they show while touch is the
//! input source (a key or a pad hides them, a touch shows them again).
//! Under the legacy style, Columns, the play screen split into
//! full-height columns, one per lane, whose boundaries are the midpoints
//! between adjacent receptors. The outer columns run to the screen edges, so
//! a finger anywhere below or above a receptor presses its lane whatever the
//! aspect ratio, and reverse scroll (which only moves the receptors up or
//! down) changes nothing. Layouts with two rows (Dancing☆Onigiri 11 keys and
//! up: one row's receptors at the top, the other's at the bottom) split the
//! screen in half as well: each half plays the row whose receptors are there.
//!
//! Pointer events rather than touch events: they carry a `pointerId` per
//! finger (several fingers are a jump) and a `pointerType`, so mouse clicks
//! are ignored while touch and pen press lanes. Each pointer is captured by
//! the host element, so a finger that slides off the canvas still releases.
//! Sliding into another column releases the old lane and presses the new
//! one. Several fingers on one lane hold it until the last one lifts; only
//! the first is a step (`LaneInput` drops a press of a control already
//! down).
//!
//! A press on the quit button (any pointer type; drawn by the scene at
//! the left edge, away from the receptors, while a touch screen plays
//! and for a moment after the mouse moved, [`set_quit_shown`],
//! [`mouse_moved`], `ddi_render::scene::quit_button`) is the `cancel`
//! control instead, which the play session treats like Escape (hold or
//! double tap to quit). A press on a multiplayer stage's attack policy
//! label (any pointer type; [`set_policy_at`]) is the `policy` control,
//! which cycles the policy. A press on the music's volume beside the quit
//! button (any pointer type; [`set_volume_at`],
//! `ddi_shell::play_volume`) is neither a control nor a lane: its edges
//! go to the play's view at the next frame ([`take_volume_edges`]),
//! never to the play's inputs or log; a press elsewhere while its slider
//! is open also closes it, and goes on. Touch is not a sync-accuracy target: event
//! timestamps are as good as the browser's touch pipeline.

use std::cell::RefCell;
use std::rc::Rc;

use ddi_engine::FieldPlace;
use ddi_platform::{DeviceId, HostTime, InputSource, RawInput};
use ddi_render::FieldGeometry;
use gloo::events::{EventListener, EventListenerOptions};
use wasm_bindgen::JsCast;
use web_sys::{Element, HtmlCanvasElement, PointerEvent};

use ddi_shell::play_volume::{Edge as VolumeEdge, Phase as VolumePhase, VolumeAt};

use super::clock::PerformanceClock;

/// Control emitted by the quit button.
pub(crate) const CANCEL_CONTROL: &str = "cancel";
/// The safe area's insets, and the canvas's size (CSS pixels) they were
/// read at.
type Insets = ((u32, u32), [f32; 4]);

thread_local! {
    /// A play runs, and a touch screen plays it (the quit button shows).
    static PLAYING: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
    static TOUCH: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
    /// When the mouse last moved over the play (`performance.now()`).
    static MOUSE_AT: std::cell::Cell<f64> = const { std::cell::Cell::new(f64::NEG_INFINITY) };
    /// Where it is drawn now (CSS pixels of the canvas: middle and radius).
    static QUIT_AT: std::cell::Cell<Option<[f32; 3]>> = const { std::cell::Cell::new(None) };
    /// Where a stage's attack policy label is drawn now (CSS pixels of the
    /// canvas: left, top, right, bottom).
    static POLICY_AT: std::cell::Cell<Option<[f32; 4]>> = const { std::cell::Cell::new(None) };
    /// A watched stage's fields a press follows (each with its seat;
    /// none: back to every field) and where one skips a stage the player
    /// is out of to its results (CSS pixels of the canvas: left, top,
    /// right, bottom), as the play drew them
    /// (`ddi_shell::play_view::Drawn`).
    static WATCH_AT: std::cell::RefCell<WatchFields> =
        const { std::cell::RefCell::new(Vec::new()) };
    static SKIP_AT: std::cell::Cell<Option<[f32; 4]>> = const { std::cell::Cell::new(None) };
    /// Where the music's volume is drawn now (CSS pixels of the canvas),
    /// and its pointers' edges since the last frame.
    static VOLUME_AT: std::cell::Cell<Option<VolumeAt>> = const { std::cell::Cell::new(None) };
    static VOLUME_EDGES: std::cell::RefCell<Vec<VolumeEdge>> =
        const { std::cell::RefCell::new(Vec::new()) };
    /// The safe area's insets (top, right, bottom, left, CSS pixels), read
    /// again when the canvas's size changes (a rotation).
    static INSETS: std::cell::Cell<Option<Insets>> = const { std::cell::Cell::new(None) };
    /// The touch controls the play was started with.
    static CHOICE: std::cell::Cell<ddi_ui::touch_controls::Choice> =
        const {
            std::cell::Cell::new(ddi_ui::touch_controls::Choice {
                style: ddi_ui::touch_controls::Style::Columns,
                hand: ddi_ui::touch_controls::Hand::Right,
            })
        };
    /// Where the play's view placed them this frame (CSS pixels of the
    /// canvas); none under the columns.
    static CONTROLS: std::cell::RefCell<Option<ddi_ui::touch_controls::Controls>> =
        const { std::cell::RefCell::new(None) };
}

/// While a play is on (round TC2: on an iPhone a finger on the touch
/// controls now and then "selected" the canvas, Safari's long-press or
/// double-tap selection landing on the page around the game), nothing of
/// the page may be selected: the page's root unselectable and without
/// the long-press callout (set on the element, not in the stylesheet,
/// whose bytes are the first round trip's), a selection's start
/// refused (`selectstart`), one that came anyway cleared
/// (`selectionchange`), and a finger's `touchstart` on the play's host
/// default-prevented (no long press, no double tap, no magnifier; the
/// pointer events the lanes read still come). Made at the play's start,
/// gone with it (the prompt's Start, the results and the menus' text
/// fields are as before). `__DDI_DEBUG.touchSelection` counts what it
/// refused.
pub(crate) struct NoSelection {
    _listeners: Vec<EventListener>,
}

/// The root's styles a play sets, each restored after it.
const NO_SELECTION_STYLES: [(&str, &str); 3] = [
    ("-webkit-user-select", "none"),
    ("user-select", "none"),
    ("-webkit-touch-callout", "none"),
];

thread_local! {
    /// `[selectstart refused, selections cleared, touchstarts]`.
    static SELECTION_COUNTS: std::cell::Cell<[u32; 3]> = const { std::cell::Cell::new([0; 3]) };
}

/// What [`NoSelection`] refused so far: `[selectstart refused,
/// selections cleared, touchstarts default-prevented]`.
pub(crate) fn selection_counts() -> [u32; 3] {
    SELECTION_COUNTS.with(|c| c.get())
}

fn count_selection(i: usize) {
    SELECTION_COUNTS.with(|c| {
        let mut v = c.get();
        v[i] = v[i].saturating_add(1);
        c.set(v);
    });
}

fn root_style() -> Option<web_sys::CssStyleDeclaration> {
    let root = web_sys::window()?.document()?.document_element()?;
    Some(root.dyn_into::<web_sys::HtmlElement>().ok()?.style())
}

impl NoSelection {
    /// From now on until dropped, for the play on `host`.
    pub(crate) fn new(host: &Element) -> NoSelection {
        if let Some(style) = root_style() {
            for (name, value) in NO_SELECTION_STYLES {
                let _ = style.set_property(name, value);
            }
        }
        let opts = EventListenerOptions::enable_prevent_default();
        let mut listeners = Vec::new();
        if let Some(doc) = web_sys::window().and_then(|w| w.document()) {
            listeners.push(EventListener::new_with_options(
                &doc,
                "selectstart",
                opts,
                |e| {
                    e.prevent_default();
                    count_selection(0);
                },
            ));
            listeners.push(EventListener::new(&doc, "selectionchange", |_| {
                if let Some(sel) = web_sys::window().and_then(|w| w.get_selection().ok().flatten())
                    && sel.range_count() > 0
                    && !sel.is_collapsed()
                {
                    let _ = sel.remove_all_ranges();
                    count_selection(1);
                }
            }));
        }
        listeners.push(EventListener::new_with_options(host, "touchstart", opts, |e| {
            if e.cancelable() {
                e.prevent_default();
                count_selection(2);
            }
        }));
        NoSelection {
            _listeners: listeners,
        }
    }
}

impl Drop for NoSelection {
    fn drop(&mut self) {
        if let Some(style) = root_style() {
            for (name, _) in NO_SELECTION_STYLES {
                let _ = style.remove_property(name);
            }
        }
    }
}

/// The touch controls a play starting now plays with.
pub(crate) fn set_choice(choice: ddi_ui::touch_controls::Choice) {
    CHOICE.with(|c| c.set(choice));
}

/// What the play's view places and draws of the touch controls, on a
/// canvas of `css` size: none under the columns, nor while no play runs;
/// shown while touch is the input source.
pub(crate) fn controls_spec(css: (f64, f64)) -> Option<ddi_shell::touch_controls::TouchSpec> {
    let choice = CHOICE.with(|c| c.get());
    if choice.style == ddi_ui::touch_controls::Style::Columns || !PLAYING.with(|p| p.get()) {
        return None;
    }
    Some(ddi_shell::touch_controls::TouchSpec {
        choice,
        insets: safe_insets(css),
        shown: crate::ui_host::kept_source().is_touch(),
        look: crate::perf::touch_look(),
    })
}

/// Where the play's view placed the touch controls this frame.
pub(crate) fn set_controls(controls: Option<ddi_ui::touch_controls::Controls>) {
    CONTROLS.with_borrow_mut(|c| *c = controls);
}

/// Where the touch controls are now (`__DDI_DEBUG.touch`).
pub(crate) fn controls() -> Option<ddi_ui::touch_controls::Controls> {
    CONTROLS.with_borrow(Clone::clone)
}

/// Whether a play runs (the quit button may show over it), and whether
/// a touch screen plays it (it shows throughout).
pub(crate) fn set_quit_shown(playing: bool, touch: bool) {
    PLAYING.with(|p| p.set(playing));
    TOUCH.with(|t| t.set(touch));
    if !playing {
        QUIT_AT.with(|q| q.set(None));
        POLICY_AT.with(|p| p.set(None));
        WATCH_AT.with(|w| w.borrow_mut().clear());
        SKIP_AT.with(|s| s.set(None));
        CONTROLS.with_borrow_mut(|c| *c = None);
        VOLUME_AT.with(|v| v.set(None));
        VOLUME_EDGES.with(|e| e.borrow_mut().clear());
    }
}

/// Where the play drew the music's volume this frame (CSS pixels), for
/// the presses on it.
pub(crate) fn set_volume_at(at: Option<VolumeAt>) {
    VOLUME_AT.with(|v| v.set(at));
}

/// Where the music's volume is drawn now (`__DDI_DEBUG.playVolume`).
pub(crate) fn volume_at() -> Option<VolumeAt> {
    VOLUME_AT.with(|v| v.get())
}

/// The music's volume's pointer edges since the last frame, for the play's
/// view (`ddi_shell::play_view::PlayView::volume_edges`).
pub(crate) fn take_volume_edges() -> Vec<VolumeEdge> {
    VOLUME_EDGES.with(|e| std::mem::take(&mut *e.borrow_mut()))
}

fn volume_edge(phase: VolumePhase, x: f64, y: f64) {
    VOLUME_EDGES.with(|e| {
        e.borrow_mut().push(VolumeEdge {
            phase,
            pos: [x as f32, y as f32],
        })
    });
}

/// The mouse moved over the play at `now_ms`: the quit button shows a
/// moment (`ddi_session::play::quit_shown`).
pub(crate) fn mouse_moved(now_ms: f64) {
    MOUSE_AT.with(|m| m.set(now_ms));
    // A pointer the prompt's screen hid (a key pressed on it) shows again
    // as the mouse moves, as it would over the menus
    // (`crate::ui_host`), which hold the canvas no more while
    // the play runs.
    if PLAYING.with(|p| p.get()) {
        super::screen::mouse_moved();
    }
}

/// Whether the quit button shows at `now_ms`.
pub(crate) fn quit_shown(now_ms: f64) -> bool {
    PLAYING.with(|p| p.get())
        && ddi_session::play::quit_shown(
            TOUCH.with(|t| t.get()),
            Some((now_ms - MOUSE_AT.with(|m| m.get())) / 1000.0),
        )
}

/// Whether a touch screen plays the play that runs (what a key does is
/// then said as what a tap does).
pub(crate) fn is_touch_screen() -> bool {
    TOUCH.with(|t| t.get())
}

/// The safe area's insets for a canvas of `css` size, read again when
/// it changes.
pub(crate) fn safe_insets(css: (f64, f64)) -> [f32; 4] {
    let key = (css.0.round() as u32, css.1.round() as u32);
    INSETS.with(|c| match c.get() {
        Some((k, v)) if k == key => v,
        _ => {
            let v = crate::web::display::safe_insets().map(|v| v as f32);
            c.set(Some((key, v)));
            v
        }
    })
}

/// Where the play drew the quit button this frame (CSS pixels), for the
/// presses on it.
pub(crate) fn set_quit_at(at: Option<[f32; 3]>) {
    QUIT_AT.with(|q| q.set(at));
}

/// Where the quit button is drawn now (CSS pixels: middle and radius),
/// `None` while it is not (`__DDI_DEBUG.quitButton`).
pub(crate) fn quit_at() -> Option<[f32; 3]> {
    QUIT_AT.with(|q| q.get())
}

/// Where the play drew a stage's attack policy label this frame (CSS
/// pixels), for the presses on it (`ddi_shell::battle_cue::policy_area`).
pub(crate) fn set_policy_at(at: Option<[f32; 4]>) {
    POLICY_AT.with(|p| p.set(at));
}

/// Where the attack policy's label takes a press now (CSS pixels: left,
/// top, right, bottom), `None` while it is not shown
/// (`__DDI_DEBUG.battle.attacks.policy_at`).
pub(crate) fn policy_at() -> Option<[f32; 4]> {
    POLICY_AT.with(|p| p.get())
}

/// A watched stage's fields that take a press: each with the seat it
/// follows (none: back to every field) and its place (CSS pixels: left,
/// top, right, bottom).
pub(crate) type WatchFields = Vec<(Option<usize>, [f32; 4])>;

/// Where a watched stage takes presses this frame (CSS pixels): its
/// fields (the play's `watch` control: that competitor is followed, or
/// every field again) and the skip's plate over the field of a player
/// out of the stage (the `skip` control).
pub(crate) fn set_watch_at(fields: WatchFields, skip: Option<[f32; 4]>) {
    WATCH_AT.with(|w| *w.borrow_mut() = fields);
    SKIP_AT.with(|s| s.set(skip));
}

/// Where a watched stage's fields and its skip take a press now
/// (`__DDI_DEBUG.watch`).
pub(crate) fn watch_at() -> (WatchFields, Option<[f32; 4]>) {
    (WATCH_AT.with(|w| w.borrow().clone()), SKIP_AT.with(|s| s.get()))
}

/// The control a press at `(x, y)` (CSS pixels of the canvas) on a
/// watched stage sends, if it lands on its skip or on one of its fields.
fn watch_control(x: f64, y: f64) -> Option<String> {
    let inside = |[l, t, r, b]: [f32; 4]| {
        (f64::from(l)..=f64::from(r)).contains(&x) && (f64::from(t)..=f64::from(b)).contains(&y)
    };
    if SKIP_AT.with(|s| s.get()).is_some_and(inside) {
        return Some(ddi_session::play::TOUCH_SKIP_CONTROL.to_string());
    }
    WATCH_AT.with(|w| {
        w.borrow()
            .iter()
            .find(|(_, r)| inside(*r))
            .map(|(seat, _)| match seat {
                Some(seat) => format!("{}{seat}", ddi_session::play::TOUCH_WATCH_CONTROL),
                None => ddi_session::play::TOUCH_WATCH_ALL.to_string(),
            })
    })
}

/// What a tracked pointer is holding.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Hold {
    Lane(u8),
    Cancel,
    /// A stage's attack policy label.
    Policy,
    /// The music's volume (no control of the play's).
    Volume,
}

struct Shared {
    queue: Vec<RawInput>,
    canvas: HtmlCanvasElement,
    /// Each lane's `(column, scroll sign)` as in the layout.
    lanes: Vec<(f32, i8)>,
    /// Reverse scroll (moves every receptor to the other end).
    reverse: bool,
    /// Where the field is across the screen.
    place: FieldPlace,
    /// Pointers currently down, by `pointerId`.
    pointers: Vec<(i32, Hold)>,
    /// What the listeners and pointer captures are on.
    host: Element,
    /// Pointers holding each lane, then the quit button.
    lane_count: Vec<u32>,
    cancel_count: u32,
    policy_count: u32,
}

pub(crate) struct TouchLanes {
    shared: Rc<RefCell<Shared>>,
    _listeners: Vec<EventListener>,
}

impl TouchLanes {
    /// Listens on the play screen's host (`.game-canvas-host`; the app's
    /// canvas, which draws the quit button, sits in a slot within it),
    /// falling back to the canvas' parent, then the canvas itself. `lanes` are the
    /// layout lanes' `(column, scroll sign)`.
    pub(crate) fn new(
        canvas: HtmlCanvasElement,
        lanes: Vec<(f32, i8)>,
        reverse: bool,
        place: FieldPlace,
    ) -> Option<TouchLanes> {
        let window = web_sys::window()?;
        let document = window.document()?;
        let host: Element = canvas
            .closest(".game-canvas-host")
            .ok()
            .flatten()
            .or_else(|| canvas.parent_element())
            .unwrap_or_else(|| canvas.clone().into());
        let count = lanes.len();
        let shared = Rc::new(RefCell::new(Shared {
            queue: Vec::new(),
            canvas,
            lanes,
            reverse,
            place,
            pointers: Vec::new(),
            host: host.clone(),
            lane_count: vec![0; count],
            cancel_count: 0,
            policy_count: 0,
        }));
        let opts = EventListenerOptions::enable_prevent_default();
        let mut listeners = Vec::new();
        {
            let shared = shared.clone();
            let host2 = host.clone();
            listeners.push(EventListener::new_with_options(
                &host,
                "pointerdown",
                opts,
                move |event| {
                    let Some(event) = event.dyn_ref::<PointerEvent>() else {
                        return;
                    };
                    let target = event.target().and_then(|t| t.dyn_into::<Element>().ok());
                    let within = |sel: &str| {
                        target
                            .as_ref()
                            .and_then(|t| t.closest(sel).ok().flatten())
                            .is_some()
                    };
                    let mut s = shared.borrow_mut();
                    // The music's volume first: its press is its own (any
                    // pointer type); one elsewhere closes its slider and
                    // goes on.
                    if event.button() == 0
                        && let Some(at) = VOLUME_AT.with(|v| v.get())
                    {
                        let rect = s.canvas.get_bounding_client_rect();
                        let x = event.client_x() as f64 - rect.left();
                        let y = event.client_y() as f64 - rect.top();
                        if at.holds([x as f32, y as f32]) {
                            event.prevent_default();
                            let id = event.pointer_id();
                            if !s.pointers.iter().any(|(p, _)| *p == id) {
                                let _ = host2.set_pointer_capture(id);
                                s.pointers.push((id, Hold::Volume));
                                volume_edge(VolumePhase::Down, x, y);
                            }
                            return;
                        }
                        if at.plate.is_some() {
                            volume_edge(VolumePhase::Down, x, y);
                        }
                    }
                    let on_quit = QUIT_AT.with(|q| q.get()).is_some_and(|[x, y, r]| {
                        let rect = s.canvas.get_bounding_client_rect();
                        let dx = event.client_x() as f64 - rect.left() - f64::from(x);
                        let dy = event.client_y() as f64 - rect.top() - f64::from(y);
                        dx * dx + dy * dy <= f64::from(r * r)
                    });
                    let on_policy = !on_quit
                        && POLICY_AT.with(|p| p.get()).is_some_and(|[l, t, r, b]| {
                            let rect = s.canvas.get_bounding_client_rect();
                            let x = event.client_x() as f64 - rect.left();
                            let y = event.client_y() as f64 - rect.top();
                            (f64::from(l)..=f64::from(r)).contains(&x)
                                && (f64::from(t)..=f64::from(b)).contains(&y)
                        });
                    // A watched stage: a press on a field follows it, one
                    // on the skip's plate skips (any pointer type; a
                    // press, with nothing to hold).
                    if !on_quit && !on_policy && event.button() == 0 {
                        let rect = s.canvas.get_bounding_client_rect();
                        let x = event.client_x() as f64 - rect.left();
                        let y = event.client_y() as f64 - rect.top();
                        if let Some(control) = watch_control(x, y) {
                            event.prevent_default();
                            s.queue.push(RawInput {
                                device: DeviceId::Touch,
                                control,
                                pressed: true,
                                host_time: PerformanceClock::from_dom_ms(event.time_stamp()),
                                slot: 0,
                            });
                            return;
                        }
                    }
                    let hold = if on_quit || on_policy {
                        if event.button() != 0 {
                            return;
                        }
                        if on_quit { Hold::Cancel } else { Hold::Policy }
                    } else if !is_touch(event) || within("a, button") {
                        // Mouse clicks press nothing; links and buttons
                        // keep working.
                        return;
                    } else {
                        // A finger: touch is the source, its controls show.
                        crate::ui_host::keep_source(ddi_ui::controller_hints::Source::Touch);
                        match s.lane_at(event.client_x() as f64, event.client_y() as f64, None) {
                            Some(lane) => Hold::Lane(lane),
                            None => return,
                        }
                    };
                    event.prevent_default();
                    let id = event.pointer_id();
                    if s.pointers.iter().any(|(p, _)| *p == id) {
                        return;
                    }
                    let _ = host2.set_pointer_capture(id);
                    s.pointers.push((id, hold));
                    s.press(hold, PerformanceClock::from_dom_ms(event.time_stamp()));
                },
            ));
        }
        {
            let shared = shared.clone();
            listeners.push(EventListener::new(&host, "pointermove", move |event| {
                let Some(event) = event.dyn_ref::<PointerEvent>() else {
                    return;
                };
                let mut s = shared.borrow_mut();
                let id = event.pointer_id();
                let Some(i) = s.pointers.iter().position(|(p, _)| *p == id) else {
                    return;
                };
                if s.pointers[i].1 == Hold::Volume {
                    let rect = s.canvas.get_bounding_client_rect();
                    volume_edge(
                        VolumePhase::Move,
                        event.client_x() as f64 - rect.left(),
                        event.client_y() as f64 - rect.top(),
                    );
                    return;
                }
                let Hold::Lane(old) = s.pointers[i].1 else {
                    return;
                };
                // A finger keeps the row it went down on: sliding only moves
                // it across, so a hold survives drifting past mid-screen.
                let Some(new) =
                    s.lane_at(event.client_x() as f64, event.client_y() as f64, Some(old))
                else {
                    return;
                };
                if new != old {
                    let now = PerformanceClock::from_dom_ms(event.time_stamp());
                    s.pointers[i].1 = Hold::Lane(new);
                    s.release(Hold::Lane(old), now);
                    s.press(Hold::Lane(new), now);
                }
            }));
        }
        // `lostpointercapture` covers a capture dropped without an up or
        // cancel; after a normal up the pointer is already forgotten.
        for name in ["pointerup", "pointercancel", "lostpointercapture"] {
            let shared = shared.clone();
            listeners.push(EventListener::new(&host, name, move |event| {
                let Some(event) = event.dyn_ref::<PointerEvent>() else {
                    return;
                };
                let mut s = shared.borrow_mut();
                let id = event.pointer_id();
                let Some(i) = s.pointers.iter().position(|(p, _)| *p == id) else {
                    return;
                };
                let (_, hold) = s.pointers.swap_remove(i);
                if hold == Hold::Volume {
                    let rect = s.canvas.get_bounding_client_rect();
                    let phase = if name == "pointerup" {
                        VolumePhase::Up
                    } else {
                        VolumePhase::Cancel
                    };
                    volume_edge(
                        phase,
                        event.client_x() as f64 - rect.left(),
                        event.client_y() as f64 - rect.top(),
                    );
                    return;
                }
                s.release(hold, PerformanceClock::from_dom_ms(event.time_stamp()));
            }));
        }
        // The long-press menu would steal the finger mid-hold.
        listeners.push(EventListener::new_with_options(
            &host,
            "contextmenu",
            opts,
            |event| event.prevent_default(),
        ));
        // No up arrives once the page loses focus or is hidden.
        {
            let shared = shared.clone();
            listeners.push(EventListener::new(&window, "blur", move |event| {
                let now = PerformanceClock::from_dom_ms(event.time_stamp());
                shared.borrow_mut().release_all(now);
            }));
        }
        {
            let shared = shared.clone();
            let doc = document.clone();
            listeners.push(EventListener::new(
                &document,
                "visibilitychange",
                move |event| {
                    if doc.hidden() {
                        let now = PerformanceClock::from_dom_ms(event.time_stamp());
                        shared.borrow_mut().release_all(now);
                    }
                },
            ));
        }
        Some(TouchLanes {
            shared,
            _listeners: listeners,
        })
    }
}

fn is_touch(event: &PointerEvent) -> bool {
    matches!(event.pointer_type().as_str(), "touch" | "pen")
}

impl Shared {
    /// The lane under a touch at `(client_x, client_y)` (CSS px in the
    /// viewport): of the lanes whose receptors are at the touched end of the
    /// screen (all of them unless the layout has two rows), the one with the
    /// nearest receptor column, which puts the boundaries at the midpoints.
    /// The receptors come from the renderer's own geometry on the canvas'
    /// backing store (CSS size × devicePixelRatio). With `row_of`, the row
    /// is that lane's instead of the touched half's.
    fn lane_at(&self, client_x: f64, client_y: f64, row_of: Option<u8>) -> Option<u8> {
        let rect = self.canvas.get_bounding_client_rect();
        // The touch controls, where the play placed them.
        let css = [
            (client_x - rect.left()) as f32,
            (client_y - rect.top()) as f32,
        ];
        if let Some(lane) = CONTROLS.with_borrow(|c| {
            c.as_ref()
                .map(|c| c.lane_at(css, row_of.map(usize::from)))
        }) {
            return lane.and_then(|l| u8::try_from(l).ok());
        }
        let dpr = web_sys::window()
            .map(|w| w.device_pixel_ratio())
            .unwrap_or(1.0);
        let span = self.lanes.iter().map(|l| l.0).fold(0.0f32, f32::max) + 1.0;
        let geo = FieldGeometry::placed(
            (rect.width() * dpr).round().max(1.0) as f32,
            (rect.height() * dpr).round().max(1.0) as f32,
            span,
            self.reverse,
            self.place,
        );
        let x = ((client_x - rect.left()) * dpr) as f32;
        let y = ((client_y - rect.top()) * dpr) as f32;
        let at_top = |sign: i8| geo.lane_receptor_y(sign) < geo.height / 2.0;
        let two_rows = self.lanes.iter().any(|l| l.1 < 0) && self.lanes.iter().any(|l| l.1 > 0);
        let touched_top = match row_of.and_then(|l| self.lanes.get(usize::from(l))) {
            Some(l) => at_top(l.1),
            None => y < geo.height / 2.0,
        };
        self.lanes
            .iter()
            .enumerate()
            .filter(|(_, l)| !two_rows || at_top(l.1) == touched_top)
            .min_by(|(_, a), (_, b)| {
                (geo.lane_x(a.0) - x)
                    .abs()
                    .total_cmp(&(geo.lane_x(b.0) - x).abs())
            })
            .map(|(lane, _)| lane as u8)
    }

    fn counter(&mut self, hold: Hold) -> Option<&mut u32> {
        match hold {
            Hold::Lane(l) => self.lane_count.get_mut(l as usize),
            Hold::Cancel => Some(&mut self.cancel_count),
            Hold::Policy => Some(&mut self.policy_count),
            Hold::Volume => None,
        }
    }

    /// One more pointer on `hold`; the first one presses it.
    fn press(&mut self, hold: Hold, at: HostTime) {
        let Some(n) = self.counter(hold) else {
            return;
        };
        *n += 1;
        if *n == 1 {
            self.emit(hold, true, at);
        }
    }

    /// One pointer fewer on `hold`; the last one releases it.
    fn release(&mut self, hold: Hold, at: HostTime) {
        let Some(n) = self.counter(hold) else {
            return;
        };
        if *n == 0 {
            return;
        }
        *n -= 1;
        if *n == 0 {
            self.emit(hold, false, at);
        }
    }

    fn emit(&mut self, hold: Hold, pressed: bool, host_time: HostTime) {
        let control = match hold {
            Hold::Lane(l) => format!("lane:{l}"),
            Hold::Cancel => CANCEL_CONTROL.to_string(),
            Hold::Policy => ddi_session::play::TOUCH_POLICY_CONTROL.to_string(),
            Hold::Volume => return,
        };
        self.queue.push(RawInput {
            device: DeviceId::Touch,
            control,
            pressed,
            host_time,
            slot: 0,
        });
    }

    /// Forget every pointer, emitting releases stamped `now`.
    fn release_all(&mut self, now: HostTime) {
        for (_, hold) in std::mem::take(&mut self.pointers) {
            if hold == Hold::Volume {
                // A drag cut off keeps the level it reached.
                volume_edge(VolumePhase::Cancel, 0.0, 0.0);
            }
            self.release(hold, now);
        }
    }
}

impl InputSource for TouchLanes {
    fn poll(&mut self, out: &mut Vec<RawInput>) {
        out.append(&mut self.shared.borrow_mut().queue);
    }
}

impl Drop for TouchLanes {
    /// Fingers still down when the session ends must not leave a pointer
    /// captured by the host (the results overlay lives in it).
    fn drop(&mut self) {
        // Collected first: releasing a capture may dispatch
        // `lostpointercapture` into listeners that borrow `shared`.
        let (host, ids): (_, Vec<i32>) = {
            let s = self.shared.borrow();
            (
                Some(s.host.clone()),
                s.pointers.iter().map(|(id, _)| *id).collect(),
            )
        };
        if let Some(host) = host {
            for id in ids {
                let _ = host.release_pointer_capture(id);
            }
        }
    }
}
