//! A recording tidied (`docs/plans/announcer-studio.md`, decision 5), and
//! our packs' sounds with it: `tools/announcer-design/kokoro.py` hands
//! each line's speech to this code (`xtask announcer tidy`), so there is
//! one leveler (`docs/plans/announcers.md`, "Louder packs"). The silence
//! at both ends is cut, with short fades where it was cut, and the level
//! is evened ([`LEVELING`]): the voiced RMS of what comes out at
//! [`VOICED_RMS_DB`], about the loudness of a song's music, under a
//! soft-knee look-ahead limiter that keeps the wave, between its samples
//! too, under [`LIMIT_DB`] (true peaks under -1 dBFS once encoded). The
//! game times `here we go` and the cheer
//! from a sound's first sample, so a line recorded after a second of
//! silence would start a second late.
//!
//! The floor the ends are cut at rises over a recording's noise. Every
//! channel is cut and leveled alike, under one gain; a take keeps its
//! channels.
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
/// limiter below.
pub const LEVELING: &str = "announcer-level-v2";
/// The level a tidied sound's voiced frames come out at, dBFS RMS: about
/// -10.5 LUFS, a song's music's loudness (v1, to 2026-11-12: -18 with no
/// limiter, about -19.5 LUFS).
pub const VOICED_RMS_DB: f64 = -9.0;
/// [`VOICED_RMS_DB`] as an amplitude (written out: no library's rounding).
const VOICED_RMS: f64 = 0.354_813_389_233_575_47;
/// True peaks are kept under this, dBFS: a sound whose loudest moment,
/// between its samples too ([`true_peaks`]), is under it is left to the
/// gain alone, and one over it goes through the limiter.
pub const PEAK_DB: f64 = -1.0;
/// [`PEAK_DB`] as an amplitude.
const PEAK: f64 = 0.891_250_938_133_745_6;
/// The limiter's ceiling, dBFS: of what goes through it, neither a sample
/// nor the wave between two comes out over it. Half a decibel under
/// [`PEAK_DB`], for an encoder's overshoot (our packs' MP3s measure
/// -1.5 dBFS at the most), and so that a limited sound read back from
/// its encoded file is left alone ([`Measure::already_tidy`]).
pub const LIMIT_DB: f64 = -1.5;
/// [`LIMIT_DB`] as an amplitude.
const LIMIT: f64 = 0.841_395_141_645_195_1;
/// The limiter's knee: from this share of the ceiling (1.5 dB under it)
/// a sample is turned down smoothly, by a parabola whose slope reaches
/// nothing at the ceiling.
const KNEE_START: f64 = 0.841_395_141_645_195_1;
/// How far ahead the limiter sees a peak coming, and over how long its
/// gain comes back after one, seconds: short, so that it shapes a peak
/// and not the syllable around it (nothing to pump), and long enough
/// that the gain moves slowly beside the wave between two samples.
const LOOK: f64 = 0.000_5;
const RELEASE: f64 = 0.001_5;
/// The most the leveling raises a sound over what would bring its voiced
/// RMS to the target with no limiter (a sound of clicks and little else
/// is not crushed forever): 24 dB, as a gain.
const MAX_OVER: f64 = 15.848_931_924_611_133;
/// The halvings the gain is found in (to a 4096th of the range).
const PASSES: usize = 12;
/// How far from where tidying would put them a take's ends and level may
/// be and it count as tidy already: what a tidied, encoded file measures
/// when read back (the most of our packs' 330 MP3s: nothing to cut and
/// 0.49 dB; of the same sounds as Opus at 32 kb/s, 0.71 dB).
const TIDY_START: f64 = 0.02;
const TIDY_END: f64 = 0.04;
const TIDY_DB: f64 = 0.75;
/// A sample this close to full scale, three in a row: it clipped.
const FULL_SCALE: f32 = 0.999;
const CLIP_RUN: usize = 3;

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
    /// The gain tidying applies after the cut, before the limiter.
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
        self.start <= TIDY_START
            && self.seconds - self.end <= TIDY_END
            && self.gain_db().abs() <= TIDY_DB
    }
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

/// The samples `start..end` of each channel, faded in and out.
fn cut<C: AsRef<[f32]>>(channels: &[C], rate: u32, start: usize, end: usize) -> Vec<Vec<f32>> {
    let fade_in = (FADE_IN * f64::from(rate)) as usize;
    let fade_out = (FADE_OUT * f64::from(rate)) as usize;
    channels
        .iter()
        .map(|c| {
            let c = c.as_ref();
            let mut out = c[start.min(c.len())..end.min(c.len())].to_vec();
            for (i, s) in out.iter_mut().take(fade_in).enumerate() {
                *s *= i as f32 / fade_in as f32;
            }
            for (i, s) in out.iter_mut().rev().take(fade_out).enumerate() {
                *s *= i as f32 / fade_out as f32;
            }
            out
        })
        .collect()
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

/// The limiter's gain for each frame whose true peaks are `peaks`
/// ([`true_peaks`]) once multiplied by `gain`: what brings the peak onto
/// the curve ([`shaped`]), seen [`LOOK`] ahead (the least over the
/// samples to come), let back up over [`RELEASE`] and averaged over the
/// look-ahead, so that it moves smoothly and is never over what its own
/// sample needs. One gain for every channel.
fn limiter(peaks: &[f64], rate: u32, gain: f64) -> Vec<f64> {
    let len = peaks.len();
    // Nothing over what true peaks are kept under: the gain alone (so a
    // sound limited before is not shaped again).
    if peaks.iter().copied().fold(0.0f64, f64::max) * gain <= PEAK {
        return vec![1.0; len];
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
    let look = ((LOOK * f64::from(rate)).round() as usize).max(1);
    let back = (1.0 / (RELEASE * f64::from(rate))).min(1.0);
    // The least of the `look` samples from each on, then the release.
    let mut ahead = vec![1.0f64; len];
    let mut queue: std::collections::VecDeque<usize> = std::collections::VecDeque::new();
    let mut next = 0;
    let mut held = 1.0f64;
    for (i, out) in ahead.iter_mut().enumerate() {
        while next < len.min(i + look) {
            while queue.back().is_some_and(|&b| need[b] >= need[next]) {
                queue.pop_back();
            }
            queue.push_back(next);
            next += 1;
        }
        while queue.front().is_some_and(|&f| f < i) {
            queue.pop_front();
        }
        let least = queue.front().map_or(1.0, |&f| need[f]);
        held = least.min(held + (1.0 - held) * back);
        *out = held;
    }
    // The mean over the last `look`, summed afresh for each sample (no
    // running sum to drift).
    (0..len)
        .map(|i| {
            let mut sum = 0.0f64;
            for k in 0..look {
                sum += if i >= k { ahead[i - k] } else { 1.0 };
            }
            (sum / look as f64).min(need[i])
        })
        .collect()
}

/// `channels` (their true peaks `peaks`) times `gain` under the limiter,
/// and the limiter's largest reduction as a gain (1: it did nothing).
fn limited<C: AsRef<[f32]>>(
    channels: &[C],
    peaks: &[f64],
    rate: u32,
    gain: f64,
) -> (Vec<Vec<f32>>, f64) {
    let gains = limiter(peaks, rate, gain);
    let least = gains.iter().copied().fold(1.0f64, f64::min);
    let out = channels
        .iter()
        .map(|c| {
            c.as_ref()
                .iter()
                .zip(&gains)
                .map(|(s, g)| (f64::from(*s) * gain * g) as f32)
                .collect()
        })
        .collect();
    (out, least)
}

/// The gain that brings the voiced RMS of `kept` (cut and faded) to
/// `target` once limited: the target over its RMS where the limiter then
/// has nothing to do, else the gain between that and [`MAX_OVER`] times
/// it whose limited sound has that RMS, found by halving (the limiter
/// takes more the louder it is driven, so the RMS only rises with the
/// gain). A sound within [`TIDY_DB`] of the target gets that little.
fn level<C: AsRef<[f32]>>(kept: &[C], rate: u32, target: f64) -> f64 {
    let frame = (FRAME * f64::from(rate)).round().max(1.0) as usize;
    let rms = voiced_rms(kept, frame);
    if rms <= 1e-9 {
        return 1.0;
    }
    let peaks = true_peaks(kept);
    let after = |gain: f64| voiced_rms(&limited(kept, &peaks, rate, gain).0, frame);
    let mut low = target / rms;
    // Near enough the target already (a tidied sound read back from its
    // encoded file): that little, whatever the limiter then takes of a
    // peak the encoder raised.
    if db(low).abs() <= TIDY_DB || after(low) >= target * 0.999 {
        return low;
    }
    let mut high = low * MAX_OVER;
    if after(high) <= target {
        return high;
    }
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

/// What tidying would do to `channels` (one slice a channel, all as
/// long) at `rate` Hz.
pub fn measure<C: AsRef<[f32]>>(channels: &[C], rate: u32) -> Measure {
    measured(channels, rate, VOICED_RMS)
}

/// [`measure`], leveled to `voiced_rms_db` in place of [`VOICED_RMS_DB`]
/// (an audition's: `kokoro.py loudness`).
pub fn measure_to<C: AsRef<[f32]>>(channels: &[C], rate: u32, voiced_rms_db: f64) -> Measure {
    measured(channels, rate, 10f64.powf(voiced_rms_db / 20.0))
}

fn measured<C: AsRef<[f32]>>(channels: &[C], rate: u32, target: f64) -> Measure {
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
    let kept = cut(channels, rate, start, end);
    m.gain = level(&kept, rate, target) as f32;
    m.start = start as f64 / f64::from(rate);
    m.end = end as f64 / f64::from(rate);
    m
}

/// `channels` tidied as `m` says, at `rate` Hz (the rate they were
/// measured at, or another: the cut and the limiter are in seconds).
pub fn apply<C: AsRef<[f32]>>(channels: &[C], rate: u32, m: &Measure) -> Vec<Vec<f32>> {
    tidied(channels, rate, m).0
}

/// [`apply`], and what the limiter took at the most, in dB (0: nothing).
pub fn tidied<C: AsRef<[f32]>>(channels: &[C], rate: u32, m: &Measure) -> (Vec<Vec<f32>>, f64) {
    let at = |s: f64| (s * f64::from(rate)).round() as usize;
    let kept = cut(channels, rate, at(m.start), at(m.end));
    let (out, least) = limited(&kept, &true_peaks(&kept), rate, f64::from(m.gain));
    (out, -db(least))
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
    /// target all the same, its peaks shaped under the ceiling: no sample
    /// over it, the gain moving smoothly (no click), the ends still
    /// faded from nothing.
    #[test]
    fn a_peaky_voice_reaches_the_target_under_the_limiter() {
        for (period, width) in [(480, 24), (400, 12), (300, 40)] {
            let t = pulses(period, width, 0.3);
            let before = crest_db(&[&t], RATE);
            assert!(before > 9.0, "{before}");
            let m = measure(&[&t], RATE);
            let (out, took) = tidied(&[&t], RATE, &m);
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
            assert!(took > 1.0, "the limiter worked: {took}");
            let after = crest_db(&[out.as_slice()], RATE);
            // The samples' crest: the ceiling over the target at the
            // most, less what the wave between samples needs.
            let least = LIMIT_DB - VOICED_RMS_DB;
            assert!(after < before - 1.0 && after <= least + 0.01 && after > least - 3.0);
            assert_eq!((out[0], *out.last().unwrap()), (0.0, 0.0));
            // The gain between neighbors never jumps: a sample's own
            // need at most.
            let at = |s: f64| (s * f64::from(RATE)).round() as usize;
            let kept = cut(&[&t], RATE, at(m.start), at(m.end));
            let gains = limiter(&true_peaks(&kept), RATE, f64::from(m.gain));
            let look = (LOOK * f64::from(RATE)).round();
            for w in gains.windows(2) {
                assert!((w[1] - w[0]).abs() <= 1.0 / look + 1e-9, "{w:?}");
            }
        }
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
    /// are, its gain nothing to speak of, whatever the limiter took the
    /// first time; and the same samples give the same samples.
    #[test]
    fn tidying_twice_changes_nothing_more() {
        for t in [
            take(1.0, 0.6, 0.1, -55.0),
            pulses(480, 24, 0.3),
            pulses(300, 8, 0.9),
        ] {
            let m = measure(&[&t], RATE);
            let once = apply(&[&t], RATE, &m);
            assert_eq!(once, apply(&[&t], RATE, &measure(&[&t], RATE)));
            let again = measure(&once, RATE);
            assert!(again.already_tidy(), "{again:?}");
            let twice = apply(&once, RATE, &again);
            assert!(
                (voiced_rms_db(&twice[0]) - voiced_rms_db(&once[0])).abs() < 0.3,
                "{} then {}",
                voiced_rms_db(&once[0]),
                voiced_rms_db(&twice[0])
            );
        }
    }

    /// An audition's level in place of the target's.
    #[test]
    fn another_level_may_be_asked_for() {
        let t = pulses(480, 24, 0.3);
        for level in [-18.0, -12.0, -7.0] {
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
}
