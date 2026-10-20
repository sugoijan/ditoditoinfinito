//! Background movies on desktop (feature `video`): on macOS, checked
//! progressive 8-bit H.264, HEVC and VP9 subsets through hardware
//! VideoToolbox; an explicit Windows experimental opt-in uses Media
//! Foundation/D3D11. Windows Auto and other movies use the software
//! decoder: FFmpeg in a sandboxed process of its own, `ddi-video-helper`
//! (`video/helper`, `helper.rs`; `docs/plans/native-video.md`, section
//! 10). The OS path reuses `ddi_library::avi` and `h264` as WebCodecs
//! does; MP4/Matroska packets come from a private helper that only
//! demuxes (`packets.rs`). Both paths have bounded reads, callbacks and
//! reorder queues, copied I420 planes, exact seeks, and per-generation
//! output. Optional `video-metal` plus DDI_VIDEO_PLANES=metal keeps
//! checked macOS NV12 surfaces for the GPU instead of copying I420. A
//! separate worker owns each native session; a deadline abandons a stuck
//! one and disables native opens for the process. Refusal, failure or
//! timeout resumes that movie in the helper at the requested position,
//! omitting frames already delivered. `Backend` can force either path;
//! `DesktopVideo::decoder_status` reports the selection.
//!
//! The helper serves the shared decoder thread, which mirrors the web
//! worker call for call:
//!
//! - the helper never opens a file: it asks for the bytes it needs and
//!   the thread serves them from the one movie each handle names (a
//!   64 KiB read-ahead, as the worker reads `Blob`s), through shared
//!   memory; frames come back through the same memory, their sizes
//!   checked here before they are copied out;
//! - messages are handled one at a time, in order: open, seek, want,
//!   close, probe, and recycle (a frame's buffer given back for the next
//!   frame, so a playing movie allocates nothing per frame);
//! - frames are decoded only on `want`, so the caller (the play's
//!   [`MovieTrack`](ddi_platform::video::MovieTrack)) sets the queue's
//!   depth; a seek cancels the wants from before it (their generation),
//!   and so does the end of the movie, reported once until the next
//!   seek; 50 broken frames in a row fail the movie;
//! - a crash (the helper dying on a broken file, a call past its
//!   deadline, a reply that breaks the protocol) fails every open movie,
//!   kills the helper and the next open starts a fresh one.
//!
//! Where the helper is: `DDI_VIDEO_HELPER`, else `video/ddi-video-helper`
//! beside the executable (a package, or a build's profile folder, where
//! `desktop/build.rs` stages it). Without it, opening a movie fails with
//! the web's message: "the video decoder is not included in this build".
//!
//! [`VideoHost`] implements `ddi_platform::video::VideoBackend` (its
//! source a path), [`DesktopVideo`] `VideoDecoder`; [`show`] hands a
//! frame to the renderer as `Renderer::set_video_frame` takes it.

use std::collections::HashMap;
use std::io::{Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::{Arc, Mutex, Weak};
use std::time::{Duration, Instant};

use ddi_platform::video::{
    VideoBackend, VideoDecoder, VideoEvent, VideoFormat, VideoFrame, VideoInfo, YuvMatrix,
};
use ddi_video_protocol::{Reply, Request};

mod arena;
pub mod helper;
pub use helper::HelperInfo;

#[cfg(any(target_os = "macos", target_os = "windows"))]
mod avi;
#[cfg(any(target_os = "macos", target_os = "windows"))]
mod bits;
#[cfg(any(target_os = "macos", target_os = "windows"))]
mod codec;
#[cfg(any(target_os = "macos", target_os = "windows"))]
mod h264_format;
#[cfg(any(target_os = "macos", target_os = "windows"))]
mod hevc;
#[cfg(any(target_os = "macos", target_os = "windows"))]
mod media;
#[cfg(any(target_os = "macos", target_os = "windows"))]
mod native;
#[cfg(any(target_os = "macos", target_os = "windows"))]
mod vp9;
#[cfg(target_os = "macos")]
mod vt;

#[cfg(target_os = "windows")]
mod mf;
#[cfg(any(target_os = "windows", test))]
mod mf_core;

#[cfg(all(target_os = "macos", feature = "video-metal"))]
mod metal;
pub mod packets;
#[cfg(all(target_os = "macos", feature = "video-metal"))]
pub use metal::{MetalStats, stats as metal_stats};

/// Selects a decoder for tests and diagnostics. Auto tries the checked OS
/// subset on macOS. Windows stays on the software helper unless explicitly
/// opted in. Strict OS selections report refusals; the opt-in Auto
/// selection recovers.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Backend {
    #[default]
    Auto,
    /// The FFmpeg helper alone (`software`; `module`, its old name, is
    /// still accepted).
    Software,
    VideoToolbox,
    /// Experimental Windows decoder, reports refusals.
    MediaFoundation,
    /// Explicit Windows opt-in with module recovery; Auto stays the module.
    MediaFoundationAuto,
}
impl Backend {
    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "auto" => Some(Self::Auto),
            "software" | "module" => Some(Self::Software),
            "videotoolbox" => Some(Self::VideoToolbox),
            "mediafoundation" => Some(Self::MediaFoundation),
            "mediafoundation-auto" => Some(Self::MediaFoundationAuto),
            _ => None,
        }
    }
    fn native_on(self, os: &str) -> bool {
        match os {
            "macos" => matches!(self, Self::Auto | Self::VideoToolbox),
            "windows" => matches!(self, Self::MediaFoundation | Self::MediaFoundationAuto),
            _ => false,
        }
    }
    fn strict(self) -> bool {
        matches!(self, Self::VideoToolbox | Self::MediaFoundation)
    }
    fn unavailable_on(self, os: &str) -> bool {
        !matches!(self, Self::Auto | Self::Software) && !self.native_on(os)
    }
    pub fn diagnostic_native() -> Self {
        if cfg!(target_os = "windows") {
            Self::MediaFoundation
        } else {
            Self::VideoToolbox
        }
    }
}

/// What actually decodes one movie, and why the OS path was left.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DecoderStatus {
    pub backend: String,
    pub fallback: Option<String>,
}

/// What a missing helper says, as the web says of a missing module.
pub const NOT_INCLUDED: &str = "the video decoder is not included in this build";
/// Broken frames in a row before the movie counts as broken.
const MAX_BAD_FRAMES: u32 = 50;
/// Frame buffers kept for the next frames (`web/src/web/video.rs`).
const SPARE_KEPT: usize = 8;
/// How far ahead a read fills its window.
const READ_AHEAD: usize = 64 * 1024;
/// Decoder messages shown, at most.
const MAX_LOG_LINES: u32 = 200;
/// The longest a request to the helper may take (a frame takes
/// milliseconds; opening a movie reads its headers): past it the helper
/// is killed and the decoder counts as crashed, so a decoder spinning on
/// a broken file cannot hold the thread.
pub const CALL_DEADLINE: Duration = Duration::from_secs(10);
/// The longest decoder message kept, bytes (longer ones are cut).
const MAX_LOG_LINE: usize = 4096;
/// Where the black-bar probe samples a movie: shares of its duration, or
/// seconds when it is unknown.
const PROBE_SHARES: [f64; 6] = [0.15, 0.3, 0.45, 0.6, 0.75, 0.9];
const PROBE_SECONDS: [f64; 6] = [1.0, 3.0, 6.0, 10.0, 15.0, 20.0];

/// Where the decoder helper stands.
#[derive(Clone, Debug, PartialEq)]
pub enum HelperStatus {
    /// Not asked for yet.
    Idle,
    /// Being started (its sandbox entered and checked).
    Loading,
    Ready {
        start_ms: f64,
        /// What it runs ("FFmpeg 9.0.2").
        decoder: String,
        /// What confines it ("seatbelt", "seccomp+landlock").
        sandbox: String,
        threads: u32,
    },
    Failed(String),
}

/// The helper's file name beside the executable.
pub const HELPER_NAME: &str = "ddi-video-helper";

/// The helper's executable: `DDI_VIDEO_HELPER`, else
/// `video/ddi-video-helper` beside the executable (as a package has it)
/// or beside any folder above it (a build's `target/<profile>/video/`,
/// where `desktop/build.rs` stages it, found from a test's
/// `target/<profile>/deps/` too). No path from the build is kept in the
/// binary.
pub fn helper_path() -> Option<PathBuf> {
    if let Some(p) = std::env::var_os("DDI_VIDEO_HELPER") {
        return Some(PathBuf::from(p)).filter(|p| p.is_file());
    }
    let exe = std::env::current_exe().ok()?;
    helper_near(&exe)
}

/// [`helper_path`] for the executable at `exe`.
fn helper_near(exe: &Path) -> Option<PathBuf> {
    let name = format!("{HELPER_NAME}{}", std::env::consts::EXE_SUFFIX);
    exe.ancestors()
        .skip(1)
        .take_while(|a| !a.file_name().is_none_or(|n| n == "target"))
        .map(|d| d.join("video").join(&name))
        .find(|p| p.is_file())
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
    #[cfg(all(target_os = "macos", feature = "video-metal"))]
    if frame.external.is_some() {
        return metal::show(renderer, device, queue, id, frame);
    }
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

/// Materializes this shell's retained native surface for diagnostics or a
/// copied-plane consumer. Normal I420 frames pass through without a copy.
pub fn copy_to_cpu(frame: VideoFrame) -> Result<VideoFrame, String> {
    if frame.external.is_some() {
        #[cfg(all(target_os = "macos", feature = "video-metal"))]
        if let Some(surface) = frame
            .external
            .as_ref()
            .and_then(|e| e.0.downcast_ref::<metal::Surface>())
        {
            let started = Instant::now();
            let planes = surface.copy()?;
            let elapsed = started.elapsed().as_secs_f64() * 1000.0;
            let mut frame = frame;
            frame.decode_ms += elapsed;
            frame.planes = planes;
            frame.external = None;
            return Ok(frame);
        }
        return Err("unknown external desktop video frame".into());
    }
    Ok(frame)
}

/// Called while encoding every play frame, even when its movie is paused.
/// Its GPU completion lease also covers cached pictures used again later.
pub fn protect_external_frames(device: &wgpu::Device, encoder: &wgpu::CommandEncoder) {
    #[cfg(all(target_os = "macos", feature = "video-metal"))]
    metal::protect(device, encoder);
    #[cfg(not(all(target_os = "macos", feature = "video-metal")))]
    let _ = (device, encoder);
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

/// A sendable frame from the decoder, optionally owning an immutable
/// completed native image. The consumer wraps that image in ExternalFrame.
struct Frame {
    pts: f64,
    width: u32,
    height: u32,
    matrix: YuvMatrix,
    full_range: bool,
    planes: Vec<u8>,
    decode_ms: f64,
    #[cfg(all(target_os = "macos", feature = "video-metal"))]
    surface: Option<metal::Surface>,
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
                external: {
                    #[cfg(all(target_os = "macos", feature = "video-metal"))]
                    {
                        f.surface
                            .map(|s| ddi_platform::video::ExternalFrame(std::rc::Rc::new(s)))
                    }
                    #[cfg(not(all(target_os = "macos", feature = "video-metal")))]
                    {
                        None
                    }
                },
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
    status: HelperStatus,
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
    /// A new decoder thread, using the helper at `helper` (`None`: none,
    /// so every movie fails as on a web build without the module).
    pub fn with_helper(helper: Option<PathBuf>) -> VideoHost {
        VideoHost::with_deadline(helper, CALL_DEADLINE)
    }

    /// [`VideoHost::with_helper`], each request to the helper limited to
    /// `deadline`.
    pub fn with_deadline(helper: Option<PathBuf>, deadline: Duration) -> VideoHost {
        Self::with_backend_deadline(helper, Backend::Software, deadline)
    }

    pub fn with_backend(helper: Option<PathBuf>, backend: Backend) -> VideoHost {
        Self::with_backend_deadline(helper, backend, CALL_DEADLINE)
    }

    fn with_backend_deadline(
        helper: Option<PathBuf>,
        backend: Backend,
        deadline: Duration,
    ) -> VideoHost {
        let (tx, rx) = mpsc::channel();
        let shared = Arc::new(Mutex::new(Shared {
            status: HelperStatus::Idle,
            movies: HashMap::new(),
            probes: HashMap::new(),
        }));
        let thread_shared = Arc::clone(&shared);
        std::thread::Builder::new()
            .name("ddi-video".into())
            .spawn(move || run(rx, thread_shared, helper, deadline, backend))
            .expect("video thread");
        VideoHost(Arc::new(Inner {
            tx: Mutex::new(tx),
            shared,
            next_id: AtomicU32::new(1),
        }))
    }

    /// The process's decoder thread: the one already running (so one
    /// helper serves every movie), else a new one on [`helper_path`]; one
    /// whose helper failed is not shared.
    pub fn shared() -> VideoHost {
        let mut slot = SHARED_HOST.lock().unwrap_or_else(|e| e.into_inner());
        if let Some(inner) = slot.upgrade() {
            let host = VideoHost(inner);
            if !matches!(host.status(), HelperStatus::Failed(_)) {
                return host;
            }
        }
        let backend = std::env::var("DDI_VIDEO_DECODER")
            .ok()
            .and_then(|s| Backend::parse(&s))
            .unwrap_or_default();
        let host = VideoHost::with_backend(helper_path(), backend);
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

    pub fn status(&self) -> HelperStatus {
        self.0
            .shared
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .status
            .clone()
    }

    /// Starts the helper now, while a start prompt is up, so a movie does
    /// not wait for it.
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
                        backend: "software".into(),
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
    /// Opening (from the request to `Opened`), and the helper's start.
    pub open_ms: f64,
    pub status: Option<HelperStatus>,
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

fn frame_showing_at(pts: &[f64], t: f64) -> usize {
    pts.partition_point(|p| *p <= t + 1e-6).saturating_sub(1)
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
                    let f = match copy_to_cpu(f) {
                        Ok(f) => f,
                        Err(e) => {
                            r.failures.push(e);
                            break 'decode;
                        }
                    };
                    if let Some((last, _)) = r.frames.last()
                        && f.pts < *last
                    {
                        r.failures.push(format!(
                            "frame {} at {:.4} s is before {:.4} s",
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
        // A decoder can release several pictures at the same DTS (HEVC
        // IDR/DPB drain), and negative opening DTS clamp to zero. ExactSeek
        // holds the last picture at that time, as playback does.
        let expect = expect.map(|_| frame_showing_at(&pts, t));
        let started = Instant::now();
        v.seek(t);
        v.want(1);
        let got = next(&mut v, &mut events);
        r.seek_ms = r.seek_ms.max(started.elapsed().as_secs_f64() * 1e3);
        r.seeks += 1;
        match (got, expect) {
            (Some(VideoEvent::Frame(f)), Some(i)) => {
                let f = match copy_to_cpu(f) {
                    Ok(f) => f,
                    Err(e) => {
                        r.failures.push(e);
                        continue;
                    }
                };
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
            Some(HelperStatus::Ready {
                start_ms,
                decoder,
                sandbox,
                threads,
            }) => format!(
                "{decoder} in {sandbox} with {threads} threads, started in {start_ms:.0} ms"
            ),
            Some(s) => format!("decoder {s:?}"),
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
/// helper said) counts as a crash of the decoder, so the thread lives on
/// and later movies get a fresh helper.
fn run(
    rx: Receiver<Msg>,
    shared: Arc<Mutex<Shared>>,
    helper: Option<PathBuf>,
    deadline: Duration,
    backend: Backend,
) {
    let mut machine = Machine::new(helper, Arc::clone(&shared), deadline);
    #[cfg(any(target_os = "macos", target_os = "windows"))]
    let mut native = native::Hub::new(backend, deadline);
    machine.backend = backend;
    while let Ok(msg) = rx.recv() {
        let id = msg.id();
        let handled = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            #[cfg(any(target_os = "macos", target_os = "windows"))]
            let msg = native.handle(&mut machine, msg);
            #[cfg(not(any(target_os = "macos", target_os = "windows")))]
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
            #[cfg(any(target_os = "macos", target_os = "windows"))]
            native.crashed(&mut machine, &what);
            machine.crashed(&format!("the host failed ({what})"), id);
        }
    }
}

/// Recovery after native decode: skip only the delivered pictures in a
/// group sharing one DTS. Exact seeks are restored to their original target,
/// so the helper coalesces the same initial pictures before this count starts.
struct Resume {
    time: f64,
    remaining: usize,
}
impl Resume {
    fn skip(&mut self, t: f64) -> bool {
        if t < self.time {
            return true;
        }
        if t > self.time || self.remaining == 0 {
            return false;
        }
        self.remaining -= 1;
        true
    }
}

/// One open movie on the thread (its handle in the helper is its id).
struct Video {
    resume: Option<Resume>,
    epoch: u32,
    last: f64,
    bad: u32,
    ended: bool,
}

pub(super) struct Machine {
    backend: Backend,
    helper_path: Option<PathBuf>,
    /// Each request's deadline.
    deadline: Duration,
    threads: u32,
    /// The helper's `Ready` status once started (put back after a later
    /// failure to start is followed by a success).
    loaded: Option<HelperStatus>,
    running: Option<helper::Helper>,
    videos: HashMap<u32, Video>,
    spare: Vec<Vec<u8>>,
    shared: Arc<Mutex<Shared>>,
}

/// Anything that ends the helper's run: it died, a request passed its
/// deadline, it broke the protocol.
#[derive(Debug)]
pub struct Crash(pub(super) String);

fn unexpected(reply: Reply) -> Crash {
    Crash(format!("an unexpected reply: {reply:?}"))
}

impl Machine {
    fn new(
        helper_path: Option<PathBuf>,
        shared: Arc<Mutex<Shared>>,
        deadline: Duration,
    ) -> Machine {
        Machine {
            backend: Backend::Software,
            helper_path,
            deadline,
            threads: helper::threads(),
            loaded: None,
            running: None,
            videos: HashMap::new(),
            spare: Vec::new(),
            shared,
        }
    }

    pub(super) fn helper_path(&self) -> Option<PathBuf> {
        self.helper_path.clone()
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
        if self.backend.unavailable_on(std::env::consts::OS)
            && let Msg::Open { id, .. } = &msg
        {
            let why = "the requested native decoder is unavailable on this platform".to_string();
            self.decoder_status(
                *id,
                DecoderStatus {
                    backend: "software".into(),
                    fallback: Some(why.clone()),
                },
            );
            if self.backend.strict() {
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

    /// Starts the helper (its sandbox entered and checked, its `Hello`
    /// answered).
    fn start(&mut self) -> Result<(), String> {
        if self.running.is_some() {
            return Ok(());
        }
        let path = self
            .helper_path
            .clone()
            .ok_or_else(|| NOT_INCLUDED.to_string())?;
        self.shared().status = HelperStatus::Loading;
        let start = Instant::now();
        let helper = helper::Helper::spawn(&path, self.deadline, self.threads)?;
        let ready = HelperStatus::Ready {
            start_ms: start.elapsed().as_secs_f64() * 1e3,
            decoder: helper.info.decoder.clone(),
            sandbox: helper.info.sandbox.clone(),
            threads: helper.info.threads,
        };
        if let HelperStatus::Ready {
            start_ms,
            decoder,
            sandbox,
            threads,
        } = &ready
        {
            eprintln!(
                "[video] decoder: {decoder} in {sandbox} with {threads} threads, started in {start_ms:.0} ms"
            );
        }
        self.running = Some(helper);
        self.loaded = Some(ready.clone());
        self.shared().status = ready;
        Ok(())
    }

    /// A running helper; `false` (with `Failed` and, for `id`, an error
    /// posted) when it could not be started.
    fn ready(&mut self, id: Option<u32>) -> bool {
        match self.start() {
            Ok(()) => {
                // A failure to start before this success is over.
                let loaded = self.loaded.clone();
                let mut shared = self.shared();
                if let (HelperStatus::Failed(_), Some(ready)) = (&shared.status, loaded) {
                    shared.status = ready;
                }
                true
            }
            Err(e) => {
                self.shared().status = HelperStatus::Failed(e.clone());
                if let Some(id) = id {
                    self.post(id, None, Event::Error(e));
                }
                false
            }
        }
    }

    /// After a crash nothing the helper holds can be trusted: it is
    /// killed, every open movie fails (and `id`, the one being opened
    /// then), and the next open starts a fresh helper.
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

    fn helper(&mut self) -> &mut helper::Helper {
        self.running.as_mut().expect("helper")
    }

    /// Opens `path` in the helper as handle `id` (its bytes served from
    /// here): the `Opened` reply, or `None` when it is not a movie the
    /// decoder knows (or cannot be read).
    fn open_file(&mut self, id: u32, path: &MovieFile, name: &str) -> Result<Option<Reply>, Crash> {
        let h = self.helper();
        let Ok(size) = h.register(id, path) else {
            return Ok(None);
        };
        let reply = h.request(&Request::Open {
            id,
            size,
            name: format!("{id}{}", extension_of(name)),
        })?;
        match reply {
            Reply::Opened { id: got, .. } if got == id => Ok(Some(reply)),
            Reply::OpenFailed { id: got } if got == id => {
                h.unregister(id);
                Ok(None)
            }
            other => Err(unexpected(other)),
        }
    }

    fn close_file(&mut self, id: u32) -> Result<(), Crash> {
        let h = self.helper();
        h.unregister(id);
        match h.request(&Request::Close { id })? {
            Reply::Closed { id: got } if got == id => Ok(()),
            other => Err(unexpected(other)),
        }
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
        let Some(Reply::Opened {
            width,
            height,
            duration,
            codec,
            ..
        }) = self.open_file(id, &path, name)?
        else {
            self.post(
                id,
                None,
                Event::Error(format!(
                    "cannot open {name} (not a movie the decoder knows)"
                )),
            );
            return Ok(());
        };
        self.videos.insert(
            id,
            Video {
                resume: None,
                epoch: 0,
                last: -1.0,
                bad: 0,
                ended: false,
            },
        );
        let info = VideoInfo {
            width,
            height,
            duration: (duration >= 0.0).then_some(duration),
            codec,
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
            let h = self.helper();
            let slot = h.next_slot();
            let start = Instant::now();
            let reply = h.request(&Request::Next { id, slot })?;
            let ms = start.elapsed().as_secs_f64() * 1e3;
            match reply {
                Reply::End { id: got, last_time } if got == id => {
                    let video = self.videos.get_mut(&id).expect("video");
                    video.ended = true;
                    // The last frame of the file, even when a seek past the
                    // end decoded it without handing it over.
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
                Reply::Bad { id: got } if got == id => {
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
                }
                Reply::Frame {
                    id: got,
                    slot: in_slot,
                    pts,
                    width,
                    height,
                    colorspace,
                    full_range,
                    ..
                } if got == id => {
                    if in_slot != slot {
                        return Err(Crash("a frame in another slot than asked".into()));
                    }
                    let video = self.videos.get_mut(&id).expect("video");
                    if video.resume.as_mut().is_some_and(|r| r.skip(pts)) {
                        video.last = pts;
                        continue;
                    }
                    video.resume = None;
                    let frame = self.frame(slot, pts, width, height, colorspace, full_range, ms)?;
                    let video = self.videos.get_mut(&id).expect("video");
                    video.bad = 0;
                    video.last = pts;
                    let g = video.epoch;
                    self.post(id, Some(g), Event::Frame(frame));
                    i += 1;
                }
                other => return Err(unexpected(other)),
            }
        }
        Ok(())
    }

    /// The planes of `width × height` the helper put in `slot`, checked
    /// against the slot and copied into a spare buffer.
    #[allow(clippy::too_many_arguments)]
    fn frame(
        &mut self,
        slot: u32,
        pts: f64,
        width: u32,
        height: u32,
        colorspace: u32,
        full_range: bool,
        decode_ms: f64,
    ) -> Result<Frame, Crash> {
        let h = self.running.as_ref().expect("helper");
        let size = planes_in_slot(&h.arena.layout, slot, width, height)?;
        let range = h.arena.layout.frame(slot).expect("checked");
        let src = h.arena.slice(&(range.start..range.start + size));
        let i = self.spare.iter().position(|v| v.capacity() >= size);
        let mut planes = i.map_or_else(|| Vec::with_capacity(size), |i| self.spare.swap_remove(i));
        planes.clear();
        planes.extend_from_slice(src);
        Ok(Frame {
            pts,
            width,
            height,
            matrix: if colorspace == 709 {
                YuvMatrix::Bt709
            } else {
                YuvMatrix::Bt601
            },
            full_range,
            planes,
            decode_ms,
            #[cfg(all(target_os = "macos", feature = "video-metal"))]
            surface: None,
        })
    }

    fn seek(&mut self, id: u32, t: f64, epoch: u32) -> Result<(), Crash> {
        let Some(video) = self.videos.get_mut(&id) else {
            return Ok(());
        };
        video.epoch = epoch;
        video.resume = None;
        video.last = -1.0;
        video.bad = 0;
        video.ended = false;
        let ok = match self.helper().request(&Request::Seek { id, t })? {
            Reply::Sought { id: got, ok } if got == id => ok,
            other => return Err(unexpected(other)),
        };
        if !ok {
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
        if self.videos.remove(&id).is_none() {
            return Ok(());
        }
        if self.running.is_some() {
            self.close_file(id)?;
        }
        Ok(())
    }

    /// Samples the movie's picture for the black-bar probe: the Y planes
    /// of a few frames spread through it, each the first from a keyframe
    /// (`SeekKey`; exactly when that lands past the last keyframe).
    fn probe(&mut self, id: u32, path: MovieFile, name: &str) -> Result<(), Crash> {
        let answer = |m: &mut Machine, a: Result<Picture, String>| {
            if let Some(p) = m.shared().probes.get_mut(&id) {
                *p = Some(a);
            }
        };
        if !self.ready(None) {
            let e = match self.shared().status.clone() {
                HelperStatus::Failed(e) => e,
                _ => NOT_INCLUDED.to_string(),
            };
            answer(self, Err(e));
            return Ok(());
        }
        let Some(Reply::Opened { duration, .. }) = self.open_file(id, &path, name)? else {
            answer(
                self,
                Err(format!(
                    "cannot open {name} (not a movie the decoder knows)"
                )),
            );
            return Ok(());
        };
        let times = if duration > 0.0 {
            PROBE_SHARES.map(|s| s * duration)
        } else {
            PROBE_SECONDS
        };
        let (mut width, mut height, mut full_range) = (0, 0, false);
        let mut planes: Vec<Vec<u8>> = Vec::new();
        for t in times {
            // A seek, then the first frame that decodes (up to eight tries):
            // `Some((slot, w, h, full_range))`, `None` at the end.
            let sample =
                |m: &mut Machine, key: bool| -> Result<Option<(u32, u32, u32, bool)>, Crash> {
                    let h = m.helper();
                    let request = if key {
                        Request::SeekKey { id, t }
                    } else {
                        Request::Seek { id, t }
                    };
                    match h.request(&request)? {
                        Reply::Sought { id: got, .. } if got == id => {}
                        other => return Err(unexpected(other)),
                    }
                    for _ in 0..8 {
                        let slot = h.next_slot();
                        match h.request(&Request::Next { id, slot })? {
                            Reply::Bad { id: got } if got == id => continue,
                            Reply::End { id: got, .. } if got == id => return Ok(None),
                            Reply::Frame {
                                id: got,
                                slot: in_slot,
                                width,
                                height,
                                full_range,
                                ..
                            } if got == id && in_slot == slot => {
                                return Ok(Some((slot, width, height, full_range)));
                            }
                            other => return Err(unexpected(other)),
                        }
                    }
                    Ok(None)
                };
            let mut got = sample(self, true)?;
            if got.is_none() {
                got = sample(self, false)?;
            }
            let Some((slot, w, h, full)) = got else {
                continue;
            };
            if planes.is_empty() {
                width = w;
                height = h;
                full_range = full;
            } else if (w, h) != (width, height) {
                continue;
            }
            let helper = self.running.as_ref().expect("helper");
            planes_in_slot(&helper.arena.layout, slot, w, h)?;
            let range = helper.arena.layout.frame(slot).expect("checked");
            let y = w as usize * h as usize;
            planes.push(helper.arena.slice(&(range.start..range.start + y)).to_vec());
        }
        self.close_file(id)?;
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

/// The bytes of a `width × height` frame's planes, if a frame of that size
/// fits `slot` (the helper's word for the size is checked, never trusted).
fn planes_in_slot(
    layout: &ddi_video_protocol::Layout,
    slot: u32,
    width: u32,
    height: u32,
) -> Result<usize, Crash> {
    let range = layout
        .frame(slot)
        .ok_or_else(|| Crash("a frame in a slot that does not exist".into()))?;
    let outside = || Crash(format!("a frame of {width}x{height} outside its slot"));
    (width as usize)
        .checked_mul(height as usize)
        .filter(|p| *p > 0 && *p <= ddi_video_protocol::MAX_PIXELS)
        .ok_or_else(outside)?;
    ddi_video_protocol::planes_len(width, height)
        .filter(|n| *n <= range.len())
        .ok_or_else(outside)
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

#[cfg(test)]
mod tests;
