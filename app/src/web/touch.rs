//! Touch lanes for phones and tablets: the play screen split into
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
//! A press on the `.touch-quit` button (any pointer type) is the `cancel`
//! control instead, which the play session treats like Escape (hold or
//! double tap to quit). Touch is not a sync-accuracy target: event
//! timestamps are as good as the browser's touch pipeline.

use std::cell::RefCell;
use std::rc::Rc;

use ddi_platform::{DeviceId, HostTime, InputSource, RawInput};
use ddi_render::FieldGeometry;
use gloo::events::{EventListener, EventListenerOptions};
use wasm_bindgen::JsCast;
use web_sys::{Element, HtmlCanvasElement, PointerEvent};

use super::clock::PerformanceClock;

/// Control emitted by the quit button.
pub(crate) const CANCEL_CONTROL: &str = "cancel";
/// Class of the on-screen quit button.
const QUIT_SELECTOR: &str = ".touch-quit";

/// What a tracked pointer is holding.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Hold {
    Lane(u8),
    Cancel,
}

struct Shared {
    queue: Vec<RawInput>,
    canvas: HtmlCanvasElement,
    /// Each lane's `(column, scroll sign)` as in the layout.
    lanes: Vec<(f32, i8)>,
    /// Reverse scroll (moves every receptor to the other end).
    reverse: bool,
    /// Pointers currently down, by `pointerId`.
    pointers: Vec<(i32, Hold)>,
    /// Pointers holding each lane, then the quit button.
    lane_count: Vec<u32>,
    cancel_count: u32,
}

pub(crate) struct TouchLanes {
    shared: Rc<RefCell<Shared>>,
    _listeners: Vec<EventListener>,
}

impl TouchLanes {
    /// Listens on the canvas' parent (the play screen host, which also holds
    /// the quit button), falling back to the canvas itself. `lanes` are the
    /// layout lanes' `(column, scroll sign)`.
    pub(crate) fn new(
        canvas: HtmlCanvasElement,
        lanes: Vec<(f32, i8)>,
        reverse: bool,
    ) -> Option<TouchLanes> {
        let window = web_sys::window()?;
        let document = window.document()?;
        let host: Element = canvas
            .parent_element()
            .unwrap_or_else(|| canvas.clone().into());
        let count = lanes.len();
        let shared = Rc::new(RefCell::new(Shared {
            queue: Vec::new(),
            canvas,
            lanes,
            reverse,
            pointers: Vec::new(),
            lane_count: vec![0; count],
            cancel_count: 0,
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
                    let hold = if within(QUIT_SELECTOR) {
                        if event.button() != 0 {
                            return;
                        }
                        Hold::Cancel
                    } else if !is_touch(event) || within("a, button") {
                        // Mouse clicks press nothing; links and buttons
                        // keep working.
                        return;
                    } else {
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
        let dpr = web_sys::window()
            .map(|w| w.device_pixel_ratio())
            .unwrap_or(1.0);
        let span = self.lanes.iter().map(|l| l.0).fold(0.0f32, f32::max) + 1.0;
        let geo = FieldGeometry::new(
            (rect.width() * dpr).round().max(1.0) as f32,
            (rect.height() * dpr).round().max(1.0) as f32,
            span,
            self.reverse,
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
                s.canvas.parent_element(),
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
