//! Skins' SVG images drawn by the SVG worker (`crates/svg-worker`, built by
//! Trunk beside the app), with the code desktop draws them with
//! (`ddi_noteskin::raster`), so an image looks the same on both shells
//! (`docs/plans/noteskin-svg.md`, step 1). A few workers draw at once, as
//! many as the device has cores to spare (at most [`MAX_WORKERS`]); they
//! are ended once none has had an image for [`IDLE_MS`], so their memory
//! goes when a skin is loaded.
//!
//! The module is fetched and compiled once a session and handed to each
//! worker, as the Opus worker's is (`opus.rs`); its files carry their
//! content's hash (`xtask stage-svg`, index.html's `ddi-svg-worker` meta).
//! Where no worker starts (its files missing), [`rasterise`] fails, and
//! the caller draws with the browser instead.
//!
//! `window.__DDI_SVG` counts the images drawn and the time the workers
//! spent drawing them, for tests.

use std::cell::RefCell;
use std::collections::{HashMap, VecDeque};

use js_sys::{Function, Object, Promise, Reflect, Uint8Array, WebAssembly};
use wasm_bindgen::JsCast;
use wasm_bindgen::prelude::*;
use wasm_bindgen_futures::JsFuture;
use web_sys::{Event, ImageBitmap, MessageEvent, Worker};

use super::js_err;

/// The worker's files' stem when the page names none (a build without the
/// stage step).
const DEFAULT_STEM: &str = "svg-worker";

/// The worker's own script: loads wasm-bindgen's glue and starts the
/// module the page sends (compiled, or its address).
const BOOT_JS: &str = r#"self.onmessage = async (e) => {
  self.onmessage = null;
  try {
    importScripts(e.data.glue);
    await wasm_bindgen({ module_or_path: e.data.module || e.data.wasm });
  } catch (err) {
    postMessage({ failed: 'the SVG worker could not start: ' + ((err && err.message) || err) });
  }
};"#;

/// The most workers drawing at once.
const MAX_WORKERS: usize = 4;

/// How long the workers stay once none has an image, ms.
const IDLE_MS: u32 = 3_000;

/// How long a worker may take to say it is ready, ms.
const READY_MS: u32 = 25_000;

#[derive(Clone, Copy)]
enum Slot {
    Starting,
    Idle,
    /// Drawing the image of this id.
    Busy(u32),
}

struct Running {
    worker: Worker,
    slot: Slot,
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

struct Job {
    id: u32,
    text: String,
    size: (u32, u32),
    /// The pixels wanted, not a bitmap (tests).
    raw: bool,
}

#[derive(Default)]
struct Pool {
    /// The module, compiled once a session.
    module: Option<WebAssembly::Module>,
    /// The boot script's address (a blob, made once).
    boot: Option<String>,
    /// The workers, by a number of their own.
    workers: HashMap<u32, Running>,
    queue: VecDeque<Job>,
    /// Each image's promise's resolve and reject, by its id.
    waiting: HashMap<u32, (Function, Function)>,
    next: u32,
    /// Ends the workers once idle.
    idle: Option<gloo::timers::callback::Timeout>,
    /// Why no worker starts, once one did not.
    broken: Option<String>,
    /// Images drawn, and the workers' time drawing them, ms.
    drawn: u32,
    drawing_ms: f64,
}

thread_local! {
    static POOL: RefCell<Pool> = RefCell::default();
    /// The module's compile, shared by the images asked for before it is done.
    static COMPILING: RefCell<Option<Promise>> = const { RefCell::new(None) };
}

/// The worker's files' stem, as index.html names it.
fn stem() -> String {
    web_sys::window()
        .and_then(|w| w.document())
        .and_then(|d| {
            d.query_selector("meta[name=\"ddi-svg-worker\"]")
                .ok()
                .flatten()
        })
        .and_then(|m| m.get_attribute("content"))
        .filter(|s| !s.is_empty() && !s.contains('/'))
        .unwrap_or_else(|| DEFAULT_STEM.to_string())
}

fn urls() -> (String, String) {
    let stem = stem();
    (
        crate::songs::asset_url(&format!("{stem}.js")),
        crate::songs::asset_url(&format!("{stem}_bg.wasm")),
    )
}

/// The module, fetched and compiled the first time.
async fn module() -> Result<WebAssembly::Module, String> {
    if let Some(m) = POOL.with_borrow(|p| p.module.clone()) {
        return Ok(m);
    }
    let promise = COMPILING.with_borrow_mut(|c| {
        c.get_or_insert_with(|| {
            wasm_bindgen_futures::future_to_promise(async {
                let (_, wasm) = urls();
                let fetch_err = |e: String| format!("the SVG worker could not be fetched: {e}");
                let resp = gloo::net::http::Request::get(&wasm)
                    .send()
                    .await
                    .map_err(|e| JsValue::from_str(&fetch_err(e.to_string())))?;
                if !resp.ok() {
                    return Err(JsValue::from_str(&fetch_err(format!(
                        "HTTP {}",
                        resp.status()
                    ))));
                }
                let bytes = resp
                    .binary()
                    .await
                    .map_err(|e| JsValue::from_str(&fetch_err(e.to_string())))?;
                JsFuture::from(WebAssembly::compile(&Uint8Array::from(&bytes[..]))).await
            })
        })
        .clone()
    });
    let compiled = JsFuture::from(promise).await;
    COMPILING.with_borrow_mut(|c| *c = None);
    let m: WebAssembly::Module = compiled
        .map_err(|e| {
            e.as_string()
                .unwrap_or_else(|| js_err("compiling the SVG worker", e))
        })?
        .dyn_into()
        .map_err(|e| js_err("compiling the SVG worker", e))?;
    POOL.with_borrow_mut(|p| p.module = Some(m.clone()));
    Ok(m)
}

/// Fetches and compiles the worker's module, unless done: a skin with SVG
/// images is about to be loaded.
pub(crate) fn warm() {
    if POOL.with_borrow(|p| p.module.is_some() || p.broken.is_some()) {
        return;
    }
    wasm_bindgen_futures::spawn_local(async {
        if let Err(e) = module().await {
            POOL.with_borrow_mut(|p| p.broken = Some(e));
        }
    });
}

/// SVG `text` (checked) drawn at `size` pixels by a worker: a promise of
/// `{bitmap, hash}`, its `ImageBitmap` (colors not premultiplied) and its
/// pixels' hash ([`drawn`] reads both). The drawing starts now; several asked for
/// at once are drawn at once.
pub(crate) fn rasterise(text: String, size: (u32, u32)) -> Promise {
    wasm_bindgen_futures::future_to_promise(async move {
        let value = draw(text, size, false).await?;
        let drawn = Object::new();
        Reflect::set(
            &drawn,
            &"hash".into(),
            &Reflect::get(&value, &"hash".into())?,
        )?;
        let bitmap = Reflect::get(&value, &"bitmap".into())?;
        if bitmap.is_instance_of::<ImageBitmap>() {
            Reflect::set(&drawn, &"bitmap".into(), &bitmap)?;
            return Ok(drawn.into());
        }
        // Pixels, where the worker could make no bitmap.
        let pixels: js_sys::ArrayBuffer = Reflect::get(&value, &"pixels".into())?.dyn_into()?;
        let image = web_sys::ImageData::new_with_js_u8_clamped_array_and_sh(
            &js_sys::Uint8ClampedArray::new(&pixels),
            size.0,
            size.1,
        )?;
        let window = web_sys::window().ok_or("no window")?;
        let bitmap = crate::web::image::bitmap("an SVG's pixels", |options| match options {
            Some(o) => {
                window.create_image_bitmap_with_image_data_and_image_bitmap_options(&image, o)
            }
            None => window.create_image_bitmap_with_image_data(&image),
        })
        .await?;
        Reflect::set(&drawn, &"bitmap".into(), &bitmap)?;
        Ok(drawn.into())
    })
}

/// Why a worker refused to draw an image, when [`rasterise`]'s promise
/// failed for that (the image's own fault, as desktop refuses it) rather
/// than for want of a worker.
pub(crate) fn refusal(error: &JsValue) -> Option<String> {
    Reflect::get(error, &"refused".into())
        .ok()
        .filter(JsValue::is_truthy)?;
    error
        .dyn_ref::<js_sys::Error>()
        .map(|e| String::from(e.message()))
}

/// What [`rasterise`] gave: the bitmap, and its pixels' hash (`None` from
/// a worker that sent none).
pub(crate) fn drawn(value: &JsValue) -> Option<(ImageBitmap, Option<u64>)> {
    let bitmap = Reflect::get(value, &"bitmap".into())
        .ok()?
        .dyn_into()
        .ok()?;
    let hash = Reflect::get(value, &"hash".into())
        .ok()
        .and_then(|h| h.as_string())
        .and_then(|h| u64::from_str_radix(&h, 16).ok());
    Some((bitmap, hash))
}

/// `window.__DDI_SVG_PIXELS(text, width, height)`, for tests: a promise
/// of the RGBA rows a worker draws (`Uint8Array`), to compare with
/// desktop's.
pub(crate) fn install_test_hook() {
    let Some(window) = web_sys::window() else {
        return;
    };
    let hook = Closure::<dyn Fn(String, u32, u32) -> Promise>::new(|text, w, h| {
        wasm_bindgen_futures::future_to_promise(async move {
            let value = draw(text, (w, h), true).await?;
            let pixels = Reflect::get(&value, &"pixels".into())?;
            Ok(Uint8Array::new(&pixels).into())
        })
    });
    let _ = Reflect::set(&window, &"__DDI_SVG_PIXELS".into(), hook.as_ref());
    hook.forget();
}

/// What a worker answered for `text` drawn at `size`: its message, with
/// the `bitmap` or the `pixels`, and their `hash`.
async fn draw(text: String, size: (u32, u32), raw: bool) -> Result<JsValue, JsValue> {
    if let Some(why) = POOL.with_borrow(|p| p.broken.clone()) {
        return Err(JsValue::from_str(&why));
    }
    if let Err(e) = module().await {
        POOL.with_borrow_mut(|p| p.broken = Some(e.clone()));
        return Err(JsValue::from_str(&e));
    }
    let mut job = Some(Job {
        id: 0,
        text,
        size,
        raw,
    });
    let drawn = Promise::new(&mut |resolve, reject| {
        POOL.with_borrow_mut(|p| {
            let mut job = job.take().expect("the promise runs its executor once");
            p.next = p.next.wrapping_add(1);
            job.id = p.next;
            p.waiting.insert(job.id, (resolve, reject));
            p.queue.push_back(job);
        });
    });
    pump();
    JsFuture::from(drawn).await
}

/// Hands waiting images to idle workers, starting more while there are
/// images and fewer than the device can spare; arms the idle timer when
/// nothing is left.
fn pump() {
    let start = POOL.with_borrow_mut(|p| {
        p.idle = None;
        let idle: Vec<u32> = p
            .workers
            .iter()
            .filter(|(_, w)| matches!(w.slot, Slot::Idle))
            .map(|(n, _)| *n)
            .collect();
        for n in idle {
            let Some(job) = p.queue.pop_front() else {
                break;
            };
            let w = p.workers.get_mut(&n).expect("an idle worker");
            let msg = Object::new();
            let set = |k: &str, v: &JsValue| {
                let _ = Reflect::set(&msg, &JsValue::from_str(k), v);
            };
            set("id", &JsValue::from(job.id));
            set("text", &JsValue::from_str(&job.text));
            set("width", &JsValue::from(job.size.0));
            set("height", &JsValue::from(job.size.1));
            set("raw", &JsValue::from(job.raw));
            match w.worker.post_message(&msg) {
                Ok(()) => w.slot = Slot::Busy(job.id),
                Err(e) => {
                    if let Some((_, reject)) = p.waiting.remove(&job.id) {
                        let _ = reject.call1(
                            &JsValue::NULL,
                            &JsValue::from_str(&js_err("sending an SVG to its worker", e)),
                        );
                    }
                }
            }
        }
        let starting = p
            .workers
            .values()
            .filter(|w| matches!(w.slot, Slot::Starting))
            .count();
        if p.queue.is_empty() && p.workers.values().all(|w| matches!(w.slot, Slot::Idle)) {
            if !p.workers.is_empty() {
                p.idle = Some(gloo::timers::callback::Timeout::new(IDLE_MS, || {
                    POOL.with_borrow_mut(|p| {
                        p.idle = None;
                        if p.queue.is_empty() {
                            p.workers.clear();
                        }
                    });
                }));
            }
            return 0;
        }
        let cores = web_sys::window()
            .map(|w| w.navigator().hardware_concurrency() as usize)
            .unwrap_or(2);
        let most = cores.saturating_sub(1).clamp(1, MAX_WORKERS);
        p.queue
            .len()
            .saturating_sub(starting)
            .min(most.saturating_sub(p.workers.len()))
    });
    for _ in 0..start {
        if let Err(e) = start_worker() {
            fail_all(&e);
            return;
        }
    }
}

/// Every waiting image fails with `why`, and so does every later one.
fn fail_all(why: &str) {
    let rejects: Vec<Function> = POOL.with_borrow_mut(|p| {
        p.broken = Some(why.to_string());
        p.queue.clear();
        p.workers.clear();
        p.waiting.drain().map(|(_, (_, reject))| reject).collect()
    });
    for reject in rejects {
        let _ = reject.call1(&JsValue::NULL, &JsValue::from_str(why));
    }
}

fn start_worker() -> Result<(), String> {
    let (glue, wasm) = urls();
    let (module, boot, n) = POOL.with_borrow_mut(|p| {
        let boot = match &p.boot {
            Some(b) => Ok(b.clone()),
            None => {
                super::files::bytes_blob(BOOT_JS.as_bytes(), "text/javascript").and_then(|blob| {
                    web_sys::Url::create_object_url_with_blob(&blob)
                        .map_err(|e| js_err("the SVG worker's script", e))
                })
            }
        };
        if let Ok(b) = &boot {
            p.boot = Some(b.clone());
        }
        p.next = p.next.wrapping_add(1);
        (p.module.clone(), boot, p.next)
    });
    let module = module.ok_or("the SVG worker is not compiled")?;
    let worker = Worker::new(&boot?).map_err(|e| js_err("starting the SVG worker", e))?;
    let on_message = Closure::<dyn FnMut(MessageEvent)>::new(move |event: MessageEvent| {
        heard(n, event.data());
    });
    let on_error = Closure::<dyn FnMut(Event)>::new(move |event: Event| {
        event.prevent_default();
        let detail = Reflect::get(&event, &JsValue::from_str("message"))
            .ok()
            .and_then(|m| m.as_string())
            .filter(|m| !m.is_empty())
            .unwrap_or_else(|| "it could not be loaded or stopped".into());
        lost(n, &format!("the SVG worker failed: {detail}"));
    });
    worker.set_onmessage(Some(on_message.as_ref().unchecked_ref()));
    worker.set_onerror(Some(on_error.as_ref().unchecked_ref()));
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
        worker.post_message(&msg)
    };
    start(true)
        .or_else(|_| start(false))
        .map_err(|e| js_err("starting the SVG worker", e))?;
    POOL.with_borrow_mut(|p| {
        p.workers.insert(
            n,
            Running {
                worker,
                slot: Slot::Starting,
                _on_message: on_message,
                _on_error: on_error,
            },
        )
    });
    gloo::timers::callback::Timeout::new(READY_MS, move || {
        let late = POOL.with_borrow(|p| {
            p.workers
                .get(&n)
                .is_some_and(|w| matches!(w.slot, Slot::Starting))
        });
        if late {
            lost(n, "the SVG worker did not start in time");
        }
    })
    .forget();
    Ok(())
}

/// What worker `n` said.
fn heard(n: u32, data: JsValue) {
    let get = |k: &str| Reflect::get(&data, &JsValue::from_str(k)).ok();
    if get("ready").is_some_and(|v| v.is_truthy()) {
        POOL.with_borrow_mut(|p| {
            if let Some(w) = p.workers.get_mut(&n) {
                w.slot = Slot::Idle;
            }
        });
        pump();
        return;
    }
    if let Some(why) = get("failed").and_then(|f| f.as_string()) {
        lost(n, &why);
        return;
    }
    let id = get("id").and_then(|v| v.as_f64()).map(|v| v as u32);
    let settle = POOL.with_borrow_mut(|p| {
        if let Some(w) = p.workers.get_mut(&n) {
            w.slot = Slot::Idle;
        }
        if let Some(ms) = get("ms").and_then(|v| v.as_f64()) {
            p.drawn += 1;
            p.drawing_ms += ms;
            publish(p);
        }
        id.and_then(|id| p.waiting.remove(&id))
    });
    if let Some((resolve, reject)) = settle {
        if let Some(error) = get("error").and_then(|e| e.as_string()) {
            // The worker refused the image itself (its check, its layers'
            // cost): no one else should draw it instead ([`refusal`]).
            let refused = js_sys::Error::new(&error);
            let _ = Reflect::set(&refused, &"refused".into(), &JsValue::TRUE);
            let _ = reject.call1(&JsValue::NULL, &refused);
        } else {
            let _ = resolve.call1(&JsValue::NULL, &data);
        }
    }
    pump();
}

/// Worker `n` is gone: the image it drew fails (another worker takes the
/// rest); when it never started and no other did, none will, and every
/// image fails.
fn lost(n: u32, why: &str) {
    let (reject, never_started) = POOL.with_borrow_mut(|p| {
        let slot = p.workers.remove(&n).map(|w| w.slot);
        let reject = match slot {
            Some(Slot::Busy(id)) => p.waiting.remove(&id).map(|(_, r)| r),
            _ => None,
        };
        let none_ready = p.workers.values().all(|w| matches!(w.slot, Slot::Starting));
        (reject, matches!(slot, Some(Slot::Starting)) && none_ready)
    });
    if let Some(reject) = reject {
        let _ = reject.call1(&JsValue::NULL, &JsValue::from_str(why));
    }
    web_sys::console::warn_1(&why.into());
    if never_started {
        fail_all(why);
    } else {
        pump();
    }
}

/// `window.__DDI_SVG`: `{drawn, drawingMs}`.
fn publish(p: &Pool) {
    let Some(w) = web_sys::window() else {
        return;
    };
    let o = Object::new();
    let _ = Reflect::set(&o, &"drawn".into(), &JsValue::from(p.drawn));
    let _ = Reflect::set(&o, &"drawingMs".into(), &JsValue::from(p.drawing_ms));
    let _ = Reflect::set(&w, &JsValue::from_str("__DDI_SVG"), &o);
}
