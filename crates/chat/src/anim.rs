//! Emote images read before they are decoded, and animated emotes' frames
//! over time (round TW4, `docs/plans/multiplayer-twitch.md`, "Animated
//! emotes (round TW4)").
//!
//! [`probe`] reads an image's container without decoding a pixel (GIF's
//! blocks, WebP's chunks, PNG's and APNG's chunks, JPEG's frame header):
//! its format, its canvas and each frame's size and duration, and checks
//! `anim-limits-v0` ([`LIMITS_NAME`]) there, before a shell spends memory
//! on pixels: a hostile or huge image (enormous sides, a decompression
//! bomb, thousands of frames) is refused, or its animation is, before it
//! is decoded. [`Probe::verdict`] says which frames a shell decodes
//! ([`plan`]: at most [`KEPT_FRAMES`], thinned evenly), and [`Clock`] and
//! [`frame_at`] which frame is drawn when: on the host's clock, held while
//! the frames run late, still under reduced motion, the same frame for
//! every copy of an emote. Nothing here is judged, logged or replayed.

use crate::cache::{CELL, fit};

/// The limits' name and version. Every number PROVISIONAL.
pub const LIMITS_NAME: &str = "anim-limits-v0";
/// Bytes of an image read at most (the proxy's cap too).
pub const MOST_BYTES: usize = 2 << 20;
/// A side of the canvas, or of any frame, at most, pixels.
pub const MOST_SIDE: u32 = 1024;
/// Frames of an animation at most: past them, its first frame, still.
pub const MOST_FRAMES: usize = 1000;
/// Pixels of all its frames at most (frames × the canvas): what decoding
/// them costs. Past it, the first frame, still.
pub const MOST_WORK: u64 = 64 << 20;
/// Frames of an emote kept at most.
pub const KEPT_FRAMES: usize = 64;
/// The atlas's units ([`CELL`] wide) an emote's frames take at most.
pub const KEPT_UNITS: u32 = 128;
/// A frame's duration at or under this is read as [`SLOW_MS`], as the
/// browsers read it.
pub const FAST_MS: u32 = 10;
pub const SLOW_MS: u32 = 100;

/// An image's container.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Format {
    Png,
    Gif,
    WebP,
    Jpeg,
}

impl Format {
    /// Its media type.
    pub fn mime(self) -> &'static str {
        match self {
            Format::Png => "image/png",
            Format::Gif => "image/gif",
            Format::WebP => "image/webp",
            Format::Jpeg => "image/jpeg",
        }
    }
}

/// Why an image is never drawn.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Refusal {
    /// More than [`MOST_BYTES`].
    TooLarge,
    /// Not PNG, GIF, WebP or JPEG.
    Unknown,
    /// Its container cannot be read (no frame, no size).
    Broken,
    /// A side of its canvas or of a frame over [`MOST_SIDE`].
    TooWide,
}

impl std::fmt::Display for Refusal {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Refusal::TooLarge => "more bytes than an emote's",
            Refusal::Unknown => "not an image of a known format",
            Refusal::Broken => "a broken image",
            Refusal::TooWide => "larger than an emote",
        })
    }
}

/// What [`probe`] read.
#[derive(Clone, Debug, PartialEq)]
pub struct Probe {
    pub format: Format,
    /// The canvas, pixels.
    pub size: [u32; 2],
    /// Frames counted (1 for a still image).
    pub frames: usize,
    /// Each frame's duration, milliseconds ([`FAST_MS`] read as
    /// [`SLOW_MS`]), for the first [`MOST_FRAMES`] + 1 frames.
    pub durations: Vec<u32>,
}

/// A frame a shell decodes and keeps: its index in the image and how long
/// it shows, milliseconds (its own and those of the frames passed over
/// after it).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Kept {
    pub index: usize,
    pub ms: u32,
}

/// How a shell decodes an image [`probe`] let through.
#[derive(Clone, Debug, PartialEq)]
pub enum Verdict {
    /// Its first frame, still: a still image, or an animation past
    /// [`MOST_FRAMES`] or [`MOST_WORK`].
    Still,
    /// These frames, in order.
    Animated(Vec<Kept>),
}

impl Probe {
    /// Whether the image moves at all.
    pub fn animated(&self) -> bool {
        self.frames > 1
    }

    /// What is decoded of it: within [`MOST_FRAMES`] and [`MOST_WORK`],
    /// its frames thinned to what the atlas keeps ([`most_kept`]); else
    /// its first frame alone.
    pub fn verdict(&self) -> Verdict {
        if !self.animated() {
            return Verdict::Still;
        }
        let area = u64::from(self.size[0]) * u64::from(self.size[1]);
        if self.frames > MOST_FRAMES || (self.frames as u64).saturating_mul(area) > MOST_WORK {
            return Verdict::Still;
        }
        let durations = &self.durations[..self.frames.min(self.durations.len())];
        Verdict::Animated(plan(durations, most_kept(self.size)))
    }
}

/// Frames of an image of `size` pixels the atlas keeps: [`KEPT_FRAMES`],
/// fewer for a wide one ([`KEPT_UNITS`] units in all).
pub fn most_kept(size: [u32; 2]) -> usize {
    let units = fit(size)[0].div_ceil(CELL).max(1);
    KEPT_FRAMES.min((KEPT_UNITS / units) as usize).max(1)
}

/// The frames kept of an animation of `durations`, `most` at most: every
/// k-th, each standing for the frames up to the next one kept, so the
/// loop takes as long.
pub fn plan(durations: &[u32], most: usize) -> Vec<Kept> {
    let n = durations.len();
    if n == 0 {
        return Vec::new();
    }
    let stride = n.div_ceil(most.max(1));
    (0..n)
        .step_by(stride)
        .map(|index| Kept {
            index,
            ms: durations[index..(index + stride).min(n)]
                .iter()
                .fold(0u32, |a, d| a.saturating_add(*d)),
        })
        .collect()
}

/// The frame of `durations` (milliseconds each) shown at `t_ms` of a loop
/// that never ends.
pub fn frame_at(durations: &[u32], t_ms: u64) -> usize {
    let total: u64 = durations.iter().map(|d| u64::from(*d)).sum();
    if total == 0 || durations.len() < 2 {
        return 0;
    }
    let mut t = t_ms % total;
    for (i, d) in durations.iter().enumerate() {
        let d = u64::from(*d);
        if t < d {
            return i;
        }
        t -= d;
    }
    durations.len() - 1
}

/// The layer's clock for animated emotes: the host's seconds, running only
/// while frames are not late and motion is not reduced (held otherwise, on
/// the frame it had; standing at zero under reduced motion).
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Clock {
    t: f64,
    last: Option<f64>,
}

impl Clock {
    /// The host second `now`: the time since the last one counts where
    /// `running`.
    pub fn advance(&mut self, now: f64, running: bool) {
        if let Some(last) = self.last
            && running
            && now > last
        {
            self.t += now - last;
        }
        self.last = Some(now);
    }

    /// Its time, milliseconds.
    pub fn ms(&self) -> u64 {
        (self.t * 1000.0).round().max(0.0) as u64
    }
}

fn read_ms(ms: u32) -> u32 {
    if ms <= FAST_MS { SLOW_MS } else { ms }
}

fn side(size: [u32; 2]) -> Result<[u32; 2], Refusal> {
    if size[0] == 0 || size[1] == 0 {
        Err(Refusal::Broken)
    } else if size[0] > MOST_SIDE || size[1] > MOST_SIDE {
        Err(Refusal::TooWide)
    } else {
        Ok(size)
    }
}

fn u16le(b: &[u8], at: usize) -> Option<u32> {
    Some(u32::from(u16::from_le_bytes([*b.get(at)?, *b.get(at + 1)?])))
}

fn u16be(b: &[u8], at: usize) -> Option<u32> {
    Some(u32::from(u16::from_be_bytes([*b.get(at)?, *b.get(at + 1)?])))
}

fn u24le(b: &[u8], at: usize) -> Option<u32> {
    Some(u32::from_le_bytes([*b.get(at)?, *b.get(at + 1)?, *b.get(at + 2)?, 0]))
}

fn u32le(b: &[u8], at: usize) -> Option<u32> {
    Some(u32::from_le_bytes(b.get(at..at + 4)?.try_into().ok()?))
}

fn u32be(b: &[u8], at: usize) -> Option<u32> {
    Some(u32::from_be_bytes(b.get(at..at + 4)?.try_into().ok()?))
}

/// Frames counted, their durations kept for the first ones.
#[derive(Default)]
struct Frames {
    count: usize,
    durations: Vec<u32>,
}

impl Frames {
    fn push(&mut self, ms: u32) {
        self.count += 1;
        if self.durations.len() <= MOST_FRAMES {
            self.durations.push(read_ms(ms));
        }
    }
}

/// Reads `bytes` as an image's container: refused where it is too large,
/// unknown, broken or wider than an emote; nothing decoded.
pub fn probe(bytes: &[u8]) -> Result<Probe, Refusal> {
    if bytes.len() > MOST_BYTES {
        return Err(Refusal::TooLarge);
    }
    if bytes.starts_with(b"GIF87a") || bytes.starts_with(b"GIF89a") {
        gif(bytes)
    } else if bytes.len() >= 12 && &bytes[..4] == b"RIFF" && &bytes[8..12] == b"WEBP" {
        webp(bytes)
    } else if bytes.starts_with(b"\x89PNG\r\n\x1a\n") {
        png(bytes)
    } else if bytes.starts_with(&[0xff, 0xd8]) {
        jpeg(bytes)
    } else {
        Err(Refusal::Unknown)
    }
}

/// Passes over GIF data sub-blocks from `at`: the position after their
/// terminator, `None` where they run past the end.
fn sub_blocks(b: &[u8], mut at: usize) -> Option<usize> {
    loop {
        let n = usize::from(*b.get(at)?);
        at += 1;
        if n == 0 {
            return Some(at);
        }
        at += n;
    }
}

fn gif(b: &[u8]) -> Result<Probe, Refusal> {
    let size = side([
        u16le(b, 6).ok_or(Refusal::Broken)?,
        u16le(b, 8).ok_or(Refusal::Broken)?,
    ])?;
    let packed = *b.get(10).ok_or(Refusal::Broken)?;
    let mut at = 13;
    if packed & 0x80 != 0 {
        at += 3 * (2usize << (packed & 7));
    }
    let mut frames = Frames::default();
    let mut delay = 0;
    // Up to the trailer, or where it is cut off (frames whole so far
    // count, as the browsers show them).
    while let Some(&kind) = b.get(at) {
        match kind {
            0x21 => {
                let Some(&label) = b.get(at + 1) else { break };
                if label == 0xf9 {
                    delay = u16le(b, at + 4).unwrap_or(0) * 10;
                }
                match sub_blocks(b, at + 2) {
                    Some(next) => at = next,
                    None => break,
                }
            }
            0x2c => {
                let (Some(w), Some(h), Some(&p)) = (u16le(b, at + 5), u16le(b, at + 7), b.get(at + 9))
                else {
                    break;
                };
                side([w.max(1), h.max(1)])?;
                at += 10;
                if p & 0x80 != 0 {
                    at += 3 * (2usize << (p & 7));
                }
                // The LZW code size, then the data.
                match sub_blocks(b, at + 1) {
                    Some(next) => at = next,
                    None => break,
                }
                frames.push(delay);
                delay = 0;
            }
            0x3b => break,
            _ => {
                if frames.count == 0 {
                    return Err(Refusal::Broken);
                }
                break;
            }
        }
    }
    if frames.count == 0 {
        return Err(Refusal::Broken);
    }
    Ok(Probe {
        format: Format::Gif,
        size,
        frames: frames.count,
        durations: frames.durations,
    })
}

fn webp(b: &[u8]) -> Result<Probe, Refusal> {
    let mut at = 12;
    let mut canvas: Option<[u32; 2]> = None;
    let mut animated = false;
    let mut still: Option<[u32; 2]> = None;
    let mut frames = Frames::default();
    while let (Some(kind), Some(len)) = (b.get(at..at + 4), u32le(b, at + 4)) {
        let data = at + 8;
        let Some(end) = data.checked_add(len as usize).filter(|e| *e <= b.len()) else {
            break;
        };
        match kind {
            b"VP8X" => {
                let flags = *b.get(data).ok_or(Refusal::Broken)?;
                animated = flags & 0x02 != 0;
                let w = u24le(b, data + 4).ok_or(Refusal::Broken)? + 1;
                let h = u24le(b, data + 7).ok_or(Refusal::Broken)? + 1;
                canvas = Some(side([w, h])?);
            }
            b"ANMF" => {
                let w = u24le(b, data + 6).ok_or(Refusal::Broken)? + 1;
                let h = u24le(b, data + 9).ok_or(Refusal::Broken)? + 1;
                side([w, h])?;
                frames.push(u24le(b, data + 12).ok_or(Refusal::Broken)?);
            }
            b"VP8 " if still.is_none() => {
                if b.get(data + 3..data + 6) != Some(&[0x9d, 0x01, 0x2a]) {
                    return Err(Refusal::Broken);
                }
                let w = u16le(b, data + 6).ok_or(Refusal::Broken)? & 0x3fff;
                let h = u16le(b, data + 8).ok_or(Refusal::Broken)? & 0x3fff;
                still = Some([w, h]);
            }
            b"VP8L" if still.is_none() => {
                if b.get(data) != Some(&0x2f) {
                    return Err(Refusal::Broken);
                }
                let bits = u32le(b, data + 1).ok_or(Refusal::Broken)?;
                still = Some([(bits & 0x3fff) + 1, ((bits >> 14) & 0x3fff) + 1]);
            }
            _ => {}
        }
        at = end + (len as usize & 1);
    }
    if animated {
        let size = canvas.ok_or(Refusal::Broken)?;
        if frames.count == 0 {
            return Err(Refusal::Broken);
        }
        return Ok(Probe {
            format: Format::WebP,
            size,
            frames: frames.count,
            durations: frames.durations,
        });
    }
    let size = side(canvas.or(still).ok_or(Refusal::Broken)?)?;
    if let Some(s) = still {
        side(s)?;
    } else {
        return Err(Refusal::Broken);
    }
    Ok(Probe {
        format: Format::WebP,
        size,
        frames: 1,
        durations: vec![0],
    })
}

fn png(b: &[u8]) -> Result<Probe, Refusal> {
    if b.get(12..16) != Some(b"IHDR") {
        return Err(Refusal::Broken);
    }
    let size = side([
        u32be(b, 16).ok_or(Refusal::Broken)?,
        u32be(b, 20).ok_or(Refusal::Broken)?,
    ])?;
    let mut at = 8;
    let mut actl = false;
    let mut frames = Frames::default();
    while let (Some(len), Some(kind)) = (u32be(b, at), b.get(at + 4..at + 8)) {
        let data = at + 8;
        let Some(end) = data
            .checked_add(len as usize)
            .and_then(|e| e.checked_add(4))
            .filter(|e| *e <= b.len())
        else {
            break;
        };
        match kind {
            b"acTL" => actl = true,
            b"fcTL" => {
                let w = u32be(b, data + 4).ok_or(Refusal::Broken)?;
                let h = u32be(b, data + 8).ok_or(Refusal::Broken)?;
                side([w, h])?;
                let num = u16be(b, data + 20).ok_or(Refusal::Broken)?;
                let den = u16be(b, data + 22).ok_or(Refusal::Broken)?;
                let den = if den == 0 { 100 } else { den };
                frames.push(num * 1000 / den);
            }
            b"IEND" => break,
            _ => {}
        }
        at = end;
    }
    if actl && frames.count > 1 {
        return Ok(Probe {
            format: Format::Png,
            size,
            frames: frames.count,
            durations: frames.durations,
        });
    }
    Ok(Probe {
        format: Format::Png,
        size,
        frames: 1,
        durations: vec![0],
    })
}

fn jpeg(b: &[u8]) -> Result<Probe, Refusal> {
    let mut at = 2;
    loop {
        if b.get(at) != Some(&0xff) {
            return Err(Refusal::Broken);
        }
        let marker = *b.get(at + 1).ok_or(Refusal::Broken)?;
        match marker {
            0xff => at += 1,
            0x01 | 0xd0..=0xd7 => at += 2,
            0xc0..=0xcf if !matches!(marker, 0xc4 | 0xc8 | 0xcc) => {
                let h = u16be(b, at + 5).ok_or(Refusal::Broken)?;
                let w = u16be(b, at + 7).ok_or(Refusal::Broken)?;
                return Ok(Probe {
                    format: Format::Jpeg,
                    size: side([w, h])?,
                    frames: 1,
                    durations: vec![0],
                });
            }
            0xd9 | 0xda => return Err(Refusal::Broken),
            _ => at += 2 + u16be(b, at + 2).ok_or(Refusal::Broken)? as usize,
        }
    }
}

/// TESTING ONLY: a GIF of `frames` frames of `size` pixels each lasting
/// `delay_cs` hundredths of a second, its pixel data a minimal valid
/// stream (every pixel the first of a two-color table): what the probe's
/// tests and the hostile images are made of (decoders read it too).
pub fn test_gif(size: [u16; 2], frames: usize, delay_cs: u16) -> Vec<u8> {
    let mut g = Vec::new();
    g.extend(b"GIF89a");
    g.extend(size[0].to_le_bytes());
    g.extend(size[1].to_le_bytes());
    // A global table of two colors.
    g.extend([0x80, 0, 0]);
    g.extend([0, 0, 0, 255, 255, 255]);
    for _ in 0..frames {
        g.extend([0x21, 0xf9, 4, 0]);
        g.extend(delay_cs.to_le_bytes());
        g.extend([0, 0]);
        g.extend([0x2c, 0, 0, 0, 0]);
        g.extend(size[0].to_le_bytes());
        g.extend(size[1].to_le_bytes());
        g.push(0);
        // Code size 2: a clear code then the end code (no pixel coded;
        // decoders fill the rest with the background).
        g.extend([2, 1, 0x2c, 0]);
    }
    g.push(0x3b);
    g
}

#[cfg(test)]
mod tests;
