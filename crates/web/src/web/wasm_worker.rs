//! What the page's wasm workers (the Opus decoder, `opus`; the SVG
//! drawing, `svg`) start alike: their files' names as index.html gives
//! them, the worker's own script, which loads wasm-bindgen's glue and
//! starts the module the page sends, and that module fetched and
//! compiled.

use js_sys::{Uint8Array, WebAssembly};
use wasm_bindgen::JsCast;
use wasm_bindgen_futures::JsFuture;

use super::js_err;

/// The worker's glue and module addresses: their stem from index.html's
/// `meta` named `meta` (the stage step's, with their content's hash),
/// else `default` (a build without it).
pub(crate) fn urls(meta: &str, default: &str) -> (String, String) {
    let stem = web_sys::window()
        .and_then(|w| w.document())
        .and_then(|d| {
            d.query_selector(&format!("meta[name=\"{meta}\"]"))
                .ok()
                .flatten()
        })
        .and_then(|m| m.get_attribute("content"))
        .filter(|s| !s.is_empty() && !s.contains('/'))
        .unwrap_or_else(|| default.to_string());
    (
        crate::songs::asset_url(&format!("{stem}.js")),
        crate::songs::asset_url(&format!("{stem}_bg.wasm")),
    )
}

/// The worker's own script as a blob's address: it loads the glue and
/// starts the module the page sends (compiled, or its address), or posts
/// `{<error_key>: why}` naming `what` when it cannot.
pub(crate) fn boot_url(error_key: &str, what: &str) -> Result<String, String> {
    let script = format!(
        r#"self.onmessage = async (e) => {{
  self.onmessage = null;
  try {{
    importScripts(e.data.glue);
    await wasm_bindgen({{ module_or_path: e.data.module || e.data.wasm }});
  }} catch (err) {{
    postMessage({{ {error_key}: '{what} could not start: ' + ((err && err.message) || err) }});
  }}
}};"#
    );
    let blob = super::files::bytes_blob(script.as_bytes(), "text/javascript")?;
    web_sys::Url::create_object_url_with_blob(&blob)
        .map_err(|e| js_err(&format!("{what}'s script"), e))
}

/// The module at `url` fetched and compiled (`what` names it in errors).
pub(crate) async fn compile(url: &str, what: &str) -> Result<WebAssembly::Module, String> {
    let fetch_err = |e: String| format!("{what} could not be fetched: {e}");
    let resp = gloo::net::http::Request::get(url)
        .send()
        .await
        .map_err(|e| fetch_err(e.to_string()))?;
    if !resp.ok() {
        return Err(fetch_err(format!("HTTP {}", resp.status())));
    }
    let bytes = resp.binary().await.map_err(|e| fetch_err(e.to_string()))?;
    let compiled = JsFuture::from(WebAssembly::compile(&Uint8Array::from(&bytes[..])))
        .await
        .map_err(|e| js_err(&format!("compiling {what}"), e))?;
    compiled
        .dyn_into()
        .map_err(|e| js_err(&format!("compiling {what}"), e))
}
