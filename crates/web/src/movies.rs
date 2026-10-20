//! Background movies during play (`docs/plans/video-backgrounds.md`, step
//! 4): the movies both shells share ([`ddi_session::movies`]: on each
//! background layer the current segment and the next one have a decoder,
//! driven by a [`MovieTrack`] on the movie's clock, a texture per movie
//! kept for the session), with the browser's decoders and textures.
//! Nothing here touches judging.
//!
//! Step 5: the worker starts, and fetches the module, as soon as the first
//! movie is known (the start prompt shows the download), the song's first
//! movie is probed for its black bars (the background frame), and the
//! automatic setting can turn the movies off for the rest of a play
//! ([`MovieSlots::turn_off`]).
//!
//! Step 6: H.264 AVIs play through the browser's decoder (WebCodecs,
//! `web::webcodecs`) and the rest through the module; the module is
//! fetched only for a movie that needs it, and a movie the browser fails
//! on moves to the module where it stands ([`MovieDecoder`]).

use ddi_library::backgrounds::BgSegment;
use ddi_platform::video::{
    ExternalFrame, MovieTrack, VideoBackend, VideoDecoder, VideoEvent, VideoFormat, VideoFrame,
};
use ddi_render::Renderer;
use ddi_session::movies::{AHEAD, MovieShell, MovieSlots};
use std::cell::RefCell;
use std::rc::Rc;
use web_sys::Blob;

use crate::web::video::{ModuleStatus, Picture, VideoWorker, WorkerVideo};
use crate::web::webcodecs::{self, DirectFrame, WebCodecsVideo};

struct Source {
    name: String,
    blob: Blob,
    format: VideoFormat,
    /// Played through the browser's decoder (until it fails on it).
    browser: bool,
}

/// A movie's decoder: the browser's, or the module's, which takes over
/// from the browser's where it stands when that one fails.
pub(crate) struct MovieDecoder {
    backend: Backend,
    /// Why the browser's decoder failed: the deck moves the movie.
    fallback: Option<String>,
}

enum Backend {
    Module(WorkerVideo),
    Browser(WebCodecsVideo),
}

impl MovieDecoder {
    fn kind(&self) -> &'static str {
        match self.backend {
            Backend::Module(_) => "module",
            Backend::Browser(_) => "webcodecs",
        }
    }

    /// Hands the movie to the module (the track then seeks it to where
    /// the movie stands).
    fn fall_back(&mut self, module: WorkerVideo) {
        self.backend = Backend::Module(module);
        self.fallback = None;
    }
}

impl VideoDecoder for MovieDecoder {
    fn seek(&mut self, t: f64) {
        match &mut self.backend {
            Backend::Module(d) => d.seek(t),
            Backend::Browser(d) => d.seek(t),
        }
    }

    fn want(&mut self, n: u32) {
        match &mut self.backend {
            Backend::Module(d) => d.want(n),
            Backend::Browser(d) => d.want(n),
        }
    }

    fn pending(&self) -> u32 {
        match &self.backend {
            Backend::Module(d) => d.pending(),
            Backend::Browser(d) => d.pending(),
        }
    }

    fn poll(&mut self, out: &mut Vec<VideoEvent>) {
        match &mut self.backend {
            Backend::Module(d) => d.poll(out),
            Backend::Browser(d) => {
                let mut events = Vec::new();
                d.poll(&mut events);
                for event in events {
                    match event {
                        // The deck moves the movie to the module.
                        VideoEvent::Error(e) => {
                            self.fallback.get_or_insert(e);
                        }
                        // The module reports the movie open again.
                        VideoEvent::Opened(_) if self.fallback.is_some() => {}
                        e if self.fallback.is_none() => out.push(e),
                        _ => {}
                    }
                }
            }
        }
    }
}

/// Why [`frame_at`] has no frame.
#[derive(Clone, Debug, PartialEq)]
pub(crate) enum GrabError {
    /// The movie cannot be decoded (or not in time).
    Failed(String),
    /// The decoder itself is not there now (the module did not load, the
    /// worker stopped): worth trying again later.
    Unavailable(String),
}

/// How long [`frame_at`] waits for a frame once the decoder is ready, ms
/// (then it gives up for now: a busy machine, tried again later).
const GRAB_WAIT_MS: f64 = 20_000.0;

/// A looping movie's frame is taken at once when its duration puts the
/// position this far before the end; otherwise its end is learned first.
const LOOP_MARGIN: f64 = 1.0;

/// How long [`frame_at`] keeps a frame while waiting to learn whether a
/// looping movie of unknown length ends before the position, ms.
const END_WAIT_MS: f64 = 2000.0;

/// The frame movie `blob` shows at `position` (movie seconds) when its
/// change starts, decoded as a play would draw it: through the browser's
/// decoder or the module, chosen as play chooses, and picked by a
/// [`MovieTrack`] (the first frame of the run as soon as it arrives). For
/// the song's opening still (`crate::stills`). A looping movie's end is
/// learned first when the position may be past it, so the frame is the
/// one at the wrapped position (StepMania's 0.5 s loop restart), as play
/// shows it. `worker` is kept for the next call (an import takes several;
/// it is the play's own worker when one runs, `VideoWorker::shared`).
pub(crate) async fn frame_at(
    blob: &Blob,
    name: &str,
    format: &VideoFormat,
    position: f64,
    looping: bool,
    worker: &mut Option<VideoWorker>,
) -> Result<VideoFrame, GrabError> {
    let now = crate::web::clock::PerformanceClock::now_ms;
    let mut browser = webcodecs::may_play(format);
    loop {
        let backend = if browser && webcodecs::available() {
            Backend::Browser(WebCodecsVideo::open(blob.clone()))
        } else {
            if worker
                .as_ref()
                .is_some_and(|w| matches!(w.status(), ModuleStatus::Failed(_)))
            {
                *worker = None;
            }
            let w = match worker {
                Some(w) => w,
                None => worker.insert(VideoWorker::shared().map_err(GrabError::Unavailable)?),
            };
            Backend::Module(w.open(blob.clone(), name, format))
        };
        let module = matches!(backend, Backend::Module(_));
        let mut decoder = MovieDecoder {
            backend,
            fallback: None,
        };
        let mut track = MovieTrack::new(looping, AHEAD, None);
        track.start(&mut decoder, position);
        let mut since = now();
        // A looping movie: its end, once learned, and its duration.
        let mut end: Option<f64> = None;
        let mut duration: Option<f64> = None;
        let mut held: Option<(VideoFrame, f64)> = None;
        loop {
            let frame = track.update(&mut decoder, position);
            if !looping || end.is_some() {
                if let Some(frame) = frame {
                    return Ok(frame);
                }
            } else {
                duration = duration.or(track.info().and_then(|i| i.duration));
                if let Some(e) = track.end() {
                    // The end is known: the position wraps from here.
                    end = Some(e);
                    track = MovieTrack::new(looping, AHEAD, end);
                    track.start(&mut decoder, position);
                    held = None;
                    continue;
                }
                let within = duration.is_some_and(|d| position + LOOP_MARGIN < d);
                if let Some(frame) = frame {
                    if within {
                        return Ok(frame);
                    }
                    held = Some((frame, now()));
                }
                // The length is not known and no end came: the position
                // was within the movie.
                if held
                    .as_ref()
                    .is_some_and(|(_, at)| now() - at > END_WAIT_MS)
                {
                    return Ok(held.take().expect("checked").0);
                }
            }
            let status = worker.as_ref().filter(|_| module).map(VideoWorker::status);
            if let Some(ModuleStatus::Failed(e)) = status {
                return Err(GrabError::Unavailable(e));
            }
            if let Some(e) = track.error() {
                return Err(GrabError::Failed(e.to_string()));
            }
            if decoder.fallback.is_some() {
                // The module takes over, as in play.
                browser = false;
                break;
            }
            // The wait counts once the module is there.
            if matches!(
                status,
                Some(ModuleStatus::Idle | ModuleStatus::Loading { .. })
            ) {
                since = now();
            }
            if now() - since > GRAB_WAIT_MS {
                return Err(GrabError::Unavailable("no frame came in time".into()));
            }
            gloo::timers::future::TimeoutFuture::new(10).await;
        }
    }
}

/// Where the browser's probe leaves its answer.
type ProbeAnswer = Rc<RefCell<Option<Result<Picture, String>>>>;

/// The black-bar probe of the song's first movie.
struct Probe {
    name: String,
    /// Sampled by the browser's decoder: its answer, once it came.
    browser: Option<ProbeAnswer>,
    /// Its id in the worker, once sent (the file may still be loading).
    id: Option<u32>,
    answer: Option<Result<Picture, String>>,
}

/// A movie's textures: the one [`MovieSlots`] shows is the one written
/// last.
struct Planes {
    name: String,
    /// Planes for decoded frames (renderer id).
    yuv: Option<usize>,
    /// An RGBA texture for the browser's own frames (the direct path):
    /// the texture, its renderer id and size.
    rgb: Option<(wgpu::Texture, usize, u32, u32)>,
}

/// What the web brings to the shared movies: the song's files and the
/// decoders that open them (the browser's, or the module's worker).
#[derive(Default)]
pub(crate) struct Decoders {
    worker: Option<VideoWorker>,
    sources: Vec<Source>,
}

impl Decoders {
    /// The session's worker, started (and told to fetch the module) when
    /// there is none.
    fn worker(&mut self) -> Result<&mut VideoWorker, String> {
        if self.worker.is_none() {
            let worker = VideoWorker::shared()?;
            worker.preload();
            self.worker = Some(worker);
        }
        Ok(self.worker.as_mut().expect("just set"))
    }

    /// Movie `name` opened in the video decoder's worker; `Err` (the movie
    /// fails) when there is no worker or it stopped (it answers nothing
    /// then).
    fn open_module(
        &mut self,
        blob: Blob,
        name: &str,
        format: &VideoFormat,
    ) -> Result<WorkerVideo, String> {
        let worker = self.worker()?;
        match worker.status() {
            ModuleStatus::Failed(e) => Err(e),
            _ => Ok(worker.open(blob, name, format)),
        }
    }
}

impl MovieShell for Decoders {
    type Decoder = MovieDecoder;

    fn open(&mut self, name: &str) -> Option<Result<MovieDecoder, String>> {
        // Not loaded yet: tried again next frame.
        let source = self.sources.iter().find(|s| s.name == name)?;
        let (blob, format, browser) = (source.blob.clone(), source.format.clone(), source.browser);
        // The browser path may have been turned off since (RGB frames).
        let backend = if browser && webcodecs::available() {
            Backend::Browser(WebCodecsVideo::open(blob))
        } else {
            match self.open_module(blob, name, &format) {
                Ok(module) => Backend::Module(module),
                Err(e) => return Some(Err(e)),
            }
        };
        Some(Ok(MovieDecoder {
            backend,
            fallback: None,
        }))
    }

    /// Moves a movie the browser's decoder failed on to the module, and
    /// its later openings too.
    fn fall_back(&mut self, name: &str, decoder: &mut MovieDecoder) -> Result<bool, String> {
        let Some(reason) = decoder.fallback.clone() else {
            return Ok(false);
        };
        web_sys::console::warn_1(
            &format!("background video {name}: the browser's decoder failed ({reason}); the video decoder takes over").into(),
        );
        let Some(source) = self.sources.iter_mut().find(|s| s.name == name) else {
            return Ok(false);
        };
        source.browser = false;
        let (blob, format) = (source.blob.clone(), source.format.clone());
        decoder.fall_back(self.open_module(blob, name, &format)?);
        Ok(true)
    }

    /// A worker that stopped is started again for a retry.
    fn stopped(&mut self) {
        if self
            .worker
            .as_ref()
            .is_some_and(|w| matches!(w.status(), ModuleStatus::Failed(_)))
        {
            self.worker = None;
        }
    }

    fn warn(&self, line: &str) {
        web_sys::console::warn_1(&line.into());
    }

    fn info(&self, line: &str) {
        web_sys::console::info_1(&line.into());
    }
}

/// The song's movies: [`MovieSlots`]' state and rules (through `Deref`),
/// with the probe, the textures and the browser frames being copied.
#[derive(Default)]
pub(crate) struct MovieDeck {
    slots: MovieSlots<Decoders>,
    /// The movie probed for the background frame.
    probe: Option<Probe>,
    planes: Vec<Planes>,
    /// The browser frames copied to the GPU last frame: kept open until
    /// that frame's work is submitted (WebGL2 copies then, as with the
    /// background images).
    kept: Vec<ExternalFrame>,
    /// Copies of browser frames still checked for an error, and the first
    /// error one reported.
    checks: u32,
    direct_error: Rc<RefCell<Option<String>>>,
}

impl std::ops::Deref for MovieDeck {
    type Target = MovieSlots<Decoders>;

    fn deref(&self) -> &Self::Target {
        &self.slots
    }
}

impl std::ops::DerefMut for MovieDeck {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.slots
    }
}

impl MovieDeck {
    /// A movie of the song, as the import kept it; `probe`: the song's
    /// first movie, whose picture sets the background frame. The worker
    /// starts fetching the module now.
    pub(crate) fn add_source(
        &mut self,
        name: String,
        blob: Blob,
        format: VideoFormat,
        probe: bool,
    ) {
        let decoders = self.slots.shell_mut();
        if decoders.sources.iter().any(|s| s.name == name) {
            return;
        }
        let browser = webcodecs::may_play(&format);
        // The module only for a movie that needs it.
        let worker = browser || decoders.worker().is_ok();
        if worker && probe && self.probe.is_none() {
            let answer = browser.then(|| {
                let answer: ProbeAnswer = Rc::default();
                let (cell, blob) = (Rc::clone(&answer), blob.clone());
                wasm_bindgen_futures::spawn_local(async move {
                    let got = webcodecs::probe(&blob).await;
                    *cell.borrow_mut() = Some(got);
                });
                answer
            });
            self.probe = Some(Probe {
                name: name.clone(),
                browser: answer,
                id: None,
                answer: None,
            });
        }
        decoders.sources.push(Source {
            name,
            blob,
            format,
            browser,
        });
        self.send_probe();
    }

    fn send_probe(&mut self) {
        let Some(probe) = self
            .probe
            .as_mut()
            .filter(|p| p.id.is_none() && p.browser.is_none())
        else {
            return;
        };
        let decoders = self.slots.shell();
        let (Some(source), Some(worker)) = (
            decoders.sources.iter().find(|s| s.name == probe.name),
            decoders.worker.as_ref(),
        ) else {
            return;
        };
        probe.id = Some(worker.probe(&source.blob, &source.name));
    }

    /// The probed movie's picture shape (width over height, its own black
    /// bars left out): `Err(true)` while the probe runs, `Err(false)` when
    /// there is none or it failed.
    pub(crate) fn picture_aspect(&mut self) -> Result<f32, bool> {
        let Some(probe) = self.probe.as_mut() else {
            return Err(false);
        };
        let browser_answer = probe.browser.as_ref().map(|cell| cell.borrow_mut().take());
        match browser_answer {
            None => {}
            Some(None) => return Err(true),
            Some(Some(Ok(p))) => {
                probe.answer = Some(Ok(p));
                probe.browser = None;
            }
            // The module samples it instead.
            Some(Some(Err(e))) => {
                web_sys::console::info_1(
                    &format!("background video {}: browser probe failed: {e}", probe.name).into(),
                );
                probe.browser = None;
                if self.slots.shell_mut().worker().is_ok() {
                    self.send_probe();
                }
                return Err(self.probe.as_ref().is_some_and(|p| p.id.is_some()));
            }
        }
        let Some(probe) = self.probe.as_mut() else {
            return Err(false);
        };
        if probe.answer.is_none() {
            let Some(id) = probe.id else {
                return Err(true);
            };
            probe.answer = self
                .slots
                .shell()
                .worker
                .as_ref()
                .and_then(|w| w.probe_result(id));
            if let Some(a) = &probe.answer {
                let line = match a {
                    Ok(p) => format!("background video {}: picture {p:?}", probe.name),
                    Err(e) => format!("background video {}: probe failed: {e}", probe.name),
                };
                web_sys::console::info_1(&line.into());
            }
        }
        match &probe.answer {
            None => Err(true),
            Some(Ok(p)) => p.aspect().ok_or(false),
            Some(Err(_)) => Err(false),
        }
    }

    /// Where the module stands, once a movie asked for it.
    pub(crate) fn module_status(&self) -> Option<ModuleStatus> {
        self.slots.shell().worker.as_ref().map(VideoWorker::status)
    }

    /// Closes every decoder (the song ended): [`MovieSlots::stop`]. Sources,
    /// textures and what was learned stay for a retry (with a new worker
    /// if the old one stopped).
    pub(crate) fn stop(&mut self) {
        // Their copies were submitted with the frame before.
        self.kept.clear();
        self.slots.stop();
    }

    /// Stops for good (the session is over): the next play starts with the
    /// movies on.
    pub(crate) fn reset(&mut self) {
        self.kept.clear();
        self.slots.reset();
    }

    /// Opens, feeds and closes decoders for song second `t` and uploads the
    /// frames due; `layers` the background layers' schedules.
    pub(crate) fn update(
        &mut self,
        layers: &[Vec<BgSegment>],
        t: f64,
        renderer: &mut Renderer,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
    ) {
        // Last frame's work is submitted: its browser frames may close.
        self.kept.clear();
        if self.slots.shell().sources.is_empty() || self.slots.idle(layers) {
            return;
        }
        // A browser frame could not be drawn: the module plays every movie.
        let direct_error = self.direct_error.borrow_mut().take();
        if let Some(e) = direct_error {
            webcodecs::direct_failed(&e);
            for decoder in self.slots.decoders_mut() {
                if matches!(decoder.backend, Backend::Browser(_)) {
                    decoder.fallback.get_or_insert_with(|| e.clone());
                }
            }
        }
        let due = self.slots.update(layers, t);
        for (name, width, height, external) in due.early {
            self.texture(&name, width, height, external, renderer, device);
        }
        for (name, frame) in due.show {
            self.upload(&name, &frame, renderer, device, queue);
            crate::web::video::recycle(frame.planes);
        }
    }

    /// The texture of movie `name`, made for a `width × height` frame when
    /// it has none; `None` for a frame too large for the GPU.
    fn texture(
        &mut self,
        name: &str,
        width: u32,
        height: u32,
        external: bool,
        renderer: &mut Renderer,
        device: &wgpu::Device,
    ) -> Option<usize> {
        let limit = device.limits().max_texture_dimension_2d;
        if !self.slots.fits(name, width, height, limit) {
            return None;
        }
        let i = match self.planes.iter().position(|t| t.name == name) {
            Some(i) => i,
            None => {
                self.planes.push(Planes {
                    name: name.to_string(),
                    yuv: None,
                    rgb: None,
                });
                self.planes.len() - 1
            }
        };
        let t = &mut self.planes[i];
        let id = if external {
            if !t
                .rgb
                .as_ref()
                .is_some_and(|r| (r.2, r.3) == (width, height))
            {
                let texture = renderer.create_background_texture(device, width, height);
                let id = renderer.add_background(device, &texture);
                t.rgb = Some((texture, id, width, height));
            }
            t.rgb.as_ref().map(|r| r.1)
        } else {
            Some(
                *t.yuv
                    .get_or_insert_with(|| renderer.add_video(device, width, height)),
            )
        }?;
        self.slots.set_texture(name, id);
        Some(id)
    }

    fn upload(
        &mut self,
        name: &str,
        frame: &VideoFrame,
        renderer: &mut Renderer,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
    ) {
        if let Some(external) = &frame.external {
            self.upload_direct(name, frame, external, renderer, device, queue);
            return;
        }
        // A malformed frame never reaches a texture (it would show green:
        // new textures are zeroed).
        if frame.split().is_none() {
            self.slots.fail(name, "a malformed frame".into());
            return;
        }
        let Some(id) = self.texture(name, frame.width, frame.height, false, renderer, device)
        else {
            return;
        };
        let written = renderer.set_video_frame(
            device,
            queue,
            id,
            frame.width,
            frame.height,
            &frame.planes,
            frame.matrix == ddi_platform::video::YuvMatrix::Bt709,
            frame.full_range,
        );
        if written {
            self.slots.shown(name);
        }
    }

    /// Copies a browser frame (the direct path) into the movie's RGBA
    /// texture, GPU to GPU, in the browser's colors. The first copies are
    /// checked for errors: a browser whose frames cannot be copied moves
    /// every movie to the module.
    fn upload_direct(
        &mut self,
        name: &str,
        frame: &VideoFrame,
        external: &ExternalFrame,
        renderer: &mut Renderer,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
    ) {
        let Some(direct) = external.0.downcast_ref::<DirectFrame>() else {
            self.slots.fail(name, "a frame of another kind".into());
            return;
        };
        if self
            .texture(name, frame.width, frame.height, true, renderer, device)
            .is_none()
        {
            return;
        }
        let Some((texture, ..)) = self
            .planes
            .iter()
            .find(|t| t.name == name)
            .and_then(|t| t.rgb.as_ref())
        else {
            return;
        };
        let scope =
            (self.checks < 4).then(|| device.push_error_scope(wgpu::ErrorFilter::Validation));
        crate::web::gfx::copy_bitmap(queue, direct.bitmap(), texture, frame.width, frame.height);
        renderer.background_texture_changed();
        if let Some(scope) = scope {
            self.checks += 1;
            let error = Rc::clone(&self.direct_error);
            let popped = scope.pop();
            wasm_bindgen_futures::spawn_local(async move {
                if let Some(e) = popped.await {
                    error.borrow_mut().get_or_insert(e.to_string());
                }
            });
        }
        self.kept.push(external.clone());
        self.slots.shown(name);
    }

    /// For `window.__DDI_DEBUG.video`.
    pub(crate) fn debug(&self) -> js_sys::Object {
        let obj = js_sys::Object::new();
        let set = |k: &str, v: wasm_bindgen::JsValue| {
            let _ = js_sys::Reflect::set(&obj, &k.into(), &v);
        };
        let s = self.stats();
        set("decoded", (s.decoded as f64).into());
        set("shown", (s.shown as f64).into());
        set("dropped", (s.dropped as f64).into());
        set("late", (s.late as f64).into());
        set("seeks", (s.seeks as f64).into());
        set("open", (self.opened().len() as f64).into());
        set("paused", self.paused().into());
        let playing = js_sys::Array::new();
        let decoders = js_sys::Array::new();
        let layers = js_sys::Array::new();
        for open in self.opened() {
            playing.push(&open.name.clone().into());
            decoders.push(&open.decoder.kind().into());
            layers.push(&(open.layer as f64 + 1.0).into());
        }
        set("playing", playing.into());
        set("decoders", decoders.into());
        // The background layer each plays on (1 or 2).
        set("layers", layers.into());
        set("browser_frames", webcodecs::mode_name().into());
        let failed = js_sys::Array::new();
        for (name, e) in self.failed() {
            failed.push(&format!("{name}: {e}").into());
        }
        set("failed", failed.into());
        let status = match self.module_status() {
            None | Some(ModuleStatus::Idle) => "idle".to_string(),
            Some(ModuleStatus::Loading { loaded, total }) => format!("loading {loaded}/{total}"),
            Some(ModuleStatus::Ready { .. }) => "ready".to_string(),
            Some(ModuleStatus::Failed(e)) => format!("failed: {e}"),
        };
        set("module", status.into());
        obj
    }
}
