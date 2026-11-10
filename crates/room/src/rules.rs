//! Room policy and match rules. The room's policy holds operational values
//! (the competitor cap, readiness timing); a match's rules are locked when
//! the match starts. The next match's rules are an explicit draft the room
//! keeps beside the running match's (`state::NextMatchDraft`).

use crate::awards::AwardScheme;
use crate::member::OpponentSpec;
use serde::{Deserialize, Serialize};

/// Operational limits and timing for one room. These are policy values,
/// checked in one place each, not array sizes or UI timers.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct RoomPolicy {
    /// The competitor cap, humans and CPUs together. Nine (the maintainer,
    /// 2026-11-11: eight opponents beside the player; first eight,
    /// `INITIAL_COMPETITOR_CAP`); nothing else in the model assumes it.
    pub max_competitors: u16,
    /// Room members who are not administrators, watchers included.
    pub max_members: u16,
    /// Extra room for administrators entering to help, so a full room never
    /// shuts them out. Bounded; the real limit is a server-capacity decision.
    pub admin_reserve: u16,
    /// The longest match a rules draft may ask for.
    pub max_stages: u16,
    /// Finished matches kept as records in the room.
    pub history: u16,
    pub readiness: ReadinessPolicy,
    /// When an online stage settles on its members' reports (M5d).
    #[serde(default)]
    pub outcomes: OutcomePolicy,
}

impl RoomPolicy {
    /// The operational cap (`docs/plans/multiplayer.md`, "Eight first and
    /// larger parties later"; raised to nine, eight opponents beside the
    /// player, `docs/plans/multiplayer-prototype-progress.md`, M6b).
    pub const INITIAL_COMPETITOR_CAP: u16 = 9;

    /// DEVELOPMENT ONLY: a practice room's cap for trying far more CPUs
    /// than a match allows (`docs/plans/multiplayer-prototype-progress.md`,
    /// M6b). Never a hosted room's, never reachable from the game's menus.
    pub const DEVELOPMENT_COMPETITOR_CAP: u16 = 129;

    /// DEVELOPMENT ONLY: this policy with the cap raised to
    /// [`RoomPolicy::DEVELOPMENT_COMPETITOR_CAP`].
    pub fn development() -> Self {
        Self {
            max_competitors: Self::DEVELOPMENT_COMPETITOR_CAP,
            ..Self::default()
        }
    }

    /// The one place the competitor cap is checked: reservations, CPU seats
    /// and rules all come through here.
    pub fn check_competitors(&self, requested: usize) -> Result<(), CapExceeded> {
        if requested > usize::from(self.max_competitors) {
            Err(CapExceeded {
                requested: requested.try_into().unwrap_or(u16::MAX),
                cap: self.max_competitors,
            })
        } else {
            Ok(())
        }
    }
}

impl Default for RoomPolicy {
    fn default() -> Self {
        Self {
            max_competitors: Self::INITIAL_COMPETITOR_CAP,
            max_members: 32,
            admin_reserve: 4,
            max_stages: 16,
            history: 8,
            readiness: ReadinessPolicy::default(),
            outcomes: OutcomePolicy::default(),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct CapExceeded {
    pub requested: u16,
    pub cap: u16,
}

/// How a host-triggered start treats unready people
/// (`docs/plans/multiplayer.md`, "Hosted readiness without an indefinite
/// wait"): Start opens a window when someone eligible is unready; at its
/// deadline the ready and loaded start and the others sit out the stage
/// with zero points. Nobody's unreadiness vetoes the start. PROVISIONAL:
/// every number here is a working value, not a decided one.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReadinessPolicy {
    /// Names this policy and its version in records and on the wire.
    pub id: String,
    /// The readiness window. 15 s is the studies' test value.
    pub window_ms: u64,
    /// The committed start's countdown.
    pub countdown_ms: u64,
    /// Ready and loaded people needed to start (a window or at once).
    pub min_ready_humans: u16,
    /// Competitors needed to start, CPUs included.
    pub min_competitors: u16,
}

impl ReadinessPolicy {
    pub const HOSTED_DEADLINE_V0: &'static str = "hosted-deadline-v0";
}

impl Default for ReadinessPolicy {
    fn default() -> Self {
        Self {
            id: Self::HOSTED_DEADLINE_V0.into(),
            window_ms: 15_000,
            countdown_ms: 3_000,
            min_ready_humans: 1,
            min_competitors: 2,
        }
    }
}

/// When a stage played on its members' own devices (an online room's: no
/// CPUs) settles: once every starter reported its run
/// (`Command::ReportOutcome`), or at a deadline, those who never reported
/// then counted as forfeits. PROVISIONAL (`self-reported-v0`): the song's
/// length is not the room's to know, so the deadline is a wait after the
/// first report of a song played to its end (each device starts its play
/// on its own Start press after the countdown, so it covers a late press
/// too), and a cap from the start for a stage nobody finished.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct OutcomePolicy {
    pub id: String,
    /// After the first report of the song played to its end.
    pub after_song_end_ms: u64,
    /// After the stage's start, whatever was reported.
    pub stage_cap_ms: u64,
}

impl OutcomePolicy {
    pub const SELF_REPORTED_V0: &'static str = "self-reported-v0";
}

impl Default for OutcomePolicy {
    fn default() -> Self {
        Self {
            id: Self::SELF_REPORTED_V0.into(),
            after_song_end_ms: 60_000,
            stage_cap_ms: 20 * 60_000,
        }
    }
}

/// Where a stage's content comes from, which decides whether choosing it
/// makes it available at once.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ContentSource {
    /// Local practice: the player's own library. Chosen content is available.
    LocalLibrary,
    /// A hosted room: chosen content is pending until the authority confirms
    /// it was shared.
    Shared,
}

/// Room access: future admission only. Changing it never removes members,
/// changes match rules or clears readiness.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Access {
    pub admission: Admission,
    /// Bumped to replace the invite; the authority keeps the secret itself.
    pub invite_generation: u32,
}

impl Default for Access {
    fn default() -> Self {
        Self {
            admission: Admission::Open,
            invite_generation: 1,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Admission {
    /// Newcomers with the invite may enter.
    Open,
    /// No new members; present and returning members are unaffected.
    Closed,
}

/// One match's rules, locked from its start to its end.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct MatchRules {
    /// Songs in the match. Four is a working default, not a limit.
    pub stages: u16,
    /// Playing seats, humans and CPUs. At most the room's competitor cap.
    pub seats: u16,
    pub cpu: CpuRules,
    /// Everyone returns at the next stage after an elimination (default).
    /// Off: elimination, a forfeit or a missed stage ends the match for them.
    pub return_each_stage: bool,
    /// Newcomers may enter at a later stage's boundary into a vacant seat,
    /// with zero points for the stages they missed. Default Off.
    pub mid_match_entry: bool,
    /// The battle preset, opaque to the room (attacks, defense, gauge).
    pub battle: String,
    pub awards: AwardScheme,
}

impl MatchRules {
    pub const PROVISIONAL_BATTLE: &'static str = "provisional-standard";

    /// Local CPU practice: one player and three CPUs, four stages, the
    /// battle the practice stages are played under (PROVISIONAL:
    /// `BattlePreset::GaugeSurvivalV0`).
    pub fn practice(opponent: OpponentSpec) -> Self {
        Self {
            stages: 4,
            seats: 4,
            cpu: CpuRules {
                mode: CpuMode::Fixed { count: 3 },
                opponent,
            },
            battle: crate::battle::BattlePreset::GAUGE_SURVIVAL_V0.into(),
            ..Self::hosted()
        }
    }

    /// A hosted room's starting rules: four stages, eight seats, no CPUs.
    pub fn hosted() -> Self {
        Self {
            stages: 4,
            seats: RoomPolicy::INITIAL_COMPETITOR_CAP,
            cpu: CpuRules {
                mode: CpuMode::Off,
                opponent: OpponentSpec {
                    level: 0,
                    seed: 0,
                    personality: None,
                },
            },
            return_each_stage: true,
            mid_match_entry: false,
            battle: Self::PROVISIONAL_BATTLE.into(),
            awards: AwardScheme::PlacementPointsV1,
        }
    }

    /// How many CPUs a match with `humans` admitted people would seat.
    pub fn cpu_seats(&self, humans: usize) -> usize {
        match self.cpu.mode {
            CpuMode::Off => 0,
            CpuMode::Fixed { count } => usize::from(count),
            CpuMode::Fill => usize::from(self.seats).saturating_sub(humans),
        }
    }

    /// Check these rules for a match of the `humans` people who want to
    /// play. Run when the draft is saved, again at the match boundary and
    /// at the start, since reservations change in between.
    pub fn validate(&self, policy: &RoomPolicy, humans: usize) -> Result<(), RulesProblem> {
        if self.stages == 0 || self.stages > policy.max_stages {
            return Err(RulesProblem::StageCount {
                stages: self.stages,
                max: policy.max_stages,
            });
        }
        if let Err(cap) = policy.check_competitors(self.seats.into()) {
            return Err(RulesProblem::OverCap(cap));
        }
        if self.seats < policy.readiness.min_competitors {
            return Err(RulesProblem::TooFewSeats {
                seats: self.seats,
                min: policy.readiness.min_competitors,
            });
        }
        if !self.return_each_stage && self.mid_match_entry {
            return Err(RulesProblem::EliminationWithLateEntry);
        }
        let seats = usize::from(self.seats);
        if humans > seats {
            return Err(RulesProblem::HumansExceedSeats {
                humans: humans.try_into().unwrap_or(u16::MAX),
                seats: self.seats,
            });
        }
        let cpus = self.cpu_seats(humans);
        if humans + cpus > seats {
            return Err(RulesProblem::CpusExceedSeats {
                humans: humans.try_into().unwrap_or(u16::MAX),
                cpus: cpus.try_into().unwrap_or(u16::MAX),
                seats: self.seats,
            });
        }
        Ok(())
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct CpuRules {
    pub mode: CpuMode,
    /// The common profile of the match's CPUs; each seat gets its own seed.
    pub opponent: OpponentSpec,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CpuMode {
    Off,
    Fixed {
        count: u16,
    },
    /// Fill the seats people leave open at the roster's lock.
    Fill,
}

/// Why a set of rules cannot be used (now).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RulesProblem {
    StageCount {
        stages: u16,
        max: u16,
    },
    OverCap(CapExceeded),
    TooFewSeats {
        seats: u16,
        min: u16,
    },
    /// Persistent elimination and mid-match entry cannot be combined: a
    /// late entrant would skip the elimination that defines the format.
    EliminationWithLateEntry,
    /// More people want to play than the rules seat.
    HumansExceedSeats {
        humans: u16,
        seats: u16,
    },
    /// A fixed CPU count leaves no room for the people who want to play.
    CpusExceedSeats {
        humans: u16,
        cpus: u16,
        seats: u16,
    },
}

#[cfg(test)]
mod tests {
    use super::*;

    fn spec() -> OpponentSpec {
        OpponentSpec {
            level: 2,
            seed: 1,
            personality: None,
        }
    }

    #[test]
    fn the_cap_is_one_policy_check() {
        let policy = RoomPolicy::default();
        assert!(policy.check_competitors(9).is_ok());
        assert_eq!(
            policy.check_competitors(10),
            Err(CapExceeded {
                requested: 10,
                cap: 9
            })
        );
        let mut rules = MatchRules::hosted();
        rules.seats = 10;
        assert_eq!(
            rules.validate(&policy, 1),
            Err(RulesProblem::OverCap(CapExceeded {
                requested: 10,
                cap: 9
            }))
        );
        // A larger policy is a value, not an architecture change.
        let large = RoomPolicy {
            max_competitors: 128,
            ..RoomPolicy::default()
        };
        assert!(rules.validate(&large, 1).is_ok());
    }

    #[test]
    fn rules_are_checked_against_the_people_who_want_to_play() {
        let policy = RoomPolicy::default();
        let practice = MatchRules::practice(spec());
        assert!(practice.validate(&policy, 1).is_ok());
        assert_eq!(
            practice.validate(&policy, 2),
            Err(RulesProblem::CpusExceedSeats {
                humans: 2,
                cpus: 3,
                seats: 4
            })
        );
        let mut fill = MatchRules::hosted();
        fill.cpu.mode = CpuMode::Fill;
        fill.seats = 4;
        assert_eq!(fill.cpu_seats(3), 1);
        assert_eq!(
            fill.validate(&policy, 5),
            Err(RulesProblem::HumansExceedSeats {
                humans: 5,
                seats: 4
            })
        );
        let mut both = MatchRules::hosted();
        both.return_each_stage = false;
        both.mid_match_entry = true;
        assert_eq!(
            both.validate(&policy, 1),
            Err(RulesProblem::EliminationWithLateEntry)
        );
        let mut none = MatchRules::hosted();
        none.stages = 0;
        assert!(matches!(
            none.validate(&policy, 1),
            Err(RulesProblem::StageCount { .. })
        ));
    }
}
