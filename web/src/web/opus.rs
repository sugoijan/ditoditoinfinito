//! The Ogg Opus fallback (PLAN.md decision 9): a file the browser's
//! `decodeAudioData` cannot read (WebKit builds without Opus, Safari
//! before 18.4) is decoded by the Opus worker (`opus-worker/`, built by
//! Trunk beside the app and fetched only now) into a float WAV at 48 kHz,
//! which `decodeAudioData` then reads at the context's rate, resampled as
//! the browser resamples its own decodes.
//!
//! The worker's module is fetched and compiled once a session, here, and
//! handed to each worker; a worker a decode, so its memory goes when it is
//! done and a decode no longer wanted is stopped by ending it. Its files
//! carry their content's hash (`xtask stage-opus` names them and tells the
//! page in index.html's `ddi-opus-worker` meta), so a page never mixes an
//! old build's files with a new one's. A worker that does not say it is
//! ready within [`READY_MS`] (its files missing, its start failing) is
//! ended with an error.

use std::cell::RefCell;
use std::rc::Rc;

use js_sys::{Array, Object, Reflect, Uint8Array, WebAssembly};
use wasm_bindgen::JsCast;
use wasm_bindgen::prelude::*;
use wasm_bindgen_futures::JsFuture;
use web_sys::{AudioBuffer, BaseAudioContext, Event, MessageEvent, Worker};

use super::js_err;

/// The worker's files' stem when the page names none (a build without the
/// stage step).
const DEFAULT_STEM: &str = "opus-worker";

/// The worker's own script: loads wasm-bindgen's glue and starts the
/// module the page sends (compiled, or its address), and says why if it
/// cannot. The decoder's code then answers `{ready}` and decodes.
const BOOT_JS: &str = r#"self.onmessage = async (e) => {
  self.onmessage = null;
  try {
    importScripts(e.data.glue);
    await wasm_bindgen({ module_or_path: e.data.module || e.data.wasm });
  } catch (err) {
    postMessage({ error: 'the Opus decoder could not start: ' + ((err && err.message) || err) });
  }
};"#;

/// How long a worker may take to say it is ready, ms.
const READY_MS: f64 = 25_000.0;

/// How often the page looks whether the worker answered or the decode
/// is still wanted.
const POLL_MS: u32 = 20;

thread_local! {
    /// The module, compiled once a session.
    static MODULE: RefCell<Option<WebAssembly::Module>> = const { RefCell::new(None) };
    /// The worker's boot script's address (a blob, made once).
    static BOOT: RefCell<Option<String>> = const { RefCell::new(None) };
}

/// What the worker said last.
enum State {
    Starting,
    Ready,
    Done(Result<JsValue, String>),
}

/// The worker, ended when dropped.
struct Running {
    worker: Worker,
    _on_message: Closure<dyn FnMut(MessageEvent)>,
    _on_error: Closure<dyn FnMut(Event)>,
}

impl Drop for Running {
    fn drop(&mut self) {
        self.worker.set_onmessage(None);
        self.worker.set_onerror(None);
        self.worker.terminate();
    }
}

/// The worker's files' stem, as index.html names it.
fn stem() -> String {
    web_sys::window()
        .and_then(|w| w.document())
        .and_then(|d| {
            d.query_selector("meta[name=\"ddi-opus-worker\"]")
                .ok()
                .flatten()
        })
        .and_then(|m| m.get_attribute("content"))
        .filter(|s| !s.is_empty() && !s.contains('/'))
        .unwrap_or_else(|| DEFAULT_STEM.to_string())
}

/// The module, fetched and compiled the first time.
async fn module(url: &str) -> Result<WebAssembly::Module, String> {
    if let Some(m) = MODULE.with(|m| m.borrow().clone()) {
        return Ok(m);
    }
    let fetch_err = |e: String| format!("the Opus decoder could not be fetched: {e}");
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
        .map_err(|e| js_err("compiling the Opus decoder", e))?;
    let m: WebAssembly::Module = compiled
        .dyn_into()
        .map_err(|e| js_err("compiling the Opus decoder", e))?;
    MODULE.with(|c| *c.borrow_mut() = Some(m.clone()));
    Ok(m)
}

/// The boot script's address.
fn boot() -> Result<String, String> {
    if let Some(u) = BOOT.with(|b| b.borrow().clone()) {
        return Ok(u);
    }
    let blob = super::files::bytes_blob(BOOT_JS.as_bytes(), "text/javascript")?;
    let url = web_sys::Url::create_object_url_with_blob(&blob)
        .map_err(|e| js_err("the Opus decoder's script", e))?;
    BOOT.with(|b| *b.borrow_mut() = Some(url.clone()));
    Ok(url)
}

/// `bytes` (Ogg Opus) decoded in a worker, read by `ctx` at its rate;
/// `Err` once `gone` says the decode is no longer wanted.
pub(crate) async fn decode(
    ctx: &BaseAudioContext,
    bytes: &[u8],
    gone: &dyn Fn() -> bool,
) -> Result<AudioBuffer, String> {
    let stem = stem();
    let glue = crate::songs::asset_url(&format!("{stem}.js"));
    let wasm = crate::songs::asset_url(&format!("{stem}_bg.wasm"));
    let module = module(&wasm).await?;
    if gone() {
        return Err("the decode is no longer wanted".into());
    }
    let state = Rc::new(RefCell::new(State::Starting));
    let worker = Worker::new(&boot()?).map_err(|e| js_err("starting the Opus decoder", e))?;
    let on_message = {
        let state = Rc::clone(&state);
        Closure::<dyn FnMut(MessageEvent)>::new(move |event: MessageEvent| {
            let data = event.data();
            let get = |k: &str| Reflect::get(&data, &JsValue::from_str(k)).ok();
            let next = if get("ready").is_some_and(|v| v.is_truthy()) {
                State::Ready
            } else if let Some(error) = get("error").and_then(|e| e.as_string()) {
                State::Done(Err(error))
            } else {
                State::Done(Ok(data))
            };
            *state.borrow_mut() = next;
        })
    };
    let on_error = {
        let state = Rc::clone(&state);
        Closure::<dyn FnMut(Event)>::new(move |event: Event| {
            event.prevent_default();
            let detail = Reflect::get(&event, &JsValue::from_str("message"))
                .ok()
                .and_then(|m| m.as_string())
                .filter(|m| !m.is_empty())
                .unwrap_or_else(|| "it could not be loaded or stopped".into());
            *state.borrow_mut() = State::Done(Err(format!("the Opus decoder failed: {detail}")));
        })
    };
    worker.set_onmessage(Some(on_message.as_ref().unchecked_ref()));
    worker.set_onerror(Some(on_error.as_ref().unchecked_ref()));
    let running = Running {
        worker,
        _on_message: on_message,
        _on_error: on_error,
    };
    // The module goes over compiled; where a browser cannot send one, the
    // worker fetches it itself (from the HTTP cache).
    let start = |with_module: bool| {
        let msg = Object::new();
        let _ = Reflect::set(&msg, &"glue".into(), &JsValue::from_str(&glue));
        if with_module {
            let _ = Reflect::set(&msg, &"module".into(), &module);
        } else {
            let _ = Reflect::set(&msg, &"wasm".into(), &JsValue::from_str(&wasm));
        }
        running.worker.post_message(&msg)
    };
    start(true)
        .or_else(|_| start(false))
        .map_err(|e| js_err("starting the Opus decoder", e))?;
    let began = js_sys::Date::now();
    let mut sent = false;
    let data = loop {
        gloo::timers::future::TimeoutFuture::new(POLL_MS).await;
        if gone() {
            return Err("the decode is no longer wanted".into());
        }
        match &*state.borrow() {
            State::Starting if js_sys::Date::now() - began > READY_MS => {
                return Err(format!(
                    "the Opus decoder did not start within {} s",
                    READY_MS / 1000.0
                ));
            }
            State::Starting => continue,
            State::Ready if sent => continue,
            State::Ready => {}
            State::Done(result) => break result.clone()?,
        }
        // The worker listens: the file goes over (a copy, transferred).
        let file = Uint8Array::from(bytes).buffer();
        let msg = Object::new();
        let _ = Reflect::set(&msg, &JsValue::from_str("bytes"), &file);
        running
            .worker
            .post_message_with_transfer(&msg, &Array::of1(&file))
            .map_err(|e| js_err("sending the file to the Opus decoder", e))?;
        sent = true;
    };
    drop(running);
    let get = |k: &str| Reflect::get(&data, &JsValue::from_str(k)).unwrap_or(JsValue::UNDEFINED);
    let skipped = get("skipped").as_f64().unwrap_or(0.0);
    if skipped > 0.0 {
        web_sys::console::warn_1(
            &format!("audio decode: skipped {skipped} undecodable packets").into(),
        );
    }
    let wav: js_sys::ArrayBuffer = get("wav")
        .dyn_into()
        .map_err(|_| "the Opus decoder sent no audio".to_string())?;
    let promise = ctx
        .decode_audio_data(&wav)
        .map_err(|e| js_err("decodeAudioData (the Opus decoder's WAV)", e))?;
    JsFuture::from(promise)
        .await
        .map_err(|e| js_err("decodeAudioData (the Opus decoder's WAV)", e))?
        .dyn_into::<AudioBuffer>()
        .map_err(|e| js_err("decodeAudioData result", e))
}
