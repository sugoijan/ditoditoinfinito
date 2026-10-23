//! The music's overview (`docs/plans/editor.md`, step 5): its loudness
//! over the whole song, a few thousand bins, made once from the music
//! the shell decoded for listening (desktop on a thread, the web as it
//! comes) and drawn as a strip along the field.

/// The most bins an overview has.
pub const MAX_BINS: usize = 4096;
/// The shortest bin, seconds.
pub const MIN_BIN: f64 = 0.01;
/// Decibels under the loudest bin drawn at all: a quiet intro or a
/// break stands out from the loud parts.
const RANGE_DB: f32 = 30.0;

/// The music's loudness, bin by bin.
#[derive(Clone, Debug, PartialEq)]
pub struct Overview {
    /// The song second the first bin starts at.
    pub start: f64,
    /// Seconds a bin.
    pub bin: f64,
    /// Each bin's loudness from 0 to 1: its RMS in decibels under the
    /// loudest bin's, over [`RANGE_DB`].
    pub levels: Vec<f32>,
}

impl Overview {
    /// The song second the music ends at.
    pub fn end(&self) -> f64 {
        self.start + self.bin * self.levels.len() as f64
    }

    /// The loudest level over song seconds `a..b` (0 outside the music).
    pub fn level(&self, a: f64, b: f64) -> f32 {
        if self.levels.is_empty() || self.bin <= 0.0 {
            return 0.0;
        }
        let n = self.levels.len() as f64;
        let i = ((a - self.start) / self.bin).floor().clamp(0.0, n) as usize;
        let j = ((b - self.start) / self.bin).ceil().clamp(0.0, n) as usize;
        self.levels[i..j.max(i)]
            .iter()
            .fold(0.0f32, |m, v| m.max(*v))
    }
}

/// An overview being made from a sound's samples, any channel in any
/// order.
pub struct Builder {
    start: f64,
    rate: f64,
    /// Frames a bin (not whole).
    per_bin: f64,
    sums: Vec<f64>,
    counts: Vec<u32>,
}

impl Builder {
    /// For `frames` frames at `rate` Hz, the first heard at song second
    /// `start` (an MP3's lead on the web).
    pub fn new(rate: f64, frames: usize, start: f64) -> Builder {
        let seconds = frames as f64 / rate.max(1.0);
        let bins = ((seconds / MIN_BIN).ceil() as usize).clamp(1, MAX_BINS);
        Builder {
            start,
            rate: rate.max(1.0),
            per_bin: (frames.max(1) as f64 / bins as f64).max(1.0),
            sums: vec![0.0; bins],
            counts: vec![0; bins],
        }
    }

    /// One channel's samples from frame `at` on.
    pub fn add(&mut self, at: usize, samples: &[f32]) {
        let bins = self.sums.len();
        let mut f = at;
        let end = at + samples.len();
        while f < end {
            let bin = ((f as f64 / self.per_bin) as usize).min(bins - 1);
            let next = if bin + 1 == bins {
                end
            } else {
                (((bin + 1) as f64 * self.per_bin).ceil() as usize).clamp(f + 1, end)
            };
            let sum: f64 = samples[f - at..next - at]
                .iter()
                .map(|v| f64::from(*v) * f64::from(*v))
                .sum();
            self.sums[bin] += sum;
            self.counts[bin] += (next - f) as u32;
            f = next;
        }
    }

    /// Interleaved samples of `channels` channels from frame `at` on.
    pub fn add_interleaved(&mut self, at: usize, data: &[f32], channels: usize) {
        let channels = channels.max(1);
        let mut frame = Vec::with_capacity(4096);
        for (i, chunk) in data.chunks(4096 * channels).enumerate() {
            for c in 0..channels {
                frame.clear();
                frame.extend(chunk.iter().skip(c).step_by(channels));
                self.add(at + i * 4096, &frame);
            }
        }
    }

    pub fn finish(self) -> Overview {
        let rms: Vec<f64> = self
            .sums
            .iter()
            .zip(&self.counts)
            .map(|(s, n)| {
                if *n > 0 {
                    (s / f64::from(*n)).sqrt()
                } else {
                    0.0
                }
            })
            .collect();
        let max = rms.iter().fold(0.0f64, |m, v| m.max(*v));
        let levels = rms
            .iter()
            .map(|r| {
                if max <= 0.0 || *r <= 0.0 {
                    return 0.0;
                }
                let db = 20.0 * (r / max).log10() as f32;
                (1.0 + db / RANGE_DB).clamp(0.0, 1.0)
            })
            .collect();
        Overview {
            start: self.start,
            bin: self.per_bin / self.rate,
            levels,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn loud_and_quiet_parts_and_their_seconds() {
        // 10 s at 1 kHz: silent, then loud from 4 s, a tenth as loud
        // (−20 dB) from 7 s.
        let rate = 1000.0;
        let samples: Vec<f32> = (0..10_000)
            .map(|i| match i {
                0..4000 => 0.0,
                4000..7000 => {
                    if i % 2 == 0 {
                        0.5
                    } else {
                        -0.5
                    }
                }
                _ => {
                    if i % 2 == 0 {
                        0.05
                    } else {
                        -0.05
                    }
                }
            })
            .collect();
        let mut b = Builder::new(rate, samples.len(), 0.25);
        // Stereo, the same on both.
        let stereo: Vec<f32> = samples.iter().flat_map(|v| [*v, *v]).collect();
        b.add_interleaved(0, &stereo, 2);
        let o = b.finish();
        assert_eq!(o.levels.len(), 1000);
        assert!((o.bin - 0.01).abs() < 1e-12);
        assert!((o.end() - 10.25).abs() < 1e-9);
        assert_eq!(o.level(0.25, 4.2), 0.0);
        assert_eq!(o.level(4.5, 5.0), 1.0);
        let quiet = o.level(7.5, 8.0);
        assert!((quiet - (1.0 - 20.0 / RANGE_DB)).abs() < 1e-4, "{quiet}");
        assert_eq!(o.level(20.0, 30.0), 0.0, "after the music");
    }

    #[test]
    fn long_music_keeps_to_its_bins_and_short_music_has_one() {
        let b = Builder::new(48_000.0, 48_000 * 600, 0.0);
        let o = b.finish();
        assert_eq!(o.levels.len(), MAX_BINS);
        assert!((o.end() - 600.0).abs() < 1e-6);
        let mut b = Builder::new(48_000.0, 10, 0.0);
        b.add(0, &[0.5; 10]);
        let o = b.finish();
        assert_eq!(o.levels, vec![1.0]);
    }
}
