//! A recording tidied (`docs/plans/announcer-studio.md`, decision 5), and
//! our packs' sounds with it: `tools/announcer-design/kokoro.py` hands
//! each line's speech to this code (`xtask announcer tidy`), so there is
//! one leveler (`docs/plans/announcers.md`, "Louder packs" and "Smoother
//! leveling"). The silence at both ends is cut, with short fades where it
//! was cut, and the level is evened ([`LEVELING`]) by a chain that
//! reaches it smoothly ([`CHAIN`]):
//!
//! 1. a phase rotator ([`rotated`]): a voice whose wave is narrow pulses
//!    (all its harmonics at their peak at once) has them spread in time,
//!    which changes no frequency's level and lowers the peaks;
//! 2. a compressor on the envelope ([`compressor`]): the level over the
//!    next few milliseconds, against the sound's own voiced level, turns
//!    the louder syllables down, so that a line is evener before anything
//!    is asked of its peaks;
//! 3. one gain, to a voiced RMS of [`VOICED_RMS_DB`] for what comes out;
//! 4. a limiter ([`limiter`]) for the peaks left over, which keeps the
//!    wave, between its samples too, under [`LIMIT_DB`] (true peaks under
//!    -1 dBFS once encoded): its gain comes down over its look-ahead, is
//!    held over a pitch period and let back up over tens of milliseconds,
//!    so that it rides a syllable's level and does not reshape its wave;
//!    only what a peak needs beyond a few decibels of that is taken of
//!    the peak alone, shaping it.
//!
//! The game times `here we go` and the cheer from a sound's first sample,
//! so a line recorded after a second of silence would start a second
//! late.
//!
//! The floor the ends are cut at rises over a recording's noise. Every
//! channel is cut and leveled alike, under one gain; a take keeps its
//! channels. A sound at the level already (within [`TIDY_DB`] of it: one
//! tidied before, read back from its file) gets that little gain and the
//! limiter for a stray peak, and is not rotated or compressed again.
//!
//! The leveling is exact arithmetic in a fixed order (sums, products,
//! quotients and square roots of IEEE floats, no fused operations and no
//! library function a sample goes through), so the same samples give the
//! same samples on every system; only the cut's and the voiced frames'
//! thresholds are compared in decibels (a logarithm, whose last bit a
//! system's library may round differently: a frame exactly on a
//! threshold).

use serde::{Deserialize, Serialize};

/// Levels are measured over frames this long.
const FRAME: f64 = 0.01;
/// The ends are cut where frames fall this far under the loudest…
const FLOOR_DB: f64 = -45.0;
/// …or under a recording's noise by this much, never closer to the
/// loudest than [`HIGHEST_FLOOR_DB`].
const OVER_NOISE_DB: f64 = 10.0;
const HIGHEST_FLOOR_DB: f64 = -20.0;
/// What is kept before the first frame over the floor and after the last.
const HEAD: f64 = 0.03;
const TAIL: f64 = 0.1;
const FADE_IN: f64 = 0.005;
const FADE_OUT: f64 = 0.015;
/// Frames this close to the loudest are voiced, and leveled.
const VOICED_DB: f64 = -30.0;
/// The leveling's name (a pack's record says it): the target and the
/// chain below ([`CHAIN`]).
pub const LEVELING: &str = "announcer-level-v3";
/// The level a tidied sound's voiced frames come out at, dBFS RMS: about
/// -13 LUFS, the loudest at which the limiter has little left to do for
/// our voices (a median of 4 dB or less of each pack's sounds; v1, to
/// 2026-11-12: -18 with no limiter, about -19.5 LUFS; v2: -9, a song's
/// music's loudness, by one gain into a fast limiter that took a median
/// of 8 and 15 dB of our packs' peaks).
pub const VOICED_RMS_DB: f64 = -12.0;
/// [`VOICED_RMS_DB`] as an amplitude (written out: no library's rounding).
const VOICED_RMS: f64 = 0.251_188_643_150_958;
/// True peaks are kept under this, dBFS: a sound whose loudest moment,
/// between its samples too ([`true_peaks`]), is under it is left to the
/// gain alone, and one over it goes through the limiter.
pub const PEAK_DB: f64 = -1.0;
/// [`PEAK_DB`] as an amplitude.
const PEAK: f64 = 0.891_250_938_133_745_6;
/// The limiter's ceiling, dBFS: of what goes through it, neither a sample
/// nor the wave between two comes out over it. Half a decibel under
/// [`PEAK_DB`], for an encoder's overshoot, and so that a limited sound
/// read back from its encoded file is left alone
/// ([`Measure::already_tidy`]).
pub const LIMIT_DB: f64 = -1.5;
/// [`LIMIT_DB`] as an amplitude.
const LIMIT: f64 = 0.841_395_141_645_195_1;
/// The limiter's knee: from this share of the ceiling (1.5 dB under it)
/// a peak is turned down smoothly, by a parabola whose slope reaches
/// nothing at the ceiling.
const KNEE_START: f64 = 0.841_395_141_645_195_1;
/// The most the leveling raises a sound over what would bring its voiced
/// RMS to the target with no limiter (a sound of clicks and little else
/// is not raised forever): 24 dB, as a gain.
const MAX_OVER: f64 = 15.848_931_924_611_133;
/// The halvings the gain is found in.
const PASSES: usize = 12;
/// How far from where tidying would put them a take's ends and level may
/// be and it count as tidy already: what a tidied, encoded file measures
/// when read back (the most of our packs' 421 MP3s: 10 ms to cut at the
/// start and 0.47 dB; of the same sounds through the export's Opus,
/// 10 ms and 0.10 dB).
const TIDY_START: f64 = 0.02;
const TIDY_END: f64 = 0.04;
/// The level's part of that, dB either way.
pub const TIDY_DB: f64 = 0.75;
/// [`TIDY_DB`] up and down as gains, the nearest `f32` inside each: a
/// gain between them is that of a sound at the level already
/// ([`at_level`]).
const TIDY_UP: f32 = 1.090_184_4;
const TIDY_DOWN: f32 = 0.917_275_97;
/// A sample this close to full scale, three in a row: it clipped.
const FULL_SCALE: f32 = 0.999;
const CLIP_RUN: usize = 3;

/// The dynamics chain's settings: the rotator's, the compressor's, then
/// the limiter's (times in seconds). [`CHAIN`] is the leveling's; the
/// study in this file's tests tries others.
#[derive(Clone, Copy, Debug, PartialEq)]
struct Chain {
    /// The phase rotator before the compressor: how many first-order
    /// all-pass stages, and their corner in Hz.
    stages: usize,
    corner: f64,
    /// The compressor's threshold over the sound's own voiced RMS, as an
    /// amplitude: a quiet take and a loud one are compressed alike.
    threshold: f64,
    /// Its slope over the threshold, as `power / 2^roots`: the gain is
    /// `(1 + p)^-(power / 2^roots)` for a power `p` over the
    /// threshold's (square roots and products alone), which is a ratio
    /// of `1 / (1 - 2 power / 2^roots)` to 1 far over the threshold and
    /// a knee about 12 dB wide around it.
    roots: u32,
    power: u32,
    /// The envelope: the mean power of the `window` to come, followed
    /// over `attack` where it rises and `release` where it falls.
    window: f64,
    attack: f64,
    release: f64,
    /// The limiter rides the level for what a peak needs up to `ride`
    /// (the least gain of that, as an amplitude): how far ahead it sees
    /// a peak coming (its gain comes down over that long), how long it
    /// holds its gain after one (a voice's pitch period: the next pulse
    /// finds it there), and over how long it lets it back up.
    ride: f64,
    look: f64,
    hold: f64,
    back: f64,
    /// What a peak needs beyond that, the limiter takes of the peak
    /// alone, shaping it: seen this far ahead, let back up over this
    /// long (v2's whole limiter was this).
    shape_look: f64,
    shape_back: f64,
}

/// The leveling's chain, chosen by the study over our two packs' speech
/// (`docs/plans/announcers.md`, "Smoother leveling"): eight all-pass
/// stages at 200 Hz (the Japanese voice's peaks 2.8 dB lower, the
/// English one's as they were; four stages do half of that); a
/// compressor of four to one from 6 dB under the voiced level, over a
/// 10 ms window, 3 ms up and 100 ms down (lower thresholds and higher
/// ratios compress more and leave the limiter no less); a limiter that
/// rides up to 3 dB, 2 ms ahead, held 8 ms, back over 50 ms, and shapes
/// what is over that as v2's did, 0.5 ms ahead and back over 1.5 ms.
const CHAIN: Chain = Chain {
    stages: 8,
    corner: 200.0,
    threshold: 0.501_187_233_627_272_2,
    roots: 3,
    power: 3,
    window: 0.01,
    attack: 0.003,
    release: 0.1,
    ride: 0.707_945_784_384_137_9,
    look: 0.002,
    hold: 0.008,
    back: 0.05,
    shape_look: 0.000_5,
    shape_back: 0.001_5,
};

/// What tidying a take does to it, measured on its decoded samples.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct Measure {
    /// As decoded.
    pub rate: u32,
    pub channels: u16,
    /// Its length as it came, in seconds.
    pub seconds: f64,
    /// It reached full scale when recorded (tidying cannot undo that).
    pub clipped: bool,
    /// The part tidying keeps, in seconds from its start.
    pub start: f64,
    pub end: f64,
    /// The gain tidying applies after the cut and the compressor (which
    /// keeps a sound's voiced level), before the limiter: about how much
    /// louder it comes out. Within [`TIDY_DB`] of none, the sound is at
    /// the level already and gets the gain alone ([`at_level`]).
    pub gain: f32,
}

impl Measure {
    /// Its length once tidied.
    pub fn tidied_seconds(&self) -> f64 {
        self.end - self.start
    }

    /// The gain in dB (positive: made louder).
    pub fn gain_db(&self) -> f64 {
        20.0 * f64::from(self.gain.max(1e-9)).log10()
    }

    /// Whether tidying would leave it about as it is: a take tidied
    /// before, which a pack opened again brings back.
    pub fn already_tidy(&self) -> bool {
        self.start <= TIDY_START && self.seconds - self.end <= TIDY_END && at_level(self.gain)
    }
}

/// What the chain did to a sound ([`tidied`]), each in dB (0: nothing).
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Did {
    /// The compressor's largest reduction, and its median over the
    /// voiced frames.
    pub compressed_db: f64,
    pub compressed_median_db: f64,
    /// The limiter's largest reduction…
    pub limited_db: f64,
    /// …of which by shaping a peak (the rest by riding the level)…
    pub peaks_db: f64,
    /// …and how much of the level it took: the voiced RMS of what it
    /// gave under that of what it was given.
    pub limited_level_db: f64,
    /// What of the result is not the sound (rotated, which changes no
    /// level) times one gain, over the result: the compressor's and the
    /// limiter's work together, a gain that moves slowly as much as one
    /// that reshapes the wave (-300: none).
    pub changed_db: f64,
}

/// Whether `gain` is that of a sound at the level already: it gets the
/// gain and the limiter, and is not compressed (again).
fn at_level(gain: f32) -> bool {
    (TIDY_DOWN..=TIDY_UP).contains(&gain)
}

fn db(ratio: f64) -> f64 {
    20.0 * ratio.max(1e-12).log10()
}

/// Each frame's power over the channels together.
fn powers<C: AsRef<[f32]>>(channels: &[C], frame: usize) -> Vec<f64> {
    let len = channels.first().map_or(0, |c| c.as_ref().len());
    let n = len / frame.max(1);
    (0..n)
        .map(|i| {
            let mut sum = 0.0f64;
            for c in channels {
                for &s in &c.as_ref()[i * frame..(i + 1) * frame] {
                    sum += f64::from(s) * f64::from(s);
                }
            }
            sum / (frame * channels.len().max(1)) as f64
        })
        .collect()
}

/// Each frame's level in dB under the loudest frame's.
fn levels(powers: &[f64]) -> Vec<f64> {
    let loudest = powers.iter().copied().fold(0.0f64, f64::max);
    powers
        .iter()
        .map(|p| db((p / loudest.max(1e-24)).sqrt()))
        .collect()
}

fn clipped<C: AsRef<[f32]>>(channels: &[C]) -> bool {
    channels.iter().any(|c| {
        let mut run = 0;
        c.as_ref().iter().any(|s| {
            run = if s.abs() >= FULL_SCALE { run + 1 } else { 0 };
            run >= CLIP_RUN
        })
    })
}

/// The samples `start..end` of each channel.
fn part<C: AsRef<[f32]>>(channels: &[C], start: usize, end: usize) -> Vec<Vec<f32>> {
    channels
        .iter()
        .map(|c| {
            let c = c.as_ref();
            c[start.min(c.len())..end.min(c.len())].to_vec()
        })
        .collect()
}

/// `planes` faded in and out.
fn fade(planes: &mut [Vec<f32>], rate: u32) {
    let fade_in = (FADE_IN * f64::from(rate)) as usize;
    let fade_out = (FADE_OUT * f64::from(rate)) as usize;
    for c in planes {
        for (i, s) in c.iter_mut().take(fade_in).enumerate() {
            *s *= i as f32 / fade_in as f32;
        }
        for (i, s) in c.iter_mut().rev().take(fade_out).enumerate() {
            *s *= i as f32 / fade_out as f32;
        }
    }
}

/// The RMS of the voiced frames of `channels`: those within
/// [`VOICED_DB`] of the loudest.
fn voiced_rms<C: AsRef<[f32]>>(channels: &[C], frame: usize) -> f64 {
    let powers = powers(channels, frame);
    let mut sum = 0.0f64;
    let mut count = 0usize;
    for (p, l) in powers.iter().zip(levels(&powers)) {
        if l > VOICED_DB {
            sum += p;
            count += 1;
        }
    }
    (sum / count.max(1) as f64).sqrt()
}

/// `planes` through the phase rotator: `chain.stages` first-order
/// all-pass filters at `chain.corner`, which change no frequency's level
/// and spread a pulse's harmonics in time (nothing before the pulse: the
/// filters look back only).
fn rotated(planes: Vec<Vec<f32>>, rate: u32, chain: &Chain) -> Vec<Vec<f32>> {
    if chain.stages == 0 {
        return planes;
    }
    // The corner's tangent taken as its angle (a corner far under the
    // rate's half): no library function.
    let k = std::f64::consts::PI * chain.corner / f64::from(rate);
    let a = (k - 1.0) / (k + 1.0);
    planes
        .into_iter()
        .map(|c| {
            let mut wave: Vec<f64> = c.iter().map(|s| f64::from(*s)).collect();
            for _ in 0..chain.stages {
                let (mut before, mut out) = (0.0f64, 0.0f64);
                for s in &mut wave {
                    let x = *s;
                    out = a * x + before - a * out;
                    before = x;
                    *s = out;
                }
            }
            wave.into_iter().map(|s| s as f32).collect()
        })
        .collect()
}

/// The compressor's gain for a power `over` the threshold's (as a
/// ratio): `(1 + over)^-(power / 2^roots)`.
fn slope(over: f64, chain: &Chain) -> f64 {
    let mut root = 1.0 + over;
    for _ in 0..chain.roots {
        root = root.sqrt();
    }
    let mut down = 1.0f64;
    for _ in 0..chain.power {
        down *= root;
    }
    1.0 / down
}

/// The compressor's gain for each frame of `planes`, whose voiced RMS is
/// `level`: the mean power of the window to come (every channel's
/// together), followed quickly up and slowly down, against the
/// threshold's ([`slope`]). It sees a syllable coming, so its gain is
/// down by the time the syllable starts, and it moves over a pitch
/// period or more: it turns a syllable down and leaves its wave as it
/// is. One gain for every channel.
fn compressor(planes: &[Vec<f32>], rate: u32, level: f64, chain: &Chain) -> Vec<f64> {
    let len = planes.first().map_or(0, Vec::len);
    let count = planes.len().max(1) as f64;
    // The power up to each frame, summed once in order.
    let mut sums = Vec::with_capacity(len + 1);
    let mut sum = 0.0f64;
    sums.push(sum);
    for n in 0..len {
        let mut power = 0.0f64;
        for c in planes {
            power += f64::from(c[n]) * f64::from(c[n]);
        }
        sum += power / count;
        sums.push(sum);
    }
    let window = ((chain.window * f64::from(rate)).round() as usize).max(1);
    let attack = 1.0 / (1.0 + chain.attack * f64::from(rate));
    let release = 1.0 / (1.0 + chain.release * f64::from(rate));
    let unit = (level * chain.threshold) * (level * chain.threshold);
    let mut followed = 0.0f64;
    (0..len)
        .map(|n| {
            let ahead = ((sums[len.min(n + window)] - sums[n]) / window as f64).max(0.0);
            let step = if ahead > followed { attack } else { release };
            followed += (ahead - followed) * step;
            slope(followed / unit.max(1e-24), chain)
        })
        .collect()
}

/// A sound as the limiter is given it, before the gain.
struct Ready {
    /// The sound cut, rotated and faded: what the gains are applied to.
    given: Vec<Vec<f32>>,
    planes: Vec<Vec<f32>>,
    /// Its true peaks ([`true_peaks`]).
    peaks: Vec<f64>,
    /// The compressor's largest reduction and its median over the voiced
    /// frames, as gains (1: nothing).
    compressed: (f64, f64),
}

/// The `part` kept of a sound, faded, as the limiter is given it: as it
/// is where it is `alone` (at the level already), else through the
/// rotator and the compressor, which is made up so that the voiced RMS
/// stays what it was.
fn ready(part: Vec<Vec<f32>>, rate: u32, alone: bool, chain: &Chain) -> Ready {
    let frame = (FRAME * f64::from(rate)).round().max(1.0) as usize;
    let mut planes = if alone {
        part
    } else {
        rotated(part, rate, chain)
    };
    fade(&mut planes, rate);
    let given = planes.clone();
    let level = voiced_rms(&planes, frame);
    let mut compressed = (1.0, 1.0);
    if !alone && level > 1e-9 {
        let gains = compressor(&planes, rate, level, chain);
        // What it did, over the voiced frames.
        let powers = powers(&planes, frame);
        let mut voiced: Vec<f64> = powers
            .iter()
            .zip(levels(&powers))
            .enumerate()
            .filter(|(_, (_, l))| *l > VOICED_DB)
            .map(|(i, _)| gains[i * frame + frame / 2])
            .collect();
        voiced.sort_by(f64::total_cmp);
        let least = gains.iter().copied().fold(1.0f64, f64::min);
        compressed = (least, voiced.get(voiced.len() / 2).copied().unwrap_or(1.0));
        let turned: Vec<Vec<f32>> = planes
            .iter()
            .map(|c| {
                c.iter()
                    .zip(&gains)
                    .map(|(s, g)| (f64::from(*s) * g) as f32)
                    .collect()
            })
            .collect();
        // Made up to the level it had.
        let up = level / voiced_rms(&turned, frame).max(1e-12);
        planes = turned
            .into_iter()
            .map(|c| c.into_iter().map(|s| (f64::from(s) * up) as f32).collect())
            .collect();
    }
    let peaks = true_peaks(&planes);
    Ready {
        given,
        planes,
        peaks,
        compressed,
    }
}

/// The limiter's curve: what an amplitude `u` (not negative) comes out
/// as. As it is under the knee, the ceiling from where the parabola
/// reaches it, the parabola between.
fn shaped(u: f64) -> f64 {
    let a = LIMIT * KNEE_START;
    let b = 2.0 * LIMIT - a;
    if u <= a {
        u
    } else if u >= b {
        LIMIT
    } else {
        u - (u - a) * (u - a) / (2.0 * (b - a))
    }
}

/// The wave between two samples, a quarter and a half of the way from
/// the eighth of sixteen samples to the ninth (a windowed sinc; three
/// quarters is the quarter's read backwards): what a player's or an
/// encoder's resampling makes of the samples, where the true peak is.
const QUARTER: [f64; 16] = [
    -0.000_485_277_351_803_045_1,
    0.002_281_606_008_485_074_4,
    -0.006_969_168_193_591_43,
    0.017_004_801_029_672_78,
    -0.036_405_808_994_262_67,
    0.073_993_896_835_671_37,
    -0.164_263_486_779_471_25,
    0.897_048_995_760_827_9,
    0.290_384_410_134_700_45,
    -0.107_319_586_469_774_13,
    0.051_934_270_296_060_216,
    -0.025_173_997_938_088_39,
    0.011_119_479_021_164_563,
    -0.004_134_832_482_144_66,
    0.001_137_861_182_167_084_6,
    -0.000_153_162_059_613_977_23,
];
const HALF: [f64; 16] = [
    -0.000_407_424_612_603_410_96,
    0.002_313_431_253_747_151_7,
    -0.007_650_904_232_903_501,
    0.019_536_803_083_894_275,
    -0.042_913_236_012_765_19,
    0.087_595_590_429_898_84,
    -0.185_853_036_668_395_03,
    0.627_378_776_759_126_9,
    0.627_378_776_759_126_9,
    -0.185_853_036_668_395_03,
    0.087_595_590_429_898_84,
    -0.042_913_236_012_765_19,
    0.019_536_803_083_894_275,
    -0.007_650_904_232_903_501,
    0.002_313_431_253_747_151_7,
    -0.000_407_424_612_603_410_96,
];

/// Each frame's true peak over the channels: the loudest of its sample
/// and of the wave a quarter, a half and three quarters of the way to
/// the samples on either side of it.
fn true_peaks<C: AsRef<[f32]>>(channels: &[C]) -> Vec<f64> {
    let len = channels.first().map_or(0, |c| c.as_ref().len());
    let mut peaks = vec![0.0f64; len];
    for c in channels {
        let c = c.as_ref();
        let at = |i: isize| {
            if i >= 0 && (i as usize) < c.len() {
                f64::from(c[i as usize])
            } else {
                0.0
            }
        };
        for n in 0..len {
            let mut between = 0.0f64;
            let (mut quarter, mut half, mut three) = (0.0f64, 0.0f64, 0.0f64);
            for j in 0..16 {
                let x = at(n as isize - 7 + j as isize);
                quarter += x * QUARTER[j];
                half += x * HALF[j];
                three += x * QUARTER[15 - j];
            }
            for v in [quarter, half, three] {
                between = between.max(v.abs());
            }
            // Between this sample and the next: both answer for it.
            let own = f64::from(c[n]).abs().max(between);
            peaks[n] = peaks[n].max(own);
            if n + 1 < len {
                peaks[n + 1] = peaks[n + 1].max(between);
            }
        }
    }
    peaks
}

/// A gain that follows what each frame `need`s: the least over the
/// frames from `hold` before each to `look` after it, let back up over
/// `back` and averaged over the look-ahead, so that it comes down
/// smoothly before a peak and is never over what its own frame needs.
fn followed(need: &[f64], rate: u32, look: f64, hold: f64, back: f64) -> Vec<f64> {
    let len = need.len();
    let look = ((look * f64::from(rate)).round() as usize).max(1);
    let hold = (hold * f64::from(rate)).round() as usize;
    let back = 1.0 / (1.0 + back * f64::from(rate));
    // Summed as it goes, in order, for the mean.
    let mut sums = Vec::with_capacity(len + 1);
    let mut sum = 0.0f64;
    sums.push(sum);
    let mut queue: std::collections::VecDeque<usize> = std::collections::VecDeque::new();
    let mut next = 0;
    let mut held = 1.0f64;
    for i in 0..len {
        while next < len.min(i + look) {
            while queue.back().is_some_and(|&b| need[b] >= need[next]) {
                queue.pop_back();
            }
            queue.push_back(next);
            next += 1;
        }
        while queue.front().is_some_and(|&f| f + hold < i) {
            queue.pop_front();
        }
        let least = queue.front().map_or(1.0, |&f| need[f]);
        held = least.min(held + (1.0 - held) * back);
        sum += held;
        sums.push(sum);
    }
    // The mean over the last `look` (ones before the sound).
    (0..len)
        .map(|i| {
            let from = (i + 1).saturating_sub(look);
            let before = (look - (i + 1 - from)) as f64;
            ((sums[i + 1] - sums[from] + before) / look as f64).min(need[i])
        })
        .collect()
}

/// The limiter's gain for each frame whose true peaks are `peaks`
/// ([`true_peaks`]) once multiplied by `gain`, and the least of the part
/// of it that shapes peaks (1: none was shaped). What brings a peak onto
/// the curve ([`shaped`]) is taken in two parts. Up to `chain.ride`, by
/// riding the level: seen `chain.look` ahead, held `chain.hold` after
/// and let back up over `chain.back`, so the gain stays down from one
/// pitch pulse to the next and the wave keeps its shape. What a peak
/// needs beyond that, of the peak alone: seen `chain.shape_look` ahead
/// and let back over `chain.shape_back`. One gain for every channel.
fn limiter(peaks: &[f64], rate: u32, gain: f64, chain: &Chain) -> (Vec<f64>, f64) {
    let len = peaks.len();
    // Nothing over what true peaks are kept under: the gain alone (so a
    // sound limited before is not shaped again).
    if peaks.iter().copied().fold(0.0f64, f64::max) * gain <= PEAK {
        return (vec![1.0; len], 1.0);
    }
    let need: Vec<f64> = peaks
        .iter()
        .map(|p| {
            let peak = p * gain;
            if peak > LIMIT * KNEE_START {
                shaped(peak) / peak
            } else {
                1.0
            }
        })
        .collect();
    let riding: Vec<f64> = need.iter().map(|n| n.max(chain.ride)).collect();
    let mut gains = followed(&riding, rate, chain.look, chain.hold, chain.back);
    let beyond: Vec<f64> = need.iter().zip(&riding).map(|(n, r)| n / r).collect();
    let mut least = 1.0f64;
    if beyond.iter().any(|b| *b < 1.0) {
        let shaping = followed(&beyond, rate, chain.shape_look, 0.0, chain.shape_back);
        for (g, s) in gains.iter_mut().zip(&shaping) {
            *g *= s;
            least = least.min(*s);
        }
    }
    (gains, least)
}

/// `planes` (their true peaks `peaks`) times `gain` under the limiter,
/// the limiter's largest reduction as a gain (1: it did nothing) and
/// that of its shaping of peaks.
fn limited(
    planes: &[Vec<f32>],
    peaks: &[f64],
    rate: u32,
    gain: f64,
    chain: &Chain,
) -> (Vec<Vec<f32>>, f64, f64) {
    let (gains, shaping) = limiter(peaks, rate, gain, chain);
    let least = gains.iter().copied().fold(1.0f64, f64::min);
    let out = planes
        .iter()
        .map(|c| {
            c.iter()
                .zip(&gains)
                .map(|(s, g)| (f64::from(*s) * gain * g) as f32)
                .collect()
        })
        .collect();
    (out, least, shaping)
}

/// The gain for the `part` kept of a sound, so that its voiced RMS comes
/// out at `target`. A sound within [`TIDY_DB`] of the target gets that
/// little, alone ([`at_level`]). Else it is the gain after the
/// compressor: the target over its voiced RMS where the limiter then has
/// nothing to do, else the gain between that and [`MAX_OVER`] times it
/// whose limited sound has that RMS, found by halving (the limiter takes
/// more the louder it is driven, so the RMS only rises with the gain).
fn level(part: &[Vec<f32>], rate: u32, target: f64, chain: &Chain) -> f32 {
    let frame = (FRAME * f64::from(rate)).round().max(1.0) as usize;
    let mut plain = part.to_vec();
    fade(&mut plain, rate);
    let rms = voiced_rms(&plain, frame);
    if rms <= 1e-9 {
        return 1.0;
    }
    // Near enough the target already (a tidied sound read back from its
    // encoded file): that little, whatever the limiter then takes of a
    // peak the encoder raised.
    let near = (target / rms) as f32;
    if at_level(near) {
        return near;
    }
    let ready = ready(part.to_vec(), rate, false, chain);
    let rms = voiced_rms(&ready.planes, frame);
    let after = |gain: f64| {
        voiced_rms(
            &limited(&ready.planes, &ready.peaks, rate, gain, chain).0,
            frame,
        )
    };
    let mut low = target / rms.max(1e-12);
    let gain = if after(low) >= target * 0.999 {
        low
    } else {
        let mut high = low * MAX_OVER;
        if after(high) <= target {
            high
        } else {
            for _ in 0..PASSES {
                let mid = (low * high).sqrt();
                if after(mid) < target {
                    low = mid;
                } else {
                    high = mid;
                }
            }
            (low * high).sqrt()
        }
    };
    // Never a gain that says "at the level already": the chain is asked
    // for by a gain outside that.
    let gain = gain as f32;
    if !at_level(gain) {
        gain
    } else if gain >= 1.0 {
        TIDY_UP.next_up()
    } else {
        TIDY_DOWN.next_down()
    }
}

/// What tidying would do to `channels` (one slice a channel, all as
/// long) at `rate` Hz.
pub fn measure<C: AsRef<[f32]>>(channels: &[C], rate: u32) -> Measure {
    measured(channels, rate, VOICED_RMS, &CHAIN)
}

/// [`measure`], leveled to `voiced_rms_db` in place of [`VOICED_RMS_DB`]
/// (an audition's: `kokoro.py loudness`).
pub fn measure_to<C: AsRef<[f32]>>(channels: &[C], rate: u32, voiced_rms_db: f64) -> Measure {
    measured(channels, rate, 10f64.powf(voiced_rms_db / 20.0), &CHAIN)
}

fn measured<C: AsRef<[f32]>>(channels: &[C], rate: u32, target: f64, chain: &Chain) -> Measure {
    let len = channels.first().map_or(0, |c| c.as_ref().len());
    let seconds = len as f64 / f64::from(rate.max(1));
    let mut m = Measure {
        rate,
        channels: channels.len() as u16,
        seconds,
        clipped: clipped(channels),
        start: 0.0,
        end: seconds,
        gain: 1.0,
    };
    let frame = (FRAME * f64::from(rate)).round().max(1.0) as usize;
    let all = levels(&powers(channels, frame));
    if all.is_empty() || all.iter().all(|l| *l <= -239.0) {
        return m;
    }
    // The noise: the level the quietest twentieth of the frames reach.
    let mut sorted = all.clone();
    sorted.sort_by(f64::total_cmp);
    let noise = sorted[sorted.len() / 20];
    let floor = FLOOR_DB.max((noise + OVER_NOISE_DB).min(HIGHEST_FLOOR_DB));
    let first = all.iter().position(|l| *l > floor).unwrap_or(0);
    let last = all
        .iter()
        .rposition(|l| *l > floor)
        .unwrap_or(all.len() - 1);
    let start = (first * frame).saturating_sub((HEAD * f64::from(rate)) as usize);
    let end = ((last + 1) * frame + (TAIL * f64::from(rate)) as usize).min(len);
    m.gain = level(&part(channels, start, end), rate, target, chain);
    m.start = start as f64 / f64::from(rate);
    m.end = end as f64 / f64::from(rate);
    m
}

/// `channels` tidied as `m` says, at `rate` Hz (the rate they were
/// measured at, or another: the cut and the chain are in seconds).
pub fn apply<C: AsRef<[f32]>>(channels: &[C], rate: u32, m: &Measure) -> Vec<Vec<f32>> {
    tidied(channels, rate, m).0
}

/// [`apply`], and what the chain did.
pub fn tidied<C: AsRef<[f32]>>(channels: &[C], rate: u32, m: &Measure) -> (Vec<Vec<f32>>, Did) {
    chained(channels, rate, m, &CHAIN)
}

fn chained<C: AsRef<[f32]>>(
    channels: &[C],
    rate: u32,
    m: &Measure,
    chain: &Chain,
) -> (Vec<Vec<f32>>, Did) {
    let at = |s: f64| (s * f64::from(rate)).round() as usize;
    let frame = (FRAME * f64::from(rate)).round().max(1.0) as usize;
    let ready = ready(
        part(channels, at(m.start), at(m.end)),
        rate,
        at_level(m.gain),
        chain,
    );
    let gain = f64::from(m.gain);
    let (out, least, shaping) = limited(&ready.planes, &ready.peaks, rate, gain, chain);
    let given = gain * voiced_rms(&ready.planes, frame);
    // As decibels taken (0, never -0, where nothing was).
    let taken = |gain: f64| 0.0 - db(gain);
    let took = if least < 1.0 && given > 1e-12 {
        taken(voiced_rms(&out, frame) / given)
    } else {
        0.0
    };
    // What of the result is not what was given times one gain (the one
    // that fits it best).
    let (mut xy, mut xx, mut yy) = (0.0f64, 0.0f64, 0.0f64);
    for (x, y) in ready.given.iter().zip(&out) {
        for (a, b) in x.iter().zip(y) {
            let (a, b) = (f64::from(*a), f64::from(*b));
            xy += a * b;
            xx += a * a;
            yy += b * b;
        }
    }
    let fit = xy / xx.max(1e-24);
    let mut error = 0.0f64;
    for (x, y) in ready.given.iter().zip(&out) {
        for (a, b) in x.iter().zip(y) {
            let e = f64::from(*b) - fit * f64::from(*a);
            error += e * e;
        }
    }
    let did = Did {
        changed_db: 10.0 * (error / yy.max(1e-24)).max(1e-30).log10(),
        compressed_db: taken(ready.compressed.0),
        compressed_median_db: taken(ready.compressed.1),
        limited_db: taken(least),
        peaks_db: taken(shaping),
        limited_level_db: took,
    };
    (out, did)
}

/// How far the loudest sample of `channels` is over their voiced RMS, in
/// dB (a sound's crest, as the leveling sees it).
pub fn crest_db<C: AsRef<[f32]>>(channels: &[C], rate: u32) -> f64 {
    let frame = (FRAME * f64::from(rate)).round().max(1.0) as usize;
    let peak = channels
        .iter()
        .flat_map(|c| c.as_ref().iter())
        .fold(0.0f32, |a, s| a.max(s.abs()));
    db(f64::from(peak) / voiced_rms(channels, frame).max(1e-12))
}

/// The voiced RMS of `channels`, dBFS.
pub fn voiced_rms_db<C: AsRef<[f32]>>(channels: &[C], rate: u32) -> f64 {
    let frame = (FRAME * f64::from(rate)).round().max(1.0) as usize;
    db(voiced_rms(channels, frame))
}

#[cfg(test)]
mod tests {
    use super::*;

    const RATE: u32 = 48_000;

    /// A deterministic noise in `-1..1`.
    fn noise(i: usize) -> f32 {
        let x = (i as u32).wrapping_mul(2_654_435_761) ^ 0x9E37_79B9;
        (x >> 8) as f32 / (1 << 23) as f32 - 1.0
    }

    /// `quiet` seconds of noise at `noise_db`, `voice` seconds of a tone at
    /// `amp` over it, then `quiet` seconds more.
    fn take(quiet: f64, voice: f64, amp: f32, noise_db: f64) -> Vec<f32> {
        let n = |s: f64| (s * f64::from(RATE)) as usize;
        let level = 10f32.powf(noise_db as f32 / 20.0);
        (0..n(quiet * 2.0 + voice))
            .map(|i| {
                let t = i as f64 / f64::from(RATE);
                let tone = if t >= quiet && t < quiet + voice {
                    amp * (t * 220.0 * std::f64::consts::TAU).sin() as f32
                } else {
                    0.0
                };
                tone + level * noise(i)
            })
            .collect()
    }

    fn voiced_rms_db(c: &[f32]) -> f64 {
        let frame = (FRAME * f64::from(RATE)) as usize;
        let p = powers(&[c], frame);
        let voiced: Vec<f64> = p
            .iter()
            .zip(levels(&p))
            .filter(|(_, l)| *l > VOICED_DB)
            .map(|(p, _)| *p)
            .collect();
        db((voiced.iter().sum::<f64>() / voiced.len() as f64).sqrt())
    }

    #[test]
    fn a_take_is_cut_to_its_voice_and_leveled() {
        for noise_db in [-90.0, -55.0, -40.0] {
            let t = take(1.0, 0.6, 0.1, noise_db);
            let m = measure(&[&t], RATE);
            assert!(!m.clipped);
            assert!(
                (m.start - (1.0 - HEAD)).abs() < 0.011,
                "{noise_db}: {}",
                m.start
            );
            assert!(
                (m.end - (1.6 + TAIL)).abs() < 0.011,
                "{noise_db}: {}",
                m.end
            );
            let out = apply(&[&t], RATE, &m);
            assert!((voiced_rms_db(&out[0]) - VOICED_RMS_DB).abs() < 0.5);
            assert_eq!(out[0][0], 0.0);
        }
    }

    /// A voice of pulses, as speech is: each period a narrow pulse over a
    /// quiet hum, `crest` dB between its tip and its RMS or so, 0.8 s
    /// between half-second silences.
    fn pulses(period: usize, width: usize, amp: f32) -> Vec<f32> {
        let n = |s: f64| (s * f64::from(RATE)) as usize;
        (0..n(1.8))
            .map(|i| {
                if i < n(0.5) || i >= n(1.3) {
                    return 0.0;
                }
                let k = i % period;
                let hum = 0.04 * amp * (i as f64 * 0.02).sin() as f32;
                if k < width {
                    let x = k as f32 / width as f32;
                    amp * (1.0 - (2.0 * x - 1.0) * (2.0 * x - 1.0)) + hum
                } else {
                    hum
                }
            })
            .collect()
    }

    fn peak_db(c: &[f32]) -> f64 {
        db(f64::from(c.iter().fold(0.0f32, |a, s| a.max(s.abs()))))
    }

    /// A voice whose peaks stand far over its level comes out at the
    /// level all the same, its peaks under the ceiling: no sample over
    /// it, the gain moving smoothly (no click), the ends still faded
    /// from nothing, and less asked of the limiter than the gain alone
    /// would ask.
    #[test]
    fn a_peaky_voice_reaches_the_level_under_the_limiter() {
        for (period, width) in [(480, 24), (400, 12), (300, 40)] {
            let t = pulses(period, width, 0.3);
            let before = crest_db(&[&t], RATE);
            assert!(before > 9.0, "{before}");
            let m = measure(&[&t], RATE);
            let (out, did) = tidied(&[&t], RATE, &m);
            let out = &out[0];
            assert!(
                (voiced_rms_db(out) - VOICED_RMS_DB).abs() < 0.05,
                "{period}/{width}: {}",
                voiced_rms_db(out)
            );
            assert!(peak_db(out) <= LIMIT_DB + 1e-6, "{}", peak_db(out));
            // Between the samples too, as nearly as a gain that moves
            // beside the wave allows.
            let between = true_peaks(&[out.as_slice()])
                .into_iter()
                .fold(0.0f64, f64::max);
            assert!(db(between) <= LIMIT_DB + 0.3, "{}", db(between));
            let after = crest_db(&[out.as_slice()], RATE);
            // The samples' crest: the ceiling over the level at the most.
            let least = LIMIT_DB - VOICED_RMS_DB;
            assert!(after < before - 1.0 && after <= least + 0.01, "{after}");
            assert_eq!((out[0], *out.last().unwrap()), (0.0, 0.0));
            assert!(did.compressed_db > 3.0, "{did:?}");
            // The gain between neighbors never jumps: a frame's share of
            // each look-ahead at the most.
            let at = |s: f64| (s * f64::from(RATE)).round() as usize;
            let ready = ready(part(&[&t], at(m.start), at(m.end)), RATE, false, &CHAIN);
            let (gains, _) = limiter(&ready.peaks, RATE, f64::from(m.gain), &CHAIN);
            let step = 1.0 / (CHAIN.look * f64::from(RATE)).round()
                + 1.0 / (CHAIN.shape_look * f64::from(RATE)).round();
            for w in gains.windows(2) {
                assert!((w[1] - w[0]).abs() <= step + 1e-9, "{w:?}");
            }
        }
    }

    /// The phase rotator changes no level and spreads a pulse: a tone
    /// comes out as loud as it went in, a voice of narrow pulses with
    /// its energy and lower peaks, nothing before the first of them, and
    /// as long.
    #[test]
    fn the_rotator_lowers_peaks_and_no_level() {
        let rms = |c: &[f32]| {
            (c.iter().map(|s| f64::from(*s) * f64::from(*s)).sum::<f64>() / c.len() as f64).sqrt()
        };
        for hz in [80.0, 200.0, 1000.0, 5000.0] {
            let tone: Vec<f32> = (0..RATE as usize)
                .map(|i| {
                    (0.5 * (i as f64 * hz * std::f64::consts::TAU / f64::from(RATE)).sin()) as f32
                })
                .collect();
            let out = rotated(vec![tone.clone()], RATE, &CHAIN);
            let half = tone.len() / 2;
            assert!(
                db(rms(&out[0][half..]) / rms(&tone[half..])).abs() < 0.01,
                "{hz}"
            );
        }
        // A voice's pulses, wide enough that its low harmonics carry it.
        let voice: Vec<f32> = (0..RATE as usize)
            .map(|i| {
                let k = (i % 400) as f32;
                if i >= 4_800 && k < 96.0 {
                    0.5 * (1.0 - (k / 48.0 - 1.0) * (k / 48.0 - 1.0))
                } else {
                    0.0
                }
            })
            .collect();
        let out = rotated(vec![voice.clone()], RATE, &CHAIN);
        assert_eq!(out[0].len(), voice.len());
        assert!(out[0][..4_800].iter().all(|s| *s == 0.0));
        assert!(db(rms(&out[0]) / rms(&voice)).abs() < 0.05);
        assert!(peak_db(&out[0]) < peak_db(&voice) - 2.0, "{}", peak_db(&out[0]));
        // None asked for: the sound as it is.
        let none = Chain { stages: 0, ..CHAIN };
        assert_eq!(rotated(vec![voice.clone()], RATE, &none), vec![voice]);
    }

    /// A line of a quiet syllable and a loud one, 12 dB apart.
    fn syllables(amp: f32) -> Vec<f32> {
        let n = |s: f64| (s * f64::from(RATE)) as usize;
        (0..n(1.6))
            .map(|i| {
                let level = if (n(0.3)..n(0.7)).contains(&i) {
                    0.25
                } else if (n(0.8)..n(1.2)).contains(&i) {
                    1.0
                } else {
                    0.0
                };
                amp * level * (i as f64 * 0.03).sin() as f32
            })
            .collect()
    }

    /// The compressor brings a line's syllables closer, by a gain that
    /// is down before a loud one starts, and alike for a quiet take and
    /// a loud one of the same line.
    #[test]
    fn the_compressor_evens_a_line_whatever_its_level() {
        let t = syllables(0.2);
        let m = measure(&[&t], RATE);
        let (out, did) = tidied(&[&t], RATE, &m);
        let n = |s: f64| ((s - m.start) * f64::from(RATE)) as usize;
        let rms = |c: &[f32]| {
            (c.iter().map(|s| f64::from(*s) * f64::from(*s)).sum::<f64>() / c.len() as f64).sqrt()
        };
        let apart = db(rms(&out[0][n(0.85)..n(1.15)]) / rms(&out[0][n(0.35)..n(0.65)]));
        assert!(apart < 12.0 - 3.0 && apart > 3.0, "{apart}");
        assert!(did.compressed_db > 3.0 && did.compressed_median_db > 0.5, "{did:?}");
        assert!((voiced_rms_db(&out[0]) - VOICED_RMS_DB).abs() < 0.05);
        // The loud syllable's first period is no louder than its middle.
        let first = out[0][n(0.8)..n(0.81)].iter().fold(0.0f32, |a, s| a.max(s.abs()));
        let middle = out[0][n(1.0)..n(1.01)].iter().fold(0.0f32, |a, s| a.max(s.abs()));
        assert!(db(f64::from(first / middle)) < 1.0, "{first} {middle}");
        // The same line recorded 20 dB quieter: the same sound.
        let quiet = syllables(0.02);
        let again = apply(&[&quiet], RATE, &measure(&[&quiet], RATE));
        assert_eq!(again[0].len(), out[0].len());
        for (a, b) in again[0].iter().zip(&out[0]) {
            assert!((a - b).abs() < 1e-4, "{a} {b}");
        }
        // Its slope: nothing under the threshold's power to speak of,
        // four to one far over it.
        assert_eq!(slope(0.0, &CHAIN), 1.0);
        assert!(-db(slope(0.01, &CHAIN)) < 0.05);
        let far = db(slope(1.0e6, &CHAIN)) - db(slope(1.0e4, &CHAIN));
        assert!((far + 20.0 * 0.75).abs() < 0.01, "{far}");
    }

    /// A tone under narrow pulses `amp` high, a hundred a second: a
    /// voice whose peaks the rotator leaves standing.
    fn pulsed(amp: f32) -> Vec<f32> {
        let n = |s: f64| (s * f64::from(RATE)) as usize;
        (0..n(1.4))
            .map(|i| {
                if i < n(0.3) || i >= n(1.1) {
                    return 0.0;
                }
                // Four periods of the tone to each pulse: every period
                // of the voice is the same.
                let k = i % 480;
                let tone = 0.1 * (k as f64 * std::f64::consts::TAU / 120.0).sin() as f32;
                if k < 12 {
                    tone + amp * (1.0 - (k as f32 / 6.0 - 1.0) * (k as f32 / 6.0 - 1.0))
                } else {
                    tone
                }
            })
            .collect()
    }

    /// A voice with a little over its level in its peaks: the limiter
    /// rides its level (down before the peaks, held from one pulse to
    /// the next) and shapes nothing. One with much over it: the limiter
    /// rides as far as it may and takes the rest of the peaks alone.
    #[test]
    fn the_limiter_rides_what_little_it_is_asked() {
        let n = |s: f64| (s * f64::from(RATE)) as usize;
        let t = pulsed(0.4);
        let m = measure(&[&t], RATE);
        let (out, did) = tidied(&[&t], RATE, &m);
        assert!(
            did.limited_db > 0.3 && did.limited_db < 3.0 && did.peaks_db == 0.0,
            "{did:?}"
        );
        assert!(did.limited_level_db > 0.0 && did.limited_level_db < did.limited_db);
        assert!((voiced_rms_db(&out[0]) - VOICED_RMS_DB).abs() < 0.05);
        assert!(peak_db(&out[0]) <= LIMIT_DB + 1e-6);
        let at = |s: f64| (s * f64::from(RATE)).round() as usize;
        let ready = ready(part(&[&t], at(m.start), at(m.end)), RATE, false, &CHAIN);
        let (gains, shaping) = limiter(&ready.peaks, RATE, f64::from(m.gain), &CHAIN);
        assert_eq!(shaping, 1.0);
        // From one pulse to the next, in the middle of the voice, the
        // gain hardly moves.
        let middle = &gains[n(0.6) - at(m.start)..n(0.8) - at(m.start)];
        let (low, high) = middle
            .iter()
            .fold((1.0f64, 0.0f64), |(l, h), g| (l.min(*g), h.max(*g)));
        assert!(low < 1.0 && db(high / low) < 0.25, "{low} {high}");

        let t = pulsed(0.7);
        let m = measure(&[&t], RATE);
        let (out, did) = tidied(&[&t], RATE, &m);
        assert!(did.limited_db > 4.0 && did.peaks_db > 1.0, "{did:?}");
        assert!(
            (did.limited_db - did.peaks_db + db(CHAIN.ride)).abs() < 1e-6,
            "{did:?}"
        );
        assert!((voiced_rms_db(&out[0]) - VOICED_RMS_DB).abs() < 0.05);
        assert!(peak_db(&out[0]) <= LIMIT_DB + 1e-6);
    }

    /// The curve: untouched under the knee, never over the ceiling, never
    /// turned up, rising all the way.
    #[test]
    fn the_limiter_s_curve_is_smooth_and_stops_at_the_ceiling() {
        let mut last = 0.0;
        for i in 0..4000 {
            let u = f64::from(i) / 1000.0;
            let y = shaped(u);
            assert!(y <= LIMIT && y <= u && y >= last, "{u}: {y}");
            if u <= LIMIT * KNEE_START {
                assert_eq!(y, u);
            }
            last = y;
        }
        assert_eq!(shaped(10.0), LIMIT);
        assert!((db(LIMIT) - LIMIT_DB).abs() < 1e-9);
        assert!((db(VOICED_RMS) - VOICED_RMS_DB).abs() < 1e-9);
        assert!((db(KNEE_START) + 1.5).abs() < 1e-9);
        assert!((db(PEAK) - PEAK_DB).abs() < 1e-9);
        assert!((db(MAX_OVER) - 24.0).abs() < 1e-9);
        assert!((db(CHAIN.threshold) + 6.0).abs() < 1e-9);
        assert!((db(CHAIN.ride) + 3.0).abs() < 1e-9);
        assert!((db(f64::from(TIDY_UP)) - TIDY_DB).abs() < 1e-6);
        assert!((db(f64::from(TIDY_DOWN)) + TIDY_DB).abs() < 1e-6);
        assert!(at_level(1.0) && at_level(TIDY_UP) && at_level(TIDY_DOWN));
        assert!(!at_level(TIDY_UP.next_up()) && !at_level(TIDY_DOWN.next_down()));
    }

    /// A click far louder than the voice no longer holds the level down:
    /// the limiter shapes it, and nothing comes out over the ceiling.
    #[test]
    fn a_click_is_shaped_and_peaks_stay_under_the_ceiling() {
        let mut t = take(0.2, 0.5, 0.05, -90.0);
        t[RATE as usize / 2] = 0.9;
        let m = measure(&[&t], RATE);
        let out = apply(&[&t], RATE, &m);
        assert!(peak_db(&out[0]) <= LIMIT_DB + 1e-6, "{}", peak_db(&out[0]));
        assert!((voiced_rms_db(&out[0]) - VOICED_RMS_DB).abs() < 0.5);
    }

    /// Tidied again, a tidied take stays as it is: its ends where they
    /// are, its gain nothing to speak of, nothing rotated or compressed
    /// a second time, whatever the chain took the first time; and the
    /// same samples give the same samples.
    #[test]
    fn tidying_twice_changes_nothing_more() {
        for t in [
            take(1.0, 0.6, 0.1, -55.0),
            pulses(480, 24, 0.3),
            pulses(300, 8, 0.9),
            syllables(0.2),
        ] {
            let m = measure(&[&t], RATE);
            assert!(!at_level(m.gain), "{m:?}");
            let (once, did) = tidied(&[&t], RATE, &m);
            assert!(did.compressed_db > 0.0, "{did:?}");
            assert_eq!(once, apply(&[&t], RATE, &measure(&[&t], RATE)));
            let again = measure(&once, RATE);
            assert!(again.already_tidy(), "{again:?}");
            let (twice, did) = tidied(&once, RATE, &again);
            assert_eq!((did.compressed_db, did.limited_db), (0.0, 0.0), "{did:?}");
            // The gain alone, on what the fades left.
            let i = once[0].len() / 2;
            assert_eq!(twice[0][i], (f64::from(once[0][i]) * f64::from(again.gain)) as f32);
            assert!(
                (voiced_rms_db(&twice[0]) - voiced_rms_db(&once[0])).abs() < 0.3,
                "{} then {}",
                voiced_rms_db(&once[0]),
                voiced_rms_db(&twice[0])
            );
        }
        // And a take that comes at the level, whatever its dynamics.
        let t = syllables(0.2);
        let frame = RATE as usize / 100;
        let up = (VOICED_RMS / voiced_rms(&[&t], frame)) as f32;
        let t: Vec<f32> = t.iter().map(|s| s * up).collect();
        let m = measure(&[&t], RATE);
        assert!(at_level(m.gain) && m.gain_db().abs() < 0.3, "{m:?}");
        let (_, did) = tidied(&[&t], RATE, &m);
        assert_eq!(did.compressed_db, 0.0);
        // The chain is never asked for by a gain that says "at the level".
        for level in [-13.0, -12.5, -11.5, -11.0] {
            let m = measure_to(&[&t], RATE, level);
            let (out, did) = tidied(&[&t], RATE, &m);
            assert_eq!(at_level(m.gain), did.compressed_db == 0.0, "{level}: {m:?}");
            assert!((voiced_rms_db(&out[0]) - level).abs() < 0.8, "{level}");
        }
    }

    /// An audition's level in place of the target's.
    #[test]
    fn another_level_may_be_asked_for() {
        let t = pulses(480, 24, 0.3);
        for level in [-18.0, -12.0, -9.0, -7.0] {
            let m = measure_to(&[&t], RATE, level);
            let out = apply(&[&t], RATE, &m);
            assert!((voiced_rms_db(&out[0]) - level).abs() < 0.05, "{level}");
            assert!(peak_db(&out[0]) <= LIMIT_DB + 1e-6);
        }
        assert_eq!(
            measure_to(&[&t], RATE, VOICED_RMS_DB).start,
            measure(&[&t], RATE).start
        );
    }

    #[test]
    fn stereo_is_cut_and_leveled_alike() {
        let left = take(0.5, 0.4, 0.2, -70.0);
        let right: Vec<f32> = left.iter().map(|s| s * 0.5).collect();
        let m = measure(&[&left, &right], RATE);
        assert_eq!(m.channels, 2);
        let out = apply(&[&left, &right], RATE, &m);
        assert_eq!(out.len(), 2);
        assert_eq!(out[0].len(), out[1].len());
        let i = out[0].len() / 2;
        assert!((out[1][i] - out[0][i] * 0.5).abs() < 1e-6);
    }

    #[test]
    fn clipping_and_silence_are_told() {
        let mut t = take(0.1, 0.3, 1.2, -90.0);
        for s in &mut t {
            *s = s.clamp(-1.0, 1.0);
        }
        assert!(measure(&[&t], RATE).clipped);
        let silent = vec![0.0f32; RATE as usize];
        let m = measure(&[&silent], RATE);
        assert_eq!((m.start, m.end, m.gain), (0.0, 1.0, 1.0));
        let empty: Vec<f32> = Vec::new();
        assert_eq!(measure(&[&empty], RATE).seconds, 0.0);
    }

    /// Measured at one rate, applied at another: the same cut in seconds.
    #[test]
    fn the_cut_is_in_seconds() {
        let t = take(0.5, 0.5, 0.1, -80.0);
        let m = measure(&[&t], RATE);
        let half: Vec<f32> = t.iter().step_by(2).copied().collect();
        let out = apply(&[&half], RATE / 2, &m);
        let want = (m.tidied_seconds() * f64::from(RATE / 2)).round() as usize;
        assert!(out[0].len().abs_diff(want) <= 1);
    }

    /// A float or 16-bit mono WAV's samples and rate (the study's).
    fn read_wav(bytes: &[u8]) -> (Vec<f32>, u32) {
        let rate = u32::from_le_bytes(bytes[24..28].try_into().unwrap());
        let float = bytes[20] == 3;
        let data = &bytes[44..];
        let samples = if float {
            data.as_chunks::<4>()
                .0
                .iter()
                .map(|b| f32::from_le_bytes(*b))
                .collect()
        } else {
            data.as_chunks::<2>()
                .0
                .iter()
                .map(|b| f32::from(i16::from_le_bytes(*b)) / 32768.0)
                .collect()
        };
        (samples, rate)
    }

    fn write_wav(samples: &[f32], rate: u32) -> Vec<u8> {
        let len = (samples.len() * 4) as u32;
        let mut out = Vec::new();
        out.extend_from_slice(b"RIFF");
        out.extend_from_slice(&(36 + len).to_le_bytes());
        out.extend_from_slice(b"WAVEfmt ");
        out.extend_from_slice(&16u32.to_le_bytes());
        out.extend_from_slice(&3u16.to_le_bytes());
        out.extend_from_slice(&1u16.to_le_bytes());
        out.extend_from_slice(&rate.to_le_bytes());
        out.extend_from_slice(&(rate * 4).to_le_bytes());
        out.extend_from_slice(&4u16.to_le_bytes());
        out.extend_from_slice(&32u16.to_le_bytes());
        out.extend_from_slice(b"data");
        out.extend_from_slice(&len.to_le_bytes());
        for s in samples {
            out.extend_from_slice(&s.to_le_bytes());
        }
        out
    }

    /// A chain written as `name:key=value,key=value` over [`CHAIN`]
    /// (levels in dB, times in milliseconds).
    fn chain_of(text: &str) -> (String, Chain) {
        let (name, rest) = text.split_once(':').unwrap_or((text, ""));
        let mut c = CHAIN;
        for pair in rest.split(',').filter(|p| !p.is_empty()) {
            let (key, value) = pair.split_once('=').expect("key=value");
            let v: f64 = value.parse().expect("a number");
            match key {
                "stages" => c.stages = v as usize,
                "corner" => c.corner = v,
                "threshold" => c.threshold = 10f64.powf(v / 20.0),
                "roots" => c.roots = v as u32,
                "power" => c.power = v as u32,
                "window" => c.window = v / 1000.0,
                "attack" => c.attack = v / 1000.0,
                "release" => c.release = v / 1000.0,
                "look" => c.look = v / 1000.0,
                "hold" => c.hold = v / 1000.0,
                "back" => c.back = v / 1000.0,
                "ride" => c.ride = 10f64.powf(-v / 20.0),
                "shape_look" => c.shape_look = v / 1000.0,
                "shape_back" => c.shape_back = v / 1000.0,
                _ => panic!("{key}?"),
            }
        }
        (name.to_string(), c)
    }

    fn median(values: &mut [f64]) -> f64 {
        values.sort_by(f64::total_cmp);
        values.get(values.len() / 2).copied().unwrap_or(f64::NAN)
    }

    /// The study the chain's settings were chosen by
    /// (`docs/plans/announcers.md`, "Smoother leveling"): every WAV of
    /// each folder under `DDI_TIDY_STUDY` (mono, float or 16-bit: a
    /// pack's speech as it was made) through each chain of
    /// `DDI_TIDY_CHAINS` (`name:key=value,…;…`, [`chain_of`]; the
    /// leveling's own where none is given) at each level of
    /// `DDI_TIDY_LEVELS` (dBFS, `-9` by default), with what the
    /// compressor and the limiter did over each folder; the results
    /// written under `DDI_TIDY_OUT` where it is set.
    /// `cargo test --release -p ddi-library --lib tidy::tests::study -- --ignored --nocapture`
    #[test]
    #[ignore = "a study over local sounds: DDI_TIDY_STUDY"]
    fn study() {
        let Some(root) = std::env::var_os("DDI_TIDY_STUDY") else {
            return;
        };
        let root = std::path::PathBuf::from(root);
        let out = std::env::var_os("DDI_TIDY_OUT").map(std::path::PathBuf::from);
        let chains: Vec<(String, Chain)> = std::env::var("DDI_TIDY_CHAINS")
            .unwrap_or_else(|_| "v3".into())
            .split(';')
            .filter(|c| !c.is_empty())
            .map(chain_of)
            .collect();
        let levels: Vec<f64> = std::env::var("DDI_TIDY_LEVELS")
            .unwrap_or_else(|_| "-9".into())
            .split(',')
            .map(|l| l.parse().expect("a level"))
            .collect();
        let mut packs: Vec<std::path::PathBuf> = std::fs::read_dir(&root)
            .unwrap()
            .map(|e| e.unwrap().path())
            .filter(|p| p.is_dir())
            .collect();
        packs.sort();
        println!(
            "{:<26} {:>5} {:<8} {:>4} | {:>7} {:>6} | {:>19} | {:>19} | {:>19} | {:>5} | {:>11}",
            "chain",
            "level",
            "pack",
            "n",
            "rms med",
            "min",
            "compressor med/most",
            "limiter med/p90/max",
            "shaping med/p90/max",
            "took",
            "crest in>out"
        );
        for pack in &packs {
            let mut files: Vec<std::path::PathBuf> = std::fs::read_dir(pack)
                .unwrap()
                .map(|e| e.unwrap().path())
                .filter(|p| p.extension().is_some_and(|e| e == "wav"))
                .collect();
            files.sort();
            let sounds: Vec<(Vec<f32>, u32)> = files
                .iter()
                .map(|f| read_wav(&std::fs::read(f).unwrap()))
                .collect();
            let name = pack.file_name().unwrap().to_string_lossy().into_owned();
            for (chain_name, chain) in &chains {
                for level in &levels {
                    let target = 10f64.powf(level / 20.0);
                    let (mut rms, mut c_med, mut c_most, mut l_most, mut s_most, mut took) = (
                        Vec::new(),
                        Vec::new(),
                        Vec::new(),
                        Vec::new(),
                        Vec::new(),
                        Vec::new(),
                    );
                    let (mut crest_in, mut crest_out) = (Vec::new(), Vec::new());
                    for ((samples, rate), file) in sounds.iter().zip(&files) {
                        let m = measured(&[samples], *rate, target, chain);
                        let (tidied, did) = chained(&[samples], *rate, &m, chain);
                        let at = |s: f64| (s * f64::from(*rate)).round() as usize;
                        crest_in.push(crest_db(&[&samples[at(m.start)..at(m.end)]], *rate));
                        crest_out.push(crest_db(&tidied, *rate));
                        rms.push(super::voiced_rms_db(&tidied, *rate));
                        c_med.push(did.compressed_median_db);
                        c_most.push(did.compressed_db);
                        l_most.push(did.limited_db);
                        s_most.push(did.peaks_db);
                        took.push(did.limited_level_db);
                        if let Some(out) = &out {
                            let dir = out.join(chain_name).join(format!("{level}")).join(&name);
                            std::fs::create_dir_all(&dir).unwrap();
                            std::fs::write(
                                dir.join(file.file_name().unwrap()),
                                write_wav(&tidied[0], *rate),
                            )
                            .unwrap();
                        }
                    }
                    let n = rms.len();
                    let p90 = |v: &[f64]| {
                        let mut s = v.to_vec();
                        s.sort_by(f64::total_cmp);
                        s[(n * 9 / 10).min(n - 1)]
                    };
                    let most = |v: &[f64]| v.iter().copied().fold(f64::MIN, f64::max);
                    let least = |v: &[f64]| v.iter().copied().fold(f64::MAX, f64::min);
                    println!(
                        "{:<26} {:>5} {:<8} {:>4} | {:>7.2} {:>6.2} | {:>6.2} {:>6.2} {:>5.2} | {:>6.2} {:>6.2} {:>5.2} | {:>6.2} {:>6.2} {:>5.2} | {:>5.2} | {:>5.2}>{:<5.2}",
                        chain_name,
                        level,
                        name,
                        n,
                        median(&mut rms.clone()),
                        least(&rms),
                        median(&mut c_med),
                        median(&mut c_most.clone()),
                        most(&c_most),
                        median(&mut l_most.clone()),
                        p90(&l_most),
                        most(&l_most),
                        median(&mut s_most.clone()),
                        p90(&s_most),
                        most(&s_most),
                        median(&mut took),
                        median(&mut crest_in),
                        median(&mut crest_out),
                    );
                }
            }
        }
    }
}
