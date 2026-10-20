//! A play's background movies on desktop (feature `video`): the FFmpeg
//! helper in its sandbox or the checked OS path ([`crate::video`]), a
//! [`MovieTrack`] per open one setting its queue's depth and its seeks,
//! frames shown through `Renderer::add_video`. The web's `movies.rs`
//! without its browser path: the same schedule rules
//! (`ddi_library::backgrounds`, both layers on the one clock a movie
//! has), lead time and queue depth, so a movie changes, loops and stops
//! where the web's does.

use ddi_library::backgrounds::{self, BgImage, BgSegment, LAYERS};
use ddi_platform::video::{
    MovieStats, MovieTrack, StopReason, VideoBackend, VideoCodec, VideoContainer, VideoFormat,
    VideoFrame,
};
use ddi_render::Renderer;

use crate::video::{DesktopVideo, HelperStatus, MovieFile, VideoHost};

/// How long before its change a movie is opened (the web's).
const LEAD_SECONDS: f64 = 2.0;
/// Frames decoded ahead (the web's).
const AHEAD: u32 = 3;

/// A decoder moving to another segment of its movie seeks unless the
/// movie runs on from where it was, within this (a segment following
/// another of the same file, a cut fade, the other layer taking it over).
const CONTINUOUS_SECONDS: f64 = 0.1;

struct Open {
    /// The background layer and the segment of its schedule it plays.
    layer: usize,
    segment: usize,
    /// The movie position it was last given.
    position: f64,
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
    /// Each layer's current segment at the last update.
    current: [Option<usize>; LAYERS],
    /// The drawn movie the automatic rule watches, `(layer, segment)`.
    guard_key: std::cell::Cell<Option<(usize, usize)>>,
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

    /// Where the decoder helper stands, once a movie asked for it.
    pub fn helper_status(&self) -> Option<HelperStatus> {
        self.host.as_ref().map(VideoHost::status)
    }

    /// The movie being drawn now (its texture holds a frame of this run),
    /// for the automatic setting's rule, as `segment * LAYERS + layer`:
    /// while the one it named is still drawn it stays, so a change on the
    /// other layer does not start the rule's warm-up again.
    pub fn drawing(&self, layers: &[Vec<BgSegment>]) -> Option<usize> {
        let drawing: Vec<(usize, usize)> = self
            .current
            .iter()
            .enumerate()
            .filter_map(|(layer, current)| {
                let current = (*current)?;
                let BgImage::Movie(name) = &layers.get(layer)?.get(current)?.image else {
                    return None;
                };
                self.shown_ids()
                    .any(|(what, _)| what == BgImage::Movie(name.clone()))
                    .then_some((layer, current))
            })
            .collect();
        let key = self
            .guard_key
            .get()
            .filter(|k| drawing.contains(k))
            .or_else(|| drawing.first().copied());
        self.guard_key.set(key);
        key.map(|(layer, segment)| segment * LAYERS + layer)
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
        self.current = [None; LAYERS];
        self.guard_key.set(None);
        self.paused = false;
        if self
            .host
            .as_ref()
            .is_some_and(|h| matches!(h.status(), HelperStatus::Failed(_)))
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
    /// the frames due; `layers` the background layers' schedules.
    pub fn update(
        &mut self,
        layers: &[Vec<BgSegment>],
        t: f64,
        renderer: &mut Renderer,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
    ) {
        if layers.iter().all(Vec::is_empty)
            || self.sources.is_empty()
            || self.off.is_some()
            || self.paused
        {
            return;
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
        for open in &mut self.open {
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
            if let Some(e) = open.track.error() {
                errors.push((open.name.clone(), e.to_string()));
                continue;
            }
            if currents[open.layer] == Some(open.segment) {
                if let Some(frame) = frame.or_else(|| open.held.take()) {
                    uploads.push((open.name.clone(), frame));
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
        for (name, frame) in uploads {
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

    fn open_segment(&mut self, layers: &[Vec<BgSegment>], layer: usize, segment: usize, t: f64) {
        let schedule = &layers[layer];
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
        let format = VideoFormat {
            container: VideoContainer::Unknown,
            codec: VideoCodec::Unknown,
        };
        let mut decoder = self.host().open(path, &name, &format);
        let end = self.ends.iter().find(|(n, _)| *n == name).map(|(_, e)| *e);
        let at = t.max(schedule[segment].seconds);
        let looping = backgrounds::movie_loops_in(layers, layer, segment, at);
        let mut track = MovieTrack::new(looping, AHEAD, end);
        track.start(
            &mut decoder,
            backgrounds::movie_position_in(layers, layer, segment, at),
        );
        self.open.push(Open {
            layer,
            segment,
            position: backgrounds::movie_position_in(layers, layer, segment, at),
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
        if frame.external.is_none() && frame.split().is_none() {
            self.fail(name, "a malformed frame".into());
            return;
        }
        let Some(id) = self.texture(name, frame.width, frame.height, renderer, device) else {
            return;
        };
        if crate::video::show(renderer, device, queue, id, frame) {
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
