//! Gamepad input source.
//!
//! The Gamepad API has no button events, so pads are polled. Polling once per
//! animation frame adds up to a frame of latency and jitter, so during play a
//! `MessageChannel` ping-pong wakes the main thread continuously and polls
//! whenever at least [`FAST_INTERVAL_MS`] has passed (`setTimeout` is clamped
//! to 4 ms when nested; message tasks are not). The loop only runs while a
//! pad is connected, fast polling is on and the tab is visible; a rAF loop
//! polls once per frame regardless, which finds new pads and restarts the
//! fast loop after the tab was hidden. With no pad connected the frame
//! loop rests (P13): the pads are looked for twice a second instead, and at
//! once when the browser says one connected (`gamepadconnected`).
//!
//! Reading the pads happens in a small JS function that packs every pad into
//! one `Float64Array`, so a poll is one call into JS instead of dozens of
//! property reads. Edge detection and timestamps are
//! [`ddi_platform::gamepad::PadTracker`]'s job.

use std::cell::RefCell;
use std::collections::BTreeMap;
use std::rc::{Rc, Weak};

use ddi_platform::gamepad::{Control, PadSnapshot, PadStats, PadTracker};
use ddi_platform::{HostTime, InputSource, RawInput};
use gloo::render::{AnimationFrame, request_animation_frame};
use js_sys::Float64Array;
use wasm_bindgen::prelude::*;

use super::clock::PerformanceClock;

/// Minimum time between polls of the fast loop.
const FAST_INTERVAL_MS: f64 = 1.0;
/// How often pads are looked for while none is connected.
const IDLE_POLL_MS: u32 = 500;
/// Floats in the shared read buffer: enough for several pads with the
/// largest button and axis counts browsers report.
const BUFFER_LEN: u32 = 4096;

#[wasm_bindgen(inline_js = r#"
let ids = [];
let generation = 0;

// Packs the connected pads into `buf`: per pad [index, timestamp, standard,
// buttonCount, axisCount, buttons (pressed 0/1)…, axes…]. Returns the number
// of floats written.
export function ddiReadPads(buf) {
    let pads;
    try {
        pads = navigator.getGamepads ? navigator.getGamepads() : [];
    } catch (e) {
        return 0;
    }
    let o = 0;
    for (let i = 0; i < pads.length; i++) {
        const p = pads[i];
        if (!p || p.connected === false) continue;
        const buttons = p.buttons || [];
        const axes = p.axes || [];
        if (o + 5 + buttons.length + axes.length > buf.length) break;
        if (ids[p.index] !== p.id) {
            ids[p.index] = p.id;
            generation++;
        }
        buf[o++] = p.index;
        buf[o++] = p.timestamp || 0;
        buf[o++] = p.mapping === "standard" ? 1 : 0;
        buf[o++] = buttons.length;
        buf[o++] = axes.length;
        for (let b = 0; b < buttons.length; b++) {
            const x = buttons[b];
            buf[o++] = (typeof x === "object" ? x.pressed : x > 0.5) ? 1 : 0;
        }
        for (let a = 0; a < axes.length; a++) {
            const v = axes[a];
            buf[o++] = typeof v === "number" ? v : 0;
        }
    }
    return o;
}

export function ddiPadGeneration() {
    return generation;
}

export function ddiPadId(index) {
    return ids[index] || "";
}

// Calls `tick(now)` every `interval` ms, alternating a message task and a
// timeout so the browser never clamps the timeout to 4 ms and the thread
// sleeps in between. Stops by itself when the document is hidden.
export class DdiPadPump {
    constructor(tick, interval) {
        this.tick = tick;
        this.interval = interval;
        this.running = false;
        this.pending = false;
        this.last = 0;
        this.channel = new MessageChannel();
        this.channel.port1.onmessage = () => this.wake();
        this.post = () => this.channel.port2.postMessage(0);
    }
    start() {
        if (this.running || document.hidden) return;
        this.running = true;
        // A wake still pending from before a stop() carries on; posting
        // another would run two chains.
        if (!this.pending) {
            this.pending = true;
            this.channel.port2.postMessage(0);
        }
    }
    stop() {
        this.running = false;
    }
    isRunning() {
        return this.running;
    }
    wake() {
        this.pending = false;
        if (!this.running) return;
        if (document.hidden) {
            this.running = false;
            return;
        }
        let now = performance.now();
        if (now - this.last >= this.interval) {
            this.last = now;
            this.tick(now);
            now = performance.now();
        }
        if (!this.running) return;
        // Sleep until the next poll is due. A timeout set from a message
        // task has nesting level 1, so it is not clamped to 4 ms; its
        // callback posts a message, which resets the nesting again.
        const wait = this.interval - (now - this.last);
        this.pending = true;
        setTimeout(this.post, wait > 0 ? wait : 0);
    }
    close() {
        this.running = false;
        this.channel.port1.onmessage = null;
        this.channel.port1.close();
        this.channel.port2.close();
    }
}
"#)]
extern "C" {
    #[wasm_bindgen(js_name = ddiReadPads)]
    fn read_pads(buf: &Float64Array) -> u32;
    #[wasm_bindgen(js_name = ddiPadGeneration)]
    fn pad_generation() -> f64;
    #[wasm_bindgen(js_name = ddiPadId)]
    fn pad_id(index: u32) -> String;

    type DdiPadPump;
    #[wasm_bindgen(constructor)]
    fn new(tick: &js_sys::Function, interval: f64) -> DdiPadPump;
    #[wasm_bindgen(method)]
    fn start(this: &DdiPadPump);
    #[wasm_bindgen(method)]
    fn stop(this: &DdiPadPump);
    #[wasm_bindgen(method, js_name = isRunning)]
    fn is_running(this: &DdiPadPump) -> bool;
    #[wasm_bindgen(method)]
    fn close(this: &DdiPadPump);
}

/// A connected pad, for the UI and the debug overlay.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct PadInfo {
    pub(crate) index: u32,
    pub(crate) id: String,
    pub(crate) standard: bool,
    pub(crate) stats: PadStats,
    /// Controls held right now.
    pub(crate) active: Vec<Control>,
}

struct Slot {
    tracker: PadTracker,
    id: String,
    standard: bool,
}

/// Called synchronously from the poll that saw an edge (so it may start
/// gesture-gated APIs where the browser treats pad presses as a gesture).
type Listener = Box<dyn FnMut(&RawInput)>;

struct State {
    buf: Float64Array,
    scratch: Vec<f64>,
    /// The pads the last poll read, by index.
    seen: Vec<u32>,
    generation: f64,
    slots: BTreeMap<u32, Slot>,
    snapshot: PadSnapshot,
    queue: Vec<RawInput>,
    listener: Option<Listener>,
    /// Whether edges are queued for [`InputSource::poll`] (off where a
    /// listener takes them all and nothing drains the queue).
    queue_edges: bool,
    /// When this hub started, on the performance timeline.
    created: HostTime,
    fast: bool,
    /// Polls in the current one-second window, and the last full count.
    window_start: f64,
    window_polls: u32,
    polls_per_second: u32,
}

impl State {
    /// Reads every pad at `now_ms` and queues the edges; returns how many
    /// edges were added.
    fn poll(&mut self, now_ms: f64) -> usize {
        let before = self.queue.len();
        let now = HostTime(now_ms / 1000.0);
        let n = read_pads(&self.buf) as usize;
        self.scratch.resize(n, 0.0);
        if n > 0 {
            self.buf.subarray(0, n as u32).copy_to(&mut self.scratch);
        }
        let generation = pad_generation();
        let ids_changed = generation != self.generation;
        self.generation = generation;

        self.seen.clear();
        let mut o = 0;
        while o + 5 <= n {
            let index = self.scratch[o] as u32;
            let nb = self.scratch[o + 3] as usize;
            let na = self.scratch[o + 4] as usize;
            if o + 5 + nb + na > n {
                break;
            }
            let snap = &mut self.snapshot;
            snap.index = index;
            snap.timestamp = HostTime(self.scratch[o + 1] / 1000.0);
            snap.standard = self.scratch[o + 2] != 0.0;
            snap.buttons.clear();
            snap.buttons
                .extend(self.scratch[o + 5..o + 5 + nb].iter().map(|v| *v != 0.0));
            snap.axes.clear();
            snap.axes
                .extend_from_slice(&self.scratch[o + 5 + nb..o + 5 + nb + na]);
            o += 5 + nb + na;
            self.seen.push(index);

            let known = self.slots.get(&index).map(|s| s.id.as_str());
            match known {
                Some(id) if !ids_changed => {
                    snap.id.clear();
                    snap.id.push_str(id);
                }
                _ => snap.id = pad_id(index),
            }
            if known != Some(snap.id.as_str()) {
                // A different pad took the slot.
                if let Some(mut old) = self.slots.remove(&index) {
                    old.tracker.release_all(now, &mut self.queue);
                }
                self.slots.insert(
                    index,
                    Slot {
                        // A button still held from the page before is not
                        // a press here.
                        tracker: PadTracker::for_slot(&snap.id, snap.index)
                            .held_before(self.created),
                        id: snap.id.clone(),
                        standard: snap.standard,
                    },
                );
            }
            if let Some(slot) = self.slots.get_mut(&index) {
                slot.standard = snap.standard;
                slot.tracker.update(snap, now, &mut self.queue);
            }
        }
        // The pads gone, released in index order.
        let (seen, queue) = (&self.seen, &mut self.queue);
        self.slots.retain(|k, slot| {
            let kept = seen.contains(k);
            if !kept {
                slot.tracker.release_all(now, queue);
            }
            kept
        });

        if now_ms - self.window_start >= 1000.0 {
            self.polls_per_second = self.window_polls;
            self.window_polls = 0;
            self.window_start = now_ms;
        }
        self.window_polls += 1;
        self.queue.len() - before
    }
}

/// Polls `state` and hands the new edges to the listener, outside the
/// borrow so the listener may call back into the hub.
fn poll_and_notify(state: &Rc<RefCell<State>>, now_ms: f64) {
    let (fresh, mut listener) = {
        let mut s = state.borrow_mut();
        let added = s.poll(now_ms);
        if added == 0 || s.listener.is_none() {
            return;
        }
        let start = s.queue.len() - added;
        (s.queue[start..].to_vec(), s.listener.take())
    };
    if let Some(l) = listener.as_mut() {
        for edge in &fresh {
            l(edge);
        }
    }
    let mut s = state.borrow_mut();
    if s.listener.is_none() {
        s.listener = listener;
    }
    if !s.queue_edges {
        s.queue.clear();
    }
}

struct Hub {
    state: Rc<RefCell<State>>,
    pump: DdiPadPump,
    _tick: Closure<dyn FnMut(f64)>,
    raf: Rc<RefCell<Option<AnimationFrame>>>,
    /// The look for pads while none is connected, and what cuts it short.
    idle: RefCell<Option<gloo::timers::callback::Timeout>>,
    connected: RefCell<Option<gloo::events::EventListener>>,
}

impl Drop for Hub {
    fn drop(&mut self) {
        self.pump.close();
        self.raf.borrow_mut().take();
        self.idle.borrow_mut().take();
        self.connected.borrow_mut().take();
    }
}

thread_local! {
    /// The pads of the page's UI ([`Gamepads::for_page`]).
    static PAGE: RefCell<Weak<Hub>> = const { RefCell::new(Weak::new()) };
}

/// The pads connected to the page's UI, as a play reads them (the chart
/// editor's recording binds its lanes with them).
pub(crate) fn page_pads() -> Vec<ddi_engine::ConnectedPad> {
    let Some(hub) = PAGE.with(|p| p.borrow().upgrade()) else {
        return Vec::new();
    };
    Gamepads(hub)
        .pads()
        .into_iter()
        .map(|p| ddi_engine::ConnectedPad {
            id: p.id,
            index: p.index,
            standard: p.standard,
        })
        .collect()
}

/// Polls every gamepad; cheap to clone (shared state).
#[derive(Clone)]
pub(crate) struct Gamepads(Rc<Hub>);

/// Clones of one hub are equal (so a hub can be a component property).
impl PartialEq for Gamepads {
    fn eq(&self, other: &Gamepads) -> bool {
        Rc::ptr_eq(&self.0, &other.0)
    }
}

impl Gamepads {
    /// `None` where the Gamepad API is missing.
    pub(crate) fn new() -> Option<Gamepads> {
        let navigator = web_sys::window()?.navigator();
        if !js_sys::Reflect::has(&navigator, &JsValue::from_str("getGamepads")).unwrap_or(false) {
            return None;
        }
        let state = Rc::new(RefCell::new(State {
            buf: Float64Array::new_with_length(BUFFER_LEN),
            scratch: Vec::new(),
            seen: Vec::new(),
            generation: -1.0,
            slots: BTreeMap::new(),
            snapshot: PadSnapshot::default(),
            queue: Vec::new(),
            listener: None,
            queue_edges: true,
            created: HostTime(
                web_sys::window()
                    .and_then(|w| w.performance())
                    .map_or(0.0, |p| p.now() / 1000.0),
            ),
            fast: false,
            window_start: 0.0,
            window_polls: 0,
            polls_per_second: 0,
        }));
        let tick = {
            let weak: Weak<RefCell<State>> = Rc::downgrade(&state);
            Closure::<dyn FnMut(f64)>::new(move |now_ms: f64| {
                if let Some(state) = weak.upgrade() {
                    poll_and_notify(&state, now_ms);
                }
            })
        };
        let pump = DdiPadPump::new(tick.as_ref().unchecked_ref(), FAST_INTERVAL_MS);
        let hub = Rc::new(Hub {
            state,
            pump,
            _tick: tick,
            raf: Rc::new(RefCell::new(None)),
            idle: RefCell::new(None),
            connected: RefCell::new(None),
        });
        if let Some(window) = web_sys::window() {
            let weak = Rc::downgrade(&hub);
            *hub.connected.borrow_mut() = Some(gloo::events::EventListener::new(
                &window,
                "gamepadconnected",
                move |_| {
                    // Resting: look now.
                    if let Some(hub) = weak.upgrade()
                        && hub.idle.borrow_mut().take().is_some()
                    {
                        schedule_frame(Rc::downgrade(&hub));
                    }
                },
            ));
        }
        schedule_frame(Rc::downgrade(&hub));
        Some(Gamepads(hub))
    }

    /// Fast (1 ms) polling while a pad is connected, or per frame only.
    pub(crate) fn set_fast(&self, fast: bool) {
        self.0.state.borrow_mut().fast = fast;
        self.0.update_pump();
    }

    /// Edge callback, run synchronously inside the poll that saw the edge.
    pub(crate) fn set_listener(&self, listener: Option<Listener>) {
        self.0.state.borrow_mut().listener = listener;
    }

    /// Whether edges are queued for [`InputSource::poll`] (on by default).
    pub(crate) fn set_queue(&self, on: bool) {
        let mut s = self.0.state.borrow_mut();
        s.queue_edges = on;
        if !on {
            s.queue.clear();
        }
    }

    /// Drops queued edges (e.g. presses made before a session started).
    pub(crate) fn clear(&self) {
        self.0.state.borrow_mut().queue.clear();
    }

    /// These are the page's pads ([`page_pads`]).
    pub(crate) fn for_page(&self) {
        PAGE.with(|p| *p.borrow_mut() = Rc::downgrade(&self.0));
    }

    pub(crate) fn pads(&self) -> Vec<PadInfo> {
        self.0
            .state
            .borrow()
            .slots
            .iter()
            .map(|(index, s)| PadInfo {
                index: *index,
                id: s.id.clone(),
                standard: s.standard,
                stats: s.tracker.stats(),
                active: s.tracker.active(),
            })
            .collect()
    }

    pub(crate) fn polls_per_second(&self) -> u32 {
        self.0.state.borrow().polls_per_second
    }

    pub(crate) fn fast_running(&self) -> bool {
        self.0.pump.is_running()
    }
}

impl Hub {
    fn update_pump(&self) {
        let (fast, any) = {
            let s = self.state.borrow();
            (s.fast, !s.slots.is_empty())
        };
        if fast && any {
            self.pump.start();
        } else {
            self.pump.stop();
        }
    }
}

fn schedule_frame(hub: Weak<Hub>) {
    let Some(strong) = hub.upgrade() else {
        return;
    };
    let raf = strong.raf.clone();
    drop(strong);
    let handle = request_animation_frame(move |_| {
        let Some(hub) = hub.upgrade() else {
            return;
        };
        // Not the rAF timestamp: that is the frame's start, possibly
        // milliseconds before this callback runs, and would stamp edges
        // early.
        poll_and_notify(&hub.state, PerformanceClock::now_ms());
        hub.update_pump();
        let weak = Rc::downgrade(&hub);
        if hub.state.borrow().slots.is_empty() {
            idle_poll(weak);
            return;
        }
        drop(hub);
        schedule_frame(weak);
    });
    *raf.borrow_mut() = Some(handle);
}

/// No pad: no frames for them (P13); a timer looks for one now and then
/// and goes back to polling on frames once one is there.
fn idle_poll(hub: Weak<Hub>) {
    let Some(strong) = hub.upgrade() else {
        return;
    };
    let again = hub.clone();
    *strong.idle.borrow_mut() = Some(gloo::timers::callback::Timeout::new(
        IDLE_POLL_MS,
        move || {
            let Some(hub) = again.upgrade() else {
                return;
            };
            hub.idle.borrow_mut().take();
            poll_and_notify(&hub.state, PerformanceClock::now_ms());
            hub.update_pump();
            let weak = Rc::downgrade(&hub);
            let none = hub.state.borrow().slots.is_empty();
            drop(hub);
            if none {
                idle_poll(weak);
            } else {
                schedule_frame(weak);
            }
        },
    ));
}

impl InputSource for Gamepads {
    fn poll(&mut self, out: &mut Vec<RawInput>) {
        out.append(&mut self.0.state.borrow_mut().queue);
    }
}
