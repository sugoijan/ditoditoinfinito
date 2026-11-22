//! Adaptive charts: stepping down and up a song's ladder (a multiplayer
//! match's rule "Charts: Adaptive", `docs/plans/multiplayer-prototype-progress.md`,
//! "Adaptive charts"). PROVISIONAL, named: [`ADAPTIVE_V5`] (what is
//! played, [`ADAPTIVE`]; `adaptive-v0` and `adaptive-v1` were retired on
//! 2026-11-14, `adaptive-v2` and `adaptive-v3` on 2026-11-15, `adaptive-v4`
//! on 2026-11-16: v3 was v4 with an early step up that asked 95% of the
//! taps in the two best judgments, v4 is v5 with a normal step up after
//! eight bars rather than six).
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
//! who held full life with no miss and no shock stepped on moves to the
//! next harder rung from the next boundary, the old chart played up to
//! it, life unchanged: early, after [`AdaptiveRule::up_bars`] bars with
//! [`AdaptiveRule::accuracy`] of their taps in the
//! [`AdaptiveRule::early_best`] best judgments, or after
//! the longer [`AdaptiveRule::normal_bars`] with
//! [`AdaptiveRule::normal_accuracy`] in the
//! [`AdaptiveRule::normal_best`] best ([`AdaptiveRule::return_bars`] to
//! go back up to a rung whose life ran out, and never again to one whose
//! life ran out [`AdaptiveRule::closed_after`] times: the rung is closed
//! to them for the rest of the stage).
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
    /// earn an early step up (counted from the last step's boundary), with
    /// [`AdaptiveRule::accuracy`]; 0 never steps up.
    pub up_bars: u32,
    /// The bars of full life, with no miss and no shock stepped on, that
    /// earn a normal step up, with [`AdaptiveRule::normal_accuracy`]; 0
    /// has none (the early step up alone).
    pub normal_bars: u32,
    /// The least share of the taps judged in a normal step up's bars in
    /// the ruleset's [`AdaptiveRule::normal_best`] best judgments; 0 asks
    /// nothing of them.
    pub normal_accuracy: f64,
    /// How many of the ruleset's best judgments
    /// [`AdaptiveRule::normal_accuracy`] counts (3: W0, W1 or W2 under
    /// DDI).
    pub normal_best: u8,
    /// The bars it takes instead to step back up to a rung whose life
    /// ran out (the hysteresis that keeps anyone from bouncing), early or
    /// normal: with either's accuracy over them.
    pub return_bars: u32,
    /// The times life may run out on a rung before it is closed to the
    /// competitor for the rest of the stage (never stepped up to again,
    /// nor past it); 0 never closes one.
    pub closed_after: u32,
    /// The least share of the taps judged in the bars that earn an early
    /// step up that must be in the ruleset's [`AdaptiveRule::early_best`]
    /// best judgments; 0 asks nothing of them. Bars without a tap ask
    /// nothing.
    pub accuracy: f64,
    /// How many of the ruleset's best judgments [`AdaptiveRule::accuracy`]
    /// counts (3: W0, W1 or W2 under DDI; W1 to W3 where a ruleset has no
    /// W0).
    pub early_best: u8,
    /// The seconds past the reading horizon a step down's boundary is at
    /// least (the breather: life held at [`AdaptiveRule::floor`] until
    /// then); a step up's is [`AdaptiveRule::lead`].
    pub down_lead: f64,
}

/// `adaptive-v5` (PROVISIONAL; the maintainer's decisions of 2026-11-15,
/// "Ease it to 90% in the three best", after feedback round H's "a strict
/// criteria for an early step up, and a less strict one for a normal step
/// up", and of 2026-11-16, after playing it: the normal step up after six
/// bars rather than eight): half the life back (the start's) at a step
/// down, the boundary the next bar line at least the reading horizon
/// ahead, life kept at one point or more until then; a step up from the
/// next bar line at least the reading horizon and a second ahead, after
/// full life without a miss, a hold let go or a mine or shock stepped on
/// for either
/// - **early**: four bars (16 beats) with [`ADAPTIVE_EARLY_ACCURACY_V5`]
///   of their taps in the three best judgments (`adaptive-v3` asked 95%
///   in the two best, which people almost never reached), or
/// - **normal**: [`ADAPTIVE_NORMAL_BARS_V5`] bars with
///   [`ADAPTIVE_NORMAL_ACCURACY_V5`] of their taps in the three best
///   (`adaptive-v4` asked eight bars);
///
/// sixteen bars back up to a rung whose life ran out, by either's
/// accuracy over them, and a rung closed once life ran out on it twice
/// (`adaptive-v1`, the maintainer, 2026-11-14: "after stepping down from
/// a chart the barrier to step up should be higher than it originally
/// is, and if you manage to step up and end up stepping down, you can't
/// step up again"). Every rung's score as earned. The values measured in
/// `docs/plans/multiplayer-prototype-progress.md`, "Feedback round H",
/// "Round D1" and "Round P1a".
pub const ADAPTIVE_V5: AdaptiveRule = AdaptiveRule {
    name: "adaptive-v5",
    life: 0.5,
    lead: 1.0,
    bar_beats: 4,
    floor: 0.01,
    up_bars: 4,
    normal_bars: ADAPTIVE_NORMAL_BARS_V5,
    normal_accuracy: ADAPTIVE_NORMAL_ACCURACY_V5,
    normal_best: 3,
    return_bars: 16,
    closed_after: 2,
    accuracy: ADAPTIVE_EARLY_ACCURACY_V5,
    early_best: 3,
    down_lead: 0.0,
};

/// PROVISIONAL (`adaptive-v5`, as `adaptive-v4`'s): an early step up's
/// share of its taps in the three best judgments, 90%.
pub const ADAPTIVE_EARLY_ACCURACY_V5: f64 = 0.90;

/// PROVISIONAL (`adaptive-v5`): the bars of a normal step up (eight
/// under `adaptive-v4`).
pub const ADAPTIVE_NORMAL_BARS_V5: u32 = 6;

/// PROVISIONAL (`adaptive-v5`, as `adaptive-v4`'s): a normal step up's
/// share of its taps in the three best judgments.
pub const ADAPTIVE_NORMAL_ACCURACY_V5: f64 = 0.80;

/// The rule a stage plays ([`ADAPTIVE_V5`]).
pub const ADAPTIVE: AdaptiveRule = ADAPTIVE_V5;

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
    /// A step up earned early ([`AdaptiveRule::up_bars`] with
    /// [`AdaptiveRule::accuracy`]); a normal one, or a step back up to a
    /// rung whose life ran out, otherwise.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub early: bool,
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

    /// The rung number chart `chart` is on this ladder (0 the stage's),
    /// none where it is none of its charts.
    pub fn rung_of_chart(&self, chart: usize) -> Option<u8> {
        if chart == self.stage {
            return Some(0);
        }
        self.below
            .iter()
            .chain(&self.above)
            .position(|&c| c == chart)
            .and_then(|i| u8::try_from(i + 1).ok())
    }

    /// Its charts from the easiest to the hardest: the easier rungs turned
    /// over, the stage's, then the harder ones.
    pub fn line(&self) -> Vec<usize> {
        self.below
            .iter()
            .rev()
            .copied()
            .chain(std::iter::once(self.stage))
            .chain(self.above.iter().copied())
            .collect()
    }

    /// The same charts as the ladder of a play started on rung `rung`
    /// (room model revision 23: under Adaptive each competitor starts on
    /// their own chart): that chart its own, those easier than it below
    /// (the next first), those harder above. What a CPU's play is planned
    /// on (its steps numbered from its own chart, put back on the stage's
    /// numbers by [`Ladder::rung_of_chart`]); none for a rung it has not.
    pub fn around(&self, rung: u8) -> Option<Ladder> {
        let chart = self.chart(rung)?;
        let line = self.line();
        let at = line.iter().position(|&c| c == chart)?;
        Some(Ladder {
            stage: chart,
            below: line[..at].iter().rev().copied().collect(),
            above: line[at + 1..].to_vec(),
        })
    }
}

/// `cpu-start-v0` (round OC, PROVISIONAL): the rung of `ladder` (a stage
/// on `song`) a CPU of level `level` (an index of the ten, 0 the weakest)
/// starts on under Adaptive, as every device works it out from the charts
/// the battle plan's ladder names: the difficulty each pair of levels is
/// at home on (levels 1 and 2 Beginner, 3 and 4 Easy, 5 and 6 Medium, 7
/// and 8 Hard, 9 and 10 Challenge), else the nearest the ladder has, the
/// easier of two as near; of several charts of that difficulty the
/// lowest meter for the first of the pair and the highest for the
/// second. An Edit (only ever the stage's chart, never a rung of a
/// slotted one) only where the ladder has nothing else. A song of one
/// chart: that one.
pub fn cpu_start(song: &Song, ladder: &Ladder, level: usize) -> u8 {
    let line = ladder.line();
    let charts: Vec<(Difficulty, u32)> = line
        .iter()
        .map(|&c| song.charts.get(c).map_or((Difficulty::Edit, 0), |c| (c.difficulty, c.meter)))
        .collect();
    cpu_start_in(&charts, level)
        .and_then(|i| ladder.rung_of_chart(line[i]))
        .unwrap_or(0)
}

/// [`cpu_start`] over a ladder's charts given as their difficulties and
/// meters from the easiest to the hardest ([`Ladder::line`]'s order): the
/// place of the one a CPU of level `level` starts on; none for an empty
/// line. What a screen shows before the stage's song is read (the song
/// list's charts) is worked out by the same rule.
pub fn cpu_start_in(line: &[(Difficulty, u32)], level: usize) -> Option<usize> {
    let level = level.min(9);
    let want = (level / 2) as i32;
    let slotted: Vec<(usize, u8, u32)> = line
        .iter()
        .enumerate()
        .filter_map(|(i, &(d, m))| Some((i, slot(d)?, m)))
        .collect();
    let Some(best) = slotted
        .iter()
        .map(|&(_, s, _)| (i32::from(s) - want).abs())
        .min()
    else {
        // An Edit alone (or nothing): its first.
        return (!line.is_empty()).then_some(0);
    };
    // The easier of two slots as near: the lower one.
    let target = slotted
        .iter()
        .filter(|&&(_, s, _)| (i32::from(s) - want).abs() == best)
        .map(|&(_, s, _)| s)
        .min()
        .unwrap_or(0);
    let mut of: Vec<(usize, u32)> = slotted
        .iter()
        .filter(|&&(_, s, _)| s == target)
        .map(|&(i, _, m)| (i, m))
        .collect();
    // By meter from the lowest, then by place on the line (stable).
    of.sort_by_key(|&(_, m)| m);
    let pick = if level.is_multiple_of(2) { of.first() } else { of.last() };
    pick.map(|&(i, _)| i)
}

/// [`cpu_start`]'s rule's name.
pub const CPU_START: &str = "cpu-start-v0";

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
    rungs(song, chart, false)
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
    rungs(song, chart, true)
}

/// [`above`] (`harder`) or [`below`]: the fitting charts harder or
/// easier than the stage's, the nearest slot first, a slot's charts by
/// meter from the nearest, then by their place in the song.
fn rungs(song: &Song, chart: usize, harder: bool) -> Vec<usize> {
    let Some(stage) = song.charts.get(chart) else {
        return Vec::new();
    };
    let want = if harder {
        std::cmp::Ordering::Greater
    } else {
        std::cmp::Ordering::Less
    };
    let mut rungs: Vec<(u8, u32, usize)> = fitting(song, chart)
        .filter_map(|(i, c)| {
            let s = slot(c.difficulty)?;
            (compare((stage.difficulty, stage.meter), (c.difficulty, c.meter)) == Some(want))
                .then_some((s, c.meter, i))
        })
        .collect();
    if harder {
        rungs.sort_by(|a, b| a.0.cmp(&b.0).then(a.1.cmp(&b.1)).then(a.2.cmp(&b.2)));
    } else {
        rungs.sort_by(|a, b| b.0.cmp(&a.0).then(b.1.cmp(&a.1)).then(a.2.cmp(&b.2)));
    }
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
    compare(stage, other) == Some(std::cmp::Ordering::Less)
}

/// Whether a chart of difficulty and meter `other` is harder than the
/// stage's as [`above`] has it: a higher slot above a slotted chart; a
/// slotted chart of a higher meter above an Edit; never an Edit.
pub fn is_harder(stage: (Difficulty, u32), other: (Difficulty, u32)) -> bool {
    compare(stage, other) == Some(std::cmp::Ordering::Greater)
}

/// How a chart of difficulty and meter `other` is placed against the
/// stage's: by slot against a slotted chart, by meter (a slotted one)
/// against an Edit; none for an Edit.
fn compare(stage: (Difficulty, u32), other: (Difficulty, u32)) -> Option<std::cmp::Ordering> {
    let s = slot(other.0)?;
    Some(match slot(stage.0) {
        Some(top) => s.cmp(&top),
        None => other.1.cmp(&stage.1),
    })
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

impl AdaptiveRule {
    /// The boundary of a step (`down`, or up) at judged song second `at`
    /// for a player who reads `horizon` seconds ahead: the first bar line
    /// at least `horizon` plus the step's lead ([`AdaptiveRule::down_lead`]
    /// down, [`AdaptiveRule::lead`] up) past `at`, on `timing`: its tick
    /// and its second.
    pub fn boundary(&self, down: bool, timing: &TimingMap, at: f64, horizon: f64) -> (Tick, f64) {
        let lead = if down { self.down_lead } else { self.lead };
        boundary(self.bar_beats, lead, timing, at, horizon)
    }
}

/// [`AdaptiveRule::boundary`] by bars of `bar_beats` and `lead`.
fn boundary(bar_beats: u32, lead: f64, timing: &TimingMap, at: f64, horizon: f64) -> (Tick, f64) {
    let target = at + horizon.max(0.0) + lead;
    let bar = i64::from(bar_beats.max(1)) * TICKS_PER_BEAT;
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
        let song = crate::test_songs::bundled("beatucada");
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

    /// A step up's boundary: the next bar line at least the horizon and
    /// a second ahead.
    #[test]
    fn a_step_ups_boundary_is_a_bar_line_past_the_horizon_and_a_second() {
        // 120 BPM: a beat is half a second, a bar two seconds.
        let timing = TimingMap::constant(120.0, 0.0);
        assert_eq!(ADAPTIVE.lead, 1.0);
        let (tick, at) = ADAPTIVE.boundary(false, &timing, 10.0, 2.5);
        // 13.5 s is beat 27: the next bar line is beat 28, 14 s.
        assert_eq!(tick, Tick(28 * TICKS_PER_BEAT));
        assert!((at - 14.0).abs() < 1e-9);
        // Exactly on a bar line: that one.
        let (_, at) = ADAPTIVE.boundary(false, &timing, 11.0, 0.0);
        assert!((at - 12.0).abs() < 1e-9);
    }

    /// `adaptive-v5`'s step down: the next bar line at least the horizon
    /// ahead, without the extra second; its step up keeps the second. Its
    /// values, by name.
    #[test]
    fn a_step_downs_breather_has_no_extra_second() {
        let timing = TimingMap::constant(120.0, 0.0);
        assert_eq!(ADAPTIVE_V5.down_lead, 0.0);
        // 11.5 s: beat 23, the bar line at 12 s; with the second, 14 s.
        let (_, at) = ADAPTIVE_V5.boundary(true, &timing, 10.0, 1.5);
        assert!((at - 12.0).abs() < 1e-9);
        let (_, at) = ADAPTIVE_V5.boundary(false, &timing, 10.0, 1.5);
        assert!((at - 14.0).abs() < 1e-9);
        assert_eq!(ADAPTIVE, ADAPTIVE_V5);
        assert_eq!(ADAPTIVE_V5.name, "adaptive-v5");
        assert_eq!(
            (ADAPTIVE_V5.life, ADAPTIVE_V5.floor, ADAPTIVE_V5.bar_beats),
            (0.5, 0.01, 4)
        );
        assert_eq!(
            (ADAPTIVE_V5.up_bars, ADAPTIVE_V5.return_bars, ADAPTIVE_V5.closed_after),
            (4, 16, 2)
        );
        // The early step up: 90% in the three best.
        assert_eq!((ADAPTIVE_V5.accuracy, ADAPTIVE_V5.early_best), (0.90, 3));
        assert_eq!(
            (ADAPTIVE_V5.normal_bars, ADAPTIVE_V5.normal_accuracy, ADAPTIVE_V5.normal_best),
            (6, 0.80, 3)
        );
    }
}
