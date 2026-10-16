//! A play's background movies on desktop (feature `video`): the FFmpeg
//! module under wasmtime ([`crate::video`]) for every movie, a
//! [`MovieTrack`] per open one setting its queue's depth and its seeks,
//! frames shown through `Renderer::add_video`. The web's `movies.rs`
//! without its browser path: the same schedule rules
//! (`ddi_library::backgrounds`), lead time and queue depth, so a movie
//! changes, loops and stops where the web's does.

use ddi_library::backgrounds::{self, BgImage, BgSegment};
use ddi_platform::video::{
    MovieStats, MovieTrack, StopReason, VideoBackend, VideoCodec, VideoContainer, VideoFormat,
    VideoFrame,
};
use ddi_render::Renderer;

use crate::video::{DesktopVideo, ModuleStatus, MovieFile, VideoHost};

/// How long before its change a movie is opened (the web's).
const LEAD_SECONDS: f64 = 2.0;
/// Frames decoded ahead (the web's).
const AHEAD: u32 = 3;

struct Open {
    segment: usize,
    name: String,
    decoder: DesktopVideo,
    track: MovieTrack,
    /// A frame that came due before its segment started.
    held: Option<VideoFrame>,
}

/// The first movie's picture probe: its name, its id, its answer (the
/// picture's shape, or none).
type Probe = (String, Option<u32>, Option<Result<f32, ()>>);

struct Texture {
    name: String,
    id: usize,
    /// It holds a frame of this run (before that, the song background
    /// shows).
    fresh: bool,
}

/// The song's movies.
#[derive(Default)]
pub struct Movies {
    host: Option<VideoHost>,
    /// `(name, file)`, or why it cannot be had.
    sources: Vec<(String, Result<MovieFile, String>)>,
    open: Vec<Open>,
    textures: Vec<Texture>,
    ends: Vec<(String, f64)>,
    failed: Vec<(String, String)>,
    current: Option<usize>,
    closed: MovieStats,
    /// The first movie's picture probe, and its answer.
    probe: Option<Probe>,
    shown_any: bool,
    off: Option<StopReason>,
    paused: bool,
    trouble: Vec<(String, String)>,
}

impl Movies {
    fn host(&mut self) -> &mut VideoHost {
        self.host.get_or_insert_with(|| {
            let h = VideoHost::shared();
            h.preload();
            h
        })
    }

    /// A movie of the song, at `at` (a file, or a range of one: a zip's
    /// stored entry), or why it cannot be had. The first one added is
    /// probed for the background frame.
    pub fn add(&mut self, name: &str, at: Result<crate::play::load::MovieAt, String>) {
        if self.sources.iter().any(|(n, _)| n == name) {
            return;
        }
        let path = at.map(|(path, range)| MovieFile { path, range });
        if let Ok(p) = &path
            && self.probe.is_none()
        {
            let id = self.host().probe(p, name);
            self.probe = Some((name.to_string(), Some(id), None));
        }
        self.sources.push((name.to_string(), path));
    }

    /// The probed movie's picture shape: `Err(true)` while the probe runs.
    pub fn picture_aspect(&mut self) -> Result<f32, bool> {
        let host = self.host.clone();
        let Some((_, id, answer)) = self.probe.as_mut() else {
            return Err(false);
        };
        if answer.is_none()
            && let (Some(i), Some(h)) = (*id, host)
        {
            *answer = h.probe_result(i).map(|r| {
                r.ok()
                    .and_then(|p| p.bounds)
                    .filter(|b| b.3 > 0)
                    .map(|b| b.2 as f32 / b.3 as f32)
                    .ok_or(())
            });
        }
        match answer {
            None => Err(true),
            Some(Ok(a)) => Ok(*a),
            Some(Err(())) => Err(false),
        }
    }

    pub fn shown_any(&self) -> bool {
        self.shown_any
    }

    /// Where the module stands, once a movie asked for it.
    pub fn module_status(&self) -> Option<ModuleStatus> {
        self.host.as_ref().map(VideoHost::status)
    }

    /// The movie segment drawn now (its texture holds a frame of this run).
    pub fn drawing(&self, schedule: &[BgSegment]) -> Option<usize> {
        let current = self.current?;
        let BgImage::Movie(name) = &schedule.get(current)?.image else {
            return None;
        };
        self.shown_ids()
            .any(|(what, _)| what == BgImage::Movie(name.clone()))
            .then_some(current)
    }

    pub fn turn_off(&mut self, reason: StopReason) {
        if self.off.is_some() {
            return;
        }
        eprintln!("background videos turned off for this play: {reason:?}");
        self.off = Some(reason);
        self.paused = false;
        for open in std::mem::take(&mut self.open) {
            self.close(open);
        }
        for t in &mut self.textures {
            t.fresh = false;
        }
    }

    pub fn pause(&mut self) {
        if self.paused || self.off.is_some() {
            return;
        }
        self.paused = true;
        for open in std::mem::take(&mut self.open) {
            self.close(open);
        }
    }

    pub fn resume(&mut self) {
        self.paused = false;
    }

    /// Whether the opening movie (segment 1) is expected to play.
    pub fn opening_plays(&self, schedule: &[BgSegment]) -> bool {
        let Some(BgImage::Movie(name)) = schedule.get(1).map(|s| &s.image) else {
            return false;
        };
        self.off.is_none()
            && !self.failed.iter().any(|(n, _)| n == name)
            && !self.trouble.iter().any(|(n, _)| n == name)
    }

    pub fn off(&self) -> Option<StopReason> {
        self.off
    }

    /// What kept movies from playing, in a line for the results.
    pub fn trouble(&self) -> Option<String> {
        let ((name, first), rest) = self.trouble.split_first()?;
        Some(if rest.is_empty() {
            format!("the video {name} could not play: {first}")
        } else if rest.iter().all(|(_, e)| e == first) {
            format!("{} videos could not play: {first}", rest.len() + 1)
        } else {
            format!("{} videos could not play; {name}: {first}", rest.len() + 1)
        })
    }

    /// Closes every decoder (the song ended); a retry starts them again.
    pub fn stop(&mut self) {
        for open in std::mem::take(&mut self.open) {
            self.close(open);
        }
        for t in &mut self.textures {
            t.fresh = false;
        }
        self.failed.clear();
        self.current = None;
        self.paused = false;
        if self
            .host
            .as_ref()
            .is_some_and(|h| matches!(h.status(), ModuleStatus::Failed(_)))
        {
            self.host = None;
        }
    }

    /// For the next play: the movies on again.
    pub fn reset(&mut self) {
        self.stop();
        self.off = None;
        self.trouble.clear();
    }

    /// Frames that came late, so far.
    pub fn late(&self) -> u64 {
        self.stats().late
    }

    pub fn stats(&self) -> MovieStats {
        let mut s = self.closed;
        for o in &self.open {
            add_stats(&mut s, &o.track.stats);
        }
        s
    }

    /// The textures holding a frame of this run.
    pub fn shown_ids(&self) -> impl Iterator<Item = (BgImage, usize)> + '_ {
        self.textures
            .iter()
            .filter(|t| t.fresh && !self.failed.iter().any(|(n, _)| *n == t.name))
            .map(|t| (BgImage::Movie(t.name.clone()), t.id))
    }

    /// Opens, feeds and closes decoders for song second `t`, and shows
    /// the frame due.
    pub fn update(
        &mut self,
        schedule: &[BgSegment],
        t: f64,
        renderer: &mut Renderer,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
    ) {
        if schedule.is_empty() || self.sources.is_empty() || self.off.is_some() || self.paused {
            return;
        }
        let current = backgrounds::state_at(schedule, t).current;
        if self.current != Some(current) {
            self.current = Some(current);
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
                early.push((open.name.clone(), frame.width, frame.height));
                if let Some(old) = open.held.replace(frame) {
                    self_recycle(&self.host, old);
                }
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
        for (name, w, h) in early {
            self.texture(&name, w, h, renderer, device);
        }
        if let Some((name, frame)) = upload {
            self.upload(&name, &frame, renderer, device, queue);
            self_recycle(&self.host, frame);
        }
    }

    fn close(&mut self, open: Open) {
        add_stats(&mut self.closed, &open.track.stats);
        if let Some(end) = open.track.end() {
            match self.ends.iter_mut().find(|(n, _)| *n == open.name) {
                Some((_, e)) => *e = end,
                None => self.ends.push((open.name.clone(), end)),
            }
        }
    }

    fn fail(&mut self, name: &str, e: String) {
        if self.failed.iter().any(|(n, _)| n == name) {
            return;
        }
        eprintln!("background video {name}: {e}; the song background shows");
        if !self.trouble.iter().any(|(n, _)| n == name) {
            self.trouble.push((name.to_string(), e.clone()));
        }
        self.failed.push((name.to_string(), e));
    }

    fn set_fresh(&mut self, name: &str, fresh: bool) {
        if let Some(t) = self.textures.iter_mut().find(|t| t.name == name) {
            t.fresh = fresh;
        }
    }

    fn open_segment(&mut self, schedule: &[BgSegment], segment: usize, t: f64) {
        let BgImage::Movie(name) = &schedule[segment].image else {
            return;
        };
        if self.failed.iter().any(|(n, _)| n == name) {
            return;
        }
        let Some((_, path)) = self.sources.iter().find(|(n, _)| n == name) else {
            return;
        };
        let name = name.clone();
        let path = match path.clone() {
            Ok(p) => p,
            Err(e) => {
                self.fail(&name, e);
                return;
            }
        };
        if let ModuleStatus::Failed(e) = self.host().status() {
            self.fail(&name, e);
            return;
        }
        let format = VideoFormat {
            container: VideoContainer::Unknown,
            codec: VideoCodec::Unknown,
        };
        let mut decoder = self.host().open(path, &name, &format);
        let end = self.ends.iter().find(|(n, _)| *n == name).map(|(_, e)| *e);
        let mut track = MovieTrack::new(backgrounds::movie_loops(schedule, segment), AHEAD, end);
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
        if frame.split().is_none() {
            self.fail(name, "a malformed frame".into());
            return;
        }
        let Some(id) = self.texture(name, frame.width, frame.height, renderer, device) else {
            return;
        };
        if renderer.set_video_frame(
            device,
            queue,
            id,
            frame.width,
            frame.height,
            &frame.planes,
            frame.matrix == ddi_platform::video::YuvMatrix::Bt709,
            frame.full_range,
        ) {
            self.shown_any = true;
            self.set_fresh(name, true);
        }
    }
}

/// A frame's planes back to the decoder thread.
fn self_recycle(host: &Option<VideoHost>, frame: VideoFrame) {
    if let Some(h) = host {
        h.recycle(frame.planes);
    }
}

fn add_stats(total: &mut MovieStats, s: &MovieStats) {
    total.decoded += s.decoded;
    total.shown += s.shown;
    total.dropped += s.dropped;
    total.late += s.late;
    total.seeks += s.seeks;
}
