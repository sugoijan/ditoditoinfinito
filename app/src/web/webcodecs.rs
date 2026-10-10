//! Background movies decoded by the browser (WebCodecs; H.264 in AVI,
//! `docs/plans/video-backgrounds.md`, step 6). Most pack movies are H.264,
//! which every browser decodes, usually in hardware: those play without
//! the FFmpeg module, which is then not even fetched.
//!
//! [`WebCodecsVideo`] is a [`VideoDecoder`] like the module's. It reads the
//! AVI's index (`ddi_library::avi`) and feeds the browser's decoder one
//! chunk at a time. Each chunk's picture order count, read from its slice
//! header, replays FFmpeg's choice of which picture each packet releases
//! (`h264::OutputOrder`): that is the display order and the times the
//! module gives. A browser frame is known by its own chunk (carried in its
//! timestamp), so the order the browser gives frames out in does not
//! matter (Chrome reorders by the bitstream, Firefox and Safari by the
//! timestamps). Seeks are the module's (`h264::seek_start`, `ExactSeek`).
//!
//! Frames are copied out of the browser (`VideoFrame.copyTo`, I420 or
//! NV12) into the module's YUV planes and closed at once: a hardware
//! decoder has few output buffers, and the track keeps frames queued (and
//! short movies whole). Only streams whose reorder delay is fixed from the
//! start (the SPS's bitstream restriction), progressive, 8-bit 4:2:0, with
//! POC type 0 or 2 and an index come here; anything else, any failure, and
//! a decoder that stops answering are the module's (the caller falls
//! back).
//!
//! Only YUV frames give the module's pixels. A browser whose hardware
//! decoder hands out RGB (Firefox on macOS gives `BGRX`, already converted
//! with its own guess at the matrix and range) is asked for its software
//! decoder instead (`hardwareAcceleration: "prefer-software"`, from then
//! on); when that is refused or gives RGB too, the browser path is off for
//! the session ([`available`]) and every movie plays through the module.
//!
//! Runs on the page: the decoding is the browser's, off the main thread.

use std::cell::{Cell, RefCell};
use std::collections::{BTreeSet, HashMap, VecDeque};
use std::rc::Rc;

use ddi_library::avi::{self, Avi, Chunk};
use ddi_library::h264::{self, ExactSeek, Offer, OutputOrder, PictureOrder, Stream};
use ddi_platform::video::{
    VideoCodec, VideoContainer, VideoDecoder, VideoEvent, VideoFormat, VideoFrame, VideoInfo,
    YuvMatrix,
};
use js_sys::{Array, Object, Promise, Reflect, Uint8Array};
use wasm_bindgen::prelude::*;
use wasm_bindgen_futures::{JsFuture, spawn_local};
use web_sys::Blob;

use super::clock::PerformanceClock;
use super::js_err;
use super::video::Picture;

#[wasm_bindgen]
extern "C" {
    #[wasm_bindgen(js_name = VideoDecoder)]
    type JsVideoDecoder;
    #[wasm_bindgen(constructor, js_class = "VideoDecoder", catch)]
    fn new(init: &Object) -> Result<JsVideoDecoder, JsValue>;
    #[wasm_bindgen(method, catch)]
    fn configure(this: &JsVideoDecoder, config: &Object) -> Result<(), JsValue>;
    #[wasm_bindgen(method, catch)]
    fn decode(this: &JsVideoDecoder, chunk: &JsChunk) -> Result<(), JsValue>;
    #[wasm_bindgen(method)]
    fn flush(this: &JsVideoDecoder) -> Promise;
    #[wasm_bindgen(method, catch)]
    fn reset(this: &JsVideoDecoder) -> Result<(), JsValue>;
    #[wasm_bindgen(method, catch)]
    fn close(this: &JsVideoDecoder) -> Result<(), JsValue>;
    #[wasm_bindgen(method, getter, js_name = decodeQueueSize)]
    fn decode_queue_size(this: &JsVideoDecoder) -> u32;
    #[wasm_bindgen(static_method_of = JsVideoDecoder, js_class = "VideoDecoder", js_name = isConfigSupported)]
    fn is_config_supported(config: &Object) -> Promise;

    #[wasm_bindgen(js_name = EncodedVideoChunk)]
    type JsChunk;
    #[wasm_bindgen(constructor, js_class = "EncodedVideoChunk", catch)]
    fn new(init: &Object) -> Result<JsChunk, JsValue>;

    #[wasm_bindgen(js_name = VideoFrame)]
    type JsFrame;
    #[wasm_bindgen(method, getter)]
    fn format(this: &JsFrame) -> Option<String>;
    #[wasm_bindgen(method, getter)]
    fn timestamp(this: &JsFrame) -> f64;
    #[wasm_bindgen(method, getter, js_name = visibleRect)]
    fn visible_rect(this: &JsFrame) -> Option<Object>;
    #[wasm_bindgen(method, getter, js_name = colorSpace)]
    fn color_space(this: &JsFrame) -> Option<Object>;
    #[wasm_bindgen(method, catch, js_name = allocationSize)]
    fn allocation_size(this: &JsFrame, options: &Object) -> Result<f64, JsValue>;
    #[wasm_bindgen(method, js_name = copyTo)]
    fn copy_to(this: &JsFrame, destination: &Uint8Array, options: &Object) -> Promise;
    #[wasm_bindgen(method, js_name = close)]
    fn close_frame(this: &JsFrame);
}

/// Chunk timestamps carry the run (`× RUN_SPAN`) and the chunk's index, so
/// every frame is known by its chunk and a frame from before a seek is
/// told apart. Browsers keep timestamps as integer microseconds.
const RUN_SPAN: f64 = 16_777_216.0;
/// Chunks fed whose frames have not come out, at most.
const IN_FLIGHT: usize = 32;
/// Frames given out beyond a missing one before it counts as one the
/// browser dropped.
const SKIP_AFTER: usize = 8;
/// With frames wanted, a decoder silent this long (ms) is fed one more
/// chunk ahead (it holds more than thought)…
const NUDGE_MS: f64 = 40.0;
/// …and one silent this long has stopped: the module takes over.
const STALL_MS: f64 = 4000.0;
/// Bytes read from the file at a time, and chunks.
const READ_BYTES: u64 = 1 << 20;
const READ_CHUNKS: usize = 32;
/// FFmpeg's probe holds a frame back when it sees a B slice in a stream
/// whose SPS says none (`demux.c`); chunks looked at for that.
const PROBE_CHUNKS: usize = 64;

/// How the browser's decoder is asked to decode, learned this session.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Mode {
    /// As the browser likes (usually in hardware).
    Default,
    /// Its default decoder gave RGB frames: its software decoder.
    Software,
    /// That gave RGB too, or is refused: the module plays every movie.
    Off,
}

thread_local! {
    static MODE: Cell<Mode> = const { Cell::new(Mode::Default) };
}

/// The start of the error for an RGB frame, which changes [`MODE`].
const RGB_FRAMES: &str = "frames in RGB";

/// An RGB frame came out with the decoder in `software` or not: whether to
/// try the software decoder (else the browser path is off).
fn rgb_frames(software: bool) -> bool {
    let next = if software { Mode::Off } else { Mode::Software };
    MODE.with(|m| {
        if m.get() != Mode::Off && m.get() != next {
            web_sys::console::info_1(
                &match next {
                    Mode::Software => "video: the browser's decoder gives RGB frames; asking for its software decoder",
                    _ => "video: the browser gives only RGB frames; the video decoder plays every movie",
                }
                .into(),
            );
            m.set(next);
        }
    });
    !software
}

/// Testing aid (`window.__DDI_FAKE_RGB = true`): the default decoder's
/// frames count as RGB, as Firefox on macOS gives them, so the switch to
/// the software decoder can be checked in Chrome.
fn fake_rgb() -> bool {
    Reflect::get(&js_sys::global(), &"__DDI_FAKE_RGB".into()).is_ok_and(|v| v.is_truthy())
}

/// Whether this browser has WebCodecs video decoding (and the headless
/// tests have not asked for the module: `window.__DDI_VIDEO_DECODER =
/// "module"`).
pub(crate) fn available() -> bool {
    if MODE.with(Cell::get) == Mode::Off {
        return false;
    }
    let global = js_sys::global();
    let forced = Reflect::get(&global, &"__DDI_VIDEO_DECODER".into())
        .ok()
        .and_then(|v| v.as_string());
    forced.as_deref() != Some("module")
        && Reflect::has(&global, &"VideoDecoder".into()).unwrap_or(false)
        && Reflect::has(&global, &"EncodedVideoChunk".into()).unwrap_or(false)
}

/// Whether a movie of this format may play through WebCodecs (the file
/// itself decides the rest when it is opened).
pub(crate) fn may_play(format: &VideoFormat) -> bool {
    format.container == VideoContainer::Avi && format.codec == VideoCodec::H264 && available()
}

fn set(obj: &Object, key: &str, value: impl Into<JsValue>) {
    let _ = Reflect::set(obj, &key.into(), &value.into());
}

fn get(obj: &JsValue, key: &str) -> JsValue {
    Reflect::get(obj, &key.into()).unwrap_or(JsValue::UNDEFINED)
}

async fn read(blob: &Blob, start: u64, end: u64) -> Result<Vec<u8>, String> {
    let end = end.min(blob.size() as u64);
    if start >= end {
        return Ok(Vec::new());
    }
    let part = blob
        .slice_with_f64_and_f64(start as f64, end as f64)
        .map_err(|e| js_err("reading the movie", e))?;
    super::files::blob_bytes(&part).await
}

/// A movie's stream, as far as decoding it here needs.
struct Media {
    avi: Avi,
    chunks: Vec<Chunk>,
    stream: Stream,
    delay: u32,
    config: Object,
    /// Decoding in software (`hardwareAcceleration: "prefer-software"`).
    software: bool,
}

impl Media {
    fn frame_time(&self) -> f64 {
        self.avi.frame_time()
    }

    /// Chunk `i` for the browser: a key chunk only for an IDR (with the
    /// parameter sets in front, for Annex B streams).
    fn encoded(&self, i: usize, run: u32, data: &[u8], idr: bool) -> Result<JsChunk, String> {
        let bytes = if idr && !self.stream.parameter_sets.is_empty() {
            let mut out = self.stream.parameter_sets.clone();
            out.extend_from_slice(data);
            out
        } else {
            data.to_vec()
        };
        let init = Object::new();
        set(&init, "type", if idr { "key" } else { "delta" });
        set(&init, "timestamp", f64::from(run) * RUN_SPAN + i as f64);
        set(&init, "data", Uint8Array::from(&bytes[..]));
        JsChunk::new(&init).map_err(|e| js_err("EncodedVideoChunk", e))
    }
}

/// The video chunks from the file's index: the OpenDML one when there is
/// one (FFmpeg prefers it), else the legacy one, looking past `JUNK`.
async fn index(blob: &Blob, avi: &Avi) -> Result<Vec<Chunk>, String> {
    if !avi.odml.is_empty() {
        let mut all = Vec::new();
        let mut frame = avi.start;
        for &(at, size) in &avi.odml {
            let (part, next) = avi.parse_ix(&read(blob, at, at + u64::from(size)).await?, frame);
            all.extend(part);
            frame = next;
        }
        return Ok(all);
    }
    let mut at = avi.idx1_at();
    for _ in 0..16 {
        let header = read(blob, at, at + 8).await?;
        if header.starts_with(b"idx1") {
            let size = u64::from(u32::from_le_bytes(
                header[4..8].try_into().unwrap_or([0; 4]),
            ));
            return Ok(avi.parse_idx1(&read(blob, at, at + 8 + size).await?));
        }
        match avi.after(at, &header) {
            Some(next) => at = next,
            None => break,
        }
    }
    Err("the movie has no index".into())
}

/// Reads the AVI's header, index and parameter sets and asks the browser
/// whether it decodes the stream.
async fn load(blob: &Blob) -> Result<Media, String> {
    let head = read(blob, 0, avi::HEADER_LEN as u64).await?;
    let avi = avi::parse_header(&head)
        .map_err(|e| format!("not an AVI the browser path reads: {e:?}"))?;
    let mut chunks = index(blob, &avi).await?;
    // An index pointing past the end of the file (a truncated movie):
    // the chunks up to there.
    let size = blob.size() as u64;
    if let Some(cut) = chunks
        .iter()
        .position(|c| c.offset.saturating_add(u64::from(c.size)) > size)
    {
        chunks.truncate(cut);
    }
    let first = *chunks.first().ok_or("the movie's index is empty")?;
    let head_end = first.offset + u64::from(first.size);
    let data = read(blob, first.offset - 8, head_end).await?;
    if !avi.check_chunk(&data, &first) {
        return Err("the movie's index does not point at its frames".into());
    }
    let stream =
        h264::stream(&avi.extradata, &data[8..]).ok_or("no H.264 parameter sets in the movie")?;
    let sps = &stream.sps;
    let mut delay = sps
        .reorder_delay()
        .ok_or("the stream does not fix its reorder delay")?;
    if !sps.frame_mbs_only
        || sps.chroma_format_idc != 1
        || sps.bit_depth != 8
        || !matches!(sps.poc_type, 0 | 2)
    {
        return Err("not a progressive 8-bit 4:2:0 stream".into());
    }
    if delay == 0 {
        // FFmpeg's probe: a B slice early on holds one frame back.
        let probe: Vec<&Chunk> = chunks
            .iter()
            .take(PROBE_CHUNKS)
            .take_while(|c| c.offset + u64::from(c.size) <= first.offset + READ_BYTES)
            .collect();
        if let Some(last) = probe.last() {
            let span = read(blob, first.offset, last.offset + u64::from(last.size)).await?;
            let mut order = PictureOrder::new(&stream);
            let b = probe.iter().any(|c| {
                let at = (c.offset.saturating_sub(first.offset)) as usize;
                span.get(at..at + c.size as usize)
                    .and_then(|d| order.picture(d, stream.framing, sps))
                    .is_some_and(|p| p.b)
            });
            if b {
                delay = 1;
            }
        }
    }
    let config = Object::new();
    set(&config, "codec", sps.codec());
    set(&config, "codedWidth", sps.width);
    set(&config, "codedHeight", sps.height);
    let software = match MODE.with(Cell::get) {
        Mode::Default => false,
        Mode::Software => true,
        Mode::Off => return Err("the browser gives only RGB frames".into()),
    };
    if software {
        set(&config, "hardwareAcceleration", "prefer-software");
    }
    if let Some(avcc) = &stream.avcc {
        set(&config, "description", Uint8Array::from(&avcc[..]));
    }
    let support = JsFuture::from(JsVideoDecoder::is_config_supported(&config))
        .await
        .map_err(|e| js_err("VideoDecoder.isConfigSupported", e))?;
    if !get(&support, "supported").is_truthy() {
        if software {
            rgb_frames(true);
        }
        return Err(format!("this browser does not decode {}", sps.codec()));
    }
    Ok(Media {
        avi,
        chunks,
        stream,
        delay,
        config,
        software,
    })
}

/// A frame copied out of the browser: its visible picture as YUV 4:2:0
/// planes, rows packed, and its range when the browser says (a decoder may
/// hand over full-range frames from a limited-range stream).
struct Copied {
    width: u32,
    height: u32,
    planes: Vec<u8>,
    full_range: Option<bool>,
}

/// Copies `frame` out. Only the formats a browser's H.264 decoder gives
/// (I420, NV12) are taken; a frame kept on the GPU has none.
async fn planes_of(frame: &JsFrame) -> Result<Copied, String> {
    let format = frame
        .format()
        .ok_or("the browser keeps the frames on the GPU")?;
    if matches!(format.as_str(), "RGBA" | "RGBX" | "BGRA" | "BGRX") {
        return Err(format!("{RGB_FRAMES} ({format})"));
    }
    if !matches!(format.as_str(), "I420" | "I420A" | "NV12") {
        return Err(format!("frames in {format} are not taken"));
    }
    let rect = frame.visible_rect().ok_or("a frame without a picture")?;
    let num = |k: &str| get(&rect, k).as_f64().unwrap_or(0.0);
    let (x, y, w, h) = (
        num("x"),
        num("y"),
        num("width") as usize,
        num("height") as usize,
    );
    if w == 0 || h == 0 {
        return Err("an empty frame".into());
    }
    let full_range = frame
        .color_space()
        .and_then(|c| get(&c, "fullRange").as_bool());
    let options = Object::new();
    let r = Object::new();
    set(&r, "x", x);
    set(&r, "y", y);
    set(&r, "width", w as f64);
    set(&r, "height", h as f64);
    set(&options, "rect", r);
    let size = frame
        .allocation_size(&options)
        .map_err(|e| js_err("VideoFrame.allocationSize", e))?;
    let buffer = Uint8Array::new_with_length(size as u32);
    let layout = JsFuture::from(frame.copy_to(&buffer, &options))
        .await
        .map_err(|e| js_err("VideoFrame.copyTo", e))?;
    let layout = Array::from(&layout);
    let plane = |i: u32| -> (usize, usize) {
        let p = layout.get(i);
        let n = |k: &str| get(&p, k).as_f64().unwrap_or(0.0) as usize;
        (n("offset"), n("stride"))
    };
    let data = buffer.to_vec();
    let (cw, ch) = (w.div_ceil(2), h.div_ceil(2));
    let mut out = Vec::with_capacity(w * h + 2 * cw * ch);
    let rows =
        |out: &mut Vec<u8>, (offset, stride): (usize, usize), width: usize, height: usize| {
            for row in 0..height {
                let at = offset + row * stride;
                out.extend_from_slice(data.get(at..at + width).ok_or("a short frame")?);
            }
            Ok::<(), String>(())
        };
    rows(&mut out, plane(0), w, h)?;
    if format == "NV12" {
        let (offset, stride) = plane(1);
        let mut v = Vec::with_capacity(cw * ch);
        for row in 0..ch {
            let at = offset + row * stride;
            let pairs = data.get(at..at + 2 * cw).ok_or("a short frame")?;
            for pair in pairs.as_chunks::<2>().0 {
                out.push(pair[0]);
                v.push(pair[1]);
            }
        }
        out.extend(v);
    } else {
        rows(&mut out, plane(1), cw, ch)?;
        rows(&mut out, plane(2), cw, ch)?;
    }
    Ok(Copied {
        width: w as u32,
        height: h as u32,
        planes: out,
        full_range,
    })
}

/// One pass of the decoder from a chunk on (straight through, or a seek).
struct Run {
    id: u32,
    start: usize,
    /// The next chunk to feed.
    next: usize,
    pictures: PictureOrder,
    order: OutputOrder,
    /// The frames to give out, in display order: `(chunk, frame number)`.
    sequence: VecDeque<(usize, u64)>,
    /// Pictures FFmpeg drops (they come out of the browser all the same).
    dropped: BTreeSet<usize>,
    /// Frames copied out, by chunk.
    copied: HashMap<usize, Result<Copied, String>>,
    /// Copies under way.
    copying: usize,
    /// Frames the browser gave in this run.
    outputs: usize,
    seek: ExactSeek<VideoFrame>,
    target: Option<f64>,
    retries: u32,
    /// Data read and not fed yet: `(index, bytes)`.
    data: VecDeque<(usize, Vec<u8>)>,
    reading: bool,
    flushing: bool,
    flushed: bool,
    /// The last frame time given out.
    last: Option<f64>,
    /// Chunks the browser may hold before giving a frame, as learned.
    allowance: usize,
    /// When the browser last gave a frame (or the run began), and when it
    /// was last fed one more ahead, ms.
    heard: f64,
    nudged: f64,
}

/// The decoder's output and error callbacks, kept alive with it.
type Callbacks = (Closure<dyn FnMut(JsFrame)>, Closure<dyn FnMut(JsValue)>);

enum Ready {
    Frame(VideoFrame),
    End(Option<f64>),
}

struct Inner {
    blob: Blob,
    media: Option<Media>,
    decoder: Option<JsVideoDecoder>,
    run: Option<Run>,
    runs: u32,
    /// Frames asked for and not polled.
    wanted: u32,
    ready: VecDeque<Ready>,
    events: Vec<VideoEvent>,
    failed: bool,
    /// A seek asked for while the movie was still opening (`Some(None)`:
    /// straight from the start).
    queued: Option<Option<f64>>,
    _closures: Option<Callbacks>,
}

/// One movie decoded by the browser; dropping it closes the decoder.
pub(crate) struct WebCodecsVideo(Rc<RefCell<Inner>>);

impl WebCodecsVideo {
    /// Opens `blob` (an H.264 AVI); `Opened`, or an `Error`, follows.
    pub(crate) fn open(blob: Blob) -> WebCodecsVideo {
        let inner = Rc::new(RefCell::new(Inner {
            blob: blob.clone(),
            media: None,
            decoder: None,
            run: None,
            runs: 0,
            wanted: 0,
            ready: VecDeque::new(),
            events: Vec::new(),
            failed: false,
            queued: Some(None),
            _closures: None,
        }));
        let weak = Rc::downgrade(&inner);
        spawn_local(async move {
            let loaded = load(&blob).await;
            let Some(inner) = weak.upgrade() else {
                return;
            };
            match loaded.and_then(|media| start_decoder(&inner, media)) {
                Ok(()) => pump(&inner),
                Err(e) => fail(&inner, e),
            }
        });
        WebCodecsVideo(inner)
    }
}

/// Makes the browser's decoder for `media` and reports the movie open. Its
/// callbacks run later, never inside a call into the decoder.
fn start_decoder(inner: &Rc<RefCell<Inner>>, media: Media) -> Result<(), String> {
    let weak = Rc::downgrade(inner);
    let on_frame = Closure::<dyn FnMut(JsFrame)>::new({
        let weak = weak.clone();
        move |frame: JsFrame| {
            let weak = weak.clone();
            spawn_local(async move {
                match weak.upgrade() {
                    Some(inner) => output(&inner, frame),
                    None => frame.close_frame(),
                }
            });
        }
    });
    let on_error = Closure::<dyn FnMut(JsValue)>::new({
        let weak = weak.clone();
        move |e: JsValue| {
            let weak = weak.clone();
            spawn_local(async move {
                if let Some(inner) = weak.upgrade() {
                    let message = get(&e, "message")
                        .as_string()
                        .unwrap_or_else(|| format!("{e:?}"));
                    fail(&inner, format!("the browser's decoder failed: {message}"));
                }
            });
        }
    });
    let init = Object::new();
    set(&init, "output", on_frame.as_ref().clone());
    set(&init, "error", on_error.as_ref().clone());
    let decoder = JsVideoDecoder::new(&init).map_err(|e| js_err("VideoDecoder", e))?;
    if let Err(e) = decoder.configure(&media.config) {
        let _ = decoder.close();
        return Err(js_err("VideoDecoder.configure", e));
    }
    let mut s = inner.borrow_mut();
    let last = media.chunks.last().map_or(0, |c| c.frame);
    s.events.push(VideoEvent::Opened(VideoInfo {
        width: media.stream.sps.width,
        height: media.stream.sps.height,
        duration: Some(f64::from(last + 1) * media.frame_time()),
        codec: if media.software {
            "h264 (software)".into()
        } else {
            "h264".into()
        },
    }));
    s.media = Some(media);
    s.decoder = Some(decoder);
    s._closures = Some((on_frame, on_error));
    let target = s.queued.take().flatten();
    begin(&mut s, target, None);
    Ok(())
}

/// Starts a run for a seek to `target` (or straight from the start); a
/// retry gives the chunk (`None` within: from the start, for good).
fn begin(s: &mut Inner, target: Option<f64>, retry: Option<(Option<usize>, u32)>) {
    let Some(media) = &s.media else {
        s.queued = Some(target);
        return;
    };
    let (start, retries, may_retry) = match retry {
        Some((Some(i), retries)) => (i, retries, i > 0),
        Some((None, _)) => (0, 0, false),
        None => {
            let start = target
                .and_then(|t| h264::seek_start(&media.chunks, t / media.frame_time(), media.delay));
            // No keyframe that early: the module decodes from the start
            // and never goes back.
            (start.unwrap_or(0), 0, start.is_some_and(|i| i > 0))
        }
    };
    if let Some(d) = &s.decoder
        && s.run.is_some()
    {
        let _ = d.reset();
        let _ = d.configure(&media.config);
    }
    s.runs += 1;
    let now = PerformanceClock::now_ms();
    s.run = Some(Run {
        id: s.runs,
        start,
        next: start,
        pictures: PictureOrder::new(&media.stream),
        order: OutputOrder::new(media.delay),
        sequence: VecDeque::new(),
        dropped: BTreeSet::new(),
        copied: HashMap::new(),
        copying: 0,
        outputs: 0,
        seek: ExactSeek::new(target, may_retry),
        target,
        retries,
        data: VecDeque::new(),
        reading: false,
        flushing: false,
        flushed: false,
        last: None,
        allowance: media.delay as usize + 2,
        heard: now,
        nudged: now,
    });
}

fn fail(inner: &Rc<RefCell<Inner>>, e: String) {
    let mut s = inner.borrow_mut();
    if s.failed {
        return;
    }
    s.failed = true;
    s.events.push(VideoEvent::Error(e));
    s.run = None;
    if let Some(d) = s.decoder.take() {
        let _ = d.close();
    }
}

/// A frame from the browser: known by its chunk, copied out, then given
/// out in display order.
fn output(inner: &Rc<RefCell<Inner>>, frame: JsFrame) {
    let (run_id, chunk, fake_rgb) = {
        let mut s = inner.borrow_mut();
        let fake = !s.media.as_ref().is_some_and(|m| m.software) && fake_rgb();
        let ts = frame.timestamp();
        let from = (ts / RUN_SPAN).floor();
        let chunk = (ts - from * RUN_SPAN).round() as usize;
        let Some(run) = s.run.as_mut().filter(|r| f64::from(r.id) == from) else {
            frame.close_frame();
            return;
        };
        run.outputs += 1;
        run.heard = PerformanceClock::now_ms();
        if run.dropped.contains(&chunk) {
            frame.close_frame();
            return;
        }
        run.copying += 1;
        (run.id, chunk, fake)
    };
    let weak = Rc::downgrade(inner);
    spawn_local(async move {
        let copied = if fake_rgb {
            Err(format!("{RGB_FRAMES} (BGRX, faked)"))
        } else {
            planes_of(&frame).await
        };
        frame.close_frame();
        let Some(inner) = weak.upgrade() else { return };
        {
            let mut s = inner.borrow_mut();
            let Some(run) = s.run.as_mut().filter(|r| r.id == run_id) else {
                return;
            };
            run.copying -= 1;
            run.copied.insert(chunk, copied);
        }
        give_out(&inner);
        pump(&inner);
    });
}

/// Gives out the frames copied so far, in display order, through the
/// exact seek.
fn give_out(inner: &Rc<RefCell<Inner>>) {
    let (error, software) = {
        let mut guard = inner.borrow_mut();
        let s = &mut *guard;
        let (Some(media), Some(run)) = (&s.media, s.run.as_mut()) else {
            return;
        };
        let sps = &media.stream.sps;
        let frame_time = media.frame_time();
        let mut error = None;
        let mut retry = false;
        let mut software = false;
        while let Some(&(chunk, stamp)) = run.sequence.front() {
            let Some(copied) = run.copied.remove(&chunk) else {
                // Still coming, or one the browser dropped: skipped once
                // enough later frames came, or all of them.
                let later = run
                    .sequence
                    .iter()
                    .skip(1)
                    .filter(|(c, _)| run.copied.contains_key(c))
                    .count();
                if later >= SKIP_AFTER || (run.flushed && run.copying == 0) {
                    run.sequence.pop_front();
                    continue;
                }
                break;
            };
            run.sequence.pop_front();
            let copied = match copied {
                Ok(c) => c,
                // RGB from the first frame on: the software decoder, from
                // the start of this run.
                Err(e)
                    if e.starts_with(RGB_FRAMES)
                        && run.last.is_none()
                        && rgb_frames(media.software) =>
                {
                    software = true;
                    break;
                }
                Err(e) => {
                    error = Some(e);
                    break;
                }
            };
            let t = stamp as f64 * frame_time;
            let frame = VideoFrame {
                pts: t,
                width: copied.width,
                height: copied.height,
                matrix: if sps.bt709() {
                    YuvMatrix::Bt709
                } else {
                    YuvMatrix::Bt601
                },
                full_range: copied.full_range.unwrap_or(sps.full_range),
                planes: copied.planes,
                decode_ms: 0.0,
            };
            run.last = Some(t);
            match run.seek.offer(frame, t) {
                Offer::Show(frames) => s
                    .ready
                    .extend(frames.into_iter().map(|(f, _)| Ready::Frame(f))),
                Offer::Retry => {
                    retry = true;
                    break;
                }
            }
        }
        if retry {
            // The first frame came after the target: from further back.
            let landed = media.chunks[run.start].frame;
            let back = h264::seek_back(&media.chunks, landed, run.retries, 1.0 / frame_time);
            let (target, retries) = (run.target, run.retries + 1);
            begin(s, target, Some((back, retries)));
        } else if run.flushed && run.copying == 0 && run.sequence.is_empty() {
            // The end: the frame held for a seek into the last one, then
            // the end, once.
            run.flushed = false;
            if let Some((f, _)) = run.seek.end(frame_time) {
                s.ready.push_back(Ready::Frame(f));
            }
            s.ready.push_back(Ready::End(run.last));
        }
        (error, software)
    };
    if let Some(e) = error {
        fail(inner, e);
    } else if software {
        to_software(inner);
    }
}

/// Reconfigures the decoder for the browser's software decoder and starts
/// the run again (no frame of it was given out).
fn to_software(inner: &Rc<RefCell<Inner>>) {
    let config = {
        let s = inner.borrow();
        let Some(media) = &s.media else { return };
        let config = Object::new();
        let _ = Object::assign(&config, &media.config);
        set(&config, "hardwareAcceleration", "prefer-software");
        config
    };
    let weak = Rc::downgrade(inner);
    spawn_local(async move {
        let support = JsFuture::from(JsVideoDecoder::is_config_supported(&config)).await;
        let Some(inner) = weak.upgrade() else { return };
        if !support.is_ok_and(|s| get(&s, "supported").is_truthy()) {
            rgb_frames(true);
            return fail(&inner, "the browser gives only RGB frames".into());
        }
        {
            let mut guard = inner.borrow_mut();
            let s = &mut *guard;
            let Some(media) = s.media.as_mut() else {
                return;
            };
            media.config = config;
            media.software = true;
            s.events.push(VideoEvent::Opened(VideoInfo {
                width: media.stream.sps.width,
                height: media.stream.sps.height,
                duration: Some(
                    f64::from(media.chunks.last().map_or(0, |c| c.frame) + 1) * media.frame_time(),
                ),
                codec: "h264 (software)".into(),
            }));
            let target = s.run.as_ref().and_then(|r| r.target);
            begin(s, target, None);
        }
        pump(&inner);
    });
}

/// Keeps the decoder fed while frames are wanted: reads chunks ahead,
/// decodes them in order, and flushes after the last.
fn pump(inner: &Rc<RefCell<Inner>>) {
    let mut read_from = None;
    let mut flush = None;
    let mut error = None;
    {
        let mut guard = inner.borrow_mut();
        let s = &mut *guard;
        if s.failed {
            return;
        }
        let (Some(media), Some(decoder), Some(run)) = (&s.media, &s.decoder, s.run.as_mut()) else {
            return;
        };
        let frames_ready = s
            .ready
            .iter()
            .filter(|r| matches!(r, Ready::Frame(_)))
            .count();
        let in_decoder = (run.next - run.start).saturating_sub(run.outputs);
        let coming = frames_ready + run.copied.len() + run.copying;
        let now = PerformanceClock::now_ms();
        let starved = (frames_ready as u32) < s.wanted && !run.flushing;
        if starved && in_decoder > 0 && now - run.heard > STALL_MS {
            error = Some("the browser's decoder stopped answering".to_string());
        } else {
            // A decoder that holds more frames than thought is fed one
            // more ahead each time it goes quiet.
            if starved && in_decoder >= run.allowance && now - run.heard.max(run.nudged) > NUDGE_MS
            {
                run.allowance = (run.allowance + 1).min(IN_FLIGHT);
                run.nudged = now;
            }
            let mut fed = in_decoder;
            while coming + fed < s.wanted as usize + run.allowance
                && fed < IN_FLIGHT
                && run.next < media.chunks.len()
                && decoder.decode_queue_size() < 4
            {
                let Some((i, _)) = run.data.front() else {
                    if !run.reading {
                        run.reading = true;
                        read_from = Some((run.id, run.next));
                    }
                    break;
                };
                if *i != run.next {
                    run.data.clear();
                    continue;
                }
                let (i, data) = run.data.pop_front().expect("checked");
                let Some(picture) =
                    run.pictures
                        .picture(&data, media.stream.framing, &media.stream.sps)
                else {
                    error = Some("a chunk without a picture the browser path reads".into());
                    break;
                };
                if i == run.start && !picture.idr {
                    error = Some("a keyframe of the index is not an IDR picture".into());
                    break;
                }
                let released = run.order.push(i, picture, media.chunks[i].frame);
                run.sequence.extend(released.out);
                run.dropped.extend(released.dropped);
                match media.encoded(i, run.id, &data, picture.idr).and_then(|c| {
                    decoder
                        .decode(&c)
                        .map_err(|e| js_err("VideoDecoder.decode", e))
                }) {
                    Ok(()) => {
                        run.next += 1;
                        fed += 1;
                    }
                    Err(e) => {
                        error = Some(e);
                        break;
                    }
                }
            }
            if error.is_none() && run.next == media.chunks.len() && !run.flushing {
                run.flushing = true;
                run.sequence.extend(run.order.drain());
                flush = Some((run.id, decoder.flush()));
            }
        }
    }
    if let Some(e) = error {
        fail(inner, e);
        return;
    }
    if let Some((run_id, from)) = read_from {
        read_ahead(inner, run_id, from);
    }
    if let Some((run_id, promise)) = flush {
        let weak = Rc::downgrade(inner);
        spawn_local(async move {
            let done = JsFuture::from(promise).await;
            let Some(inner) = weak.upgrade() else { return };
            {
                let mut s = inner.borrow_mut();
                match s.run.as_mut() {
                    Some(run) if run.id == run_id => run.flushed = done.is_ok(),
                    // A reset aborts the flush; that run is over anyway.
                    _ => return,
                }
            }
            if let Err(e) = done {
                fail(&inner, js_err("VideoDecoder.flush", e));
                return;
            }
            give_out(&inner);
        });
    }
}

/// Reads the chunks from `from` on (one span of the file) for run `run_id`.
fn read_ahead(inner: &Rc<RefCell<Inner>>, run_id: u32, from: usize) {
    let (blob, spans) = {
        let s = inner.borrow();
        let Some(media) = &s.media else { return };
        let first = media.chunks[from];
        let mut spans = vec![(from, first)];
        for (i, c) in media
            .chunks
            .iter()
            .enumerate()
            .skip(from + 1)
            .take(READ_CHUNKS - 1)
        {
            if c.offset < first.offset || c.offset + u64::from(c.size) > first.offset + READ_BYTES {
                break;
            }
            spans.push((i, *c));
        }
        (s.blob.clone(), spans)
    };
    let weak = Rc::downgrade(inner);
    spawn_local(async move {
        let start = spans[0].1.offset;
        let end = spans
            .iter()
            .map(|(_, c)| c.offset + u64::from(c.size))
            .max()
            .unwrap_or(start);
        let bytes = read(&blob, start, end).await;
        let Some(inner) = weak.upgrade() else { return };
        let bytes = match bytes {
            Ok(b) => b,
            Err(e) => return fail(&inner, e),
        };
        let short = {
            let mut s = inner.borrow_mut();
            let Some(run) = s.run.as_mut().filter(|r| r.id == run_id) else {
                return;
            };
            run.reading = false;
            let before = run.data.len();
            for (i, c) in spans {
                let at = (c.offset - start) as usize;
                match bytes.get(at..at + c.size as usize) {
                    Some(data) => run.data.push_back((i, data.to_vec())),
                    None => break,
                }
            }
            run.data.len() == before
        };
        if short {
            // The file ends inside a chunk the index promised.
            return fail(&inner, "the movie ends before its index does".into());
        }
        pump(&inner);
    });
}

impl VideoDecoder for WebCodecsVideo {
    fn seek(&mut self, t: f64) {
        {
            let mut s = self.0.borrow_mut();
            s.wanted = 0;
            s.ready.clear();
            s.events
                .retain(|e| !matches!(e, VideoEvent::Frame(_) | VideoEvent::End { .. }));
            if s.failed {
                return;
            }
            if s.media.is_some() {
                begin(&mut s, Some(t), None);
            } else {
                s.queued = Some(Some(t));
            }
        }
        pump(&self.0);
    }

    fn want(&mut self, n: u32) {
        self.0.borrow_mut().wanted += n;
        pump(&self.0);
    }

    fn pending(&self) -> u32 {
        self.0.borrow().wanted
    }

    fn poll(&mut self, out: &mut Vec<VideoEvent>) {
        {
            let mut guard = self.0.borrow_mut();
            let s = &mut *guard;
            out.append(&mut s.events);
            while let Some(front) = s.ready.front() {
                match front {
                    Ready::Frame(_) if s.wanted == 0 => break,
                    Ready::Frame(_) => {
                        s.wanted -= 1;
                        if let Some(Ready::Frame(f)) = s.ready.pop_front() {
                            out.push(VideoEvent::Frame(f));
                        }
                    }
                    Ready::End(last) => {
                        out.push(VideoEvent::End { last_pts: *last });
                        s.wanted = 0;
                        s.ready.pop_front();
                    }
                }
            }
            if s.failed {
                s.wanted = 0;
            }
        }
        pump(&self.0);
    }
}

impl Drop for WebCodecsVideo {
    fn drop(&mut self) {
        let mut s = self.0.borrow_mut();
        s.run = None;
        if let Some(d) = s.decoder.take() {
            let _ = d.close();
        }
    }
}

/// The black-bar probe through the browser: the frame of the IDR keyframe
/// at or before each of a few times spread through the movie (the module
/// samples the same way).
pub(crate) async fn probe(blob: &Blob) -> Result<Picture, String> {
    let software = MODE.with(Cell::get) == Mode::Software;
    match probe_once(blob).await {
        // RGB frames: once more through the software decoder.
        Err(e) if e.starts_with(RGB_FRAMES) && rgb_frames(software) => probe_once(blob).await,
        Err(e) if e.starts_with(RGB_FRAMES) => Err(e),
        r => r,
    }
}

async fn probe_once(blob: &Blob) -> Result<Picture, String> {
    let media = load(blob).await?;
    let frames: Rc<RefCell<Vec<JsFrame>>> = Rc::default();
    let on_frame = Closure::<dyn FnMut(JsFrame)>::new({
        let frames = Rc::clone(&frames);
        move |f: JsFrame| frames.borrow_mut().push(f)
    });
    let on_error = Closure::<dyn FnMut(JsValue)>::new(|_| {});
    let init = Object::new();
    set(&init, "output", on_frame.as_ref().clone());
    set(&init, "error", on_error.as_ref().clone());
    let decoder = JsVideoDecoder::new(&init).map_err(|e| js_err("VideoDecoder", e))?;
    let last = media.chunks.last().map_or(0, |c| c.frame);
    let mut planes = Vec::new();
    let (mut width, mut height, mut full) = (0, 0, media.stream.sps.full_range);
    let mut result = Ok(());
    for share in [0.15, 0.3, 0.45, 0.6, 0.75, 0.9] {
        let target = share * f64::from(last);
        let Some(i) = media
            .chunks
            .iter()
            .rposition(|c| c.key && f64::from(c.frame) <= target)
        else {
            continue;
        };
        let c = media.chunks[i];
        let data = match read(blob, c.offset, c.offset + u64::from(c.size)).await {
            Ok(d) => d,
            Err(e) => {
                result = Err(e);
                break;
            }
        };
        if !h264::is_idr(&data, media.stream.framing) {
            continue;
        }
        let _ = decoder.reset();
        let decoded = decoder.configure(&media.config).and_then(|_| {
            media
                .encoded(i, 0, &data, true)
                .map_err(JsValue::from)
                .and_then(|c| decoder.decode(&c))
        });
        if let Err(e) = decoded {
            result = Err(js_err("decoding a probe frame", e));
            break;
        }
        if let Err(e) = JsFuture::from(decoder.flush()).await {
            result = Err(js_err("VideoDecoder.flush", e));
            break;
        }
        let taken: Vec<JsFrame> = frames.borrow_mut().drain(..).collect();
        for f in taken {
            let copied = planes_of(&f).await;
            f.close_frame();
            match copied {
                Ok(c) if planes.is_empty() || (c.width, c.height) == (width, height) => {
                    width = c.width;
                    height = c.height;
                    full = c.full_range.unwrap_or(full);
                    planes.push(c.planes[..(c.width * c.height) as usize].to_vec());
                }
                Ok(_) => {}
                Err(e) => result = Err(e),
            }
        }
        if result.is_err() {
            break;
        }
    }
    for f in frames.borrow_mut().drain(..) {
        f.close_frame();
    }
    let _ = decoder.close();
    result?;
    if planes.is_empty() {
        return Err("no keyframe decoded".into());
    }
    let samples: Vec<&[u8]> = planes.iter().map(Vec::as_slice).collect();
    Ok(Picture {
        width,
        height,
        bounds: ddi_platform::video::picture_bounds(&samples, width, height, full),
    })
}
