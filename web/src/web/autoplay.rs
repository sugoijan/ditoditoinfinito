//! When audio may start. Browsers start an `AudioContext` (make one
//! running, or resume one) only within a user activation, or once the
//! page is allowed to (Chrome after any input on the page; Firefox says
//! with `navigator.getAutoplayPolicy`). Asked outside them, Firefox logs
//! "An AudioContext was prevented from starting automatically" and the
//! context waits, suspended, for an activation: none is asked for then.

use wasm_bindgen::{JsCast, JsValue};
use web_sys::{Event, KeyboardEvent};

/// The page's `navigator.userActivation`, where the browser has one.
fn user_activation() -> Option<JsValue> {
    let nav = web_sys::window()?.navigator();
    js_sys::Reflect::get(&nav, &"userActivation".into())
        .ok()
        .filter(JsValue::is_object)
}

fn flag(o: &JsValue, name: &str) -> Option<bool> {
    js_sys::Reflect::get(o, &name.into()).ok()?.as_bool()
}

/// Whether event `e`, being handled, is a user activation (audio may start
/// in its handler): the browser's word (`userActivation.isActive`) where
/// it has one, else the event's kind (a key but Escape and the modifiers
/// alone, a click, a press). A modifier, Escape, a wheel are not.
pub(crate) fn activated(e: &Event) -> bool {
    if let Some(ua) = user_activation()
        && let Some(active) = flag(&ua, "isActive")
    {
        return active;
    }
    if let Some(k) = e.dyn_ref::<KeyboardEvent>() {
        return !matches!(
            k.key().as_str(),
            "Escape"
                | "Shift"
                | "Control"
                | "Alt"
                | "AltGraph"
                | "Meta"
                | "Super"
                | "Hyper"
                | "OS"
                | "Fn"
                | "FnLock"
                | "Symbol"
                | "CapsLock"
                | "NumLock"
                | "ScrollLock"
        );
    }
    matches!(
        e.type_().as_str(),
        "click" | "mousedown" | "pointerdown" | "pointerup" | "touchend"
    )
}

/// Whether an audio context may start now, outside an activation: Firefox
/// says (`navigator.getAutoplayPolicy("audiocontext")`); elsewhere, once
/// the page has had the player's input (Chrome's rule; Safari may still
/// want a fresh one, and gets the next).
pub(crate) fn may_start() -> bool {
    if let Some(nav) = web_sys::window().map(|w| w.navigator())
        && let Ok(f) = js_sys::Reflect::get(&nav, &"getAutoplayPolicy".into())
        && let Some(f) = f.dyn_ref::<js_sys::Function>()
    {
        return f
            .call1(&nav, &"audiocontext".into())
            .ok()
            .and_then(|v| v.as_string())
            .is_some_and(|p| p == "allowed");
    }
    user_activation().and_then(|ua| flag(&ua, "hasBeenActive")) == Some(true)
}
