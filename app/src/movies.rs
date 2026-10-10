//! Background movies during play (`docs/plans/video-backgrounds.md`, step
//! 4). Opens the decoder of each movie segment ahead of it (the current
//! segment and the next one: two at most), drives it with a [`MovieTrack`]
//! on the movie's clock ([`backgrounds::movie_position`], from the same song
//! time the notes are drawn at) and uploads at most one frame per display
//! frame. Each movie draws into its own texture, kept for the session, so
//! a movie that fades out or resumes later keeps its last frame, as
//! StepMania pauses it; a decoder moves on to its movie's next segment
//! instead of opening the file again. Nothing here touches judging.
//!
//! Step 5: the worker starts, and fetches the module, as soon as the first
//! movie is known (the start prompt shows the download), the song's first
//! movie is probed for its black bars (the background frame), and the
//! automatic setting can turn the movies off for the rest of a play
//! ([`MovieDeck::turn_off`]).

use ddi_library::backgrounds::{self, BgImage, BgSegment};
use ddi_platform::video::{
    MovieStats, MovieTrack, StopReason, VideoBackend, VideoFormat, VideoFrame,
};
use ddi_render::Renderer;
use web_sys::Blob;

use crate::web::video::{ModuleStatus, Picture, VideoWorker, WorkerVideo};

/// A movie is opened this long before its segment starts: opening and
/// seeking a long-GOP H.264 file took up to 1.2 s (step 1).
const LEAD_SECONDS: f64 = 2.0;

/// Frames kept decoded or asked for, per movie.
const AHEAD: u32 = 3;

struct Source {
    name: String,
    blob: Blob,
    format: VideoFormat,
}

struct Open {
    /// The schedule segment it plays.
    segment: usize,
    name: String,
    decoder: WorkerVideo,
    track: MovieTrack,
    /// A frame that came due before its segment started (prefetch): shown
    /// when the segment starts, so the change never waits for a decode.
    held: Option<VideoFrame>,
}

/// The black-bar probe of the song's first movie.
struct Probe {
    name: String,
    /// Its id in the worker, once sent (the file may still be loading).
    id: Option<u32>,
    answer: Option<Result<Picture, String>>,
}

/// A movie's texture.
struct Texture {
    name: String,
    id: usize,
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
    /// The segment current at the last update.
    current: Option<usize>,
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
        if self.worker().is_ok() && probe && self.probe.is_none() {
            self.probe = Some(Probe {
                name: name.clone(),
                id: None,
                answer: None,
            });
        }
        self.sources.push(Source { name, blob, format });
        self.send_probe();
    }

    /// The session's worker, started (and told to fetch the module) when
    /// there is none.
    fn worker(&mut self) -> Result<&mut VideoWorker, String> {
        if self.worker.is_none() {
            let worker = VideoWorker::new()?;
            worker.preload();
            self.worker = Some(worker);
        }
        Ok(self.worker.as_mut().expect("just set"))
    }

    fn send_probe(&mut self) {
        let Some(probe) = self.probe.as_mut().filter(|p| p.id.is_none()) else {
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
        let Some(id) = probe.id else {
            return Err(true);
        };
        if probe.answer.is_none() {
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

    /// The movie segment being drawn now (its texture holds a frame of
    /// this run), for the automatic setting's rule.
    pub(crate) fn drawing(&self, schedule: &[BgSegment]) -> Option<usize> {
        let current = self.current?;
        let BgImage::Movie(name) = &schedule.get(current)?.image else {
            return None;
        };
        self.shown_ids()
            .any(|(what, _)| what == BgImage::Movie(name.clone()))
            .then_some(current)
    }

    /// Stops every movie for the rest of this play (the song background
    /// shows); a retry plays them again.
    pub(crate) fn turn_off(&mut self, reason: StopReason) {
        if self.off.is_some() {
            return;
        }
        web_sys::console::warn_1(
            &format!("background videos turned off for this play: {reason:?}").into(),
        );
        self.off = Some(reason);
        for open in std::mem::take(&mut self.open) {
            self.close(open);
        }
        for texture in &mut self.textures {
            texture.fresh = false;
        }
        self.version += 1;
    }

    /// Why the movies were turned off for this play, if they were.
    pub(crate) fn off(&self) -> Option<StopReason> {
        self.off
    }

    /// Closes every decoder (the song ended). Sources, textures and what
    /// was learned stay for a retry, which shows no old frame and tries
    /// failed movies again (with a new worker if the old one stopped);
    /// why the movies were turned off stays for the results.
    pub(crate) fn stop(&mut self) {
        for open in self.open.drain(..) {
            add_stats(&mut self.closed, &open.track.stats);
        }
        for texture in &mut self.textures {
            texture.fresh = false;
        }
        self.failed.clear();
        self.current = None;
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
    /// frame due.
    pub(crate) fn update(
        &mut self,
        schedule: &[BgSegment],
        t: f64,
        renderer: &mut Renderer,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
    ) {
        if schedule.is_empty() || self.sources.is_empty() || self.off.is_some() {
            return;
        }
        let current = backgrounds::state_at(schedule, t).current;
        if self.current != Some(current) {
            self.current = Some(current);
            // A movie that starts over shows nothing old meanwhile.
            if let BgImage::Movie(name) = &schedule[current].image
                && schedule[current].effect.restarts()
            {
                self.set_fresh(name, false);
            }
        }
        let next = schedule
            .iter()
            .enumerate()
            .skip(current + 1)
            .take_while(|(_, s)| s.seconds - t < LEAD_SECONDS)
            .find(|(_, s)| matches!(s.image, BgImage::Movie(_)))
            .map(|(i, _)| i);
        let wanted: Vec<usize> = [Some(current), next]
            .into_iter()
            .flatten()
            .filter(|&i| matches!(schedule[i].image, BgImage::Movie(_)))
            .collect();
        // A decoder whose segment is over moves to its movie's wanted
        // segment; the others close (a movie fading out stays on its last
        // frame in its texture).
        for &segment in &wanted {
            if self.open.iter().any(|o| o.segment == segment) {
                continue;
            }
            let BgImage::Movie(name) = &schedule[segment].image else {
                continue;
            };
            if let Some(open) = self
                .open
                .iter_mut()
                .find(|o| o.name == *name && !wanted.contains(&o.segment))
            {
                open.segment = segment;
                open.held = None;
                open.track
                    .set_looping(backgrounds::movie_loops(schedule, segment));
                let at = t.max(schedule[segment].seconds);
                open.track.start(
                    &mut open.decoder,
                    backgrounds::movie_position(schedule, segment, at),
                );
            }
        }
        let mut i = 0;
        while i < self.open.len() {
            if wanted.contains(&self.open[i].segment) {
                i += 1;
            } else {
                let open = self.open.remove(i);
                self.close(open);
            }
        }
        for &segment in &wanted {
            if !self.open.iter().any(|o| o.segment == segment) {
                self.open_segment(schedule, segment, t);
            }
        }
        let mut upload = None;
        let mut early = Vec::new();
        let mut errors = Vec::new();
        for open in &mut self.open {
            let position = backgrounds::movie_position(schedule, open.segment, t);
            let frame = open.track.update(&mut open.decoder, position);
            if let Some(e) = open.track.error() {
                errors.push((open.name.clone(), e.to_string()));
                continue;
            }
            if open.segment == current {
                if let Some(frame) = frame.or_else(|| open.held.take()) {
                    upload = Some((open.name.clone(), frame));
                }
            } else if let Some(frame) = frame {
                // Its texture is made now, not on the frame of the change.
                early.push((open.name.clone(), frame.width, frame.height));
                open.held = Some(frame);
            }
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
        for (name, width, height) in early {
            self.texture(&name, width, height, renderer, device);
        }
        if let Some((name, frame)) = upload {
            self.upload(&name, &frame, renderer, device, queue);
        }
    }

    fn close(&mut self, open: Open) {
        add_stats(&mut self.closed, &open.track.stats);
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
        self.failed.push((name.to_string(), e));
        self.version += 1;
    }

    fn set_fresh(&mut self, name: &str, fresh: bool) {
        if let Some(t) = self.textures.iter_mut().find(|t| t.name == name)
            && t.fresh != fresh
        {
            t.fresh = fresh;
            self.version += 1;
        }
    }

    fn open_segment(&mut self, schedule: &[BgSegment], segment: usize, t: f64) {
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
        let (blob, format) = (source.blob.clone(), source.format.clone());
        let name = name.clone();
        let worker = match self.worker() {
            Ok(w) => w,
            Err(e) => {
                self.fail(&name, e);
                return;
            }
        };
        // A worker that stopped answers nothing: the movie cannot play.
        if let ModuleStatus::Failed(e) = worker.status() {
            self.fail(&name, e);
            return;
        }
        let mut decoder = worker.open(blob, &name, &format);
        let end = self.ends.iter().find(|(n, _)| *n == name).map(|(_, e)| *e);
        let mut track = MovieTrack::new(backgrounds::movie_loops(schedule, segment), AHEAD, end);
        // From where the movie stands when the segment starts, or now when
        // it already has.
        let at = t.max(schedule[segment].seconds);
        track.start(
            &mut decoder,
            backgrounds::movie_position(schedule, segment, at),
        );
        self.open.push(Open {
            segment,
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
        if let Some(t) = self.textures.iter().find(|t| t.name == name) {
            return Some(t.id);
        }
        let id = renderer.add_video(device, width, height);
        self.textures.push(Texture {
            name: name.to_string(),
            id,
            fresh: false,
        });
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
        // A malformed frame never reaches a texture (it would show green:
        // new textures are zeroed).
        if frame.split().is_none() {
            self.fail(name, "a malformed frame".into());
            return;
        }
        let Some(id) = self.texture(name, frame.width, frame.height, renderer, device) else {
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

    /// Counts over every track so far.
    pub(crate) fn stats(&self) -> MovieStats {
        let mut total = self.closed;
        for open in &self.open {
            add_stats(&mut total, &open.track.stats);
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
        let playing = js_sys::Array::new();
        for open in &self.open {
            playing.push(&open.name.clone().into());
        }
        set("playing", playing.into());
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

fn add_stats(total: &mut MovieStats, s: &MovieStats) {
    total.decoded += s.decoded;
    total.shown += s.shown;
    total.dropped += s.dropped;
    total.late += s.late;
    total.seeks += s.seeks;
}
