//! The battle a stage is played under, by name and version, and how its
//! judged runs become a confirmed ranking. Pure: the stage runner
//! (`ddi_session::battle`) judges the plays and hands the runs here; a
//! server can rank the same way.
//!
//! Two presets, PROVISIONAL. [`BattlePreset::GaugeSurvivalV0`], the
//! client study's default for the first playable slice
//! (`docs/plans/multiplayer-prototype-progress.md`, M3): life is the
//! ruleset's gauge; an empty gauge is out of the stage (never the match,
//! unless the match's rules say so); no attacks, no mines added, no
//! targeting. [`BattlePreset::GaugeAttacksV0`], the same with attacks
//! (`crate::attacks`), which the match's Attacks rule chooses
//! (`MatchRules::attacks`). They stand in for the design's standard battle
//! (`docs/plans/multiplayer.md`) until that is decided, and are never
//! presented as it.

use crate::awards::{Finish, Finisher};
use crate::ids::ParticipantId;
use serde::{Deserialize, Serialize};

/// A named, versioned battle preset. Locked with the match's rules
/// (`MatchRules::battle` holds its id).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BattlePreset {
    /// PROVISIONAL. Each competitor's life is their own ruleset gauge,
    /// judged as in a single play; when it empties they are out of the
    /// stage. The stage is decided once at most one is left before the
    /// song's end (they finish the song; nothing they do after changes
    /// the result); otherwise at the song's end. Ranked by [`rank`].
    GaugeSurvivalV0,
    /// PROVISIONAL: [`BattlePreset::GaugeSurvivalV0`] with attacks
    /// (`crate::attacks`, [`crate::attacks::ATTACKS_V0`]): each competitor
    /// earns power by timing, shown as a bar above life; a full bar is
    /// spent on eight shocks in someone's play; settled by
    /// [`crate::attacks::SETTLEMENT_V0`], ranked by [`rank`] as survival.
    /// The match's Attacks rule (`MatchRules::attacks`: On by default in
    /// CPU practice, where attacks are offered) chooses it.
    GaugeAttacksV0,
}

impl BattlePreset {
    pub const GAUGE_SURVIVAL_V0: &'static str = "gauge-survival-v0";
    pub const GAUGE_ATTACKS_V0: &'static str = "gauge-attacks-v0";

    pub fn id(self) -> &'static str {
        match self {
            Self::GaugeSurvivalV0 => Self::GAUGE_SURVIVAL_V0,
            Self::GaugeAttacksV0 => Self::GAUGE_ATTACKS_V0,
        }
    }

    /// The preset an id names, if it is one this build plays.
    pub fn parse(id: &str) -> Option<Self> {
        match id {
            Self::GAUGE_SURVIVAL_V0 => Some(Self::GaugeSurvivalV0),
            Self::GAUGE_ATTACKS_V0 => Some(Self::GaugeAttacksV0),
            _ => None,
        }
    }

    /// Its competitors earn attack power (`crate::attacks`).
    pub fn attacks(self) -> bool {
        matches!(self, Self::GaugeAttacksV0)
    }

    /// Not a decided design: what shows it says so.
    pub fn provisional(self) -> bool {
        true
    }
}

/// How one starter's stage went, as judged.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Run {
    /// Still in (at the song's end, or when the stage was decided): their
    /// gauge's life (`0..=1`), their score as a fraction of the chart's
    /// best (`0..=1`; the ruleset's percent, money or EX; DDI's
    /// proportional part, points over the most their notes give), where
    /// the ruleset counts one (DDI) their absolute score, and the score
    /// as the room shows it and ranks by it ([`rank`]): under DDI the
    /// sum (the proportional part plus the absolute score); a CPU's its
    /// own (it plays the whole chart; before `cpu-play-v1`, put on the
    /// chart's scale by whoever judged it).
    Live {
        life: f64,
        accuracy: f64,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        absolute: Option<Absolute>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        score: Option<i64>,
        /// The steps they made down and up the song's ladder (a match's
        /// "Charts: Adaptive", [`crate::Charts::Adaptive`]): 0 where none,
        /// left out then; never ranked by. ADDITIVE: read as 0 where
        /// missing.
        #[serde(default, skip_serializing_if = "is_zero")]
        steps: u8,
    },
    /// Their gauge emptied at judged song second `at`.
    Out { at: f64 },
    /// Left the stage while competing (a quit, an interruption).
    Forfeit,
}

fn is_zero(n: &u8) -> bool {
    *n == 0
}

/// A score not divided by the chart's size (DDI's absolute score, `100 ×
/// points + W0s`) and the most points the notes played give: two
/// competitors' scores compare only where `most` is the same, that is
/// where they played the same notes (a CPU plays a tap-only copy of the
/// chart, whose most differs from the full chart's wherever it has holds).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Absolute {
    pub score: i64,
    pub most: i64,
}

/// A run as a member reports it to an online room (M5d,
/// `Command::ReportOutcome`): [`Run`] in whole numbers (millionths of life
/// and accuracy, milliseconds), so the room's state stays comparable and
/// every authority ranks it alike. SELF-REPORTED: the room cannot check it
/// (`docs/plans/multiplayer-prototype-progress.md`, its trust limits).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ReportedRun {
    /// `absolute` and `score` are ADDITIVE to protocol 2 (the DDI
    /// ruleset's rounds 3 and 3b): left out where there is none, read as
    /// none where missing, so a report without them is what it was and an
    /// authority one build behind drops them (it then ranks by the
    /// proportional part, as before).
    Live {
        life_ppm: u32,
        accuracy_ppm: u32,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        absolute: Option<Absolute>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        score: Option<i64>,
        /// As [`Run::Live`]'s; an authority one build behind drops it
        /// (and is never offered the rule, `RoomPolicy::step_down`).
        #[serde(default, skip_serializing_if = "is_zero")]
        steps: u8,
    },
    Out { at_ms: u64 },
    Forfeit,
}

impl ReportedRun {
    pub const SCALE: f64 = 1_000_000.0;

    pub fn of(run: Run) -> ReportedRun {
        let ppm = |x: f64| (x.clamp(0.0, 1.0) * Self::SCALE).round() as u32;
        match run {
            Run::Live {
                life,
                accuracy,
                absolute,
                score,
                steps,
            } => ReportedRun::Live {
                life_ppm: ppm(life),
                accuracy_ppm: ppm(accuracy),
                absolute,
                score,
                steps,
            },
            Run::Out { at } => ReportedRun::Out {
                at_ms: (at.max(0.0) * 1_000.0).round() as u64,
            },
            Run::Forfeit => ReportedRun::Forfeit,
        }
    }

    pub fn run(self) -> Run {
        match self {
            ReportedRun::Live {
                life_ppm,
                accuracy_ppm,
                absolute,
                score,
                steps,
            } => Run::Live {
                life: f64::from(life_ppm) / Self::SCALE,
                accuracy: f64::from(accuracy_ppm) / Self::SCALE,
                absolute,
                score,
                steps,
            },
            ReportedRun::Out { at_ms } => Run::Out {
                at: at_ms as f64 / 1_000.0,
            },
            ReportedRun::Forfeit => Run::Forfeit,
        }
    }

    /// The song was played to its end: the song is over for everyone, the
    /// room waits only a short while more for the others' reports.
    pub fn finished_song(self) -> bool {
        matches!(self, ReportedRun::Live { .. })
    }
}

/// How a starter performed, beside their run (feedback round B,
/// `docs/plans/multiplayer-prototype-progress.md`): what a stage's
/// standings show of them. SELF-REPORTED as the run is, shown only, never
/// ranked by. Plain numbers and the ruleset's own word, so the room's
/// state stays comparable; which tier a count belongs to is the match's
/// ruleset's to say (everyone plays under it).
///
/// ADDITIVE to protocol 2, no new version: it travels as an optional
/// field of `Command::ReportOutcome` and `Event::OutcomeReported` and is
/// kept in `StagePlay::details`; every field has a default. An authority
/// that predates it ignores the field (the event then carries none), a
/// client that predates it ignores it in events and snapshots, and one
/// that gets none shows what the run itself says.
#[derive(Clone, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct RunDetail {
    /// The ruleset's score where it counts one, as shown (money; DDI's
    /// score, the sum of its proportional part and its absolute score; a
    /// CPU's its own, since it plays the whole chart, `cpu-play-v1`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub score: Option<i64>,
    /// Its percent in hundredths of a percent (`9950` is 99.50 %; DDI's
    /// may be negative).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub percent: Option<i32>,
    /// The grade, in the ruleset's own word.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub grade: String,
    /// Life at the end, in millionths (`0` once out).
    #[serde(default)]
    pub life_ppm: u32,
    /// When their gauge emptied, in milliseconds of judged song time.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub out_at_ms: Option<u64>,
    #[serde(default)]
    pub max_combo: u32,
    /// The taps counted by the ruleset's tiers as its results list them,
    /// best first, the misses last.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub tiers: Vec<u32>,
    /// Where the battle has attacks: those they launched, those sent
    /// against them (`received`, M8 round 3: additive as the rest) and
    /// the shocks they stepped on.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub attacks: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub received: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub shocks: Option<u32>,
    /// The chart they finished on where it is not the stage's (a match's
    /// "Charts: Adaptive"): its name as the song list names it (data);
    /// none on the stage's chart. Additive as the rest.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub chart: Option<String>,
    /// That chart is harder than the stage's (they stepped up the
    /// ladder); easier otherwise. Additive: left out while false, read
    /// as false where missing (a step down, all there was before).
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub harder: bool,
}

impl RunDetail {
    /// The longest grade (characters) and the most tiers a room keeps of
    /// a report: what it is sent is cut to them; the longest chart name.
    pub const MAX_GRADE: usize = 24;
    pub const MAX_TIERS: usize = 12;
    pub const MAX_CHART: usize = 32;

    /// The detail within the room's bounds.
    pub fn bounded(mut self) -> RunDetail {
        if self.grade.chars().count() > Self::MAX_GRADE {
            self.grade = self.grade.chars().take(Self::MAX_GRADE).collect();
        }
        if let Some(c) = self.chart.as_mut()
            && c.chars().count() > Self::MAX_CHART
        {
            *c = c.chars().take(Self::MAX_CHART).collect();
        }
        self.tiers.truncate(Self::MAX_TIERS);
        self.life_ppm = self.life_ppm.min(ReportedRun::SCALE as u32);
        self
    }
}

/// A starter and their run.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct Entrant {
    pub participant: ParticipantId,
    pub run: Run,
}

/// Whether the stage is decided before the song's end: at most one starter
/// is still in. With one, they won and finish the song; with none, the
/// last out share the top place.
pub fn decided(entrants: &[Entrant]) -> bool {
    entrants
        .iter()
        .filter(|e| matches!(e.run, Run::Live { .. }))
        .count()
        <= 1
}

/// [`BattlePreset::GaugeSurvivalV0`]'s ranking (and
/// [`BattlePreset::GaugeAttacksV0`]'s), best first, each group tied
/// (`docs/plans/multiplayer.md`, "Elimination and a truthful result"):
/// those still in by life, then by the score ([`Run::Live`]'s `score`:
/// under DDI the sum, everyone's on the chart's own scale), or under
/// [`RankBy::Score`] (a match's "Charts: Adaptive", `adaptive-v0`) by
/// the score as earned, whatever charts it was earned on, life only
/// breaking a tie (the steps a run made are never ranked by); equal ones
/// tied. Then those out, later first (the same second: tied); then
/// forfeits, tied. Participants are ordered by id within a group.
///
/// Where someone still in reported no score (a ruleset without one, a
/// build before round 3b), those still in are ranked as before it: by
/// life, then accuracy (DDI's proportional part), then, among those who
/// played the same notes ([`Absolute::most`] equal), the absolute score,
/// each set of the same notes ordered by its own and the sets' bests
/// sharing a place.
pub fn rank(entrants: &[Entrant], by: crate::rules::RankBy) -> Vec<Vec<Finisher>> {
    let scored = entrants.iter().all(|e| match e.run {
        Run::Live { score, .. } => score.is_some(),
        _ => true,
    });
    let mut live: Vec<(u8, f64, f64, Option<Absolute>, ParticipantId)> = Vec::new();
    let mut by_score: Vec<(u8, f64, i64, ParticipantId)> = Vec::new();
    let mut out: Vec<(f64, ParticipantId)> = Vec::new();
    let mut forfeit: Vec<ParticipantId> = Vec::new();
    for e in entrants {
        match e.run {
            Run::Live {
                life,
                score: Some(score),
                ..
            } if scored => by_score.push((0, life, score, e.participant)),
            Run::Live {
                life,
                accuracy,
                absolute,
                ..
            } => live.push((0, life, accuracy, absolute, e.participant)),
            Run::Out { at } => out.push((at, e.participant)),
            Run::Forfeit => forfeit.push(e.participant),
        }
    }
    live.sort_by(|a, b| {
        a.0.cmp(&b.0)
            .then(b.1.total_cmp(&a.1))
            .then(b.2.total_cmp(&a.2))
            .then(a.4.cmp(&b.4))
    });
    out.sort_by(|a, b| b.0.total_cmp(&a.0).then(a.1.cmp(&b.1)));
    forfeit.sort();
    let mut ranking: Vec<Vec<Finisher>> = Vec::new();
    let finisher = |participant, finish| Finisher {
        participant,
        finish,
    };
    // Life, then the score (or the score, then life): equal ones share
    // a place.
    by_score.sort_by(|a, b| {
        let (life, score) = (b.1.total_cmp(&a.1), b.2.cmp(&a.2));
        match by {
            crate::rules::RankBy::Life => life.then(score),
            crate::rules::RankBy::Score => score.then(life),
        }
        .then(a.3.cmp(&b.3))
    });
    let mut last: Option<(u8, f64, i64)> = None;
    for (steps, life, score, id) in by_score {
        let f = finisher(id, Finish::Survived);
        match ranking.last_mut() {
            Some(group) if last == Some((steps, life, score)) => group.push(f),
            _ => ranking.push(vec![f]),
        }
        last = Some((steps, life, score));
    }
    // Without everyone's score: each run of equal life and accuracy,
    // split by the absolute score.
    let mut i = 0;
    while i < live.len() {
        let same = live[i..]
            .iter()
            .take_while(|l| l.0 == live[i].0 && l.1 == live[i].1 && l.2 == live[i].2)
            .count();
        // Per set of the same notes (none known: a set of its own), the
        // distinct scores from the best: a competitor's level is its
        // score's place among them.
        let level = |l: &(u8, f64, f64, Option<Absolute>, ParticipantId)| -> usize {
            let Some(mine) = l.3 else {
                return 0;
            };
            let mut better: Vec<i64> = live[i..i + same]
                .iter()
                .filter_map(|o| o.3)
                .filter(|o| o.most == mine.most && o.score > mine.score)
                .map(|o| o.score)
                .collect();
            better.sort_unstable();
            better.dedup();
            better.len()
        };
        let mut levels: Vec<(usize, ParticipantId)> =
            live[i..i + same].iter().map(|l| (level(l), l.4)).collect();
        levels.sort();
        let mut last: Option<usize> = None;
        for (level, id) in levels {
            let f = finisher(id, Finish::Survived);
            match ranking.last_mut() {
                Some(group) if last == Some(level) => group.push(f),
                _ => ranking.push(vec![f]),
            }
            last = Some(level);
        }
        i += same;
    }
    let mut last: Option<f64> = None;
    for (at, id) in out {
        let f = finisher(id, Finish::Eliminated);
        match ranking.last_mut() {
            Some(group) if last == Some(at) => group.push(f),
            _ => ranking.push(vec![f]),
        }
        last = Some(at);
    }
    if !forfeit.is_empty() {
        ranking.push(
            forfeit
                .into_iter()
                .map(|id| finisher(id, Finish::Forfeit))
                .collect(),
        );
    }
    ranking
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::rules::RankBy;

    fn e(id: u64, run: Run) -> Entrant {
        Entrant {
            participant: ParticipantId(id),
            run,
        }
    }

    fn ids(ranking: &[Vec<Finisher>]) -> Vec<Vec<u64>> {
        ranking
            .iter()
            .map(|g| g.iter().map(|f| f.participant.0).collect())
            .collect()
    }

    #[test]
    fn reported_runs_round_to_whole_numbers_and_back() {
        let live = ReportedRun::of(Run::Live {
            life: 0.5,
            accuracy: 0.912_345_67,
            absolute: None,
            score: None,
            steps: 0,
        });
        assert_eq!(
            live,
            ReportedRun::Live {
                life_ppm: 500_000,
                accuracy_ppm: 912_346,
                absolute: None,
                score: None,
                steps: 0,
            }
        );
        assert!(live.finished_song());
        assert_eq!(
            ReportedRun::of(Run::Out { at: 12.3456 }),
            ReportedRun::Out { at_ms: 12_346 }
        );
        assert_eq!(
            ReportedRun::Out { at_ms: 1_500 }.run(),
            Run::Out { at: 1.5 }
        );
        assert_eq!(ReportedRun::of(Run::Forfeit).run(), Run::Forfeit);
        // Without an absolute score a report is on the wire what it was.
        assert_eq!(
            serde_json::to_string(&live).unwrap(),
            r#"{"live":{"life_ppm":500000,"accuracy_ppm":912346}}"#
        );
        assert_eq!(
            serde_json::from_str::<ReportedRun>(
                r#"{"live":{"life_ppm":500000,"accuracy_ppm":912346}}"#
            )
            .unwrap(),
            live
        );
        // With one it goes there and back.
        let absolute = Some(Absolute {
            score: 1_234_567,
            most: 50_000,
        });
        let scored = ReportedRun::of(Run::Live {
            life: 1.0,
            accuracy: 0.9,
            absolute,
            score: None,
            steps: 0,
        });
        assert_eq!(
            serde_json::to_string(&scored).unwrap(),
            r#"{"live":{"life_ppm":1000000,"accuracy_ppm":900000,"absolute":{"score":1234567,"most":50000}}}"#
        );
        assert_eq!(
            scored.run(),
            Run::Live {
                life: 1.0,
                accuracy: 0.9,
                absolute,
                score: None,
                steps: 0,
            }
        );
    }

    fn live(life: f64, accuracy: f64, absolute: Option<(i64, i64)>) -> Run {
        Run::Live {
            life,
            accuracy,
            absolute: absolute.map(|(score, most)| Absolute { score, most }),
            score: None,
            steps: 0,
        }
    }

    fn summed(life: f64, score: i64) -> Run {
        Run::Live {
            life,
            accuracy: 0.5,
            absolute: None,
            score: Some(score),
            steps: 0,
        }
    }

    /// Adaptive (`adaptive-v0`): among those still in, the score as
    /// earned first, life only breaking a tie, the steps never; those
    /// out after, as ever. As picked keeps life first. A report without
    /// steps reads as none, and one with none says none.
    #[test]
    fn under_adaptive_charts_those_still_in_rank_by_the_score_and_never_by_steps() {
        let stepped = |life: f64, score: i64, steps: u8| Run::Live {
            life,
            accuracy: 0.5,
            absolute: None,
            score: Some(score),
            steps,
        };
        let field = [
            e(1, stepped(1.0, 9_000_000, 1)),
            e(2, stepped(0.2, 1_000, 0)),
            e(3, stepped(0.9, 9_500_000, 2)),
            e(4, Run::Out { at: 80.0 }),
            e(5, stepped(0.5, 5_000_000, 1)),
            e(6, stepped(0.4, 5_000_000, 0)),
        ];
        let r = rank(&field, RankBy::Score);
        assert_eq!(ids(&r), vec![vec![3], vec![1], vec![5], vec![6], vec![2], vec![4]]);
        let r = rank(&field, RankBy::Life);
        assert_eq!(ids(&r), vec![vec![1], vec![3], vec![5], vec![6], vec![2], vec![4]]);
        // The same score and life: tied, whatever the steps.
        let r = rank(&[e(1, stepped(0.7, 10, 3)), e(2, stepped(0.7, 10, 0))], RankBy::Score);
        assert_eq!(ids(&r), vec![vec![1, 2]]);
        // As before rounds 3b too (someone without a score).
        let r = rank(&[
            e(1, Run::Live { life: 1.0, accuracy: 0.9, absolute: None, score: None, steps: 1 }),
            e(2, live(0.1, 0.1, None)),
        ], RankBy::Score);
        assert_eq!(ids(&r), vec![vec![1], vec![2]]);
        // The wire: steps left out at none, read as none where missing.
        let none = serde_json::to_value(ReportedRun::of(stepped(1.0, 5, 0))).unwrap();
        assert!(none["live"].get("steps").is_none(), "{none}");
        let two = ReportedRun::of(stepped(1.0, 5, 2));
        let json = serde_json::to_value(two).unwrap();
        assert_eq!(json["live"]["steps"], 2);
        assert_eq!(serde_json::from_value::<ReportedRun>(json).unwrap(), two);
        assert_eq!(two.run(), stepped(1.0, 5, 2));
    }

    /// Round 3b: life, then the score (the sum), whatever notes each
    /// played (a CPU's its own); equal ones tied; those
    /// out and forfeits after, as ever.
    #[test]
    fn those_still_in_rank_by_life_then_the_score() {
        let r = rank(&[
            e(1, summed(0.9, 5_900_000)),
            e(2, summed(1.0, 4_000_000)),
            e(3, summed(0.9, 5_900_001)),
            e(4, Run::Out { at: 40.0 }),
            e(5, summed(0.9, 5_900_000)),
            e(6, Run::Forfeit),
        ], RankBy::Life);
        assert_eq!(ids(&r), vec![vec![2], vec![3], vec![1, 5], vec![4], vec![6]]);
        // A negative score still ranks.
        let r = rank(&[e(1, summed(0.2, -300)), e(2, summed(0.2, -299))], RankBy::Life);
        assert_eq!(ids(&r), vec![vec![2], vec![1]]);
        // One still in without a score (an older build): everyone as
        // before round 3b, by life then the proportional part.
        let r = rank(&[
            e(1, summed(1.0, 9_000_000)),
            e(2, live(1.0, 0.9, None)),
        ], RankBy::Life);
        assert_eq!(ids(&r), vec![vec![2], vec![1]]);
        // The score travels in a report, and is left out where there is
        // none.
        let run = summed(1.0, 6_001_000);
        let reported = ReportedRun::of(run);
        assert_eq!(
            serde_json::to_string(&reported).unwrap(),
            r#"{"live":{"life_ppm":1000000,"accuracy_ppm":500000,"score":6001000}}"#
        );
        assert_eq!(reported.run(), run);
    }

    #[test]
    fn equal_life_and_score_are_split_by_the_absolute_score_of_the_same_notes() {
        // Life first, then the proportional score, whatever the absolute.
        let r = rank(&[
            e(1, live(0.9, 0.99, Some((10, 500)))),
            e(2, live(1.0, 0.5, Some((1, 500)))),
            e(3, live(0.9, 0.995, Some((5, 400)))),
        ], RankBy::Life);
        assert_eq!(ids(&r), vec![vec![2], vec![3], vec![1]]);
        // Equal life and score, the same notes: the absolute score (its
        // W0s) decides; the same one again shares the place.
        let r = rank(&[
            e(1, live(1.0, 1.0, Some((500_003, 5_000)))),
            e(2, live(1.0, 1.0, Some((500_010, 5_000)))),
            e(3, live(1.0, 1.0, Some((500_003, 5_000)))),
            e(4, live(1.0, 0.9, Some((999_999, 5_000)))),
        ], RankBy::Life);
        assert_eq!(ids(&r), vec![vec![2], vec![1, 3], vec![4]]);
        // Other notes (a CPU's tap-only copy against the full chart) do
        // not compare: the person shares with the best of the CPUs, the
        // CPU that lost its own tie is behind.
        let r = rank(&[
            e(1, live(1.0, 1.0, Some((600_000, 6_000)))),
            e(2, live(1.0, 1.0, Some((400_007, 4_000)))),
            e(3, live(1.0, 1.0, Some((400_002, 4_000)))),
        ], RankBy::Life);
        assert_eq!(ids(&r), vec![vec![1, 2], vec![3]]);
        // No absolute score known (a ruleset without one, an older
        // build's report): tied, as before.
        let r = rank(&[
            e(1, live(1.0, 1.0, None)),
            e(2, live(1.0, 1.0, Some((7, 4_000)))),
            e(3, live(1.0, 1.0, None)),
        ], RankBy::Life);
        assert_eq!(ids(&r), vec![vec![1, 2, 3]]);
    }

    #[test]
    fn the_preset_is_named_versioned_and_provisional() {
        let a = BattlePreset::GaugeAttacksV0;
        assert_eq!(BattlePreset::parse(a.id()), Some(a));
        assert!(a.attacks() && a.provisional());
        let p = BattlePreset::GaugeSurvivalV0;
        assert!(!p.attacks());
        assert_eq!(BattlePreset::parse(p.id()), Some(p));
        assert_eq!(BattlePreset::parse("provisional-standard"), None);
        assert!(p.provisional());
    }

    #[test]
    fn survivors_by_life_then_accuracy_then_the_out_later_first_then_forfeits() {
        let ranking = rank(&[
            e(1, Run::Out { at: 30.0 }),
            e(
                2,
                Run::Live {
                    life: 0.5,
                    accuracy: 0.9,
                    absolute: None,
                    score: None,
                    steps: 0,
                },
            ),
            e(3, Run::Forfeit),
            e(
                4,
                Run::Live {
                    life: 0.8,
                    accuracy: 0.7,
                    absolute: None,
                    score: None,
                    steps: 0,
                },
            ),
            e(5, Run::Out { at: 50.0 }),
            e(
                6,
                Run::Live {
                    life: 0.5,
                    accuracy: 0.95,
                    absolute: None,
                    score: None,
                    steps: 0,
                },
            ),
        ], RankBy::Life);
        assert_eq!(
            ids(&ranking),
            vec![vec![4], vec![6], vec![2], vec![5], vec![1], vec![3]]
        );
        assert_eq!(ranking[3][0].finish, Finish::Eliminated);
        assert_eq!(ranking[5][0].finish, Finish::Forfeit);
    }

    #[test]
    fn equal_runs_share_a_place() {
        let same = Run::Live {
            life: 1.0,
            accuracy: 0.9,
            absolute: None,
            score: None,
            steps: 0,
        };
        let ranking = rank(&[
            e(2, same),
            e(1, same),
            e(3, Run::Out { at: 10.0 }),
            e(4, Run::Out { at: 10.0 }),
            e(5, Run::Forfeit),
            e(6, Run::Forfeit),
        ], RankBy::Life);
        assert_eq!(ids(&ranking), vec![vec![1, 2], vec![3, 4], vec![5, 6]]);
    }

    #[test]
    fn decided_once_at_most_one_is_still_in() {
        let live = Run::Live {
            life: 0.2,
            accuracy: 0.5,
            absolute: None,
            score: None,
            steps: 0,
        };
        assert!(!decided(&[
            e(1, live),
            e(2, live),
            e(3, Run::Out { at: 1.0 })
        ]));
        assert!(decided(&[e(1, live), e(2, Run::Out { at: 1.0 })]));
        assert!(decided(&[
            e(1, Run::Out { at: 2.0 }),
            e(2, Run::Out { at: 2.0 })
        ]));
        // The last two out together share first: a draw, no sole winner.
        let r = rank(&[e(1, Run::Out { at: 2.0 }), e(2, Run::Out { at: 2.0 })], RankBy::Life);
        assert_eq!(ids(&r), vec![vec![1, 2]]);
    }
}
