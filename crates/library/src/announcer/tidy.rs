//! A recording tidied as our packs' sounds are made
//! (`tools/announcer-design/kokoro.py`'s `trim` and `finish`;
//! `docs/plans/announcer-studio.md`, decision 5): the silence at both
//! ends cut, short fades where it was cut, and the level evened to a
//! voiced RMS of -18 dBFS with peaks under -1 dBFS. The game times `here
//! we go` and the cheer from a sound's first sample, so a line recorded
//! after a second of silence would start a second late.
//!
//! Recordings are not synthesis: the floor the ends are cut at rises
//! over a recording's noise, and the cut keeps a little more on each
//! side than kokoro.py does. Every channel is cut and leveled alike; a
//! take keeps its channels.

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
/// The level our packs are made at.
pub const VOICED_RMS_DB: f64 = -18.0;
const PEAK_DB: f64 = -1.0;
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
    /// The gain tidying applies after the cut.
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

/// What tidying would do to `channels` (one slice a channel, all as
/// long) at `rate` Hz.
pub fn measure<C: AsRef<[f32]>>(channels: &[C], rate: u32) -> Measure {
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
    // The level, over the voiced frames of what is kept.
    let kept_powers = powers(&kept, frame);
    let voiced: Vec<f64> = kept_powers
        .iter()
        .zip(levels(&kept_powers))
        .filter(|(_, l)| *l > VOICED_DB)
        .map(|(p, _)| *p)
        .collect();
    let rms = (voiced.iter().sum::<f64>() / voiced.len().max(1) as f64).sqrt();
    let peak = kept
        .iter()
        .flat_map(|c| c.iter())
        .fold(0.0f32, |a, s| a.max(s.abs()));
    let to = |db: f64| 10f64.powf(db / 20.0);
    let gain = (to(VOICED_RMS_DB) / rms.max(1e-9)).min(to(PEAK_DB) / f64::from(peak).max(1e-9));
    m.start = start as f64 / f64::from(rate);
    m.end = end as f64 / f64::from(rate);
    m.gain = gain as f32;
    m
}

/// `channels` tidied as `m` says, at `rate` Hz (the rate they were
/// measured at, or another: the cut is in seconds).
pub fn apply<C: AsRef<[f32]>>(channels: &[C], rate: u32, m: &Measure) -> Vec<Vec<f32>> {
    let at = |s: f64| (s * f64::from(rate)).round() as usize;
    let mut out = cut(channels, rate, at(m.start), at(m.end));
    for c in &mut out {
        for s in c.iter_mut() {
            *s *= m.gain;
        }
    }
    out
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

    #[test]
    fn peaks_stay_under_a_decibel_below_full_scale() {
        // A click far louder than the voice holds the gain down.
        let mut t = take(0.2, 0.5, 0.05, -90.0);
        t[RATE as usize / 2] = 0.9;
        let m = measure(&[&t], RATE);
        let out = apply(&[&t], RATE, &m);
        let peak = out[0].iter().fold(0.0f32, |a, s| a.max(s.abs()));
        assert!(db(f64::from(peak)) <= PEAK_DB + 0.01, "{peak}");
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
