//! Keeping the library: a site's storage is "best effort" by default, and
//! browsers may clear it (IndexedDB and every other kind together) when
//! space runs low. `navigator.storage.persist()` asks to keep it; Firefox
//! shows a prompt for that, so it is requested from a click or a drop.

use wasm_bindgen_futures::JsFuture;

/// Asks the browser to keep this site's storage. Call it synchronously from
/// a user gesture; the answer arrives later (`true` when granted).
pub(crate) fn request_persist() -> Option<JsFuture> {
    let storage = web_sys::window()?.navigator().storage();
    storage.persist().ok().map(JsFuture::from)
}

/// Whether the storage is kept (`None` when the browser cannot say).
pub(crate) async fn persisted() -> Option<bool> {
    let storage = web_sys::window()?.navigator().storage();
    JsFuture::from(storage.persisted().ok()?)
        .await
        .ok()?
        .as_bool()
}
