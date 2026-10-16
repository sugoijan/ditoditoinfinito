//! Every format a simfile's music comes in, decoded natively (the
//! desktop's decoder, feature `decode-all`; `docs/plans/desktop.md`, D3
//! and D9): MP3, Ogg Vorbis, Ogg Opus, FLAC, WAV and AIFF through
//! Symphonia's readers, Opus through `rusty-opus` (pure Rust, checked
//! against libopus in step 7.0), and [`resample`] to the output's rate
//! once, as `decodeAudioData` does.
//!
//! How each format is cut matches what charts were synced against:
//!
//! - **Ogg Opus**: the `OpusHead` pre-skip dropped and the stream ended at
//!   the last granule (RFC 7845), the header's output gain applied. The
//!   decoder clamps to ±1 where libopus's float output can overshoot
//!   (loud masters peak at 1.07): only those peaks differ.
//! - **Ogg Vorbis, FLAC, WAV**: Symphonia's own trimming (Vorbis's
//!   granule positions), the same samples libvorbis and libFLAC give.
//! - **MP3** as StepMania 5.1's `RageSoundReader_MP3` (libmad) decodes it,
//!   which is what simfiles were synced against: no encoder delay or
//!   padding removed (the LAME tag's figures are ignored), a first frame
//!   holding a `Xing` tag skipped, and one holding an `Info` (or `VBRI`)
//!   tag decoded as a frame of silence, as StepMania keeps it "to match
//!   DWI sync" (BASS treats it as a silent frame). Frames that cannot be
//!   decoded (a cut file's first frames pointing at a missing bit
//!   reservoir) stay as silence of their length, as BASS and StepMania
//!   do, so later frames keep their time. The decoder is
//!   [`crate::decode`]'s, which the web shares: browsers decode the same
//!   file with the delay trimmed and the tag frame dropped (about 25 ms
//!   earlier, 51 ms with an `Info` tag), and the web lines theirs up with
//!   it ([`crate::mp3::align`]).

use std::fmt;
use std::io::Cursor;

use symphonia::core::codecs::audio::AudioDecoderOptions;
use symphonia::core::codecs::audio::well_known::{CODEC_ID_FLAC, CODEC_ID_MP3, CODEC_ID_OPUS};
use symphonia::core::errors::Error as SymphoniaError;
use symphonia::core::formats::probe::Hint;
use symphonia::core::formats::{FormatOptions, FormatReader};
use symphonia::core::io::MediaSourceStream;
use symphonia::core::meta::MetadataOptions;
use symphonia::default::formats::OggReader;

pub use crate::decode::DecodedAudio;

/// Opus always decodes at 48 kHz.
const OPUS_RATE: u32 = 48_000;
/// The longest Opus packet: 120 ms at 48 kHz.
const OPUS_MAX_FRAMES: usize = 5_760;
/// Reservations are capped so a lying header cannot ask for gigabytes.
const MAX_RESERVE_SECONDS: u64 = 20 * 60;

/// Why a file could not be decoded.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum DecodeError {
    /// No reader knows the file, or no decoder its codec.
    Unsupported(String),
    /// The file is broken beyond packet-level recovery.
    Malformed(String),
    /// The file has no audio track, or none could be decoded.
    NoAudio,
}

impl fmt::Display for DecodeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            DecodeError::Unsupported(what) => write!(f, "unsupported audio: {what}"),
            DecodeError::Malformed(what) => write!(f, "malformed audio: {what}"),
            DecodeError::NoAudio => f.write_str("no audio could be decoded"),
        }
    }
}

impl std::error::Error for DecodeError {}

/// What to decode.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Range {
    /// Keep the audio from this second on (what is before is decoded and
    /// dropped as it comes, never all held: exact, and cheap).
    pub from: f64,
    /// Stop once this many seconds are decoded (a preview needs its end,
    /// not the song's); `None`: all of it.
    pub until: Option<f64>,
    /// Whose decode an MP3 is to line up with: StepMania's (as decoded),
    /// or Chrome's for a Dancing☆Onigiri work (StepMania's with
    /// [`crate::mp3::FirstFrame::target_lag`] cut from its start). The
    /// seconds above are on that timeline.
    pub target: crate::mp3::Target,
}

/// Decodes a whole file held in memory; `extension` (`"mp3"`, `"ogg"`)
/// helps the probe.
pub fn decode(bytes: &[u8], extension: Option<&str>) -> Result<DecodedAudio, DecodeError> {
    decode_range(bytes, extension, Range::default())
}

/// Decodes the part `range` of a file held in memory.
pub fn decode_range(
    bytes: &[u8],
    extension: Option<&str>,
    range: Range,
) -> Result<DecodedAudio, DecodeError> {
    // MP3s as StepMania's reader finds them: after junk, ID3v2 tags
    // anywhere before the first frame, or in a WAV of format 0x55, where
    // the probe would not look or no reader would take them.
    if crate::decode::is_mp3(bytes) {
        return decode_mp3(bytes, range);
    }
    let mut hint = Hint::new();
    if let Some(ext) = extension {
        hint.with_extension(ext);
    }
    // An Ogg stream ends with its last page (garbage after it would stop
    // the reader looking for the length).
    let bytes = if bytes.starts_with(b"OggS") {
        &bytes[..crate::decode::ogg_pages(bytes)]
    } else {
        bytes
    };
    let source = MediaSourceStream::new(Box::new(Cursor::new(bytes)), Default::default());
    let probed = symphonia::default::get_probe().probe(
        &hint,
        source,
        FormatOptions::default(),
        MetadataOptions::default(),
    );
    let reader: Box<dyn FormatReader + '_> = match probed {
        Ok(r) => r,
        // The probe reads the end for tags and gives up on garbage there;
        // an Ogg stream is read from its start without it.
        Err(_) if bytes.starts_with(b"OggS") => {
            let source = MediaSourceStream::new(Box::new(Cursor::new(bytes)), Default::default());
            Box::new(
                OggReader::try_new(source, FormatOptions::default())
                    .map_err(|e| DecodeError::Malformed(e.to_string()))?,
            )
        }
        Err(SymphoniaError::Unsupported(what)) => {
            return Err(DecodeError::Unsupported(what.to_string()));
        }
        Err(e) => return Err(DecodeError::Malformed(e.to_string())),
    };
    let codec = reader
        .tracks()
        .iter()
        .find_map(|t| Some(t.codec_params.as_ref()?.audio()?.codec))
        .ok_or(DecodeError::NoAudio)?;
    if codec == CODEC_ID_OPUS {
        decode_opus(reader, range)
    } else if codec == CODEC_ID_MP3 {
        // One MP3 decoder for both shells: `decode`'s, StepMania's way.
        drop(reader);
        decode_mp3(bytes, range)
    } else {
        let pad = codec == CODEC_ID_FLAC || is_pcm(codec);
        decode_symphonia(reader, range, pad)
    }
}

/// PCM codecs (WAV, AIFF): their packets' times are exact.
fn is_pcm(codec: symphonia::core::codecs::audio::AudioCodecId) -> bool {
    use symphonia::core::codecs::audio::well_known::*;
    [
        CODEC_ID_PCM_S8,
        CODEC_ID_PCM_U8,
        CODEC_ID_PCM_S16LE,
        CODEC_ID_PCM_S16BE,
        CODEC_ID_PCM_S24LE,
        CODEC_ID_PCM_S24BE,
        CODEC_ID_PCM_S32LE,
        CODEC_ID_PCM_S32BE,
        CODEC_ID_PCM_F32LE,
        CODEC_ID_PCM_F32BE,
        CODEC_ID_PCM_F64LE,
        CODEC_ID_PCM_F64BE,
    ]
    .contains(&codec)
}

/// An MP3 as StepMania 5.1 decodes it (`crate::decode`): from its first
/// frame wherever that is, the part `range` asks for.
fn decode_mp3(bytes: &[u8], range: Range) -> Result<DecodedAudio, DecodeError> {
    let err = |e: crate::decode::DecodeError| match e {
        crate::decode::DecodeError::UnsupportedCodec(what) => DecodeError::Unsupported(what.into()),
        e => DecodeError::Malformed(e.to_string()),
    };
    // A work's music starts where Chrome's decode has it.
    let first = crate::mp3::first_frame(bytes);
    let shift = first.map_or(0, |f| f.target_lag(range.target).max(0) as usize);
    let until = range
        .until
        .map(|u| u + first.map_or(0.0, |f| shift as f64 / f64::from(f.header.rate.max(1))));
    let mut d = crate::decode::Decoder::mp3_part(bytes, until).map_err(err)?;
    let rate = d.sample_rate();
    d.skip(shift + frames_at(range.from, rate));
    let want = limit(range.until, rate).map(|u| u.saturating_sub(frames_at(range.from, rate)));
    while want.is_none_or(|w| d.frames() < w) && !d.step(16).map_err(err)? {}
    let mut audio = d.finish().map_err(err)?;
    if let Some(w) = want {
        for c in &mut audio.channels {
            c.truncate(w);
        }
    }
    Ok(audio)
}

/// The frame at second `t`.
fn frames_at(t: f64, rate: u32) -> usize {
    (t.max(0.0) * f64::from(rate)).round() as usize
}

/// The frames up to `until`, if it limits them.
fn limit(until: Option<f64>, rate: u32) -> Option<usize> {
    until.map(|s| (s.max(0.0) * f64::from(rate)).ceil() as usize)
}

/// Planar channels reserved for `frames` (capped).
fn reserve(channels: usize, frames: u64, rate: u32) -> Vec<Vec<f32>> {
    let frames = frames.min(u64::from(rate) * MAX_RESERVE_SECONDS) as usize;
    (0..channels).map(|_| Vec::with_capacity(frames)).collect()
}

/// Where decoded audio goes: the frames before the range's start
/// dropped as they come, the rest kept up to its end.
struct Kept {
    channels: Vec<Vec<f32>>,
    /// Frames still to drop, and to keep after them (`None`: all).
    skip: usize,
    want: Option<usize>,
    skipped_packets: usize,
}

impl Kept {
    fn new(channels: usize, rate: u32, total: u64, range: Range) -> Kept {
        let skip = frames_at(range.from, rate);
        let want = limit(range.until, rate).map(|u| u.saturating_sub(skip));
        let room = want.map_or(total.saturating_sub(skip as u64), |w| w as u64);
        Kept {
            channels: reserve(channels, room, rate),
            skip,
            want,
            skipped_packets: 0,
        }
    }

    fn frames(&self) -> usize {
        self.channels.first().map_or(0, Vec::len)
    }

    fn full(&self) -> bool {
        self.want.is_some_and(|w| self.frames() >= w)
    }

    /// Room for `n` frames written by `write` into the channels' tails,
    /// then the skip and the end applied.
    fn push(&mut self, n: usize, write: impl FnOnce(&mut [&mut [f32]])) {
        let mut tails: Vec<&mut [f32]> = self
            .channels
            .iter_mut()
            .map(|c| {
                let start = c.len();
                c.resize(start + n, 0.0);
                &mut c[start..]
            })
            .collect();
        write(&mut tails);
        self.settle();
    }

    fn silence(&mut self, n: usize) {
        self.push(n, |_| {});
    }

    fn settle(&mut self) {
        if self.skip > 0 {
            let k = self.skip.min(self.frames());
            for c in &mut self.channels {
                c.drain(..k);
            }
            self.skip -= k;
        }
        if let Some(w) = self.want {
            for c in &mut self.channels {
                c.truncate(w);
            }
        }
    }

    fn finish(self, rate: u32) -> Result<DecodedAudio, DecodeError> {
        if self.frames() == 0 {
            return Err(DecodeError::NoAudio);
        }
        Ok(DecodedAudio {
            sample_rate: rate,
            channels: self.channels,
            skipped_packets: self.skipped_packets,
        })
    }
}

/// Container errors in a row after which a stream ends there (garbage
/// after the music).
const MAX_ERRORS: u32 = 32;

/// The next packet, `None` at the end. Container errors (a page with a
/// bad CRC, junk) are counted and read past, the reader resyncing, up to
/// [`MAX_ERRORS`] in a row; what they lost comes back as silence where
/// packets carry their time ([`Gaps`]).
fn next_packet(
    reader: &mut Box<dyn FormatReader + '_>,
    errors: &mut u32,
    skipped: &mut usize,
) -> Result<Option<symphonia::core::packet::Packet>, DecodeError> {
    loop {
        match reader.next_packet() {
            Ok(p) => {
                *errors = 0;
                return Ok(p);
            }
            Err(SymphoniaError::ResetRequired) => return Ok(None),
            Err(SymphoniaError::IoError(e)) if e.kind() == std::io::ErrorKind::UnexpectedEof => {
                return Ok(None);
            }
            Err(_) => {
                *errors += 1;
                *skipped += 1;
                if *errors >= MAX_ERRORS {
                    return Ok(None);
                }
            }
        }
    }
}

/// Where the stream is, from packets' times: a packet that starts after
/// the last one ended follows audio the reader dropped (a corrupt FLAC
/// frame, an Ogg page with a bad CRC), which comes back as silence of
/// its length so the audio after stays in time.
#[derive(Default)]
struct Gaps {
    /// The last packet's end, in its timestamps (`None` before one).
    end: Option<i64>,
}

impl Gaps {
    /// The frames missing before `p`.
    fn before(&mut self, p: &symphonia::core::packet::Packet) -> usize {
        let start = p.pts.get();
        let whole = (p.dur.get() + p.trim_start.get() + p.trim_end.get()) as i64;
        let gap = self.end.map_or(0, |e| (start - e).max(0) as usize);
        self.end = Some(start + whole);
        gap
    }
}

/// Any codec Symphonia decodes, with its own trimming of encoder delay
/// and padding; audio the reader dropped comes back as silence
/// ([`Gaps`]). `pad`: the header's length is exact (FLAC, PCM): a stream
/// cut short is padded to it.
fn decode_symphonia(
    mut reader: Box<dyn FormatReader + '_>,
    range: Range,
    pad: bool,
) -> Result<DecodedAudio, DecodeError> {
    let track = reader
        .tracks()
        .iter()
        .find(|t| t.codec_params.as_ref().and_then(|p| p.audio()).is_some())
        .ok_or(DecodeError::NoAudio)?;
    let track_id = track.id;
    let total = track.num_frames;
    let params = track
        .codec_params
        .as_ref()
        .and_then(|p| p.audio())
        .ok_or(DecodeError::NoAudio)?
        .clone();
    let rate = params
        .sample_rate
        .filter(|&r| r > 0)
        .ok_or_else(|| DecodeError::Malformed("no sample rate".into()))?;
    let channel_count = params
        .channels
        .as_ref()
        .map(|c| c.count())
        .filter(|&n| n > 0)
        .ok_or_else(|| DecodeError::Malformed("no channel layout".into()))?;
    let mut decoder = symphonia::default::get_codecs()
        .make_audio_decoder(&params, &AudioDecoderOptions::default())
        .map_err(|e| DecodeError::Unsupported(e.to_string()))?;
    let mut out = Kept::new(channel_count, rate, total.unwrap_or(0), range);
    // Frames of the stream so far (kept, dropped and filled).
    let mut at: u64 = 0;
    let mut errors = 0;
    let mut gaps = Gaps::default();
    while !out.full() {
        let Some(packet) = next_packet(&mut reader, &mut errors, &mut out.skipped_packets)? else {
            break;
        };
        if packet.track_id != track_id {
            continue;
        }
        let gap = gaps.before(&packet);
        if gap > 0 {
            out.silence(gap);
            at += gap as u64;
        }
        let frames = match decoder.decode(&packet) {
            Ok(buf) if buf.frames() > 0 && buf.num_planes() == channel_count => {
                let n = buf.frames();
                out.push(n, |tails| buf.copy_to_slice_planar(tails));
                n
            }
            Ok(_) => 0,
            Err(SymphoniaError::DecodeError(_) | SymphoniaError::IoError(_)) => {
                out.skipped_packets += 1;
                // Silence of the packet's length keeps what follows on
                // time.
                let n = packet.dur.get() as usize;
                out.silence(n);
                n
            }
            Err(e) => return Err(DecodeError::Malformed(e.to_string())),
        };
        at += frames as u64;
    }
    if pad
        && let Some(total) = total
        && total > at
        && !out.full()
    {
        out.silence((total - at) as usize);
    }
    out.finish(rate)
}

/// The fields of an `OpusHead` (RFC 7845, section 5.1) this decoder uses.
#[derive(Clone, Copy, Debug, PartialEq)]
struct OpusHead {
    channels: usize,
    pre_skip: usize,
    /// The output gain, as a factor.
    gain: f32,
    family: u8,
}

fn opus_head(head: &[u8]) -> Option<OpusHead> {
    if head.len() < 19 || &head[..8] != b"OpusHead" {
        return None;
    }
    let q8 = i16::from_le_bytes([head[16], head[17]]);
    Some(OpusHead {
        channels: usize::from(head[9]),
        pre_skip: usize::from(u16::from_le_bytes([head[10], head[11]])),
        gain: 10f32.powf(f32::from(q8) / (20.0 * 256.0)),
        family: head[18],
    })
}

/// Ogg Opus: packets from Symphonia's Ogg reader into `rusty-opus`; the
/// pre-skip dropped, the stream ended at the last granule, the gain
/// applied.
fn decode_opus(
    mut reader: Box<dyn FormatReader + '_>,
    range: Range,
) -> Result<DecodedAudio, DecodeError> {
    let track = reader
        .tracks()
        .iter()
        .find(|t| {
            t.codec_params
                .as_ref()
                .and_then(|p| p.audio())
                .is_some_and(|p| p.codec == CODEC_ID_OPUS)
        })
        .ok_or(DecodeError::NoAudio)?;
    let track_id = track.id;
    let head = track
        .codec_params
        .as_ref()
        .and_then(|p| p.audio())
        .and_then(|p| p.extra_data.as_deref())
        .and_then(opus_head)
        .ok_or_else(|| DecodeError::Malformed("no OpusHead".into()))?;
    if head.family != 0 || !(1..=2).contains(&head.channels) {
        return Err(DecodeError::Unsupported(format!(
            "Opus with {} channels (mapping family {})",
            head.channels, head.family
        )));
    }
    // The last granule position counts the pre-skip: the stream ends
    // there (RFC 7845, section 4.5).
    let end = track
        .num_frames
        .map(|granule| (granule as usize).saturating_sub(head.pre_skip));
    let range = match (range.until, end) {
        (Some(u), Some(e)) => Range {
            until: Some(u.min(e as f64 / f64::from(OPUS_RATE))),
            ..range
        },
        (None, Some(e)) => Range {
            until: Some(e as f64 / f64::from(OPUS_RATE)),
            ..range
        },
        _ => range,
    };
    let mut decoder = rusty_opus::OpusDecoder::new(OPUS_RATE as i32, head.channels)
        .map_err(|e| DecodeError::Malformed(format!("{e:?}")))?;
    let ch = head.channels;
    let mut out = Kept::new(ch, OPUS_RATE, end.unwrap_or(0) as u64, range);
    let mut pre_skip = head.pre_skip;
    let mut buf = vec![0f32; OPUS_MAX_FRAMES * ch];
    let mut errors = 0;
    let mut gaps = Gaps::default();
    while !out.full() {
        let Some(packet) = next_packet(&mut reader, &mut errors, &mut out.skipped_packets)? else {
            break;
        };
        if packet.track_id != track_id {
            continue;
        }
        let gap = gaps.before(&packet);
        if gap > 0 {
            let drop = pre_skip.min(gap);
            pre_skip -= drop;
            out.silence(gap - drop);
        }
        let n = match decoder.decode(&packet.data, OPUS_MAX_FRAMES, &mut buf) {
            Ok(n) => n,
            Err(_) => {
                // Concealment keeps the length (libopus's PLC, as a
                // browser would for a lost packet).
                out.skipped_packets += 1;
                decoder
                    .decode(
                        &[],
                        packet.dur.get().min(OPUS_MAX_FRAMES as u64) as usize,
                        &mut buf,
                    )
                    .unwrap_or(0)
            }
        };
        // The pre-skip is not the song's: dropped before the range counts.
        let drop = pre_skip.min(n);
        pre_skip -= drop;
        let frames = &buf[drop * ch..n * ch];
        let gain = head.gain;
        out.push(n - drop, |tails| {
            for (i, frame) in frames.chunks_exact(ch).enumerate() {
                for (c, &s) in frame.iter().enumerate() {
                    tails[c][i] = s * gain;
                }
            }
        });
    }
    out.finish(OPUS_RATE)
}

/// `audio` at `rate`, through rubato's FFT resampler (the whole clip at
/// once, its delay trimmed); `audio` itself when the rates agree.
pub fn resample(audio: DecodedAudio, rate: u32) -> Result<DecodedAudio, DecodeError> {
    let channels = audio.channels.len();
    let skipped = audio.skipped_packets;
    let data = resample_interleaved(audio, rate)?;
    let n = data.len() / channels.max(1);
    let mut planar: Vec<Vec<f32>> = (0..channels).map(|_| Vec::with_capacity(n)).collect();
    for frame in data.chunks_exact(channels.max(1)) {
        for (c, &s) in frame.iter().enumerate() {
            planar[c].push(s);
        }
    }
    Ok(DecodedAudio {
        sample_rate: rate,
        channels: planar,
        skipped_packets: skipped,
    })
}

/// `audio` at `rate`, interleaved: what a mixer plays, made with one
/// copy of the audio beside the decoded one (rubato writes interleaved;
/// the decoded audio is freed before this returns).
pub fn resample_interleaved(audio: DecodedAudio, rate: u32) -> Result<Vec<f32>, DecodeError> {
    use rubato::audioadapter_buffers::direct::SequentialSliceOfVecs;
    use rubato::{Fft, FixedSync, Resampler};
    let channels = audio.channels.len();
    let frames = audio.frames();
    if audio.sample_rate == rate || frames == 0 || channels == 0 {
        let mut data = Vec::with_capacity(frames * channels);
        for i in 0..frames {
            for c in &audio.channels {
                data.push(c[i]);
            }
        }
        return Ok(data);
    }
    let mut resampler = Fft::<f32>::new(
        audio.sample_rate as usize,
        rate as usize,
        1024,
        channels,
        FixedSync::Input,
    )
    .map_err(|e| DecodeError::Malformed(e.to_string()))?;
    let input = SequentialSliceOfVecs::new(&audio.channels, channels, frames)
        .map_err(|e| DecodeError::Malformed(e.to_string()))?;
    let out = resampler
        .process_all(&input, frames, None)
        .map_err(|e| DecodeError::Malformed(e.to_string()))?;
    Ok(out.take_data())
}

/// MP3's first frames as StepMania 5.1 reads them (shared with the web).
pub use crate::mp3;

#[cfg(test)]
mod tests;
