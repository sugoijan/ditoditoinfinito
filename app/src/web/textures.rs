//! Loads the 2D pass's textures (`docs/plans/ui-core.md`, P9): a
//! texture's source is a path of the site (bundled songs and art) or
//! `files:<key>`, a file kept in the library's IndexedDB `files` store
//! (an imported banner). The browser decodes it off the main thread and
//! scales it down to the size it is drawn at (`createImageBitmap`,
//! decision 16); the screen puts it on the device at the next frame.
//!
//! A source can also be a file already in memory, kept under a name
//! ([`keep`]): the play's background, which its results draw again.
//!
//! `window.__DDI_TEXTURE_DELAY_MS` (testing) holds every decoded image
//! back that long, so a test sees the placeholder first.

use std::cell::RefCell;
use std::collections::HashMap;

use ddi_render::texture_cache::{TextureId, TextureKey};
use wasm_bindgen::JsCast;
use web_sys::Blob;

thread_local! {
    /// Files already in memory, by the source they stand for: the play's
    /// background, which its results draw again without reading it twice.
    static KEPT: RefCell<HashMap<String, Blob>> = RefCell::new(HashMap::new());
}

/// Keeps `blob` as the file of `source` (replacing any before), or lets
/// it go with `None`.
pub(crate) fn keep(source: &str, blob: Option<Blob>) {
    KEPT.with(|k| match blob {
        Some(b) => {
            k.borrow_mut().insert(source.to_string(), b);
        }
        None => {
            k.borrow_mut().remove(source);
        }
    });
}

/// Whether a file is kept for `source`.
pub(crate) fn is_kept(source: &str) -> bool {
    KEPT.with(|k| k.borrow().contains_key(source))
}

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
    let kept = KEPT.with(|k| k.borrow().get(&*key.source).cloned());
    let blob = match key.source.strip_prefix("files:") {
        _ if kept.is_some() => kept.expect("checked"),
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
