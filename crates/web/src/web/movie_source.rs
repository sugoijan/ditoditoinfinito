//! Where a movie's bytes come from: a `Blob` (a stored or linked file, a
//! bundled song's file fetched whole) or a URL with a known size (a
//! shared song's movie, `crate::share`), read by ranges as the decoders
//! ask for them, so a play starts before the movie is down (the decoder
//! worker reads it with synchronous ranged requests,
//! `web/video_worker.js`; the browser path and the probes read it here).

use js_sys::Object;
use wasm_bindgen::{JsCast, JsValue};
use web_sys::Blob;

#[derive(Clone, Debug)]
pub(crate) enum MovieSource {
    Blob(Blob),
    /// At `url`, `size` bytes, served with ranges.
    Remote {
        url: String,
        size: u64,
    },
}

impl MovieSource {
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
            MovieSource::Remote { url, .. } => {
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
            MovieSource::Remote { url, size } => {
                set("url", url.as_str().into());
                set("size", (*size as f64).into());
            }
        }
    }
}
