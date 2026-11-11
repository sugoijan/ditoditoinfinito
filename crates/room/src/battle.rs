//! The battle a stage is played under, by name and version, and how its
//! judged runs become a confirmed ranking. Pure: the stage runner
//! (`ddi_session::battle`) judges the plays and hands the runs here; a
//! server can rank the same way.
//!
//! One preset exists, PROVISIONAL: [`BattlePreset::GaugeSurvivalV0`], the
//! client study's default for the first playable slice
//! (`docs/plans/multiplayer-prototype-progress.md`, M3). Life is the
//! ruleset's gauge; an empty gauge is out of the stage (never the match,
//! unless the match's rules say so); no attacks, no mines added, no
//! targeting. It stands in for the design's standard battle (life, attack
//! power, recent-accuracy weakening, `docs/plans/multiplayer.md`) until
//! that is built, and is never presented as it.

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
    /// PROVISIONAL, DEVELOPMENT ONLY in M8a: [`BattlePreset::GaugeSurvivalV0`]
    /// with attack power (`crate::attacks`, [`crate::attacks::ATTACKS_V0`]):
    /// each competitor earns power by timing, shown as a bar above life.
    /// Nothing is launched yet (later rounds); ranked by [`rank`] as
    /// survival. Reached only by the shells' development switches
    /// (`--mp-dev-attacks`, the web's `mpattacks=1`).
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
    /// gauge's life (`0..=1`) and their score as a fraction of the chart's
    /// best (`0..=1`; the ruleset's percent, money or EX).
    Live { life: f64, accuracy: f64 },
    /// Their gauge emptied at judged song second `at`.
    Out { at: f64 },
    /// Left the stage while competing (a quit, an interruption).
    Forfeit,
}

/// A run as a member reports it to an online room (M5d,
/// `Command::ReportOutcome`): [`Run`] in whole numbers (millionths of life
/// and accuracy, milliseconds), so the room's state stays comparable and
/// every authority ranks it alike. SELF-REPORTED: the room cannot check it
/// (`docs/plans/multiplayer-prototype-progress.md`, its trust limits).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ReportedRun {
    Live { life_ppm: u32, accuracy_ppm: u32 },
    Out { at_ms: u64 },
    Forfeit,
}

impl ReportedRun {
    pub const SCALE: f64 = 1_000_000.0;

    pub fn of(run: Run) -> ReportedRun {
        let ppm = |x: f64| (x.clamp(0.0, 1.0) * Self::SCALE).round() as u32;
        match run {
            Run::Live { life, accuracy } => ReportedRun::Live {
                life_ppm: ppm(life),
                accuracy_ppm: ppm(accuracy),
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
            } => Run::Live {
                life: f64::from(life_ppm) / Self::SCALE,
                accuracy: f64::from(accuracy_ppm) / Self::SCALE,
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

/// [`BattlePreset::GaugeSurvivalV0`]'s ranking, best first, each group
/// tied (`docs/plans/multiplayer.md`, "Elimination and a truthful
/// result"): those still in by life, then accuracy (both equal: tied);
/// then those out, later first (the same second: tied); then forfeits,
/// tied. Participants are ordered by id within a group.
pub fn rank(entrants: &[Entrant]) -> Vec<Vec<Finisher>> {
    let mut live: Vec<(f64, f64, ParticipantId)> = Vec::new();
    let mut out: Vec<(f64, ParticipantId)> = Vec::new();
    let mut forfeit: Vec<ParticipantId> = Vec::new();
    for e in entrants {
        match e.run {
            Run::Live { life, accuracy } => live.push((life, accuracy, e.participant)),
            Run::Out { at } => out.push((at, e.participant)),
            Run::Forfeit => forfeit.push(e.participant),
        }
    }
    live.sort_by(|a, b| {
        b.0.total_cmp(&a.0)
            .then(b.1.total_cmp(&a.1))
            .then(a.2.cmp(&b.2))
    });
    out.sort_by(|a, b| b.0.total_cmp(&a.0).then(a.1.cmp(&b.1)));
    forfeit.sort();
    let mut ranking: Vec<Vec<Finisher>> = Vec::new();
    let finisher = |participant, finish| Finisher {
        participant,
        finish,
    };
    let mut last: Option<(f64, f64)> = None;
    for (life, accuracy, id) in live {
        let f = finisher(id, Finish::Survived);
        match ranking.last_mut() {
            Some(group) if last == Some((life, accuracy)) => group.push(f),
            _ => ranking.push(vec![f]),
        }
        last = Some((life, accuracy));
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
        });
        assert_eq!(
            live,
            ReportedRun::Live {
                life_ppm: 500_000,
                accuracy_ppm: 912_346
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
        assert_eq!(
            serde_json::to_string(&live).unwrap(),
            r#"{"live":{"life_ppm":500000,"accuracy_ppm":912346}}"#
        );
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
                },
            ),
            e(3, Run::Forfeit),
            e(
                4,
                Run::Live {
                    life: 0.8,
                    accuracy: 0.7,
                },
            ),
            e(5, Run::Out { at: 50.0 }),
            e(
                6,
                Run::Live {
                    life: 0.5,
                    accuracy: 0.95,
                },
            ),
        ]);
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
        };
        let ranking = rank(&[
            e(2, same),
            e(1, same),
            e(3, Run::Out { at: 10.0 }),
            e(4, Run::Out { at: 10.0 }),
            e(5, Run::Forfeit),
            e(6, Run::Forfeit),
        ]);
        assert_eq!(ids(&ranking), vec![vec![1, 2], vec![3, 4], vec![5, 6]]);
    }

    #[test]
    fn decided_once_at_most_one_is_still_in() {
        let live = Run::Live {
            life: 0.2,
            accuracy: 0.5,
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
        let r = rank(&[e(1, Run::Out { at: 2.0 }), e(2, Run::Out { at: 2.0 })]);
        assert_eq!(ids(&r), vec![vec![1, 2]]);
    }
}
