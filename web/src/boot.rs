//! Bridge to the boot shell defined inline in `index.html`. All calls are
//! best-effort: if the shell is missing (tests) they silently no-op.

use std::cell::Cell;

use js_sys::{Function, Reflect};
use wasm_bindgen::{JsCast, JsValue};

thread_local! {
    static READY_SENT: Cell<bool> = const { Cell::new(false) };
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

#[allow(dead_code)]
pub(crate) fn set_progress(value: f32) {
    call("setProgress", &[JsValue::from_f64(value as f64)]);
}

#[allow(dead_code)]
pub(crate) fn fail(code: &str, message: &str, hint: &str) {
    call(
        "fail",
        &[
            JsValue::from_str(code),
            JsValue::from_str(message),
            JsValue::from_str(hint),
        ],
    );
}

/// Reports a Rust panic to the boot shell, which owns recovery (overlay +
/// reload). The wasm instance is unusable once this is called.
pub(crate) fn crash(message: &str) {
    call("crash", &[JsValue::from_str(message)]);
}

/// Fades out the boot shell. Idempotent.
pub(crate) fn ready() {
    let already_sent = READY_SENT.with(|flag| flag.replace(true));
    if already_sent {
        return;
    }
    call("ready", &[]);
}
