//! Video decoding for background movies: the frames a decoder hands the
//! game loop and the traits the shells implement
//! (`docs/plans/video-backgrounds.md`). On the web a worker runs the FFmpeg
//! module (`video/`); a desktop shell would link FFmpeg itself. Nothing here
//! touches judging: movies follow the audio clock like still backgrounds.

use serde::{Deserialize, Serialize};

/// A movie file's container, as sniffed at import
/// (`ddi_library::video::sniff`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum VideoContainer {
    Avi,
    /// ISO media: MP4, QuickTime `.mov`, F4V, 3GP.
    Mp4,
    /// Matroska and WebM.
    Matroska,
    Ogg,
    /// ASF (`.wmv`).
    Asf,
    Flv,
    /// MPEG program stream (`.mpg`, `.mpeg`, VOB).
    MpegPs,
    /// MPEG transport stream.
    MpegTs,
    /// A raw MPEG-1/2 video stream (what packs often name `.avi`).
    MpegVideo,
    /// Not recognised; FFmpeg may still know it.
    Unknown,
}

/// A movie's video codec, as far as the first bytes tell. Named where the
/// choice of decoder (WebCodecs or the FFmpeg module) or the import report
/// needs it; anything else by FFmpeg's name or its fourcc.
#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum VideoCodec {
    H264,
    Hevc,
    Mpeg1,
    Mpeg2,
    /// MPEG-4 Part 2 (Xvid, DivX 4/5).
    Mpeg4,
    Vp8,
    Vp9,
    Av1,
    Other(String),
    /// The container was recognised but does not say yet (an MP4 whose
    /// index sits at the end, a stream without its header in the first
    /// bytes).
    Unknown,
}

/// What a movie file is.
#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct VideoFormat {
    pub container: VideoContainer,
    pub codec: VideoCodec,
}

impl VideoFormat {
    /// Whether the game can play it: the FFmpeg module has every video
    /// decoder FFmpeg builds in except AV1 (only hardware or libdav1d decode
    /// it; StepMania cannot either). Unknown formats are tried.
    pub fn playable(&self) -> bool {
        self.codec != VideoCodec::Av1
    }

    /// For the import report, e.g. "H.264 in AVI".
    pub fn describe(&self) -> String {
        let codec = match &self.codec {
            VideoCodec::H264 => "H.264",
            VideoCodec::Hevc => "HEVC",
            VideoCodec::Mpeg1 => "MPEG-1",
            VideoCodec::Mpeg2 => "MPEG-2",
            VideoCodec::Mpeg4 => "MPEG-4 Part 2",
            VideoCodec::Vp8 => "VP8",
            VideoCodec::Vp9 => "VP9",
            VideoCodec::Av1 => "AV1",
            VideoCodec::Other(name) => name.as_str(),
            VideoCodec::Unknown => "",
        };
        let container = match self.container {
            VideoContainer::Avi => "AVI",
            VideoContainer::Mp4 => "MP4",
            VideoContainer::Matroska => "Matroska",
            VideoContainer::Ogg => "Ogg",
            VideoContainer::Asf => "ASF",
            VideoContainer::Flv => "FLV",
            VideoContainer::MpegPs => "an MPEG program stream",
            VideoContainer::MpegTs => "an MPEG transport stream",
            VideoContainer::MpegVideo => "a raw MPEG stream",
            VideoContainer::Unknown => "",
        };
        match (codec.is_empty(), container.is_empty()) {
            (true, true) => "an unrecognised format".into(),
            (true, false) => container.to_string(),
            (false, true) => codec.to_string(),
            (false, false) => format!("{codec} in {container}"),
        }
    }
}

/// The YUV-to-RGB matrix of a frame.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum YuvMatrix {
    Bt601,
    Bt709,
}

impl YuvMatrix {
    /// Luma weights of red and blue (`Kr`, `Kb`).
    pub fn weights(self) -> (f32, f32) {
        match self {
            YuvMatrix::Bt601 => (0.299, 0.114),
            YuvMatrix::Bt709 => (0.2126, 0.0722),
        }
    }
}

/// A frame the platform keeps itself (a browser's frame on the GPU), drawn
/// without its pixels passing through here; the last clone dropped
/// releases it.
#[derive(Clone)]
pub struct ExternalFrame(pub std::rc::Rc<dyn std::any::Any>);

impl std::fmt::Debug for ExternalFrame {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("ExternalFrame")
    }
}

impl PartialEq for ExternalFrame {
    fn eq(&self, other: &Self) -> bool {
        std::rc::Rc::ptr_eq(&self.0, &other.0)
    }
}

/// One decoded frame as 8-bit YUV 4:2:0, or one the platform keeps
/// ([`VideoFrame::external`]: its planes are empty).
#[derive(Clone, Debug, PartialEq)]
pub struct VideoFrame {
    /// Seconds from the start of the movie, stamped as StepMania stamps
    /// frames (the decode time of the packet that released the frame), so
    /// streams with B-frames start a frame or two after zero.
    pub pts: f64,
    pub width: u32,
    pub height: u32,
    pub matrix: YuvMatrix,
    pub full_range: bool,
    /// Y (`width` × `height`), then U and V ([`chroma_size`] each), rows
    /// packed with no padding.
    pub planes: Vec<u8>,
    /// Milliseconds the decoder spent on this frame (diagnostics).
    pub decode_ms: f64,
    /// The frame as the platform keeps it, when it is not in `planes`
    /// (`width × height` is its size; matrix and range do not apply).
    pub external: Option<ExternalFrame>,
}

/// Size of each chroma plane of a 4:2:0 frame (odd sizes round up).
pub fn chroma_size(width: u32, height: u32) -> (u32, u32) {
    (width.div_ceil(2), height.div_ceil(2))
}

/// Bytes of a frame's planes.
pub fn planes_len(width: u32, height: u32) -> usize {
    let (cw, ch) = chroma_size(width, height);
    width as usize * height as usize + 2 * cw as usize * ch as usize
}

impl VideoFrame {
    /// The Y, U and V planes; `None` when `planes` has the wrong length.
    pub fn split(&self) -> Option<(&[u8], &[u8], &[u8])> {
        if self.planes.len() != planes_len(self.width, self.height) {
            return None;
        }
        let luma = self.width as usize * self.height as usize;
        let (cw, ch) = chroma_size(self.width, self.height);
        let chroma = cw as usize * ch as usize;
        let (y, rest) = self.planes.split_at(luma);
        let (u, v) = rest.split_at(chroma);
        Some((y, u, v))
    }

    /// The colour of pixel (`x`, `y`) as non-linear RGB in 0–1 (see
    /// [`yuv_to_rgb`]); `None` outside the frame or for malformed planes.
    pub fn rgb_at(&self, x: u32, y: u32) -> Option<[f32; 3]> {
        if x >= self.width || y >= self.height {
            return None;
        }
        let (yp, up, vp) = self.split()?;
        let (cw, _) = chroma_size(self.width, self.height);
        let c = (y / 2) as usize * cw as usize + (x / 2) as usize;
        Some(yuv_to_rgb(
            yp[y as usize * self.width as usize + x as usize],
            up[c],
            vp[c],
            self.matrix,
            self.full_range,
        ))
    }
}

impl VideoFrame {
    /// The frame as RGBA bytes (`width × height`, opaque), as the
    /// background shader draws it at its own size: chroma sampled
    /// bilinearly at the luma pixel's centre (centre-sited, edges clamped),
    /// then [`yuv_to_rgb`], rounded to 8 bits. `None` for malformed planes
    /// (or a frame the platform keeps). For a still that must match the
    /// movie's first drawn frame (the song's opening still).
    pub fn rgba(&self) -> Option<Vec<u8>> {
        let (yp, up, vp) = self.split()?;
        let (w, h) = (self.width as usize, self.height as usize);
        let (cw, ch) = chroma_size(self.width, self.height);
        let (cw, ch) = (cw as usize, ch as usize);
        // Where each luma column or row falls among the chroma texels
        // (texel centres at i + 0.5 in units of the chroma plane).
        let taps = |n: usize, cn: usize| -> Vec<(usize, usize, f32)> {
            (0..n)
                .map(|i| {
                    let at = ((i as f32 + 0.5) * cn as f32 / n as f32 - 0.5).max(0.0);
                    let lo = (at.floor() as usize).min(cn - 1);
                    let hi = (lo + 1).min(cn - 1);
                    (lo, hi, at - at.floor())
                })
                .collect()
        };
        let (xs, ys) = (taps(w, cw), taps(h, ch));
        let sample =
            |plane: &[u8], (x0, x1, fx): (usize, usize, f32), (y0, y1, fy): (usize, usize, f32)| {
                let p = |x: usize, y: usize| f32::from(plane[y * cw + x]);
                let top = p(x0, y0) + (p(x1, y0) - p(x0, y0)) * fx;
                let bottom = p(x0, y1) + (p(x1, y1) - p(x0, y1)) * fx;
                top + (bottom - top) * fy
            };
        let mut out = vec![0u8; w * h * 4];
        for (y, &ty) in ys.iter().enumerate() {
            for (x, &tx) in xs.iter().enumerate() {
                let rgb = yuv_to_rgb_f(
                    f32::from(yp[y * w + x]),
                    sample(up, tx, ty),
                    sample(vp, tx, ty),
                    self.matrix,
                    self.full_range,
                );
                let o = (y * w + x) * 4;
                for (c, v) in rgb.into_iter().enumerate() {
                    out[o + c] = (v * 255.0).round() as u8;
                }
                out[o + 3] = 255;
            }
        }
        Some(out)
    }
}

/// Largest luma standard deviation (of 255) of a picture that shows
/// nothing ([`blank_picture`]): a flat field with the codec's noise (a
/// dim silhouette, Bad Apple!!'s, spreads about 6).
pub const BLANK_SPREAD: f64 = 3.0;
/// Mean luma (of 255) at or below which a flat picture is black, or at or
/// above `255 −` this, white ([`blank_picture`]).
pub const BLANK_EDGE: f64 = 32.0;
/// Pixels further than this from the mean luma (of 255) ...
pub const BLANK_OUTLIER: f64 = 16.0;
/// ... may be at most this share of a blank picture (a small title on
/// black is a picture).
pub const BLANK_OUTLIERS: f64 = 0.005;

/// Whether an RGBA picture shows nothing: nearly uniform (luma spread at
/// most [`BLANK_SPREAD`], hardly a pixel more than [`BLANK_OUTLIER`] from
/// the mean) and near black or near white (mean luma within
/// [`BLANK_EDGE`] of either end), as the black or white frame many movies
/// open on. A song's opening still that is blank shows nothing of the song,
/// so the menus show its background image instead. Luma is BT.601's on
/// the encoded values; an empty picture is blank.
pub fn blank_picture(rgba: &[u8]) -> bool {
    let pixels = rgba.as_chunks::<4>().0;
    if pixels.is_empty() {
        return true;
    }
    let luma =
        |p: &[u8; 4]| 0.299 * f64::from(p[0]) + 0.587 * f64::from(p[1]) + 0.114 * f64::from(p[2]);
    let n = pixels.len() as f64;
    let (sum, squares) = pixels
        .iter()
        .map(luma)
        .fold((0.0, 0.0), |(s, q), y| (s + y, q + y * y));
    let mean = sum / n;
    let spread = (squares / n - mean * mean).max(0.0).sqrt();
    let outliers = pixels
        .iter()
        .filter(|p| (luma(p) - mean).abs() > BLANK_OUTLIER)
        .count() as f64;
    spread <= BLANK_SPREAD
        && outliers <= BLANK_OUTLIERS * n
        && (mean <= BLANK_EDGE || mean >= 255.0 - BLANK_EDGE)
}

/// YUV to non-linear (gamma-encoded, as the video carries it) RGB in 0–1,
/// clamped. This is the reference the background shader follows.
pub fn yuv_to_rgb(y: u8, u: u8, v: u8, matrix: YuvMatrix, full_range: bool) -> [f32; 3] {
    yuv_to_rgb_f(f32::from(y), f32::from(u), f32::from(v), matrix, full_range)
}

/// [`yuv_to_rgb`] of interpolated samples (0–255 each).
fn yuv_to_rgb_f(y: f32, u: f32, v: f32, matrix: YuvMatrix, full_range: bool) -> [f32; 3] {
    let (u, v) = (u - 128.0, v - 128.0);
    let (luma, cb, cr) = if full_range {
        (y / 255.0, u / 255.0, v / 255.0)
    } else {
        ((y - 16.0) / 219.0, u / 224.0, v / 224.0)
    };
    let (kr, kb) = matrix.weights();
    let r = luma + 2.0 * (1.0 - kr) * cr;
    let b = luma + 2.0 * (1.0 - kb) * cb;
    let g = (luma - kr * r - kb * b) / (1.0 - kr - kb);
    [r.clamp(0.0, 1.0), g.clamp(0.0, 1.0), b.clamp(0.0, 1.0)]
}

/// What an open movie reports once the decoder has read its header.
#[derive(Clone, Debug, PartialEq)]
pub struct VideoInfo {
    pub width: u32,
    pub height: u32,
    /// Length in seconds, `None` when the container only guesses it (a raw
    /// MPEG-2 stream): the end is then known when it is reached.
    pub duration: Option<f64>,
    /// The decoder's name for the codec (`h264`, `mpeg4`, `mpeg2video`).
    pub codec: String,
}

/// Something that happened to an open movie, in the order it happened.
#[derive(Clone, Debug, PartialEq)]
pub enum VideoEvent {
    Opened(VideoInfo),
    Frame(VideoFrame),
    /// No more frames until the next seek; `last_pts` is the last frame's.
    End {
        last_pts: Option<f64>,
    },
    /// The movie cannot be played (no decoder, unreadable file, crash).
    Error(String),
}

/// One open movie. Frames are decoded only as asked for, so the caller sets
/// the queue depth; dropping it closes the movie.
pub trait VideoDecoder {
    /// Moves to `t` seconds: the next frame is the one showing at `t` (a
    /// frame before a gap of dropped frames shows until the next one), and
    /// no frame decoded before the seek is delivered after it. Cancels the
    /// frames asked for and not delivered yet.
    fn seek(&mut self, t: f64);
    /// Asks for `n` more frames. [`VideoEvent::End`] cancels what is left
    /// of them, and is reported once until the next seek.
    fn want(&mut self, n: u32);
    /// Frames asked for and not delivered by [`poll`](Self::poll) yet.
    fn pending(&self) -> u32;
    /// Moves the events that arrived since the last call into `out`.
    fn poll(&mut self, out: &mut Vec<VideoEvent>);
}

/// Opens movies. `Source` is the shell's handle on a file (a `Blob` on the
/// web, a path on the desktop).
pub trait VideoBackend {
    type Source;
    type Decoder: VideoDecoder;

    /// Starts opening `source`; `name` is the file's name (its extension is
    /// a hint for the demuxer) and `format` what the import sniffed.
    fn open(&mut self, source: Self::Source, name: &str, format: &VideoFormat) -> Self::Decoder;
}

/// Where StepMania's movie clock goes after a movie's last frame when it
/// loops (`MovieTexture_Generic::UpdateFrame`, 5_1-new: "best effort"
/// 0.5 s, for the gap in looping preview music): later laps skip the first
/// half second.
pub const LOOP_RESTART: f64 = 0.5;

/// A decoded frame this far behind the clock counts as late (the decoder is
/// not keeping up).
pub const LATE_SECONDS: f64 = 0.5;

/// When the clock jumps this far ahead between two updates (the tab was in
/// the background, a long frame), seeking is quicker than decoding up to
/// it. A decoder that is merely slow is not helped by seeking: its frames
/// are shown as they come, counted late.
pub const SEEK_GAP_SECONDS: f64 = 2.0;

/// When the position goes back this far within a lap (a change that
/// starts the movie over while it plays, such as one on the other
/// background layer sharing its clock), the track seeks back to it: the
/// frames it holds are ahead. Less than this is the clock's jitter.
pub const SEEK_BACK_SECONDS: f64 = 0.1;

/// A run from a movie's start keeps its frames up to this many bytes; when
/// it reaches the end within them, every later lap plays from memory, as
/// StepMania loops without seeking (a seek per lap would be slower than a
/// lap of a short movie).
pub const LOOP_CACHE_BYTES: usize = 24 << 20;

/// A short movie started over behind the clock shows no frame further
/// behind it than this (s) while it catches up ([`MovieTrack`]).
pub const CATCH_UP_SECONDS: f64 = 0.1;

/// Counts for the debug state and the "cannot keep up" rule.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct MovieStats {
    pub decoded: u64,
    pub shown: u64,
    /// Decoded frames passed over without being shown (the clock was
    /// already past them).
    pub dropped: u64,
    /// Frames that arrived more than [`LATE_SECONDS`] after their time.
    pub late: u64,
    pub seeks: u64,
}

/// Plays one movie for the game loop: keeps frames decoded ahead, picks the
/// frame for a position on the movie's clock (the newest not after it; the
/// first frame of a run as soon as it arrives), loops or holds the last
/// frame at the end as StepMania does, and seeks when the clock jumps or
/// loops. Positions are in movie seconds before looping
/// (`ddi_library::backgrounds::movie_position`).
pub struct MovieTrack {
    looping: bool,
    ahead: u32,
    queue: std::collections::VecDeque<VideoFrame>,
    /// Where the decoder was last sent, in movie seconds.
    origin: f64,
    /// Which lap that was (0 before the first loop).
    lap: u64,
    /// The last frame's time, once known (from this track or an earlier
    /// one of the same movie).
    end: Option<f64>,
    /// No more frames until the next seek.
    ended: bool,
    /// A seek has gone out and nothing has come back yet: no other seek
    /// until it does (a slow seek must not be overtaken).
    seeking: bool,
    /// Time of the frame shown last in this run, `None` before the first.
    shown: Option<f64>,
    last_decoded: Option<f64>,
    /// Where the clock was at the previous update.
    previous: Option<f64>,
    /// Frames of a run from the start, while they fit [`LOOP_CACHE_BYTES`].
    collecting: Option<(Vec<VideoFrame>, usize)>,
    /// A short movie started over from 0 behind the clock (to keep every
    /// frame): its frames are decoded quickly up to where the clock is,
    /// and only the one due then is shown (not the run of stale ones, which
    /// would also count as late).
    catch_up: bool,
    /// The decoder hands over frames the platform keeps: they are never
    /// kept for later laps, so a lap never starts over from 0 for them.
    external_frames: bool,
    /// Every frame of the movie, once a run from the start reached the end
    /// within the budget: laps play from here.
    all_frames: Option<Vec<VideoFrame>>,
    info: Option<VideoInfo>,
    error: Option<String>,
    pub stats: MovieStats,
    events: Vec<VideoEvent>,
}

impl MovieTrack {
    /// `looping`: loop at the end (else hold the last frame); `ahead`:
    /// frames to keep decoded or asked for; `end`: the last frame's time if
    /// an earlier track of the movie learned it.
    pub fn new(looping: bool, ahead: u32, end: Option<f64>) -> MovieTrack {
        MovieTrack {
            looping,
            ahead: ahead.max(1),
            queue: std::collections::VecDeque::new(),
            origin: 0.0,
            lap: 0,
            end: end.filter(|e| e.is_finite() && *e >= 0.0),
            ended: false,
            seeking: false,
            shown: None,
            last_decoded: None,
            previous: None,
            collecting: None,
            catch_up: false,
            external_frames: false,
            all_frames: None,
            info: None,
            error: None,
            stats: MovieStats::default(),
            events: Vec::new(),
        }
    }

    pub fn info(&self) -> Option<&VideoInfo> {
        self.info.as_ref()
    }

    /// Loops at the end or holds the last frame, from now on (a later
    /// change of the movie may stop it looping).
    pub fn set_looping(&mut self, looping: bool) {
        self.looping = looping;
    }

    /// The last frame's time, once known: for the next track of the movie.
    pub fn end(&self) -> Option<f64> {
        self.end
    }

    /// Why the movie cannot play, once the decoder said so.
    pub fn error(&self) -> Option<&str> {
        self.error.as_deref()
    }

    /// Where `position` (before looping) is in the file, and on which lap.
    /// Until the end is known, the position itself.
    pub fn local(&self, position: f64) -> (f64, u64) {
        let position = if position.is_finite() {
            position.max(0.0)
        } else {
            0.0
        };
        let Some(end) = self.end.filter(|&e| position >= e) else {
            return (position, 0);
        };
        if !self.looping {
            return (end, 0);
        }
        // A movie shorter than the restart point starts over from 0.
        let restart = if end > LOOP_RESTART {
            LOOP_RESTART
        } else {
            0.0
        };
        let period = (end - restart).max(1e-3);
        let laps = ((position - end) / period).floor();
        let local = restart + (position - end - laps * period);
        (
            local.clamp(restart, end),
            (laps.min(1e15) as u64).saturating_add(1),
        )
    }

    /// Starts a run at `position`: the decoder seeks there and decodes
    /// ahead. For a change becoming current (or about to).
    pub fn start(&mut self, decoder: &mut impl VideoDecoder, position: f64) {
        let (local, lap) = self.local(position);
        self.shown = None;
        self.previous = None;
        if self.all_frames.is_some() {
            self.lap = lap;
            return;
        }
        self.seek(decoder, local, lap);
    }

    fn seek(&mut self, decoder: &mut impl VideoDecoder, local: f64, lap: u64) {
        decoder.seek(local);
        self.catch_up = false;
        self.origin = local;
        self.lap = lap;
        self.stats.dropped += self.queue.len() as u64;
        self.queue.clear();
        self.ended = false;
        self.seeking = true;
        self.last_decoded = None;
        // A run from the start may hold the whole movie.
        self.collecting = (local <= 1e-6).then(|| (Vec::new(), 0));
        self.stats.seeks += 1;
        decoder.want(self.ahead);
    }

    /// The frame to upload for `position` now, if another one is due. Keeps
    /// the decoder fed; never returns the same frame twice in a run.
    pub fn update(&mut self, decoder: &mut impl VideoDecoder, position: f64) -> Option<VideoFrame> {
        decoder.poll(&mut self.events);
        let now = self.local(position).0;
        for event in std::mem::take(&mut self.events) {
            match event {
                VideoEvent::Opened(info) => self.info = Some(info),
                VideoEvent::Frame(frame) => {
                    self.stats.decoded += 1;
                    if now - frame.pts > LATE_SECONDS && !self.catch_up {
                        self.stats.late += 1;
                    }
                    self.seeking = false;
                    self.last_decoded = Some(frame.pts);
                    // A frame the platform keeps holds a decoder's buffer:
                    // never kept for later laps.
                    if frame.external.is_some() {
                        self.external_frames = true;
                        self.collecting = None;
                    }
                    if let Some((frames, bytes)) = self.collecting.as_mut() {
                        if *bytes + frame.planes.len() <= LOOP_CACHE_BYTES {
                            *bytes += frame.planes.len();
                            frames.push(frame.clone());
                        } else {
                            self.collecting = None;
                        }
                    }
                    self.queue.push_back(frame);
                }
                VideoEvent::End { last_pts } => {
                    self.ended = true;
                    self.seeking = false;
                    // The decoder reports the file's last frame even when a
                    // seek past it decoded it without handing it over.
                    if let Some(last) = last_pts.or(self.last_decoded).filter(|l| l.is_finite()) {
                        self.end = Some(last);
                    }
                    if let Some((frames, _)) = self.collecting.take()
                        && !frames.is_empty()
                    {
                        self.all_frames = Some(frames);
                    }
                }
                VideoEvent::Error(e) => self.error = Some(e),
            }
        }
        if self.error.is_some() {
            return None;
        }
        let (local, lap) = self.local(position);
        if let Some(frames) = &self.all_frames {
            return self.frame_in_memory(frames.len(), local, lap);
        }
        if !self.seeking {
            if lap != self.lap && self.looping {
                // A short movie starts over from 0 so that the run keeps
                // every frame and later laps need no seek.
                let short = !self.external_frames && self.end.is_some_and(|e| e <= 10.0);
                self.seek(decoder, if short { 0.0 } else { local }, lap);
                self.catch_up = short && local > CATCH_UP_SECONDS;
            } else {
                let jumped = self
                    .previous
                    .is_some_and(|p| local - p > SEEK_GAP_SECONDS && lap == self.lap);
                let back = self
                    .previous
                    .is_some_and(|p| p - local > SEEK_BACK_SECONDS && lap == self.lap);
                let before = local + 1e-6 < self.origin;
                if (jumped && !self.ended) || before || back {
                    self.seek(decoder, local, lap);
                }
            }
        }
        self.previous = Some(local);
        let due = self.queue.iter().rposition(|f| f.pts <= local + 1e-9);
        let frame = match due {
            // Catching up: frames well behind the clock go unshown.
            Some(i) if self.catch_up && local - self.queue[i].pts > CATCH_UP_SECONDS => {
                self.stats.dropped += i as u64 + 1;
                self.queue.drain(..=i);
                None
            }
            Some(i) => {
                self.catch_up = false;
                self.stats.dropped += i as u64;
                self.queue.drain(..i);
                self.queue.pop_front()
            }
            // StepMania shows a run's first frame as soon as it is decoded.
            None if self.shown.is_none() && !self.seeking && !self.catch_up => {
                self.queue.pop_front()
            }
            None => None,
        };
        if let Some(f) = &frame {
            self.shown = Some(f.pts);
            self.stats.shown += 1;
        }
        // Catching up, more at once.
        let ahead = if self.catch_up {
            self.ahead * 4
        } else {
            self.ahead
        };
        let have = self.queue.len() as u32 + decoder.pending();
        if !self.ended && have < ahead {
            decoder.want(ahead - have);
        }
        frame
    }

    /// The frame due at `local` from the frames kept in memory.
    fn frame_in_memory(&mut self, count: usize, local: f64, lap: u64) -> Option<VideoFrame> {
        let frames = self.all_frames.as_ref()?;
        let i = frames
            .iter()
            .rposition(|f| f.pts <= local + 1e-9)
            .unwrap_or(0)
            .min(count - 1);
        let pts = frames[i].pts;
        self.lap = lap;
        // The same picture again (a one-frame movie, or one lap ending on
        // the frame the next starts with) needs no upload.
        if self.shown == Some(pts) {
            return None;
        }
        self.shown = Some(pts);
        self.stats.shown += 1;
        Some(frames[i].clone())
    }
}

/// Why the automatic video setting turned a movie off.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StopReason {
    /// The display fell behind its refresh rate while the movie played.
    DroppedFrames,
    /// The decoder's frames came too late to be shown on time.
    DecoderBehind,
}

/// A movie plays this long (ms of wall time) before [`VideoGuard`] judges
/// it: textures and the decoder warm up.
pub const GUARD_WARMUP_MS: f64 = 2000.0;
/// A measured second below this share of the refresh rate counts as slow.
pub const GUARD_SLOW_SHARE: f64 = 0.8;
/// Consecutive slow seconds that turn the movie off.
pub const GUARD_SLOW_SECONDS: u32 = 3;
/// A measured second with this many late frames counts as behind ...
pub const GUARD_LATE_FRAMES: u64 = 2;
/// ... and this many behind seconds in a row turn the movie off (a single
/// stall, a collection pause, makes one burst of late frames, not two).
pub const GUARD_BEHIND_SECONDS: u32 = 2;
/// A measured second holding a frame this long (ms) had a one-off stall,
/// not a steady loss: the tab was hidden, the module compiled, a
/// collection paused the page, a seek caught up. The movie warms up again
/// from there, so the stall and what follows it count for nothing.
pub const GUARD_HITCH_MS: f64 = 250.0;

/// One measuring second of the display, for [`VideoGuard::update`].
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Second {
    /// Frames per second over it.
    pub fps: f64,
    /// Its longest frame interval, ms.
    pub worst_ms: f64,
}

/// Of a trial pause ([`VideoGuard`]), seconds judged at most, and its
/// length in wall time (ms): undecided by then (a hidden tab, stalls), the
/// movie plays on ...
pub const GUARD_TRIAL_SECONDS: u32 = 3;
pub const GUARD_TRIAL_MS: f64 = 3000.0;
/// ... and how many of them must keep up for the movie to be the cause:
/// at [`GUARD_SLOW_SHARE`] of the refresh rate or more, and faster than the
/// slow seconds that started the trial by this share of it (so a display
/// hovering at the threshold, busy with something else, is not blamed on
/// the movie by noise).
pub const GUARD_TRIAL_GOOD: u32 = 2;
pub const GUARD_TRIAL_GAIN: f64 = 0.15;

/// What [`VideoGuard::update`] asks of the movies.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum GuardAction {
    /// Pause them (decoding and uploads stop; the frame shown stays) to
    /// see whether the display keeps up without them.
    Pause,
    /// Play on: the display fell behind with them paused too, so the
    /// dropped frames were not theirs.
    Resume,
    /// Turn them off for the rest of the play.
    Off(StopReason),
}

/// The rule of the automatic video setting
/// (`docs/plans/video-backgrounds.md`, step 5): while a movie is drawn,
/// past its first [`GUARD_WARMUP_MS`], two seconds in a row with late
/// frames turn it off for the rest of the song (the decoder cannot keep
/// up). Three seconds in a row below [`GUARD_SLOW_SHARE`] of the refresh
/// rate pause it for a trial: when the display then keeps up
/// ([`GUARD_TRIAL_GOOD`] of [`GUARD_TRIAL_SECONDS`], clearly faster than
/// before: [`GUARD_TRIAL_GAIN`]), the movie was the cause and goes off; when it does not, the machine is busy with
/// something else, the movie plays on, and dropped frames are not held
/// against it for the rest of the play. Only sustained loss counts: a
/// second with a stall ([`GUARD_HITCH_MS`]) starts the warm-up again; in
/// a trial it counts as not keeping up, and the trial ends after
/// [`GUARD_TRIAL_MS`] whatever came.
#[derive(Clone, Debug, Default)]
pub struct VideoGuard {
    /// What is drawn and since when (ms); a new movie or segment starts the
    /// warm-up again.
    shown: Option<(usize, f64)>,
    /// Late frames counted when the current second began.
    late_base: Option<u64>,
    slow_seconds: u32,
    behind_seconds: u32,
    /// The slow seconds counted, their frame rates summed.
    slow_fps: f64,
    /// A trial pause: `(since when (ms), seconds judged, seconds that
    /// kept up, the frame rate of the slow seconds before it)`.
    trial: Option<(f64, u32, u32, f64)>,
    /// A trial found the display slow without the movie: dropped frames
    /// are no longer the movie's.
    excused: bool,
}

impl VideoGuard {
    /// One display frame at `now_ms`: `shown` identifies the movie being
    /// drawn (`None`: none), `second` is a measuring second that ended with
    /// this frame, `refresh` the display's rate and `late` the late frames
    /// counted so far ([`MovieStats::late`]).
    pub fn update(
        &mut self,
        now_ms: f64,
        shown: Option<usize>,
        second: Option<Second>,
        refresh: f64,
        late: u64,
    ) -> Option<GuardAction> {
        let keeps_up = |s: Second| s.fps >= GUARD_SLOW_SHARE * refresh;
        if let Some((since, judged, good, before)) = self.trial.as_mut() {
            if let Some(s) = second {
                *judged += 1;
                let gained = s.fps >= *before + GUARD_TRIAL_GAIN * refresh;
                *good += u32::from(s.worst_ms <= GUARD_HITCH_MS && keeps_up(s) && gained);
            }
            let (judged, good) = (*judged, *good);
            if good >= GUARD_TRIAL_GOOD {
                self.trial = None;
                return Some(GuardAction::Off(StopReason::DroppedFrames));
            }
            if judged - good > GUARD_TRIAL_SECONDS - GUARD_TRIAL_GOOD
                || now_ms - *since >= GUARD_TRIAL_MS
            {
                *self = VideoGuard {
                    excused: true,
                    ..VideoGuard::default()
                };
                return Some(GuardAction::Resume);
            }
            return None;
        }
        let fresh = |shown: Option<(usize, f64)>, excused: bool| VideoGuard {
            shown,
            excused,
            ..VideoGuard::default()
        };
        let Some(shown) = shown else {
            *self = fresh(None, self.excused);
            return None;
        };
        let hitch = second.is_some_and(|s| s.worst_ms > GUARD_HITCH_MS);
        if hitch || self.shown.is_none_or(|(s, _)| s != shown) {
            *self = fresh(Some((shown, now_ms)), self.excused);
        }
        let since = self.shown.map_or(now_ms, |(_, t)| t);
        if now_ms - since < GUARD_WARMUP_MS {
            return None;
        }
        let Some(s) = second else {
            // The first second judged starts after the warm-up.
            self.late_base.get_or_insert(late);
            return None;
        };
        let base = self.late_base.replace(late)?;
        let count = |n: &mut u32, bad: bool| *n = if bad { *n + 1 } else { 0 };
        count(&mut self.slow_seconds, !keeps_up(s));
        self.slow_fps = if self.slow_seconds == 0 {
            0.0
        } else {
            self.slow_fps + s.fps
        };
        count(
            &mut self.behind_seconds,
            late.saturating_sub(base) >= GUARD_LATE_FRAMES,
        );
        if self.behind_seconds >= GUARD_BEHIND_SECONDS {
            Some(GuardAction::Off(StopReason::DecoderBehind))
        } else if self.slow_seconds >= GUARD_SLOW_SECONDS && !self.excused {
            self.trial = Some((now_ms, 0, 0, self.slow_fps / f64::from(self.slow_seconds)));
            Some(GuardAction::Pause)
        } else {
            None
        }
    }

    /// Where the rule stands, for the debug state: `watching`, `trial`
    /// (the movie paused to see whether the display keeps up without it)
    /// or `excused` (it did not: dropped frames are not the movie's).
    pub fn state(&self) -> &'static str {
        match (self.trial.is_some(), self.excused) {
            (true, _) => "trial",
            (false, true) => "excused",
            _ => "watching",
        }
    }
}

/// Luma at or below this (limited range; scaled for full range) is black
/// for [`picture_bounds`]: bars are coded black with some noise.
const BAR_LUMA: u32 = 16 + 12;

/// The picture inside a movie's own black bars (pillarboxed or letterboxed
/// into its frame): `(x, y, width, height)` of the union of the non-black
/// areas of sampled frames' Y planes (`width × height` each), or `None`
/// when every frame is black (fade-ins are skipped; dark scenes are covered
/// by the other samples). A row or column is black when its mean is at
/// most [`BAR_LUMA`] and no pixel is far above it.
pub fn picture_bounds(
    frames: &[&[u8]],
    width: u32,
    height: u32,
    full_range: bool,
) -> Option<(u32, u32, u32, u32)> {
    let (w, h) = (width as usize, height as usize);
    if w == 0 || h == 0 {
        return None;
    }
    let (mean_max, peak_max) = if full_range {
        (u32::from(BAR_LUMA as u8 - 16) * 255 / 219, 64 * 255 / 219)
    } else {
        (BAR_LUMA, 80)
    };
    let black = |samples: &mut dyn Iterator<Item = u8>| {
        let (mut sum, mut n, mut peak) = (0u32, 0u32, 0u8);
        for s in samples {
            sum += u32::from(s);
            n += 1;
            peak = peak.max(s);
        }
        n == 0 || (sum <= mean_max * n && u32::from(peak) <= peak_max)
    };
    let mut union: Option<(usize, usize, usize, usize)> = None;
    for y_plane in frames {
        if y_plane.len() < w * h {
            continue;
        }
        let row = |r: usize| black(&mut y_plane[r * w..(r + 1) * w].iter().copied());
        let Some(top) = (0..h).find(|&r| !row(r)) else {
            continue;
        };
        let bottom = (0..h).rev().find(|&r| !row(r)).unwrap_or(top);
        let column = |c: usize| black(&mut (top..=bottom).map(|r| y_plane[r * w + c]));
        let left = (0..w).find(|&c| !column(c)).unwrap_or(0);
        let right = (0..w).rev().find(|&c| !column(c)).unwrap_or(w - 1);
        union = Some(match union {
            None => (left, top, right, bottom),
            Some((l, t, r, b)) => (l.min(left), t.min(top), r.max(right), b.max(bottom)),
        });
    }
    let (l, t, r, b) = union?;
    Some((l as u32, t as u32, (r - l + 1) as u32, (b - t + 1) as u32))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn close(a: [f32; 3], b: [f32; 3]) -> bool {
        a.iter().zip(b).all(|(x, y)| (x - y).abs() < 0.02)
    }

    #[test]
    fn limited_and_full_range_black_and_white() {
        for m in [YuvMatrix::Bt601, YuvMatrix::Bt709] {
            assert_eq!(yuv_to_rgb(16, 128, 128, m, false), [0.0, 0.0, 0.0]);
            assert_eq!(yuv_to_rgb(235, 128, 128, m, false), [1.0, 1.0, 1.0]);
            assert_eq!(yuv_to_rgb(0, 128, 128, m, true), [0.0, 0.0, 0.0]);
            assert_eq!(yuv_to_rgb(255, 128, 128, m, true), [1.0, 1.0, 1.0]);
            // Below black and above white clamp.
            assert_eq!(yuv_to_rgb(0, 128, 128, m, false), [0.0, 0.0, 0.0]);
            assert_eq!(yuv_to_rgb(255, 128, 128, m, false), [1.0, 1.0, 1.0]);
        }
    }

    #[test]
    fn primaries_in_each_matrix() {
        // Limited-range codes of the primaries (Rec. 601 and Rec. 709).
        assert!(close(
            yuv_to_rgb(81, 90, 240, YuvMatrix::Bt601, false),
            [1.0, 0.0, 0.0]
        ));
        assert!(close(
            yuv_to_rgb(145, 54, 34, YuvMatrix::Bt601, false),
            [0.0, 1.0, 0.0]
        ));
        assert!(close(
            yuv_to_rgb(41, 240, 110, YuvMatrix::Bt601, false),
            [0.0, 0.0, 1.0]
        ));
        assert!(close(
            yuv_to_rgb(63, 102, 240, YuvMatrix::Bt709, false),
            [1.0, 0.0, 0.0]
        ));
        assert!(close(
            yuv_to_rgb(173, 42, 26, YuvMatrix::Bt709, false),
            [0.0, 1.0, 0.0]
        ));
        assert!(close(
            yuv_to_rgb(32, 240, 118, YuvMatrix::Bt709, false),
            [0.0, 0.0, 1.0]
        ));
        // The same red read with the wrong matrix is visibly off.
        assert!(!close(
            yuv_to_rgb(63, 102, 240, YuvMatrix::Bt601, false),
            [1.0, 0.0, 0.0]
        ));
    }

    #[test]
    fn formats_describe_and_play() {
        let h264 = VideoFormat {
            container: VideoContainer::Avi,
            codec: VideoCodec::H264,
        };
        assert_eq!(h264.describe(), "H.264 in AVI");
        assert!(h264.playable());
        let av1 = VideoFormat {
            container: VideoContainer::Mp4,
            codec: VideoCodec::Av1,
        };
        assert!(!av1.playable());
        let unknown = VideoFormat {
            container: VideoContainer::Unknown,
            codec: VideoCodec::Unknown,
        };
        assert_eq!(unknown.describe(), "an unrecognised format");
        assert!(unknown.playable());
        // The manifest's wire format.
        assert_eq!(
            serde_json::to_string(&h264).unwrap(),
            r#"{"container":"avi","codec":"h264"}"#
        );
        let other = VideoFormat {
            container: VideoContainer::MpegPs,
            codec: VideoCodec::Other("wmv3".into()),
        };
        let json = serde_json::to_string(&other).unwrap();
        assert_eq!(json, r#"{"container":"mpegps","codec":{"other":"wmv3"}}"#);
        assert_eq!(serde_json::from_str::<VideoFormat>(&json).unwrap(), other);
    }

    /// A movie of `frames` frames at 30 fps, stamped `lag` frames late as
    /// the module stamps B-frame streams; frames come back `delay` polls
    /// after they are asked for.
    struct FakeDecoder {
        frames: usize,
        lag: usize,
        delay: usize,
        plane_len: usize,
        next: usize,
        asked: u32,
        in_flight: Vec<(usize, usize)>,
        ended: bool,
        seeks: Vec<f64>,
        events: Vec<VideoEvent>,
        /// Frames the platform keeps (a browser's GPU frames).
        external: bool,
    }

    impl FakeDecoder {
        fn new(frames: usize, lag: usize, delay: usize) -> FakeDecoder {
            FakeDecoder {
                frames,
                lag,
                delay,
                plane_len: 6,
                next: 0,
                asked: 0,
                in_flight: Vec::new(),
                ended: false,
                seeks: Vec::new(),
                events: Vec::new(),
                external: false,
            }
        }

        fn pts(&self, i: usize) -> f64 {
            (i + self.lag) as f64 / 30.0
        }
    }

    impl VideoDecoder for FakeDecoder {
        fn seek(&mut self, t: f64) {
            self.seeks.push(t);
            self.in_flight.clear();
            self.asked = 0;
            self.ended = false;
            // The frame showing at `t`; past the last frame's period, the end
            // (the shim decodes up to it and reports its time at the end).
            self.next = if t >= self.pts(self.frames - 1) + 1.0 / 30.0 {
                self.frames
            } else {
                (0..self.frames)
                    .rev()
                    .find(|&i| self.pts(i) <= t + 1e-9)
                    .unwrap_or(0)
            };
        }

        fn want(&mut self, n: u32) {
            self.asked += n;
        }

        fn pending(&self) -> u32 {
            self.asked
        }

        fn poll(&mut self, out: &mut Vec<VideoEvent>) {
            out.append(&mut self.events);
            let mut end = None;
            while self.asked > 0 && !self.ended {
                if self.next >= self.frames {
                    self.ended = true;
                    self.asked = 0;
                    end = Some(VideoEvent::End {
                        last_pts: Some(self.pts(self.frames - 1)),
                    });
                    break;
                }
                self.in_flight.push((self.next, self.delay));
                self.next += 1;
                self.asked -= 1;
            }
            let mut ready = Vec::new();
            self.in_flight.retain_mut(|(i, wait)| {
                if *wait == 0 {
                    ready.push(*i);
                    false
                } else {
                    *wait -= 1;
                    true
                }
            });
            let ready_frames = ready;
            for i in ready_frames {
                out.push(VideoEvent::Frame(VideoFrame {
                    pts: self.pts(i),
                    width: 2,
                    height: 2,
                    matrix: YuvMatrix::Bt601,
                    full_range: false,
                    planes: if self.external {
                        Vec::new()
                    } else {
                        vec![i as u8; self.plane_len]
                    },
                    decode_ms: 0.0,
                    external: self
                        .external
                        .then(|| ExternalFrame(std::rc::Rc::new(i as u8))),
                }));
            }
            // As the worker posts them: the end after the frames before it,
            // once nothing is still on its way.
            if let Some(end) = end {
                if self.in_flight.is_empty() {
                    out.push(end);
                } else {
                    self.events.push(end);
                }
            }
        }
    }

    /// The frame numbers shown at each of `positions`, `None` where no new
    /// frame was due.
    fn play(track: &mut MovieTrack, d: &mut FakeDecoder, positions: &[f64]) -> Vec<Option<u8>> {
        positions
            .iter()
            .map(|&p| {
                track.update(d, p).map(|f| match &f.external {
                    Some(e) => *e.0.downcast_ref::<u8>().expect("a fake frame"),
                    None => f.planes[0],
                })
            })
            .collect()
    }

    fn display_frames(from: f64, to: f64) -> Vec<f64> {
        let n = ((to - from) * 60.0).round() as usize;
        (0..=n).map(|i| from + i as f64 / 60.0).collect()
    }

    #[test]
    fn a_track_shows_the_newest_frame_due_and_the_first_at_once() {
        let mut d = FakeDecoder::new(60, 2, 0);
        let mut track = MovieTrack::new(true, 3, None);
        track.start(&mut d, 0.0);
        let shown = play(&mut track, &mut d, &display_frames(0.0, 0.5));
        // Frame 0 at once, though stamped at 2/30 s; then each frame from
        // its time on, every other 60 Hz display frame.
        assert_eq!(shown[0], Some(0));
        assert_eq!(shown[1..4], [None, None, None]);
        // At 4/60 = 2/30 s frame 0 is due, but it was shown: frame 1 waits
        // for 3/30 s (6/60).
        assert_eq!(shown[6], Some(1));
        assert_eq!(shown[8], Some(2));
        assert_eq!(shown[30], Some(13));
        assert_eq!(track.stats.dropped, 0);
        assert_eq!(track.stats.late, 0);
        assert!(d.pending() + track.queue.len() as u32 <= 3);
    }

    #[test]
    fn a_track_loops_like_stepmania_and_holds_without_looping() {
        // 30 frames, the last at 29/30 s.
        let mut d = FakeDecoder::new(30, 0, 0);
        let mut track = MovieTrack::new(true, 3, None);
        track.start(&mut d, 0.0);
        play(&mut track, &mut d, &display_frames(0.0, 0.95));
        // Past the end: the clock goes on from 0.5 s.
        let end = 29.0 / 30.0;
        assert_eq!(track.local(end + 0.1).0, LOOP_RESTART + 0.1);
        assert_eq!(track.local(end + 0.1).1, 1);
        // A short movie decoded from its start loops from memory: the frame
        // showing at 0.6 s (18/30) at once, with no seek.
        let shown = play(&mut track, &mut d, &[end + 0.1]);
        assert_eq!(d.seeks, vec![0.0]);
        assert_eq!(shown, vec![Some(18)]);
        // A second lap: the period is end - 0.5.
        assert_eq!(track.local(end + (end - 0.5) + 0.05).1, 2);

        let mut d = FakeDecoder::new(30, 0, 0);
        let mut held = MovieTrack::new(false, 3, None);
        held.start(&mut d, 0.0);
        play(&mut held, &mut d, &display_frames(0.0, 1.0));
        let after = play(&mut held, &mut d, &display_frames(1.0, 2.0));
        assert!(after.iter().all(Option::is_none), "the last frame stays");
        assert_eq!(held.local(5.0), (end, 0));
        assert_eq!(d.seeks.len(), 1);
    }

    #[test]
    fn a_track_seeks_on_jumps_and_counts_late_frames() {
        let mut d = FakeDecoder::new(600, 0, 0);
        let mut track = MovieTrack::new(true, 3, None);
        track.start(&mut d, 0.0);
        play(&mut track, &mut d, &[0.0, 0.1]);
        // Ten seconds on: seek rather than decode through.
        let shown = play(&mut track, &mut d, &[10.0, 10.0]);
        assert_eq!(d.seeks.last().copied(), Some(10.0));
        assert_eq!(shown, vec![None, Some(44)]); // frame 300 = 300 mod 256
        // A decoder that answers late: frames arrive behind the clock.
        let mut slow = FakeDecoder::new(600, 0, 40);
        let mut track = MovieTrack::new(true, 3, None);
        track.start(&mut slow, 0.0);
        play(&mut track, &mut slow, &display_frames(0.0, 1.5));
        assert!(track.stats.late > 0);
        assert!(track.stats.dropped > 0);
    }

    #[test]
    fn a_track_seeks_back_when_its_movie_starts_over() {
        // Started at 0, played to 5 s, then the shared clock starts it over
        // (a restarting change on the other layer): 0.5 s.
        let mut d = FakeDecoder::new(600, 0, 0);
        let mut track = MovieTrack::new(true, 3, None);
        track.start(&mut d, 0.0);
        play(&mut track, &mut d, &display_frames(0.0, 5.0));
        let shown = play(&mut track, &mut d, &display_frames(0.5, 1.0));
        assert_eq!(d.seeks.last().copied(), Some(0.5));
        // Frames come again from 0.5 s (frame 15) on.
        assert_eq!(shown.iter().flatten().next(), Some(&15));
        assert!(shown.iter().flatten().count() >= 14, "{shown:?}");
        // A small step back (the clock's jitter) does not seek.
        let seeks = d.seeks.len();
        play(&mut track, &mut d, &[0.95, 1.0]);
        assert_eq!(d.seeks.len(), seeks);
    }

    #[test]
    fn a_track_loops_a_big_movie_by_seeking_and_a_tiny_one_from_memory() {
        // 2 s of 1 MB frames: more than the cache holds.
        let mut d = FakeDecoder::new(60, 0, 0);
        d.plane_len = 1 << 20;
        let mut track = MovieTrack::new(true, 3, None);
        track.start(&mut d, 0.0);
        let first = play(&mut track, &mut d, &display_frames(0.0, 2.0));
        assert!(first.iter().flatten().count() > 50);
        // Past the end: a seek back to 0 (a short movie), then frames again.
        let later = play(&mut track, &mut d, &display_frames(2.1, 3.0));
        assert!(d.seeks.len() >= 2 && d.seeks[1] == 0.0, "{:?}", d.seeks);
        assert!(later.iter().flatten().count() > 20);
        // A one-frame movie: shown once, then nothing to seek for.
        let mut one = FakeDecoder::new(1, 0, 0);
        let mut track = MovieTrack::new(true, 3, None);
        track.start(&mut one, 0.0);
        let shown = play(&mut track, &mut one, &display_frames(0.0, 3.0));
        assert_eq!(shown.iter().flatten().collect::<Vec<_>>(), vec![&0]);
        assert_eq!(one.seeks.len(), 1);
    }

    #[test]
    fn a_short_loop_joined_past_its_end_shows_no_stale_frames() {
        // 20 frames (0.63 s at 30 fps), joined 3.2 s in, end unknown: the
        // seek past the end teaches it, the track starts over from 0 to keep
        // the run, and catches up without showing what is behind the clock.
        let mut d = FakeDecoder::new(20, 0, 0);
        let mut track = MovieTrack::new(true, 3, None);
        track.start(&mut d, 3.2);
        let positions = display_frames(3.2, 4.2);
        let shown = play(&mut track, &mut d, &positions);
        let end = track.end().expect("learned");
        let mut first = None;
        for (p, f) in positions.iter().zip(&shown) {
            let Some(i) = f else { continue };
            let pts = f64::from(*i) / 30.0;
            let (local, _) = track.local(*p);
            // Within the catch-up margin (or just wrapped to the restart).
            assert!(
                local - pts <= CATCH_UP_SECONDS + 1.0 / 30.0 || local < LOOP_RESTART + 0.1,
                "frame {i} at local {local} (end {end})"
            );
            first.get_or_insert(*p);
        }
        assert!(first.is_some_and(|p| p < 3.35), "{first:?}");
        assert_eq!(track.stats.late, 0);
    }

    #[test]
    fn frames_the_platform_keeps_are_never_kept_for_later_laps() {
        // A short movie loops from memory, once its frames are its own…
        let mut own = FakeDecoder::new(20, 0, 0);
        let mut track = MovieTrack::new(true, 3, None);
        track.start(&mut own, 0.0);
        let _ = play(&mut track, &mut own, &display_frames(0.0, 3.0));
        assert_eq!(own.seeks.len(), 1);
        // …but a browser's frames hold its decoder's buffers: each lap
        // decodes again.
        let mut kept = FakeDecoder::new(20, 0, 0);
        kept.external = true;
        let mut track = MovieTrack::new(true, 3, None);
        track.start(&mut kept, 0.0);
        let shown = play(&mut track, &mut kept, &display_frames(0.0, 3.0));
        assert!(kept.seeks.len() >= 3, "{:?}", kept.seeks);
        assert!(shown.iter().flatten().count() > 60);
    }

    #[test]
    fn a_track_resumed_past_the_end_wraps_instead_of_stalling() {
        // A 5 s movie (150 frames, the last at 149/30 s) resumed at 12 s.
        let end = 149.0 / 30.0;
        // Without knowing its end: the seek finds the end, then it wraps.
        let mut d = FakeDecoder::new(150, 0, 0);
        let mut track = MovieTrack::new(true, 3, None);
        track.start(&mut d, 12.0);
        let shown = play(&mut track, &mut d, &display_frames(12.0, 13.0));
        assert_eq!(track.end(), Some(end));
        // It starts over from 0 to keep the run and catches up unseen.
        assert!(shown.iter().flatten().count() > 15, "{shown:?}");
        // Knowing it (an earlier track learned it): straight to the frame.
        let mut d = FakeDecoder::new(150, 0, 0);
        let mut track = MovieTrack::new(true, 3, Some(end));
        track.start(&mut d, 12.0);
        let (local, _) = track.local(12.0);
        assert_eq!(d.seeks, vec![local]);
        let shown = play(&mut track, &mut d, &[12.0, 12.0]);
        assert_eq!(shown[0], Some((local * 30.0 + 1e-9).floor() as u8));
    }

    #[test]
    fn a_slow_seek_is_not_overtaken() {
        // Each answer takes 150 polls (2.5 s at 60 Hz), longer than the
        // seek gap: it must not seek again before the first one answers.
        let mut d = FakeDecoder::new(1200, 0, 150);
        let mut track = MovieTrack::new(true, 3, None);
        track.start(&mut d, 0.0);
        let shown = play(&mut track, &mut d, &display_frames(0.0, 10.0));
        assert!(shown.iter().flatten().count() > 0);
        assert!(d.seeks.len() <= 4, "{} seeks", d.seeks.len());
    }

    #[test]
    fn absurd_positions_do_not_panic() {
        let mut track = MovieTrack::new(true, 3, Some(2.0));
        for p in [1e300, f64::INFINITY, f64::NAN, -5.0] {
            let (local, _) = track.local(p);
            assert!(local.is_finite() && (0.0..=2.0).contains(&local));
        }
        let mut d = FakeDecoder::new(60, 0, 0);
        track.start(&mut d, f64::INFINITY);
        play(&mut track, &mut d, &[1e300, f64::NAN]);
    }

    #[test]
    fn a_track_reports_decoder_errors() {
        let mut d = FakeDecoder::new(30, 0, 0);
        d.events.push(VideoEvent::Error("broken".into()));
        let mut track = MovieTrack::new(true, 3, None);
        track.start(&mut d, 0.0);
        assert_eq!(track.update(&mut d, 0.0), None);
        assert_eq!(track.error(), Some("broken"));
    }

    #[test]
    fn odd_sizes_round_chroma_up() {
        assert_eq!(chroma_size(64, 48), (32, 24));
        assert_eq!(chroma_size(161, 91), (81, 46));
        assert_eq!(planes_len(161, 91), 161 * 91 + 2 * 81 * 46);
        assert_eq!(planes_len(1, 1), 3);
    }

    #[test]
    fn planes_split_and_sample() {
        // 3×3: Y 9 bytes, U and V 2×2 each.
        let mut planes: Vec<u8> = (0..9).map(|i| 16 + i * 20).collect();
        planes.extend([128, 128, 128, 240]); // U
        planes.extend([128, 128, 128, 240]); // V
        let frame = VideoFrame {
            pts: 0.0,
            width: 3,
            height: 3,
            matrix: YuvMatrix::Bt601,
            full_range: false,
            planes,
            decode_ms: 0.0,
            external: None,
        };
        let (y, u, v) = frame.split().unwrap();
        assert_eq!((y.len(), u.len(), v.len()), (9, 4, 4));
        assert_eq!(frame.rgb_at(0, 0), Some([0.0, 0.0, 0.0]));
        // The bottom-right pixel reads the last chroma sample.
        let [r, _, b] = frame.rgb_at(2, 2).unwrap();
        assert!(r > 0.9 && b > 0.9);
        assert_eq!(frame.rgb_at(3, 0), None);
        let short = VideoFrame {
            planes: vec![0; 5],
            ..frame
        };
        assert_eq!(short.split(), None);
        assert_eq!(short.rgba(), None);
    }

    #[test]
    fn blank_pictures_are_flat_and_near_black_or_white() {
        let flat = |v: u8| [v, v, v, 255].repeat(100);
        assert!(blank_picture(&flat(0)));
        assert!(blank_picture(&flat(16)));
        assert!(blank_picture(&flat(250)));
        assert!(blank_picture(&[]));
        // A flat mid grey or colour is a picture (a title card).
        assert!(!blank_picture(&flat(128)));
        assert!(!blank_picture(&[200, 30, 30, 255].repeat(100)));
        // Black with the codec's noise is still blank ...
        let noisy: Vec<u8> = (0..100u8)
            .flat_map(|i| {
                let v = 8 + i % 9;
                [v, v, v, 255]
            })
            .collect();
        assert!(blank_picture(&noisy));
        // ... a dim silhouette (luma 14 and 26, spread 6) is not ...
        let dim: Vec<u8> = (0..100u8)
            .flat_map(|i| {
                if i % 2 == 0 {
                    [14, 14, 14, 255]
                } else {
                    [26, 26, 26, 255]
                }
            })
            .collect();
        assert!(!blank_picture(&dim));
        // ... nor black with a small white title (1 % of it) ...
        let small: Vec<u8> = (0..200u8)
            .flat_map(|i| {
                if i < 2 {
                    [255, 255, 255, 255]
                } else {
                    [0, 0, 0, 255]
                }
            })
            .collect();
        assert!(!blank_picture(&small));
        // ... nor black with a white title on a tenth of it.
        let titled: Vec<u8> = (0..100u8)
            .flat_map(|i| {
                if i < 10 {
                    [255, 255, 255, 255]
                } else {
                    [0, 0, 0, 255]
                }
            })
            .collect();
        assert!(!blank_picture(&titled));
    }

    #[test]
    fn rgba_follows_the_shader() {
        // 4×2: two chroma columns, red on the left, blue on the right
        // (BT.709, full range), mid-grey luma.
        let red = [128u8, 255]; // U, V of a reddish sample
        let blue = [255u8, 128];
        let mut planes = vec![128u8; 8];
        planes.extend([red[0], blue[0]]); // U
        planes.extend([red[1], blue[1]]); // V
        let frame = VideoFrame {
            pts: 0.0,
            width: 4,
            height: 2,
            matrix: YuvMatrix::Bt709,
            full_range: true,
            planes,
            decode_ms: 0.0,
            external: None,
        };
        let rgba = frame.rgba().unwrap();
        assert_eq!(rgba.len(), 4 * 2 * 4);
        let px = |x: usize, y: usize| &rgba[(y * 4 + x) * 4..(y * 4 + x) * 4 + 4];
        let byte = |v: f32| (v * 255.0).round() as u8;
        // The outer columns sit on their chroma sample (edges clamped): the
        // conversion of that sample exactly.
        let left = yuv_to_rgb(128, red[0], red[1], YuvMatrix::Bt709, true).map(byte);
        let right = yuv_to_rgb(128, blue[0], blue[1], YuvMatrix::Bt709, true).map(byte);
        assert_eq!(px(0, 0), [left[0], left[1], left[2], 255]);
        assert_eq!(px(3, 1), [right[0], right[1], right[2], 255]);
        // The inner ones are a quarter of the way to the other sample
        // (centre-sited chroma, interpolated as the GPU's sampler does).
        let inner = yuv_to_rgb_f(
            128.0,
            128.0 + 127.0 * 0.25,
            255.0 - 127.0 * 0.25,
            YuvMatrix::Bt709,
            true,
        );
        assert_eq!(&px(1, 0)[..3], &inner.map(byte));
        // A uniform frame converts every pixel as `rgb_at` does.
        let mut flat = vec![100u8; 9];
        flat.extend([90; 8]);
        let flat = VideoFrame {
            width: 3,
            height: 3,
            matrix: YuvMatrix::Bt601,
            full_range: false,
            planes: flat,
            ..frame
        };
        let rgba = flat.rgba().unwrap();
        let expected = flat.rgb_at(1, 1).unwrap().map(byte);
        assert!(rgba.chunks(4).all(|p| p[..3] == expected && p[3] == 255));
    }

    /// A second of evenly spaced frames.
    fn steady(fps: f64) -> Second {
        Second {
            fps,
            worst_ms: 1000.0 / fps.max(1.0),
        }
    }

    #[test]
    fn the_guard_lets_one_off_stalls_pass() {
        // A 60 Hz display: a slow second, then one with a 600 ms stall (a
        // hidden tab, a collection), then two slow ones: the stall started
        // the warm-up again, so three slow seconds in a row never come.
        let mut g = VideoGuard::default();
        let stall = Second {
            fps: 20.0,
            worst_ms: 600.0,
        };
        let seconds = [
            steady(60.0),
            steady(60.0),
            steady(30.0),
            stall,
            steady(30.0),
            steady(30.0),
        ];
        for (i, s) in seconds.iter().enumerate() {
            let ms = 1000.0 * (i + 1) as f64;
            assert_eq!(g.update(ms, Some(0), Some(*s), 60.0, 0), None, "{i}");
        }
        // Late frames from catching up after a stall are not counted
        // either: the warm-up hides them.
        let mut g = VideoGuard::default();
        let mut late = 0;
        for i in 1..=6u32 {
            let ms = 1000.0 * f64::from(i);
            let s = if i == 4 { stall } else { steady(60.0) };
            if i >= 4 {
                late += 3;
            }
            assert_eq!(g.update(ms, Some(0), Some(s), 60.0, late), None, "{i}");
        }
        // A display that stays slow is still caught: three slow seconds
        // after the warm-up that followed the stall.
        let mut stop = None;
        for i in 7..=20u32 {
            let ms = 1000.0 * f64::from(i);
            if let Some(r) = g.update(ms, Some(0), Some(steady(30.0)), 60.0, late) {
                stop = Some((r, i));
                break;
            }
        }
        assert_eq!(stop, Some((GuardAction::Pause, 9)));
    }

    #[test]
    fn the_guard_waits_out_the_warm_up_then_counts_slow_seconds() {
        let mut g = VideoGuard::default();
        // 30 fps on a 60 Hz display, one measuring second ending each 1000 ms.
        let mut stop = None;
        let mut at = 0.0;
        for ms in (0..=8000).step_by(50) {
            let ms = f64::from(ms);
            let second = (ms > 0.0 && ms % 1000.0 == 0.0).then_some(steady(30.0));
            if let Some(r) = g.update(ms, Some(0), second, 60.0, 0) {
                stop = Some(r);
                at = ms;
                break;
            }
        }
        // Warm-up to 2 s, the first second judged ends at 3 s: 3, 4, 5;
        // then a trial.
        assert_eq!(stop, Some(GuardAction::Pause));
        assert_eq!(at, 5000.0);
        assert_eq!(g.state(), "trial");
        // A good second in between starts the count again.
        let mut g = VideoGuard::default();
        let rates = [60.0, 60.0, 30.0, 30.0, 59.0, 30.0, 30.0, 58.0];
        for (i, fps) in rates.iter().enumerate() {
            let ms = 1000.0 * (i + 1) as f64;
            assert_eq!(g.update(ms, Some(0), Some(steady(*fps)), 60.0, 0), None);
        }
        // Without a movie, nothing counts, and a new one warms up again.
        let mut g = VideoGuard::default();
        for i in 1..=10 {
            let shown = if i < 6 { None } else { Some(i) };
            let ms = 1000.0 * f64::from(i as u32);
            assert_eq!(g.update(ms, shown, Some(steady(10.0)), 60.0, 0), None);
        }
    }

    #[test]
    fn a_trial_tells_the_movies_cost_from_a_busy_machine() {
        // Shown from 1 s, warm to 3 s, then three slow seconds to 6 s: paused.
        let slow = |g: &mut VideoGuard| {
            for i in 1..=6u32 {
                let ms = 1000.0 * f64::from(i);
                let action = g.update(ms, Some(0), Some(steady(30.0)), 60.0, 0);
                assert_eq!(action, (i == 6).then_some(GuardAction::Pause), "{i}");
            }
        };
        // Without the movie the display keeps up: the movie was the cause.
        let mut g = VideoGuard::default();
        slow(&mut g);
        assert_eq!(g.update(7000.0, Some(0), Some(steady(60.0)), 60.0, 0), None);
        // Slow at 45 fps, then barely over the threshold (48) without a
        // clear gain (9 asked): noise, not the movie's doing.
        let mut h = VideoGuard::default();
        for i in 1..=6u32 {
            h.update(1000.0 * f64::from(i), Some(0), Some(steady(45.0)), 60.0, 0);
        }
        assert_eq!(h.state(), "trial");
        assert_eq!(h.update(7000.0, Some(0), Some(steady(49.0)), 60.0, 0), None);
        assert_eq!(
            h.update(8000.0, Some(0), Some(steady(50.0)), 60.0, 0),
            Some(GuardAction::Resume)
        );
        // A stall in the trial counts as not keeping up (one is allowed).
        let stall = Second {
            fps: 50.0,
            worst_ms: 400.0,
        };
        assert_eq!(g.update(8000.0, Some(0), Some(stall), 60.0, 0), None);
        assert_eq!(
            g.update(9000.0, Some(0), Some(steady(59.0)), 60.0, 0),
            Some(GuardAction::Off(StopReason::DroppedFrames))
        );
        // Still slow without it: the machine is busy; the movie plays on
        // and slow seconds no longer count against it.
        let mut g = VideoGuard::default();
        slow(&mut g);
        assert_eq!(g.update(7000.0, Some(0), Some(steady(60.0)), 60.0, 0), None);
        assert_eq!(g.update(8000.0, Some(0), Some(steady(30.0)), 60.0, 0), None);
        assert_eq!(
            g.update(9000.0, Some(0), Some(steady(31.0)), 60.0, 0),
            Some(GuardAction::Resume)
        );
        assert_eq!(g.state(), "excused");
        for i in 10..=30u32 {
            let ms = 1000.0 * f64::from(i);
            assert_eq!(g.update(ms, Some(1), Some(steady(30.0)), 60.0, 0), None);
        }
        // A decoder that falls behind is still turned off.
        let mut stop = None;
        for i in 31..=40u32 {
            let ms = 1000.0 * f64::from(i);
            if let Some(a) = g.update(ms, Some(1), Some(steady(30.0)), 60.0, u64::from(i) * 10) {
                stop = Some(a);
                break;
            }
        }
        assert_eq!(stop, Some(GuardAction::Off(StopReason::DecoderBehind)));
    }

    #[test]
    fn a_trial_ends_in_three_seconds_whatever_comes() {
        let pause = |g: &mut VideoGuard| {
            for i in 1..=6u32 {
                g.update(1000.0 * f64::from(i), Some(0), Some(steady(30.0)), 60.0, 0);
            }
            assert_eq!(g.state(), "trial");
        };
        // Stalls every second (frames of 400 ms): not keeping up, decided
        // after two of them.
        let stall = Second {
            fps: 60.0,
            worst_ms: 400.0,
        };
        let mut g = VideoGuard::default();
        pause(&mut g);
        assert_eq!(g.update(7000.0, Some(0), Some(stall), 60.0, 0), None);
        assert_eq!(
            g.update(8000.0, Some(0), Some(stall), 60.0, 0),
            Some(GuardAction::Resume)
        );
        // No measured second at all (a hidden tab draws no frames): the
        // first frame after three seconds ends the trial.
        let mut g = VideoGuard::default();
        pause(&mut g);
        assert_eq!(g.update(8000.0, Some(0), None, 60.0, 0), None);
        assert_eq!(
            g.update(16000.0, Some(0), None, 60.0, 0),
            Some(GuardAction::Resume)
        );
        assert_eq!(g.state(), "excused");
    }

    #[test]
    fn the_guard_stops_a_decoder_that_stays_behind() {
        let mut g = VideoGuard::default();
        let mut late = 0;
        let mut stop = None;
        for i in 1..=10u32 {
            let ms = 1000.0 * f64::from(i);
            // Late frames from the start: the warm-up ones do not count.
            late += 20;
            if let Some(r) = g.update(ms, Some(3), Some(steady(60.0)), 60.0, late) {
                stop = Some((r, i));
                break;
            }
        }
        // Drawn from 1 s: the warm-up ends at 3 s and the judged seconds
        // at 4 s and 5 s.
        assert_eq!(stop, Some((GuardAction::Off(StopReason::DecoderBehind), 5)));
        // One burst (a stall) is not enough.
        let mut g = VideoGuard::default();
        let lates = [0, 0, 0, 5, 5, 5, 5, 9, 9, 9];
        for (i, l) in lates.iter().enumerate() {
            let ms = 1000.0 * (i + 1) as f64;
            assert_eq!(
                g.update(ms, Some(0), Some(steady(60.0)), 60.0, *l),
                None,
                "{i}"
            );
        }
    }

    #[test]
    fn picture_bounds_find_bars_across_samples() {
        let (w, h) = (16usize, 12usize);
        // A 4:3-in-16:9-like pillarbox: columns 2..14 hold the picture.
        let frame = |fill: &dyn Fn(usize, usize) -> u8| -> Vec<u8> {
            (0..w * h).map(|i| fill(i % w, i / w)).collect()
        };
        let black = frame(&|_, _| 16);
        let dark_left = frame(&|x, _| {
            if (2..8).contains(&x) {
                18
            } else if (2..14).contains(&x) {
                120
            } else {
                17
            }
        });
        let bright = frame(&|x, _| if (2..14).contains(&x) { 100 } else { 16 });
        let samples: [&[u8]; 3] = [&black, &dark_left, &bright];
        assert_eq!(
            picture_bounds(&samples, 16, 12, false),
            Some((2, 0, 12, 12))
        );
        // Only black frames, or none: no answer.
        assert_eq!(picture_bounds(&[&black], 16, 12, false), None);
        assert_eq!(picture_bounds(&[], 16, 12, false), None);
        // Letterbox in full range: rows 0..2 and 10..12 at 0.
        let letterbox = frame(&|_, y| if (2..10).contains(&y) { 90 } else { 0 });
        assert_eq!(
            picture_bounds(&[&letterbox], 16, 12, true),
            Some((0, 2, 16, 8))
        );
        // A bright pixel in a bar keeps its column.
        let mut speck = bright.clone();
        speck[w * 11] = 200;
        assert_eq!(
            picture_bounds(&[&speck], 16, 12, false),
            Some((0, 0, 14, 12))
        );
    }
}
