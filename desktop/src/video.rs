//! Background movies on desktop (feature `video`): on macOS, checked
//! progressive 8-bit H.264 AVIs through hardware VideoToolbox; every other
//! movie through the sandboxed FFmpeg module under wasmtime. The OS path
//! reuses `ddi_library::avi` and `h264` as WebCodecs does, with bounded
//! reads, callbacks and reorder queues, copied I420 planes, exact seeks,
//! and per-generation output. A separate worker owns each native session;
//! a deadline abandons a stuck one and disables native opens for the process.
//! Refusal, failure or timeout resumes that movie in the module at the
//! requested position, omitting frames already delivered. `Backend` can
//! force either path; `DesktopVideo::decoder_status` reports the selection.
//!
//! The module (`target/video/ddivideo.wasm`, `video/build.sh`) runs on
//! the shared decoder thread. It mirrors the web worker call for call:
//!
//! - the module is a WASI reactor; its 20 imported calls are implemented
//!   here over the movie files, as the worker implements them over
//!   `Blob`s (a 64 KiB read-ahead, the files named `/v/<id>.<ext>`, the
//!   rest refused), so it reads nothing else;
//! - messages are handled one at a time, in order: open, seek, want,
//!   close, probe, and recycle (a frame's buffer given back for the next
//!   frame, so a playing movie allocates nothing per frame);
//! - frames are decoded only on `want`, so the caller (the play's
//!   [`MovieTrack`](ddi_platform::video::MovieTrack)) sets the queue's
//!   depth; a seek cancels the wants from before it (their generation),
//!   and so does the end of the movie, reported once until the next
//!   seek; 50 broken frames in a row fail the movie;
//! - a trap (a broken file reaching a bug, the 1 GiB memory cap) fails
//!   every open movie and the next open starts a fresh instance.
//!
//! The module is compiled once (about a second with Cranelift) and kept
//! in the cache folder, serialised, keyed by the module and wasmtime's
//! version; memory is capped at 1 GiB, as the module's own build caps it
//! for the web. Where the module is: `DDI_VIDEO_MODULE`, beside the
//! executable (`video/ddivideo.wasm`), then the repository's
//! `target/video/` (a dev build). Without it, opening a movie fails with
//! the web's message: "the video decoder is not included in this build".
//!
//! [`VideoHost`] implements `ddi_platform::video::VideoBackend` (its
//! source a path), [`DesktopVideo`] `VideoDecoder`; [`show`] hands a
//! frame to the renderer as `Renderer::set_video_frame` takes it.

use std::collections::HashMap;
use std::io::{Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, Ordering};
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::{Arc, Mutex, Weak};
use std::time::{Duration, Instant};

use ddi_platform::video::{
    VideoBackend, VideoDecoder, VideoEvent, VideoFormat, VideoFrame, VideoInfo, YuvMatrix,
};
use wasmtime::{
    Caller, Config, Engine, Instance, Linker, Memory, Module, OptLevel, Store, StoreLimits,
    StoreLimitsBuilder, Trap, TypedFunc, WasmParams, WasmResults,
};

#[cfg(target_os = "macos")]
mod avi;
#[cfg(target_os = "macos")]
mod native;
#[cfg(target_os = "macos")]
mod vt;

pub mod packets;

/// Selects a decoder for tests and diagnostics. Auto tries the checked OS
/// subset, falling back to the module; VideoToolbox reports refusals.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Backend {
    #[default]
    Auto,
    Module,
    VideoToolbox,
}
impl Backend {
    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "auto" => Some(Self::Auto),
            "module" => Some(Self::Module),
            "videotoolbox" => Some(Self::VideoToolbox),
            _ => None,
        }
    }
}

/// What actually decodes one movie, and why the OS path was left.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DecoderStatus {
    pub backend: String,
    pub fallback: Option<String>,
}

/// What a missing module says, as on the web.
pub const NOT_INCLUDED: &str = "the video decoder is not included in this build";
/// The module's memory, at most (its build caps it so for the web).
pub const MAX_MEMORY: usize = 1 << 30;
/// Broken frames in a row before the movie counts as broken.
const MAX_BAD_FRAMES: u32 = 50;
/// Frame buffers kept for the next frames (`web/src/web/video.rs`).
const SPARE_KEPT: usize = 8;
/// How far ahead a read fills its window.
const READ_AHEAD: usize = 64 * 1024;
/// Decoder messages shown, at most.
const MAX_LOG_LINES: u32 = 200;
/// How often the engine's epoch moves on (the calls' deadlines are
/// counted in these).
const TICK: Duration = Duration::from_millis(50);
/// The longest a call into the module may run (a frame takes
/// milliseconds; opening a movie reads its headers): past it the call
/// traps and the decoder counts as crashed, so a module spinning on a
/// broken file cannot hold the thread.
pub const CALL_DEADLINE: Duration = Duration::from_secs(10);
/// Files the module may hold open at once (a movie and its probe each
/// hold one or two).
const MAX_OPEN_FILES: usize = 32;
/// The longest decoder message kept, bytes (longer ones are cut).
const MAX_LOG_LINE: usize = 4096;
/// Where the black-bar probe samples a movie: shares of its duration, or
/// seconds when it is unknown.
const PROBE_SHARES: [f64; 6] = [0.15, 0.3, 0.45, 0.6, 0.75, 0.9];
const PROBE_SECONDS: [f64; 6] = [1.0, 3.0, 6.0, 10.0, 15.0, 20.0];

/// Where the module is.
#[derive(Clone, Debug, PartialEq)]
pub enum ModuleStatus {
    /// Not asked for yet.
    Idle,
    /// Being compiled (or read from the cache).
    Loading,
    Ready {
        load_ms: f64,
        /// Read back from the cache, not compiled.
        cached: bool,
    },
    Failed(String),
}

/// The module's file: `DDI_VIDEO_MODULE`, beside the executable
/// (`video/ddivideo.wasm`, as a package has it), then, for an executable
/// built in a workspace (any profile: `target/debug`, `target/desktop`,
/// a test's `target/debug/deps`), the workspace's `target/video/`,
/// found by walking up from the executable to its `target` folder (no
/// path from the build is kept in the binary).
pub fn module_path() -> Option<PathBuf> {
    if let Some(p) = std::env::var_os("DDI_VIDEO_MODULE") {
        return Some(PathBuf::from(p)).filter(|p| p.is_file());
    }
    let exe = std::env::current_exe().ok()?;
    module_near(&exe)
}

/// [`module_path`] for the executable at `exe`.
fn module_near(exe: &Path) -> Option<PathBuf> {
    let beside = exe.parent().map(|d| d.join("video").join("ddivideo.wasm"));
    let built = exe
        .ancestors()
        .skip(1)
        .find(|a| a.file_name().is_some_and(|n| n == "target"))
        .map(|t| t.join("video").join("ddivideo.wasm"));
    beside.into_iter().chain(built).find(|p| p.is_file())
}

/// Where compiled modules are kept: the user's cache folder.
fn cache_dir() -> Option<PathBuf> {
    std::env::var_os("DDI_VIDEO_CACHE")
        .map(PathBuf::from)
        .or_else(|| crate::paths::cache_dir().map(|d| d.join("video")))
}

/// A movie's picture as the black-bar probe found it.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Picture {
    pub width: u32,
    pub height: u32,
    /// The area inside the movie's own black bars, `(x, y, width,
    /// height)`; `None` when every sample was black.
    pub bounds: Option<(u32, u32, u32, u32)>,
}

/// Hands a frame to the renderer's movie `id` (from
/// `Renderer::add_video`): the planes as they are, the matrix and range
/// the module reported. Whether it was written.
pub fn show(
    renderer: &mut ddi_render::Renderer,
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    id: usize,
    frame: &VideoFrame,
) -> bool {
    renderer.set_video_frame(
        device,
        queue,
        id,
        frame.width,
        frame.height,
        &frame.planes,
        frame.matrix == YuvMatrix::Bt709,
        frame.full_range,
    )
}

// --- The host: the game's side -------------------------------------------

/// A movie's bytes: a file, or a range of one (a zip's stored entry, read
/// in place).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MovieFile {
    pub path: PathBuf,
    /// `(offset, length)` in the file; `None`: all of it.
    pub range: Option<(u64, u64)>,
}

impl MovieFile {
    /// Its length in bytes (0 when it cannot be read).
    fn size(&self) -> u64 {
        let len = std::fs::metadata(&self.path).map_or(0, |m| m.len());
        match self.range {
            Some((at, n)) => n.min(len.saturating_sub(at)),
            None => len,
        }
    }
}

impl From<PathBuf> for MovieFile {
    fn from(path: PathBuf) -> MovieFile {
        MovieFile { path, range: None }
    }
}

enum Msg {
    Load,
    Open {
        id: u32,
        path: MovieFile,
        name: String,
    },
    Seek {
        id: u32,
        t: f64,
        epoch: u32,
    },
    Want {
        id: u32,
        n: u32,
        epoch: u32,
    },
    Close {
        id: u32,
    },
    Probe {
        id: u32,
        path: MovieFile,
        name: String,
    },
    Recycle(Vec<u8>),
}

impl Msg {
    /// The movie (or probe) it is about.
    fn id(&self) -> Option<u32> {
        match self {
            Msg::Open { id, .. }
            | Msg::Seek { id, .. }
            | Msg::Want { id, .. }
            | Msg::Close { id }
            | Msg::Probe { id, .. } => Some(*id),
            Msg::Load | Msg::Recycle(_) => None,
        }
    }
}

/// A frame as the thread hands it over (`VideoFrame` without the
/// platform's own kind, which may not cross threads).
struct Frame {
    pts: f64,
    width: u32,
    height: u32,
    matrix: YuvMatrix,
    full_range: bool,
    planes: Vec<u8>,
    decode_ms: f64,
}

/// What the thread tells a movie (`VideoEvent`, sendable).
enum Event {
    Opened(VideoInfo),
    Frame(Frame),
    End { last_pts: Option<f64> },
    Error(String),
}

impl Event {
    fn into_video_event(self) -> VideoEvent {
        match self {
            Event::Opened(i) => VideoEvent::Opened(i),
            Event::Frame(f) => VideoEvent::Frame(VideoFrame {
                pts: f.pts,
                width: f.width,
                height: f.height,
                matrix: f.matrix,
                full_range: f.full_range,
                planes: f.planes,
                decode_ms: f.decode_ms,
                external: None,
            }),
            Event::End { last_pts } => VideoEvent::End { last_pts },
            Event::Error(e) => VideoEvent::Error(e),
        }
    }
}

struct Movie {
    decoder: DecoderStatus,
    /// Seeks so far; frames tagged with an older one are dropped.
    generation: u32,
    events: Vec<Event>,
}

struct Shared {
    status: ModuleStatus,
    movies: HashMap<u32, Movie>,
    probes: HashMap<u32, Option<Result<Picture, String>>>,
}

impl Shared {
    /// An event from the decoder thread for movie `id`, of generation
    /// `epoch` (frames and ends of an older one are dropped, their buffers
    /// given back).
    fn post(&mut self, id: u32, epoch: Option<u32>, event: Event) -> Option<Vec<u8>> {
        let Some(movie) = self.movies.get_mut(&id) else {
            return match event {
                Event::Frame(f) => Some(f.planes),
                _ => None,
            };
        };
        if epoch.is_some_and(|g| g != movie.generation) {
            return match event {
                Event::Frame(f) => Some(f.planes),
                _ => None,
            };
        }
        movie.events.push(event);
        None
    }
}

struct Inner {
    tx: Mutex<Sender<Msg>>,
    shared: Arc<Mutex<Shared>>,
    next_id: AtomicU32,
}

/// The decoder thread for background movies; clones share it. It stops
/// when the last clone and movie are dropped.
#[derive(Clone)]
pub struct VideoHost(Arc<Inner>);

static SHARED_HOST: Mutex<Weak<Inner>> = Mutex::new(Weak::new());

impl VideoHost {
    /// A new decoder thread, using the module at `module` (`None`: none,
    /// so every movie fails as on a web build without it).
    pub fn with_module(module: Option<PathBuf>) -> VideoHost {
        VideoHost::with_deadline(module, CALL_DEADLINE)
    }

    /// [`VideoHost::with_module`], calls into the module limited to
    /// `deadline` each.
    pub fn with_deadline(module: Option<PathBuf>, deadline: Duration) -> VideoHost {
        Self::with_backend_deadline(module, Backend::Module, deadline)
    }

    pub fn with_backend(module: Option<PathBuf>, backend: Backend) -> VideoHost {
        Self::with_backend_deadline(module, backend, CALL_DEADLINE)
    }

    fn with_backend_deadline(
        module: Option<PathBuf>,
        backend: Backend,
        deadline: Duration,
    ) -> VideoHost {
        let (tx, rx) = mpsc::channel();
        let shared = Arc::new(Mutex::new(Shared {
            status: ModuleStatus::Idle,
            movies: HashMap::new(),
            probes: HashMap::new(),
        }));
        let thread_shared = Arc::clone(&shared);
        std::thread::Builder::new()
            .name("ddi-video".into())
            .spawn(move || run(rx, thread_shared, module, deadline, backend))
            .expect("video thread");
        VideoHost(Arc::new(Inner {
            tx: Mutex::new(tx),
            shared,
            next_id: AtomicU32::new(1),
        }))
    }

    /// The process's decoder thread: the one already running (so the
    /// module is compiled once), else a new one on [`module_path`]; one
    /// whose module failed is not shared.
    pub fn shared() -> VideoHost {
        let mut slot = SHARED_HOST.lock().unwrap_or_else(|e| e.into_inner());
        if let Some(inner) = slot.upgrade() {
            let host = VideoHost(inner);
            if !matches!(host.status(), ModuleStatus::Failed(_)) {
                return host;
            }
        }
        let backend = std::env::var("DDI_VIDEO_DECODER")
            .ok()
            .and_then(|s| Backend::parse(&s))
            .unwrap_or_default();
        let host = VideoHost::with_backend(module_path(), backend);
        *slot = Arc::downgrade(&host.0);
        host
    }

    fn send(&self, msg: Msg) {
        let tx = self.0.tx.lock().unwrap_or_else(|e| e.into_inner());
        let _ = tx.send(msg);
    }

    fn next_id(&self) -> u32 {
        self.0.next_id.fetch_add(1, Ordering::Relaxed)
    }

    pub fn status(&self) -> ModuleStatus {
        self.0
            .shared
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .status
            .clone()
    }

    /// Compiles (or reads back) the module now, while a start prompt is
    /// up, so a movie does not wait for it.
    pub fn preload(&self) {
        self.send(Msg::Load);
    }

    /// Gives a frame's planes back once drawn (or passed over): the next
    /// frame is copied into them.
    pub fn recycle(&self, planes: Vec<u8>) {
        if planes.capacity() > 0 {
            self.send(Msg::Recycle(planes));
        }
    }

    /// Samples a movie's picture for its black bars; the answer comes with
    /// [`VideoHost::probe_result`].
    pub fn probe(&self, path: &MovieFile, name: &str) -> u32 {
        let id = self.next_id();
        self.0
            .shared
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .probes
            .insert(id, None);
        self.send(Msg::Probe {
            id,
            path: path.clone(),
            name: name.to_string(),
        });
        id
    }

    /// The answer to probe `id`, once it came (then forgotten).
    pub fn probe_result(&self, id: u32) -> Option<Result<Picture, String>> {
        let mut shared = self.0.shared.lock().unwrap_or_else(|e| e.into_inner());
        let done = shared.probes.get(&id)?.is_some();
        done.then(|| shared.probes.remove(&id).flatten()).flatten()
    }
}

impl VideoBackend for VideoHost {
    type Source = MovieFile;
    type Decoder = DesktopVideo;

    fn open(&mut self, source: MovieFile, name: &str, _format: &VideoFormat) -> DesktopVideo {
        let id = self.next_id();
        self.0
            .shared
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .movies
            .insert(
                id,
                Movie {
                    decoder: DecoderStatus {
                        backend: "module".into(),
                        fallback: None,
                    },
                    generation: 0,
                    events: Vec::new(),
                },
            );
        self.send(Msg::Open {
            id,
            path: source,
            name: name.to_string(),
        });
        DesktopVideo {
            id,
            generation: 0,
            pending: 0,
            host: self.clone(),
        }
    }
}

/// One movie open on the decoder thread; dropping it closes the movie.
pub struct DesktopVideo {
    id: u32,
    generation: u32,
    /// Frames asked for in this generation and not polled yet.
    pending: u32,
    host: VideoHost,
}

impl DesktopVideo {
    pub fn decoder_status(&self) -> Option<DecoderStatus> {
        self.host
            .0
            .shared
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .movies
            .get(&self.id)
            .map(|m| m.decoder.clone())
    }
}

impl VideoDecoder for DesktopVideo {
    fn seek(&mut self, t: f64) {
        self.generation += 1;
        self.pending = 0;
        let freed = {
            let mut shared = self.host.0.shared.lock().unwrap_or_else(|e| e.into_inner());
            let mut freed = Vec::new();
            if let Some(movie) = shared.movies.get_mut(&self.id) {
                movie.generation = self.generation;
                // Frames that arrived before the seek but were not polled.
                movie.events.retain_mut(|e| match e {
                    Event::Frame(f) => {
                        freed.push(std::mem::take(&mut f.planes));
                        false
                    }
                    Event::End { .. } => false,
                    _ => true,
                });
            }
            freed
        };
        for planes in freed {
            self.host.recycle(planes);
        }
        self.host.send(Msg::Seek {
            id: self.id,
            t,
            epoch: self.generation,
        });
    }

    fn want(&mut self, n: u32) {
        self.pending += n;
        self.host.send(Msg::Want {
            id: self.id,
            n,
            epoch: self.generation,
        });
    }

    fn pending(&self) -> u32 {
        self.pending
    }

    fn poll(&mut self, out: &mut Vec<VideoEvent>) {
        let mut shared = self.host.0.shared.lock().unwrap_or_else(|e| e.into_inner());
        let Some(movie) = shared.movies.get_mut(&self.id) else {
            return;
        };
        for event in movie.events.drain(..) {
            match event {
                Event::Frame(_) => self.pending = self.pending.saturating_sub(1),
                Event::End { .. } | Event::Error(_) => self.pending = 0,
                Event::Opened(_) => {}
            }
            out.push(event.into_video_event());
        }
    }
}

impl Drop for DesktopVideo {
    fn drop(&mut self) {
        let movie = self
            .host
            .0
            .shared
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .movies
            .remove(&self.id);
        if let Some(movie) = movie {
            for e in movie.events {
                if let Event::Frame(f) = e {
                    self.host.recycle(f.planes);
                }
            }
        }
        self.host.send(Msg::Close { id: self.id });
    }
}

// --- A check of a movie (`--video-check`) -----------------------------------

/// What [`check`] found.
#[derive(Clone, Debug, Default)]
pub struct CheckReport {
    pub info: Option<VideoInfo>,
    /// Each frame decoded straight through: its time and its planes'
    /// CRC-32 (zlib's, as `video/check.mjs` takes it).
    pub frames: Vec<(f64, u32)>,
    pub ms_mean: f64,
    pub ms_p99: f64,
    pub ms_max: f64,
    /// The slowest frame's index.
    pub slowest: usize,
    /// Opening (from the request to `Opened`), and the module's load.
    pub open_ms: f64,
    pub status: Option<ModuleStatus>,
    pub decoder: Option<DecoderStatus>,
    /// Worst PSNR of Y, U, V against a raw YUV 4:2:0 reference (dB;
    /// infinite when equal), and the reference's frames.
    pub psnr: Option<([f64; 3], usize)>,
    /// Seeks made, and the slowest (ms).
    pub seeks: usize,
    pub seek_ms: f64,
    /// What did not hold.
    pub failures: Vec<String>,
}

/// Decodes `path` as a play would (through [`VideoHost`] and
/// [`DesktopVideo`], a few frames asked for at a time), then seeks to
/// each frame's time (about 60 of them), to the middle of each period
/// and of each gap of dropped frames, to the end and past it, and loops
/// to the start: every seek must land on the frame decoding straight
/// through gave, byte for byte (`video/check.mjs`'s checks). With
/// `reference`, every frame is compared with a raw YUV 4:2:0 file.
pub fn check(host: &mut VideoHost, path: &Path, reference: Option<&Path>) -> CheckReport {
    use std::time::Duration;
    let mut r = CheckReport::default();
    let name = path
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default();
    let start = Instant::now();
    let format = VideoFormat {
        container: ddi_platform::video::VideoContainer::Unknown,
        codec: ddi_platform::video::VideoCodec::Unknown,
    };
    let mut v = host.open(path.to_path_buf().into(), &name, &format);
    let deadline = Duration::from_secs(60);
    let mut events = Vec::new();
    // The next event, waiting for it.
    let next = |v: &mut DesktopVideo, events: &mut Vec<VideoEvent>| -> Option<VideoEvent> {
        let t = Instant::now();
        loop {
            if !events.is_empty() {
                return Some(events.remove(0));
            }
            v.poll(events);
            if events.is_empty() {
                if t.elapsed() > deadline {
                    return None;
                }
                std::thread::sleep(Duration::from_micros(200));
            }
        }
    };
    match next(&mut v, &mut events) {
        Some(VideoEvent::Opened(info)) => r.info = Some(info),
        Some(VideoEvent::Error(e)) => {
            r.failures.push(e);
            r.status = Some(host.status());
            r.decoder = v.decoder_status();
            return r;
        }
        other => {
            r.failures.push(format!("no answer to the open: {other:?}"));
            return r;
        }
    }
    r.decoder = v.decoder_status();
    r.open_ms = start.elapsed().as_secs_f64() * 1e3;
    r.status = Some(host.status());
    let mut reference = reference.and_then(|p| std::fs::File::open(p).ok());
    let mut psnr = [f64::INFINITY; 3];
    let mut ref_frames = 0;
    let mut ms = Vec::new();
    // Straight through, eight frames asked for at a time.
    'decode: loop {
        v.want(8);
        for _ in 0..8 {
            match next(&mut v, &mut events) {
                Some(VideoEvent::Frame(f)) => {
                    if let Some((last, _)) = r.frames.last()
                        && f.pts <= *last
                    {
                        r.failures.push(format!(
                            "frame {} at {:.4} s is not after {:.4} s",
                            r.frames.len(),
                            f.pts,
                            last
                        ));
                    }
                    if let Some(file) = reference.as_mut() {
                        let mut expected = vec![0u8; f.planes.len()];
                        let at = (r.frames.len() * f.planes.len()) as u64;
                        if file.seek(SeekFrom::Start(at)).is_ok()
                            && file.read_exact(&mut expected).is_ok()
                        {
                            ref_frames += 1;
                            let y = (f.width * f.height) as usize;
                            let c = (f.planes.len() - y) / 2;
                            for (k, (a, b)) in [(0, y), (y, y + c), (y + c, f.planes.len())]
                                .into_iter()
                                .enumerate()
                            {
                                psnr[k] = psnr[k].min(plane_psnr(&f.planes[a..b], &expected[a..b]));
                            }
                        }
                    }
                    ms.push(f.decode_ms);
                    r.frames.push((f.pts, crc32(&f.planes)));
                    host.recycle(f.planes);
                }
                Some(VideoEvent::End { .. }) => break 'decode,
                Some(VideoEvent::Error(e)) => {
                    r.failures.push(e);
                    break 'decode;
                }
                Some(VideoEvent::Opened(_)) => {}
                None => {
                    r.failures.push("no frame within a minute".into());
                    break 'decode;
                }
            }
        }
    }
    if reference.is_some() {
        r.psnr = Some((psnr, ref_frames));
    }
    let n = r.frames.len();
    if n == 0 {
        r.failures.push("no frame decoded".into());
        r.decoder = v.decoder_status();
        return r;
    }
    r.slowest = ms
        .iter()
        .enumerate()
        .max_by(|a, b| a.1.total_cmp(b.1))
        .map_or(0, |(i, _)| i);
    ms.sort_by(f64::total_cmp);
    r.ms_mean = ms.iter().sum::<f64>() / n as f64;
    r.ms_p99 = ms[((n - 1) as f64 * 0.99) as usize];
    r.ms_max = ms[n - 1];
    // Seeks: frame times, mid-periods, gaps, the end, past it, the start.
    let pts: Vec<f64> = r.frames.iter().map(|f| f.0).collect();
    let step = (n / 60).max(1);
    let mut targets: Vec<(f64, Option<usize>)> = vec![(0.0, Some(0)), (pts[n - 1], Some(n - 1))];
    for i in (0..n).step_by(step) {
        targets.push((pts[i], Some(i)));
        if i + 1 < n {
            targets.push(((pts[i] + pts[i + 1]) / 2.0, Some(i)));
        }
    }
    let mut periods: Vec<f64> = pts.windows(2).map(|w| w[1] - w[0]).collect();
    periods.sort_by(f64::total_cmp);
    if let Some(&period) = periods.get(periods.len() / 2) {
        for i in 0..n.saturating_sub(1) {
            if pts[i + 1] - pts[i] > 1.5 * period {
                targets.push(((pts[i] + pts[i + 1]) / 2.0, Some(i)));
            }
        }
    }
    targets.push((pts[n - 1] + 10.0, None));
    // A loop: back to the start after the end.
    targets.push((0.0, Some(0)));
    for (t, expect) in targets {
        let started = Instant::now();
        v.seek(t);
        v.want(1);
        let got = next(&mut v, &mut events);
        r.seek_ms = r.seek_ms.max(started.elapsed().as_secs_f64() * 1e3);
        r.seeks += 1;
        match (got, expect) {
            (Some(VideoEvent::Frame(f)), Some(i)) => {
                if (f.pts - pts[i]).abs() > 1e-6 {
                    r.failures.push(format!(
                        "seek to {t:.4} s gave the frame at {:.4} s, expected frame {i} at {:.4} s",
                        f.pts, pts[i]
                    ));
                } else if crc32(&f.planes) != r.frames[i].1 {
                    r.failures
                        .push(format!("seek to {t:.4} s gave frame {i} with other pixels"));
                }
                host.recycle(f.planes);
            }
            (Some(VideoEvent::End { .. }), None) => {}
            (got, _) => r.failures.push(format!(
                "seek to {t:.4} s: {:?}",
                got.map(|e| match e {
                    VideoEvent::Frame(f) => format!("frame at {:.4}", f.pts),
                    other => format!("{other:?}"),
                })
            )),
        }
    }
    r.decoder = v.decoder_status();
    r
}

impl CheckReport {
    /// The report as `video/check.mjs` words it.
    pub fn lines(&self, name: &str) -> Vec<String> {
        let mut out = Vec::new();
        if let Some(d) = &self.decoder {
            out.push(format!(
                "decoder: {}{}",
                d.backend,
                d.fallback
                    .as_ref()
                    .map_or(String::new(), |why| format!(" (fallback: {why})"))
            ));
        }
        let status = match &self.status {
            Some(ModuleStatus::Ready { load_ms, cached }) => format!(
                "module {} in {load_ms:.0} ms",
                if *cached {
                    "read from the cache"
                } else {
                    "compiled"
                }
            ),
            Some(s) => format!("module {s:?}"),
            None => String::new(),
        };
        if let Some(i) = &self.info {
            out.push(format!(
                "{name}: {} {}x{}, duration {}, opened in {:.0} ms ({status})",
                i.codec,
                i.width,
                i.height,
                i.duration.map_or("unknown".into(), |d| format!("{d:.3} s")),
                self.open_ms
            ));
        } else {
            out.push(format!("{name}: not opened ({status})"));
        }
        if let (Some(first), Some(last)) = (self.frames.first(), self.frames.last()) {
            out.push(format!(
                "  {} frames, {:.2} ms/frame (p99 {:.2}, max {:.2} at frame {}; {:.0} frames a second), pts {:.3}..{:.3} s",
                self.frames.len(),
                self.ms_mean,
                self.ms_p99,
                self.ms_max,
                self.slowest,
                1000.0 / self.ms_mean.max(1e-6),
                first.0,
                last.0
            ));
        }
        if let Some((p, n)) = self.psnr {
            let f = |x: f64| {
                if x.is_infinite() {
                    "exact".to_string()
                } else {
                    format!("{x:.1} dB")
                }
            };
            out.push(format!(
                "  against the reference ({n} frames): worst PSNR Y {}, U {}, V {}",
                f(p[0]),
                f(p[1]),
                f(p[2])
            ));
        }
        if self.seeks > 0 {
            out.push(format!(
                "  {} seeks (and a loop) checked, slowest {:.0} ms",
                self.seeks, self.seek_ms
            ));
        }
        for f in &self.failures {
            out.push(format!("  FAIL {f}"));
        }
        out
    }
}

fn plane_psnr(a: &[u8], b: &[u8]) -> f64 {
    let se: f64 = a
        .iter()
        .zip(b)
        .map(|(&x, &y)| (f64::from(x) - f64::from(y)).powi(2))
        .sum();
    if se == 0.0 {
        f64::INFINITY
    } else {
        10.0 * (255.0 * 255.0 * a.len() as f64 / se).log10()
    }
}

/// zlib's CRC-32 (Node's `zlib.crc32`).
pub fn crc32(bytes: &[u8]) -> u32 {
    static TABLE: std::sync::OnceLock<[u32; 256]> = std::sync::OnceLock::new();
    let table = TABLE.get_or_init(|| {
        let mut t = [0u32; 256];
        for (i, e) in t.iter_mut().enumerate() {
            let mut c = i as u32;
            for _ in 0..8 {
                c = if c & 1 != 0 {
                    0xedb8_8320 ^ (c >> 1)
                } else {
                    c >> 1
                };
            }
            *e = c;
        }
        t
    });
    !bytes.iter().fold(!0u32, |c, &b| {
        table[((c ^ u32::from(b)) & 0xff) as usize] ^ (c >> 8)
    })
}

// --- The decoder thread ----------------------------------------------------

/// The thread: messages one at a time, in order, until every host is
/// gone. A panic while handling one (a bug of ours, met by what the
/// module did) counts as a crash of the decoder, so the thread lives on
/// and later movies get a fresh instance.
fn run(
    rx: Receiver<Msg>,
    shared: Arc<Mutex<Shared>>,
    module: Option<PathBuf>,
    deadline: Duration,
    backend: Backend,
) {
    let mut machine = Machine::new(module, Arc::clone(&shared), deadline);
    #[cfg(target_os = "macos")]
    let mut native = native::Hub::new(backend, deadline);
    #[cfg(not(target_os = "macos"))]
    {
        machine.backend = backend;
    }
    while let Ok(msg) = rx.recv() {
        let id = msg.id();
        let handled = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            #[cfg(target_os = "macos")]
            let msg = native.handle(&mut machine, msg);
            #[cfg(not(target_os = "macos"))]
            let msg = Some(msg);
            if let Some(msg) = msg {
                machine.handle(msg);
            }
        }));
        if let Err(panic) = handled {
            let what = panic
                .downcast_ref::<&str>()
                .map(|s| s.to_string())
                .or_else(|| panic.downcast_ref::<String>().cloned())
                .unwrap_or_else(|| "a panic".into());
            #[cfg(target_os = "macos")]
            native.crashed(&mut machine, &what);
            machine.crashed(&format!("the host failed ({what})"), id);
        }
    }
}

/// Calls `f` in the module with a deadline ([`CALL_DEADLINE`], or the
/// host's): a trap, `abort()`, the memory cap or the deadline passing is
/// a [`Crash`].
fn call<P: WasmParams, R: WasmResults>(
    store: &mut Store<Wasi>,
    f: &TypedFunc<P, R>,
    params: P,
) -> Result<R, Crash> {
    let ticks = store.data().deadline_ticks;
    store.set_epoch_deadline(ticks);
    f.call(&mut *store, params).map_err(|e| {
        if e.downcast_ref::<Trap>() == Some(&Trap::Interrupt) {
            Crash(format!(
                "a call took longer than {:.0} s",
                ticks as f64 * TICK.as_secs_f64()
            ))
        } else {
            Crash(format!("{e:#}"))
        }
    })
}

/// Moves the engine's epoch on every [`TICK`] (what the calls' deadlines
/// count), until dropped.
struct Ticker(Arc<AtomicBool>);

impl Ticker {
    fn start(engine: Engine) -> Ticker {
        let stop = Arc::new(AtomicBool::new(false));
        let flag = Arc::clone(&stop);
        let _ = std::thread::Builder::new()
            .name("ddi-video-ticker".into())
            .spawn(move || {
                while !flag.load(Ordering::Relaxed) {
                    std::thread::sleep(TICK);
                    engine.increment_epoch();
                }
            });
        Ticker(stop)
    }
}

impl Drop for Ticker {
    fn drop(&mut self) {
        self.0.store(true, Ordering::Relaxed);
    }
}

/// The module's exports this needs.
struct Exports {
    memory: Memory,
    malloc: TypedFunc<i32, i32>,
    free: TypedFunc<i32, ()>,
    open: TypedFunc<i32, i32>,
    width: TypedFunc<i32, i32>,
    height: TypedFunc<i32, i32>,
    codec: TypedFunc<i32, i32>,
    colorspace: TypedFunc<i32, i32>,
    full_range: TypedFunc<i32, i32>,
    planes: TypedFunc<i32, i32>,
    planes_size: TypedFunc<i32, i32>,
    last_time: TypedFunc<i32, f64>,
    duration: TypedFunc<i32, f64>,
    next: TypedFunc<i32, f64>,
    seek: TypedFunc<(i32, f64), i32>,
    seek_key: Option<TypedFunc<(i32, f64), i32>>,
    close: TypedFunc<i32, ()>,
}

/// A running instance.
struct Running {
    instance: Instance,
    store: Store<Wasi>,
    x: Exports,
}

/// One open movie on the thread.
struct Video {
    skip_through: Option<f64>,
    ptr: i32,
    file_name: String,
    epoch: u32,
    last: f64,
    bad: u32,
    ended: bool,
}

struct Machine {
    #[cfg(not(target_os = "macos"))]
    backend: Backend,
    module_path: Option<PathBuf>,
    /// Each call's deadline, in ticks.
    deadline_ticks: u64,
    engine: Option<Engine>,
    _ticker: Option<Ticker>,
    /// The module's `Ready` status once loaded (put back after a later
    /// failure to start an instance is followed by a success).
    loaded: Option<ModuleStatus>,
    module: Option<Module>,
    running: Option<Running>,
    videos: HashMap<u32, Video>,
    spare: Vec<Vec<u8>>,
    shared: Arc<Mutex<Shared>>,
}

/// Anything thrown out of the module (a trap, `abort()`, the memory cap).
struct Crash(String);

impl From<wasmtime::Error> for Crash {
    fn from(e: wasmtime::Error) -> Crash {
        Crash(format!("{e:#}"))
    }
}

impl Machine {
    fn new(
        module_path: Option<PathBuf>,
        shared: Arc<Mutex<Shared>>,
        deadline: Duration,
    ) -> Machine {
        Machine {
            #[cfg(not(target_os = "macos"))]
            backend: Backend::Module,
            module_path,
            deadline_ticks: (deadline.as_secs_f64() / TICK.as_secs_f64())
                .ceil()
                .max(1.0) as u64,
            engine: None,
            _ticker: None,
            loaded: None,
            module: None,
            running: None,
            videos: HashMap::new(),
            spare: Vec::new(),
            shared,
        }
    }

    fn shared(&self) -> std::sync::MutexGuard<'_, Shared> {
        self.shared.lock().unwrap_or_else(|e| e.into_inner())
    }

    /// Posts `event` for movie `id` (`epoch`: a frame's or an end's).
    fn post(&mut self, id: u32, epoch: Option<u32>, event: Event) {
        let back = self.shared().post(id, epoch, event);
        if let Some(planes) = back {
            self.keep_spare(planes);
        }
    }

    fn decoder_status(&self, id: u32, status: DecoderStatus) {
        eprintln!(
            "[video] movie {id}: {}{}",
            status.backend,
            status
                .fallback
                .as_ref()
                .map_or(String::new(), |e| format!(" ({e})"))
        );
        if let Some(movie) = self.shared().movies.get_mut(&id) {
            movie.decoder = status;
        }
    }

    fn keep_spare(&mut self, planes: Vec<u8>) {
        if planes.capacity() == 0 {
            return;
        }
        // The oldest goes when full, so a movie of another size takes over.
        if self.spare.len() >= SPARE_KEPT {
            self.spare.remove(0);
        }
        self.spare.push(planes);
    }

    fn handle(&mut self, msg: Msg) {
        #[cfg(not(target_os = "macos"))]
        if self.backend != Backend::Module
            && let Msg::Open { id, .. } = &msg
        {
            let why = "VideoToolbox is unavailable on this platform".to_string();
            self.decoder_status(
                *id,
                DecoderStatus {
                    backend: "module".into(),
                    fallback: Some(why.clone()),
                },
            );
            if self.backend == Backend::VideoToolbox {
                self.post(*id, None, Event::Error(why));
                return;
            }
        }

        let id = msg.id();
        // A movie whose open failed has no state; later messages are
        // dropped.
        if matches!(msg, Msg::Seek { .. } | Msg::Want { .. } | Msg::Close { .. })
            && !id.is_some_and(|id| self.videos.contains_key(&id))
        {
            return;
        }
        let result = match msg {
            Msg::Load => {
                let _ = self.ready(None);
                Ok(())
            }
            Msg::Recycle(planes) => {
                self.keep_spare(planes);
                Ok(())
            }
            Msg::Open { id, path, name } => self.open(id, path, &name),
            Msg::Seek { id, t, epoch } => self.seek(id, t, epoch),
            Msg::Want { id, n, epoch } => self.want(id, n, epoch),
            Msg::Close { id } => self.close(id),
            Msg::Probe { id, path, name } => self.probe(id, path, &name),
        };
        if let Err(Crash(cause)) = result {
            self.crashed(&cause, id);
        }
    }

    /// The module, compiled (or read back from the cache), ready.
    fn compiled(&mut self) -> Result<(), String> {
        if self.module.is_some() {
            return Ok(());
        }
        let path = self
            .module_path
            .clone()
            .ok_or_else(|| NOT_INCLUDED.to_string())?;
        self.shared().status = ModuleStatus::Loading;
        let start = Instant::now();
        let engine = match &self.engine {
            Some(e) => e.clone(),
            None => {
                let mut config = Config::new();
                config.cranelift_opt_level(OptLevel::Speed);
                config.parallel_compilation(true);
                // Deadlines on calls (`call`).
                config.epoch_interruption(true);
                let e = Engine::new(&config).map_err(|e| format!("{e:#}"))?;
                self._ticker = Some(Ticker::start(e.clone()));
                self.engine = Some(e.clone());
                e
            }
        };
        let (module, cached) = load_module(&engine, &path)?;
        self.module = Some(module);
        let ready = ModuleStatus::Ready {
            load_ms: start.elapsed().as_secs_f64() * 1e3,
            cached,
        };
        self.loaded = Some(ready.clone());
        self.shared().status = ready;
        Ok(())
    }

    /// A running instance; `false` (with `Failed` and, for `id`, an
    /// error posted) when the module could not be had.
    fn ready(&mut self, id: Option<u32>) -> bool {
        let result = self.compiled().and_then(|()| self.instantiate());
        match result {
            Ok(()) => {
                // A failure to start before this success is over.
                let loaded = self.loaded.clone();
                let mut shared = self.shared();
                if let (ModuleStatus::Failed(_), Some(ready)) = (&shared.status, loaded) {
                    shared.status = ready;
                }
                true
            }
            Err(e) => {
                self.shared().status = ModuleStatus::Failed(e.clone());
                if let Some(id) = id {
                    self.post(id, None, Event::Error(e));
                }
                false
            }
        }
    }

    fn instantiate(&mut self) -> Result<(), String> {
        if self.running.is_some() {
            return Ok(());
        }
        let (Some(engine), Some(module)) = (&self.engine, &self.module) else {
            return Err(NOT_INCLUDED.into());
        };
        let mut store = Store::new(
            engine,
            Wasi {
                limits: StoreLimitsBuilder::new()
                    .memory_size(MAX_MEMORY)
                    .instances(1)
                    .build(),
                deadline_ticks: self.deadline_ticks,
                ..Wasi::default()
            },
        );
        store.limiter(|w| &mut w.limits);
        // Instantiating may run the module's start code too.
        store.set_epoch_deadline(self.deadline_ticks);
        let mut linker = Linker::new(engine);
        wasi::add(&mut linker).map_err(|e| format!("{e:#}"))?;
        let instance = linker
            .instantiate(&mut store, module)
            .map_err(|e| format!("the video decoder does not start: {e:#}"))?;
        let x = exports(&instance, &mut store).map_err(|e| format!("{e:#}"))?;
        if let Ok(init) = instance.get_typed_func::<(), ()>(&mut store, "_initialize") {
            call(&mut store, &init, ())
                .map_err(|Crash(e)| format!("the video decoder does not start: {e}"))?;
        }
        self.running = Some(Running { instance, store, x });
        Ok(())
    }

    /// After a crash the module's memory cannot be trusted: every open
    /// movie fails (and `id`, the one being opened then), and the next
    /// open starts a fresh instance.
    fn crashed(&mut self, cause: &str, id: Option<u32>) {
        let message = format!("the video decoder crashed: {cause}");
        eprintln!("[video] {message}");
        let open: Vec<u32> = self.videos.keys().copied().collect();
        for v in &open {
            self.post(*v, None, Event::Error(message.clone()));
        }
        if let Some(id) = id
            && !open.contains(&id)
        {
            self.post(id, None, Event::Error(message.clone()));
            if let Some(probe) = self.shared().probes.get_mut(&id) {
                probe.get_or_insert_with(|| Err(message.clone()));
            }
        }
        self.videos.clear();
        self.running = None;
    }

    /// Opens `path` with the existing decode ABI.
    fn open_file(&mut self, file_name: &str, path: MovieFile) -> Result<i32, Crash> {
        let open = self.running.as_ref().expect("instance").x.open.clone();
        self.open_file_using(file_name, path, open)
    }

    /// The file/range sandbox and path allocation are shared by decode and
    /// demux. A trap leaves cleanup to the caller's instance disposal.
    fn open_file_using(
        &mut self,
        file_name: &str,
        path: MovieFile,
        open: TypedFunc<i32, i32>,
    ) -> Result<i32, Crash> {
        let r = self.running.as_mut().expect("instance");
        r.store.data_mut().files.insert(file_name.to_string(), path);
        let c = format!("/v/{file_name}\0");
        let p = call(&mut r.store, &r.x.malloc, c.len() as i32)?;
        if p == 0 {
            return Err(Crash("cannot allocate the movie's path".into()));
        }
        r.x.memory
            .write(&mut r.store, p as u32 as usize, c.as_bytes())
            .map_err(|e| Crash(e.to_string()))?;
        let ptr = call(&mut r.store, &open, p)?;
        call(&mut r.store, &r.x.free, p)?;
        if ptr == 0 {
            r.store.data_mut().files.remove(file_name);
        }
        Ok(ptr)
    }

    fn open(&mut self, id: u32, path: MovieFile, name: &str) -> Result<(), Crash> {
        self.open_announced(id, path, name, true)
    }

    fn open_announced(
        &mut self,
        id: u32,
        path: MovieFile,
        name: &str,
        announce: bool,
    ) -> Result<(), Crash> {
        if !self.ready(Some(id)) {
            return Ok(());
        }
        let file_name = format!("{id}{}", extension_of(name));
        let ptr = self.open_file(&file_name, path)?;
        if ptr == 0 {
            self.post(
                id,
                None,
                Event::Error(format!(
                    "cannot open {name} (not a movie the decoder knows)"
                )),
            );
            return Ok(());
        }
        self.videos.insert(
            id,
            Video {
                skip_through: None,
                ptr,
                file_name,
                epoch: 0,
                last: -1.0,
                bad: 0,
                ended: false,
            },
        );
        let r = self.running.as_mut().expect("instance");
        let duration = call(&mut r.store, &r.x.duration, ptr)?;
        let info = VideoInfo {
            width: call(&mut r.store, &r.x.width, ptr)?.max(0) as u32,
            height: call(&mut r.store, &r.x.height, ptr)?.max(0) as u32,
            duration: (duration >= 0.0).then_some(duration),
            codec: {
                let at = call(&mut r.store, &r.x.codec, ptr)?;
                c_string(r, at)
            },
        };
        if announce {
            self.post(id, None, Event::Opened(info));
        }
        Ok(())
    }

    fn want(&mut self, id: u32, n: u32, epoch: u32) -> Result<(), Crash> {
        let mut i = 0;
        while i < n {
            let Some(video) = self.videos.get(&id) else {
                return Ok(());
            };
            if epoch != video.epoch || video.ended {
                return Ok(());
            }
            let ptr = video.ptr;
            let r = self.running.as_mut().expect("instance");
            let start = Instant::now();
            let pts = call(&mut r.store, &r.x.next, ptr)?;
            let ms = start.elapsed().as_secs_f64() * 1e3;
            if pts == -1.0 {
                let last_time = call(&mut r.store, &r.x.last_time, ptr)?;
                let video = self.videos.get_mut(&id).expect("video");
                video.ended = true;
                // The last frame of the file, even when a seek past the end
                // decoded it without handing it over.
                let last = video.last.max(last_time);
                let g = video.epoch;
                self.post(
                    id,
                    Some(g),
                    Event::End {
                        last_pts: (last >= 0.0).then_some(last),
                    },
                );
                return Ok(());
            }
            if pts < 0.0 {
                let video = self.videos.get_mut(&id).expect("video");
                video.bad += 1;
                if video.bad > MAX_BAD_FRAMES {
                    self.post(
                        id,
                        None,
                        Event::Error(format!(
                            "the movie does not decode ({MAX_BAD_FRAMES} broken frames in a row)"
                        )),
                    );
                    self.close(id)?;
                    return Ok(());
                }
                continue;
            }
            if self
                .videos
                .get(&id)
                .and_then(|v| v.skip_through)
                .is_some_and(|t| pts <= t + 1e-6)
            {
                self.videos.get_mut(&id).unwrap().last = pts;
                continue;
            }
            let frame = self.frame(ptr, pts, ms)?;
            let video = self.videos.get_mut(&id).expect("video");
            video.bad = 0;
            video.last = pts;
            let g = video.epoch;
            self.post(id, Some(g), Event::Frame(frame));
            i += 1;
        }
        Ok(())
    }

    /// The frame just decoded, its planes copied into a spare buffer.
    fn frame(&mut self, ptr: i32, pts: f64, decode_ms: f64) -> Result<Frame, Crash> {
        let r = self.running.as_mut().expect("instance");
        let at = call(&mut r.store, &r.x.planes, ptr)? as u32 as usize;
        let size = call(&mut r.store, &r.x.planes_size, ptr)? as u32 as usize;
        let width = call(&mut r.store, &r.x.width, ptr)?.max(0) as u32;
        let height = call(&mut r.store, &r.x.height, ptr)?.max(0) as u32;
        let matrix = if call(&mut r.store, &r.x.colorspace, ptr)? == 709 {
            YuvMatrix::Bt709
        } else {
            YuvMatrix::Bt601
        };
        let full_range = call(&mut r.store, &r.x.full_range, ptr)? != 0;
        let data = r.x.memory.data(&r.store);
        let src = data
            .get(at..at.saturating_add(size))
            .ok_or_else(|| Crash("frame planes outside memory".into()))?;
        let i = self.spare.iter().position(|v| v.capacity() >= size);
        let mut planes = i.map_or_else(|| Vec::with_capacity(size), |i| self.spare.swap_remove(i));
        planes.clear();
        planes.extend_from_slice(src);
        Ok(Frame {
            pts,
            width,
            height,
            matrix,
            full_range,
            planes,
            decode_ms,
        })
    }

    fn seek(&mut self, id: u32, t: f64, epoch: u32) -> Result<(), Crash> {
        let Some(video) = self.videos.get_mut(&id) else {
            return Ok(());
        };
        video.epoch = epoch;
        video.skip_through = None;
        video.last = -1.0;
        video.bad = 0;
        video.ended = false;
        let ptr = video.ptr;
        let r = self.running.as_mut().expect("instance");
        if call(&mut r.store, &r.x.seek, (ptr, t))? != 0 {
            self.post(
                id,
                None,
                Event::Error("the movie could not be read again after seeking".into()),
            );
            self.close(id)?;
        }
        Ok(())
    }

    fn close(&mut self, id: u32) -> Result<(), Crash> {
        let Some(video) = self.videos.remove(&id) else {
            return Ok(());
        };
        if let Some(r) = self.running.as_mut() {
            call(&mut r.store, &r.x.close, video.ptr)?;
            r.store.data_mut().files.remove(&video.file_name);
        }
        Ok(())
    }

    /// Samples the movie's picture for the black-bar probe: the Y planes
    /// of a few frames spread through it, each the first from a keyframe
    /// (`ddi_seek_key`; exactly when that lands past the last keyframe).
    fn probe(&mut self, id: u32, path: MovieFile, name: &str) -> Result<(), Crash> {
        let answer = |m: &mut Machine, a: Result<Picture, String>| {
            if let Some(p) = m.shared().probes.get_mut(&id) {
                *p = Some(a);
            }
        };
        if !self.ready(None) {
            let e = match self.shared().status.clone() {
                ModuleStatus::Failed(e) => e,
                _ => NOT_INCLUDED.to_string(),
            };
            answer(self, Err(e));
            return Ok(());
        }
        let file_name = format!("probe-{id}{}", extension_of(name));
        let ptr = self.open_file(&file_name, path)?;
        if ptr == 0 {
            answer(
                self,
                Err(format!(
                    "cannot open {name} (not a movie the decoder knows)"
                )),
            );
            return Ok(());
        }
        let r = self.running.as_mut().expect("instance");
        let duration = call(&mut r.store, &r.x.duration, ptr)?;
        let times = if duration > 0.0 {
            PROBE_SHARES.map(|s| s * duration)
        } else {
            PROBE_SECONDS
        };
        let (mut width, mut height, mut full_range) = (0, 0, false);
        let mut planes: Vec<Vec<u8>> = Vec::new();
        for t in times {
            let sample = |r: &mut Running, key: bool| -> Result<f64, Crash> {
                match (&r.x.seek_key, key) {
                    (Some(k), true) => call(&mut r.store, k, (ptr, t))?,
                    _ => call(&mut r.store, &r.x.seek, (ptr, t))?,
                };
                let mut pts = -2.0;
                for _ in 0..8 {
                    if pts != -2.0 {
                        break;
                    }
                    pts = call(&mut r.store, &r.x.next, ptr)?;
                }
                Ok(pts)
            };
            let mut pts = sample(r, true)?;
            if pts == -1.0 {
                pts = sample(r, false)?;
            }
            if pts < 0.0 {
                continue;
            }
            let w = call(&mut r.store, &r.x.width, ptr)?.max(0) as u32;
            let h = call(&mut r.store, &r.x.height, ptr)?.max(0) as u32;
            if planes.is_empty() {
                width = w;
                height = h;
                full_range = call(&mut r.store, &r.x.full_range, ptr)? != 0;
            } else if (w, h) != (width, height) {
                continue;
            }
            let at = call(&mut r.store, &r.x.planes, ptr)? as u32 as usize;
            let y = (w as usize).saturating_mul(h as usize);
            let plane =
                r.x.memory
                    .data(&r.store)
                    .get(at..at.saturating_add(y))
                    .ok_or_else(|| Crash("a probed frame outside memory".into()))?;
            planes.push(plane.to_vec());
        }
        call(&mut r.store, &r.x.close, ptr)?;
        r.store.data_mut().files.remove(&file_name);
        let samples: Vec<&[u8]> = planes.iter().map(Vec::as_slice).collect();
        let bounds = ddi_platform::video::picture_bounds(&samples, width, height, full_range);
        answer(
            self,
            Ok(Picture {
                width,
                height,
                bounds,
            }),
        );
        Ok(())
    }
}

/// `.ext` of a file name (lowercase), or nothing.
fn extension_of(name: &str) -> String {
    match name.rsplit_once('.') {
        Some((_, ext))
            if (1..=8).contains(&ext.len()) && ext.bytes().all(|b| b.is_ascii_alphanumeric()) =>
        {
            format!(".{}", ext.to_ascii_lowercase())
        }
        _ => String::new(),
    }
}

/// A NUL-terminated string in the module's memory.
fn c_string(r: &Running, ptr: i32) -> String {
    let data = r.x.memory.data(&r.store);
    let from = ptr as u32 as usize;
    let s = data.get(from..).unwrap_or(&[]);
    let end = s.iter().position(|&b| b == 0).unwrap_or(s.len());
    String::from_utf8_lossy(&s[..end]).into_owned()
}

fn exports(instance: &Instance, store: &mut Store<Wasi>) -> wasmtime::Result<Exports> {
    Ok(Exports {
        memory: instance
            .get_memory(&mut *store, "memory")
            .ok_or_else(|| wasmtime::Error::msg("the module has no memory"))?,
        malloc: instance.get_typed_func(&mut *store, "ddi_malloc")?,
        free: instance.get_typed_func(&mut *store, "ddi_free")?,
        open: instance.get_typed_func(&mut *store, "ddi_open")?,
        width: instance.get_typed_func(&mut *store, "ddi_width")?,
        height: instance.get_typed_func(&mut *store, "ddi_height")?,
        codec: instance.get_typed_func(&mut *store, "ddi_codec")?,
        colorspace: instance.get_typed_func(&mut *store, "ddi_colorspace")?,
        full_range: instance.get_typed_func(&mut *store, "ddi_full_range")?,
        planes: instance.get_typed_func(&mut *store, "ddi_planes")?,
        planes_size: instance.get_typed_func(&mut *store, "ddi_planes_size")?,
        last_time: instance.get_typed_func(&mut *store, "ddi_last_time")?,
        duration: instance.get_typed_func(&mut *store, "ddi_duration")?,
        next: instance.get_typed_func(&mut *store, "ddi_next")?,
        seek: instance.get_typed_func(&mut *store, "ddi_seek")?,
        seek_key: instance.get_typed_func(&mut *store, "ddi_seek_key").ok(),
        close: instance.get_typed_func(&mut *store, "ddi_close")?,
    })
}

/// The module at `path`, from the cache when it was compiled before by
/// this wasmtime (its own check refuses another version's or config's),
/// else compiled and kept there. Whether it came from the cache.
fn load_module(engine: &Engine, path: &Path) -> Result<(Module, bool), String> {
    load_module_in(engine, path, cache_dir())
}

/// [`load_module`] with the cache in `dir`.
fn load_module_in(
    engine: &Engine,
    path: &Path,
    dir: Option<PathBuf>,
) -> Result<(Module, bool), String> {
    let bytes = std::fs::read(path).map_err(|e| match e.kind() {
        std::io::ErrorKind::NotFound => NOT_INCLUDED.to_string(),
        _ => format!("reading the video decoder failed: {e}"),
    })?;
    let name = cache_name(engine, &bytes);
    let cached = dir.as_ref().map(|d| d.join(&name));
    if let Some(c) = &cached
        && let Ok(file) = std::fs::read(c)
    {
        if let Some(code) = unseal(&file) {
            // SAFETY: `deserialize` runs the machine code in `code`, so
            // it must be what `Module::serialize` wrote. It is: the file
            // is in the user's own cache folder, named by the module's
            // hash and this engine's, written by this function (aside,
            // then renamed into place), and its bytes match the length
            // and checksums sealed with them, so a file cut short or
            // damaged on disk is compiled again instead of run. wasmtime
            // checks the header against this engine's version and
            // settings. (Nothing here defends against someone who can
            // write to the user's cache folder: they can run code as the
            // user anyway.)
            if let Ok(m) = unsafe { Module::deserialize(engine, code) } {
                return Ok((m, true));
            }
        }
        let _ = std::fs::remove_file(c);
    }
    let module = Module::new(engine, &bytes)
        .map_err(|e| format!("the video decoder does not compile: {e:#}"))?;
    if let (Some(c), Some(dir)) = (cached, dir)
        && std::fs::create_dir_all(&dir).is_ok()
        && let Ok(data) = module.serialize()
    {
        // Written aside and moved in, so a half-written file is never
        // read; the name is this process's own.
        static TMP: AtomicU64 = AtomicU64::new(0);
        let tmp = dir.join(format!(
            "{name}.tmp-{}-{}",
            std::process::id(),
            TMP.fetch_add(1, Ordering::Relaxed)
        ));
        if std::fs::write(&tmp, seal(&data)).is_ok() {
            let _ = std::fs::rename(&tmp, &c);
        }
        let _ = std::fs::remove_file(&tmp);
        prune_cache(&dir, &name);
    }
    Ok((module, false))
}

/// What a cache file starts with.
const CACHE_MAGIC: &[u8; 8] = b"DDIVIDC1";

/// A compiled module as kept in the cache: [`CACHE_MAGIC`], its length,
/// two checksums of it (CRC-32 and SipHash), then the bytes.
fn seal(code: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(code.len() + 28);
    out.extend_from_slice(CACHE_MAGIC);
    out.extend_from_slice(&(code.len() as u64).to_le_bytes());
    out.extend_from_slice(&crc32(code).to_le_bytes());
    out.extend_from_slice(&siphash(code).to_le_bytes());
    out.extend_from_slice(code);
    out
}

/// The bytes [`seal`] kept, if the file is whole and unchanged.
fn unseal(file: &[u8]) -> Option<&[u8]> {
    let rest = file.strip_prefix(CACHE_MAGIC)?;
    let len = u64::from_le_bytes(rest.get(..8)?.try_into().ok()?);
    let crc = u32::from_le_bytes(rest.get(8..12)?.try_into().ok()?);
    let sip = u64::from_le_bytes(rest.get(12..20)?.try_into().ok()?);
    let code = rest.get(20..)?;
    (code.len() as u64 == len && crc32(code) == crc && siphash(code) == sip).then_some(code)
}

fn siphash(bytes: &[u8]) -> u64 {
    use std::hash::{Hash, Hasher};
    let mut h = std::collections::hash_map::DefaultHasher::new();
    bytes.hash(&mut h);
    h.finish()
}

/// Removes the cache's other modules (an older module's or wasmtime's,
/// never read again) and temporary files left by a process that died
/// mid-write (an hour old).
fn prune_cache(dir: &Path, keep: &str) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for e in entries.flatten() {
        let name = e.file_name().to_string_lossy().into_owned();
        if !name.starts_with("ddivideo-") || name == keep {
            continue;
        }
        let stale = name.contains(".tmp")
            && e.metadata()
                .and_then(|m| m.modified())
                .ok()
                .and_then(|t| t.elapsed().ok())
                .is_some_and(|age| age > Duration::from_secs(3600));
        if name.ends_with(".cwasm") || stale {
            let _ = std::fs::remove_file(e.path());
        }
    }
}

/// The cache file's name: the module's hash and length, and the engine's
/// (wasmtime's version and settings).
fn cache_name(engine: &Engine, bytes: &[u8]) -> String {
    use std::hash::{Hash, Hasher};
    let mut compat = std::collections::hash_map::DefaultHasher::new();
    engine.precompile_compatibility_hash().hash(&mut compat);
    format!(
        "ddivideo-{:016x}-{}-{:016x}.cwasm",
        siphash(bytes),
        bytes.len(),
        compat.finish()
    )
}

// --- WASI: the module's 20 calls ---------------------------------------------

/// What the calls work on (`video_worker.js`'s `files`, `fds`).
#[derive(Default)]
struct Wasi {
    /// File name in `/v` → the file.
    files: HashMap<String, MovieFile>,
    fds: HashMap<u32, OpenFile>,
    next_fd: u32,
    log: [String; 2],
    log_lines: u32,
    limits: StoreLimits,
    /// Each call's deadline, in ticks (`call`).
    deadline_ticks: u64,
}

struct OpenFile {
    file: std::fs::File,
    /// Where its bytes start in `file` (a zip's stored entry).
    base: u64,
    size: u64,
    pos: u64,
    cache: Vec<u8>,
    cache_at: u64,
}

impl OpenFile {
    /// Bytes `[pos, pos + n)`, through a read-ahead window.
    fn read_at(&mut self, pos: u64, n: usize) -> std::io::Result<&[u8]> {
        let end = self.size.min(pos + n as u64);
        if pos >= end {
            return Ok(&[]);
        }
        let inside = pos >= self.cache_at && end <= self.cache_at + self.cache.len() as u64;
        if !inside {
            let to = self.size.min(pos + n.max(READ_AHEAD) as u64);
            self.cache.resize((to - pos) as usize, 0);
            self.file.seek(SeekFrom::Start(self.base + pos))?;
            self.file.read_exact(&mut self.cache)?;
            self.cache_at = pos;
        }
        let from = (pos - self.cache_at) as usize;
        Ok(&self.cache[from..from + (end - pos) as usize])
    }
}

mod wasi {
    use super::*;

    const SUCCESS: i32 = 0;
    const BADF: i32 = 8;
    const INVAL: i32 = 28;
    const IO: i32 = 29;
    const ISDIR: i32 = 31;
    const NOENT: i32 = 44;
    const NOSYS: i32 = 52;
    const NOTDIR: i32 = 54;
    const MFILE: i32 = 33;
    const ROFS: i32 = 69;
    const SPIPE: i32 = 70;
    const CHARACTER_DEVICE: u8 = 2;
    const DIRECTORY: u8 = 3;
    const REGULAR_FILE: u8 = 4;
    const CREAT: i32 = 1;
    const OFLAG_DIRECTORY: i32 = 2;
    const EXCL: i32 = 4;
    const TRUNC: i32 = 8;
    /// The module opens `/v/<name>`.
    const PREOPEN: &str = "/v";
    const PREOPEN_FD: u32 = 3;

    type C<'a> = Caller<'a, Wasi>;

    /// The module's memory and the calls' state.
    fn parts<'a>(c: &'a mut C<'_>) -> Option<(&'a mut [u8], &'a mut Wasi)> {
        let mem = c.get_export("memory")?.into_memory()?;
        Some(mem.data_and_store_mut(c))
    }

    fn put(mem: &mut [u8], at: i32, bytes: &[u8]) -> bool {
        let at = at as u32 as usize;
        match mem.get_mut(at..at + bytes.len()) {
            Some(d) => {
                d.copy_from_slice(bytes);
                true
            }
            None => false,
        }
    }

    fn get_u32(mem: &[u8], at: usize) -> Option<u32> {
        Some(u32::from_le_bytes(mem.get(at..at + 4)?.try_into().ok()?))
    }

    fn name(mem: &[u8], ptr: i32, len: i32) -> String {
        let (p, l) = (ptr as u32 as usize, len as u32 as usize);
        let s = String::from_utf8_lossy(mem.get(p..p + l).unwrap_or(&[])).into_owned();
        s.strip_prefix("./").map(str::to_string).unwrap_or(s)
    }

    /// A `filestat` of `filetype` and `size` at `ptr` (64 bytes).
    fn filestat(mem: &mut [u8], ptr: i32, filetype: u8, size: u64) -> bool {
        let mut st = [0u8; 64];
        st[16] = filetype;
        st[24..32].copy_from_slice(&1u64.to_le_bytes());
        st[32..40].copy_from_slice(&size.to_le_bytes());
        put(mem, ptr, &st)
    }

    fn log(w: &mut Wasi, fd: usize, text: &str) {
        w.log[fd - 1].push_str(text);
        // A line that never ends is cut, not kept growing.
        if w.log[fd - 1].len() > MAX_LOG_LINE && !w.log[fd - 1].contains('\n') {
            let mut cut = MAX_LOG_LINE;
            while !w.log[fd - 1].is_char_boundary(cut) {
                cut -= 1;
            }
            w.log[fd - 1].truncate(cut);
            w.log[fd - 1].push('\n');
        }
        while let Some(nl) = w.log[fd - 1].find('\n') {
            let line: String = w.log[fd - 1].drain(..=nl).collect();
            w.log_lines += 1;
            if w.log_lines < MAX_LOG_LINES {
                eprintln!("[video] {}", line.trim_end());
            } else if w.log_lines == MAX_LOG_LINES {
                eprintln!("[video] further decoder messages are not shown");
            }
        }
    }

    pub(super) fn add(l: &mut Linker<Wasi>) -> wasmtime::Result<()> {
        const M: &str = "wasi_snapshot_preview1";
        l.func_wrap(M, "environ_get", |_: C<'_>, _: i32, _: i32| SUCCESS)?;
        l.func_wrap(
            M,
            "environ_sizes_get",
            |mut c: C<'_>, count: i32, size: i32| {
                let Some((mem, _)) = parts(&mut c) else {
                    return INVAL;
                };
                if put(mem, count, &0u32.to_le_bytes()) && put(mem, size, &0u32.to_le_bytes()) {
                    SUCCESS
                } else {
                    INVAL
                }
            },
        )?;
        l.func_wrap(
            M,
            "clock_time_get",
            |mut c: C<'_>, id: i32, _p: i64, out: i32| {
                let ns: u64 = if id == 0 {
                    std::time::SystemTime::now()
                        .duration_since(std::time::UNIX_EPOCH)
                        .map_or(0, |d| d.as_nanos() as u64)
                } else {
                    crate::clock::now_ns()
                };
                let Some((mem, _)) = parts(&mut c) else {
                    return INVAL;
                };
                if put(mem, out, &ns.to_le_bytes()) {
                    SUCCESS
                } else {
                    INVAL
                }
            },
        )?;
        l.func_wrap(M, "fd_prestat_get", |mut c: C<'_>, fd: i32, out: i32| {
            if fd as u32 != PREOPEN_FD {
                return BADF;
            }
            let Some((mem, _)) = parts(&mut c) else {
                return INVAL;
            };
            let mut p = [0u8; 8];
            p[4..8].copy_from_slice(&(PREOPEN.len() as u32).to_le_bytes());
            if put(mem, out, &p) { SUCCESS } else { INVAL }
        })?;
        l.func_wrap(
            M,
            "fd_prestat_dir_name",
            |mut c: C<'_>, fd: i32, ptr: i32, len: i32| {
                if fd as u32 != PREOPEN_FD {
                    return BADF;
                }
                let Some((mem, _)) = parts(&mut c) else {
                    return INVAL;
                };
                let n = (len.max(0) as usize).min(PREOPEN.len());
                if put(mem, ptr, &PREOPEN.as_bytes()[..n]) {
                    SUCCESS
                } else {
                    INVAL
                }
            },
        )?;
        l.func_wrap(M, "fd_fdstat_get", |mut c: C<'_>, fd: i32, out: i32| {
            let Some((mem, w)) = parts(&mut c) else {
                return INVAL;
            };
            let fd = fd as u32;
            let kind = if fd <= 2 {
                CHARACTER_DEVICE
            } else if fd == PREOPEN_FD {
                DIRECTORY
            } else if w.fds.contains_key(&fd) {
                REGULAR_FILE
            } else {
                return BADF;
            };
            let mut st = [0u8; 24];
            st[0] = kind;
            st[8..16].copy_from_slice(&u64::MAX.to_le_bytes());
            st[16..24].copy_from_slice(&u64::MAX.to_le_bytes());
            if put(mem, out, &st) { SUCCESS } else { INVAL }
        })?;
        l.func_wrap(M, "fd_fdstat_set_flags", |_: C<'_>, _: i32, _: i32| NOSYS)?;
        l.func_wrap(M, "fd_filestat_get", |mut c: C<'_>, fd: i32, out: i32| {
            let Some((mem, w)) = parts(&mut c) else {
                return INVAL;
            };
            let fd = fd as u32;
            let ok = if fd == PREOPEN_FD {
                filestat(mem, out, DIRECTORY, 0)
            } else if let Some(f) = w.fds.get(&fd) {
                filestat(mem, out, REGULAR_FILE, f.size)
            } else if fd <= 2 {
                filestat(mem, out, CHARACTER_DEVICE, 0)
            } else {
                return BADF;
            };
            if ok { SUCCESS } else { INVAL }
        })?;
        l.func_wrap(
            M,
            "path_filestat_get",
            |mut c: C<'_>, fd: i32, _flags: i32, ptr: i32, len: i32, out: i32| {
                if fd as u32 != PREOPEN_FD {
                    return BADF;
                }
                let Some((mem, w)) = parts(&mut c) else {
                    return INVAL;
                };
                let n = name(mem, ptr, len);
                let ok = if n.is_empty() || n == "." {
                    filestat(mem, out, DIRECTORY, 0)
                } else if let Some(p) = w.files.get(&n) {
                    filestat(mem, out, REGULAR_FILE, p.size())
                } else {
                    return NOENT;
                };
                if ok { SUCCESS } else { INVAL }
            },
        )?;
        l.func_wrap(
            M,
            "path_open",
            |mut c: C<'_>,
             fd: i32,
             _dirflags: i32,
             ptr: i32,
             len: i32,
             oflags: i32,
             _rights: i64,
             _inheriting: i64,
             _fdflags: i32,
             out: i32| {
                if fd as u32 != PREOPEN_FD {
                    return BADF;
                }
                let Some((mem, w)) = parts(&mut c) else {
                    return INVAL;
                };
                let n = name(mem, ptr, len);
                if oflags & (CREAT | EXCL | TRUNC) != 0 {
                    return ROFS;
                }
                let Some(path) = w.files.get(&n) else {
                    return NOENT;
                };
                if oflags & OFLAG_DIRECTORY != 0 {
                    return NOTDIR;
                }
                if w.fds.len() >= MAX_OPEN_FILES {
                    return MFILE;
                }
                let Ok(file) = std::fs::File::open(&path.path) else {
                    return NOENT;
                };
                let len = file.metadata().map_or(0, |m| m.len());
                // A range past the file's end is cut to it.
                let (base, size) = match path.range {
                    Some((at, n)) => (at.min(len), n.min(len.saturating_sub(at))),
                    None => (0, len),
                };
                let opened = w.next_fd.max(PREOPEN_FD + 1);
                w.next_fd = opened + 1;
                w.fds.insert(
                    opened,
                    OpenFile {
                        file,
                        base,
                        size,
                        pos: 0,
                        cache: Vec::new(),
                        cache_at: 0,
                    },
                );
                if put(mem, out, &opened.to_le_bytes()) {
                    SUCCESS
                } else {
                    INVAL
                }
            },
        )?;
        l.func_wrap(
            M,
            "fd_read",
            |mut c: C<'_>, fd: i32, iovs: i32, count: i32, out: i32| {
                let Some((mem, w)) = parts(&mut c) else {
                    return INVAL;
                };
                let fd = fd as u32;
                let Some(file) = w.fds.get_mut(&fd) else {
                    return if fd == PREOPEN_FD { ISDIR } else { BADF };
                };
                let mut total: u32 = 0;
                for i in 0..count.max(0) as usize {
                    let at = iovs as u32 as usize + i * 8;
                    let (Some(ptr), Some(len)) = (get_u32(mem, at), get_u32(mem, at + 4)) else {
                        return INVAL;
                    };
                    let chunk = match file.read_at(file.pos, len as usize) {
                        Ok(c) => c,
                        Err(e) => {
                            eprintln!("[video] reading the file failed: {e}");
                            if total == 0 {
                                return IO;
                            }
                            break;
                        }
                    };
                    let got = chunk.len();
                    if !put(mem, ptr as i32, chunk) {
                        return INVAL;
                    }
                    file.pos += got as u64;
                    total = total.saturating_add(got as u32);
                    if got < len as usize {
                        break;
                    }
                }
                if put(mem, out, &total.to_le_bytes()) {
                    SUCCESS
                } else {
                    INVAL
                }
            },
        )?;
        l.func_wrap(
            M,
            "fd_seek",
            |mut c: C<'_>, fd: i32, offset: i64, whence: i32, out: i32| {
                let Some((mem, w)) = parts(&mut c) else {
                    return INVAL;
                };
                let fd = fd as u32;
                let Some(file) = w.fds.get_mut(&fd) else {
                    return if fd <= 2 { SPIPE } else { BADF };
                };
                let from = match whence {
                    0 => 0,
                    1 => file.pos as i64,
                    2 => file.size as i64,
                    _ => return INVAL,
                };
                let Some(pos) = from.checked_add(offset).filter(|&p| p >= 0) else {
                    return INVAL;
                };
                file.pos = pos as u64;
                if put(mem, out, &(pos as u64).to_le_bytes()) {
                    SUCCESS
                } else {
                    INVAL
                }
            },
        )?;
        l.func_wrap(
            M,
            "fd_write",
            |mut c: C<'_>, fd: i32, iovs: i32, count: i32, out: i32| {
                if fd != 1 && fd != 2 {
                    return BADF;
                }
                let Some((mem, w)) = parts(&mut c) else {
                    return INVAL;
                };
                let mut total: u32 = 0;
                for i in 0..count.max(0) as usize {
                    let at = iovs as u32 as usize + i * 8;
                    let (Some(ptr), Some(len)) = (get_u32(mem, at), get_u32(mem, at + 4)) else {
                        return INVAL;
                    };
                    // Only the start of a long write is looked at.
                    let (p, l) = (ptr as usize, (len as usize).min(MAX_LOG_LINE + 1));
                    let text =
                        String::from_utf8_lossy(mem.get(p..p.saturating_add(l)).unwrap_or(&[]))
                            .into_owned();
                    log(w, fd as usize, &text);
                    total = total.saturating_add(len);
                }
                if put(mem, out, &total.to_le_bytes()) {
                    SUCCESS
                } else {
                    INVAL
                }
            },
        )?;
        l.func_wrap(M, "fd_close", |mut c: C<'_>, fd: i32| {
            if c.data_mut().fds.remove(&(fd as u32)).is_some() {
                SUCCESS
            } else {
                BADF
            }
        })?;
        l.func_wrap(
            M,
            "fd_readdir",
            |_: C<'_>, _: i32, _: i32, _: i32, _: i64, _: i32| NOSYS,
        )?;
        l.func_wrap(
            M,
            "path_remove_directory",
            |_: C<'_>, _: i32, _: i32, _: i32| NOSYS,
        )?;
        l.func_wrap(
            M,
            "path_rename",
            |_: C<'_>, _: i32, _: i32, _: i32, _: i32, _: i32, _: i32| NOSYS,
        )?;
        l.func_wrap(M, "path_unlink_file", |_: C<'_>, _: i32, _: i32, _: i32| {
            NOSYS
        })?;
        l.func_wrap(
            M,
            "poll_oneoff",
            |_: C<'_>, _: i32, _: i32, _: i32, _: i32| NOSYS,
        )?;
        l.func_wrap(
            M,
            "proc_exit",
            |_: C<'_>, code: i32| -> wasmtime::Result<()> {
                Err(wasmtime::Error::msg(format!(
                    "the video decoder exited ({code})"
                )))
            },
        )?;
        Ok(())
    }
}

#[cfg(test)]
mod tests;
