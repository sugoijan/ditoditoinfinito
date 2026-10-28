//! Bridge to the boot shell defined inline in `index.html`. All calls are
//! best-effort: if the shell is missing (tests) they silently no-op.

use std::cell::Cell;

use js_sys::{Function, Reflect};
use wasm_bindgen::{JsCast, JsValue};

thread_local! {
    static READY_SENT: Cell<bool> = const { Cell::new(false) };
    /// [`ready_when_settled`] was asked: the UI host drawing fades the
    /// shell once its screen is at rest.
    static WAITING: Cell<bool> = const { Cell::new(false) };
}

fn with_boot<F: FnOnce(&js_sys::Object)>(action: F) {
    let Some(window) = web_sys::window() else {
        return;
    };
    let Ok(value) = Reflect::get(&window, &JsValue::from_str("__DDI_BOOT")) else {
        return;
    };
    if value.is_null() || value.is_undefined() {
        return;
    }
    let Ok(obj) = value.dyn_into::<js_sys::Object>() else {
        return;
    };
    action(&obj);
}

fn call(method: &str, args: &[JsValue]) {
    with_boot(|boot| {
        let Ok(value) = Reflect::get(boot, &JsValue::from_str(method)) else {
            return;
        };
        let Ok(func) = value.dyn_into::<Function>() else {
            return;
        };
        let array = js_sys::Array::new();
        for arg in args {
            array.push(arg);
        }
        let _ = func.apply(boot, &array);
    });
}

/// Reports a Rust panic to the boot shell, which owns recovery (overlay +
/// reload). The wasm instance is unusable once this is called.
pub(crate) fn crash(message: &str) {
    call("crash", &[JsValue::from_str(message)]);
}

/// How far the game readied what its first screen needs, 0 to 1 (the
/// bar's part after the download).
pub(crate) fn progress(ratio: f64) {
    call("setStartProgress", &[JsValue::from_f64(ratio)]);
}

/// Fades out the boot shell. Idempotent.
pub(crate) fn ready() {
    let already_sent = READY_SENT.with(|flag| flag.replace(true));
    if already_sent {
        return;
    }
    WAITING.with(|w| w.set(false));
    call("ready", &[]);
}

/// Fades out the boot shell once the screen under it is at rest: the
/// shell's title stands where the start menu's lands, on the same
/// clock, so the fade shows the menu in place rather than its title
/// sliding in under a title. Where no UI host draws (a plain page, a
/// play), now; and never later than a moment after, should the screen
/// keep moving.
pub(crate) fn ready_when_settled() {
    if READY_SENT.with(Cell::get) {
        return;
    }
    if crate::components::ui_dev::hosts() == 0 {
        ready();
        return;
    }
    WAITING.with(|w| w.set(true));
    gloo::timers::callback::Timeout::new(1500, ready).forget();
}

/// Whether [`ready_when_settled`] waits for a settled screen
/// (`ui_dev`'s frame calls [`ready`] then).
pub(crate) fn waiting() -> bool {
    WAITING.with(Cell::get)
}

/// Whether the shell is still up, its title showing: a start menu built
/// now comes up under it (`StartScreen::in_place`).
pub(crate) fn shown() -> bool {
    !READY_SENT.with(Cell::get)
}
