//! A play's background movies, both shells' (`docs/plans/video-backgrounds.md`,
//! steps 4–9; `docs/plans/native-video.md`): which movie segments have a
//! decoder (on each background layer the current segment and the next
//! one starting within [`LEAD_SECONDS`], a file current on both layers
//! once, [`backgrounds::movie_segments_at`]), each driven by a
//! [`MovieTrack`] on the movie's clock ([`backgrounds::movie_position_in`],
//! from the same song time the notes are drawn at, shared by both layers
//! as StepMania shares a movie's texture), the frames due each display
//! frame, and what shows: a movie's texture, kept for the session, holds
//! its last frame, so a movie that fades out or resumes later keeps it, as
//! StepMania pauses it; a decoder moves on to its movie's next segment
//! instead of opening the file again. Nothing here touches judging.
//!
//! [`MovieSlots`] keeps that state; the shell ([`MovieShell`]) opens the
//! decoders (the web's WebCodecs or the module's worker, desktop's OS
//! decoders or helper process), may move one to another decoder, and makes
//! the textures and uploads the frames [`MovieSlots::update`] hands it.

use ddi_library::backgrounds::{self, BgImage, BgSegment, LAYERS};
use ddi_platform::video::{MovieStats, MovieTrack, StopReason, VideoDecoder, VideoFrame};
use std::cell::Cell;

/// A movie is opened this long before its segment starts: opening and
/// seeking a long-GOP H.264 file took up to 1.2 s (the web's step 1).
pub const LEAD_SECONDS: f64 = 2.0;

/// Frames kept decoded or asked for, per movie.
pub const AHEAD: u32 = 3;

/// A decoder moving to another segment of its movie seeks unless the
/// movie runs on from where it was, within this (a segment following
/// another of the same file, a cut fade, the other layer taking it over).
pub const CONTINUOUS_SECONDS: f64 = 0.1;

/// What a shell brings to its movies.
pub trait MovieShell {
    type Decoder: VideoDecoder;

    /// Opens movie `name`: `None` while its file is not there yet (tried
    /// again next frame), `Err` why it cannot play (it then fails).
    fn open(&mut self, name: &str) -> Option<Result<Self::Decoder, String>>;

    /// Called for each open movie after its track was fed: `Ok(true)` when
    /// its decoder was handed to another decoder (the track then seeks it
    /// to where the movie stands), `Err` why the movie cannot play.
    fn fall_back(&mut self, _name: &str, _decoder: &mut Self::Decoder) -> Result<bool, String> {
        Ok(false)
    }

    /// A frame that came due before its segment started, replaced by a
    /// newer one before it was shown.
    fn discard(&mut self, _frame: VideoFrame) {}

    /// Every decoder was closed ([`MovieSlots::stop`]).
    fn stopped(&mut self) {}

    /// A line for the log: a movie failed, or the movies were turned off.
    fn warn(&self, line: &str);

    /// A line for the log about the automatic setting's trial.
    fn info(&self, _line: &str) {}
}

/// An open movie: its decoder and track, and the segment it plays.
pub struct OpenMovie<D> {
    /// The background layer and the segment of its schedule it plays.
    pub layer: usize,
    pub segment: usize,
    pub name: String,
    pub decoder: D,
    /// The movie position it was last given.
    position: f64,
    track: MovieTrack,
    /// A frame that came due before its segment started (prefetch): shown
    /// when the segment starts, so the change never waits for a decode.
    held: Option<VideoFrame>,
}

/// A movie's texture as the backdrop sees it.
struct MovieTexture {
    name: String,
    /// The renderer id shown.
    id: usize,
    /// It holds a frame of the current run: before that (a retry, a change
    /// that starts the movie over), the song background shows instead of
    /// an old frame.
    fresh: bool,
}

/// What [`MovieSlots::update`] leaves the shell to do, in this order.
#[derive(Debug, Default)]
pub struct DueFrames {
    /// Movies whose segment has not started with a frame decoded for it:
    /// their textures are made now, not on the frame of the change,
    /// `(name, width, height, external)` ([`VideoFrame::external`]).
    pub early: Vec<(String, u32, u32, bool)>,
    /// Frames to show now, `(name, frame)`: at most one per movie.
    pub show: Vec<(String, VideoFrame)>,
}

/// A play's movies over shell `S` (module doc).
pub struct MovieSlots<S: MovieShell> {
    shell: S,
    open: Vec<OpenMovie<S::Decoder>>,
    textures: Vec<MovieTexture>,
    /// Each movie's last frame time, once a track learned it, for the
    /// movie's later tracks (a resume past the end wraps at once).
    ends: Vec<(String, f64)>,
    /// Movies that could not play this session: the song background shows.
    failed: Vec<(String, String)>,
    /// The segment current on each layer at the last update.
    current: [Option<usize>; LAYERS],
    /// The drawn movie the automatic rule watches, `(layer, segment)`.
    guard_key: Cell<Option<(usize, usize)>>,
    /// Counts of closed tracks, plus the open ones for [`MovieSlots::stats`].
    closed: MovieStats,
    /// Changes whenever [`MovieSlots::shown_ids`] would.
    version: u64,
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
}

impl<S: MovieShell + Default> Default for MovieSlots<S> {
    fn default() -> Self {
        MovieSlots::new(S::default())
    }
}

impl<S: MovieShell> MovieSlots<S> {
    pub fn new(shell: S) -> Self {
        MovieSlots {
            shell,
            open: Vec::new(),
            textures: Vec::new(),
            ends: Vec::new(),
            failed: Vec::new(),
            current: [None; LAYERS],
            guard_key: Cell::new(None),
            closed: MovieStats::default(),
            version: 0,
            shown_any: false,
            off: None,
            paused: false,
            trouble: Vec::new(),
        }
    }

    pub fn shell(&self) -> &S {
        &self.shell
    }

    pub fn shell_mut(&mut self) -> &mut S {
        &mut self.shell
    }

    /// The movies open now.
    pub fn opened(&self) -> &[OpenMovie<S::Decoder>] {
        &self.open
    }

    /// The open movies' decoders.
    pub fn decoders_mut(&mut self) -> impl Iterator<Item = &mut S::Decoder> {
        self.open.iter_mut().map(|o| &mut o.decoder)
    }

    /// Movies that could not play since the song started, `(name, why)`.
    pub fn failed(&self) -> &[(String, String)] {
        &self.failed
    }

    /// A movie frame has been shown this session (the background frame is
    /// settled from then on).
    pub fn shown_any(&self) -> bool {
        self.shown_any
    }

    /// Changes whenever [`MovieSlots::shown_ids`] would: a texture made or
    /// replaced, a movie's first frame of a run or its start over, a
    /// failure, the movies turned off or stopped.
    pub fn version(&self) -> u64 {
        self.version
    }

    /// The movie being drawn now (its texture holds a frame of this run),
    /// for the automatic setting's rule, as `segment * LAYERS + layer`:
    /// while the one it named is still drawn it stays, so a change on the
    /// other layer does not start the rule's warm-up again.
    pub fn drawing(&self, layers: &[Vec<BgSegment>]) -> Option<usize> {
        backgrounds::drawing_movie(layers, &self.current, &self.guard_key, |name| {
            // Shown (`MovieSlots::shown_ids`).
            self.textures.iter().any(|t| {
                t.fresh && t.name == name && !self.failed.iter().any(|(n, _)| *n == t.name)
            })
        })
    }

    /// Stops every movie for the rest of this play: the song's own
    /// background shows (not the opening still, [`MovieSlots::opening_plays`]);
    /// a retry plays them again.
    pub fn turn_off(&mut self, reason: StopReason) {
        if self.off.is_some() {
            return;
        }
        self.shell.warn(&format!(
            "background videos turned off for this play: {reason:?}"
        ));
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
    pub fn pause(&mut self) {
        if self.paused || self.off.is_some() {
            return;
        }
        self.shell
            .info("background videos paused: does the display keep up without them?");
        self.paused = true;
        for open in std::mem::take(&mut self.open) {
            self.close(open);
        }
    }

    /// Paused by the automatic setting's trial.
    pub fn paused(&self) -> bool {
        self.paused
    }

    /// Plays on after a trial: the movies open again where they stand.
    pub fn resume(&mut self) {
        if self.paused {
            self.shell
                .info("background videos play on: the display fell behind without them too");
        }
        self.paused = false;
    }

    /// Whether the song's opening movie (schedule segment 1) is expected to
    /// play this play: the opening still stands in for it until its first
    /// frame only then; off, failed or turned off, the song background
    /// shows, as StepMania would.
    pub fn opening_plays(&self, schedule: &[BgSegment]) -> bool {
        backgrounds::opening_plays(schedule, self.off.is_some(), &self.failed, &self.trouble)
    }

    /// Why the movies were turned off for this play, if they were.
    pub fn off(&self) -> Option<StopReason> {
        self.off
    }

    /// What kept movies from playing this play, in a line for the results.
    pub fn trouble(&self) -> Option<String> {
        ddi_platform::video::trouble_line(&self.trouble)
    }

    /// Closes every decoder (the song ended). Textures and what was
    /// learned (each movie's end) stay for a retry, which shows no old
    /// frame and tries failed movies again; why the movies were turned
    /// off stays for the results.
    pub fn stop(&mut self) {
        for open in std::mem::take(&mut self.open) {
            self.close(open);
        }
        // Nothing shows afterwards: a change only when something did
        // (desktop stops every frame once the song ended).
        if self.textures.iter().any(|t| t.fresh) {
            self.version += 1;
        }
        for texture in &mut self.textures {
            texture.fresh = false;
        }
        self.failed.clear();
        self.current = [None; LAYERS];
        self.guard_key.set(None);
        self.paused = false;
        self.shell.stopped();
    }

    /// Stops for good (the session is over): the next play starts with the
    /// movies on.
    pub fn reset(&mut self) {
        self.stop();
        self.off = None;
        self.trouble.clear();
    }

    /// Counts over every track so far.
    pub fn stats(&self) -> MovieStats {
        let mut total = self.closed;
        for open in &self.open {
            total.add(&open.track.stats);
        }
        total
    }

    /// Frames that came late, so far.
    pub fn late(&self) -> u64 {
        self.stats().late
    }

    /// The movie textures that hold a frame of this run, for
    /// [`backgrounds::shown`]: before its first frame, or once it failed, a
    /// movie shows the song background.
    pub fn shown_ids(&self) -> impl Iterator<Item = (BgImage, usize)> + '_ {
        self.textures
            .iter()
            .filter(|t| t.fresh && !self.failed.iter().any(|(n, _)| *n == t.name))
            .map(|t| (BgImage::Movie(t.name.clone()), t.id))
    }

    /// Nothing to do for `layers` now: no schedule, or the movies turned
    /// off or paused.
    pub fn idle(&self, layers: &[Vec<BgSegment>]) -> bool {
        layers.iter().all(Vec::is_empty) || self.off.is_some() || self.paused
    }

    /// Opens, feeds and closes decoders for song second `t`; `layers` the
    /// background layers' schedules. The shell then makes the textures and
    /// shows the frames it returns.
    pub fn update(&mut self, layers: &[Vec<BgSegment>], t: f64) -> DueFrames {
        let mut due = DueFrames::default();
        if self.idle(layers) {
            return due;
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
                    due.show.push((open.name.clone(), frame));
                }
            } else if let Some(frame) = frame {
                due.early.push((
                    open.name.clone(),
                    frame.width,
                    frame.height,
                    frame.external.is_some(),
                ));
                if let Some(old) = open.held.replace(frame) {
                    self.shell.discard(old);
                }
            }
        }
        // A movie its shell moved to another decoder starts again where it
        // stands.
        for i in 0..self.open.len() {
            let open = &mut self.open[i];
            match self.shell.fall_back(&open.name, &mut open.decoder) {
                Ok(false) => {}
                Ok(true) => {
                    open.held = None;
                    let (layer, segment) = (open.layer, open.segment);
                    let at = t.max(layers[layer][segment].seconds);
                    open.track.start(
                        &mut open.decoder,
                        backgrounds::movie_position_in(layers, layer, segment, at),
                    );
                }
                Err(e) => {
                    let name = open.name.clone();
                    self.fail(&name, e);
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
        due
    }

    fn close(&mut self, open: OpenMovie<S::Decoder>) {
        self.closed.add(&open.track.stats);
        if let Some(end) = open.track.end() {
            match self.ends.iter_mut().find(|(n, _)| *n == open.name) {
                Some((_, e)) => *e = end,
                None => self.ends.push((open.name, end)),
            }
        }
    }

    /// Movie `name` cannot play this play: the song background shows for
    /// it, and the results say why.
    pub fn fail(&mut self, name: &str, e: String) {
        if self.failed.iter().any(|(n, _)| n == name) {
            return;
        }
        self.shell.warn(&format!(
            "background video {name}: {e}; the song background shows"
        ));
        if !self.trouble.iter().any(|(n, _)| n == name) {
            self.trouble.push((name.to_string(), e.clone()));
        }
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

    fn open_segment(&mut self, layers: &[Vec<BgSegment>], layer: usize, segment: usize, t: f64) {
        let schedule = &layers[layer];
        let BgImage::Movie(name) = &schedule[segment].image else {
            return;
        };
        if self.failed.iter().any(|(n, _)| n == name) {
            return;
        }
        let mut decoder = match self.shell.open(name) {
            None => return,
            Some(Ok(decoder)) => decoder,
            Some(Err(e)) => {
                self.fail(name, e);
                return;
            }
        };
        let end = self.ends.iter().find(|(n, _)| n == name).map(|(_, e)| *e);
        let at = t.max(schedule[segment].seconds);
        let looping = backgrounds::movie_loops_in(layers, layer, segment, at);
        let mut track = MovieTrack::new(looping, AHEAD, end);
        // From where the movie stands when the segment starts, or now when
        // it already has.
        let position = backgrounds::movie_position_in(layers, layer, segment, at);
        track.start(&mut decoder, position);
        self.open.push(OpenMovie {
            layer,
            segment,
            name: name.clone(),
            decoder,
            position,
            track,
            held: None,
        });
    }

    /// Whether a `width × height` frame of movie `name` fits a texture of
    /// at most `limit` pixels a side; the movie fails when it does not.
    pub fn fits(&mut self, name: &str, width: u32, height: u32, limit: u32) -> bool {
        if width.max(height) > limit || width == 0 || height == 0 {
            self.fail(
                name,
                format!("{width}×{height} exceeds the GPU's {limit} px limit"),
            );
            return false;
        }
        true
    }

    /// The renderer id movie `name`'s texture has, once it has one.
    pub fn texture(&self, name: &str) -> Option<usize> {
        self.textures.iter().find(|t| t.name == name).map(|t| t.id)
    }

    /// Movie `name` draws into texture `id` from now on (it shows once a
    /// frame of this run is in it, [`MovieSlots::shown`]).
    pub fn set_texture(&mut self, name: &str, id: usize) {
        match self.textures.iter_mut().find(|t| t.name == name) {
            Some(t) if t.id == id => return,
            Some(t) => t.id = id,
            None => self.textures.push(MovieTexture {
                name: name.to_string(),
                id,
                fresh: false,
            }),
        }
        self.version += 1;
    }

    /// A frame of movie `name` went into its texture.
    pub fn shown(&mut self, name: &str) {
        self.shown_any = true;
        self.set_fresh(name, true);
    }
}

/// The textures a play's backdrop picks from ([`backgrounds::shown`]): the
/// song's images, the movies' textures that hold a frame of this run, and
/// the second layer's opaque stand-in for a missing song background
/// ([`BgImage::Plain`]). Made again only when an image was added or the
/// movies' [`MovieSlots::version`] changed, not every frame.
#[derive(Debug, Default)]
pub struct BackdropTextures {
    list: Vec<(BgImage, usize)>,
    /// `(images, movies' version)` the list was made for.
    key: Option<(usize, u64)>,
}

impl BackdropTextures {
    /// The list for `images` (which only grow) and the movies at `version`,
    /// `movies` their [`MovieSlots::shown_ids`]; `plain` the plain
    /// backdrop's renderer id.
    pub fn get<I: IntoIterator<Item = (BgImage, usize)>>(
        &mut self,
        images: &[(BgImage, usize)],
        version: u64,
        movies: impl FnOnce() -> I,
        plain: usize,
    ) -> &[(BgImage, usize)] {
        let key = (images.len(), version);
        if self.key != Some(key) {
            self.list.clear();
            self.list.extend(images.iter().cloned());
            self.list.extend(movies());
            self.list.push((BgImage::Plain, plain));
            self.key = Some(key);
        }
        &self.list
    }
}

#[cfg(test)]
mod tests;
