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
