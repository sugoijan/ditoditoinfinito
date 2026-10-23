//! The music's analysis for the editor's audio view
//! (`docs/plans/editor.md`, "The audio view"): made once from the music
//! the shell decoded for listening, the very samples listening plays
//! and with the same start (the web's MP3 lead), so song second `s` is
//! the same sample in what is heard and what is drawn. Three things
//! come of one pass over the samples: the overview's loudness
//! ([`overview`]), the waveform (the mid channel, kept to the sample,
//! with a min/max pyramid over it) and the spectrogram (a short-time
//! Fourier transform in log-spaced bands, each frame at its window's
//! middle).
//!
//! [`Builder`] is fed the samples in order and stepped in slices
//! ([`Builder::step`]): the web steps it a few milliseconds a frame,
//! desktop runs it whole on a thread. Stepped in any slices, it makes
//! the same analysis.
//!
//! What it costs: the mid channel as 16-bit samples (2 bytes a sample)
//! and its pyramid (about half a byte a sample), and a byte for each of
//! [`BANDS`] bands every [`hop`] (5 ms): a 5-minute song at 48 kHz
//! keeps about 36 MB for the waveform and 7.7 MB for the spectrogram,
//! against the 115 MB of its decoded stereo music.

use super::overview::{self, Overview};

/// The spectrogram's bands, log-spaced from [`LOW_HZ`] to [`HIGH_HZ`].
pub const BANDS: usize = 128;
pub const LOW_HZ: f64 = 30.0;
pub const HIGH_HZ: f64 = 12_000.0;
/// Decibels under the song's loudest band drawn at all.
pub const RANGE_DB: f32 = 70.0;
/// Decibels under full scale a stored band's 0 is ([`Spectrogram`]'s
/// bytes cover this range).
const FLOOR_DB: f32 = 96.0;
/// The pyramid's fan-out: each level's block is this many of the level
/// under it.
const FAN: usize = 16;
/// Samples the pyramid takes in at a time (one unit of a step).
const CHUNK: usize = 4096;

/// The analysis' window and hop at `rate` Hz: the power of two nearest
/// 21.3 ms (1024 at 44.1 and 48 kHz), and 5 ms.
pub fn window(rate: f64) -> usize {
    let n = (rate.max(1000.0) * 0.0213).log2().round() as u32;
    1usize << n.clamp(8, 13)
}

/// Samples between spectrogram frames at `rate` Hz (5 ms).
pub fn hop(rate: f64) -> usize {
    ((rate.max(1000.0) * 0.005).floor() as usize).max(1)
}

/// The loudness over a span of the waveform, as fractions of the song's
/// loudest sample.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Peak {
    pub min: f32,
    pub max: f32,
    pub rms: f32,
}

impl Peak {
    /// Both spans'.
    pub fn merge(self, o: Peak) -> Peak {
        Peak {
            min: self.min.min(o.min),
            max: self.max.max(o.max),
            rms: self.rms.max(o.rms),
        }
    }
}

/// One level of the pyramid: each block's least and greatest sample and
/// its samples' squares summed (in full scale's units).
#[derive(Clone, Debug, Default, PartialEq)]
struct Level {
    min: Vec<i16>,
    max: Vec<i16>,
    squares: Vec<f32>,
}

/// The mid channel, sample by sample, and the pyramid over it.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Waveform {
    samples: Vec<i16>,
    /// Blocks of `FAN`, `FAN²`, … samples.
    levels: Vec<Level>,
    /// The loudest sample (its magnitude, at least 1).
    peak: i16,
}

/// What a query of the waveform gathers.
#[derive(Clone, Copy)]
struct Gather {
    min: i16,
    max: i16,
    squares: f64,
    count: usize,
}

impl Gather {
    fn sample(&mut self, v: i16) {
        self.min = self.min.min(v);
        self.max = self.max.max(v);
        let f = f64::from(v) / 32768.0;
        self.squares += f * f;
        self.count += 1;
    }

    fn block(&mut self, l: &Level, i: usize, n: usize) {
        self.min = self.min.min(l.min[i]);
        self.max = self.max.max(l.max[i]);
        self.squares += f64::from(l.squares[i]);
        self.count += n;
    }
}

impl Waveform {
    /// Samples kept.
    pub fn len(&self) -> usize {
        self.samples.len()
    }

    pub fn is_empty(&self) -> bool {
        self.samples.is_empty()
    }

    /// The span of samples `[i, j)` (`None` when empty), from the
    /// pyramid's blocks where whole ones fit and the samples at the ends.
    pub fn span(&self, i: usize, j: usize) -> Option<Peak> {
        let j = j.min(self.samples.len());
        if i >= j {
            return None;
        }
        let mut g = Gather {
            min: i16::MAX,
            max: i16::MIN,
            squares: 0.0,
            count: 0,
        };
        let (mut lo, mut hi) = (i, j);
        // `size`: the samples a block of the level `level` (0: the
        // samples themselves) holds.
        let (mut size, mut level) = (1usize, 0usize);
        loop {
            let next = size * FAN;
            let (up, down) = (lo.div_ceil(next) * next, hi / next * next);
            let top = level >= self.levels.len();
            if top || up >= down {
                self.gather(&mut g, level, size, lo, hi);
                break;
            }
            self.gather(&mut g, level, size, lo, up);
            self.gather(&mut g, level, size, down, hi);
            (lo, hi) = (up, down);
            size = next;
            level += 1;
        }
        let peak = f32::from(self.peak.max(1));
        Some(Peak {
            min: f32::from(g.min) / peak,
            max: f32::from(g.max) / peak,
            rms: ((g.squares / g.count.max(1) as f64).sqrt() * 32768.0) as f32 / peak,
        })
    }

    /// Samples `[lo, hi)` (whole blocks of `size` at `level`) into `g`.
    fn gather(&self, g: &mut Gather, level: usize, size: usize, lo: usize, hi: usize) {
        if level == 0 {
            for &v in &self.samples[lo..hi] {
                g.sample(v);
            }
        } else {
            let l = &self.levels[level - 1];
            for b in lo / size..hi / size {
                g.block(l, b, size);
            }
        }
    }
}

/// The spectrogram: [`BANDS`] bytes a frame, frame `k` the window
/// around sample `k * hop`.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Spectrogram {
    /// Samples between frames, and a window's.
    pub hop: usize,
    pub window: usize,
    /// Each frame's bands, low to high: 0 at [`FLOOR_DB`] under full
    /// scale or less, 255 at full scale.
    bands: Vec<u8>,
    /// The loudest band's byte.
    loudest: u8,
}

impl Spectrogram {
    pub fn frames(&self) -> usize {
        self.bands.len() / BANDS
    }

    fn frame(&self, k: usize) -> &[u8] {
        &self.bands[k * BANDS..(k + 1) * BANDS]
    }

    /// Each byte's level from 0 to 1: [`RANGE_DB`] under the song's
    /// loudest band and up.
    pub fn levels(&self) -> [f32; 256] {
        let step = FLOOR_DB / 255.0;
        let top = f32::from(self.loudest) * step;
        std::array::from_fn(|v| ((v as f32 * step - (top - RANGE_DB)) / RANGE_DB).clamp(0.0, 1.0))
    }
}

/// The analysis of a song's music.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Analysis {
    /// The song second the first sample is heard at.
    pub start: f64,
    pub rate: f64,
    pub overview: Overview,
    pub wave: Waveform,
    pub spectrogram: Spectrogram,
}

impl Analysis {
    /// The song second the music ends at.
    pub fn end(&self) -> f64 {
        self.start + self.wave.len() as f64 / self.rate
    }

    /// The first sample heard at or after song second `s`.
    fn sample_at(&self, s: f64) -> usize {
        ((s - self.start) * self.rate).ceil().max(0.0) as usize
    }

    /// The samples heard over song seconds `[a, b)`.
    pub fn wave(&self, a: f64, b: f64) -> Option<Peak> {
        self.wave.span(self.sample_at(a), self.sample_at(b))
    }

    /// The spectrogram over song seconds `[a, b)` into `out` (a byte a
    /// band): the loudest of the frames whose middles are in it, else
    /// the frames either side of its middle in proportion. Whether there
    /// is any.
    pub fn spectrum(&self, a: f64, b: f64, out: &mut [u8; BANDS]) -> bool {
        let sp = &self.spectrogram;
        let n = sp.frames();
        if n == 0 || b <= a {
            return false;
        }
        let per = sp.hop as f64 / self.rate;
        let first = ((a - self.start) / per).ceil().max(0.0) as usize;
        let last = (((b - self.start) / per).ceil().max(0.0) as usize).min(n);
        if first < last {
            out.copy_from_slice(sp.frame(first));
            for k in first + 1..last {
                for (o, v) in out.iter_mut().zip(sp.frame(k)) {
                    *o = (*o).max(*v);
                }
            }
            return true;
        }
        let f = ((a + b) / 2.0 - self.start) / per;
        if f < 0.0 || f > (n - 1) as f64 {
            return false;
        }
        let k = (f.floor() as usize).min(n - 1);
        // The next frame's share, of 256.
        let t = ((f - k as f64) * 256.0).round().clamp(0.0, 256.0) as u32;
        let (p, q) = (sp.frame(k), sp.frame((k + 1).min(n - 1)));
        for ((o, x), y) in out.iter_mut().zip(p).zip(q) {
            *o = ((u32::from(*x) * (256 - t) + u32::from(*y) * t + 128) >> 8) as u8;
        }
        true
    }
}

/// A radix-2 transform of `n` complex values, its twiddles and order.
#[derive(Clone)]
struct Fft {
    n: usize,
    reverse: Vec<u32>,
    twiddles: Vec<(f32, f32)>,
}

impl Fft {
    fn new(n: usize) -> Fft {
        let bits = n.trailing_zeros();
        let reverse = (0..n as u32)
            .map(|i| {
                if bits == 0 {
                    0
                } else {
                    i.reverse_bits() >> (32 - bits)
                }
            })
            .collect();
        let twiddles = (0..n / 2)
            .map(|k| {
                let a = -2.0 * std::f64::consts::PI * k as f64 / n as f64;
                (a.cos() as f32, a.sin() as f32)
            })
            .collect();
        Fft {
            n,
            reverse,
            twiddles,
        }
    }

    /// In place.
    fn run(&self, re: &mut [f32], im: &mut [f32]) {
        let n = self.n;
        for i in 0..n {
            let j = self.reverse[i] as usize;
            if j > i {
                re.swap(i, j);
                im.swap(i, j);
            }
        }
        let mut len = 2;
        while len <= n {
            let half = len / 2;
            let stride = n / len;
            for start in (0..n).step_by(len) {
                for k in 0..half {
                    let (wr, wi) = self.twiddles[k * stride];
                    let (a, b) = (start + k, start + k + half);
                    let tr = re[b] * wr - im[b] * wi;
                    let ti = re[b] * wi + im[b] * wr;
                    re[b] = re[a] - tr;
                    im[b] = im[a] - ti;
                    re[a] += tr;
                    im[a] += ti;
                }
            }
            len *= 2;
        }
    }
}

/// How a band is made of the transform's bins: the energy of bins
/// `[from, to)` (the bands widen with the frequency, as music's energy
/// falls: steady across them), or a bin's between bins `at` and
/// `at + 1` in proportion (a band narrower than a bin).
#[derive(Clone, Copy, Debug)]
enum Band {
    Sum { from: usize, to: usize },
    Between { at: usize, t: f32 },
}

/// The spectrogram being made, a frame at a time.
#[derive(Clone)]
struct Frames {
    hop: usize,
    window: usize,
    hann: Vec<f32>,
    fft: Fft,
    bands: [Band; BANDS],
    /// The power of a full-scale sine's bin.
    full: f32,
    re: Vec<f32>,
    im: Vec<f32>,
    power: Vec<f32>,
    /// The real transform's twiddles, a bin each.
    turns: Vec<(f32, f32)>,
    out: Vec<u8>,
}

impl Frames {
    fn new(rate: f64) -> Frames {
        let window = window(rate);
        let hann = (0..window)
            .map(|i| {
                (0.5 - 0.5 * (2.0 * std::f64::consts::PI * i as f64 / window as f64).cos()) as f32
            })
            .collect();
        let bin_hz = rate / window as f64;
        let high = HIGH_HZ.min(rate * 0.475);
        let bands = std::array::from_fn(|b| {
            let edge = |b: usize| LOW_HZ * (high / LOW_HZ).powf(b as f64 / BANDS as f64);
            let (lo, hi) = (edge(b), edge(b + 1));
            let from = (lo / bin_hz).ceil() as usize;
            let to = ((hi / bin_hz).ceil() as usize).min(window / 2 + 1);
            if to > from {
                Band::Sum { from, to }
            } else {
                let f = (lo * hi).sqrt() / bin_hz;
                Band::Between {
                    at: (f.floor() as usize).min(window / 2 - 1),
                    t: f.fract() as f32,
                }
            }
        });
        let full = (window as f32 / 4.0).powi(2);
        Frames {
            hop: hop(rate),
            window,
            hann,
            fft: Fft::new(window / 2),
            bands,
            full,
            re: vec![0.0; window / 2],
            im: vec![0.0; window / 2],
            power: vec![0.0; window / 2 + 1],
            turns: (0..=window / 2)
                .map(|k| {
                    let a = -2.0 * std::f64::consts::PI * k as f64 / window as f64;
                    (a.cos() as f32, a.sin() as f32)
                })
                .collect(),
            out: Vec::new(),
        }
    }

    /// Frame `k` of `samples` (all `total` of them known up to its end).
    fn frame(&mut self, samples: &[i16], k: usize) {
        let n = self.window;
        let half = n / 2;
        let center = k * self.hop;
        let at = |i: usize| -> f32 {
            // The window's sample `i`: zero outside the music.
            let s = (center + i).checked_sub(half);
            match s.and_then(|s| samples.get(s)) {
                Some(v) => f32::from(*v) / 32768.0,
                None => 0.0,
            }
        };
        // A real transform of `n` as a complex one of `n / 2`: even
        // samples real, odd imaginary.
        for m in 0..half {
            self.re[m] = at(2 * m) * self.hann[2 * m];
            self.im[m] = at(2 * m + 1) * self.hann[2 * m + 1];
        }
        self.fft.run(&mut self.re, &mut self.im);
        for k in 0..=half {
            let (zr, zi) = (self.re[k % half], self.im[k % half]);
            let (cr, ci) = (self.re[(half - k) % half], -self.im[(half - k) % half]);
            // Even part and odd part, the odd turned by its twiddle.
            let (er, ei) = ((zr + cr) / 2.0, (zi + ci) / 2.0);
            let (dr, di) = ((zr - cr) / 2.0, (zi - ci) / 2.0);
            // (d / i) = (di, -dr)
            let (or, oi) = (di, -dr);
            let (wr, wi) = self.turns[k];
            let (xr, xi) = (er + or * wr - oi * wi, ei + or * wi + oi * wr);
            self.power[k] = xr * xr + xi * xi;
        }
        let step = 255.0 / FLOOR_DB;
        for band in &self.bands {
            let p = match *band {
                Band::Sum { from, to } => self.power[from..to].iter().sum::<f32>(),
                Band::Between { at, t } => {
                    self.power[at] + (self.power[at + 1] - self.power[at]) * t
                }
            };
            let db = 10.0 * (p / self.full).max(1e-20).log10();
            self.out
                .push(((db + FLOOR_DB) * step).round().clamp(0.0, 255.0) as u8);
        }
    }
}

/// An analysis being made: fed the samples in order, stepped in slices.
pub struct Builder {
    start: f64,
    rate: f64,
    total: usize,
    samples: Vec<i16>,
    overview: overview::Builder,
    levels: Vec<Level>,
    /// Samples the pyramid took in.
    leveled: usize,
    frames: Frames,
    /// Frames made, and the number there are.
    made: usize,
    count: usize,
    mid: Vec<f32>,
}

impl Builder {
    /// For `frames` frames at `rate` Hz, the first heard at song second
    /// `start` (an MP3's lead on the web).
    pub fn new(rate: f64, frames: usize, start: f64) -> Builder {
        let rate = rate.max(1.0);
        let mut f = Frames::new(rate);
        let count = if frames == 0 {
            0
        } else {
            (frames - 1) / f.hop + 1
        };
        f.out.reserve_exact(count * BANDS);
        // The pyramid's levels: while a block is shorter than the music.
        let mut levels = Vec::new();
        let mut size = FAN;
        while size <= frames {
            let n = frames.div_ceil(size);
            levels.push(Level {
                min: Vec::with_capacity(n),
                max: Vec::with_capacity(n),
                squares: Vec::with_capacity(n),
            });
            size *= FAN;
        }
        Builder {
            start,
            rate,
            total: frames,
            samples: Vec::with_capacity(frames),
            overview: overview::Builder::new(rate, frames, start),
            levels,
            leveled: 0,
            frames: f,
            made: 0,
            count,
            mid: Vec::new(),
        }
    }

    /// Frames taken in so far.
    pub fn fed(&self) -> usize {
        self.samples.len()
    }

    /// The next frames of each channel (planar, the same length; more
    /// than the music's frames are left out).
    pub fn add_planar(&mut self, channels: &[&[f32]]) {
        let Some(n) = channels.iter().map(|c| c.len()).min() else {
            return;
        };
        let n = n.min(self.total - self.samples.len());
        let k = channels.len() as f32;
        self.mid.clear();
        self.mid
            .extend((0..n).map(|i| channels.iter().map(|c| c[i]).sum::<f32>() / k));
        self.take_mid();
    }

    /// The next frames, interleaved `channels` to a frame.
    pub fn add_interleaved(&mut self, data: &[f32], channels: usize) {
        let channels = channels.max(1);
        let k = channels as f32;
        let n = (data.len() / channels).min(self.total - self.samples.len());
        self.mid.clear();
        self.mid.extend(
            data.chunks_exact(channels)
                .take(n)
                .map(|f| f.iter().sum::<f32>() / k),
        );
        self.take_mid();
    }

    fn take_mid(&mut self) {
        let at = self.samples.len();
        self.overview.add(at, &self.mid);
        self.samples.extend(
            self.mid
                .iter()
                .map(|v| (v.clamp(-1.0, 1.0) * 32767.0).round() as i16),
        );
    }

    /// Every sample is in and analyzed.
    pub fn done(&self) -> bool {
        self.samples.len() == self.total && self.leveled == self.total && self.made == self.count
    }

    /// Does up to `units` of the work the samples so far allow (a unit:
    /// a spectrogram frame, or [`CHUNK`] samples into the pyramid);
    /// whether it is all done.
    pub fn step(&mut self, units: usize) -> bool {
        let mut left = units;
        let fed = self.samples.len();
        while left > 0 && self.leveled < fed {
            let to = (self.leveled + CHUNK).min(fed);
            // Whole blocks only, until the music's end.
            let to = if to == self.total { to } else { to / FAN * FAN };
            if to <= self.leveled {
                break;
            }
            self.level(self.leveled, to);
            self.leveled = to;
            left -= 1;
        }
        let half = self.frames.window / 2;
        while left > 0 && self.made < self.count {
            let end = self.made * self.frames.hop + half;
            if end > fed && fed < self.total {
                break;
            }
            self.frames.frame(&self.samples, self.made);
            self.made += 1;
            left -= 1;
        }
        self.done()
    }

    /// The pyramid's blocks from sample `from` to `to` (`from` on a
    /// block's start; `to` too, or the music's end).
    fn level(&mut self, from: usize, to: usize) {
        let mut size = FAN;
        for l in 0..self.levels.len() {
            let first = from / size;
            let last = if to == self.total {
                to.div_ceil(size)
            } else {
                to / size
            };
            for b in first..last {
                if self.levels[l].min.len() > b {
                    continue;
                }
                let (lo, hi) = (b * size, ((b + 1) * size).min(self.total));
                let (mut mn, mut mx, mut sq) = (i16::MAX, i16::MIN, 0.0f64);
                if l == 0 {
                    for &v in &self.samples[lo..hi] {
                        mn = mn.min(v);
                        mx = mx.max(v);
                        let f = f64::from(v) / 32768.0;
                        sq += f * f;
                    }
                } else {
                    let below = &self.levels[l - 1];
                    let (a, z) = (b * FAN, ((b + 1) * FAN).min(below.min.len()));
                    for i in a..z {
                        mn = mn.min(below.min[i]);
                        mx = mx.max(below.max[i]);
                        sq += f64::from(below.squares[i]);
                    }
                }
                let level = &mut self.levels[l];
                level.min.push(mn);
                level.max.push(mx);
                level.squares.push(sq as f32);
            }
            size *= FAN;
        }
    }

    /// The analysis (stepped to the end first).
    pub fn finish(mut self) -> Analysis {
        while !self.step(usize::MAX) {
            if self.samples.len() < self.total {
                // Samples never fed: silence.
                let n = self.total - self.samples.len();
                self.mid.clear();
                self.mid.resize(n, 0.0);
                self.take_mid();
            }
        }
        let peak = self
            .samples
            .iter()
            .map(|v| v.unsigned_abs())
            .max()
            .unwrap_or(0)
            .clamp(1, i16::MAX as u16) as i16;
        let loudest = self.frames.out.iter().copied().max().unwrap_or(0);
        Analysis {
            start: self.start,
            rate: self.rate,
            overview: self.overview.finish(),
            wave: Waveform {
                samples: self.samples,
                levels: self.levels,
                peak,
            },
            spectrogram: Spectrogram {
                hop: self.frames.hop,
                window: self.frames.window,
                bands: self.frames.out,
                loudest,
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Clicks (a decaying 2 kHz burst) at `at` seconds in `seconds` of
    /// quiet noise at `rate`.
    fn clicks(rate: f64, seconds: f64, at: &[f64]) -> Vec<f32> {
        let n = (rate * seconds) as usize;
        let mut s: Vec<f32> = (0..n)
            .map(|i| ((i as f32 * 12.9898).sin() * 43758.547).fract() * 0.002)
            .collect();
        for &t in at {
            let i0 = (t * rate).round() as usize;
            for i in 0..(rate * 0.03) as usize {
                if let Some(v) = s.get_mut(i0 + i) {
                    let x = i as f64 / rate;
                    *v += ((2.0 * std::f64::consts::PI * 2000.0 * x).sin() * (-x * 200.0).exp())
                        as f32
                        * 0.8;
                }
            }
        }
        s
    }

    /// Short broadband ticks (three samples) at `at` seconds in quiet.
    fn ticks(rate: f64, seconds: f64, at: &[f64]) -> Vec<f32> {
        let mut s = clicks(rate, seconds, &[]);
        for &t in at {
            let i0 = (t * rate).round() as usize;
            for (i, v) in [0.9, -0.6, 0.3].into_iter().enumerate() {
                s[i0 + i] += v;
            }
        }
        s
    }

    #[test]
    fn the_window_and_the_hop() {
        assert_eq!(window(44_100.0), 1024);
        assert_eq!(window(48_000.0), 1024);
        assert_eq!(window(22_050.0), 512);
        assert_eq!(window(96_000.0), 2048);
        assert_eq!(hop(48_000.0), 240);
        assert_eq!(hop(44_100.0), 220);
    }

    #[test]
    fn the_transform_finds_a_sine() {
        let fft = Fft::new(256);
        let mut re: Vec<f32> = (0..256)
            .map(|i| (2.0 * std::f32::consts::PI * 10.0 * i as f32 / 256.0).cos())
            .collect();
        let mut im = vec![0.0; 256];
        fft.run(&mut re, &mut im);
        let mag: Vec<f32> = re
            .iter()
            .zip(&im)
            .map(|(r, i)| (r * r + i * i).sqrt())
            .collect();
        assert!((mag[10] - 128.0).abs() < 1e-2, "{}", mag[10]);
        assert!((mag[246] - 128.0).abs() < 1e-2);
        assert!(mag[11] < 1e-2 && mag[3] < 1e-2);
    }

    #[test]
    fn a_full_scale_sine_is_near_the_top_in_its_band() {
        let rate = 48_000.0;
        let s: Vec<f32> = (0..48_000)
            .map(|i| (2.0 * std::f64::consts::PI * 1000.0 * i as f64 / rate).sin() as f32)
            .collect();
        let mut b = Builder::new(rate, s.len(), 0.0);
        b.add_interleaved(&s, 1);
        let a = b.finish();
        let mut out = [0u8; BANDS];
        assert!(a.spectrum(0.5, 0.5 + 1e-4, &mut out));
        let (band, &v) = out.iter().enumerate().max_by_key(|(_, v)| **v).unwrap();
        let edge = |b: usize| LOW_HZ * (HIGH_HZ / LOW_HZ).powf(b as f64 / BANDS as f64);
        assert!(edge(band) <= 1100.0 && edge(band + 1) >= 900.0, "{band}");
        // Within 4 dB of full scale.
        assert!(f32::from(v) * FLOOR_DB / 255.0 > FLOOR_DB - 4.0, "{v}");
        assert_eq!(a.spectrogram.levels()[usize::from(v)], 1.0);
        // The loudest sample, and the waveform's span of one cycle.
        let p = a.wave(0.5, 0.501).unwrap();
        assert!(p.max > 0.99 && p.min < -0.99, "{p:?}");
        assert!(
            (p.rms - std::f32::consts::FRAC_1_SQRT_2).abs() < 0.02,
            "{p:?}"
        );
    }

    #[test]
    fn the_pyramid_agrees_with_the_samples() {
        let rate = 8000.0;
        let s = clicks(rate, 7.3, &[0.5, 3.0, 6.9]);
        let mut b = Builder::new(rate, s.len(), 0.0);
        b.add_interleaved(&s, 1);
        let a = b.finish();
        let w = &a.wave;
        assert!(w.levels.len() >= 3, "{}", w.levels.len());
        for (i, j) in [
            (0, 1),
            (5, 6),
            (15, 17),
            (100, 5000),
            (4095, 4097),
            (0, s.len()),
            (3, s.len()),
            (17_000, 58_399),
        ] {
            let got = w.span(i, j).unwrap();
            let q: Vec<i16> = s[i..j]
                .iter()
                .map(|v| (v.clamp(-1.0, 1.0) * 32767.0).round() as i16)
                .collect();
            let peak = f32::from(w.peak);
            let mn = f32::from(*q.iter().min().unwrap()) / peak;
            let mx = f32::from(*q.iter().max().unwrap()) / peak;
            let rms = (q
                .iter()
                .map(|v| (f64::from(*v) / 32768.0).powi(2))
                .sum::<f64>()
                / q.len() as f64)
                .sqrt() as f32
                * 32768.0
                / peak;
            assert_eq!((got.min, got.max), (mn, mx), "{i}..{j}");
            assert!((got.rms - rms).abs() < 1e-4 * rms.max(1.0), "{i}..{j}");
        }
        assert_eq!(w.span(10, 10), None);
        assert_eq!(w.span(s.len(), s.len() + 9), None);
    }

    #[test]
    fn a_click_is_at_its_second_in_the_waveform_and_the_spectrogram() {
        let rate = 44_100.0;
        let lead = 0.026;
        let at = [0.75, 1.5, 2.012_345, 3.3];
        let s = clicks(rate, 4.0, &at);
        let stereo: Vec<f32> = s.iter().flat_map(|v| [*v, *v]).collect();
        let mut b = Builder::new(rate, s.len(), lead);
        b.add_interleaved(&stereo, 2);
        let a = b.finish();
        assert!((a.end() - (lead + 4.0)).abs() < 1e-6);
        let hop_s = a.spectrogram.hop as f64 / rate;
        for &t in &at {
            let heard = t + lead;
            // Waveform: nothing loud just before, loud just after.
            let before = a.wave(heard - 0.002, heard - 1e-6).unwrap();
            let after = a.wave(heard, heard + 0.002).unwrap();
            assert!(before.max < 0.05, "{t}: {before:?}");
            assert!(after.max > 0.5, "{t}: {after:?}");
        }
        // Spectrogram: a frame is its window's middle, so the frame of
        // the most energy of a tick is within half a hop of it.
        let s = ticks(rate, 4.0, &at);
        let mut b = Builder::new(rate, s.len(), lead);
        b.add_interleaved(&s, 1);
        let a = b.finish();
        for &t in &at {
            let heard = t + lead;
            let mut best = (f64::NAN, 0u8);
            let mut out = [0u8; BANDS];
            let mut s = heard - 0.03;
            while s < heard + 0.03 {
                a.spectrum(s, s + hop_s, &mut out);
                let band = out
                    [(BANDS as f64 * (2000.0f64 / LOW_HZ).ln() / (HIGH_HZ / LOW_HZ).ln()) as usize];
                if band > best.1 {
                    best = (s, band);
                }
                s += hop_s;
            }
            // The frame's middle: the span's first frame.
            let k = ((best.0 - lead) / hop_s).ceil();
            let middle = lead + k * hop_s;
            assert!(
                (middle - heard).abs() <= hop_s / 2.0 + 1e-9,
                "{t}: {middle}"
            );
        }
    }

    #[test]
    fn stepped_in_slices_it_is_the_same() {
        let rate = 22_050.0;
        let s = clicks(rate, 3.1, &[0.4, 1.9]);
        let stereo: Vec<f32> = s.iter().flat_map(|v| [*v, -*v * 0.5]).collect();
        let mut whole = Builder::new(rate, s.len(), 0.1);
        whole.add_interleaved(&stereo, 2);
        let whole = whole.finish();
        // Fed in uneven blocks, planar, a few units a step between.
        let (l, r): (Vec<f32>, Vec<f32>) = stereo.chunks(2).map(|f| (f[0], f[1])).unzip();
        let mut b = Builder::new(rate, s.len(), 0.1);
        let mut at = 0;
        let mut size = 1;
        let mut steps = 0;
        while at < s.len() {
            let n = size.min(s.len() - at);
            b.add_planar(&[&l[at..at + n], &r[at..at + n]]);
            at += n;
            size = size * 3 + 7;
            b.step(3);
            steps += 1;
        }
        while !b.step(2) {
            steps += 1;
        }
        assert!(steps > 20);
        let sliced = b.finish();
        assert_eq!(sliced, whole);
        assert!(sliced.spectrogram.frames() > 600);
        assert_eq!(sliced.overview.levels.len(), whole.overview.levels.len());
    }

    #[test]
    fn empty_and_tiny_music() {
        let a = Builder::new(48_000.0, 0, 0.0).finish();
        assert!(a.wave.is_empty());
        assert_eq!(a.wave(0.0, 1.0), None);
        let mut out = [0u8; BANDS];
        assert!(!a.spectrum(0.0, 1.0, &mut out));
        let mut b = Builder::new(48_000.0, 5, 0.0);
        b.add_interleaved(&[0.5, -0.5, 0.25, 0.0, 1.0], 1);
        let a = b.finish();
        assert_eq!(a.wave(0.0, 1.0).unwrap().max, 1.0);
        assert_eq!(a.spectrogram.frames(), 1);
    }
}
