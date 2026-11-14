//! Adaptive charts: stepping down and up a song's ladder (a multiplayer
//! match's rule "Charts: Adaptive", `docs/plans/multiplayer-prototype-progress.md`,
//! "Adaptive charts"). PROVISIONAL, named: [`ADAPTIVE_V1`] (what is
//! played, [`ADAPTIVE`]), [`ADAPTIVE_V0`] kept for comparison.
//!
//! A stage's **ladder** is the other charts of its song in the stage
//! chart's style, easier ([`below`]) and harder ([`above`]); everyone
//! starts on the stage's chart. A competitor whose life reaches zero
//! with a rung below is not out: from a boundary far enough ahead to be
//! read ([`boundary`]) their notes are the next easier rung's; what was
//! left of the old chart is taken away unjudged at once (a breather
//! until the boundary, nothing lost or gained in it); their life is
//! given back to a share and cannot drop below one point until the
//! boundary. On the lowest rung zero life is out as ever. A competitor
//! who held full life for [`AdaptiveRule::up_bars`] bars with no miss and
//! no shock stepped on moves to the next harder rung from the next
//! boundary, the old chart played up to it, life unchanged
//! ([`AdaptiveRule::return_bars`] to go back up to a rung whose life ran
//! out, and never again to one whose life ran out
//! [`AdaptiveRule::closed_after`] times: the rung is closed to them for
//! the rest of the stage).
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
pub struct AdaptiveRule {
    pub name: &'static str,
    /// The life given back at a step down, `0..=1` (DDI's 50 of 100).
    pub life: f32,
    /// The seconds past the reading horizon a boundary is at least.
    pub lead: f64,
    /// The beats a bar line is every (StepMania's measure; time
    /// signatures are not read).
    pub bar_beats: u32,
    /// The least life a breather leaves, `0..=1` (one point of DDI's
    /// 100).
    pub floor: f32,
    /// The bars of full life, with no miss and no shock stepped on, that
    /// earn a step up (counted from the last step's boundary); 0 never
    /// steps up.
    pub up_bars: u32,
    /// The bars it takes instead to step back up to a rung whose life
    /// ran out (the hysteresis that keeps anyone from bouncing).
    pub return_bars: u32,
    /// The times life may run out on a rung before it is closed to the
    /// competitor for the rest of the stage (never stepped up to again,
    /// nor past it); 0 never closes one.
    pub closed_after: u32,
    /// What a stretch played on a rung counts for in a stage's score,
    /// by its place against the stage's chart (negative easier): 1 on
    /// every rung (no balancing; the place for it, [`rung_score_factor`]).
    pub score_factor: f64,
}

/// The values chosen for the prototype (PROVISIONAL): half the life back
/// (the start's) at a step down, the boundary the next bar line at least
/// the reading horizon plus a second ahead, life kept at one point or
/// more until then; a step up after four bars (16 beats) of full life
/// without a miss or a shock stepped on, eight back up to a rung whose
/// life ran out; every rung's score as earned.
pub const ADAPTIVE_V0: AdaptiveRule = AdaptiveRule {
    name: "adaptive-v0",
    life: 0.5,
    lead: 1.0,
    bar_beats: 4,
    floor: 0.01,
    up_bars: 4,
    return_bars: 8,
    closed_after: 0,
    score_factor: 1.0,
};

/// `adaptive-v1` (the maintainer, 2026-11-14: "after stepping down from a
/// chart the barrier to step up should be higher than it originally is,
/// and if you manage to step up and end up stepping down, you can't step
/// up again"): [`ADAPTIVE_V0`] with sixteen bars of full life, not eight,
/// to step back up to a rung whose life ran out, and a rung closed once
/// life ran out on it twice. The worst a stage can bounce: down, up once
/// after the higher barrier, down, and there they stay. PROVISIONAL.
pub const ADAPTIVE_V1: AdaptiveRule = AdaptiveRule {
    name: "adaptive-v1",
    return_bars: 16,
    closed_after: 2,
    ..ADAPTIVE_V0
};

/// The rule a stage plays ([`ADAPTIVE_V1`]).
pub const ADAPTIVE: AdaptiveRule = ADAPTIVE_V1;

/// What a stretch on the rung `level` places from the stage's chart
/// (negative easier, 0 the stage's) counts for in a stage's score under
/// `rule`: [`AdaptiveRule::score_factor`] on every rung but the stage's.
/// 1 under [`ADAPTIVE_V0`]: nothing is balanced until the maintainer has
/// felt the rule.
pub fn rung_score_factor(rule: &AdaptiveRule, level: i8) -> f64 {
    if level == 0 { 1.0 } else { rule.score_factor }
}

/// One step of a play from a rung to the next ([`crate::Results::steps`]).
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct Step {
    /// When it was decided (judged song seconds): for a step down, when
    /// life reached zero (the old chart's notes not judged by then were
    /// taken away); for a step up, the bar line that ended its full bars.
    pub at: f64,
    /// The boundary: the new chart's notes from here on.
    pub boundary: f64,
    /// Its tick on the stage's timing.
    pub tick: Tick,
    /// The rung stepped to: 0 the stage's chart, then the ladder's rungs
    /// as [`Ladder::rungs`] numbers them from 1 (the easier ones, then
    /// the harder ones).
    pub rung: u8,
    /// The song's chart that rung is.
    pub chart: usize,
    /// A step up, to a harder chart (the old one played to the
    /// boundary); a step down otherwise.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub up: bool,
    /// The rung stepped from (as `rung`).
    #[serde(default)]
    pub from: u8,
    /// A step down that closed the rung it left: life ran out there
    /// [`AdaptiveRule::closed_after`] times, so it is never stepped up to
    /// again in the stage.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub closes: bool,
}

/// A stage's ladder: the stage's chart and its rungs either way, each
/// the next first.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Ladder {
    pub stage: usize,
    /// The easier charts, the next easier first ([`below`]).
    pub below: Vec<usize>,
    /// The harder charts, the next harder first ([`above`]).
    pub above: Vec<usize>,
}

impl Ladder {
    /// Chart `chart` of `song`'s ladder ([`below`], [`above`]).
    pub fn of(song: &Song, chart: usize) -> Ladder {
        Ladder {
            stage: chart,
            below: below(song, chart),
            above: above(song, chart),
        }
    }

    /// Every rung, numbered from 1 in this order (as [`Step::rung`]): the
    /// easier ones, then the harder ones.
    pub fn rungs(&self) -> Vec<usize> {
        self.below.iter().chain(&self.above).copied().collect()
    }

    pub fn is_empty(&self) -> bool {
        self.below.is_empty() && self.above.is_empty()
    }

    /// The chart a rung number names (0 the stage's).
    pub fn chart(&self, rung: u8) -> Option<usize> {
        match usize::from(rung) {
            0 => Some(self.stage),
            r if r <= self.below.len() => self.below.get(r - 1).copied(),
            r => self.above.get(r - 1 - self.below.len()).copied(),
        }
    }

    /// A rung number's place from the stage's chart: negative easier.
    pub fn level(&self, rung: u8) -> i8 {
        level_of(self.below.len(), rung)
    }
}

/// A rung number's place from the stage's chart with `below` rungs
/// below it: negative easier, 0 the stage's, positive harder.
pub fn level_of(below: usize, rung: u8) -> i8 {
    let r = usize::from(rung);
    if r <= below { -(r as i8) } else { (r - below) as i8 }
}

/// The rung number of the place `level` with `below` rungs below.
pub fn rung_of(below: usize, level: i8) -> u8 {
    if level <= 0 { (-level) as u8 } else { (below as i8 + level) as u8 }
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

/// The rungs below a stage on chart `chart` of `song`: the song's other
/// charts that can be its easier rungs, the next easier first. A rung is an
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
/// Empty where the song has no easier chart.
pub fn below(song: &Song, chart: usize) -> Vec<usize> {
    let Some(stage) = song.charts.get(chart) else {
        return Vec::new();
    };
    let mut rungs: Vec<(u8, u32, usize)> = fitting(song, chart)
        .filter_map(|(i, c)| {
            let s = slot(c.difficulty)?;
            is_easier((stage.difficulty, stage.meter), (c.difficulty, c.meter))
                .then_some((s, c.meter, i))
        })
        .collect();
    rungs.sort_by(|a, b| b.0.cmp(&a.0).then(b.1.cmp(&a.1)).then(a.2.cmp(&b.2)));
    rungs.into_iter().map(|r| r.2).collect()
}

/// The rungs above a stage on chart `chart` of `song`: the song's other
/// charts that can be its harder rungs, the next harder first, as
/// [`below`] has the easier ones turned over:
///
/// - above a slotted chart, the charts of higher slots, by slot from the
///   nearest, a slot's charts by meter from the lowest, then by their
///   place in the song; Edits are never rungs of a slotted chart;
/// - above an Edit, the slotted charts of a higher meter, by slot from
///   the lowest, then by meter from the lowest, then by place.
///
/// Empty where the song has no harder chart.
pub fn above(song: &Song, chart: usize) -> Vec<usize> {
    let Some(stage) = song.charts.get(chart) else {
        return Vec::new();
    };
    let mut rungs: Vec<(u8, u32, usize)> = fitting(song, chart)
        .filter_map(|(i, c)| {
            let s = slot(c.difficulty)?;
            is_harder((stage.difficulty, stage.meter), (c.difficulty, c.meter))
                .then_some((s, c.meter, i))
        })
        .collect();
    rungs.sort_by(|a, b| a.0.cmp(&b.0).then(a.1.cmp(&b.1)).then(a.2.cmp(&b.2)));
    rungs.into_iter().map(|r| r.2).collect()
}

/// The song's charts that may be rungs of a stage on chart `chart`
/// either way: originals (no `derivation`), in its style, with notes, on
/// the same timing (the boundary and the breather are measured on one).
fn fitting(song: &Song, chart: usize) -> impl Iterator<Item = (usize, &Chart)> {
    let stage = &song.charts[chart];
    let timing = stage.timing(song);
    song.charts.iter().enumerate().filter(move |(i, c)| {
        *i != chart
            && c.derivation.is_none()
            && c.layout == stage.layout
            && !c.notes.is_empty()
            && same_timing(c.timing(song), timing)
    })
}

/// Whether a chart of difficulty and meter `other` is easier than the
/// stage's (`stage`) as [`below`] has it: a lower slot below a slotted
/// chart; a slotted chart of a lower meter below an Edit; never an Edit.
pub fn is_easier(stage: (Difficulty, u32), other: (Difficulty, u32)) -> bool {
    let Some(s) = slot(other.0) else {
        return false;
    };
    match slot(stage.0) {
        Some(top) => s < top,
        None => other.1 < stage.1,
    }
}

/// Whether a chart of difficulty and meter `other` is harder than the
/// stage's as [`above`] has it: a higher slot above a slotted chart; a
/// slotted chart of a higher meter above an Edit; never an Edit.
pub fn is_harder(stage: (Difficulty, u32), other: (Difficulty, u32)) -> bool {
    let Some(s) = slot(other.0) else {
        return false;
    };
    match slot(stage.0) {
        Some(top) => s > top,
        None => other.1 > stage.1,
    }
}

/// A rung's name as the song list shows it: a named Edit's name, else
/// its difficulty's (data, as the list's).
pub fn rung_name(chart: &Chart) -> String {
    if chart.difficulty == Difficulty::Edit && !chart.name.trim().is_empty() {
        chart.name.trim().to_string()
    } else {
        chart.difficulty.name().to_string()
    }
}

/// The boundary of a step at judged song second `at` for a player who
/// reads `horizon` seconds ahead: the first bar line at least `horizon`
/// plus the rule's lead past `at`, on `timing`: its tick and its second.
pub fn boundary(rule: &AdaptiveRule, timing: &TimingMap, at: f64, horizon: f64) -> (Tick, f64) {
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

/// The song with chart `chart`'s notes merged with its rungs' (each
/// lane's notes of every chart together; the same note of two charts
/// once, the longer kept): what a stage's battle plan is placed over, so
/// that a section safe there is safe on every rung at once and one plan
/// holds whatever chart a competitor is on.
pub fn merged(song: &Song, chart: usize, rungs: &[usize]) -> Song {
    let mut out = song.clone();
    let Some(stage) = out.charts.get_mut(chart) else {
        return out;
    };
    for &r in rungs {
        if let Some(c) = song.charts.get(r) {
            stage.notes.extend(c.notes.iter().cloned());
        }
    }
    stage
        .notes
        .sort_by_key(|n| (n.tick, n.lane, std::cmp::Reverse(n.end_tick())));
    stage.notes.dedup_by(|b, a| a.tick == b.tick && a.lane == b.lane);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_merged_chart_holds_every_rungs_notes_once() {
        let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../assets/songs/beatucada");
        let path = std::fs::read_dir(dir)
            .unwrap()
            .flatten()
            .map(|e| e.path())
            .find(|p| p.extension().is_some_and(|e| e == "ssc"))
            .unwrap();
        let song =
            ddi_library::scan::parse_simfile(&std::fs::read_to_string(path).unwrap(), "ssc").unwrap();
        let stage = song
            .charts
            .iter()
            .position(|c| c.layout == "dance-single" && c.difficulty == Difficulty::Hard)
            .unwrap();
        let rungs = below(&song, stage);
        let m = merged(&song, stage, &rungs);
        let notes = &m.charts[stage].notes;
        for c in std::iter::once(stage).chain(rungs.iter().copied()) {
            for n in &song.charts[c].notes {
                assert!(notes.iter().any(|m| m.tick == n.tick && m.lane == n.lane));
            }
        }
        assert!(notes.windows(2).all(|w| (w[0].tick, w[0].lane) < (w[1].tick, w[1].lane)));
        assert_eq!(m.charts[rungs[0]], song.charts[rungs[0]], "the rungs as they are");
    }

    #[test]
    fn the_boundary_is_a_bar_line_past_the_horizon_and_a_second() {
        // 120 BPM: a beat is half a second, a bar two seconds.
        let timing = TimingMap::constant(120.0, 0.0);
        let (tick, at) = boundary(&ADAPTIVE_V0, &timing, 10.0, 2.5);
        // 13.5 s is beat 27: the next bar line is beat 28, 14 s.
        assert_eq!(tick, Tick(28 * TICKS_PER_BEAT));
        assert!((at - 14.0).abs() < 1e-9);
        // Exactly on a bar line: that one.
        let (_, at) = boundary(&ADAPTIVE_V0, &timing, 11.0, 0.0);
        assert!((at - 12.0).abs() < 1e-9);
    }
}
