//! Pure-Rust audio decoding, the fallback for files the browser's
//! `decodeAudioData` rejects (Ogg Vorbis on Safari, mostly), and MP3 as
//! StepMania 5.1 decodes it ([`crate::mp3`]), which the web also uses to
//! line a browser's MP3 decode up with StepMania's.
//!
//! Ogg Vorbis and MP3 are decoded here, through Symphonia, and with the
//! feature `opus` Ogg Opus, through `rusty-opus` ([`crate::opus`]: the
//! pre-skip dropped, the stream ended at its last granule, the header's
//! gain applied; what WebKit builds without Opus need, PLAN.md decision
//! 9). WAV and FLAC are decoded natively by every target browser, so
//! they are reported as unsupported ([`DecodeError::UnsupportedContainer`],
//! [`DecodeError::UnsupportedCodec`]). See `docs/research/tech-stack.md` §6.
//!
//! MP3s keep everything StepMania's libmad reader keeps: no encoder or
//! decoder delay trimmed, a `Xing` tag frame skipped, an `Info` (or
//! `VBRI`) tag frame played as a frame of silence, frames that fail
//! decoded as silence of their length.
//!
//! The PCM is written straight into one `Vec<f32>` per channel (no
//! interleaved intermediate), reserved up front from the stream length the
//! reader finds (the Ogg's last page, the MP3's tag), so a song costs its
//! PCM size once (a 3-minute stereo 44.1 kHz song is ~64 MB) instead of up
//! to twice that from `Vec` doubling.

use std::borrow::Cow;
use std::fmt;
use std::io::Cursor;

use symphonia::core::codecs::audio::well_known::{
    CODEC_ID_FLAC, CODEC_ID_MP3, CODEC_ID_OPUS, CODEC_ID_VORBIS,
};
use symphonia::core::codecs::audio::{AudioDecoder, AudioDecoderOptions};
use symphonia::core::errors::Error as SymphoniaError;
use symphonia::core::formats::{FormatOptions, FormatReader};
use symphonia::core::io::MediaSourceStream;
use symphonia::default::codecs::{MpaDecoder, VorbisDecoder};
use symphonia::default::formats::{MpaReader, OggReader};

/// Upper bound on the up-front reservation, so a corrupt length field in the
/// last page cannot request gigabytes. Longer streams still decode; they
/// just grow the buffers as they go.
const MAX_RESERVE_SECONDS: u64 = 20 * 60;

/// Container errors in a row after which a stream with audio ends.
const MAX_ERRORS: u32 = 32;

/// Decoded audio at the file's own sample rate, one sample vector per
/// channel, all of equal length.
#[derive(Clone, Debug, PartialEq)]
pub struct DecodedAudio {
    pub sample_rate: u32,
    pub channels: Vec<Vec<f32>>,
    /// Packets that failed to decode and were skipped (left as a gap; MP3:
    /// as silence of their length).
    pub skipped_packets: usize,
}

impl DecodedAudio {
    /// Length in sample frames.
    pub fn frames(&self) -> usize {
        self.channels.first().map_or(0, Vec::len)
    }

    /// Length in seconds.
    pub fn duration(&self) -> f64 {
        self.frames() as f64 / f64::from(self.sample_rate)
    }
}

/// Why a file could not be decoded.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum DecodeError {
    /// Neither Ogg nor MP3. Holds the sniffed format (`"WAV"`, …).
    UnsupportedContainer(&'static str),
    /// An Ogg stream whose audio codec this decoder does not handle, or a
    /// free-format MP3. Holds the codec's name.
    UnsupportedCodec(&'static str),
    /// An Ogg stream without any audio track.
    NoAudioTrack,
    /// The stream is broken beyond packet-level recovery.
    Malformed(String),
    /// The stream was read to the end without producing a single frame.
    NoAudio { skipped_packets: usize },
}

impl fmt::Display for DecodeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            DecodeError::UnsupportedContainer(kind) => {
                write!(f, "{kind} is not supported (only {SUPPORTED} are)")
            }
            DecodeError::UnsupportedCodec(codec) => {
                write!(f, "{codec} is not supported (only {SUPPORTED} are)")
            }
            DecodeError::NoAudioTrack => f.write_str("the stream has no audio track"),
            DecodeError::Malformed(msg) => write!(f, "malformed audio stream: {msg}"),
            DecodeError::NoAudio { skipped_packets } => write!(
                f,
                "no audio could be decoded ({skipped_packets} packets failed)"
            ),
        }
    }
}

impl std::error::Error for DecodeError {}

/// What [`Decoder`] reads, for error messages.
#[cfg(feature = "opus")]
const SUPPORTED: &str = "Ogg Vorbis, Ogg Opus and MP3";
#[cfg(not(feature = "opus"))]
const SUPPORTED: &str = "Ogg Vorbis and MP3";

/// What decodes a stream's packets.
enum Codec {
    Symphonia(Box<dyn AudioDecoder>),
    #[cfg(feature = "opus")]
    Opus(Box<OpusState>),
}

/// An Opus stream's decoder and where the stream is.
#[cfg(feature = "opus")]
struct OpusState {
    stream: crate::opus::Stream,
    /// Frames of the stream so far (kept, skipped and silence put in).
    at: usize,
    /// Where it ends: the last granule less the pre-skip (RFC 7845,
    /// section 4.5), if the reader found it.
    end: Option<usize>,
    /// Where the stream is from packets' times (a later packet's start
    /// past the last one's end follows a page the reader dropped).
    gaps: Gaps,
}

/// Where the stream is, from packets' times: a packet that starts after
/// the last one ended follows audio the reader dropped (a corrupt FLAC
/// frame, an Ogg page with a bad CRC), which comes back as silence of
/// its length so the audio after stays in time.
#[cfg(feature = "opus")]
#[derive(Default)]
pub(crate) struct Gaps {
    /// The last packet's end, in its timestamps (`None` before one).
    end: Option<i64>,
}

#[cfg(feature = "opus")]
impl Gaps {
    /// The frames missing before `p`.
    pub(crate) fn before(&mut self, p: &symphonia::core::packet::Packet) -> usize {
        let start = p.pts.get();
        let whole = (p.dur.get() + p.trim_start.get() + p.trim_end.get()) as i64;
        let gap = self.end.map_or(0, |e| (start - e).max(0) as usize);
        self.end = Some(start + whole);
        gap
    }
}

/// Best-effort name of a file's format, for error messages and to tell
/// MP3 from the rest.
fn sniff(bytes: &[u8]) -> &'static str {
    match bytes {
        [b'I', b'D', b'3', ..] | [0xff, 0xe0..=0xff, ..] => "MP3",
        [
            b'R',
            b'I',
            b'F',
            b'F',
            _,
            _,
            _,
            _,
            b'W',
            b'A',
            b'V',
            b'E',
            ..,
        ] => "WAV",
        [b'f', b'L', b'a', b'C', ..] => "FLAC",
        [_, _, _, _, b'f', b't', b'y', b'p', ..] => "MP4/M4A",
        [0x1a, 0x45, 0xdf, 0xa3, ..] => "WebM/Matroska",
        _ => "this file format",
    }
}

/// Where the last whole Ogg page of `bytes` ends: garbage after the
/// stream (a tag, a cut download's tail) left out, as the Ogg reader
/// looks at the last page for the length and gives up on garbage there.
/// Garbage between pages stays (the reader resyncs past it). All of
/// `bytes` when no page is whole.
pub(crate) fn ogg_pages(bytes: &[u8]) -> usize {
    let page_end = |at: usize| -> Option<usize> {
        if bytes.get(at..at + 4)? != b"OggS" {
            return None;
        }
        let segments = usize::from(*bytes.get(at + 26)?);
        let lacing = bytes.get(at + 27..at + 27 + segments)?;
        let end = at + 27 + segments + lacing.iter().map(|&x| usize::from(x)).sum::<usize>();
        (end <= bytes.len()).then_some(end)
    };
    let mut last = 0;
    let mut at = 0;
    while at + 4 <= bytes.len() {
        match page_end(at) {
            Some(end) => {
                last = end;
                at = end;
            }
            // Past garbage to the next capture pattern.
            None => match bytes[at + 1..].windows(4).position(|w| w == b"OggS") {
                Some(k) => at += 1 + k,
                None => break,
            },
        }
    }
    if last == 0 { bytes.len() } else { last }
}

/// Whether `bytes` are an Ogg Opus stream: its first page's first packet
/// an `OpusHead` (RFC 7845, section 3). What the web's fallback sends to
/// its Opus worker, the main module not decoding Opus (PLAN.md decision 9).
pub fn is_ogg_opus(bytes: &[u8]) -> bool {
    let Some(&segments) = bytes.get(26) else {
        return false;
    };
    let body = 27 + usize::from(segments);
    bytes.starts_with(b"OggS") && bytes.get(body..body + 8) == Some(b"OpusHead")
}

/// Whether `bytes` are an MP3 as StepMania's reader finds one: not
/// another container (Ogg, FLAC, WAV, AIFF, MP4, Matroska) and a frame
/// within [`crate::mp3::RESYNC`] bytes, past ID3v2 tags, junk (files that
/// start with zeros, an APE tag) or a RIFF header around MP3 data (a WAV
/// of format 0x55, which StepMania's MP3 reader reads through).
pub fn is_mp3(bytes: &[u8]) -> bool {
    match sniff(bytes) {
        "WAV" => riff_format(bytes) == Some(0x55) && crate::mp3::first_frame(bytes).is_some(),
        "MP3" | "this file format"
            if !bytes.starts_with(b"OggS") && !bytes.starts_with(b"FORM") =>
        {
            crate::mp3::first_frame(bytes).is_some()
        }
        _ => false,
    }
}

/// The format tag of a RIFF/WAVE file's `fmt ` chunk.
fn riff_format(bytes: &[u8]) -> Option<u16> {
    let mut at = 12;
    while let Some(head) = bytes.get(at..at + 8) {
        let len = u32::from_le_bytes(head[4..8].try_into().ok()?) as usize;
        if &head[..4] == b"fmt " {
            let tag = bytes.get(at + 8..at + 10)?;
            return Some(u16::from_le_bytes([tag[0], tag[1]]));
        }
        at = at.checked_add(8 + len + (len & 1))?;
    }
    None
}

/// Incremental decoder, so a caller on a UI thread can yield between
/// batches of packets. [`decode`] runs it to completion.
pub struct Decoder<'a> {
    reader: Box<dyn FormatReader + 'a>,
    codec: Codec,
    track_id: u32,
    /// MP3: undecodable packets become silence (their full length).
    silence_errors: bool,
    out: DecodedAudio,
    /// Frames the codec gave (not silence put in).
    audio_frames: usize,
    /// Frames still to drop from the start (`skip`).
    skip: usize,
    /// Container errors in a row since the last packet.
    errors: u32,
    done: bool,
    /// The stream's channels (`out` may keep the first only).
    planes: usize,
    /// The other channels' samples of a packet, when only the first is
    /// kept.
    scratch: Vec<Vec<f32>>,
}

impl<'a> Decoder<'a> {
    /// Opens an Ogg Vorbis stream or an MP3 held in memory. Reads the
    /// headers (and an Ogg's last page, for its length); no audio is
    /// decoded yet.
    pub fn new(bytes: &'a [u8]) -> Result<Decoder<'a>, DecodeError> {
        Decoder::open(bytes, None, false)
    }

    /// [`Decoder::new`] for the first `seconds` of the audio only: room
    /// is reserved for that, not for the whole song (wasm memory, once
    /// grown, never shrinks). Decoding further still works; the
    /// channels then grow as they go.
    pub fn start(bytes: &'a [u8], seconds: f64) -> Result<Decoder<'a>, DecodeError> {
        Decoder::open(bytes, Some(seconds), false)
    }

    /// [`Decoder::start`], keeping the first channel only ([`Self::left`]:
    /// what the web lines a browser's decode up with, at half a stereo
    /// song's memory).
    pub fn start_left(bytes: &'a [u8], seconds: f64) -> Result<Decoder<'a>, DecodeError> {
        Decoder::open(bytes, Some(seconds), true)
    }

    fn open(bytes: &'a [u8], seconds: Option<f64>, left: bool) -> Result<Decoder<'a>, DecodeError> {
        if bytes.starts_with(b"OggS") {
            return Decoder::ogg(bytes, seconds, left);
        }
        // An ID3v2 tag or a frame first with no frame after: the MP3
        // decoder says why.
        if is_mp3(bytes) || sniff(bytes) == "MP3" {
            return Decoder::mp3_with(bytes, seconds, left);
        }
        if crate::mp3::free_format(bytes) {
            return Err(DecodeError::UnsupportedCodec("free-format MP3"));
        }
        Err(DecodeError::UnsupportedContainer(sniff(bytes)))
    }

    /// A decoder of `reader`'s track `track_id` through `codec`, putting
    /// its `planes` into `channels` (reserved, maybe the first only).
    fn with(
        reader: Box<dyn FormatReader + 'a>,
        codec: Codec,
        track_id: u32,
        rate: u32,
        channels: Vec<Vec<f32>>,
        planes: usize,
        silence_errors: bool,
    ) -> Decoder<'a> {
        Decoder {
            reader,
            codec,
            track_id,
            silence_errors,
            audio_frames: 0,
            skip: 0,
            errors: 0,
            out: DecodedAudio {
                sample_rate: rate,
                channels,
                skipped_packets: 0,
            },
            done: false,
            planes,
            scratch: Vec::new(),
        }
    }

    /// The Ogg reader of `bytes`, cut after their last whole page
    /// ([`ogg_pages`]).
    fn ogg_reader(bytes: &'a [u8]) -> Result<OggReader<'a>, DecodeError> {
        let bytes = &bytes[..ogg_pages(bytes)];
        let source = MediaSourceStream::new(Box::new(Cursor::new(bytes)), Default::default());
        OggReader::try_new(source, FormatOptions::default())
            .map_err(|e| DecodeError::Malformed(e.to_string()))
    }

    fn ogg(bytes: &'a [u8], seconds: Option<f64>, left: bool) -> Result<Decoder<'a>, DecodeError> {
        let reader = Decoder::ogg_reader(bytes)?;

        let audio_tracks = || {
            reader.tracks().iter().filter_map(|track| {
                let params = track.codec_params.as_ref()?.audio()?;
                Some((track, params))
            })
        };
        let Some((track, params)) = audio_tracks().find(|(_, p)| p.codec == CODEC_ID_VORBIS) else {
            #[cfg(feature = "opus")]
            if audio_tracks().any(|(_, p)| p.codec == CODEC_ID_OPUS) {
                return Decoder::opus(Box::new(reader), seconds, left);
            }
            return Err(match audio_tracks().next() {
                Some((_, p)) if p.codec == CODEC_ID_OPUS => {
                    DecodeError::UnsupportedCodec("Ogg Opus")
                }
                Some((_, p)) if p.codec == CODEC_ID_FLAC => {
                    DecodeError::UnsupportedCodec("Ogg FLAC")
                }
                Some(_) => DecodeError::UnsupportedCodec("Ogg audio of an unknown codec"),
                None => DecodeError::NoAudioTrack,
            });
        };
        let decoder = VorbisDecoder::try_new(params, &AudioDecoderOptions::default())
            .map_err(|e| DecodeError::Malformed(e.to_string()))?;
        let (track_id, rate, channels, frames) = Decoder::shape(track, params)?;
        Ok(Decoder::with(
            Box::new(reader),
            Codec::Symphonia(Box::new(decoder)),
            track_id,
            rate,
            reserve(kept(channels, left), cap(frames, rate, seconds), rate),
            channels,
            false,
        ))
    }

    /// An Ogg Opus stream held in memory, and nothing else (the Vorbis and
    /// MP3 decoders are not reached from here, so the web's Opus worker,
    /// which calls only this, leaves them out).
    #[cfg(feature = "opus")]
    pub fn ogg_opus(bytes: &'a [u8]) -> Result<Decoder<'a>, DecodeError> {
        if !bytes.starts_with(b"OggS") {
            return Err(DecodeError::UnsupportedContainer(sniff(bytes)));
        }
        let reader = Decoder::ogg_reader(bytes)?;
        Decoder::opus(Box::new(reader), None, false)
    }

    /// The Ogg Opus stream `reader` reads (its first Opus track), with
    /// room reserved for `seconds` when given and the first channel only
    /// kept when `left`.
    #[cfg(feature = "opus")]
    pub(crate) fn opus(
        reader: Box<dyn FormatReader + 'a>,
        seconds: Option<f64>,
        left: bool,
    ) -> Result<Decoder<'a>, DecodeError> {
        let track = reader
            .tracks()
            .iter()
            .find(|t| {
                t.codec_params
                    .as_ref()
                    .and_then(|p| p.audio())
                    .is_some_and(|p| p.codec == CODEC_ID_OPUS)
            })
            .ok_or(DecodeError::NoAudioTrack)?;
        let head = track
            .codec_params
            .as_ref()
            .and_then(|p| p.audio())
            .and_then(|p| p.extra_data.as_deref())
            .and_then(crate::opus::head)
            .ok_or_else(|| DecodeError::Malformed("no OpusHead".into()))?;
        let stream = crate::opus::Stream::new(head).ok_or(DecodeError::UnsupportedCodec(
            "Ogg Opus of more than two channels",
        ))?;
        let track_id = track.id;
        let end = track
            .num_frames
            .map(|granule| (granule as usize).saturating_sub(head.pre_skip));
        let rate = crate::opus::RATE;
        let channels = head.channels;
        Ok(Decoder::with(
            reader,
            Codec::Opus(Box::new(OpusState {
                stream,
                at: 0,
                end,
                gaps: Gaps::default(),
            })),
            track_id,
            rate,
            reserve(
                kept(channels, left),
                cap(end.unwrap_or(0) as u64, rate, seconds),
                rate,
            ),
            channels,
            false,
        ))
    }

    /// An MP3 as StepMania 5.1 decodes it (the module's doc), whatever
    /// its first bytes are: the first frame is looked for as StepMania
    /// does, through up to [`crate::mp3::RESYNC`] bytes of junk and any
    /// ID3v2 tags ("Can't find data" past that).
    pub fn mp3(bytes: &'a [u8]) -> Result<Decoder<'a>, DecodeError> {
        Decoder::mp3_part(bytes, None)
    }

    /// [`Decoder::mp3`], with room reserved for `seconds` when given.
    pub fn mp3_part(bytes: &'a [u8], seconds: Option<f64>) -> Result<Decoder<'a>, DecodeError> {
        Decoder::mp3_with(bytes, seconds, false)
    }

    fn mp3_with(
        bytes: &'a [u8],
        seconds: Option<f64>,
        left: bool,
    ) -> Result<Decoder<'a>, DecodeError> {
        let first = crate::mp3::first_frame(bytes).ok_or_else(|| {
            if crate::mp3::free_format(bytes) {
                DecodeError::UnsupportedCodec("free-format MP3")
            } else {
                DecodeError::Malformed(format!(
                    "no MP3 frame in the first {} bytes",
                    crate::mp3::RESYNC
                ))
            }
        })?;
        // The reader starts at the first frame, as libmad syncs to it.
        let at = first.at;
        // Later tag frames (files joined end to end) are frames of
        // silence to StepMania; Symphonia drops them, unless their tag
        // is no longer one.
        let later = crate::mp3::later_tags(bytes, &first);
        let data: Cow<'a, [u8]> = if later.is_empty() {
            Cow::Borrowed(&bytes[at..])
        } else {
            let mut owned = bytes[at..].to_vec();
            for p in later {
                owned[p - at..p - at + 4].fill(0);
            }
            Cow::Owned(owned)
        };
        let source = MediaSourceStream::new(Box::new(Cursor::new(data)), Default::default());
        let reader = MpaReader::try_new(source, FormatOptions::default())
            .map_err(|e| DecodeError::Malformed(e.to_string()))?;
        let (track, params) = reader
            .tracks()
            .iter()
            .find_map(|t| {
                let p = t.codec_params.as_ref()?.audio()?;
                (p.codec == CODEC_ID_MP3).then_some((t, p))
            })
            .ok_or(DecodeError::NoAudioTrack)?;
        let decoder = MpaDecoder::try_new(params, &AudioDecoderOptions::default().gapless(false))
            .map_err(|e| DecodeError::Malformed(e.to_string()))?;
        let (track_id, rate, channels, frames) = Decoder::shape(track, params)?;
        // The tag frame StepMania plays as silence (Symphonia drops it).
        let silence = first.silence();
        let frames = cap(frames + silence as u64 + 2 * 1152 + 1105, rate, seconds);
        let planes = channels;
        let mut channels = reserve(kept(channels, left), frames, rate);
        for c in &mut channels {
            c.resize(silence, 0.0);
        }
        Ok(Decoder::with(
            Box::new(reader),
            Codec::Symphonia(Box::new(decoder)),
            track_id,
            rate,
            channels,
            planes,
            true,
        ))
    }

    /// A track's id, rate, channels and length (frames, 0 if unknown).
    fn shape(
        track: &symphonia::core::formats::Track,
        params: &symphonia::core::codecs::audio::AudioCodecParameters,
    ) -> Result<(u32, u32, usize, u64), DecodeError> {
        let rate = params
            .sample_rate
            .filter(|&rate| rate > 0)
            .ok_or_else(|| DecodeError::Malformed("missing sample rate".into()))?;
        let channels = params
            .channels
            .as_ref()
            .map(|c| c.count())
            .filter(|&n| n > 0)
            .ok_or_else(|| DecodeError::Malformed("missing channel layout".into()))?;
        Ok((track.id, rate, channels, track.num_frames.unwrap_or(0)))
    }

    /// The audio's rate and channels.
    pub fn sample_rate(&self) -> u32 {
        self.out.sample_rate
    }

    /// Frames decoded so far (kept: after those skipped).
    pub fn frames(&self) -> usize {
        self.out.frames()
    }

    /// Drops the first `frames` frames of the audio as they are decoded
    /// (a preview needs its part, not what is before), counting the
    /// silence an MP3's tag frame is.
    pub fn skip(&mut self, frames: usize) {
        self.skip = frames;
        self.drop_skipped();
    }

    fn drop_skipped(&mut self) {
        if self.skip == 0 {
            return;
        }
        let n = self.skip.min(self.out.frames());
        for c in &mut self.out.channels {
            c.drain(..n);
        }
        self.skip -= n;
    }

    /// The first channel as decoded so far.
    pub fn left(&self) -> &[f32] {
        self.out.channels.first().map_or(&[], Vec::as_slice)
    }

    /// Decodes up to `max_packets` packets of the audio track. Returns
    /// `Ok(true)` once the end of the stream has been reached.
    ///
    /// Packets the codec rejects are skipped, as in Symphonia's examples,
    /// leaving a short gap (in practice the Vorbis decoder is lenient and
    /// turns most garbage into noise instead), or for MP3 decoded as
    /// silence of their length; pages with a bad CRC are dropped by the
    /// reader before that. A truncated file ends the stream early. Other
    /// container errors are fatal before any audio; after, the reader is
    /// asked again (it resyncs past a broken page or frame), and after
    /// [`MAX_ERRORS`] in a row the stream ends there (garbage after the
    /// music).
    pub fn step(&mut self, max_packets: usize) -> Result<bool, DecodeError> {
        let mut decoded = 0;
        while !self.done && decoded < max_packets {
            let packet = match self.reader.next_packet() {
                Ok(Some(packet)) => packet,
                // End of stream, or a chained Ogg stream starting (a new
                // song in the same file, as far as we are concerned).
                Ok(None) | Err(SymphoniaError::ResetRequired) => {
                    self.done = true;
                    break;
                }
                Err(SymphoniaError::IoError(e))
                    if e.kind() == std::io::ErrorKind::UnexpectedEof =>
                {
                    self.done = true;
                    break;
                }
                Err(e) if self.audio_frames == 0 => {
                    return Err(DecodeError::Malformed(e.to_string()));
                }
                Err(_) => {
                    self.errors += 1;
                    self.out.skipped_packets += 1;
                    if self.errors >= MAX_ERRORS {
                        self.done = true;
                    }
                    continue;
                }
            };
            self.errors = 0;
            if packet.track_id != self.track_id {
                continue;
            }
            decoded += 1;
            // One variant without the feature `opus`.
            #[allow(clippy::infallible_destructuring_match)]
            let decoder = match &mut self.codec {
                Codec::Symphonia(decoder) => decoder,
                #[cfg(feature = "opus")]
                Codec::Opus(_) => {
                    self.opus_packet(&packet);
                    continue;
                }
            };
            let buf = match decoder.decode(&packet) {
                Ok(buf) => buf,
                Err(SymphoniaError::DecodeError(_) | SymphoniaError::IoError(_)) => {
                    self.out.skipped_packets += 1;
                    if self.silence_errors {
                        // The whole frame, as the decoder would have
                        // given it untrimmed.
                        let frames = (packet.dur.get()
                            + packet.trim_start.get()
                            + packet.trim_end.get()) as usize;
                        for c in &mut self.out.channels {
                            c.resize(c.len() + frames, 0.0);
                        }
                        self.drop_skipped();
                    }
                    continue;
                }
                Err(e) => return Err(DecodeError::Malformed(e.to_string())),
            };
            let frames = buf.frames();
            if frames == 0 {
                continue;
            }
            if buf.num_planes() != self.planes {
                self.out.skipped_packets += 1;
                continue;
            }
            // Grow every channel kept by `frames` and let Symphonia write
            // (and convert, should a decoder ever emit non-f32) into the
            // tails; the others into the scratch.
            let extra = self.planes - self.out.channels.len();
            self.scratch.resize_with(extra, Vec::new);
            let mut tails: Vec<&mut [f32]> = self
                .out
                .channels
                .iter_mut()
                .map(|channel| {
                    let start = channel.len();
                    channel.resize(start + frames, 0.0);
                    &mut channel[start..]
                })
                .chain(self.scratch.iter_mut().map(|c| {
                    c.resize(frames, 0.0);
                    &mut c[..]
                }))
                .collect();
            buf.copy_to_slice_planar(&mut tails);
            self.audio_frames += frames;
            self.drop_skipped();
        }
        Ok(self.done)
    }

    /// One Opus packet into the channels: the frames lost before it (a
    /// dropped page) as silence, its own after the pre-skip, up to the
    /// stream's end.
    #[cfg(feature = "opus")]
    fn opus_packet(&mut self, packet: &symphonia::core::packet::Packet) {
        let Codec::Opus(o) = &mut self.codec else {
            return;
        };
        let lost = o.gaps.before(packet);
        let silence = o.stream.gap(lost);
        let ch = o.stream.channels();
        let (pcm, failed) = o.stream.decode(&packet.data, packet.dur.get());
        if failed {
            self.out.skipped_packets += 1;
        }
        let room = o.end.map_or(usize::MAX, |e| e.saturating_sub(o.at));
        let silence = silence.min(room);
        let n = (pcm.len() / ch).min(room - silence);
        for (k, c) in self.out.channels.iter_mut().enumerate() {
            c.resize(c.len() + silence, 0.0);
            c.extend(pcm[..n * ch].iter().skip(k).step_by(ch));
        }
        o.at += silence + n;
        self.audio_frames += n;
        if o.end.is_some_and(|e| o.at >= e) {
            self.done = true;
        }
        self.drop_skipped();
    }

    /// The decoded audio, once [`step`](Self::step) has returned `Ok(true)`
    /// (calling it earlier returns what has been decoded so far).
    pub fn finish(self) -> Result<DecodedAudio, DecodeError> {
        if self.audio_frames == 0 {
            return Err(DecodeError::NoAudio {
                skipped_packets: self.out.skipped_packets,
            });
        }
        Ok(self.out)
    }
}

/// The frames to reserve: the stream's, or `seconds`' when only a start
/// is wanted, with room for the batch of packets that passes it (a
/// caller stepping 8 or 16 packets at a time; a `Vec` that outgrows its
/// room doubles).
fn cap(frames: u64, rate: u32, seconds: Option<f64>) -> u64 {
    match seconds {
        Some(s) => {
            let want = (s.max(0.0) * f64::from(rate)).ceil() as u64 + 32 * 1152;
            if frames == 0 { want } else { frames.min(want) }
        }
        None => frames,
    }
}

/// The channels kept of `channels`.
fn kept(channels: usize, left: bool) -> usize {
    if left { channels.min(1) } else { channels }
}

/// Planar channels reserved for `frames` (capped).
pub(crate) fn reserve(channels: usize, frames: u64, rate: u32) -> Vec<Vec<f32>> {
    let frames = frames.min(u64::from(rate) * MAX_RESERVE_SECONDS) as usize;
    (0..channels).map(|_| Vec::with_capacity(frames)).collect()
}

/// Decodes a whole Ogg Vorbis file or MP3 held in memory.
pub fn decode(bytes: &[u8]) -> Result<DecodedAudio, DecodeError> {
    let mut decoder = Decoder::new(bytes)?;
    while !decoder.step(usize::MAX)? {}
    decoder.finish()
}

/// The first `seconds` of an MP3 (or Ogg Vorbis file), decoded (as
/// [`decode`] does, cut there); what the web compares a browser's decode
/// with.
pub fn decode_start(bytes: &[u8], seconds: f64) -> Result<DecodedAudio, DecodeError> {
    let mut decoder = Decoder::start(bytes, seconds)?;
    let want = (seconds.max(0.0) * f64::from(decoder.sample_rate())).ceil() as usize;
    while decoder.frames() < want && !decoder.step(8)? {}
    let mut audio = decoder.finish()?;
    for c in &mut audio.channels {
        c.truncate(want);
    }
    Ok(audio)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 0.5 s stereo sine, 440 Hz left and 880 Hz right, amplitude 1/8,
    /// 44.1 kHz, Ogg Vorbis from libvorbis (6.5 KB, all audio on one Ogg
    /// page). Our own work, MIT like the rest of the repository. Made with
    /// ffmpeg 8.1.2 and libsndfile 1.2.2's `sndfile-convert` (libvorbis
    /// 1.3.7, default quality):
    ///
    /// ```sh
    /// ffmpeg -f lavfi -i "sine=frequency=440:sample_rate=44100:duration=0.5" \
    ///   -f lavfi -i "sine=frequency=880:sample_rate=44100:duration=0.5" \
    ///   -filter_complex "[0:a][1:a]join=inputs=2:channel_layout=stereo[a]" -map "[a]" \
    ///   -c:a pcm_f32le -fflags +bitexact -flags:a +bitexact -map_metadata -1 sine.wav
    /// sndfile-convert -vorbis sine.wav sine-440l-880r.ogg
    /// ```
    ///
    /// Not ffmpeg's built-in `vorbis` encoder: its stereo streams decode
    /// differently in libvorbis (which Symphonia matches bit for bit) and in
    /// ffmpeg's own decoder, so they make a poor reference.
    const SINE: &[u8] = include_bytes!("../tests/fixtures/sine-440l-880r.ogg");
    /// 0.1 s 440 Hz mono sine in Ogg Opus (475 bytes), made with
    /// `ffmpeg -f lavfi -i "sine=frequency=440:sample_rate=48000:duration=0.1"
    /// -c:a libopus -b:a 16k -fflags +bitexact -flags:a +bitexact
    /// -map_metadata -1 sine-440.opus`.
    const OPUS: &[u8] = include_bytes!("../tests/fixtures/sine-440.opus");
    const RATE: f64 = 44_100.0;
    const FRAMES: usize = 22_050;

    /// Amplitude and phase of the `freq` component of `samples`, by
    /// correlation with a unit sine and cosine.
    fn component(samples: &[f32], freq: f64) -> (f64, f64) {
        let (mut s, mut c) = (0.0, 0.0);
        for (n, &x) in samples.iter().enumerate() {
            let w = std::f64::consts::TAU * freq * n as f64 / RATE;
            s += f64::from(x) * w.sin();
            c += f64::from(x) * w.cos();
        }
        let scale = 2.0 / samples.len() as f64;
        ((s * s + c * c).sqrt() * scale, c.atan2(s))
    }

    #[test]
    fn decodes_length_rate_and_channels() {
        let audio = decode(SINE).unwrap();
        assert_eq!(audio.sample_rate, 44_100);
        assert_eq!(audio.channels.len(), 2);
        // Exact: the encoder delay and the end padding are both trimmed.
        assert_eq!(audio.frames(), FRAMES);
        assert!(audio.channels.iter().all(|c| c.len() == FRAMES));
        assert_eq!(audio.skipped_packets, 0);
        assert!((audio.duration() - 0.5).abs() < 1e-9);
    }

    /// The web's alignment decodes the left channel only: the same
    /// samples, half the memory, room for the start asked for.
    #[test]
    fn keeps_the_left_channel_only_when_asked() {
        let whole = decode(SINE).unwrap();
        let mut d = Decoder::start_left(SINE, 0.1).unwrap();
        assert_eq!(d.out.channels.len(), 1);
        let room = d.out.channels[0].capacity();
        while !d.step(8).unwrap() {}
        assert_eq!(d.left(), &whole.channels[0][..]);
        assert!(room >= (0.1 * 44_100.0) as usize);
        let info = include_bytes!("../tests/fixtures/sine-441-info.mp3");
        let mut m = Decoder::start_left(info, 8.0).unwrap();
        while !m.step(8).unwrap() {}
        assert_eq!(m.left(), &decode(info).unwrap().channels[0][..]);
    }

    #[test]
    fn reserves_exactly_once() {
        // The reader knows the length up front, so no Vec ever regrows.
        let audio = decode(SINE).unwrap();
        for channel in &audio.channels {
            assert_eq!(channel.capacity(), channel.len());
        }
    }

    #[test]
    fn sine_is_in_the_right_channel_and_phase() {
        let audio = decode(SINE).unwrap();
        // Skip the first and last 50 ms, where the codec smears the onset
        // and the abrupt stop.
        let mid = 2_205..FRAMES - 2_205;
        let offset = mid.start as f64;
        let (left, right) = (&audio.channels[0][mid.clone()], &audio.channels[1][mid]);
        for (samples, wanted, other) in [(left, 440.0, 880.0), (right, 880.0, 440.0)] {
            let (amp, phase) = component(samples, wanted);
            assert!((amp - 0.125).abs() < 0.005, "{wanted} Hz amplitude {amp}");
            // ffmpeg's sine source has phase 0 at frame 0, so after `offset`
            // frames it is 2π·f·offset/rate. Keeping the encoder delay, or
            // dropping a block, would put it hundreds of frames off.
            let expected = std::f64::consts::TAU * wanted * offset / RATE;
            let error = (phase - expected).rem_euclid(std::f64::consts::TAU);
            let error = error.min(std::f64::consts::TAU - error);
            let frames_off = error / std::f64::consts::TAU * RATE / wanted;
            assert!(frames_off < 0.5, "{wanted} Hz off by {frames_off} frames");
            let (leak, _) = component(samples, other);
            assert!(
                leak < 0.002,
                "{other} Hz leaked into the {wanted} Hz channel: {leak}"
            );
        }
    }

    #[test]
    fn incremental_matches_one_shot() {
        let mut decoder = Decoder::new(SINE).unwrap();
        let mut steps = 0;
        while !decoder.step(1).unwrap() {
            steps += 1;
        }
        assert!(steps > 10, "{steps} steps");
        assert_eq!(decoder.finish().unwrap(), decode(SINE).unwrap());
    }

    /// An Ogg page split into its parts.
    struct Page {
        header: Vec<u8>,
        packets: Vec<Vec<u8>>,
    }

    /// Splits `bytes` into pages (assumes no packet spans two pages, true
    /// of the fixtures' audio pages).
    fn pages(bytes: &[u8]) -> Vec<Page> {
        let mut out = Vec::new();
        let mut at = 0;
        while at < bytes.len() {
            assert_eq!(&bytes[at..at + 4], b"OggS");
            let segments = usize::from(bytes[at + 26]);
            let lacing = &bytes[at + 27..at + 27 + segments];
            let mut body = at + 27 + segments;
            let mut packets = vec![Vec::new()];
            for &len in lacing {
                packets
                    .last_mut()
                    .unwrap()
                    .extend_from_slice(&bytes[body..body + usize::from(len)]);
                body += usize::from(len);
                if len < 255 {
                    packets.push(Vec::new());
                }
            }
            packets.pop_if(|p| p.is_empty());
            out.push(Page {
                header: bytes[at..at + 27].to_vec(),
                packets,
            });
            at = body;
        }
        out
    }

    /// Serialises pages back, recomputing lacing and CRCs.
    fn unpages(pages: &[Page]) -> Vec<u8> {
        let mut out = Vec::new();
        for page in pages {
            let mut lacing = Vec::new();
            for packet in &page.packets {
                lacing.extend(std::iter::repeat_n(255u8, packet.len() / 255));
                lacing.push((packet.len() % 255) as u8);
            }
            let start = out.len();
            out.extend_from_slice(&page.header);
            out[start + 26] = lacing.len() as u8;
            out.extend_from_slice(&lacing);
            for packet in &page.packets {
                out.extend_from_slice(packet);
            }
            let crc = ogg_crc(&out[start..]);
            out[start + 22..start + 26].copy_from_slice(&crc.to_le_bytes());
        }
        out
    }

    /// Ogg's CRC-32 (polynomial 0x04c11db7, unreflected, zero init) over a
    /// page, with its checksum field read as zero.
    fn ogg_crc(page: &[u8]) -> u32 {
        let mut crc = 0u32;
        for (i, &byte) in page.iter().enumerate() {
            let byte = if (22..26).contains(&i) { 0 } else { byte };
            crc ^= u32::from(byte) << 24;
            for _ in 0..8 {
                crc = if crc & 0x8000_0000 != 0 {
                    (crc << 1) ^ 0x04c1_1db7
                } else {
                    crc << 1
                };
            }
        }
        crc
    }

    #[test]
    fn page_helpers_round_trip() {
        let parsed = pages(SINE);
        // Identification header, comment + setup headers, audio.
        assert_eq!(parsed.len(), 3);
        assert_eq!(parsed[1].packets.len(), 2);
        assert_eq!(unpages(&parsed), SINE);
    }

    #[test]
    fn survives_a_garbage_packet_mid_stream() {
        // Scramble one audio packet in the middle, keeping its first byte
        // (packet type and mode) so the Ogg reader still passes it on.
        // Symphonia's Vorbis decoder is lenient and turns it into a burst
        // of noise rather than an error; either way decoding must carry on
        // and resynchronise with the packets after it.
        let mut parsed = pages(SINE);
        let audio = &mut parsed[2].packets;
        let victim = audio.len() / 2;
        let mut x = 0x9e37_79b9u32;
        for b in audio[victim].iter_mut().skip(1) {
            x ^= x << 13;
            x ^= x >> 17;
            x ^= x << 5;
            *b = x as u8;
        }
        let decoded = decode(&unpages(&parsed)).unwrap();
        let clean = decode(SINE).unwrap();
        assert_eq!(decoded.frames(), FRAMES);
        assert!(decoded.channels.iter().flatten().all(|s| s.is_finite()));
        // The damage stays within the packets overlapping the victim; the
        // last 4096 frames are bit-identical to a clean decode.
        for (got, want) in decoded.channels.iter().zip(&clean.channels) {
            assert_ne!(got, want);
            assert_eq!(got[FRAMES - 4096..], want[FRAMES - 4096..]);
        }
    }

    #[test]
    fn nothing_decodable_is_an_error_not_a_panic() {
        // Only the first audio packet: Vorbis needs two packets to overlap
        // before it outputs anything, so the stream decodes to no frames.
        let mut parsed = pages(SINE);
        parsed[2].packets.truncate(1);
        assert_eq!(
            decode(&unpages(&parsed)),
            Err(DecodeError::NoAudio { skipped_packets: 0 })
        );

        // A bad CRC on the first audio page. Later corrupt pages are
        // dropped and skipped over, but the reader needs this one to start.
        let mut bytes = SINE.to_vec();
        let last = bytes.len() - 10;
        bytes[last] ^= 0xff;
        assert!(matches!(decode(&bytes), Err(DecodeError::Malformed(_))));

        // Headers only, a file cut off mid-page, a file cut off in the
        // headers.
        let audio_page = SINE
            .windows(4)
            .enumerate()
            .filter(|(_, w)| w == b"OggS")
            .nth(2)
            .unwrap()
            .0;
        for cut in [&SINE[..audio_page], &SINE[..SINE.len() - 500], &SINE[..100]] {
            assert!(decode(cut).is_err(), "{} bytes", cut.len());
        }
    }

    #[test]
    fn rejects_other_containers() {
        // An MP3 with nothing after its tag.
        assert!(matches!(
            decode(b"ID3\x04\0\0\0\0\0\0").unwrap_err(),
            DecodeError::Malformed(_) | DecodeError::NoAudio { .. }
        ));
        assert_eq!(
            decode(b"RIFF\0\0\0\0WAVEfmt ").unwrap_err(),
            DecodeError::UnsupportedContainer("WAV")
        );
        assert_eq!(
            decode(b"").unwrap_err(),
            DecodeError::UnsupportedContainer("this file format")
        );
    }

    /// The MP3 fixtures (see `decode_all`'s tests for how they were made
    /// and checked against FFmpeg's untrimmed decode).
    const MP3_INFO: &[u8] = include_bytes!("../tests/fixtures/sine-441-info.mp3");
    const MP3_XING: &[u8] = include_bytes!("../tests/fixtures/sine-441-xing.mp3");

    #[test]
    fn mp3_decodes_as_stepmania_does() {
        assert!(is_mp3(MP3_INFO) && !is_mp3(SINE));
        let info = decode(MP3_INFO).unwrap();
        let xing = decode(MP3_XING).unwrap();
        assert_eq!(info.sample_rate, 44_100);
        // Whole frames, nothing trimmed; the `Info` frame as 1152 frames
        // of silence, then LAME's and the decoder's delay (1105).
        assert_eq!(info.frames() % 1152, 0);
        assert!(info.channels[0][..1152].iter().all(|&s| s == 0.0));
        assert!(
            info.channels[0][1152..1152 + 500]
                .iter()
                .all(|s| s.abs() < 1e-3)
        );
        // The `Xing` frame is skipped: the music starts right away, at
        // the delay.
        assert_eq!(xing.frames() % 1152, 0);
        assert!(xing.channels[0][..200].iter().all(|s| s.abs() < 1e-3));
        assert!(
            xing.channels[0][1105 + 200..1105 + 400]
                .iter()
                .any(|s| s.abs() > 0.05)
        );
        // The start of it, cut where asked.
        let start = decode_start(MP3_INFO, 0.05).unwrap();
        assert_eq!(start.frames(), 2205);
        assert_eq!(start.channels[0][..], info.channels[0][..2205]);
    }

    const MP3_BARE: &[u8] = include_bytes!("../tests/fixtures/sine-441-bare.mp3");

    #[test]
    fn mp3_found_as_stepmania_finds_it() {
        let whole = decode(MP3_INFO).unwrap();
        // Zeros first (75 of 219 local MP3s start with 417), an APE tag
        // first, a WAV of format 0x55 around it.
        let mut zeros = vec![0u8; 417];
        zeros.extend_from_slice(MP3_INFO);
        let mut ape = b"APETAGEX\xd0\x07\0\0".to_vec();
        ape.resize(32, 0);
        ape.extend_from_slice(MP3_INFO);
        let mut riff = b"RIFF\0\0\0\0WAVEfmt \x1e\0\0\0\x55\0\x01\0".to_vec();
        riff.resize(12 + 8 + 30, 0);
        riff.extend_from_slice(b"fact\x04\0\0\0\0\0\0\0data");
        riff.extend_from_slice(&(MP3_INFO.len() as u32).to_le_bytes());
        riff.extend_from_slice(MP3_INFO);
        for b in [&zeros, &ape, &riff] {
            assert!(is_mp3(b));
            assert_eq!(decode(b).unwrap().channels, whole.channels);
        }
        // A PCM WAV is not one, nor Ogg.
        let mut pcm = riff.clone();
        pcm[20] = 1;
        assert!(!is_mp3(&pcm) && !is_mp3(SINE) && !is_mp3(OPUS));
        // Free format: said so.
        let mut free = vec![0xff, 0xfb, 0x00, 0x00];
        free.resize(2000, 0x55);
        assert_eq!(
            decode(&free).unwrap_err(),
            DecodeError::UnsupportedCodec("free-format MP3")
        );
    }

    #[test]
    fn mp3_tag_frames_stepmania_plays_as_silence() {
        let info = decode(MP3_INFO).unwrap();
        let xing = decode(MP3_XING).unwrap();
        let bare = decode(MP3_BARE).unwrap();
        // Files joined end to end: the second file's `Xing` frame is a
        // frame of silence to StepMania (only the first frame is looked
        // at for a tag), not dropped.
        let mut joined = MP3_INFO.to_vec();
        joined.extend_from_slice(MP3_XING);
        joined.extend_from_slice(MP3_BARE);
        let j = decode(&joined).unwrap();
        assert_eq!(
            j.frames(),
            info.frames() + 1152 + xing.frames() + bare.frames()
        );
        // The second file's music where StepMania has it (the first
        // file's last frame overlaps into the silent one).
        let at = info.frames() + 1152;
        for (a, b) in j.channels[0][at + 2000..at + 4000]
            .iter()
            .zip(&xing.channels[0][2000..4000])
        {
            assert!((a - b).abs() < 1e-3, "{a} {b}");
        }
        // A `Xing` frame in a file with CRCs: StepMania misses the tag
        // and decodes the frame, as silence.
        let mut crc = MP3_XING.to_vec();
        crc[1] &= !1;
        let c = decode(&crc).unwrap();
        assert_eq!(c.frames(), xing.frames() + 1152);
        assert!(c.channels[0][..1152].iter().all(|&s| s == 0.0));
        assert_eq!(c.channels[0][1152..], xing.channels[0][..]);
    }

    #[test]
    fn a_start_reserves_only_the_start() {
        // A five-minute song's first 8 s: room for those (and a batch of
        // packets past them), not for the song; a short stream: itself.
        let room = 8 * 44_100 + 32 * 1152;
        assert_eq!(cap(300 * 44_100, 44_100, Some(8.0)), room);
        assert_eq!(cap(1000, 44_100, Some(8.0)), 1000);
        assert_eq!(cap(0, 44_100, Some(8.0)), room);
        assert_eq!(cap(300 * 44_100, 44_100, None), 300 * 44_100);
        let mut d = Decoder::start(MP3_INFO, 0.05).unwrap();
        let reserved = d.out.channels[0].capacity();
        while !d.step(8).unwrap() {}
        // Never grown.
        assert_eq!(d.out.channels[0].capacity(), reserved);
    }

    #[cfg(not(feature = "opus"))]
    #[test]
    fn reports_ogg_opus_as_unsupported() {
        let err = decode(OPUS).unwrap_err();
        assert_eq!(err, DecodeError::UnsupportedCodec("Ogg Opus"));
        assert_eq!(
            err.to_string(),
            "Ogg Opus is not supported (only Ogg Vorbis and MP3 are)"
        );
    }

    #[test]
    fn tells_ogg_opus() {
        assert!(is_ogg_opus(OPUS));
        assert!(!is_ogg_opus(SINE) && !is_ogg_opus(MP3_INFO) && !is_ogg_opus(b"OggS"));
        assert!(!is_ogg_opus(&OPUS[..30]));
    }

    /// Ogg Opus with the feature on: the pre-skip dropped (312 frames
    /// here), the stream ended at its last granule (0.1 s), stepping as
    /// the web does giving the same samples as one go.
    #[cfg(feature = "opus")]
    #[test]
    fn decodes_ogg_opus() {
        let audio = decode(OPUS).unwrap();
        assert_eq!(audio.sample_rate, 48_000);
        assert_eq!(audio.channels.len(), 1);
        assert_eq!(audio.frames(), 4_800);
        assert_eq!(audio.skipped_packets, 0);
        let mut d = Decoder::new(OPUS).unwrap();
        while !d.step(1).unwrap() {}
        assert_eq!(d.finish().unwrap(), audio);
        // Reserved once, from the last granule.
        assert_eq!(audio.channels[0].capacity(), audio.frames());
        let mut d = Decoder::start_left(OPUS, 0.05).unwrap();
        d.skip(100);
        while !d.step(1).unwrap() {}
        assert_eq!(d.left(), &audio.channels[0][100..]);
    }

    /// The bundled songs (all stereo Ogg Opus) through the web's fallback
    /// give desktop's decode, which `decode_all`'s tests check against
    /// libopus, sample for sample.
    #[cfg(feature = "decode-all")]
    #[test]
    fn bundled_opus_songs_decode_as_on_desktop() {
        let songs = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../assets/songs");
        let mut checked = 0;
        for dir in std::fs::read_dir(&songs).unwrap().flatten() {
            let Some(file) = std::fs::read_dir(dir.path())
                .unwrap()
                .flatten()
                .map(|e| e.path())
                .find(|p| p.extension().is_some_and(|e| e == "opus"))
            else {
                continue;
            };
            let bytes = std::fs::read(&file).unwrap();
            let web = decode(&bytes).unwrap();
            let desktop = crate::decode_all::decode(&bytes, Some("opus")).unwrap();
            assert_eq!(web.channels.len(), 2, "{}", file.display());
            assert!(web == desktop, "{}", file.display());
            // The web's worker: the same samples, as a float WAV.
            assert!(is_ogg_opus(&bytes));
            let (wav, skipped) = crate::opus::decode_wav(&bytes).unwrap();
            assert_eq!(skipped, 0);
            assert_eq!(&wav[..4], b"RIFF");
            assert_eq!(wav.len(), 44 + web.frames() * 2 * 4);
            assert_eq!(u32::from_le_bytes(wav[24..28].try_into().unwrap()), 48_000);
            let back = crate::decode_all::decode(&wav, Some("wav")).unwrap();
            assert!(back == web, "{}", file.display());
            checked += 1;
        }
        assert_eq!(checked, 4);
    }

    /// Opus of mapping family 1 (surround) is said to be unsupported, not
    /// decoded wrong.
    #[cfg(feature = "opus")]
    #[test]
    fn rejects_surround_opus() {
        let mut head = b"OpusHead\x01\x06\x38\x01\x80\xbb\0\0\0\0\x01".to_vec();
        head.extend_from_slice(&[4, 2, 0, 4, 1, 2, 3, 5]);
        let h = crate::opus::head(&head).unwrap();
        assert_eq!((h.channels, h.pre_skip, h.family), (6, 312, 1));
        assert!(crate::opus::Stream::new(h).is_none());
        let stereo = crate::opus::head(b"OpusHead\x01\x02\x38\x01\x80\xbb\0\0\0\0\0").unwrap();
        assert!(crate::opus::Stream::new(stereo).is_some());
    }
}
