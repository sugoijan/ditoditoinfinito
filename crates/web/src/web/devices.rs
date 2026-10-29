//! The browser's media devices, for the play route's audio output
//! fingerprint (`crate::play`).

/// `navigator.mediaDevices`, or `None` where it is missing: browsers leave it
/// undefined outside secure contexts (e.g. a LAN address over plain HTTP),
/// and web-sys would hand back `undefined` as if it were the object.
pub(crate) fn media_devices() -> Option<web_sys::MediaDevices> {
    let md = web_sys::window()?.navigator().media_devices().ok()?;
    (!wasm_bindgen::JsValue::from(md.clone()).is_undefined()).then_some(md)
}
