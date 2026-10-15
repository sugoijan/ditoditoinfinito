//! Web implementations of the `ddi-platform` traits plus the wgpu canvas
//! bootstrap, the app's one canvas (`screen`) and what the 2D pass needs
//! of the browser (`text`, `textures`). Nothing in here knows about
//! gameplay.

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
pub(crate) mod screen;
pub(crate) mod storage;
pub(crate) mod text;
pub(crate) mod textures;
pub(crate) mod touch;
pub(crate) mod video;
pub(crate) mod webcodecs;

use wasm_bindgen::JsValue;

pub(crate) fn js_err(context: &str, err: JsValue) -> String {
    format!("{context}: {err:?}")
}
