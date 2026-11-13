//! Step down: an easier chart instead of going out (a multiplayer match's
//! rule "At zero life: Step down", `docs/plans/multiplayer-prototype-progress.md`,
//! "Step down"). PROVISIONAL, named [`STEP_DOWN_V0`].
//!
//! A stage's **ladder** is the charts of its song in the stage chart's
//! style that are easier than the stage's chart ([`ladder`]). A competitor
//! whose life reaches zero with a rung below is not out: from a boundary
//! far enough ahead to be read ([`boundary`]) their notes are the next
//! rung's; what was left of the old chart is taken away unjudged at once
//! (a breather until the boundary, nothing lost or gained in it); their
//! life is given back to a share and cannot drop below one point until
//! the boundary. On the lowest rung zero life is out as ever.
//!
//! The play itself changes chart ([`crate::Player::add_rung`]): one
//! player, one judge, its notes rebuilt at the step (the notes judged
//! kept at their places, the new chart's put in after them), so that
//! judging stays exact and deterministic from the inputs alone, at any
//! frame rate and from the play's log, and everything that reads a play
//! (its frames, its events, its results, a battle's feed) reads one play.

use ddi_chart::{Chart, Difficulty, Song, Tick, TICKS_PER_BEAT, TimingMap};
use serde::{Deserialize, Serialize};

/// The rule's values, by name.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct StepDownRule {
    pub name: &'static str,
    /// The life given back at the step, `0..=1` (DDI's 50 of 100).
    pub life: f32,
    /// The seconds past the reading horizon the boundary is at least.
    pub lead: f64,
    /// The beats a bar line is every (StepMania's measure; time
    /// signatures are not read).
    pub bar_beats: u32,
    /// The least life the breather leaves, `0..=1` (one point of DDI's
    /// 100).
    pub floor: f32,
}

/// The values chosen for the prototype (PROVISIONAL): half the life back
/// (the start's), the next bar line at least the reading horizon plus a
/// second ahead, life kept at one point or more until then.
pub const STEP_DOWN_V0: StepDownRule = StepDownRule {
    name: "step-down-v0",
    life: 0.5,
    lead: 1.0,
    bar_beats: 4,
    floor: 0.01,
};

/// One step down of a play ([`crate::Results::steps`]).
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct StepDown {
    /// When life reached zero (judged song seconds): the old chart's
    /// notes not judged by then were taken away.
    pub at: f64,
    /// The boundary: the new chart's notes from here on.
    pub boundary: f64,
    /// Its tick on the stage's timing.
    pub tick: Tick,
    /// The rung stepped to, from 1 (0 is the stage's chart).
    pub rung: u8,
    /// The song's chart that rung is.
    pub chart: usize,
}

/// A difficulty slot's place, easiest first; an Edit has none.
fn slot(d: Difficulty) -> Option<u8> {
    match d {
        Difficulty::Beginner => Some(0),
        Difficulty::Easy => Some(1),
        Difficulty::Medium => Some(2),
        Difficulty::Hard => Some(3),
        Difficulty::Challenge => Some(4),
        Difficulty::Edit => None,
    }
}

/// Whether two timings place every note alike (the segments that move a
/// note in time or in the field; labels and combos aside).
pub fn same_timing(a: &TimingMap, b: &TimingMap) -> bool {
    a.offset_seconds == b.offset_seconds
        && a.bpms == b.bpms
        && a.stops == b.stops
        && a.delays == b.delays
        && a.warps == b.warps
        && a.speeds == b.speeds
        && a.scrolls == b.scrolls
        && a.fakes == b.fakes
}

/// The ladder of a stage on chart `chart` of `song`: the song's other
/// charts that can be its rungs, the next easier first. A rung is an
/// original chart (no `derivation`), in the stage chart's style (its
/// layout), with notes, on the same timing (the boundary and the
/// breather are measured on one), and easier:
///
/// - below a slotted chart (Beginner to Challenge), the charts of lower
///   slots, by slot from the nearest, a slot's charts by meter from the
///   highest, then by their place in the song; Edits are never rungs of
///   a slotted chart (what they are is not said);
/// - below an Edit, the slotted charts of a lower meter, by slot from
///   the highest, then by meter from the highest, then by place.
///
/// Empty where the song has no easier chart: the rule then changes
/// nothing.
pub fn ladder(song: &Song, chart: usize) -> Vec<usize> {
    let Some(stage) = song.charts.get(chart) else {
        return Vec::new();
    };
    let timing = stage.timing(song);
    let fits = |i: usize, c: &Chart| {
        i != chart
            && c.derivation.is_none()
            && c.layout == stage.layout
            && !c.notes.is_empty()
            && same_timing(c.timing(song), timing)
    };
    let mut rungs: Vec<(u8, u32, usize)> = song
        .charts
        .iter()
        .enumerate()
        .filter(|(i, c)| fits(*i, c))
        .filter_map(|(i, c)| {
            let s = slot(c.difficulty)?;
            let easier = match slot(stage.difficulty) {
                Some(top) => s < top,
                None => c.meter < stage.meter,
            };
            easier.then_some((s, c.meter, i))
        })
        .collect();
    rungs.sort_by(|a, b| b.0.cmp(&a.0).then(b.1.cmp(&a.1)).then(a.2.cmp(&b.2)));
    rungs.into_iter().map(|r| r.2).collect()
}

/// The boundary of a step at judged song second `at` for a player who
/// reads `horizon` seconds ahead: the first bar line at least `horizon`
/// plus the rule's lead past `at`, on `timing`: its tick and its second.
pub fn boundary(rule: &StepDownRule, timing: &TimingMap, at: f64, horizon: f64) -> (Tick, f64) {
    let target = at + horizon.max(0.0) + rule.lead;
    let bar = i64::from(rule.bar_beats.max(1)) * TICKS_PER_BEAT;
    let beat = timing.beat_at(target);
    let mut tick = if beat.is_finite() {
        Tick(((beat * TICKS_PER_BEAT as f64).ceil() as i64 + bar - 1).div_euclid(bar) * bar)
    } else {
        Tick(0)
    };
    // Up to the bar line at or past the target (a stop or a warp can put
    // the second of a tick before it).
    let mut guard = 0;
    while timing.seconds_at(tick) < target && guard < 10_000 {
        tick = Tick(tick.0 + bar);
        guard += 1;
    }
    (tick, timing.seconds_at(tick))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_boundary_is_a_bar_line_past_the_horizon_and_a_second() {
        // 120 BPM: a beat is half a second, a bar two seconds.
        let timing = TimingMap::constant(120.0, 0.0);
        let (tick, at) = boundary(&STEP_DOWN_V0, &timing, 10.0, 2.5);
        // 13.5 s is beat 27: the next bar line is beat 28, 14 s.
        assert_eq!(tick, Tick(28 * TICKS_PER_BEAT));
        assert!((at - 14.0).abs() < 1e-9);
        // Exactly on a bar line: that one.
        let (_, at) = boundary(&STEP_DOWN_V0, &timing, 11.0, 0.0);
        assert!((at - 12.0).abs() < 1e-9);
    }
}
