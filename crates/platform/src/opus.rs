//! Ogg Opus's packets through `rusty-opus` (pure Rust, BSD-3-Clause,
//! within float rounding of libopus: `docs/plans/desktop.md` 7.0), as
//! RFC 7845 has them played: the `OpusHead`'s pre-skip dropped, its
//! output gain applied. The stream's end (the last granule) is the
//! caller's ([`crate::decode::Decoder`], which both shells' Opus goes
//! through: the web's fallback and desktop's `decode_all`). And Ogg Opus
//! files encoded with it ([`encode`]), for the announcer studio's packs
//! where no browser's encoder is at hand (desktop, the web's fallback).

/// Opus always decodes at 48 kHz.
pub use crate::ogg_opus::RATE;
/// The longest Opus packet: 120 ms at 48 kHz.
const MAX_FRAMES: usize = 5_760;

/// The fields of an `OpusHead` (RFC 7845, section 5.1) the decoder uses.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct Head {
    pub channels: usize,
    pub pre_skip: usize,
    /// The output gain, as a factor.
    pub gain: f32,
    pub family: u8,
}

/// The `OpusHead` packet's fields (Symphonia's Ogg reader keeps the packet
/// as the track's extra data).
pub(crate) fn head(head: &[u8]) -> Option<Head> {
    if head.len() < 19 || &head[..8] != b"OpusHead" {
        return None;
    }
    let q8 = i16::from_le_bytes([head[16], head[17]]);
    Some(Head {
        channels: usize::from(head[9]),
        pre_skip: usize::from(u16::from_le_bytes([head[10], head[11]])),
        gain: 10f32.powf(f32::from(q8) / (20.0 * 256.0)),
        family: head[18],
    })
}

/// One stream's decoder: packets in, frames out with the pre-skip dropped
/// and the gain applied.
pub(crate) struct Stream {
    decoder: rusty_opus::OpusDecoder,
    channels: usize,
    /// Frames of the pre-skip still to drop.
    pre_skip: usize,
    gain: f32,
    /// One packet's frames, interleaved.
    buf: Vec<f32>,
}

impl Stream {
    /// A decoder for a stream of mapping family 0 (mono or stereo, which
    /// every simfile's music is); `None` for others.
    pub fn new(head: Head) -> Option<Stream> {
        if head.family != 0 || !(1..=2).contains(&head.channels) {
            return None;
        }
        Some(Stream {
            decoder: rusty_opus::OpusDecoder::new(RATE as i32, head.channels).ok()?,
            channels: head.channels,
            pre_skip: head.pre_skip,
            gain: head.gain,
            buf: vec![0.0; MAX_FRAMES * head.channels],
        })
    }

    pub fn channels(&self) -> usize {
        self.channels
    }

    /// The frames of silence `frames` lost before a packet come to (a page
    /// the reader dropped), what of them the pre-skip covers left out.
    pub fn gap(&mut self, frames: usize) -> usize {
        let drop = self.pre_skip.min(frames);
        self.pre_skip -= drop;
        frames - drop
    }

    /// One packet's frames, interleaved, after the pre-skip; and whether
    /// the packet failed (then concealed, `dur` frames long, by libopus's
    /// PLC, as a browser would a lost packet).
    pub fn decode(&mut self, data: &[u8], dur: u64) -> (&[f32], bool) {
        let (n, failed) = match self.decoder.decode(data, MAX_FRAMES, &mut self.buf) {
            Ok(n) => (n, false),
            Err(_) => {
                let n = self
                    .decoder
                    .decode(&[], dur.min(MAX_FRAMES as u64) as usize, &mut self.buf)
                    .unwrap_or(0);
                (n, true)
            }
        };
        // The pre-skip is not the song's.
        let drop = self.pre_skip.min(n);
        self.pre_skip -= drop;
        let frames = &mut self.buf[drop * self.channels..n * self.channels];
        if self.gain != 1.0 {
            for s in frames.iter_mut() {
                *s *= self.gain;
            }
        }
        (frames, failed)
    }
}

/// Frames a packet takes: 20 ms.
const ENCODE_FRAMES: usize = 960;
/// What a decode plays before the sound, in frames: the encoder's delay,
/// CELT's overlap, which is all it has in the mode it is asked for
/// (`encoded_sound_starts_where_it_did`). The game times lines from a
/// sound's first sample.
const ENCODE_PRE_SKIP: u16 = 120;

/// `channels` (one slice each, all as long as the first; 1 or 2 of them)
/// at `rate` Hz, which must be [`RATE`], as an Ogg Opus file: the same
/// sound gives the same bytes. CELT alone: at these bit rates it is what
/// the encoder would choose for a voice anyway, and its delay is then
/// the same whatever is said.
pub fn encode<C: AsRef<[f32]>>(channels: &[C], rate: u32) -> Result<Vec<u8>, String> {
    if rate != RATE {
        return Err(format!("Opus is not encoded at {rate} Hz"));
    }
    let count = channels.len();
    if !(1..=2).contains(&count) {
        return Err(format!("Opus is not encoded in {count} channels"));
    }
    let failed = |e: rusty_opus::Error| format!("the Opus encoder failed: {e:?}");
    let mut encoder = rusty_opus::OpusEncoder::new(
        RATE as i32,
        count,
        rusty_opus::Application::RestrictedLowDelay,
    )
    .map_err(failed)?;
    encoder.bitrate_bps = crate::ogg_opus::bitrate(count) as i32;
    encoder.complexity = 10;
    let frames = channels[0].as_ref().len();
    let wanted = (frames + usize::from(ENCODE_PRE_SKIP))
        .div_ceil(ENCODE_FRAMES)
        .max(1);
    let mut pcm = vec![0.0f32; ENCODE_FRAMES * count];
    let mut packet = vec![0u8; 4_000];
    let mut packets = Vec::with_capacity(wanted);
    for n in 0..wanted {
        for (i, frame) in pcm.chunks_exact_mut(count).enumerate() {
            for (s, c) in frame.iter_mut().zip(channels) {
                let v = c
                    .as_ref()
                    .get(n * ENCODE_FRAMES + i)
                    .copied()
                    .unwrap_or(0.0);
                *s = if v.is_finite() {
                    v.clamp(-1.0, 1.0)
                } else {
                    0.0
                };
            }
        }
        let len = encoder
            .encode(&pcm, ENCODE_FRAMES, &mut packet)
            .map_err(failed)?;
        packets.push(packet[..len].to_vec());
    }
    crate::ogg_opus::write(
        crate::ogg_opus::Stream {
            channels: count as u8,
            pre_skip: ENCODE_PRE_SKIP,
            frames: frames as u64,
        },
        &packets,
    )
}

/// An Ogg Opus file decoded whole, as a WAV of 32-bit floats at 48 kHz
/// (and the packets that failed): what the web's Opus worker hands the
/// page, whose `decodeAudioData` reads it at the context's rate with the
/// browser's own resampler, as it would have the Opus file.
pub fn decode_wav(bytes: &[u8]) -> Result<(Vec<u8>, usize), String> {
    let mut d = crate::decode::Decoder::ogg_opus(bytes).map_err(|e| e.to_string())?;
    while !d.step(64).map_err(|e| e.to_string())? {}
    let audio = d.finish().map_err(|e| e.to_string())?;
    Ok((wav(&audio)?, audio.skipped_packets))
}

/// `audio` as a WAV file of 32-bit floats (format 3), interleaved.
pub fn wav(audio: &crate::decode::DecodedAudio) -> Result<Vec<u8>, String> {
    let channels = audio.channels.len();
    let frames = audio.frames();
    let data = u32::try_from(frames * channels * 4)
        .ok()
        .filter(|&n| n <= u32::MAX - 36)
        .ok_or("the audio is too long for a WAV file")?;
    let block = (channels * 4) as u16;
    let mut out = Vec::with_capacity(44 + data as usize);
    out.extend_from_slice(b"RIFF");
    out.extend_from_slice(&(36 + data).to_le_bytes());
    out.extend_from_slice(b"WAVEfmt ");
    out.extend_from_slice(&16u32.to_le_bytes());
    out.extend_from_slice(&3u16.to_le_bytes());
    out.extend_from_slice(&(channels as u16).to_le_bytes());
    out.extend_from_slice(&audio.sample_rate.to_le_bytes());
    out.extend_from_slice(&(audio.sample_rate * u32::from(block)).to_le_bytes());
    out.extend_from_slice(&block.to_le_bytes());
    out.extend_from_slice(&32u16.to_le_bytes());
    out.extend_from_slice(b"data");
    out.extend_from_slice(&data.to_le_bytes());
    for i in 0..frames {
        for c in &audio.channels {
            out.extend_from_slice(&c[i].to_le_bytes());
        }
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A second of something like a voice: a pitched buzz with its
    /// harmonics, its loudness rising and falling.
    fn voice(frames: usize) -> Vec<f32> {
        (0..frames)
            .map(|i| {
                let t = i as f32 / RATE as f32;
                let phase = std::f32::consts::TAU * 140.0 * t;
                let sound: f32 = (1..12).map(|h| (phase * h as f32).sin() / h as f32).sum();
                sound * 0.15 * (t * 9.0).sin().abs()
            })
            .collect()
    }

    fn decoded(file: &[u8]) -> crate::decode::DecodedAudio {
        let mut d = crate::decode::Decoder::ogg_opus(file).unwrap();
        while !d.step(64).unwrap() {}
        d.finish().unwrap()
    }

    /// How alike `a` and `b` are with `b` moved `lag` frames later.
    fn likeness(a: &[f32], b: &[f32], lag: isize) -> f64 {
        let (mut ab, mut aa, mut bb) = (0.0f64, 0.0f64, 0.0f64);
        for (i, &x) in a.iter().enumerate() {
            let Some(&y) = b.get((i as isize + lag) as usize) else {
                continue;
            };
            ab += f64::from(x) * f64::from(y);
            aa += f64::from(x) * f64::from(x);
            bb += f64::from(y) * f64::from(y);
        }
        ab / (aa * bb).sqrt().max(1e-12)
    }

    #[test]
    fn encoded_sound_starts_where_it_did() {
        let sound = voice(48_000);
        let back = decoded(&encode(&[&sound[..]], RATE).unwrap());
        assert_eq!(back.sample_rate, RATE);
        assert_eq!(back.channels.len(), 1);
        assert_eq!(back.frames(), sound.len());
        assert_eq!(back.skipped_packets, 0);
        let here = likeness(&sound, &back.channels[0], 0);
        assert!(here > 0.99, "{here}");
        for lag in (-400..=400).filter(|&l| l != 0) {
            let there = likeness(&sound, &back.channels[0], lag);
            assert!(
                there < here,
                "as alike {lag} frames away: {there} against {here}"
            );
        }
    }

    #[test]
    fn its_channels_stay_apart() {
        let left = voice(24_000);
        let right: Vec<f32> = left.iter().rev().copied().collect();
        let back = decoded(&encode(&[&left[..], &right[..]], RATE).unwrap());
        assert_eq!(back.channels.len(), 2);
        assert_eq!(back.frames(), left.len());
        assert!(likeness(&left, &back.channels[0], 0) > 0.98);
        assert!(likeness(&right, &back.channels[1], 0) > 0.98);
        assert!(likeness(&left, &back.channels[1], 0) < 0.9);
    }

    #[test]
    fn every_length_comes_back_as_long() {
        // Around the frames' and the delay's edges.
        for frames in [1, 119, 120, 121, 839, 840, 841, 959, 960, 961, 1_920, 5_000] {
            let sound = voice(frames);
            let back = decoded(&encode(&[&sound[..]], RATE).unwrap());
            assert_eq!(back.frames(), frames);
        }
    }

    #[test]
    fn nothing_is_a_file_that_says_so() {
        let file = encode(&[&[][..]], RATE).unwrap();
        assert!(crate::decode::is_ogg_opus(&file));
        let mut d = crate::decode::Decoder::ogg_opus(&file).unwrap();
        while !d.step(64).unwrap() {}
        assert!(matches!(
            d.finish(),
            Err(crate::decode::DecodeError::NoAudio { .. })
        ));
    }

    #[test]
    fn it_is_a_tenth_of_a_wav_at_most() {
        let sound = voice(48_000 * 4);
        let wav = |channels: usize| 44 + sound.len() * channels * 2;
        let mono = encode(&[&sound[..]], RATE).unwrap();
        assert!(mono.len() * 10 < wav(1), "{}", mono.len());
        let stereo = encode(&[&sound[..], &sound[..]], RATE).unwrap();
        assert!(stereo.len() * 10 < wav(2), "{}", stereo.len());
        // What a pack's size is told from is about right.
        let told = crate::ogg_opus::len(4.0, 1) as f64;
        assert!((told / mono.len() as f64 - 1.0).abs() < 0.2, "{told}");
    }

    #[test]
    fn the_same_sound_gives_the_same_bytes() {
        let sound = voice(30_000);
        assert_eq!(
            encode(&[&sound[..]], RATE).unwrap(),
            encode(&[&sound[..]], RATE).unwrap()
        );
    }

    #[test]
    fn samples_no_sound_has_are_borne() {
        let mut sound = voice(4_800);
        sound[100] = f32::NAN;
        sound[200] = f32::INFINITY;
        sound[300] = f32::NEG_INFINITY;
        sound[400] = 40.0;
        sound[500] = -40.0;
        let back = decoded(&encode(&[&sound[..]], RATE).unwrap());
        assert_eq!(back.frames(), sound.len());
        assert!(back.channels[0].iter().all(|s| s.is_finite()));
        // Noise, which leaves the encoder nothing to predict.
        let mut seed = 7u32;
        let noise: Vec<f32> = (0..24_000)
            .map(|_| {
                seed = seed.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
                (seed >> 8) as f32 / (1u32 << 23) as f32 - 1.0
            })
            .collect();
        assert_eq!(
            decoded(&encode(&[&noise[..]], RATE).unwrap()).frames(),
            noise.len()
        );
    }

    #[test]
    fn what_opus_is_not_is_refused() {
        let sound = voice(960);
        assert!(encode(&[&sound[..]], 44_100).is_err());
        assert!(encode(&[&sound[..]; 3], RATE).is_err());
        assert!(encode::<&[f32]>(&[], RATE).is_err());
    }
}
