//! The music's analysis a generative background follows
//! (`docs/plans/phase-8-gaps.md`, step 5): made once when a play loads,
//! from the samples it plays, a frame every [`HOP`] seconds. As the
//! design study's (`tools/background-design/analysis.js`), less what its
//! choices left unused:
//!
//! - **bands:** [`BANDS`] log-spaced bands from [`LOW_HZ`] to
//!   [`HIGH_HZ`] of a [`window`]'s transform, each 0 to 1 over the
//!   [`RANGE_DB`] under the song's loudest band;
//! - **loudness:** the level of the hop's samples, −60 to 0 dBFS as 0
//!   to 1.
//!
//! [`Analyzer`] is fed the samples in order and stepped in slices
//! ([`Analyzer::step`]): the web steps it a few milliseconds a frame,
//! desktop runs it on a thread. It keeps only the samples the frames to
//! come still need, and makes the same analysis however it is stepped.

use crate::fft::Fft;

/// Seconds between frames.
pub const HOP: f64 = 0.02;
/// Bands, log-spaced from [`LOW_HZ`] to [`HIGH_HZ`].
pub const BANDS: usize = 32;
pub const LOW_HZ: f64 = 30.0;
pub const HIGH_HZ: f64 = 16_000.0;
/// Decibels under the song's loudest band a band's 0 is.
pub const RANGE_DB: f32 = 60.0;

/// A band's level under which it is silence.
const SILENCE_DB: f32 = -110.0;

/// The transform's length at `rate` Hz: the power of two nearest 46 ms
/// (2048 at 44.1 and 48 kHz, the study's).
pub fn window(rate: f64) -> usize {
    let n = (rate.max(1000.0) * 0.0464).log2().round() as u32;
    1usize << n.clamp(9, 14)
}

/// Samples between frames at `rate` Hz ([`HOP`]).
pub fn hop(rate: f64) -> usize {
    ((rate.max(1000.0) * HOP).round() as usize).max(1)
}

/// A song's music, analyzed.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Music {
    /// The song second of the first frame's middle.
    pub start: f64,
    /// Seconds between frames.
    pub per: f64,
    /// Each frame's bands, [`BANDS`] a frame, low to high.
    pub bands: Vec<f32>,
    pub loudness: Vec<f32>,
}

impl Music {
    pub fn frames(&self) -> usize {
        self.loudness.len()
    }

    /// Frame `k`'s bands.
    pub fn frame(&self, k: usize) -> &[f32] {
        &self.bands[k * BANDS..(k + 1) * BANDS]
    }
}

/// An analysis being made: fed the samples in order, stepped in slices.
pub struct Analyzer {
    rate: f64,
    start: f64,
    /// The music's frames (samples a channel), and the analysis'.
    total: usize,
    count: usize,
    hop: usize,
    window: usize,
    hann: Vec<f32>,
    fft: Fft,
    /// Each band's bins, `[from, to)`.
    edges: Vec<(usize, usize)>,
    /// The mid channel from sample `base` on, as far as fed.
    mid: Vec<f32>,
    base: usize,
    fed: usize,
    made: usize,
    re: Vec<f32>,
    im: Vec<f32>,
    turns: Vec<(f32, f32)>,
    db: Vec<f32>,
    loudness: Vec<f32>,
}

impl Analyzer {
    /// For `frames` frames (samples a channel) at `rate` Hz, the first
    /// heard at song second `start` (an MP3's lead on the web).
    pub fn new(rate: f64, frames: usize, start: f64) -> Analyzer {
        let rate = rate.max(1.0);
        let window = window(rate);
        let hop = hop(rate);
        let count = if frames == 0 {
            0
        } else {
            (frames / hop).max(1)
        };
        // The study's symmetric Hann window.
        let hann = (0..window)
            .map(|i| {
                (0.5 - 0.5 * (2.0 * std::f64::consts::PI * i as f64 / (window - 1) as f64).cos())
                    as f32
            })
            .collect();
        let edge = |b: usize| {
            let hz = LOW_HZ * (HIGH_HZ / LOW_HZ).powf(b as f64 / BANDS as f64);
            ((hz * window as f64 / rate).round() as usize).clamp(1, window / 2 - 1)
        };
        let edges = (0..BANDS)
            .map(|b| {
                let lo = edge(b);
                (lo, edge(b + 1).max(lo + 1))
            })
            .collect();
        Analyzer {
            rate,
            start,
            total: frames,
            count,
            hop,
            window,
            hann,
            fft: Fft::new(window / 2),
            edges,
            mid: Vec::new(),
            base: 0,
            fed: 0,
            made: 0,
            re: vec![0.0; window / 2],
            im: vec![0.0; window / 2],
            turns: (0..=window / 2)
                .map(|k| {
                    let a = -2.0 * std::f64::consts::PI * k as f64 / window as f64;
                    (a.cos() as f32, a.sin() as f32)
                })
                .collect(),
            db: Vec::with_capacity(count * BANDS),
            loudness: Vec::with_capacity(count),
        }
    }

    /// Frames (samples a channel) taken in so far.
    pub fn fed(&self) -> usize {
        self.fed
    }

    /// Analysis frames made, and the number there are.
    pub fn progress(&self) -> (usize, usize) {
        (self.made, self.count)
    }

    /// The next frames of each channel (planar, the same length; more
    /// than the music's frames are left out).
    pub fn add_planar(&mut self, channels: &[&[f32]]) {
        let Some(n) = channels.iter().map(|c| c.len()).min() else {
            return;
        };
        let n = n.min(self.total - self.fed);
        let k = channels.len() as f32;
        self.mid
            .extend((0..n).map(|i| channels.iter().map(|c| c[i]).sum::<f32>() / k));
        self.fed += n;
    }

    /// The next frames, interleaved `channels` to a frame.
    pub fn add_interleaved(&mut self, data: &[f32], channels: usize) {
        let channels = channels.max(1);
        let k = channels as f32;
        let n = (data.len() / channels).min(self.total - self.fed);
        self.mid.extend(
            data.chunks_exact(channels)
                .take(n)
                .map(|f| f.iter().sum::<f32>() / k),
        );
        self.fed += n;
    }

    /// Every frame is made.
    pub fn done(&self) -> bool {
        self.made == self.count
    }

    /// Makes up to `units` frames the samples so far allow; whether all
    /// are made.
    pub fn step(&mut self, units: usize) -> bool {
        let half = self.window / 2;
        let mut left = units;
        while left > 0 && self.made < self.count {
            let center = self.made * self.hop + self.hop / 2;
            if center + half > self.fed && self.fed < self.total {
                break;
            }
            self.frame(center);
            self.made += 1;
            left -= 1;
        }
        // What the frames to come no longer need.
        let next = (self.made * self.hop + self.hop / 2).saturating_sub(half);
        if next > self.base {
            let drop = (next - self.base).min(self.mid.len());
            self.mid.drain(..drop);
            self.base += drop;
        }
        self.done()
    }

    /// The frame around sample `center`.
    fn frame(&mut self, center: usize) {
        let n = self.window;
        let half = n / 2;
        let (mid, base) = (&self.mid, self.base);
        let at = |i: usize| -> f32 {
            (center + i)
                .checked_sub(half)
                .and_then(|s| s.checked_sub(base))
                .and_then(|s| mid.get(s))
                .copied()
                .unwrap_or(0.0)
        };
        // The level of the hop's samples around the middle.
        let from = half - self.hop / 2;
        let squares: f64 = (from..from + self.hop)
            .map(|i| f64::from(at(i)).powi(2))
            .sum();
        let db = 10.0 * (squares / self.hop as f64 + 1e-12).log10();
        self.loudness
            .push(((db + 60.0) / 60.0).clamp(0.0, 1.0) as f32);
        // A real transform of `n` as a complex one of `n / 2`, as the
        // editor's analysis makes it.
        for m in 0..half {
            self.re[m] = at(2 * m) * self.hann[2 * m];
            self.im[m] = at(2 * m + 1) * self.hann[2 * m + 1];
        }
        self.fft.run(&mut self.re, &mut self.im);
        let power = |k: usize| {
            let (zr, zi) = (self.re[k % half], self.im[k % half]);
            let (cr, ci) = (self.re[(half - k) % half], -self.im[(half - k) % half]);
            let (er, ei) = ((zr + cr) / 2.0, (zi + ci) / 2.0);
            let (dr, di) = ((zr - cr) / 2.0, (zi - ci) / 2.0);
            let (or, oi) = (di, -dr);
            let (wr, wi) = self.turns[k];
            let (xr, xi) = (er + or * wr - oi * wi, ei + or * wi + oi * wr);
            xr * xr + xi * xi
        };
        for &(from, to) in &self.edges {
            let p: f32 = (from..to).map(power).sum();
            self.db
                .push(10.0 * (p / (to - from) as f32 + 1e-12).log10());
        }
    }

    /// The analysis (stepped to the end first; samples never fed are
    /// silence).
    pub fn finish(mut self) -> Music {
        self.fed = self.total;
        while !self.step(usize::MAX) {}
        let top = self.db.iter().copied().fold(f32::MIN, f32::max);
        let bands = self
            .db
            .iter()
            .map(|&v| {
                // Silence (the floor's 1e-12) is nothing, whatever the top.
                if v < SILENCE_DB {
                    0.0
                } else {
                    ((v - (top - RANGE_DB)) / RANGE_DB).clamp(0.0, 1.0)
                }
            })
            .collect();
        let per = self.hop as f64 / self.rate;
        Music {
            start: self.start + per / 2.0,
            per,
            bands,
            loudness: self.loudness,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sine(rate: f64, hz: f64, seconds: f64, level: f32) -> Vec<f32> {
        (0..(rate * seconds) as usize)
            .map(|i| (2.0 * std::f64::consts::PI * hz * i as f64 / rate).sin() as f32 * level)
            .collect()
    }

    #[test]
    fn the_window_and_the_hop() {
        assert_eq!(window(44_100.0), 2048);
        assert_eq!(window(48_000.0), 2048);
        assert_eq!(window(96_000.0), 4096);
        assert_eq!(hop(48_000.0), 960);
        assert_eq!(hop(44_100.0), 882);
    }

    #[test]
    fn a_sine_is_loudest_in_its_band() {
        let rate = 48_000.0;
        let s = sine(rate, 1000.0, 1.0, 0.5);
        let mut a = Analyzer::new(rate, s.len(), 0.0);
        a.add_interleaved(&s, 1);
        let m = a.finish();
        assert_eq!(m.frames(), 50);
        let f = m.frame(25);
        let (band, v) = f
            .iter()
            .enumerate()
            .max_by(|a, b| a.1.total_cmp(b.1))
            .unwrap();
        let edge = |b: usize| LOW_HZ * (HIGH_HZ / LOW_HZ).powf(b as f64 / BANDS as f64);
        assert!(edge(band) <= 1000.0 && edge(band + 1) >= 1000.0, "{band}");
        assert_eq!(*v, 1.0);
        // −6 dBFS peak, −9 dBFS RMS: 0.85 of the way from −60 dB.
        assert!((m.loudness[25] - 0.85).abs() < 0.01, "{}", m.loudness[25]);
        // Frame 0's middle is half a hop in.
        assert!((m.start - 0.01).abs() < 1e-9 && (m.per - 0.02).abs() < 1e-9);
    }

    #[test]
    fn stepping_in_slices_makes_the_same_analysis_and_keeps_few_samples() {
        let rate = 44_100.0;
        let mut s = sine(rate, 220.0, 3.0, 0.3);
        for (i, v) in sine(rate, 5000.0, 3.0, 0.2).into_iter().enumerate() {
            if (i / 4410) % 2 == 0 {
                s[i] += v;
            }
        }
        let stereo: Vec<f32> = s.iter().flat_map(|v| [*v, *v * 0.5]).collect();
        let mut whole = Analyzer::new(rate, s.len(), 0.25);
        whole.add_interleaved(&stereo, 2);
        let whole = whole.finish();
        let mut sliced = Analyzer::new(rate, s.len(), 0.25);
        let right: Vec<f32> = s.iter().map(|v| v * 0.5).collect();
        let mut most = 0;
        for (l, r) in s.chunks(1000).zip(right.chunks(1000)) {
            sliced.add_planar(&[l, r]);
            // A few frames at a time, as far as the samples allow.
            let mut made = sliced.progress().0;
            while !sliced.step(3) && sliced.progress().0 > made {
                made = sliced.progress().0;
            }
            most = most.max(sliced.mid.len());
        }
        assert!(most < window(rate) + 2000, "{most}");
        assert_eq!(sliced.finish(), whole);
    }

    #[test]
    fn silence_and_nothing_analyze_quietly() {
        let m = Analyzer::new(48_000.0, 0, 0.0).finish();
        assert_eq!(m.frames(), 0);
        let m = Analyzer::new(48_000.0, 4800, 0.0).finish();
        assert!(m.loudness.iter().all(|v| *v == 0.0));
        assert!(m.bands.iter().all(|v| *v == 0.0));
    }
}
