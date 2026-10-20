//! Every format a simfile's music comes in, decoded natively (the
//! desktop's decoder, feature `decode-all`; `docs/plans/desktop.md`, D3
//! and D9): MP3, Ogg Vorbis, Ogg Opus, FLAC, WAV and AIFF through
//! Symphonia's readers, Opus through `rusty-opus` (pure Rust, checked
//! against libopus in step 7.0; [`crate::opus`], the web fallback's
//! decoder), and [`resample_interleaved`] to the output's rate once, as
//! `decodeAudioData` does.
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
use crate::decode::{Gaps, reserve};

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
    // A work's music starts where Chrome's decode has it.
    let first = crate::mp3::first_frame(bytes);
    let shift = first.map_or(0, |f| f.target_lag(range.target).max(0) as usize);
    let until = range
        .until
        .map(|u| u + first.map_or(0.0, |f| shift as f64 / f64::from(f.header.rate.max(1))));
    let d = crate::decode::Decoder::mp3_part(bytes, until).map_err(from_web)?;
    run(d, shift, range)
}

/// The frame at second `t`.
fn frames_at(t: f64, rate: u32) -> usize {
    (t.max(0.0) * f64::from(rate)).round() as usize
}

/// The frames up to `until`, if it limits them.
fn limit(until: Option<f64>, rate: u32) -> Option<usize> {
    until.map(|s| (s.max(0.0) * f64::from(rate)).ceil() as usize)
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

/// Ogg Opus: [`crate::decode::Decoder`]'s, the web fallback's (the
/// pre-skip dropped, the stream ended at the last granule, the gain
/// applied; [`crate::opus`]), the part `range` asks for.
fn decode_opus(
    reader: Box<dyn FormatReader + '_>,
    range: Range,
) -> Result<DecodedAudio, DecodeError> {
    let d = crate::decode::Decoder::opus(reader, range.until, false).map_err(from_web)?;
    run(d, 0, range)
}

/// A decode error of [`crate::decode`]'s as this module has it.
fn from_web(e: crate::decode::DecodeError) -> DecodeError {
    match e {
        crate::decode::DecodeError::UnsupportedCodec(what) => DecodeError::Unsupported(what.into()),
        e => DecodeError::Malformed(e.to_string()),
    }
}

/// `d` run to the end of `range`, `shift` frames and what is before the
/// range's start dropped.
fn run(
    mut d: crate::decode::Decoder<'_>,
    shift: usize,
    range: Range,
) -> Result<DecodedAudio, DecodeError> {
    let rate = d.sample_rate();
    d.skip(shift + frames_at(range.from, rate));
    let want = limit(range.until, rate).map(|u| u.saturating_sub(frames_at(range.from, rate)));
    while want.is_none_or(|w| d.frames() < w) && !d.step(16).map_err(from_web)? {}
    let mut audio = d.finish().map_err(from_web)?;
    if let Some(w) = want {
        for c in &mut audio.channels {
            c.truncate(w);
        }
    }
    Ok(audio)
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
