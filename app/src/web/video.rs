//! Background movies decoded by the FFmpeg module in a worker
//! (`video_worker.js`; `docs/plans/video-backgrounds.md`, step 2).
//!
//! One [`VideoWorker`] per play session holds the worker; each open movie is
//! a [`WorkerVideo`] implementing [`VideoDecoder`]. The worker's script is
//! part of the app, started from a blob URL, so it always speaks the app's
//! protocol; it fetches the module (`video/ddivideo.wasm`) on the first
//! open, and a build without the module reports that as an error.
//! Dropping the last handle terminates the worker.

use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::rc::Rc;

use ddi_platform::video::{
    VideoBackend, VideoCodec, VideoDecoder, VideoEvent, VideoFrame, VideoInfo, YuvMatrix,
};
use js_sys::{ArrayBuffer, Object, Reflect, Uint8Array};
use wasm_bindgen::prelude::*;
use web_sys::{Blob, Event, MessageEvent, Worker};

use super::js_err;

const WORKER_JS: &str = include_str!("video_worker.js");

/// Where the FFmpeg module is.
#[derive(Clone, Debug, PartialEq)]
pub(crate) enum ModuleStatus {
    /// Not asked for yet (no movie opened).
    Idle,
    /// Downloading; `total` is the module's size in bytes.
    Loading {
        loaded: f64,
        total: f64,
    },
    Ready {
        load_ms: f64,
    },
    Failed(String),
}

struct Movie {
    /// Seeks so far; frames tagged with an older one are dropped.
    generation: u32,
    events: Vec<VideoEvent>,
}

struct Shared {
    status: ModuleStatus,
    movies: HashMap<u32, Movie>,
}

struct Inner {
    worker: Worker,
    script_url: String,
    shared: Rc<RefCell<Shared>>,
    next_id: Cell<u32>,
    _on_message: Closure<dyn FnMut(MessageEvent)>,
    _on_error: Closure<dyn FnMut(Event)>,
}

impl Drop for Inner {
    fn drop(&mut self) {
        self.worker.set_onmessage(None);
        self.worker.set_onerror(None);
        self.worker.terminate();
        let _ = web_sys::Url::revoke_object_url(&self.script_url);
    }
}

/// The decoder worker of a play session.
#[derive(Clone)]
pub(crate) struct VideoWorker(Rc<Inner>);

impl VideoWorker {
    pub(crate) fn new() -> Result<VideoWorker, String> {
        let blob = super::files::bytes_blob(WORKER_JS.as_bytes(), "text/javascript")?;
        let script_url = web_sys::Url::create_object_url_with_blob(&blob)
            .map_err(|e| js_err("worker script URL", e))?;
        let worker =
            Worker::new(&script_url).map_err(|e| js_err("starting the video worker", e))?;
        let shared = Rc::new(RefCell::new(Shared {
            status: ModuleStatus::Idle,
            movies: HashMap::new(),
        }));
        let on_message = {
            let shared = Rc::clone(&shared);
            Closure::<dyn FnMut(MessageEvent)>::new(move |event: MessageEvent| {
                receive(&mut shared.borrow_mut(), &event.data());
            })
        };
        worker.set_onmessage(Some(on_message.as_ref().unchecked_ref()));
        // The script failed or the worker died: nothing more will arrive.
        let on_error = {
            let shared = Rc::clone(&shared);
            Closure::<dyn FnMut(Event)>::new(move |event: Event| {
                event.prevent_default();
                let detail = text(&event, "message");
                let message = if detail.is_empty() {
                    "the video worker stopped".to_string()
                } else {
                    format!("the video worker stopped: {detail}")
                };
                let mut shared = shared.borrow_mut();
                shared.status = ModuleStatus::Failed(message.clone());
                for movie in shared.movies.values_mut() {
                    movie.events.push(VideoEvent::Error(message.clone()));
                }
            })
        };
        worker.set_onerror(Some(on_error.as_ref().unchecked_ref()));
        let base = crate::songs::asset_url("video/");
        post(&worker, &message("init", &[("base", base.into())]));
        Ok(VideoWorker(Rc::new(Inner {
            worker,
            script_url,
            shared,
            next_id: Cell::new(1),
            _on_message: on_message,
            _on_error: on_error,
        })))
    }

    pub(crate) fn status(&self) -> ModuleStatus {
        self.0.shared.borrow().status.clone()
    }
}

impl VideoBackend for VideoWorker {
    type Source = Blob;
    type Decoder = WorkerVideo;

    fn open(&mut self, source: Blob, name: &str, _codec: &VideoCodec) -> WorkerVideo {
        let id = self.0.next_id.get();
        self.0.next_id.set(id + 1);
        self.0.shared.borrow_mut().movies.insert(
            id,
            Movie {
                generation: 0,
                events: Vec::new(),
            },
        );
        let msg = message(
            "open",
            &[
                ("id", id.into()),
                ("blob", source.into()),
                ("name", name.into()),
            ],
        );
        post(&self.0.worker, &msg);
        WorkerVideo {
            id,
            generation: 0,
            pending: 0,
            worker: Rc::clone(&self.0),
        }
    }
}

/// One movie open in the worker; dropping it closes the movie.
pub(crate) struct WorkerVideo {
    id: u32,
    generation: u32,
    /// Frames asked for in this generation and not polled yet.
    pending: u32,
    worker: Rc<Inner>,
}

impl VideoDecoder for WorkerVideo {
    fn seek(&mut self, t: f64) {
        self.generation += 1;
        self.pending = 0;
        if let Some(movie) = self.worker.shared.borrow_mut().movies.get_mut(&self.id) {
            movie.generation = self.generation;
            // Frames that arrived before the seek but were not polled yet.
            movie
                .events
                .retain(|e| !matches!(e, VideoEvent::Frame(_) | VideoEvent::End { .. }));
        }
        let msg = message(
            "seek",
            &[
                ("id", self.id.into()),
                ("t", t.into()),
                ("gen", self.generation.into()),
            ],
        );
        post(&self.worker.worker, &msg);
    }

    fn want(&mut self, n: u32) {
        self.pending += n;
        let msg = message(
            "want",
            &[
                ("id", self.id.into()),
                ("n", n.into()),
                ("gen", self.generation.into()),
            ],
        );
        post(&self.worker.worker, &msg);
    }

    fn pending(&self) -> u32 {
        self.pending
    }

    fn poll(&mut self, out: &mut Vec<VideoEvent>) {
        let mut shared = self.worker.shared.borrow_mut();
        let Some(movie) = shared.movies.get_mut(&self.id) else {
            return;
        };
        for event in &movie.events {
            match event {
                VideoEvent::Frame(_) => self.pending = self.pending.saturating_sub(1),
                VideoEvent::End { .. } | VideoEvent::Error(_) => self.pending = 0,
                VideoEvent::Opened(_) => {}
            }
        }
        out.append(&mut movie.events);
    }
}

impl Drop for WorkerVideo {
    fn drop(&mut self) {
        self.worker.shared.borrow_mut().movies.remove(&self.id);
        post(
            &self.worker.worker,
            &message("close", &[("id", self.id.into())]),
        );
    }
}

fn message(kind: &str, fields: &[(&str, JsValue)]) -> Object {
    let msg = Object::new();
    let _ = Reflect::set(&msg, &"type".into(), &kind.into());
    for (key, value) in fields {
        let _ = Reflect::set(&msg, &(*key).into(), value);
    }
    msg
}

fn post(worker: &Worker, msg: &Object) {
    if let Err(e) = worker.post_message(msg) {
        web_sys::console::warn_1(&format!("video worker: {e:?}").into());
    }
}

fn field(msg: &JsValue, key: &str) -> JsValue {
    Reflect::get(msg, &key.into()).unwrap_or(JsValue::UNDEFINED)
}

fn number(msg: &JsValue, key: &str) -> f64 {
    field(msg, key).as_f64().unwrap_or(0.0)
}

fn text(msg: &JsValue, key: &str) -> String {
    field(msg, key).as_string().unwrap_or_default()
}

/// Applies one message from the worker.
fn receive(shared: &mut Shared, msg: &JsValue) {
    let kind = text(msg, "type");
    match kind.as_str() {
        "loading" => {
            shared.status = ModuleStatus::Loading {
                loaded: number(msg, "loaded"),
                total: number(msg, "total"),
            }
        }
        "ready" => {
            shared.status = ModuleStatus::Ready {
                load_ms: number(msg, "ms"),
            }
        }
        "failed" => shared.status = ModuleStatus::Failed(text(msg, "message")),
        _ => {}
    }
    let id = number(msg, "id") as u32;
    let Some(movie) = shared.movies.get_mut(&id) else {
        return;
    };
    let current = number(msg, "gen") as u32 == movie.generation;
    let event = match kind.as_str() {
        "opened" => VideoEvent::Opened(VideoInfo {
            width: number(msg, "width") as u32,
            height: number(msg, "height") as u32,
            duration: field(msg, "duration").as_f64(),
            codec: text(msg, "codec"),
        }),
        "frame" if current => {
            let planes = field(msg, "planes")
                .dyn_into::<ArrayBuffer>()
                .map(|b| Uint8Array::new(&b).to_vec())
                .unwrap_or_default();
            VideoEvent::Frame(VideoFrame {
                pts: number(msg, "pts"),
                width: number(msg, "width") as u32,
                height: number(msg, "height") as u32,
                matrix: if number(msg, "colorspace") == 709.0 {
                    YuvMatrix::Bt709
                } else {
                    YuvMatrix::Bt601
                },
                full_range: field(msg, "full_range").is_truthy(),
                planes,
                decode_ms: number(msg, "decode_ms"),
            })
        }
        "eof" if current => VideoEvent::End {
            last_pts: field(msg, "last").as_f64(),
        },
        "error" => VideoEvent::Error(text(msg, "message")),
        _ => return,
    };
    movie.events.push(event);
}
