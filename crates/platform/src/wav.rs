//! WAV files: written as 16-bit PCM (what every browser, our decoders and
//! StepMania read: the announcer studio's tidied takes) or as 32-bit
//! floats (the web's Opus worker's decodes, the tidy tools' sounds), and
//! read from either.

/// The 44-byte header of a WAV of `channels` channels at `rate` Hz,
/// `bits` a sample in `format` (1 PCM, 3 IEEE float), `data_len` bytes
/// of samples after it.
fn header(out: &mut Vec<u8>, format: u16, bits: u16, channels: u16, rate: u32, data_len: u32) {
    let block = channels * (bits / 8);
    out.extend_from_slice(b"RIFF");
    out.extend_from_slice(&(36 + data_len).to_le_bytes());
    out.extend_from_slice(b"WAVEfmt ");
    out.extend_from_slice(&16u32.to_le_bytes());
    out.extend_from_slice(&format.to_le_bytes());
    out.extend_from_slice(&channels.to_le_bytes());
    out.extend_from_slice(&rate.to_le_bytes());
    out.extend_from_slice(&(rate * u32::from(block)).to_le_bytes());
    out.extend_from_slice(&block.to_le_bytes());
    out.extend_from_slice(&bits.to_le_bytes());
    out.extend_from_slice(b"data");
    out.extend_from_slice(&data_len.to_le_bytes());
}

/// `channels` (one slice a channel, all as long as the first; a shorter
/// one silent past its end) at `rate` Hz, each sample written by `put`
/// in `bits` bits of `format`.
fn write<C: AsRef<[f32]>>(
    channels: &[C],
    rate: u32,
    format: u16,
    bits: u16,
    put: impl Fn(&mut Vec<u8>, f32),
) -> Vec<u8> {
    let count = channels.len().max(1) as u16;
    let frames = channels.first().map_or(0, |c| c.as_ref().len());
    let data_len = (frames * usize::from(count) * usize::from(bits / 8)) as u32;
    let mut out = Vec::with_capacity(44 + data_len as usize);
    header(&mut out, format, bits, count, rate, data_len);
    for i in 0..frames {
        for c in channels {
            put(&mut out, c.as_ref().get(i).copied().unwrap_or(0.0));
        }
    }
    out
}

/// A 16-bit PCM WAV of `channels` (one slice a channel, all as long as
/// the first; samples in `-1..=1`, clamped) at `rate` Hz.
pub fn write_pcm16<C: AsRef<[f32]>>(channels: &[C], rate: u32) -> Vec<u8> {
    write(channels, rate, 1, 16, |out, s| {
        let s = (s.clamp(-1.0, 1.0) * 32767.0).round() as i16;
        out.extend_from_slice(&s.to_le_bytes());
    })
}

/// A 32-bit float WAV of `channels` (as [`write_pcm16`] takes them, the
/// samples as they are) at `rate` Hz.
pub fn write_float<C: AsRef<[f32]>>(channels: &[C], rate: u32) -> Vec<u8> {
    write(channels, rate, 3, 32, |out, s| out.extend_from_slice(&s.to_le_bytes()))
}

/// The bytes a [`write_pcm16`] of `frames` frames of `channels` channels
/// takes.
pub fn pcm16_len(frames: u64, channels: u16) -> u64 {
    44 + frames * u64::from(channels.max(1)) * 2
}

/// A WAV's samples, one `Vec` a channel, and its rate: 16-bit PCM (as the
/// game's decoders read 16 bits) or 32-bit float, its chunks walked.
pub fn read(bytes: &[u8]) -> Result<(Vec<Vec<f32>>, u32), String> {
    if bytes.len() < 12 || &bytes[..4] != b"RIFF" || &bytes[8..12] != b"WAVE" {
        return Err("not a WAV file".into());
    }
    let u16_at = |i: usize| u16::from_le_bytes([bytes[i], bytes[i + 1]]);
    let u32_at =
        |i: usize| u32::from_le_bytes([bytes[i], bytes[i + 1], bytes[i + 2], bytes[i + 3]]);
    let mut format = None;
    let mut at = 12;
    while at + 8 <= bytes.len() {
        let id = &bytes[at..at + 4];
        let len = u32_at(at + 4) as usize;
        let body = at + 8;
        let end = (body + len).min(bytes.len());
        if id == b"fmt " && len >= 16 {
            format = Some((
                u16_at(body),
                u16_at(body + 2),
                u32_at(body + 4),
                u16_at(body + 14),
            ));
        } else if id == b"data" {
            let Some((tag, channels, rate, bits)) = format else {
                return Err("its data comes before its format".into());
            };
            let channels = usize::from(channels.max(1));
            let data = &bytes[body..end];
            let samples: Vec<f32> = match (tag, bits) {
                (1, 16) => data
                    .as_chunks::<2>()
                    .0
                    .iter()
                    .map(|b| f32::from(i16::from_le_bytes(*b)) / 32768.0)
                    .collect(),
                (3, 32) => data
                    .as_chunks::<4>()
                    .0
                    .iter()
                    .map(|b| f32::from_le_bytes(*b))
                    .collect(),
                _ => {
                    return Err(format!(
                        "format {tag} at {bits} bits: 16-bit PCM or 32-bit float is read"
                    ));
                }
            };
            let mut planes = vec![Vec::with_capacity(samples.len() / channels); channels];
            for frame in samples.chunks_exact(channels) {
                for (plane, s) in planes.iter_mut().zip(frame) {
                    plane.push(*s);
                }
            }
            return Ok((planes, rate));
        }
        at = body + len + (len & 1);
    }
    Err("it has no data".into())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_stereo_wav_has_its_header_and_interleaved_samples() {
        let left = [0.0, 1.0, -1.0];
        let right = [0.5, 2.0, -0.25];
        let wav = write_pcm16(&[&left[..], &right[..]], 48_000);
        assert_eq!(wav.len() as u64, pcm16_len(3, 2));
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

    #[test]
    fn a_float_wav_has_its_header_and_reads_back() {
        let planes = vec![vec![0.25f32, -2.0, 0.5], vec![1.0, 0.0, -0.125]];
        let wav = write_float(&planes, 24_000);
        let u16_at = |i: usize| u16::from_le_bytes([wav[i], wav[i + 1]]);
        let u32_at = |i: usize| u32::from_le_bytes(wav[i..i + 4].try_into().unwrap());
        assert_eq!(wav.len(), 44 + 3 * 2 * 4);
        assert_eq!((u32_at(4), u16_at(20), u16_at(22)), (36 + 24, 3, 2));
        assert_eq!((u32_at(28), u16_at(32), u16_at(34)), (192_000, 8, 32));
        assert_eq!(read(&wav).unwrap(), (planes.clone(), 24_000));
        let (back, rate) = read(&write_pcm16(&planes, 48_000)).unwrap();
        assert_eq!((back.len(), rate), (2, 48_000));
        assert!((back[0][0] - 0.25).abs() < 1.0 / 32_000.0);
        assert!(read(b"RIFFxxxxWAVE").is_err());
        assert!(read(b"nothing").is_err());
    }
}
