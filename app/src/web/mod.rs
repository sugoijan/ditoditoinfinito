//! Web implementations of the `ddi-platform` traits plus the wgpu canvas
//! bootstrap. Nothing in here knows about gameplay.

pub(crate) mod audio;
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
pub(crate) mod storage;
pub(crate) mod touch;

use wasm_bindgen::JsValue;

pub(crate) fn js_err(context: &str, err: JsValue) -> String {
    format!("{context}: {err:?}")
}
