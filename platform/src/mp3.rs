//! MP3 as StepMania 5.1 reads it (`docs/plans/desktop.md`, D9; PLAN.md
//! decision 46): which first frame it keeps, and how far another decode
//! of the same file sits from it.
//!
//! StepMania 5.1's `RageSoundReader_MP3` (libmad) trims nothing: no
//! encoder delay, no decoder delay, no padding. A first frame holding a
//! `Xing` tag is skipped; one holding an `Info` tag is decoded as audio,
//! a frame of silence, "so we match DWI sync" (DWI's decoder, BASS, does
//! that); `VBRI`, which its `xing_parse` does not know, likewise. Charts
//! were synced against that. Browsers and gapless decoders trim LAME's
//! delay (and drop the tag frame), each in its own way, so the same file
//! is heard 0 to 77 ms earlier there. [`align`] tells which of the lags
//! the first frame allows ([`FirstFrame::candidates`]) a browser's decode
//! has, on the decoded audio itself, and [`FirstFrame::expected`] gives
//! the measured one when the music cannot tell (a tone, a loop); the
//! web moves the browser's decode to StepMania's.
//!
//! Pure bytes and samples: natively tested, the same on every target.

/// Bitrates of Layer III, kb/s, by index: MPEG-1, then MPEG-2 and 2.5.
const BITRATES: [[u32; 16]; 2] = [
    [
        0, 32, 40, 48, 56, 64, 80, 96, 112, 128, 160, 192, 224, 256, 320, 0,
    ],
    [
        0, 8, 16, 24, 32, 40, 48, 56, 64, 80, 96, 112, 128, 144, 160, 0,
    ],
];
/// Sample rates by index, MPEG-1; MPEG-2 halves them, 2.5 quarters.
const RATES: [u32; 3] = [44_100, 48_000, 32_000];

/// A Layer III frame header.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Header {
    pub mpeg1: bool,
    pub mono: bool,
    pub crc: bool,
    pub rate: u32,
    /// The frame's length in bytes, header included.
    pub len: usize,
}

impl Header {
    /// The header at the start of `b`, if one is there.
    pub fn parse(b: &[u8]) -> Option<Header> {
        if b.len() < 4 || b[0] != 0xff || b[1] & 0xe0 != 0xe0 {
            return None;
        }
        let version = (b[1] >> 3) & 3;
        let layer = (b[1] >> 1) & 3;
        let bitrate = usize::from(b[2] >> 4);
        let rate = usize::from((b[2] >> 2) & 3);
        if version == 1 || layer != 1 || bitrate == 0 || bitrate == 15 || rate == 3 {
            return None;
        }
        let mpeg1 = version == 3;
        let rate = RATES[rate]
            >> match version {
                3 => 0,
                2 => 1,
                _ => 2,
            };
        let kbps = BITRATES[usize::from(!mpeg1)][bitrate];
        let padding = usize::from((b[2] >> 1) & 1);
        let len = (if mpeg1 { 144_000 } else { 72_000 }) * kbps as usize / rate as usize + padding;
        Some(Header {
            mpeg1,
            mono: b[3] >> 6 == 3,
            crc: b[1] & 1 == 0,
            rate,
            len,
        })
    }

    /// Samples a frame holds.
    pub fn samples(&self) -> usize {
        if self.mpeg1 { 1152 } else { 576 }
    }

    /// Bytes of side information after the header (and its CRC).
    fn side(&self) -> usize {
        match (self.mpeg1, self.mono) {
            (true, true) => 17,
            (true, false) => 32,
            (false, true) => 9,
            (false, false) => 17,
        }
    }
}

/// The length of an ID3v2 tag at the start of `b` (0 without one).
pub fn id3v2_len(b: &[u8]) -> usize {
    if b.len() < 10 || &b[..3] != b"ID3" {
        return 0;
    }
    let size = b[6..10]
        .iter()
        .fold(0usize, |n, &x| (n << 7) | usize::from(x & 0x7f));
    let footer = if b[5] & 0x10 != 0 { 10 } else { 0 };
    10 + size + footer
}

/// How far StepMania 5.1 looks for the first frame, bytes, ID3v2 tags
/// not counted: it gives up ("Can't find data") once it has read more
/// than 25,000 bytes without a frame (`bytes_read > 25000` in
/// `RageSoundReader_MP3::do_mad_frame_decode`), checked only when its
/// 16 KB buffer runs dry, so it looks through two buffers' worth.
pub const RESYNC: usize = 32 * 1024;

/// What the first frame holds.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Tag {
    /// Audio.
    None,
    /// A `Xing` tag (VBR): StepMania skips the frame.
    Xing,
    /// An `Info` tag (CBR): StepMania plays it as silence.
    Info,
    /// A `VBRI` tag (Fraunhofer): played as silence too.
    Vbri,
}

/// The first frame of an MP3 and what it holds.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct FirstFrame {
    /// Where it starts (after any ID3v2 tags and junk).
    pub at: usize,
    pub header: Header,
    pub tag: Tag,
    /// The LAME (or Lavc/Lavf) extension's encoder delay and padding,
    /// samples, when the tag has one: what gapless decoders trim (with
    /// the decoder's own 529).
    pub lame: Option<(u32, u32)>,
}

/// The decoder delay of every MP3 decoder (libmad's, FFmpeg's,
/// Symphonia's): its synthesis filter's 528 samples, plus one.
pub const DECODER_DELAY: u32 = 529;
/// LAME's encoder delay when the tag does not say.
pub const LAME_DELAY: u32 = 576;

impl FirstFrame {
    /// The silence StepMania 5.1 decodes the frame as, in samples: an
    /// `Info` or `VBRI` frame's (its `xing_parse` keeps the first and
    /// does not know the second), and a `Xing` frame's in a file with
    /// CRCs (LAME writes the tag where the CRC is not counted, and
    /// StepMania looks for it after it, so it decodes the frame); 0 when
    /// it is skipped or is audio.
    pub fn silence(&self) -> usize {
        match self.tag {
            Tag::Info | Tag::Vbri => self.header.samples(),
            Tag::Xing if self.header.crc => self.header.samples(),
            Tag::None | Tag::Xing => 0,
        }
    }

    /// The encoder delay a gapless decoder would trim: the tag's, else
    /// LAME's usual.
    fn delay(&self) -> u32 {
        self.lame.map_or(LAME_DELAY, |l| l.0)
    }

    /// Where a browser's decode of the file may sit against StepMania's,
    /// in samples at the file's rate (positive: the browser's is earlier):
    /// the tag frame StepMania plays as silence ([`Self::silence`]), a
    /// frame more or less (a browser that keeps the tag frame, or WebKit's
    /// extra frame at 48 kHz), the encoder delay trimmed or not, the
    /// decoder's trimmed or not. Every lag measured in Chrome, Firefox and
    /// WebKit (PLAN.md decision 46) is one of these.
    pub fn candidates(&self) -> Vec<i64> {
        let s = self.silence() as i64;
        let f = self.header.samples() as i64;
        let d = i64::from(self.delay());
        let b = i64::from(DECODER_DELAY);
        let mut out = Vec::new();
        for k in [-f, 0, f] {
            for a in [0, d] {
                for c in [0, b] {
                    let lag = s + k + a + c;
                    if lag >= -f && !out.contains(&lag) {
                        out.push(lag);
                    }
                }
            }
        }
        out.sort_unstable();
        out
    }

    /// The lag `browser`'s `decodeAudioData` gives the file, as measured
    /// (PLAN.md decision 46), samples at the file's rate: the answer when
    /// [`align`] cannot tell. Chromium (FFmpeg) drops a tag frame and
    /// trims LAME's delay and the decoder's when the tag has LAME's
    /// figures, and so does Firefox 158 (Playwright's earlier build
    /// trimmed the decoder's only, and does for a tag without them);
    /// WebKit the decoder's always,
    /// LAME's too on stereo files at 32 kHz and up, and a frame more at
    /// 48 kHz.
    pub fn expected(&self, browser: Browser) -> i64 {
        let s = self.silence() as i64;
        let f = self.header.samples() as i64;
        let d = i64::from(self.delay());
        let b = i64::from(DECODER_DELAY);
        let tagged = self.tag != Tag::None;
        match browser {
            Browser::Chromium if !tagged => 0,
            Browser::Chromium if self.lame.is_some() => s + d + b,
            Browser::Chromium => s,
            Browser::Gecko if !tagged => 0,
            Browser::Gecko if self.lame.is_some() => s + d + b,
            Browser::Gecko => s + b,
            Browser::WebKit if !tagged => b,
            Browser::WebKit if self.header.mono || self.header.rate < 32_000 => s + b,
            Browser::WebKit if self.header.rate == 48_000 => s + f + d + b,
            Browser::WebKit => s + d + b,
        }
    }

    /// Where a song timed against `target`'s decode wants the file, in
    /// samples at its rate against StepMania's decode (positive: earlier,
    /// so that much of StepMania's start is cut): 0 for a simfile; for a
    /// Dancing☆Onigiri work, Chromium's lag ([`Self::expected`]), as
    /// danoniplus plays the browser's own decode and works are synced in
    /// Chrome (PLAN.md decision 46).
    pub fn target_lag(&self, target: Target) -> i64 {
        match target {
            Target::StepMania => 0,
            Target::Chromium => self.expected(Browser::Chromium),
        }
    }

    /// The longest lag [`Self::candidates`] holds, seconds.
    pub fn max_lag(&self) -> f64 {
        let max = self.candidates().into_iter().max().unwrap_or(0).max(0);
        max as f64 / f64::from(self.header.rate)
    }
}

/// Whose decode of an MP3 a song's notes are timed against.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Target {
    /// StepMania 5.1's (simfiles).
    #[default]
    StepMania,
    /// Chrome's `decodeAudioData` (Dancing☆Onigiri works: danoniplus
    /// schedules the browser's own decode, and works are made in Chrome).
    Chromium,
}

/// A browser engine, for the lag its MP3 decode has when [`align`]
/// cannot tell ([`FirstFrame::expected`]).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Browser {
    /// Chrome, Edge and the rest (FFmpeg's decoder).
    Chromium,
    /// Firefox.
    Gecko,
    /// Safari, and every browser on iOS.
    WebKit,
}

impl Browser {
    /// The engine a user agent string names.
    pub fn from_user_agent(ua: &str) -> Browser {
        if ua.contains("Firefox/") && !ua.contains("FxiOS") {
            Browser::Gecko
        } else if (ua.contains("Chrome/") || ua.contains("Chromium/")) && !ua.contains("CriOS") {
            Browser::Chromium
        } else if ua.contains("AppleWebKit") {
            Browser::WebKit
        } else {
            Browser::Chromium
        }
    }
}

/// Finds the first frame as StepMania's reader syncs to it: past every
/// ID3v2 tag on the way (at the start or after junk, not counted), the
/// first header followed by another where its length says (so a stray
/// `0xFF` in junk is passed over), within [`RESYNC`] bytes, as StepMania
/// gives up past that.
pub fn first_frame(bytes: &[u8]) -> Option<FirstFrame> {
    let at = find_frame(bytes, |i| {
        Header::parse(&bytes[i..]).is_some_and(|h| {
            let next = i + h.len;
            // The last frame of a short file has no successor.
            next >= bytes.len() || Header::parse(&bytes[next..]).is_some()
        })
    })?;
    let header = Header::parse(&bytes[at..])?;
    // LAME writes the tag right after the side information as if there
    // were no CRC (Symphonia and FFmpeg look for it there too).
    let tag_at = at + 4 + header.side();
    let tag = match bytes.get(tag_at..tag_at + 4) {
        Some(b"Xing") => Tag::Xing,
        Some(b"Info") => Tag::Info,
        _ if bytes.get(at + 36..at + 40) == Some(b"VBRI") => Tag::Vbri,
        _ => Tag::None,
    };
    let lame = matches!(tag, Tag::Xing | Tag::Info)
        .then(|| lame(bytes, tag_at))
        .flatten();
    Some(FirstFrame {
        at,
        header,
        tag,
        lame,
    })
}

/// The first position `is_frame` accepts, looking as StepMania does:
/// ID3v2 tags skipped wherever they are met and not counted against
/// [`RESYNC`].
fn find_frame(bytes: &[u8], is_frame: impl Fn(usize) -> bool) -> Option<usize> {
    let mut i = 0;
    let mut budget = RESYNC;
    while i < bytes.len() {
        let tag = id3v2_len(&bytes[i..]);
        if tag > 0 {
            i = i.saturating_add(tag);
            continue;
        }
        if budget == 0 {
            return None;
        }
        if is_frame(i) {
            return Some(i);
        }
        budget -= 1;
        i += 1;
    }
    None
}

/// Whether `bytes` hold a free-format MP3 (frames with no bitrate,
/// which [`Header::parse`] and Symphonia do not read): a Layer III
/// header with bitrate index 0 where the first frame would be.
pub fn free_format(bytes: &[u8]) -> bool {
    find_frame(bytes, |i| {
        let b = &bytes[i..];
        b.len() >= 4
            && b[0] == 0xff
            && b[1] & 0xe0 == 0xe0
            && (b[1] >> 3) & 3 != 1
            && (b[1] >> 1) & 3 == 1
            && b[2] >> 4 == 0
            && (b[2] >> 2) & 3 != 3
    })
    .is_some()
}

/// Where a later frame holding a `Xing`, `Info` or `VBRI` tag has its
/// tag's name (files joined end to end): StepMania decodes those frames
/// as audio, a frame of silence each (only its first frame is looked at
/// for a tag), where Symphonia drops them. Found as Symphonia finds
/// them: the name right after the side information (`VBRI` 32 bytes
/// in), the bytes before it zero.
pub fn later_tags(bytes: &[u8], first: &FirstFrame) -> Vec<usize> {
    let mut out = Vec::new();
    let from = first.at + 4;
    let mut i = from;
    while let Some(k) = bytes.get(i..).and_then(|b| {
        b.windows(4)
            .position(|w| matches!(w, b"Xing" | b"Info" | b"VBRI"))
    }) {
        let p = i + k;
        i = p + 1;
        let vbri = &bytes[p..p + 4] == b"VBRI";
        let sides: &[usize] = if vbri { &[32] } else { &[9, 17, 32] };
        for &side in sides {
            let Some(h_at) = p.checked_sub(4 + side) else {
                continue;
            };
            if h_at == first.at {
                continue;
            }
            let Some(h) = Header::parse(&bytes[h_at..]) else {
                continue;
            };
            if !vbri && h.side() != side {
                continue;
            }
            let body = h_at + if h.crc { 6 } else { 4 };
            if body <= p && bytes[body..p].iter().all(|&x| x == 0) && h_at + h.len <= bytes.len() {
                out.push(p);
                break;
            }
        }
    }
    out
}

/// The LAME extension after a Xing/Info tag at `tag_at`: encoder delay
/// and padding (12 bits each), when its encoder is LAME or FFmpeg's.
fn lame(b: &[u8], tag_at: usize) -> Option<(u32, u32)> {
    let flags = u32::from_be_bytes(b.get(tag_at + 4..tag_at + 8)?.try_into().ok()?);
    let mut o = tag_at + 8;
    for (bit, len) in [(1, 4), (2, 4), (4, 100), (8, 4)] {
        if flags & bit != 0 {
            o += len;
        }
    }
    let encoder = b.get(o..o + 4)?;
    if !matches!(encoder, b"LAME" | b"Lavf" | b"Lavc") {
        return None;
    }
    let d = b.get(o + 21..o + 24)?;
    let delay = (u32::from(d[0]) << 4) | (u32::from(d[1]) >> 4);
    let padding = ((u32::from(d[1]) & 0x0f) << 8) | u32::from(d[2]);
    Some((delay, padding))
}

/// The silence StepMania 5.1 decodes a first tag frame as (see
/// [`FirstFrame::silence`]); `None` when there is none.
pub fn silent_tag_frame(bytes: &[u8]) -> Option<usize> {
    first_frame(bytes).map(|f| f.silence()).filter(|&n| n > 0)
}

/// Where one decode of a song sits against another.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Alignment {
    /// Seconds the `reference` is later than `other`: pad `other` with
    /// this much silence at its start (cut it when negative) and the two
    /// agree.
    pub lag: f64,
    /// The mean normalised correlation there over the stretches (1: the
    /// same samples).
    pub corr: f64,
}

/// Why [`align`] gave no answer.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Unsure {
    /// No stretch of the reference has sound (yet).
    Silent,
    /// No candidate matches well (best correlation under 0.9): the
    /// decodes differ, or the lag is none of the candidates.
    NoMatch { best: f64 },
    /// Two candidates match about as well (a tone, a loop repeating at
    /// their distance): the best and the runner-up's correlations.
    Ambiguous { best: f64, second: f64 },
}

/// The length of a stretch compared, seconds.
const WINDOW: f64 = 0.2;
/// Stretches compared at most.
const STRETCHES: usize = 8;
/// Correlation a candidate needs.
const MIN_CORR: f64 = 0.9;
/// How much better than every other candidate it must be.
const MARGIN: f64 = 0.05;

/// Which of `candidates` (lags in samples at `ref_rate`, as
/// [`FirstFrame::candidates`] gives) puts `other` (a song decoded
/// elsewhere, at `other_rate`, read through `read(start, out)`, which
/// fills `out` from sample `start` with zeros past its end) on
/// `reference` (mono samples at `ref_rate` from the song's start, as
/// far as decoded).
///
/// No free search: periodic music matches at wrong lags as well as at
/// the right one, and two stretches agree on the same wrong answer. Each
/// candidate is scored at its exact offset (the reference interpolated
/// linearly at the other's samples, one sample either way allowed for a
/// resampler) on up to [`STRETCHES`] stretches of 0.2 s with sound,
/// spread over the reference; the best mean wins when it is at least 0.9
/// and beats every other candidate's by 0.05. Otherwise [`Unsure`]: the
/// caller decodes more, or takes the browser's expected lag
/// ([`FirstFrame::expected`]).
pub fn align(
    reference: &[f32],
    ref_rate: u32,
    other_rate: u32,
    candidates: &[i64],
    mut read: impl FnMut(usize, &mut [f32]),
) -> Result<Alignment, Unsure> {
    if ref_rate == 0 || other_rate == 0 || candidates.is_empty() {
        return Err(Unsure::Silent);
    }
    let (rr, or) = (f64::from(ref_rate), f64::from(other_rate));
    let win = (WINDOW * rr) as usize;
    let lo = *candidates.iter().min().unwrap_or(&0);
    let hi = *candidates.iter().max().unwrap_or(&0);
    // Stretches start late enough that every candidate has the other's
    // samples before them.
    let first = hi.max(0) as usize + 2;
    let mut loud = Vec::new();
    let mut s = first;
    while s + win + 2 <= reference.len() {
        let w = &reference[s..s + win];
        let rms = (w.iter().map(|x| f64::from(*x).powi(2)).sum::<f64>() / w.len() as f64).sqrt();
        if rms > 0.01 {
            loud.push(s);
        }
        s += win;
    }
    if loud.is_empty() {
        return Err(Unsure::Silent);
    }
    // Up to STRETCHES of them, evenly spread.
    let picked: Vec<usize> = if loud.len() <= STRETCHES {
        loud
    } else {
        (0..STRETCHES)
            .map(|i| loud[i * (loud.len() - 1) / (STRETCHES - 1)])
            .collect()
    };
    let n = (WINDOW * or) as usize;
    let mut scores = vec![0.0f64; candidates.len()];
    let mut buf = Vec::new();
    for &s in &picked {
        // The other's samples every candidate needs here, one more each
        // side for the slack.
        let from = (((s as f64 - hi as f64) * or / rr).floor() as i64 - 2).max(0) as usize;
        let to = ((s as f64 - lo as f64) * or / rr).ceil() as usize + n + 3;
        buf.clear();
        buf.resize(to - from, 0.0);
        read(from, &mut buf);
        for (score, &c) in scores.iter_mut().zip(candidates) {
            // other[j] is heard at reference sample j·rr/or + c.
            let j0 = ((s as f64 - c as f64) * or / rr).ceil() as usize;
            let x: Vec<f32> = (0..n)
                .map(|k| {
                    let t = (j0 + k) as f64 * rr / or + c as f64;
                    let i = t.floor() as usize;
                    let f = (t - i as f64) as f32;
                    let a = reference.get(i).copied().unwrap_or(0.0);
                    let b = reference.get(i + 1).copied().unwrap_or(a);
                    a + (b - a) * f
                })
                .collect();
            let best = [-1i64, 0, 1]
                .iter()
                .map(|&d| {
                    let at = (j0 as i64 + d - from as i64).max(0) as usize;
                    corr(&x, buf.get(at..at + n).unwrap_or(&[]))
                })
                .fold(f64::NEG_INFINITY, f64::max);
            *score += best / picked.len() as f64;
        }
    }
    let mut order: Vec<usize> = (0..candidates.len()).collect();
    order.sort_by(|&a, &b| scores[b].total_cmp(&scores[a]));
    let best = scores[order[0]];
    let second = order.get(1).map_or(f64::NEG_INFINITY, |&i| scores[i]);
    if best < MIN_CORR {
        return Err(Unsure::NoMatch { best });
    }
    if best - second < MARGIN {
        return Err(Unsure::Ambiguous { best, second });
    }
    Ok(Alignment {
        lag: candidates[order[0]] as f64 / rr,
        corr: best,
    })
}

/// [`align`] on a whole decode held in memory.
pub fn align_slices(
    reference: &[f32],
    ref_rate: u32,
    other: &[f32],
    other_rate: u32,
    candidates: &[i64],
) -> Result<Alignment, Unsure> {
    align(reference, ref_rate, other_rate, candidates, |start, out| {
        let have = other.get(start..).unwrap_or(&[]);
        let n = have.len().min(out.len());
        out[..n].copy_from_slice(&have[..n]);
        out[n..].fill(0.0);
    })
}

/// Normalised correlation of two equally long signals (−∞ when `y` is
/// short).
fn corr(x: &[f32], y: &[f32]) -> f64 {
    if y.len() < x.len() || x.is_empty() {
        return f64::NEG_INFINITY;
    }
    let (mut xy, mut xx, mut yy) = (0.0f64, 0.0f64, 0.0f64);
    for (&a, &b) in x.iter().zip(y) {
        let (a, b) = (f64::from(a), f64::from(b));
        xy += a * b;
        xx += a * a;
        yy += b * b;
    }
    xy / (xx * yy).sqrt().max(1e-30)
}

#[cfg(test)]
mod tests {
    use super::*;

    const INFO: &[u8] = include_bytes!("../tests/fixtures/sine-441-info.mp3");
    const BARE: &[u8] = include_bytes!("../tests/fixtures/sine-441-bare.mp3");
    const XING: &[u8] = include_bytes!("../tests/fixtures/sine-441-xing.mp3");

    #[test]
    fn first_frames_and_their_tags() {
        let f = first_frame(INFO).unwrap();
        assert_eq!(f.at, 0);
        assert_eq!(f.tag, Tag::Info);
        assert_eq!(f.header.rate, 44_100);
        assert!(f.header.mpeg1 && f.header.mono);
        assert_eq!(f.silence(), 1152);
        // LAME's usual delay; the padding fills the last frame.
        assert_eq!(f.lame.map(|l| l.0), Some(576));
        let x = first_frame(XING).unwrap();
        assert_eq!((x.tag, x.silence()), (Tag::Xing, 0));
        assert_eq!(x.lame.map(|l| l.0), Some(576));
        let b = first_frame(BARE).unwrap();
        assert_eq!((b.tag, b.lame, b.silence()), (Tag::None, None, 0));
        assert_eq!(silent_tag_frame(INFO), Some(1152));
        assert_eq!(silent_tag_frame(BARE), None);
        // After an ID3v2 tag, and past junk that looks like a header.
        let mut tagged = b"ID3\x04\0\0\0\0\0\0\xff\xfb\x90\x00junk".to_vec();
        tagged.extend_from_slice(INFO);
        let t = first_frame(&tagged).unwrap();
        assert_eq!((t.at, t.tag), (18, Tag::Info));
        assert_eq!(first_frame(b""), None);
        assert_eq!(first_frame(b"ID3\x04\0\0\x7f\x7f\x7f\x7f"), None);
    }

    #[test]
    fn frame_lengths() {
        // 128 kb/s at 44.1 kHz, MPEG-1: 417 bytes (418 padded).
        let h = Header::parse(&[0xff, 0xfb, 0x90, 0x00]).unwrap();
        assert_eq!((h.rate, h.len, h.samples()), (44_100, 417, 1152));
        let h = Header::parse(&[0xff, 0xfb, 0x92, 0x00]).unwrap();
        assert_eq!(h.len, 418);
        // MPEG-2, 64 kb/s at 22.05 kHz: 208 bytes, 576 samples.
        let h = Header::parse(&[0xff, 0xf3, 0x80, 0xc0]).unwrap();
        assert_eq!(
            (h.rate, h.len, h.samples(), h.mono),
            (22_050, 208, 576, true)
        );
        assert_eq!(Header::parse(&[0xff, 0xfb, 0xf0, 0x00]), None);
    }

    #[test]
    fn tags_in_files_with_crcs_ids_and_junk() {
        // A CRC: the tag is where LAME writes it (the CRC not counted),
        // and StepMania, looking past the CRC, decodes the `Xing` frame
        // as a frame of silence.
        let mut crc = XING.to_vec();
        crc[1] &= !1;
        let f = first_frame(&crc).unwrap();
        assert!(f.header.crc);
        assert_eq!((f.tag, f.silence()), (Tag::Xing, 1152));
        // Two ID3v2 tags in a row, then junk: every tag skipped, not
        // counted against the search.
        let mut b = b"ID3\x04\0\0\0\0\x01\0".to_vec();
        b.resize(10 + 128, 0);
        b.extend_from_slice(b"ID3\x04\0\0\0\0\x02\0");
        b.resize(b.len() + 256, 0);
        let junk = b.len();
        b.resize(junk + 30_000, 0);
        b.extend_from_slice(INFO);
        let t = first_frame(&b).unwrap();
        assert_eq!((t.at, t.tag), (junk + 30_000, Tag::Info));
        // Junk StepMania reads through (two 16 KB buffers), and more.
        let mut b = vec![0u8; RESYNC - 100];
        b.extend_from_slice(INFO);
        assert!(first_frame(&b).is_some());
        let mut b = vec![0u8; RESYNC + 10];
        b.extend_from_slice(INFO);
        assert!(first_frame(&b).is_none());
        // An ID3v2 tag met in the junk does not count either.
        // (16,000 bytes, a 20,000-byte tag, 16,000 bytes.)
        let mut b = vec![0u8; 16_000];
        b.extend_from_slice(b"ID3\x04\0\0\0\x01\x1c\x20");
        b.resize(b.len() + 20_000 + 16_000, 0);
        b.extend_from_slice(INFO);
        assert_eq!(
            first_frame(&b).map(|f| f.at),
            Some(16_000 + 20_010 + 16_000)
        );
    }

    #[test]
    fn free_format_and_later_tags() {
        // Bitrate index 0: not read, but recognised.
        let mut free = vec![0xff, 0xfb, 0x00, 0x00];
        free.resize(2000, 0x55);
        assert!(first_frame(&free).is_none());
        assert!(free_format(&free));
        assert!(!free_format(INFO));
        // Files joined end to end: the second file's tag frame found.
        let mut joined = INFO.to_vec();
        joined.extend_from_slice(XING);
        joined.extend_from_slice(BARE);
        let first = first_frame(&joined).unwrap();
        let later = later_tags(&joined, &first);
        assert_eq!(later.len(), 1);
        assert_eq!(&joined[later[0]..later[0] + 4], b"Xing");
        assert!(later[0] > INFO.len() && later[0] < INFO.len() + 64);
        assert!(later_tags(INFO, &first_frame(INFO).unwrap()).is_empty());
    }

    #[test]
    fn candidates_hold_every_measured_lag() {
        let info = first_frame(INFO).unwrap();
        let xing = first_frame(XING).unwrap();
        let bare = first_frame(BARE).unwrap();
        for lag in [1152, 1681, 2257, 3409] {
            assert!(info.candidates().contains(&lag), "{lag}");
        }
        for lag in [0, 529, 1105, 2257] {
            assert!(xing.candidates().contains(&lag), "{lag}");
            assert!(bare.candidates().contains(&lag), "{lag}");
        }
        // The browsers' measured lags (mono 44.1 kHz fixtures).
        assert_eq!(info.expected(Browser::Chromium), 2257);
        assert_eq!(info.expected(Browser::Gecko), 2257);
        assert_eq!(info.expected(Browser::WebKit), 1681);
        assert_eq!(xing.expected(Browser::Chromium), 1105);
        assert_eq!(xing.expected(Browser::Gecko), 1105);
        assert_eq!(bare.expected(Browser::Chromium), 0);
        assert_eq!(bare.expected(Browser::WebKit), 529);
        assert!((info.max_lag() - 3409.0 / 44_100.0).abs() < 1e-9);
        // MPEG-2 at 22.05 kHz: 576-sample frames.
        let mut h = info;
        h.header = Header::parse(&[0xff, 0xf3, 0x80, 0xc0]).unwrap();
        assert_eq!(h.expected(Browser::Chromium), 576 + 576 + 529);
        assert_eq!(h.expected(Browser::Gecko), 576 + 576 + 529);
        // An 8 kHz file's lags are longer than at 22 kHz, in time.
        h.header = Header::parse(&[0xff, 0xe3, 0x88, 0xc0]).unwrap();
        assert_eq!(h.header.rate, 8_000);
        assert!(h.max_lag() > 0.2);
        assert_eq!(
            Browser::from_user_agent(
                "Mozilla/5.0 (Macintosh; Intel Mac OS X 10.15; rv:131.0) Gecko/20100101 Firefox/131.0"
            ),
            Browser::Gecko
        );
        assert_eq!(
            Browser::from_user_agent(
                "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/605.1.15 (KHTML, like Gecko) Version/18.0 Safari/605.1.15"
            ),
            Browser::WebKit
        );
        assert_eq!(
            Browser::from_user_agent(
                "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/130.0.0.0 Safari/537.36"
            ),
            Browser::Chromium
        );
    }

    /// White noise from a seed (xorshift).
    fn noise(n: usize, seed: u32) -> Vec<f32> {
        let mut x = seed;
        (0..n)
            .map(|_| {
                x ^= x << 13;
                x ^= x >> 17;
                x ^= x << 5;
                (x as f32 / u32::MAX as f32 - 0.5) * 0.5
            })
            .collect()
    }

    /// Music the reviewer's files had, as functions of time.
    /// A named signal, as a function of time.
    type Signal = (&'static str, Box<dyn Fn(f64) -> f32>);

    fn signals() -> Vec<Signal> {
        use std::f64::consts::TAU;
        vec![
            (
                "tone 440",
                Box::new(|t| (0.5 * (TAU * 440.0 * t).sin()) as f32),
            ),
            (
                "saw 110",
                Box::new(|t| {
                    let s: f64 = (1..27)
                        .map(|k| (TAU * 110.0 * k as f64 * t).sin() / k as f64)
                        .sum();
                    (0.2 * s) as f32
                }),
            ),
            (
                "sub-bass",
                Box::new(|t| {
                    (0.25 * (TAU * 55.0 * t).sin() + 0.1 * (TAU * 110.0 * t).sin()) as f32
                }),
            ),
            (
                "vibrato",
                Box::new(|t| {
                    (0.3 * (TAU * 220.0 * t).sin() * (1.0 + 0.3 * (TAU * 5.0 * t).sin())) as f32
                }),
            ),
            (
                "square 441",
                Box::new(|t| {
                    let s: f64 = (0..15)
                        .map(|k| {
                            let n = (2 * k + 1) as f64;
                            (TAU * 441.0 * n * t).sin() / n
                        })
                        .sum();
                    (0.3 * s) as f32
                }),
            ),
        ]
    }

    /// One period of decaying noise looped, 44.1 kHz.
    fn hat_loop(period: usize, n: usize) -> Vec<f32> {
        let hit: Vec<f32> = noise(period, 5)
            .iter()
            .enumerate()
            .map(|(i, x)| x * (-(i as f32) / (period as f32 * 0.15)).exp())
            .collect();
        (0..n).map(|i| hit[i % period]).collect()
    }

    /// What a web page ends up with: the lag found, else the browser's.
    fn settle(first: &FirstFrame, browser: Browser, r: &[f32], o: &[f32], or: u32) -> i64 {
        match align_slices(r, 44_100, o, or, &first.candidates()) {
            Ok(a) => (a.lag * 44_100.0).round() as i64,
            Err(_) => first.expected(browser),
        }
    }

    #[test]
    fn periodic_music_never_gets_a_wrong_lag() {
        let firsts = [
            first_frame(INFO).unwrap(),
            first_frame(XING).unwrap(),
            first_frame(BARE).unwrap(),
        ];
        let seconds = 12.0;
        for browser in [Browser::Chromium, Browser::Gecko, Browser::WebKit] {
            for first in &firsts {
                let truth = first.expected(browser);
                for (name, f) in signals() {
                    // StepMania's decode: the music `truth` samples later
                    // than the browser's.
                    let r: Vec<f32> = (0..(seconds * 44_100.0) as usize)
                        .map(|i| {
                            let t = (i as i64 - truth) as f64 / 44_100.0;
                            if t < 0.0 { 0.0 } else { f(t) }
                        })
                        .collect();
                    for or in [44_100u32, 48_000] {
                        let o: Vec<f32> = (0..(seconds * f64::from(or)) as usize)
                            .map(|j| f(j as f64 / f64::from(or)))
                            .collect();
                        let got = settle(first, browser, &r, &o, or);
                        assert_eq!(got, truth, "{name} {browser:?} {:?} at {or}", first.tag);
                    }
                }
                for period in [2205, 3307, 4409, 4410, 4608] {
                    let n = (seconds * 44_100.0) as usize;
                    let o = hat_loop(period, n);
                    let mut r = vec![0.0; truth.max(0) as usize];
                    r.extend_from_slice(&o[(-truth).max(0) as usize..]);
                    let got = settle(first, browser, &r, &o, 44_100);
                    assert_eq!(got, truth, "loop {period} {browser:?} {:?}", first.tag);
                }
            }
        }
    }

    #[test]
    fn align_finds_the_lag_at_the_same_rate_and_another() {
        let info = first_frame(INFO).unwrap();
        let c = info.candidates();
        // The reference: 0.3 s of silence, then noise. The other: the
        // same 2257 frames earlier (a browser's gapless decode of an
        // `Info` file).
        let mut reference = vec![0.0; 13_230];
        reference.extend(noise(3 * 44_100, 7));
        let other = reference[2257..].to_vec();
        let a = align_slices(&reference, 44_100, &other, 44_100, &c).unwrap();
        assert!((a.lag * 44_100.0 - 2257.0).abs() < 0.5, "{a:?}");
        assert!(a.corr > 0.999);
        // Any other candidate, found as well (Firefox's 1681, WebKit's
        // 3409, a decode that keeps the tag frame).
        for lag in [1681i64, 3409, 0, 1152] {
            let other = reference[lag as usize..].to_vec();
            let a = align_slices(&reference, 44_100, &other, 44_100, &c).unwrap();
            assert_eq!((a.lag * 44_100.0).round() as i64, lag);
        }
        // At 48 kHz (linear resampling): low-passed noise so linear
        // interpolation stands for a real resampler.
        let smooth: Vec<f32> = reference
            .windows(8)
            .map(|w| w.iter().sum::<f32>() / 8.0)
            .collect();
        let at48: Vec<f32> = (0..(smooth.len() as f64 * 48_000.0 / 44_100.0) as usize - 2)
            .map(|i| {
                let t = i as f64 * 44_100.0 / 48_000.0;
                let k = t as usize;
                let f = (t - k as f64) as f32;
                smooth[k] + (smooth[k + 1] - smooth[k]) * f
            })
            .collect();
        let xing = first_frame(XING).unwrap();
        let other48 = at48[(1105.0 * 48_000.0 / 44_100.0) as usize..].to_vec();
        let a = align_slices(&smooth, 44_100, &other48, 48_000, &xing.candidates()).unwrap();
        assert_eq!((a.lag * 44_100.0).round(), 1105.0, "{a:?}");
        // Silence, or unrelated audio, or a lag that is no candidate: no
        // answer.
        assert_eq!(
            align_slices(&vec![0.0; 44_100], 44_100, &other, 44_100, &c),
            Err(Unsure::Silent)
        );
        let unrelated = noise(88_200, 99);
        assert!(matches!(
            align_slices(&reference, 44_100, &unrelated, 44_100, &c),
            Err(Unsure::NoMatch { .. })
        ));
        let odd = reference[1000..].to_vec();
        assert!(matches!(
            align_slices(&reference, 44_100, &odd, 44_100, &c),
            Err(Unsure::NoMatch { .. })
        ));
    }
}
