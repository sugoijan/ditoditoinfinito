//! WAV files written: 16-bit PCM, what every browser, our decoders and
//! StepMania read. The announcer studio writes its tidied takes so
//! (`docs/plans/announcer-studio.md`, decision 6).

/// A 16-bit PCM WAV of `channels` (one slice a channel, all as long as
/// the first; samples in `-1..=1`, clamped) at `rate` Hz.
pub fn write<C: AsRef<[f32]>>(channels: &[C], rate: u32) -> Vec<u8> {
    let count = channels.len().max(1) as u16;
    let frames = channels.first().map_or(0, |c| c.as_ref().len());
    let data_len = (frames * usize::from(count) * 2) as u32;
    let mut out = Vec::with_capacity(44 + data_len as usize);
    out.extend_from_slice(b"RIFF");
    out.extend_from_slice(&(36 + data_len).to_le_bytes());
    out.extend_from_slice(b"WAVEfmt ");
    out.extend_from_slice(&16u32.to_le_bytes());
    out.extend_from_slice(&1u16.to_le_bytes()); // PCM
    out.extend_from_slice(&count.to_le_bytes());
    out.extend_from_slice(&rate.to_le_bytes());
    out.extend_from_slice(&(rate * u32::from(count) * 2).to_le_bytes());
    out.extend_from_slice(&(count * 2).to_le_bytes());
    out.extend_from_slice(&16u16.to_le_bytes());
    out.extend_from_slice(b"data");
    out.extend_from_slice(&data_len.to_le_bytes());
    for i in 0..frames {
        for c in channels {
            let s = c.as_ref().get(i).copied().unwrap_or(0.0);
            let s = (s.clamp(-1.0, 1.0) * 32767.0).round() as i16;
            out.extend_from_slice(&s.to_le_bytes());
        }
    }
    out
}

/// The bytes a [`write`] of `frames` frames of `channels` channels takes.
pub fn len(frames: u64, channels: u16) -> u64 {
    44 + frames * u64::from(channels.max(1)) * 2
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_stereo_wav_has_its_header_and_interleaved_samples() {
        let left = [0.0, 1.0, -1.0];
        let right = [0.5, 2.0, -0.25];
        let wav = write(&[&left[..], &right[..]], 48_000);
        assert_eq!(wav.len() as u64, len(3, 2));
        assert_eq!(&wav[..4], b"RIFF");
        assert_eq!(&wav[8..16], b"WAVEfmt ");
        let u16_at = |i: usize| u16::from_le_bytes([wav[i], wav[i + 1]]);
        let u32_at = |i: usize| u32::from_le_bytes(wav[i..i + 4].try_into().unwrap());
        assert_eq!((u16_at(20), u16_at(22), u32_at(24)), (1, 2, 48_000));
        assert_eq!((u32_at(28), u16_at(32), u16_at(34)), (192_000, 4, 16));
        assert_eq!((&wav[36..40], u32_at(40)), (&b"data"[..], 12));
        let samples: Vec<i16> = wav[44..]
            .chunks(2)
            .map(|b| i16::from_le_bytes([b[0], b[1]]))
            .collect();
        assert_eq!(samples, [0, 16384, 32767, 32767, -32767, -8192]);
    }
}
