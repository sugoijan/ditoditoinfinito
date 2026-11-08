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
//!
//! The same worker is the announcer studio's encoder where the browser
//! has none ([`encode`]): WebCodecs' `AudioEncoder` comes first, as the
//! browser's decoder does, and whatever packets it makes are put in Ogg
//! here.

use std::cell::{Cell, RefCell};
use std::rc::Rc;

use ddi_platform::ogg_opus::{self, Stream};
use js_sys::{Array, ArrayBuffer, Float32Array, Object, Promise, Reflect, Uint8Array, WebAssembly};
use wasm_bindgen::JsCast;
use wasm_bindgen::prelude::*;
use wasm_bindgen_futures::JsFuture;
use web_sys::{AudioBuffer, BaseAudioContext, Event, MessageEvent, Worker};

use super::js_err;

/// How long a worker may take to say it is ready, ms.
const READY_MS: f64 = 25_000.0;

/// How often the page looks whether the worker answered or the decode
/// is still wanted.
const POLL_MS: u32 = 20;

/// Silence fed after a sound to the browser's encoder, which may keep its
/// delay's worth back at the end: frames (what is past the sound is
/// dropped as the file is written).
const ENCODER_TAIL: usize = 960;
/// What libopus's delay is at 48 kHz (`OPUS_GET_LOOKAHEAD`), for a browser
/// that does not say what its own is.
const USUAL_PRE_SKIP: u16 = 312;

#[wasm_bindgen]
extern "C" {
    #[wasm_bindgen(js_name = AudioEncoder)]
    type JsAudioEncoder;
    #[wasm_bindgen(constructor, js_class = "AudioEncoder", catch)]
    fn new(init: &Object) -> Result<JsAudioEncoder, JsValue>;
    #[wasm_bindgen(method, catch)]
    fn configure(this: &JsAudioEncoder, config: &Object) -> Result<(), JsValue>;
    #[wasm_bindgen(method, catch)]
    fn encode(this: &JsAudioEncoder, data: &JsAudioData) -> Result<(), JsValue>;
    #[wasm_bindgen(method)]
    fn flush(this: &JsAudioEncoder) -> Promise;
    #[wasm_bindgen(method, catch)]
    fn close(this: &JsAudioEncoder) -> Result<(), JsValue>;
    #[wasm_bindgen(static_method_of = JsAudioEncoder, js_class = "AudioEncoder", js_name = isConfigSupported)]
    fn is_config_supported(config: &Object) -> Promise;

    #[wasm_bindgen(js_name = AudioData)]
    type JsAudioData;
    #[wasm_bindgen(constructor, js_class = "AudioData", catch)]
    fn new(init: &Object) -> Result<JsAudioData, JsValue>;
    #[wasm_bindgen(method)]
    fn close(this: &JsAudioData);

    #[wasm_bindgen(js_name = EncodedAudioChunk)]
    type JsAudioChunk;
    #[wasm_bindgen(method, getter, js_name = byteLength)]
    fn byte_length(this: &JsAudioChunk) -> u32;
    #[wasm_bindgen(method, catch, js_name = copyTo)]
    fn copy_to(this: &JsAudioChunk, destination: &Uint8Array) -> Result<(), JsValue>;
}

thread_local! {
    /// The module, compiled once a session.
    static MODULE: RefCell<Option<WebAssembly::Module>> = const { RefCell::new(None) };
    /// The worker's boot script's address (a blob, made once).
    static BOOT: RefCell<Option<String>> = const { RefCell::new(None) };
    /// Sounds encoded so far, by the browser and by our encoder.
    static ENCODED: Cell<(u32, u32)> = const { Cell::new((0, 0)) };
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

/// The module, fetched and compiled the first time.
async fn module(url: &str) -> Result<WebAssembly::Module, String> {
    if let Some(m) = MODULE.with(|m| m.borrow().clone()) {
        return Ok(m);
    }
    let m = super::wasm_worker::compile(url, "the Opus decoder").await?;
    MODULE.with(|c| *c.borrow_mut() = Some(m.clone()));
    Ok(m)
}

/// The boot script's address.
fn boot() -> Result<String, String> {
    if let Some(u) = BOOT.with(|b| b.borrow().clone()) {
        return Ok(u);
    }
    let url = super::wasm_worker::boot_url("error", "the Opus decoder")?;
    BOOT.with(|b| *b.borrow_mut() = Some(url.clone()));
    Ok(url)
}

/// A worker started, handed its job once it listens (`send`), and ended
/// once it answers: its answer; `Err` once `gone` says the job is no
/// longer wanted.
async fn in_worker(
    send: &dyn Fn(&Worker) -> Result<(), JsValue>,
    gone: &dyn Fn() -> bool,
) -> Result<JsValue, String> {
    let (glue, wasm) = super::wasm_worker::urls("ddi-opus-worker", "opus-worker");
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
        send(&running.worker).map_err(|e| js_err("sending the Opus worker its job", e))?;
        sent = true;
    };
    drop(running);
    Ok(data)
}

/// `bytes` (Ogg Opus) decoded in a worker, read by `ctx` at its rate;
/// `Err` once `gone` says the decode is no longer wanted.
pub(crate) async fn decode(
    ctx: &BaseAudioContext,
    bytes: &[u8],
    gone: &dyn Fn() -> bool,
) -> Result<AudioBuffer, String> {
    let send = |worker: &Worker| {
        // The file goes over (a copy, transferred).
        let file = Uint8Array::from(bytes).buffer();
        let msg = Object::new();
        let _ = Reflect::set(&msg, &JsValue::from_str("bytes"), &file);
        worker.post_message_with_transfer(&msg, &Array::of1(&file))
    };
    let data = in_worker(&send, gone).await?;
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

/// `channels` at `rate` Hz as an Ogg Opus file: by the browser's encoder,
/// else by ours in a worker.
pub(crate) async fn encode(channels: &[Vec<f32>], rate: u32) -> Result<Vec<u8>, String> {
    let forced = Reflect::get(&js_sys::global(), &"__DDI_FORCE_WASM_ENCODE".into())
        .is_ok_and(|v| v.is_truthy());
    if !forced {
        match by_the_browser(channels, rate).await {
            Ok(Some(file)) => {
                ENCODED.with(|c| c.set((c.get().0 + 1, c.get().1)));
                return Ok(file);
            }
            Ok(None) => {}
            Err(e) => web_sys::console::warn_1(
                &format!("audio encode: {e}; encoded in wasm instead").into(),
            ),
        }
    }
    let file = by_our_encoder(channels, rate).await?;
    ENCODED.with(|c| c.set((c.get().0, c.get().1 + 1)));
    Ok(file)
}

/// Sounds encoded so far: by the browser, and by our encoder.
pub(crate) fn encoded() -> (u32, u32) {
    ENCODED.with(Cell::get)
}

/// `channels` encoded in a worker.
async fn by_our_encoder(channels: &[Vec<f32>], rate: u32) -> Result<Vec<u8>, String> {
    let send = |worker: &Worker| {
        let planes = Array::new();
        for c in channels {
            planes.push(&Float32Array::from(&c[..]).buffer());
        }
        let msg = Object::new();
        let _ = Reflect::set(&msg, &"encode".into(), &planes);
        let _ = Reflect::set(&msg, &"rate".into(), &JsValue::from(rate));
        worker.post_message_with_transfer(&msg, &planes)
    };
    let data = in_worker(&send, &|| false).await?;
    let file: ArrayBuffer = Reflect::get(&data, &"opus".into())
        .map_err(|e| js_err("the Opus encoder's answer", e))?
        .dyn_into()
        .map_err(|_| "the Opus encoder sent no file".to_string())?;
    Ok(Uint8Array::new(&file).to_vec())
}

/// What a browser's encoder is asked for.
fn encoder_config(channels: usize, rate: u32) -> Object {
    let set = |o: &Object, k: &str, v: JsValue| {
        let _ = Reflect::set(o, &k.into(), &v);
    };
    let opus = Object::new();
    set(&opus, "format", "opus".into());
    set(&opus, "frameDuration", 20_000.into());
    let config = Object::new();
    set(&config, "codec", "opus".into());
    set(&config, "sampleRate", rate.into());
    set(&config, "numberOfChannels", (channels as u32).into());
    set(&config, "bitrate", ogg_opus::bitrate(channels).into());
    set(&config, "bitrateMode", "variable".into());
    set(&config, "opus", opus.into());
    config
}

/// What the browser's encoder has given so far.
#[derive(Default)]
struct FromEncoder {
    packets: Vec<Vec<u8>>,
    pre_skip: Option<u16>,
    failed: Option<String>,
}

/// The pre-skip an `OpusHead` says.
fn head_pre_skip(description: &JsValue) -> Option<u16> {
    let bytes = if description.is_instance_of::<ArrayBuffer>() {
        Uint8Array::new(description).to_vec()
    } else if ArrayBuffer::is_view(description) {
        let view: &js_sys::DataView = description.unchecked_ref();
        Uint8Array::new_with_byte_offset_and_length(
            &view.buffer(),
            view.byte_offset() as u32,
            view.byte_length() as u32,
        )
        .to_vec()
    } else {
        return None;
    };
    (bytes.len() >= 12 && bytes.starts_with(b"OpusHead"))
        .then(|| u16::from_le_bytes([bytes[10], bytes[11]]))
}

/// `channels` encoded by WebCodecs' `AudioEncoder`; `None` where the
/// browser has none for this, `Err` where the one it has failed.
async fn by_the_browser(channels: &[Vec<f32>], rate: u32) -> Result<Option<Vec<u8>>, String> {
    let has =
        Reflect::get(&js_sys::global(), &"AudioEncoder".into()).is_ok_and(|v| v.is_function());
    if !has || channels.is_empty() {
        return Ok(None);
    }
    let count = channels.len();
    let config = encoder_config(count, rate);
    // Refused, or a browser whose answer is not one.
    let supported = JsFuture::from(JsAudioEncoder::is_config_supported(&config))
        .await
        .ok()
        .and_then(|v| Reflect::get(&v, &"supported".into()).ok())
        .is_some_and(|v| v.is_truthy());
    if !supported {
        return Ok(None);
    }

    let got = Rc::new(RefCell::new(FromEncoder::default()));
    let output = {
        let got = Rc::clone(&got);
        Closure::<dyn FnMut(JsAudioChunk, JsValue)>::new(
            move |chunk: JsAudioChunk, meta: JsValue| {
                let mut got = got.borrow_mut();
                let bytes = Uint8Array::new_with_length(chunk.byte_length());
                match chunk.copy_to(&bytes) {
                    Ok(()) => got.packets.push(bytes.to_vec()),
                    Err(e) => got.failed = Some(js_err("reading a packet", e)),
                }
                if let Some(skip) = Reflect::get(&meta, &"decoderConfig".into())
                    .ok()
                    .filter(|c| c.is_object())
                    .and_then(|c| Reflect::get(&c, &"description".into()).ok())
                    .and_then(|d| head_pre_skip(&d))
                {
                    got.pre_skip = Some(skip);
                }
            },
        )
    };
    let error = {
        let got = Rc::clone(&got);
        Closure::<dyn FnMut(JsValue)>::new(move |e: JsValue| {
            got.borrow_mut().failed = Some(js_err("the browser's Opus encoder", e));
        })
    };
    let init = Object::new();
    let _ = Reflect::set(&init, &"output".into(), output.as_ref());
    let _ = Reflect::set(&init, &"error".into(), error.as_ref());
    let encoder =
        JsAudioEncoder::new(&init).map_err(|e| js_err("making the browser's Opus encoder", e))?;
    let run = async {
        encoder
            .configure(&config)
            .map_err(|e| js_err("configuring the browser's Opus encoder", e))?;
        let frames = channels[0].len();
        let total = frames + ENCODER_TAIL;
        // The channels one after the other, what `f32-planar` wants, each
        // with its silence after it (an array starts as zeros).
        let planes = Float32Array::new_with_length((total * count) as u32);
        for (i, c) in channels.iter().enumerate() {
            let start = (i * total) as u32;
            planes.subarray(start, start + c.len() as u32).copy_from(c);
        }
        let init = Object::new();
        let set = |k: &str, v: JsValue| {
            let _ = Reflect::set(&init, &k.into(), &v);
        };
        set("format", "f32-planar".into());
        set("sampleRate", rate.into());
        set("numberOfFrames", (total as u32).into());
        set("numberOfChannels", (count as u32).into());
        set("timestamp", 0.into());
        set("data", planes.into());
        let data = JsAudioData::new(&init).map_err(|e| js_err("the sound for the encoder", e))?;
        let fed = encoder
            .encode(&data)
            .map_err(|e| js_err("encoding the sound", e));
        data.close();
        fed?;
        JsFuture::from(encoder.flush())
            .await
            .map_err(|e| js_err("finishing the encode", e))?;
        Ok::<_, String>(frames as u64)
    };
    let frames = run.await;
    let _ = encoder.close();
    let frames = frames?;
    let got = got.borrow();
    if let Some(e) = &got.failed {
        return Err(e.clone());
    }
    ogg_opus::write(
        Stream {
            channels: count as u8,
            pre_skip: got.pre_skip.unwrap_or(USUAL_PRE_SKIP),
            frames,
        },
        &got.packets,
    )
    .map(Some)
}
