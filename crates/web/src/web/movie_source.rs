//! Where a movie's bytes come from: a `Blob` (a stored or linked file, a
//! bundled song's file fetched whole) or a URL with a known size (a
//! shared song's movie, `crate::share`), read as the decoders ask, so a
//! play starts before the movie is down. The decoder worker buffers a
//! remote movie itself (`web/video_worker.js`); the browser path and
//! the probes read through [`RemoteBuffer`] here: the movie fetched whole
//! from the first read, in order, a read inside what has come served at
//! once, one at its edge waiting for the download, one elsewhere (an
//! index at the end, a seek past the download) fetched by range.

use std::cell::RefCell;
use std::future::Future;
use std::pin::Pin;
use std::rc::Rc;
use std::task::{Context, Poll, Waker};

use js_sys::{Object, Uint8Array};
use wasm_bindgen::{JsCast, JsValue};
use wasm_bindgen_futures::JsFuture;
use web_sys::Blob;

/// A remote movie is kept up to this much; past it, reads go by range.
const BUFFER_CAP: u64 = 256 * 1024 * 1024;

#[derive(Clone, Debug)]
pub(crate) enum MovieSource {
    Blob(Blob),
    /// At `url`, `size` bytes, served with ranges; `buffer` its download
    /// on this thread (the browser path's and the probe's reads).
    Remote {
        url: String,
        size: u64,
        buffer: Rc<RefCell<RemoteBuffer>>,
    },
}

/// A remote movie's download, in order from its start.
#[derive(Debug, Default)]
pub(crate) struct RemoteBuffer {
    chunks: Vec<(u64, Vec<u8>)>,
    buffered: u64,
    started: bool,
    done: bool,
    failed: Option<String>,
    waiters: Vec<Waker>,
}

impl RemoteBuffer {
    fn copy(&self, start: u64, end: u64) -> Vec<u8> {
        let mut out = Vec::with_capacity((end - start) as usize);
        let first = self
            .chunks
            .partition_point(|(at, data)| at + data.len() as u64 <= start);
        for (at, data) in &self.chunks[first..] {
            if *at >= end {
                break;
            }
            let from = start.max(*at) - at;
            let to = (end.min(at + data.len() as u64)) - at;
            out.extend_from_slice(&data[from as usize..to as usize]);
        }
        out
    }

    fn wake(&mut self) {
        for w in self.waiters.drain(..) {
            w.wake();
        }
    }
}

impl MovieSource {
    pub(crate) fn remote(url: String, size: u64) -> MovieSource {
        MovieSource::Remote {
            url,
            size,
            buffer: Rc::default(),
        }
    }

    pub(crate) fn size(&self) -> u64 {
        match self {
            MovieSource::Blob(b) => b.size() as u64,
            MovieSource::Remote { size, .. } => *size,
        }
    }

    /// Bytes `[start, end)` (cut at the size).
    pub(crate) async fn read(&self, start: u64, end: u64) -> Result<Vec<u8>, String> {
        let end = end.min(self.size());
        if start >= end {
            return Ok(Vec::new());
        }
        match self {
            MovieSource::Blob(blob) => {
                let part = blob
                    .slice_with_f64_and_f64(start as f64, end as f64)
                    .map_err(|e| format!("reading the movie: {e:?}"))?;
                super::files::blob_bytes(&part).await
            }
            MovieSource::Remote { url, size, buffer } => {
                if !buffer.borrow().started {
                    start_buffering(url.clone(), buffer.clone());
                }
                // Within the download, now or once it gets there (the
                // start of the file, read in order); elsewhere by range.
                if end <= BUFFER_CAP.min(*size) {
                    Buffered {
                        buffer: buffer.clone(),
                        end,
                    }
                    .await;
                    let b = buffer.borrow();
                    if end <= b.buffered {
                        return Ok(b.copy(start, end));
                    }
                }
                fetch_range(url, start, end).await
            }
        }
    }

    /// The source in a message to the decoder worker: `blob`, or `url`
    /// and `size`.
    pub(crate) fn put_into(&self, msg: &Object) {
        let set = |k: &str, v: JsValue| {
            let _ = js_sys::Reflect::set(msg, &k.into(), &v);
        };
        match self {
            MovieSource::Blob(b) => set("blob", b.clone().unchecked_into()),
            MovieSource::Remote { url, size, .. } => {
                set("url", url.as_str().into());
                set("size", (*size as f64).into());
            }
        }
    }
}

/// Resolves once the download reaches `end`, ends or fails.
struct Buffered {
    buffer: Rc<RefCell<RemoteBuffer>>,
    end: u64,
}

impl Future for Buffered {
    type Output = ();

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<()> {
        let mut b = self.buffer.borrow_mut();
        if b.buffered >= self.end || b.done {
            return Poll::Ready(());
        }
        b.waiters.push(cx.waker().clone());
        Poll::Pending
    }
}

/// The movie at `url` fetched whole into `buffer`, in order, up to the
/// cap, on a task of its own.
fn start_buffering(url: String, buffer: Rc<RefCell<RemoteBuffer>>) {
    buffer.borrow_mut().started = true;
    wasm_bindgen_futures::spawn_local(async move {
        let result = buffer_whole(&url, &buffer).await;
        let mut b = buffer.borrow_mut();
        if let Err(e) = result {
            web_sys::console::warn_1(&format!("movie download: {e}").into());
            b.failed = Some(e);
        }
        b.done = true;
        b.wake();
    });
}

async fn buffer_whole(url: &str, buffer: &Rc<RefCell<RemoteBuffer>>) -> Result<(), String> {
    let window = web_sys::window().ok_or("no window")?;
    // Out of the HTTP cache: a ranged request for the same URL while this
    // one streams would otherwise wait on the entry it is writing.
    let init = web_sys::RequestInit::new();
    init.set_cache(web_sys::RequestCache::NoStore);
    let resp: web_sys::Response = JsFuture::from(window.fetch_with_str_and_init(url, &init))
        .await
        .map_err(|e| format!("{e:?}"))?
        .dyn_into()
        .map_err(|_| "not a response")?;
    if !resp.ok() {
        return Err(format!("HTTP {}", resp.status()));
    }
    let body = resp.body().ok_or("no body")?;
    let reader: web_sys::ReadableStreamDefaultReader =
        body.get_reader().dyn_into().map_err(|_| "no reader")?;
    loop {
        let chunk = JsFuture::from(reader.read())
            .await
            .map_err(|e| format!("{e:?}"))?;
        let done = js_sys::Reflect::get(&chunk, &"done".into())
            .ok()
            .and_then(|d| d.as_bool())
            .unwrap_or(true);
        if done {
            break;
        }
        let value: Uint8Array = js_sys::Reflect::get(&chunk, &"value".into())
            .map_err(|e| format!("{e:?}"))?
            .dyn_into()
            .map_err(|_| "not bytes")?;
        let data = value.to_vec();
        let full = {
            let mut b = buffer.borrow_mut();
            let at = b.buffered;
            b.buffered += data.len() as u64;
            b.chunks.push((at, data));
            b.wake();
            b.buffered >= BUFFER_CAP
        };
        if full {
            let _ = JsFuture::from(reader.cancel()).await;
            break;
        }
    }
    Ok(())
}

/// Bytes `[start, end)` of the file at `url` by a ranged request.
async fn fetch_range(url: &str, start: u64, end: u64) -> Result<Vec<u8>, String> {
    let resp = gloo::net::http::Request::get(url)
        .header("range", &format!("bytes={start}-{}", end - 1))
        .send()
        .await
        .map_err(|e| format!("reading the movie: {e}"))?;
    match resp.status() {
        206 => resp
            .binary()
            .await
            .map_err(|e| format!("reading the movie: {e}")),
        // The whole file (no ranges): the part asked for.
        200 => {
            let all = resp
                .binary()
                .await
                .map_err(|e| format!("reading the movie: {e}"))?;
            let end = (end as usize).min(all.len());
            Ok(all
                .get(start as usize..end)
                .map(<[u8]>::to_vec)
                .unwrap_or_default())
        }
        s => Err(format!("reading the movie: HTTP {s}")),
    }
}
