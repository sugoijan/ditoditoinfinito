//! A stage's battle plan: what the room needs of the chart to run the
//! attack ledger, which it cannot read itself. The picker's device
//! computes it once the pick is accepted (the checkpoints every four beats
//! and the chart's safe sections, `ddi_session::battle::attacks`, from the
//! stage's seed, [`seed_for`]) and offers it to the room
//! (`ClientLive::OfferPlan`); the room checks its digest and bounds, keeps
//! it and says its digest to everyone (`Event::BattlePlanSet`). Every other
//! device computes its own from the same chart, rules and seed and compares
//! the digests before it says it is ready to play live; a mismatch is a
//! content mismatch, as a chart digest's is (it sits the stage out).
//!
//! The digest is FNV-1a (64 bits) over a canonical binary form of the plan:
//! it catches two devices that disagree, not a forger (results are
//! self-reported anyway).

use crate::attacks::{Schedule, Section};
use crate::ids::{ChartKey, RoomId, StageId};
use serde::{Deserialize, Serialize};
use std::fmt::Write as _;

/// A plan's digest as text: `fnv1a64:` and sixteen hex digits.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct PlanDigest(pub String);

impl PlanDigest {
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// Why a plan was refused.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PlanProblem {
    /// Its digest is not its contents'.
    Digest,
    UnknownVersion,
    /// More slots, checkpoints or cells than [`BattlePlan`]'s bounds.
    TooLarge,
    /// Checkpoints or slots not rising, or a section's cells out of order.
    Order,
}

/// The chart's facts the room's ledger runs on (`crate::attacks`).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct BattlePlan {
    pub version: u32,
    /// The ids it was made under, for the record and the comparison:
    /// `crate::attacks::ATTACKS`'s, the engine's placement's
    /// (`placement-v2`), the power classes' and the match's ruleset.
    pub attack_rules: String,
    pub placement: String,
    pub classes: String,
    pub ruleset: String,
    /// The stage's seed ([`seed_for`]): the sections' draw and the
    /// ledger's ties.
    pub seed: u64,
    /// Every checkpoint's song time, µs (every four beats from the fourth
    /// to four beats past the last note).
    pub checkpoints_us: Vec<i64>,
    pub schedule: Schedule,
    /// The stage's ladder where its zero life steps down (a match's "At
    /// zero life: Step down"): each rung's chart digest, the next easier
    /// first, so that every device proves it has the same charts (the
    /// sections are then placed over every rung at once). ADDITIVE: empty
    /// (left out, and out of the digest) where zero life is out, so a
    /// plan without it is what it was, digest and all.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub ladder: Vec<String>,
    /// Where the stage is played under per-stage rules changed during the
    /// match (`state::Match::changes`, 2026-11-13): those rules in a few
    /// plain words (the client's `StageRules::changed_tag`), so that a
    /// device that would play it under the match's own rules (one that
    /// predates the change) disagrees on the digest and sits the stage
    /// out. ADDITIVE: `None` (left out, and out of the digest) for a
    /// stage under the match's rules, whose plan is what it was.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub stage_rules: Option<String>,
    /// The match's Attacks at once for the stage (`attacks-v3`,
    /// [`crate::AttacksAtOnce::limit`]): the most attacks that may stand
    /// placed against one competitor. `None`: no limit, and for rules
    /// before `attacks-v3`, which never read it. ADDITIVE: left out (and
    /// out of the digest) while `None`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub at_once: Option<u32>,
    pub digest: PlanDigest,
}

impl BattlePlan {
    pub const VERSION: u32 = 1;
    /// Bounds a room takes (a song of more than half an hour at 240 BPM
    /// is past them).
    pub const MAX_CHECKPOINTS: usize = 2_048;
    pub const MAX_SLOTS: usize = 2_048;
    pub const MAX_CELLS: usize = 16;

    /// A plan of these facts, its digest computed.
    pub fn new(
        attack_rules: &str,
        placement: &str,
        classes: &str,
        ruleset: &str,
        seed: u64,
        checkpoints_us: Vec<i64>,
        schedule: Schedule,
    ) -> BattlePlan {
        let mut plan = BattlePlan {
            version: Self::VERSION,
            attack_rules: attack_rules.into(),
            placement: placement.into(),
            classes: classes.into(),
            ruleset: ruleset.into(),
            seed,
            checkpoints_us,
            schedule,
            ladder: Vec::new(),
            stage_rules: None,
            at_once: None,
            digest: PlanDigest(String::new()),
        };
        plan.digest = plan.compute_digest();
        plan
    }

    /// The plan with the stage's ladder (each rung's chart digest), its
    /// digest computed again.
    pub fn with_ladder(mut self, ladder: Vec<String>) -> BattlePlan {
        self.ladder = ladder;
        self.digest = self.compute_digest();
        self
    }

    /// The plan with the stage's changed rules named
    /// ([`BattlePlan::stage_rules`]), its digest computed again.
    pub fn with_stage_rules(mut self, rules: Option<String>) -> BattlePlan {
        self.stage_rules = rules;
        self.digest = self.compute_digest();
        self
    }

    /// The plan with the stage's Attacks at once ([`BattlePlan::at_once`]),
    /// its digest computed again.
    pub fn with_at_once(mut self, at_once: Option<u32>) -> BattlePlan {
        self.at_once = at_once;
        self.digest = self.compute_digest();
        self
    }

    /// The attack rules the ledger runs the stage under: those it names,
    /// with its Attacks at once (`None` for rules this build does not
    /// know).
    pub fn rules(&self) -> Option<crate::attacks::AttackRules> {
        crate::attacks::attack_rules(&self.attack_rules).map(|r| r.with_at_once(self.at_once))
    }

    /// The digest of its contents (everything but the digest).
    pub fn compute_digest(&self) -> PlanDigest {
        let mut h = Fnv::new();
        h.u64(u64::from(self.version));
        for s in [
            &self.attack_rules,
            &self.placement,
            &self.classes,
            &self.ruleset,
        ] {
            h.bytes(s.as_bytes());
        }
        h.u64(self.seed);
        h.u64(self.checkpoints_us.len() as u64);
        for &c in &self.checkpoints_us {
            h.i64(c);
        }
        h.u64(self.schedule.slots.len() as u64);
        for slot in &self.schedule.slots {
            h.i64(slot.start_us);
            match &slot.section {
                None => h.u64(0),
                Some(Section {
                    cells,
                    clear_us,
                    recover_after_us,
                }) => {
                    h.u64(1 + cells.len() as u64);
                    for c in cells {
                        h.i64(c.tick);
                        h.u64(u64::from(c.lane));
                        h.i64(c.at_us);
                    }
                    h.i64(*clear_us);
                    h.i64(*recover_after_us);
                }
            }
        }
        // The ladder only where there is one: a plan without it keeps
        // the digest it had.
        if !self.ladder.is_empty() {
            h.u64(0x5354_4550);
            h.u64(self.ladder.len() as u64);
            for d in &self.ladder {
                h.bytes(d.as_bytes());
            }
        }
        // The stage's changed rules only where there are some, as the
        // ladder.
        if let Some(r) = &self.stage_rules {
            h.u64(0x5255_4c45);
            h.bytes(r.as_bytes());
        }
        // Attacks at once only where there is a limit, as the ladder.
        if let Some(n) = self.at_once {
            h.u64(0x4f4e_4345);
            h.u64(u64::from(n));
        }
        let mut text = String::from("fnv1a64:");
        let _ = write!(text, "{:016x}", h.0);
        PlanDigest(text)
    }

    /// Whether a room takes it: its digest its own, within the bounds,
    /// everything in order.
    pub fn check(&self) -> Result<(), PlanProblem> {
        if self.version != Self::VERSION {
            return Err(PlanProblem::UnknownVersion);
        }
        if self.checkpoints_us.len() > Self::MAX_CHECKPOINTS
            || self.schedule.slots.len() > Self::MAX_SLOTS
            || [
                &self.attack_rules,
                &self.placement,
                &self.classes,
                &self.ruleset,
            ]
            .iter()
            .any(|s| s.len() > 64)
            || self.stage_rules.as_ref().is_some_and(|r| r.len() > 64)
        {
            return Err(PlanProblem::TooLarge);
        }
        if self.schedule.slots.iter().any(|s| {
            s.section
                .as_ref()
                .is_some_and(|c| c.cells.len() > Self::MAX_CELLS)
        }) {
            return Err(PlanProblem::TooLarge);
        }
        let rising = |v: Vec<i64>| v.windows(2).all(|w| w[1] > w[0]);
        if !rising(self.checkpoints_us.clone())
            || !rising(self.schedule.slots.iter().map(|s| s.start_us).collect())
        {
            return Err(PlanProblem::Order);
        }
        for s in self
            .schedule
            .slots
            .iter()
            .filter_map(|s| s.section.as_ref())
        {
            if s.cells.windows(2).any(|w| w[1].at_us < w[0].at_us)
                || s.cells.iter().any(|c| c.at_us > s.clear_us)
            {
                return Err(PlanProblem::Order);
            }
        }
        if self.compute_digest() != self.digest {
            return Err(PlanProblem::Digest);
        }
        Ok(())
    }
}

/// The seed of an online stage's battle: from the room, the stage and its
/// chart, so every device knows it and nobody chooses it.
pub fn seed_for(room: &RoomId, stage: StageId, chart: &ChartKey) -> u64 {
    let mut h = Fnv::new();
    h.bytes(b"ddi-stage-seed");
    h.bytes(room.as_str().as_bytes());
    h.u64(stage.0);
    h.bytes(chart.as_str().as_bytes());
    h.0
}

struct Fnv(u64);

impl Fnv {
    fn new() -> Self {
        Fnv(0xcbf2_9ce4_8422_2325)
    }
    fn byte(&mut self, b: u8) {
        self.0 ^= u64::from(b);
        self.0 = self.0.wrapping_mul(0x0000_0100_0000_01b3);
    }
    fn u64(&mut self, x: u64) {
        for b in x.to_le_bytes() {
            self.byte(b);
        }
    }
    fn i64(&mut self, x: i64) {
        self.u64(x as u64);
    }
    fn bytes(&mut self, b: &[u8]) {
        self.u64(b.len() as u64);
        for &x in b {
            self.byte(x);
        }
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use crate::attacks::{Cell, Slot};

    /// A plan of `n` slots of 2 s, each with a section of eight shocks a
    /// quarter second apart where `room(j)`, checkpoints every 2 s.
    pub(crate) fn plan(n: usize, seed: u64, room: impl Fn(usize) -> bool) -> BattlePlan {
        const S: i64 = 1_000_000;
        let slots = (0..n)
            .map(|j| {
                let start_us = j as i64 * 2 * S;
                Slot {
                    start_us,
                    section: room(j).then(|| {
                        let cells: Vec<Cell> = (0..8)
                            .map(|i| Cell {
                                tick: (j as i64 * 4 * 48) + i * 24,
                                lane: (i % 4) as u8,
                                at_us: start_us + i * S / 4,
                            })
                            .collect();
                        let last = cells.last().unwrap().at_us;
                        Section {
                            clear_us: last + 100_000,
                            recover_after_us: last + 100_000,
                            cells,
                        }
                    }),
                }
            })
            .collect();
        BattlePlan::new(
            crate::attacks::ATTACKS.id,
            "placement-v2",
            crate::attacks::POWER_CLASSES_V0.id,
            "ddi",
            seed,
            (1..=n as i64).map(|k| k * 2 * S).collect(),
            Schedule { slots },
        )
    }

    #[test]
    fn a_plan_is_checked_by_its_digest_and_bounds() {
        let p = plan(40, 7, |j| j % 3 != 0);
        assert!(p.digest.as_str().starts_with("fnv1a64:"));
        assert_eq!(p.check(), Ok(()));
        // The same facts, the same digest; another seed, another.
        assert_eq!(plan(40, 7, |j| j % 3 != 0).digest, p.digest);
        assert_ne!(plan(40, 8, |j| j % 3 != 0).digest, p.digest);
        // A shock moved: the digest no longer its contents'.
        let mut moved = p.clone();
        moved.schedule.slots[1].section.as_mut().unwrap().cells[0].lane = 3;
        assert_eq!(moved.check(), Err(PlanProblem::Digest));
        let mut back = p.clone();
        back.checkpoints_us.swap(0, 1);
        assert_eq!(back.check(), Err(PlanProblem::Order));
        let big = plan(BattlePlan::MAX_SLOTS + 1, 7, |_| false);
        assert_eq!(big.check(), Err(PlanProblem::TooLarge));
        // Read back from JSON, the same plan.
        let text = serde_json::to_string(&p).unwrap();
        assert_eq!(serde_json::from_str::<BattlePlan>(&text).unwrap(), p);
    }

    /// A plan without a ladder keeps the digest it had; one with a ladder
    /// is told apart by it, and its JSON names it.
    #[test]
    fn a_ladder_is_in_the_digest_only_where_there_is_one() {
        let p = plan(40, 7, |j| j % 3 != 0);
        let before = p.digest.clone();
        assert_eq!(p.clone().with_ladder(Vec::new()).digest, before);
        let json = serde_json::to_value(&p).unwrap();
        assert!(json.get("ladder").is_none());
        let laddered = p.clone().with_ladder(vec!["a".into(), "b".into()]);
        assert_ne!(laddered.digest, before);
        assert_ne!(p.clone().with_ladder(vec!["a".into()]).digest, laddered.digest);
        let json = serde_json::to_value(&laddered).unwrap();
        assert_eq!(json["ladder"], serde_json::json!(["a", "b"]));
        let back: BattlePlan = serde_json::from_value(json).unwrap();
        assert_eq!(back.compute_digest(), laddered.digest);
    }

    #[test]
    fn a_stages_changed_rules_are_in_the_digest_only_where_named() {
        let p = plan(40, 7, |j| j % 3 != 0);
        let before = p.digest.clone();
        assert_eq!(p.clone().with_stage_rules(None).digest, before);
        let named = p.clone().with_stage_rules(Some("hard/boos/out".into()));
        assert_ne!(
            named.digest, before,
            "a device under the match's rules disagrees"
        );
        assert_ne!(
            p.clone()
                .with_stage_rules(Some("hard/no-boos/out".into()))
                .digest,
            named.digest
        );
        assert!(named.check().is_ok());
        let json = serde_json::to_value(&named).unwrap();
        assert_eq!(json["stage_rules"], "hard/boos/out");
        assert!(
            serde_json::to_value(&p)
                .unwrap()
                .get("stage_rules")
                .is_none()
        );
        let back: BattlePlan = serde_json::from_value(json).unwrap();
        assert_eq!(back.compute_digest(), named.digest);
    }

    #[test]
    fn the_seed_is_everyone_s_and_differs_by_stage() {
        let room = RoomId::new("Room000001");
        let chart = ChartKey::new("abc");
        let a = seed_for(&room, StageId(3), &chart);
        assert_eq!(a, seed_for(&room, StageId(3), &chart));
        assert_ne!(a, seed_for(&room, StageId(4), &chart));
        assert_ne!(a, seed_for(&RoomId::new("Room000002"), StageId(3), &chart));
    }
}
