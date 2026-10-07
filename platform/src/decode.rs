//! Pure-Rust audio decoding, the fallback for files the browser's
//! `decodeAudioData` rejects (Ogg Vorbis on Safari, mostly).
//!
//! Only Ogg Vorbis is decoded here, through Symphonia. MP3, WAV and FLAC are
//! decoded natively by every target browser, and Symphonia has no pure-Rust
//! Opus decoder, so Ogg Opus is reported as [`DecodeError::UnsupportedCodec`].
//! See `docs/research/tech-stack.md` §6.
//!
//! The PCM is written straight into one `Vec<f32>` per channel (no
//! interleaved intermediate), reserved up front from the stream length the
//! Ogg reader finds in the last page, so a song costs its PCM size once
//! (a 3-minute stereo 44.1 kHz song is ~64 MB) instead of up to twice that
//! from `Vec` doubling.

use std::fmt;
use std::io::Cursor;

use symphonia::core::codecs::audio::well_known::{CODEC_ID_FLAC, CODEC_ID_OPUS, CODEC_ID_VORBIS};
use symphonia::core::codecs::audio::{AudioDecoder, AudioDecoderOptions};
use symphonia::core::errors::Error as SymphoniaError;
use symphonia::core::formats::{FormatOptions, FormatReader};
use symphonia::core::io::MediaSourceStream;
use symphonia::default::codecs::VorbisDecoder;
use symphonia::default::formats::OggReader;

/// Upper bound on the up-front reservation, so a corrupt length field in the
/// last page cannot request gigabytes. Longer streams still decode; they
/// just grow the buffers as they go.
const MAX_RESERVE_SECONDS: u64 = 20 * 60;

/// Decoded audio at the file's own sample rate, one sample vector per
/// channel, all of equal length.
#[derive(Clone, Debug, PartialEq)]
pub struct DecodedAudio {
    pub sample_rate: u32,
    pub channels: Vec<Vec<f32>>,
    /// Packets that failed to decode and were skipped (left as a gap).
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
    /// Not an Ogg stream. Holds the sniffed format (`"MP3"`, `"WAV"`, …).
    UnsupportedContainer(&'static str),
    /// An Ogg stream whose audio codec this decoder does not handle.
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
                write!(f, "{kind} is not supported (only Ogg Vorbis is)")
            }
            DecodeError::UnsupportedCodec(codec) => {
                write!(f, "Ogg {codec} is not supported (only Ogg Vorbis is)")
            }
            DecodeError::NoAudioTrack => f.write_str("the Ogg stream has no audio track"),
            DecodeError::Malformed(msg) => write!(f, "malformed Ogg Vorbis stream: {msg}"),
            DecodeError::NoAudio { skipped_packets } => write!(
                f,
                "no audio could be decoded ({skipped_packets} packets failed)"
            ),
        }
    }
}

impl std::error::Error for DecodeError {}

/// Best-effort name of a non-Ogg file's format, for error messages.
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

/// Incremental decoder, so a caller on a UI thread can yield between
/// batches of packets. [`decode`] runs it to completion.
pub struct Decoder<'a> {
    reader: OggReader<'a>,
    decoder: VorbisDecoder,
    track_id: u32,
    out: DecodedAudio,
    done: bool,
}

impl<'a> Decoder<'a> {
    /// Opens an Ogg Vorbis stream held in memory. Reads the headers and
    /// scans the last page for the stream length; no audio is decoded yet.
    pub fn new(bytes: &'a [u8]) -> Result<Decoder<'a>, DecodeError> {
        if !bytes.starts_with(b"OggS") {
            return Err(DecodeError::UnsupportedContainer(sniff(bytes)));
        }
        let source = MediaSourceStream::new(Box::new(Cursor::new(bytes)), Default::default());
        let reader = OggReader::try_new(source, FormatOptions::default())
            .map_err(|e| DecodeError::Malformed(e.to_string()))?;

        let audio_tracks = || {
            reader.tracks().iter().filter_map(|track| {
                let params = track.codec_params.as_ref()?.audio()?;
                Some((track, params))
            })
        };
        let Some((track, params)) = audio_tracks().find(|(_, p)| p.codec == CODEC_ID_VORBIS) else {
            return Err(match audio_tracks().next() {
                Some((_, p)) if p.codec == CODEC_ID_OPUS => DecodeError::UnsupportedCodec("Opus"),
                Some((_, p)) if p.codec == CODEC_ID_FLAC => DecodeError::UnsupportedCodec("FLAC"),
                Some(_) => DecodeError::UnsupportedCodec("audio of an unknown codec"),
                None => DecodeError::NoAudioTrack,
            });
        };
        let sample_rate = params
            .sample_rate
            .filter(|&rate| rate > 0)
            .ok_or_else(|| DecodeError::Malformed("missing sample rate".into()))?;
        let channel_count = params
            .channels
            .as_ref()
            .map(|c| c.count())
            .filter(|&n| n > 0)
            .ok_or_else(|| DecodeError::Malformed("missing channel layout".into()))?;
        let decoder = VorbisDecoder::try_new(params, &AudioDecoderOptions::default())
            .map_err(|e| DecodeError::Malformed(e.to_string()))?;

        let reserve = track
            .num_frames
            .unwrap_or(0)
            .min(u64::from(sample_rate) * MAX_RESERVE_SECONDS) as usize;
        let channels = (0..channel_count)
            .map(|_| Vec::with_capacity(reserve))
            .collect();
        let track_id = track.id;
        Ok(Decoder {
            reader,
            decoder,
            track_id,
            out: DecodedAudio {
                sample_rate,
                channels,
                skipped_packets: 0,
            },
            done: false,
        })
    }

    /// Decodes up to `max_packets` packets of the audio track. Returns
    /// `Ok(true)` once the end of the stream has been reached.
    ///
    /// Packets the codec rejects are skipped, as in Symphonia's examples,
    /// leaving a short gap (in practice the Vorbis decoder is lenient and
    /// turns most garbage into noise instead); pages with a bad CRC are
    /// dropped by the reader before that. A truncated file ends the stream
    /// early. Any other container error is fatal.
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
                Err(e) => return Err(DecodeError::Malformed(e.to_string())),
            };
            if packet.track_id != self.track_id {
                continue;
            }
            decoded += 1;
            let buf = match self.decoder.decode(&packet) {
                Ok(buf) => buf,
                Err(SymphoniaError::DecodeError(_) | SymphoniaError::IoError(_)) => {
                    self.out.skipped_packets += 1;
                    continue;
                }
                Err(e) => return Err(DecodeError::Malformed(e.to_string())),
            };
            let frames = buf.frames();
            if frames == 0 {
                continue;
            }
            if buf.num_planes() != self.out.channels.len() {
                self.out.skipped_packets += 1;
                continue;
            }
            // Grow every channel by `frames` and let Symphonia write (and
            // convert, should a decoder ever emit non-f32) into the tails.
            let mut tails: Vec<&mut [f32]> = self
                .out
                .channels
                .iter_mut()
                .map(|channel| {
                    let start = channel.len();
                    channel.resize(start + frames, 0.0);
                    &mut channel[start..]
                })
                .collect();
            buf.copy_to_slice_planar(&mut tails);
        }
        Ok(self.done)
    }

    /// The decoded audio, once [`step`](Self::step) has returned `Ok(true)`
    /// (calling it earlier returns what has been decoded so far).
    pub fn finish(self) -> Result<DecodedAudio, DecodeError> {
        if self.out.frames() == 0 {
            return Err(DecodeError::NoAudio {
                skipped_packets: self.out.skipped_packets,
            });
        }
        Ok(self.out)
    }
}

/// Decodes a whole Ogg Vorbis file held in memory.
pub fn decode(bytes: &[u8]) -> Result<DecodedAudio, DecodeError> {
    let mut decoder = Decoder::new(bytes)?;
    while !decoder.step(usize::MAX)? {}
    decoder.finish()
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
        assert_eq!(
            decode(b"ID3\x04\0\0\0\0\0\0").unwrap_err(),
            DecodeError::UnsupportedContainer("MP3")
        );
        assert_eq!(
            decode(b"RIFF\0\0\0\0WAVEfmt ").unwrap_err(),
            DecodeError::UnsupportedContainer("WAV")
        );
        assert_eq!(
            decode(b"").unwrap_err(),
            DecodeError::UnsupportedContainer("this file format")
        );
    }

    #[test]
    fn reports_ogg_opus_as_unsupported() {
        let err = decode(OPUS).unwrap_err();
        assert_eq!(err, DecodeError::UnsupportedCodec("Opus"));
        assert_eq!(
            err.to_string(),
            "Ogg Opus is not supported (only Ogg Vorbis is)"
        );
    }
}
