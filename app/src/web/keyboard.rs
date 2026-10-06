//! Keyboard input source. `keydown`/`keyup` on the window are stamped with
//! `Event.timeStamp` (performance timeline), repeats are dropped, and a
//! pressed-set de-duplicates keys the OS reports twice. Losing focus
//! (`blur`) releases every held key, since the matching `keyup` will never
//! arrive.

use std::cell::RefCell;
use std::collections::HashSet;
use std::rc::Rc;

use ddi_platform::{DeviceId, HostTime, InputSource, RawInput};
use gloo::events::{EventListener, EventListenerOptions};
use wasm_bindgen::JsCast;
use web_sys::KeyboardEvent;

use super::clock::PerformanceClock;

#[derive(Default)]
struct Shared {
    queue: Vec<RawInput>,
    pressed: HashSet<String>,
    /// Codes whose default action is suppressed while this source is active.
    capture: HashSet<String>,
}

pub(crate) struct WebKeyboard {
    shared: Rc<RefCell<Shared>>,
    _down: EventListener,
    _up: EventListener,
    _blur: EventListener,
}

impl WebKeyboard {
    /// `capture` lists `KeyboardEvent.code` values whose default (page scroll
    /// on arrows/space) is prevented.
    pub(crate) fn new(capture: impl IntoIterator<Item = String>) -> Option<WebKeyboard> {
        let window = web_sys::window()?;
        let shared = Rc::new(RefCell::new(Shared {
            capture: capture.into_iter().collect(),
            ..Shared::default()
        }));
        let opts = EventListenerOptions::enable_prevent_default();
        let down = {
            let shared = shared.clone();
            EventListener::new_with_options(&window, "keydown", opts, move |event| {
                let Some(event) = event.dyn_ref::<KeyboardEvent>() else {
                    return;
                };
                let code = event.code();
                let mut s = shared.borrow_mut();
                if s.capture.contains(&code) {
                    event.prevent_default();
                }
                if event.repeat() || !s.pressed.insert(code.clone()) {
                    return;
                }
                s.queue.push(RawInput {
                    device: DeviceId::Keyboard,
                    control: code,
                    pressed: true,
                    host_time: PerformanceClock::from_dom_ms(event.time_stamp()),
                });
            })
        };
        let up = {
            let shared = shared.clone();
            EventListener::new_with_options(&window, "keyup", opts, move |event| {
                let Some(event) = event.dyn_ref::<KeyboardEvent>() else {
                    return;
                };
                let code = event.code();
                let mut s = shared.borrow_mut();
                if s.capture.contains(&code) {
                    event.prevent_default();
                }
                if !s.pressed.remove(&code) {
                    return;
                }
                s.queue.push(RawInput {
                    device: DeviceId::Keyboard,
                    control: code,
                    pressed: false,
                    host_time: PerformanceClock::from_dom_ms(event.time_stamp()),
                });
            })
        };
        let blur = {
            let shared = shared.clone();
            EventListener::new(&window, "blur", move |event| {
                let now = PerformanceClock::from_dom_ms(event.time_stamp());
                Shared::release_all(&mut shared.borrow_mut(), now);
            })
        };
        Some(WebKeyboard {
            shared,
            _down: down,
            _up: up,
            _blur: blur,
        })
    }
}

impl Shared {
    /// Forget every held key, emitting releases stamped `now`.
    fn release_all(&mut self, now: HostTime) {
        let codes: Vec<String> = self.pressed.drain().collect();
        for code in codes {
            self.queue.push(RawInput {
                device: DeviceId::Keyboard,
                control: code,
                pressed: false,
                host_time: now,
            });
        }
    }
}

impl InputSource for WebKeyboard {
    fn poll(&mut self, out: &mut Vec<RawInput>) {
        out.append(&mut self.shared.borrow_mut().queue);
    }
}
