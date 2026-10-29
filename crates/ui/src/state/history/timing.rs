//! What the kept plays say of the timing on the devices in use
//! (`docs/plans/quiet-calibration.md`): whether the hits run early or
//! late on them as a rule (a calibration would help), are noise around
//! zero (the offsets are right as they are), or say nothing yet. The
//! start prompt's quiet suggestion to calibrate is kept or dropped by
//! it; nothing is ever applied from it.

use super::{Ending, History, PlaySummary};
use crate::state::settings::same_audio_device;

/// A play counts with at least this many judged hits.
pub const MIN_HITS_PER_PLAY: u32 = 50;
/// The last plays looked at, at most.
pub const WINDOW: usize = 10;
/// A pooled mean at least this far from zero, ms, is a shift.
pub const SHIFT_MS: f64 = 15.0;
/// A pooled mean within this of zero, ms, over [`FINE_HITS`] hits, is
/// noise: the offsets are fine.
pub const FINE_MS: f64 = 8.0;
pub const FINE_HITS: u32 = 300;

/// The verdict ([`timing_verdict`]).
#[derive(Clone, Debug, PartialEq)]
pub enum Verdict {
    /// The hits run `ms` late (early when negative) as a rule, across
    /// `plays` plays of `hits` hits: a calibration would help.
    Shift { ms: f64, plays: usize, hits: u32 },
    /// Noise around zero over enough hits: the offsets are right.
    Fine { plays: usize, hits: u32 },
    /// Too few hits, a wide spread or mixed signs: nothing to say.
    Inconclusive { plays: usize, hits: u32 },
}

/// The verdict for the audio output `audio` and the display `display`
/// under the audio offset `audio_offset` (seconds) in effect now, from
/// the kept plays on them: the player's own, finished (not quit, not
/// failed, not assisted), with at least [`MIN_HITS_PER_PLAY`] hits, the
/// last [`WINDOW`] of them. A play made under another audio offset is
/// read as it would have been under this one (its deltas move by the
/// difference). The pooled mean is hit-weighted; its standard error from
/// each play's spread. A shift needs the mean at least [`SHIFT_MS`] from
/// zero and three standard errors from it, with every play on the same
/// side (one play counts only with [`FINE_HITS`] hits); fine needs the
/// mean within [`FINE_MS`] of zero over [`FINE_HITS`] hits.
pub fn timing_verdict(history: &History, audio: &str, display: &str, audio_offset: f64) -> Verdict {
    let mut plays: Vec<(&PlaySummary, f64, u32, f64)> = history
        .iter()
        .filter_map(|(_, s)| {
            let clock = s.clock.as_ref()?;
            let w = s.worked.as_ref()?;
            let counted = s.counts()
                && s.ended == Ending::Finished
                && !w.assist
                && w.hits >= MIN_HITS_PER_PLAY
                && clock.display == display
                && same_audio_device(&clock.audio, audio);
            // A larger audio offset moves every hit earlier on the judged
            // timeline: what was measured under the play's offset reads
            // lower by the raise since.
            let mean = w.mean - (audio_offset - clock.audio_offset);
            counted.then_some((s, mean, w.hits, w.stddev))
        })
        .collect();
    plays.sort_by(|a, b| b.0.time.total_cmp(&a.0.time));
    plays.truncate(WINDOW);
    let count = plays.len();
    let hits: u32 = plays.iter().map(|p| p.2).sum();
    if count == 0 {
        return Verdict::Inconclusive { plays: 0, hits: 0 };
    }
    let weight = f64::from(hits);
    let mean: f64 = plays.iter().map(|p| p.1 * f64::from(p.2)).sum::<f64>() / weight;
    let variance: f64 = plays
        .iter()
        .map(|p| f64::from(p.2) * p.3 * p.3)
        .sum::<f64>()
        / (weight * weight);
    let se = variance.sqrt();
    let ms = mean * 1000.0;
    let same_side = plays.iter().all(|p| p.1.signum() == mean.signum());
    let enough_for_one = count >= 2 || hits >= FINE_HITS;
    if ms.abs() >= SHIFT_MS && mean.abs() >= 3.0 * se && same_side && enough_for_one {
        return Verdict::Shift {
            ms,
            plays: count,
            hits,
        };
    }
    if ms.abs() < FINE_MS && hits >= FINE_HITS {
        return Verdict::Fine { plays: count, hits };
    }
    Verdict::Inconclusive { plays: count, hits }
}
