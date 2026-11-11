//! Battle attacks' ledger (`docs/plans/multiplayer-prototype-progress.md`,
//! M8; the maintainer's decisions of 2026-11-11, D0–D8): each
//! competitor's attack power, earned by timing and credited at checkpoints
//! every four beats. Pure and chart-free, so the local stage runner
//! (`ddi_session::battle::attacks`) and later an online room's authority
//! keep the same ledger from the same reports; whole numbers only (power
//! units, microseconds), keyed by participant in order.
//!
//! This round (M8a) keeps the ledger and shows the power: nothing is
//! launched yet. Targeting, commitments, delivery and settlement come in
//! later rounds; [`AttackEvent`] and [`StageBattle::checkpoint`] are where
//! they go.
//!
//! PROVISIONAL: every number here is a working value
//! ([`ATTACKS_V0`], [`POWER_CLASSES_V0`]), named and versioned.

use crate::ids::ParticipantId;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// The attack rules a stage is played under: the bar's size, what an
/// attack spends, how often power is credited. PROVISIONAL.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct AttackRules {
    /// Names these values and their version in records.
    pub id: &'static str,
    /// The most power a competitor holds; what is earned past it is lost.
    pub cap: u32,
    /// What an attack spends (later rounds: a launch needs a full bar).
    pub cost: u32,
    /// Beats between checkpoints, where power is credited.
    pub checkpoint_beats: u32,
}

/// `attacks-v0` (PROVISIONAL): a bar of 100, an attack 100, checkpoints
/// every four beats. With [`POWER_CLASSES_V0`] a bar fills from 50 notes
/// in the best timing window, 100 in the next.
pub const ATTACKS_V0: AttackRules = AttackRules {
    id: "attacks-v0",
    cap: 100,
    cost: 100,
    checkpoint_beats: 4,
};

/// What each judgment earns, by the match's ruleset (D0, D2): power by
/// timing class, the tiers in the engine's order (`Judgement::index`: the
/// best first, misses last). Hold results, mines and stray presses earn
/// nothing (they have no timing).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PowerClasses {
    pub id: &'static str,
    table: &'static [(&'static str, [u8; 6])],
}

/// `power-classes-v0` (PROVISIONAL, D2): about the Fantastic window earns
/// 2, about the Excellent window 1, anything worse 0. ITG Fantastic 2 /
/// Excellent 1; ITG FA+ blue and white Fantastic 2 / Excellent 1; SM5
/// Flawless 2 / Perfect 1; DDR A, X and Extreme Marvelous 2 / Perfect 1;
/// FFR Amazing 2 / Perfect 1. A ruleset not listed earns nothing.
pub const POWER_CLASSES_V0: PowerClasses = PowerClasses {
    id: "power-classes-v0",
    table: &[
        ("itg", [2, 1, 0, 0, 0, 0]),
        ("itg-fa+", [2, 1, 0, 0, 0, 0]),
        ("sm5", [2, 1, 0, 0, 0, 0]),
        ("ddr-a", [2, 1, 0, 0, 0, 0]),
        ("ddr-x", [2, 1, 0, 0, 0, 0]),
        ("ddr-extreme", [2, 1, 0, 0, 0, 0]),
        ("ffr", [2, 1, 0, 0, 0, 0]),
    ],
};

impl PowerClasses {
    /// The power a judgment of `tier` earns under `ruleset`.
    pub fn earned(&self, ruleset: &str, tier: u8) -> u32 {
        self.table
            .iter()
            .find(|(id, _)| *id == ruleset)
            .and_then(|(_, t)| t.get(usize::from(tier)))
            .map_or(0, |&p| u32::from(p))
    }

    /// The rulesets it lists.
    pub fn rulesets(&self) -> impl Iterator<Item = &'static str> {
        self.table.iter().map(|(id, _)| *id)
    }
}

/// One judgment of a note of the original chart, as a competitor's play
/// judged it: when (judged song time, microseconds) and its tier
/// (`Judgement::index`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Judged {
    pub at_us: i64,
    pub tier: u8,
}

/// What a competitor's play reports at a checkpoint: its judgments since
/// the previous one (each before this checkpoint's time), and when it went
/// out, if it did. Power is the ledger's to compute, never reported.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Report {
    pub participant: ParticipantId,
    pub judged: Vec<Judged>,
    pub out_at_us: Option<i64>,
}

/// What a checkpoint changed that a view or a voice would tell.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AttackEvent {
    /// Its bar became full (later rounds: it waits for a target, then is
    /// spent at a commitment).
    Charged { participant: ParticipantId },
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
struct Seat {
    power: u32,
    /// Earned past the cap, lost.
    overflow: u32,
    out_at_us: Option<i64>,
}

/// The ledger after a checkpoint: what [`StageBattle::record`] keeps, so
/// that two runs of a stage (another frame rate, a replay) compare equal.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct CheckpointRecord {
    pub at_us: i64,
    /// Each competitor's power after it, in participant order.
    pub power: Vec<(ParticipantId, u32)>,
}

/// One stage's attack ledger.
#[derive(Clone, Debug)]
pub struct StageBattle {
    rules: AttackRules,
    classes: PowerClasses,
    ruleset: String,
    checkpoints_us: Vec<i64>,
    next: usize,
    seats: BTreeMap<ParticipantId, Seat>,
    record: Vec<CheckpointRecord>,
}

impl StageBattle {
    /// The ledger of a stage played under `ruleset` (the match's) by
    /// `participants`, its checkpoints at `checkpoints_us` (the chart's
    /// every `rules.checkpoint_beats` beats, in song microseconds; kept
    /// only while they rise).
    pub fn new(
        rules: AttackRules,
        classes: PowerClasses,
        ruleset: &str,
        checkpoints_us: Vec<i64>,
        participants: impl IntoIterator<Item = ParticipantId>,
    ) -> StageBattle {
        let mut rising: Vec<i64> = Vec::with_capacity(checkpoints_us.len());
        for t in checkpoints_us {
            if rising.last().is_none_or(|&l| t > l) {
                rising.push(t);
            }
        }
        StageBattle {
            rules,
            classes,
            ruleset: ruleset.into(),
            checkpoints_us: rising,
            next: 0,
            seats: participants
                .into_iter()
                .map(|p| (p, Seat::default()))
                .collect(),
            record: Vec::new(),
        }
    }

    pub fn rules(&self) -> AttackRules {
        self.rules
    }

    /// The next checkpoint to run and its time, if any is left.
    pub fn next_checkpoint(&self) -> Option<(usize, i64)> {
        self.checkpoints_us
            .get(self.next)
            .map(|&at| (self.next, at))
    }

    pub fn checkpoints_us(&self) -> &[i64] {
        &self.checkpoints_us
    }

    /// Checkpoint `k` (the next one) from everyone's `reports`: in order,
    /// those out before it are out; then each competitor still in is
    /// credited what its judgments earned, up to the cap (the rest lost).
    /// What it changed that is worth telling. A report of anyone not in
    /// the stage, or of a checkpoint not the next, changes nothing.
    pub fn checkpoint(&mut self, k: usize, reports: &[Report]) -> Vec<AttackEvent> {
        let Some((next, at)) = self.next_checkpoint() else {
            return Vec::new();
        };
        if k != next {
            return Vec::new();
        }
        self.next += 1;
        let mut events = Vec::new();
        // (1) Eliminations.
        for r in reports {
            if let (Some(seat), Some(out)) = (self.seats.get_mut(&r.participant), r.out_at_us)
                && out < at
                && seat.out_at_us.is_none()
            {
                seat.out_at_us = Some(out);
            }
        }
        // (2) Power, by timing (D2), capped.
        for r in reports {
            let Some(seat) = self.seats.get_mut(&r.participant) else {
                continue;
            };
            if seat.out_at_us.is_some() {
                continue;
            }
            let earned: u32 = r
                .judged
                .iter()
                .filter(|j| j.at_us < at)
                .map(|j| self.classes.earned(&self.ruleset, j.tier))
                .sum();
            let was = seat.power;
            let total = seat.power.saturating_add(earned);
            seat.power = total.min(self.rules.cap);
            seat.overflow = seat.overflow.saturating_add(total - seat.power);
            if was < self.rules.cap && seat.power >= self.rules.cap {
                events.push(AttackEvent::Charged {
                    participant: r.participant,
                });
            }
        }
        // (3)–(6) Recoveries, expiries, launches and commitments: later
        // rounds (M8 S4).
        self.record.push(CheckpointRecord {
            at_us: at,
            power: self.seats.iter().map(|(p, s)| (*p, s.power)).collect(),
        });
        events
    }

    /// `participant`'s power (0 for anyone not in the stage).
    pub fn power(&self, participant: ParticipantId) -> u32 {
        self.seats.get(&participant).map_or(0, |s| s.power)
    }

    /// Its power as a share of the bar, `0..=1`.
    pub fn fill(&self, participant: ParticipantId) -> f32 {
        (self.power(participant) as f32 / self.rules.cap.max(1) as f32).min(1.0)
    }

    /// Its bar is full (enough for an attack).
    pub fn charged(&self, participant: ParticipantId) -> bool {
        self.power(participant) >= self.rules.cost
    }

    /// What it earned past the cap, lost.
    pub fn overflow(&self, participant: ParticipantId) -> u32 {
        self.seats.get(&participant).map_or(0, |s| s.overflow)
    }

    /// Out of the stage, as the ledger has it.
    pub fn out(&self, participant: ParticipantId) -> bool {
        self.seats
            .get(&participant)
            .is_some_and(|s| s.out_at_us.is_some())
    }

    /// The ledger after each checkpoint run so far.
    pub fn record(&self) -> &[CheckpointRecord] {
        &self.record
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const A: ParticipantId = ParticipantId(1);
    const B: ParticipantId = ParticipantId(2);

    fn ledger() -> StageBattle {
        StageBattle::new(
            ATTACKS_V0,
            POWER_CLASSES_V0,
            "itg-fa+",
            vec![2_000_000, 4_000_000, 6_000_000, 8_000_000],
            [A, B],
        )
    }

    fn report(p: ParticipantId, tiers: &[(i64, u8)]) -> Report {
        Report {
            participant: p,
            judged: tiers
                .iter()
                .map(|&(at_us, tier)| Judged { at_us, tier })
                .collect(),
            out_at_us: None,
        }
    }

    #[test]
    fn the_classes_are_named_and_cover_every_match_ruleset() {
        assert_eq!(POWER_CLASSES_V0.id, "power-classes-v0");
        for id in crate::MatchRules::RULESETS {
            assert_eq!(POWER_CLASSES_V0.earned(id, 0), 2, "{id}");
            assert_eq!(POWER_CLASSES_V0.earned(id, 1), 1, "{id}");
            for tier in 2..6 {
                assert_eq!(POWER_CLASSES_V0.earned(id, tier), 0, "{id}");
            }
        }
        assert_eq!(POWER_CLASSES_V0.earned("danoni", 0), 0);
        assert_eq!(POWER_CLASSES_V0.earned("itg", 9), 0);
    }

    #[test]
    fn power_is_earned_by_timing_and_credited_at_checkpoints() {
        let mut l = ledger();
        assert_eq!(l.next_checkpoint(), Some((0, 2_000_000)));
        // Two best, one next best, two worse; one after the checkpoint
        // waits for the next.
        let r = report(A, &[(100, 0), (200, 0), (300, 1), (400, 2), (500, 5)]);
        assert!(l.checkpoint(0, &[r, report(B, &[(1_000, 1)])]).is_empty());
        assert_eq!((l.power(A), l.power(B)), (5, 1));
        assert!(!l.charged(A));
        // Not the next checkpoint: nothing.
        assert!(l.checkpoint(0, &[report(A, &[(2_100_000, 0)])]).is_empty());
        assert_eq!(l.power(A), 5);
        let late = report(A, &[(3_000_000, 0), (4_000_000, 0)]);
        l.checkpoint(1, &[late]);
        assert_eq!(
            l.power(A),
            7,
            "one at the checkpoint's time is not before it"
        );
        assert_eq!(l.record().len(), 2);
        assert_eq!(l.record()[1].power, vec![(A, 7), (B, 1)]);
    }

    #[test]
    fn the_bar_caps_at_a_hundred_and_the_overflow_is_lost() {
        let mut l = ledger();
        let many: Vec<(i64, u8)> = (0..60).map(|i| (i * 1_000, 0)).collect();
        let events = l.checkpoint(0, &[report(A, &many)]);
        assert_eq!(events, vec![AttackEvent::Charged { participant: A }]);
        assert_eq!((l.power(A), l.overflow(A)), (100, 20));
        assert!(l.charged(A));
        assert_eq!(l.fill(A), 1.0);
        // Full already: no second Charged, the rest lost too.
        let more = report(A, &[(2_500_000, 0)]);
        assert!(l.checkpoint(1, &[more]).is_empty());
        assert_eq!((l.power(A), l.overflow(A)), (100, 22));
    }

    #[test]
    fn those_out_earn_nothing_more() {
        let mut l = ledger();
        let mut r = report(A, &[(100, 0)]);
        r.out_at_us = Some(1_500_000);
        l.checkpoint(0, &[r]);
        assert!(l.out(A));
        assert_eq!(l.power(A), 0);
        // Out after the checkpoint: still in at it.
        let mut r = report(B, &[(100, 0)]);
        r.out_at_us = Some(2_000_000);
        l = ledger();
        l.checkpoint(0, &[r]);
        assert!(!l.out(B));
        assert_eq!(l.power(B), 2);
    }

    #[test]
    fn checkpoints_only_rise() {
        let l = StageBattle::new(
            ATTACKS_V0,
            POWER_CLASSES_V0,
            "itg",
            vec![1, 1, 3, 2, 4],
            [A],
        );
        assert_eq!(l.checkpoints_us(), &[1, 3, 4]);
    }
}
