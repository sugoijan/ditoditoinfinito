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

/// One decoded frame as 8-bit YUV 4:2:0.
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

/// YUV to non-linear (gamma-encoded, as the video carries it) RGB in 0–1,
/// clamped. This is the reference the background shader follows.
pub fn yuv_to_rgb(y: u8, u: u8, v: u8, matrix: YuvMatrix, full_range: bool) -> [f32; 3] {
    let (y, u, v) = (f32::from(y), f32::from(u) - 128.0, f32::from(v) - 128.0);
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

/// A run from a movie's start keeps its frames up to this many bytes; when
/// it reaches the end within them, every later lap plays from memory, as
/// StepMania loops without seeking (a seek per lap would be slower than a
/// lap of a short movie).
pub const LOOP_CACHE_BYTES: usize = 24 << 20;

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
                    if now - frame.pts > LATE_SECONDS {
                        self.stats.late += 1;
                    }
                    self.seeking = false;
                    self.last_decoded = Some(frame.pts);
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
                let short = self.end.is_some_and(|e| e <= 10.0);
                self.seek(decoder, if short { 0.0 } else { local }, lap);
            } else {
                let jumped = self
                    .previous
                    .is_some_and(|p| local - p > SEEK_GAP_SECONDS && lap == self.lap);
                let before = local + 1e-6 < self.origin;
                if (jumped && !self.ended) || before {
                    self.seek(decoder, local, lap);
                }
            }
        }
        self.previous = Some(local);
        let due = self.queue.iter().rposition(|f| f.pts <= local + 1e-9);
        let frame = match due {
            Some(i) => {
                self.stats.dropped += i as u64;
                self.queue.drain(..i);
                self.queue.pop_front()
            }
            // StepMania shows a run's first frame as soon as it is decoded.
            None if self.shown.is_none() && !self.seeking => self.queue.pop_front(),
            None => None,
        };
        if let Some(f) = &frame {
            self.shown = Some(f.pts);
            self.stats.shown += 1;
        }
        let have = self.queue.len() as u32 + decoder.pending();
        if !self.ended && have < self.ahead {
            decoder.want(self.ahead - have);
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
                    planes: vec![i as u8; self.plane_len],
                    decode_ms: 0.0,
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
            .map(|&p| track.update(d, p).map(|f| f.planes[0]))
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
    fn a_track_resumed_past_the_end_wraps_instead_of_stalling() {
        // A 5 s movie (150 frames, the last at 149/30 s) resumed at 12 s.
        let end = 149.0 / 30.0;
        // Without knowing its end: the seek finds the end, then it wraps.
        let mut d = FakeDecoder::new(150, 0, 0);
        let mut track = MovieTrack::new(true, 3, None);
        track.start(&mut d, 12.0);
        let shown = play(&mut track, &mut d, &display_frames(12.0, 13.0));
        assert_eq!(track.end(), Some(end));
        assert!(shown.iter().flatten().count() > 20, "{shown:?}");
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
    }
}
