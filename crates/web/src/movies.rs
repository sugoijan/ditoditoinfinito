//! Background movies during play (`docs/plans/video-backgrounds.md`, step
//! 4). Opens the decoder of each movie segment ahead of it (on each
//! background layer the current segment and the next one: two at most per
//! layer), drives it with a [`MovieTrack`] on the movie's clock
//! ([`backgrounds::movie_position_in`], from the same song time the notes
//! are drawn at, shared by both layers as StepMania shares a movie's
//! texture) and uploads at most one frame per layer and display frame. Each movie draws into its own texture, kept for the session, so
//! a movie that fades out or resumes later keeps its last frame, as
//! StepMania pauses it; a decoder moves on to its movie's next segment
//! instead of opening the file again. Nothing here touches judging.
//!
//! Step 5: the worker starts, and fetches the module, as soon as the first
//! movie is known (the start prompt shows the download), the song's first
//! movie is probed for its black bars (the background frame), and the
//! automatic setting can turn the movies off for the rest of a play
//! ([`MovieDeck::turn_off`]).
//!
//! Step 6: H.264 AVIs play through the browser's decoder (WebCodecs,
//! `web::webcodecs`) and the rest through the module; the module is
//! fetched only for a movie that needs it, and a movie the browser fails
//! on moves to the module where it stands ([`MovieDecoder`]).

use ddi_library::backgrounds::{self, BgImage, BgSegment, LAYERS};
use ddi_platform::video::{
    ExternalFrame, MovieStats, MovieTrack, StopReason, VideoBackend, VideoDecoder, VideoEvent,
    VideoFormat, VideoFrame,
};
use ddi_render::Renderer;
use std::cell::RefCell;
use std::rc::Rc;
use web_sys::Blob;

use crate::web::video::{ModuleStatus, Picture, VideoWorker, WorkerVideo};
use crate::web::webcodecs::{self, DirectFrame, WebCodecsVideo};

/// A movie is opened this long before its segment starts: opening and
/// seeking a long-GOP H.264 file took up to 1.2 s (step 1).
const LEAD_SECONDS: f64 = 2.0;

/// Frames kept decoded or asked for, per movie.
const AHEAD: u32 = 3;

/// A decoder moving to another segment of its movie seeks unless the
/// movie runs on from where it was, within this (a segment following
/// another of the same file, a cut fade, the other layer taking it over).
const CONTINUOUS_SECONDS: f64 = 0.1;

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

struct Open {
    /// The background layer and the segment of its schedule it plays.
    layer: usize,
    segment: usize,
    /// The movie position it was last given.
    position: f64,
    name: String,
    decoder: MovieDecoder,
    track: MovieTrack,
    /// A frame that came due before its segment started (prefetch): shown
    /// when the segment starts, so the change never waits for a decode.
    held: Option<VideoFrame>,
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

/// A movie's texture.
struct Texture {
    name: String,
    /// The texture shown: `yuv` or `rgb`'s, the one written last.
    id: usize,
    /// Planes for decoded frames (renderer id).
    yuv: Option<usize>,
    /// An RGBA texture for the browser's own frames (the direct path):
    /// the texture, its renderer id and size.
    rgb: Option<(wgpu::Texture, usize, u32, u32)>,
    /// It holds a frame of the current run: before that (a retry, a change
    /// that starts the movie over), the song background shows instead of
    /// an old frame.
    fresh: bool,
}

#[derive(Default)]
pub(crate) struct MovieDeck {
    worker: Option<VideoWorker>,
    sources: Vec<Source>,
    open: Vec<Open>,
    textures: Vec<Texture>,
    /// Each movie's last frame time, once a track learned it, for the
    /// movie's later tracks (a resume past the end wraps at once).
    ends: Vec<(String, f64)>,
    /// Movies that could not play this session: the song background shows.
    failed: Vec<(String, String)>,
    /// The segment current on each layer at the last update.
    current: [Option<usize>; LAYERS],
    /// The drawn movie the automatic rule watches, `(layer, segment)`.
    guard_key: std::cell::Cell<Option<(usize, usize)>>,
    /// Counts of closed tracks, plus the open ones for [`MovieDeck::stats`].
    closed: MovieStats,
    /// Changes whenever [`MovieDeck::shown_ids`] would.
    version: u64,
    /// The movie probed for the background frame.
    probe: Option<Probe>,
    /// A frame of a movie has been shown this session.
    shown_any: bool,
    /// Turned off for the rest of this play by the automatic setting.
    off: Option<StopReason>,
    /// Paused by the automatic setting's trial: nothing decodes or goes
    /// up, the frame shown stays.
    paused: bool,
    /// Movies that failed this play, for the results (`failed` is cleared
    /// when the song ends).
    trouble: Vec<(String, String)>,
    /// The browser frames copied to the GPU last frame: kept open until
    /// that frame's work is submitted (WebGL2 copies then, as with the
    /// background images).
    kept: Vec<ExternalFrame>,
    /// Copies of browser frames still checked for an error, and the first
    /// error one reported.
    checks: u32,
    direct_error: Rc<RefCell<Option<String>>>,
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
        if self.sources.iter().any(|s| s.name == name) {
            return;
        }
        let browser = webcodecs::may_play(&format);
        // The module only for a movie that needs it.
        let worker = browser || self.worker().is_ok();
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
        self.sources.push(Source {
            name,
            blob,
            format,
            browser,
        });
        self.send_probe();
    }

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

    fn send_probe(&mut self) {
        let Some(probe) = self
            .probe
            .as_mut()
            .filter(|p| p.id.is_none() && p.browser.is_none())
        else {
            return;
        };
        let (Some(source), Some(worker)) = (
            self.sources.iter().find(|s| s.name == probe.name),
            self.worker.as_ref(),
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
                if self.worker().is_ok() {
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
            probe.answer = self.worker.as_ref().and_then(|w| w.probe_result(id));
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

    /// A movie frame has been shown this session (the background frame is
    /// settled from then on).
    pub(crate) fn shown_any(&self) -> bool {
        self.shown_any
    }

    /// Where the module stands, once a movie asked for it.
    pub(crate) fn module_status(&self) -> Option<ModuleStatus> {
        self.worker.as_ref().map(VideoWorker::status)
    }

    /// The movie being drawn now (its texture holds a frame of this run),
    /// for the automatic setting's rule, as `segment * LAYERS + layer`:
    /// while the one it named is still drawn it stays, so a change on the
    /// other layer does not start the rule's warm-up again.
    pub(crate) fn drawing(&self, layers: &[Vec<BgSegment>]) -> Option<usize> {
        backgrounds::drawing_movie(layers, &self.current, &self.guard_key, |name| {
            // Shown (`MovieDeck::shown_ids`).
            self.textures.iter().any(|t| {
                t.fresh && t.name == name && !self.failed.iter().any(|(n, _)| *n == t.name)
            })
        })
    }

    /// Stops every movie for the rest of this play: the song's own
    /// background shows (not the opening still, [`MovieDeck::opening_plays`]);
    /// a retry plays them again.
    pub(crate) fn turn_off(&mut self, reason: StopReason) {
        if self.off.is_some() {
            return;
        }
        web_sys::console::warn_1(
            &format!("background videos turned off for this play: {reason:?}").into(),
        );
        self.off = Some(reason);
        self.paused = false;
        for open in std::mem::take(&mut self.open) {
            self.close(open);
        }
        for texture in &mut self.textures {
            texture.fresh = false;
        }
        self.version += 1;
    }

    /// Pauses every movie (the automatic setting's trial: does the display
    /// keep up without them?): decoders close, the frames shown stay.
    pub(crate) fn pause(&mut self) {
        if self.paused || self.off.is_some() {
            return;
        }
        web_sys::console::info_1(
            &"background videos paused: does the display keep up without them?".into(),
        );
        self.paused = true;
        for open in std::mem::take(&mut self.open) {
            self.close(open);
        }
    }

    /// Paused by the automatic setting's trial.
    pub(crate) fn paused(&self) -> bool {
        self.paused
    }

    /// Plays on after a trial: the movies open again where they stand.
    pub(crate) fn resume(&mut self) {
        if self.paused {
            web_sys::console::info_1(
                &"background videos play on: the display fell behind without them too".into(),
            );
        }
        self.paused = false;
    }

    /// Whether the song's opening movie (schedule segment 1) is expected to
    /// play this play: the opening still stands in for it until its first
    /// frame only then; off, failed or turned off, the song background
    /// shows, as StepMania would.
    pub(crate) fn opening_plays(&self, schedule: &[BgSegment]) -> bool {
        backgrounds::opening_plays(schedule, self.off.is_some(), &self.failed, &self.trouble)
    }

    /// Why the movies were turned off for this play, if they were.
    pub(crate) fn off(&self) -> Option<StopReason> {
        self.off
    }

    /// What kept movies from playing this play, in a line for the results
    /// (a build without the module says so once, not per movie).
    pub(crate) fn trouble(&self) -> Option<String> {
        ddi_platform::video::trouble_line(&self.trouble)
    }

    /// Closes every decoder (the song ended). Sources, textures and what
    /// was learned stay for a retry, which shows no old frame and tries
    /// failed movies again (with a new worker if the old one stopped);
    /// why the movies were turned off stays for the results.
    pub(crate) fn stop(&mut self) {
        for open in self.open.drain(..) {
            self.closed.add(&open.track.stats);
        }
        for texture in &mut self.textures {
            texture.fresh = false;
        }
        // Their copies were submitted with the frame before.
        self.kept.clear();
        self.failed.clear();
        self.current = [None; LAYERS];
        self.guard_key.set(None);
        self.paused = false;
        if self
            .worker
            .as_ref()
            .is_some_and(|w| matches!(w.status(), ModuleStatus::Failed(_)))
        {
            self.worker = None;
        }
        self.version += 1;
    }

    /// Stops for good (the session is over): the next play starts with the
    /// movies on.
    pub(crate) fn reset(&mut self) {
        self.stop();
        self.off = None;
        self.trouble.clear();
    }

    /// Changes whenever [`MovieDeck::shown_ids`] would.
    pub(crate) fn version(&self) -> u64 {
        self.version
    }

    /// The movie textures that hold a frame of this run, for
    /// [`backgrounds::shown`]: before its first frame, or once it failed, a
    /// movie shows the song background.
    pub(crate) fn shown_ids(&self) -> impl Iterator<Item = (BgImage, usize)> + '_ {
        self.textures
            .iter()
            .filter(|t| t.fresh && !self.failed.iter().any(|(n, _)| *n == t.name))
            .map(|t| (BgImage::Movie(t.name.clone()), t.id))
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
        if layers.iter().all(Vec::is_empty)
            || self.sources.is_empty()
            || self.off.is_some()
            || self.paused
        {
            return;
        }
        // A browser frame could not be drawn: the module plays every movie.
        let direct_error = self.direct_error.borrow_mut().take();
        if let Some(e) = direct_error {
            webcodecs::direct_failed(&e);
            for open in &mut self.open {
                if matches!(open.decoder.backend, Backend::Browser(_)) {
                    open.decoder.fallback.get_or_insert_with(|| e.clone());
                }
            }
        }
        // Each layer's current segment, and the movie segments to decode
        // (a file current on both layers once).
        let movies = backgrounds::movie_segments_at(layers, t, LEAD_SECONDS);
        let currents = movies.currents;
        let wanted = movies.wanted;
        for (layer, current) in currents.iter().enumerate() {
            let Some(current) = *current else {
                continue;
            };
            if self.current[layer] != Some(current) {
                self.current[layer] = Some(current);
                // A movie that starts over shows nothing old meanwhile.
                let seg = &layers[layer][current];
                if let BgImage::Movie(name) = &seg.image
                    && seg.effect.restarts()
                {
                    self.set_fresh(name, false);
                }
            }
        }
        // A decoder whose segment is no longer wanted moves to its movie's
        // wanted segment (seeking only where the movie does not run on from
        // where it was); the others close (a movie fading out stays on its
        // last frame in its texture).
        for &(layer, segment) in &wanted {
            if self
                .open
                .iter()
                .any(|o| (o.layer, o.segment) == (layer, segment))
            {
                continue;
            }
            let BgImage::Movie(name) = &layers[layer][segment].image else {
                continue;
            };
            if let Some(open) = self
                .open
                .iter_mut()
                .find(|o| o.name == *name && !wanted.contains(&(o.layer, o.segment)))
            {
                open.layer = layer;
                open.segment = segment;
                let at = t.max(layers[layer][segment].seconds);
                let position = backgrounds::movie_position_in(layers, layer, segment, at);
                if (position - open.position).abs() > CONTINUOUS_SECONDS {
                    open.held = None;
                    open.track.start(&mut open.decoder, position);
                }
            }
        }
        let mut i = 0;
        while i < self.open.len() {
            if wanted.contains(&(self.open[i].layer, self.open[i].segment)) {
                i += 1;
            } else {
                let open = self.open.remove(i);
                self.close(open);
            }
        }
        for &(layer, segment) in &wanted {
            if !self
                .open
                .iter()
                .any(|o| (o.layer, o.segment) == (layer, segment))
            {
                self.open_segment(layers, layer, segment, t);
            }
        }
        let mut uploads = Vec::new();
        let mut early = Vec::new();
        let mut errors = Vec::new();
        let mut moved = Vec::new();
        for (i, open) in self.open.iter_mut().enumerate() {
            let position = backgrounds::movie_position_in(layers, open.layer, open.segment, t);
            // A change on either layer may stop it looping mid-run.
            open.track.set_looping(backgrounds::movie_loops_in(
                layers,
                open.layer,
                open.segment,
                t,
            ));
            open.position = position;
            let frame = open.track.update(&mut open.decoder, position);
            if open.decoder.fallback.is_some() {
                moved.push(i);
            }
            if let Some(e) = open.track.error() {
                errors.push((open.name.clone(), e.to_string()));
                continue;
            }
            if currents[open.layer] == Some(open.segment) {
                if let Some(frame) = frame.or_else(|| open.held.take()) {
                    uploads.push((open.name.clone(), frame));
                }
            } else if let Some(frame) = frame {
                // Its texture is made now, not on the frame of the change.
                early.push((
                    open.name.clone(),
                    frame.width,
                    frame.height,
                    frame.external.is_some(),
                ));
                open.held = Some(frame);
            }
        }
        for i in moved {
            self.fall_back(i, layers, t);
        }
        for (name, e) in errors {
            self.fail(&name, e);
        }
        let failed = std::mem::take(&mut self.failed);
        let (keep, gone): (Vec<_>, Vec<_>) = std::mem::take(&mut self.open)
            .into_iter()
            .partition(|o| !failed.iter().any(|(n, _)| *n == o.name));
        self.failed = failed;
        self.open = keep;
        for open in gone {
            self.close(open);
        }
        for (name, width, height, external) in early {
            self.texture(&name, width, height, external, renderer, device);
        }
        for (name, frame) in uploads {
            self.upload(&name, &frame, renderer, device, queue);
            crate::web::video::recycle(frame.planes);
        }
    }

    fn close(&mut self, open: Open) {
        self.closed.add(&open.track.stats);
        if let Some(end) = open.track.end() {
            self.learn_end(&open.name, end);
        }
    }

    fn learn_end(&mut self, name: &str, end: f64) {
        match self.ends.iter_mut().find(|(n, _)| n == name) {
            Some((_, e)) => *e = end,
            None => self.ends.push((name.to_string(), end)),
        }
    }

    fn fail(&mut self, name: &str, e: String) {
        if self.failed.iter().any(|(n, _)| n == name) {
            return;
        }
        web_sys::console::warn_1(
            &format!("background video {name}: {e}; the song background shows").into(),
        );
        if !self.trouble.iter().any(|(n, _)| n == name) {
            self.trouble.push((name.to_string(), e.clone()));
        }
        self.failed.push((name.to_string(), e));
        self.version += 1;
    }

    /// Moves open movie `i` from the browser's decoder to the module, and
    /// its later openings too, seeking it to where the movie stands at
    /// song second `t`.
    fn fall_back(&mut self, i: usize, layers: &[Vec<BgSegment>], t: f64) {
        let open = &self.open[i];
        let name = open.name.clone();
        let reason = open.decoder.fallback.clone().unwrap_or_default();
        web_sys::console::warn_1(
            &format!("background video {name}: the browser's decoder failed ({reason}); the video decoder takes over").into(),
        );
        let Some(source) = self.sources.iter_mut().find(|s| s.name == name) else {
            return;
        };
        source.browser = false;
        let (blob, format) = (source.blob.clone(), source.format.clone());
        let Some(module) = self.open_module(blob, &name, &format) else {
            return;
        };
        let open = &mut self.open[i];
        open.decoder.fall_back(module);
        open.held = None;
        let (layer, segment) = (open.layer, open.segment);
        let at = t.max(layers[layer][segment].seconds);
        open.track.start(
            &mut open.decoder,
            backgrounds::movie_position_in(layers, layer, segment, at),
        );
    }

    /// Movie `name` opened in the video decoder's worker; `None`, the
    /// movie failed, when there is no worker or it stopped (it answers
    /// nothing then).
    fn open_module(&mut self, blob: Blob, name: &str, format: &VideoFormat) -> Option<WorkerVideo> {
        let module = match self.worker() {
            Ok(worker) => match worker.status() {
                ModuleStatus::Failed(e) => Err(e),
                _ => Ok(worker.open(blob, name, format)),
            },
            Err(e) => Err(e),
        };
        module.map_err(|e| self.fail(name, e)).ok()
    }

    fn set_fresh(&mut self, name: &str, fresh: bool) {
        if let Some(t) = self.textures.iter_mut().find(|t| t.name == name)
            && t.fresh != fresh
        {
            t.fresh = fresh;
            self.version += 1;
        }
    }

    fn open_segment(&mut self, layers: &[Vec<BgSegment>], layer: usize, segment: usize, t: f64) {
        let schedule = &layers[layer];
        let BgImage::Movie(name) = &schedule[segment].image else {
            return;
        };
        if self.failed.iter().any(|(n, _)| n == name) {
            return;
        }
        let Some(source) = self.sources.iter().find(|s| s.name == *name) else {
            // Not loaded yet: tried again next frame.
            return;
        };
        let (blob, format, browser) = (source.blob.clone(), source.format.clone(), source.browser);
        let name = name.clone();
        // The browser path may have been turned off since (RGB frames).
        let backend = if browser && webcodecs::available() {
            Backend::Browser(WebCodecsVideo::open(blob))
        } else {
            let Some(module) = self.open_module(blob, &name, &format) else {
                return;
            };
            Backend::Module(module)
        };
        let mut decoder = MovieDecoder {
            backend,
            fallback: None,
        };
        let end = self.ends.iter().find(|(n, _)| *n == name).map(|(_, e)| *e);
        let at = t.max(schedule[segment].seconds);
        let looping = backgrounds::movie_loops_in(layers, layer, segment, at);
        let mut track = MovieTrack::new(looping, AHEAD, end);
        // From where the movie stands when the segment starts, or now when
        // it already has.
        let position = backgrounds::movie_position_in(layers, layer, segment, at);
        track.start(&mut decoder, position);
        self.open.push(Open {
            layer,
            segment,
            position,
            name,
            decoder,
            track,
            held: None,
        });
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
        if width.max(height) > limit || width == 0 || height == 0 {
            self.fail(
                name,
                format!("{width}×{height} exceeds the GPU's {limit} px limit"),
            );
            return None;
        }
        let i = match self.textures.iter().position(|t| t.name == name) {
            Some(i) => i,
            None => {
                self.textures.push(Texture {
                    name: name.to_string(),
                    id: usize::MAX,
                    yuv: None,
                    rgb: None,
                    fresh: false,
                });
                self.textures.len() - 1
            }
        };
        let t = &mut self.textures[i];
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
        if t.id != id {
            t.id = id;
            self.version += 1;
        }
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
            self.fail(name, "a malformed frame".into());
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
            self.shown_any = true;
            self.set_fresh(name, true);
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
            self.fail(name, "a frame of another kind".into());
            return;
        };
        if self
            .texture(name, frame.width, frame.height, true, renderer, device)
            .is_none()
        {
            return;
        }
        let Some((texture, ..)) = self
            .textures
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
        self.shown_any = true;
        self.set_fresh(name, true);
    }

    /// Counts over every track so far.
    pub(crate) fn stats(&self) -> MovieStats {
        let mut total = self.closed;
        for open in &self.open {
            total.add(&open.track.stats);
        }
        total
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
        set("open", (self.open.len() as f64).into());
        set("paused", self.paused.into());
        let playing = js_sys::Array::new();
        let decoders = js_sys::Array::new();
        let layers = js_sys::Array::new();
        for open in &self.open {
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
        for (name, e) in &self.failed {
            failed.push(&format!("{name}: {e}").into());
        }
        set("failed", failed.into());
        let status = match self.worker.as_ref().map(VideoWorker::status) {
            None | Some(ModuleStatus::Idle) => "idle".to_string(),
            Some(ModuleStatus::Loading { loaded, total }) => format!("loading {loaded}/{total}"),
            Some(ModuleStatus::Ready { .. }) => "ready".to_string(),
            Some(ModuleStatus::Failed(e)) => format!("failed: {e}"),
        };
        set("module", status.into());
        obj
    }
}
