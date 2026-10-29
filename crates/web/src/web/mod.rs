//! Web implementations of the `ddi-platform` traits plus the wgpu canvas
//! bootstrap, the app's one canvas (`screen`) and what the 2D pass needs
//! of the browser (`text`, `textures`). Nothing in here knows about
//! gameplay.

pub(crate) mod audio;
pub(crate) mod autoplay;
pub(crate) mod clock;
pub(crate) mod devices;
pub(crate) mod display;
pub(crate) mod files;
pub(crate) mod folders;
pub(crate) mod gamepad;
pub(crate) mod gfx;
pub(crate) mod idb;
pub(crate) mod image;
pub(crate) mod keyboard;
pub(crate) mod kv;
pub(crate) mod movie_source;
pub(crate) mod opus;
pub(crate) mod screen;
pub(crate) mod storage;
pub(crate) mod svg;
pub(crate) mod text;
pub(crate) mod textures;
pub(crate) mod touch;
pub(crate) mod video;
pub(crate) mod webcodecs;

use wasm_bindgen::JsValue;

pub(crate) fn js_err(context: &str, err: JsValue) -> String {
    format!("{context}: {err:?}")
}

/// `value` as a plain JS value, through JSON (`null` when it is not one).
pub(crate) fn json(value: &impl serde::Serialize) -> JsValue {
    serde_json::to_string(value)
        .ok()
        .and_then(|s| js_sys::JSON::parse(&s).ok())
        .unwrap_or(JsValue::NULL)
}

/// `window.__DDI_DEBUG`, what tests read, made when missing; `None`
/// without a window.
pub(crate) fn debug_object() -> Option<js_sys::Object> {
    let window = web_sys::window()?;
    let key = JsValue::from_str("__DDI_DEBUG");
    let debug = js_sys::Reflect::get(&window, &key)
        .ok()
        .filter(JsValue::is_object)
        .map(js_sys::Object::from)
        .unwrap_or_else(|| {
            let o = js_sys::Object::new();
            let _ = js_sys::Reflect::set(&window, &key, &o);
            o
        });
    Some(debug)
}

/// Deletes `keys` from `window.__DDI_DEBUG` (what a page said goes with
/// it), when there is one.
pub(crate) fn forget_debug(keys: &[&str]) {
    if let Some(d) = web_sys::window()
        .and_then(|w| js_sys::Reflect::get(&w, &"__DDI_DEBUG".into()).ok())
        .filter(JsValue::is_object)
    {
        let d = js_sys::Object::from(d);
        for key in keys {
            let _ = js_sys::Reflect::delete_property(&d, &(*key).into());
        }
    }
}
