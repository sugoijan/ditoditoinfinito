//! Loads the 2D pass's textures (`docs/plans/ui-core.md`, P9): a
//! texture's source is a path of the site (bundled songs and art) or
//! `files:<key>`, a file kept in the library's IndexedDB `files` store
//! (an imported banner). The browser decodes it off the main thread and
//! scales it down to the size it is drawn at (`createImageBitmap`,
//! decision 16); the screen puts it on the device at the next frame.
//!
//! `window.__DDI_TEXTURE_DELAY_MS` (testing) holds every decoded image
//! back that long, so a test sees the placeholder first.

use ddi_render::texture_cache::{TextureId, TextureKey};
use wasm_bindgen::JsCast;
use web_sys::Blob;

/// Starts loading texture `id`; the screen gets the result.
pub(crate) fn load(id: TextureId, key: TextureKey, limit: u32) {
    wasm_bindgen_futures::spawn_local(async move {
        let result = decode(&key, limit).await;
        if let Some(ms) = delay_ms() {
            gloo::timers::future::TimeoutFuture::new(ms).await;
        }
        super::screen::texture_decoded(id, result);
    });
}

async fn decode(key: &TextureKey, limit: u32) -> Result<web_sys::ImageBitmap, String> {
    let blob = match key.source.strip_prefix("files:") {
        Some(file) => {
            let db = super::idb::Db::open().await?;
            db.get("files", file)
                .await?
                .ok_or_else(|| format!("no file {file}"))?
                .dyn_into::<Blob>()
                .map_err(|_| format!("{file} is not a file"))?
        }
        None => {
            let bytes = crate::songs::fetch_bytes(&key.source).await?;
            super::files::bytes_blob(&bytes, "")?
        }
    };
    let side = match key.max_side {
        0 => limit,
        s => s.min(limit),
    };
    super::image::decode(&blob, side).await
}

fn delay_ms() -> Option<u32> {
    let w = web_sys::window()?;
    js_sys::Reflect::get(&w, &"__DDI_TEXTURE_DELAY_MS".into())
        .ok()?
        .as_f64()
        .filter(|ms| *ms > 0.0)
        .map(|ms| ms.min(60_000.0) as u32)
}
