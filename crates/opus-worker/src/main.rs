//! The web's Ogg Opus fallback (PLAN.md decision 9): a worker that decodes
//! one file with `rusty-opus` (`ddi_platform::opus`, desktop's decoder:
//! the pre-skip dropped, the stream ended at its last granule, the gain
//! applied) into a WAV of 32-bit floats at 48 kHz, which the page's
//! `decodeAudioData` reads at the context's rate with the browser's own
//! resampler, as it would have read the Opus file. So browsers that cannot
//! decode Opus (WebKit builds without it, Safari before 18.4) still play
//! the songs. It is a module of its own, fetched only then: the decoder
//! costs every other visitor nothing, and its memory goes when the page
//! ends the worker.
//!
//! It encodes too, for the announcer studio's packs where the browser has
//! no Opus encoder of its own (`ddi_platform::opus::encode`).
//!
//! Protocol (`crates/web/src/web/opus.rs`, whose boot script loads this module
//! from the compiled `WebAssembly.Module` the page sends): the worker says
//! `{ready: true}` once it listens; the page sends `{bytes: ArrayBuffer}`
//! and the worker answers `{wav: ArrayBuffer, skipped}` (transferred) or
//! `{error}`, or it sends `{encode: [ArrayBuffer of f32, a channel each],
//! rate}` and the worker answers `{opus: ArrayBuffer}` or `{error}`.

#[cfg(target_arch = "wasm32")]
fn main() {
    use js_sys::{Array, Float32Array, Object, Reflect, Uint8Array};
    use wasm_bindgen::JsCast;
    use wasm_bindgen::prelude::*;
    use web_sys::{DedicatedWorkerGlobalScope, MessageEvent};

    let scope: DedicatedWorkerGlobalScope = js_sys::global().unchecked_into();
    let set = |o: &Object, k: &str, v: &JsValue| {
        let _ = Reflect::set(o, &JsValue::from_str(k), v);
    };
    let reply = scope.clone();
    let on_message = Closure::<dyn FnMut(MessageEvent)>::new(move |event: MessageEvent| {
        let data = event.data();
        let get = |k: &str| Reflect::get(&data, &JsValue::from_str(k)).ok();
        let msg = Object::new();
        let transfer = Array::new();
        let sent = |key: &str, file: Vec<u8>| {
            let file = Uint8Array::from(&file[..]).buffer();
            set(&msg, key, &file);
            transfer.push(&file);
        };
        if let Some(planes) = get("encode").filter(Array::is_array) {
            let channels: Vec<Vec<f32>> = Array::from(&planes)
                .iter()
                .map(|p| Float32Array::new(&p).to_vec())
                .collect();
            let rate = get("rate").and_then(|r| r.as_f64()).unwrap_or(0.0) as u32;
            match ddi_platform::opus::encode(&channels, rate) {
                Ok(file) => sent("opus", file),
                Err(e) => set(&msg, "error", &JsValue::from_str(&e)),
            }
        } else {
            let bytes = get("bytes")
                .filter(|b| b.is_instance_of::<js_sys::ArrayBuffer>())
                .map(|b| Uint8Array::new(&b).to_vec());
            match bytes
                .ok_or_else(|| "no file was sent".to_string())
                .and_then(|b| ddi_platform::opus::decode_wav(&b))
            {
                Ok((wav, skipped)) => {
                    sent("wav", wav);
                    set(&msg, "skipped", &JsValue::from(skipped as u32));
                }
                Err(e) => set(&msg, "error", &JsValue::from_str(&e)),
            }
        }
        let _ = reply.post_message_with_transfer(&msg, &transfer);
    });
    scope.set_onmessage(Some(on_message.as_ref().unchecked_ref()));
    on_message.forget();
    let ready = Object::new();
    set(&ready, "ready", &JsValue::TRUE);
    let _ = scope.post_message(&ready);
}

#[cfg(not(target_arch = "wasm32"))]
fn main() {}
