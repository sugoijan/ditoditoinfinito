//! Prepared CPU laws are loaded before the start gesture; never run inference
//! or fetch from the audio/input callback. Local filesystem metadata is ignored.
use ddi_engine::cpu::{MAX_BYTES, Profile};
use serde::Deserialize;
use wasm_bindgen::prelude::*;

#[wasm_bindgen(inline_js = r#"
export async function ddiCpuProfile(url, limit) {
    const abort = new AbortController();
    const timer = setTimeout(() => abort.abort(), 30000);
    let reader;
    try {
        const response = await fetch(url, {mode: 'same-origin', redirect: 'error', signal: abort.signal});
        if (!response.ok) throw new Error(`CPU profile: HTTP ${response.status}`);
        const size = Number(response.headers.get('content-length'));
        if (size > limit) throw new Error('CPU profile exceeds 64 MiB');
        reader = response.body.getReader();
        const chunks = []; let total = 0;
        while (true) {
            const {value, done} = await reader.read();
            if (done) break;
            total += value.byteLength;
            if (total > limit) throw new Error('CPU profile exceeds 64 MiB');
            chunks.push(value);
        }
        const bytes = new Uint8Array(total); let offset = 0;
        for (const chunk of chunks) {bytes.set(chunk, offset); offset += chunk.byteLength;}
        return bytes;
    } finally {
        abort.abort();
        if (reader) reader.releaseLock();
        clearTimeout(timer);
    }
}
"#)]
extern "C" {
    #[wasm_bindgen(catch, js_name = ddiCpuProfile)]
    async fn fetch_profile(url: &str, limit: f64) -> Result<js_sys::Uint8Array, JsValue>;
}

#[derive(Deserialize)]
struct Prepared {
    profile: Profile,
}

pub(crate) async fn load(path: &str) -> Result<Profile, String> {
    let bytes = fetch_profile(&crate::songs::asset_url(path), MAX_BYTES as f64)
        .await
        .map_err(|e| format!("CPU profile {path} could not be loaded: {e:?}"))?
        .to_vec();
    let prepared: Prepared =
        serde_json::from_slice(&bytes).map_err(|e| format!("Invalid CPU profile: {e}"))?;
    prepared.profile.validate()?;
    Ok(prepared.profile)
}
