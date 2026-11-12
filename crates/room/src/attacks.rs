//! Battle attacks' ledger (`docs/plans/multiplayer-prototype-progress.md`,
//! M8; the maintainer's decisions of 2026-11-11, D0–D8): each
//! competitor's attack power, earned by timing and credited at checkpoints
//! every four beats. Pure and chart-free, so the local stage runner
//! (`ddi_session::battle::attacks`) and later an online room's authority
//! keep the same ledger from the same reports; whole numbers only (power
//! units, microseconds), keyed by participant in order.
//!
//! M8a kept the ledger and showed the power; M8b launches: a full bar is
//! spent at a checkpoint on an attack of eight shocks (D4: never fewer)
//! against a target its sender's policy picks ([`Targeting`]), in a
//! section of the chart safe for them ([`Schedule`]) that starts no
//! sooner than the target's own lead ([`LEAD_V0`], D5); under
//! `attacks-v0` one attack at a time against anyone, until they have
//! recovered from it; under `attacks-v1` ([`ATTACKS`], round 3b) no
//! recovery, and one more may be reserved behind the one on its way or
//! passing, in a later section (a chain); one against someone who went
//! out expires. Each checkpoint runs in a fixed order
//! ([`StageBattle::checkpoint`]), so every authority given the same
//! reports comes to the same record ([`StageBattle::record`]). The stage
//! is settled ([`SETTLEMENT_V0`]) only once no committed attack against
//! anyone still in is outstanding.
//!
//! PROVISIONAL: every number here is a working value
//! ([`ATTACKS_V0`], [`ATTACKS_V1`], [`POWER_CLASSES_V0`], [`LEAD_V0`],
//! [`SETTLEMENT_V0`]), named and versioned.

use crate::awards::{Finish, Finisher};
use crate::battle::{Entrant, Run};
use crate::ids::ParticipantId;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

/// The attack rules a stage is played under: the bar's size, what an
/// attack spends, how often power is credited. PROVISIONAL.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct AttackRules {
    /// Names these values and their version in records.
    pub id: &'static str,
    /// The most power a competitor holds; what is earned past it is lost.
    pub cap: u32,
    /// What an attack spends: a launch needs this much.
    pub cost: u32,
    /// Beats between checkpoints, where power is credited (and between
    /// the possible starts of a [`Schedule`]'s sections).
    pub checkpoint_beats: u32,
    /// How far past the first start it may take a commitment looks for a
    /// safe section, beats.
    pub search_beats: u32,
    /// A recipient recovers from an attack once this many of its own
    /// judgments came after the attack's last shock …
    pub recover_judgments: u32,
    /// … and at least this many beats have passed since
    /// ([`Section::recover_after_us`], the schedule's maker counts them).
    pub recover_beats: u32,
    /// How many more attacks may be reserved against a recipient while
    /// one is on its way or passing (a chain): each lands in the first
    /// safe section that starts after the one before it has cleared, so
    /// two attacks' shocks never share a section.
    pub chain: u32,
}

/// `attacks-v0` (PROVISIONAL): a bar of 100, an attack 100, checkpoints
/// every four beats. With [`POWER_CLASSES_V0`] a bar fills from 50 notes
/// in the best timing window, 100 in the next. A commitment looks 16 beats
/// past its earliest start; recovery takes eight of the recipient's
/// judgments and four beats after the attack's last shock.
pub const ATTACKS_V0: AttackRules = AttackRules {
    id: "attacks-v0",
    cap: 100,
    cost: 100,
    checkpoint_beats: 4,
    search_beats: 16,
    recover_judgments: 8,
    recover_beats: 4,
    chain: 0,
};

/// `attacks-v1` (PROVISIONAL; the maintainer, 2026-11-12: "we can let the
/// recovery be even tighter, maybe even allowing chaining attacks"): the
/// bar, the cost and the checkpoints of `attacks-v0`; no recovery period
/// (an attack against someone is over once its last shock's window has
/// passed, whatever lane that shock locked); and one more attack may be
/// reserved against them while one is on its way or passing, never more
/// than one waiting.
pub const ATTACKS_V1: AttackRules = AttackRules {
    id: "attacks-v1",
    cap: 100,
    cost: 100,
    checkpoint_beats: 4,
    search_beats: 16,
    recover_judgments: 0,
    recover_beats: 0,
    chain: 1,
};

/// The rules attacks are played under.
pub const ATTACKS: AttackRules = ATTACKS_V1;

/// How soon after its checkpoint an attack's section may start for a
/// target (D5): the target's reading horizon (how long its notes are
/// shown before they are judged), a warning and a built-in delay, at
/// least four beats (the next checkpoint), at most a cap (a slower
/// setting is raised on a stage so that it fits).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Lead {
    pub id: &'static str,
    pub warning_us: i64,
    pub delay_us: i64,
    pub cap_us: i64,
}

/// `lead-v0` (PROVISIONAL, D5): 1.5 s of warning, 0.1 s of delay, at
/// most 10 s in all.
pub const LEAD_V0: Lead = Lead {
    id: "lead-v0",
    warning_us: 1_500_000,
    delay_us: 100_000,
    cap_us: 10_000_000,
};

impl Lead {
    /// The lead of a target reading `horizon_us` ahead, capped.
    pub fn of(&self, horizon_us: i64) -> i64 {
        (horizon_us.max(0) + self.warning_us + self.delay_us).min(self.cap_us)
    }

    /// The longest horizon that fits under the cap.
    pub fn horizon_cap_us(&self) -> i64 {
        self.cap_us - self.warning_us - self.delay_us
    }
}

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
/// FFR Amazing 2 / Perfect 1; DDI (the rooms' ruleset since its round 3)
/// W0 and W1 2 / W2 1: W0 is judged inside W1 and reported as it, so the
/// tiers a report carries need nothing new. A ruleset not listed earns
/// nothing.
pub const POWER_CLASSES_V0: PowerClasses = PowerClasses {
    id: "power-classes-v0",
    table: &[
        ("ddi", [2, 1, 0, 0, 0, 0]),
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

/// How a competitor picks whom to attack (D8; the policy, never a target
/// chosen by hand).
#[derive(
    Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize,
)]
#[serde(rename_all = "snake_case")]
pub enum Targeting {
    /// Whoever has been sent the fewest attacks this stage (seeded ties).
    #[default]
    Spread,
    /// Whoever attacked them last, while they can be; else Spread.
    Retaliate,
    /// Whoever has the most life (ties by score, then seeded).
    Leader,
}

impl Targeting {
    /// The policies in the order a press cycles them (D8).
    pub const ALL: [Targeting; 3] = [Targeting::Spread, Targeting::Retaliate, Targeting::Leader];
}

/// One competitor of a stage's battle: its policy and its lead (D5,
/// [`LEAD_V0`]: how soon an attack's section may start for it).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct RosterSeat {
    pub participant: ParticipantId,
    pub targeting: Targeting,
    pub lead_us: i64,
}

/// Who plays a stage's battle and the seed its ties are drawn from.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct StageRoster {
    pub seats: Vec<RosterSeat>,
    pub seed: u64,
}

/// A shock of a section: its row (ticks of the chart), its lane and its
/// time (song microseconds).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Cell {
    pub tick: i64,
    pub lane: u8,
    pub at_us: i64,
}

/// A section of the chart with room for an attack's shocks (made by the
/// engine's placement, `ddi_engine::battle::schedule`): its shocks in time
/// order; when it is over (the last shock's time and its window); when its
/// recipient may first recover ([`AttackRules::recover_beats`] beats
/// after).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Section {
    pub cells: Vec<Cell>,
    pub clear_us: i64,
    pub recover_after_us: i64,
}

/// One possible start: every [`AttackRules::checkpoint_beats`] beats from
/// the chart's beginning, with a section where the chart has room.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Slot {
    pub start_us: i64,
    pub section: Option<Section>,
}

/// Every possible start of a stage's chart, in time order. One for
/// everyone (the same chart, ruleset and seed).
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Schedule {
    pub slots: Vec<Slot>,
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
/// the previous one (each before this checkpoint's time), its life just
/// before the checkpoint (millionths) and when it went out, if it did.
/// Power is the ledger's to compute, never reported. `targeting`: its
/// policy as it stood just before the checkpoint, where it changed it
/// during the play (M8 round 3, D8: a change applies from the next
/// checkpoint); none, the policy it has. Additive.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Report {
    pub participant: ParticipantId,
    pub judged: Vec<Judged>,
    pub life_ppm: u32,
    pub out_at_us: Option<i64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub targeting: Option<Targeting>,
}

/// Why a full bar waits.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BlockReason {
    /// No one can be attacked: everyone else out, or each already under
    /// as many attacks as the rules allow (reserved, active or
    /// recovering; under `attacks-v1` one passing and one waiting).
    NoEligible,
    /// Its target's chart has no safe section in reach.
    NoSafeSection,
}

/// An attack's number in its stage, from 1.
pub type AttackId = u32;

/// Where an attack is.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AttackState {
    /// Committed; its shocks still to come or coming.
    Active,
    /// Its last shock passed; its recipient not recovered yet.
    Cleared,
    /// Its recipient may be attacked again.
    Recovered,
    /// Its recipient went out first (nothing is refunded).
    Expired,
}

/// An attack: who sent it to whom, at which checkpoint, its shocks (all
/// eight, D4) and where it is.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Attack {
    pub id: AttackId,
    pub sender: ParticipantId,
    pub recipient: ParticipantId,
    /// The checkpoint it was committed at, and its section's slot.
    pub checkpoint: usize,
    pub slot: usize,
    pub cells: Vec<Cell>,
    pub clear_us: i64,
    pub recover_after_us: i64,
    pub state: AttackState,
    /// The recipient's judgments after its last shock, toward recovery.
    pub after_clear: u32,
}

/// What a checkpoint changed that a view or a voice would tell.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AttackEvent {
    /// Its bar became full.
    Charged {
        participant: ParticipantId,
    },
    /// An attack was committed ([`StageBattle::attack`] has its shocks).
    Committed {
        attack: AttackId,
        sender: ParticipantId,
        recipient: ParticipantId,
    },
    /// A full bar waits (told when it starts waiting or its reason
    /// changes).
    Blocked {
        sender: ParticipantId,
        reason: BlockReason,
    },
    Cleared {
        attack: AttackId,
    },
    Recovered {
        attack: AttackId,
    },
    Expired {
        attack: AttackId,
    },
    /// A competitor changed its policy during the play: `targeting` from
    /// this checkpoint's launches on.
    Retargeted {
        participant: ParticipantId,
        targeting: Targeting,
    },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Seat {
    power: u32,
    /// Earned past the cap, lost.
    overflow: u32,
    /// Everything earned, the cap aside: Leader's tie-break.
    earned: u64,
    out_at_us: Option<i64>,
    targeting: Targeting,
    lead_us: i64,
    life_ppm: u32,
    /// Attacks admitted against it this stage (Spread's count).
    admitted: u32,
    /// Who attacked it last, as of the last batch (Retaliate's memory).
    last_sender: Option<ParticipantId>,
    blocked: Option<BlockReason>,
    /// Checkpoints its full bar waited, by reason.
    waits: [u32; 2],
    launched: u32,
}

impl Seat {
    fn new(s: &RosterSeat) -> Seat {
        Seat {
            power: 0,
            overflow: 0,
            earned: 0,
            out_at_us: None,
            targeting: s.targeting,
            lead_us: s.lead_us.max(0),
            life_ppm: 1_000_000,
            admitted: 0,
            last_sender: None,
            blocked: None,
            waits: [0; 2],
            launched: 0,
        }
    }
}

/// A competitor's attacks in numbers (the debug state, the study's
/// counts).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct SeatStats {
    pub launched: u32,
    pub received: u32,
    /// Checkpoints its full bar waited for no one to attack, and for a
    /// safe section.
    pub waited_no_eligible: u32,
    pub waited_no_safe_section: u32,
}

/// The ledger after a checkpoint: what [`StageBattle::record`] keeps, so
/// that two runs of a stage (another frame rate, a replay) compare equal.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct CheckpointRecord {
    pub at_us: i64,
    /// Each competitor's power after it, in participant order.
    pub power: Vec<(ParticipantId, u32)>,
    /// What it changed.
    pub events: Vec<AttackEvent>,
}

/// One stage's attack ledger.
#[derive(Clone, Debug)]
pub struct StageBattle {
    rules: AttackRules,
    classes: PowerClasses,
    ruleset: String,
    checkpoints_us: Vec<i64>,
    schedule: Schedule,
    seed: u64,
    next: usize,
    seats: BTreeMap<ParticipantId, Seat>,
    attacks: Vec<Attack>,
    record: Vec<CheckpointRecord>,
}

/// SplitMix64.
fn mix(mut x: u64) -> u64 {
    x = x.wrapping_add(0x9E37_79B9_7F4A_7C15);
    x = (x ^ (x >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    x = (x ^ (x >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    x ^ (x >> 31)
}

impl StageBattle {
    /// The ledger of a stage played under `ruleset` (the match's) by
    /// `roster`, its checkpoints at `checkpoints_us` (the chart's every
    /// `rules.checkpoint_beats` beats from the first, in song
    /// microseconds; kept only while they rise), its attacks placed by
    /// `schedule`.
    pub fn new(
        rules: AttackRules,
        classes: PowerClasses,
        ruleset: &str,
        checkpoints_us: Vec<i64>,
        roster: StageRoster,
        schedule: Schedule,
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
            schedule,
            seed: roster.seed,
            next: 0,
            seats: roster
                .seats
                .iter()
                .map(|s| (s.participant, Seat::new(s)))
                .collect(),
            attacks: Vec::new(),
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

    /// Checkpoint `k` (the next one) from everyone's `reports`, in this
    /// order: (1) those out before it are out; (2) each competitor still
    /// in is credited what its judgments earned, up to the cap (the rest
    /// lost), its life is as reported and its policy as it left it
    /// ([`Report::targeting`]); (3) attacks whose last shock
    /// passed are cleared, and a cleared one's recipient recovers once
    /// enough of its judgments came after it and enough beats passed;
    /// (4) attacks against those out expire; (5) full bars launch, in an
    /// order turning each checkpoint, each at the target its policy picks
    /// among those who can be attacked (judged on the state before any of
    /// this checkpoint's launches; Retaliate's memory changes after
    /// them), in (6) the first safe section from the target's lead within
    /// [`AttackRules::search_beats`]; else it waits. What it changed. A
    /// report of anyone not in the stage, or of a checkpoint not the
    /// next, changes nothing.
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
        // (2) Power, by timing (D2), capped; life.
        for r in reports {
            let Some(seat) = self.seats.get_mut(&r.participant) else {
                continue;
            };
            if seat.out_at_us.is_some() {
                continue;
            }
            seat.life_ppm = r.life_ppm.min(1_000_000);
            // Its policy, where it changed it since the last checkpoint:
            // this checkpoint's launches go by it.
            if let Some(t) = r.targeting
                && t != seat.targeting
            {
                seat.targeting = t;
                events.push(AttackEvent::Retargeted {
                    participant: r.participant,
                    targeting: t,
                });
            }
            let earned: u32 = r
                .judged
                .iter()
                .filter(|j| j.at_us < at)
                .map(|j| self.classes.earned(&self.ruleset, j.tier))
                .sum();
            seat.earned = seat.earned.saturating_add(u64::from(earned));
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
        // (3) Clearing and recovery.
        for a in &mut self.attacks {
            if a.state == AttackState::Active && a.clear_us < at {
                a.state = AttackState::Cleared;
                events.push(AttackEvent::Cleared { attack: a.id });
            }
            if a.state != AttackState::Cleared {
                continue;
            }
            let after = reports
                .iter()
                .filter(|r| r.participant == a.recipient)
                .flat_map(|r| r.judged.iter())
                .filter(|j| j.at_us > a.clear_us && j.at_us < at)
                .count();
            a.after_clear = a.after_clear.saturating_add(after as u32);
            if a.after_clear >= self.rules.recover_judgments && at >= a.recover_after_us {
                a.state = AttackState::Recovered;
                events.push(AttackEvent::Recovered { attack: a.id });
            }
        }
        // (4) Expiry: the recipient out (no refund).
        for a in &mut self.attacks {
            if matches!(a.state, AttackState::Active | AttackState::Cleared)
                && self
                    .seats
                    .get(&a.recipient)
                    .is_none_or(|s| s.out_at_us.is_some())
            {
                a.state = AttackState::Expired;
                events.push(AttackEvent::Expired { attack: a.id });
            }
        }
        // (5), (6) Launches and commitments.
        self.launch(k, at, &mut events);
        self.record.push(CheckpointRecord {
            at_us: at,
            power: self.seats.iter().map(|(p, s)| (*p, s.power)).collect(),
            events: events.clone(),
        });
        events
    }

    /// The full bars of checkpoint `k` (at `at`) launched, or waiting.
    fn launch(&mut self, k: usize, at: i64, events: &mut Vec<AttackEvent>) {
        let order: Vec<ParticipantId> = self.seats.keys().copied().collect();
        let n = order.len();
        if n == 0 {
            return;
        }
        // Who is under attacks already (reserved, active or recovering):
        // how many, and when the last of them clears.
        let mut taken: BTreeMap<ParticipantId, (u32, i64)> = BTreeMap::new();
        for a in &self.attacks {
            if matches!(a.state, AttackState::Active | AttackState::Cleared) {
                let t = taken.entry(a.recipient).or_insert((0, i64::MIN));
                t.0 += 1;
                t.1 = t.1.max(a.clear_us);
            }
        }
        let room = 1 + self.rules.chain;
        // The state the policies judge by: before this batch.
        let before = self.seats.clone();
        let tie = |sender: ParticipantId, p: ParticipantId| {
            mix(self.seed ^ mix(k as u64) ^ mix(sender.0.rotate_left(21)) ^ mix(p.0))
        };
        let earliest_next = self.checkpoints_us.get(k + 1).copied();
        let mut memory: Vec<(ParticipantId, ParticipantId)> = Vec::new();
        for i in 0..n {
            let sender = order[(k + i) % n];
            let seat = before[&sender];
            if seat.out_at_us.is_some() || seat.power < self.rules.cost {
                continue;
            }
            let eligible: Vec<ParticipantId> = order
                .iter()
                .copied()
                .filter(|p| {
                    *p != sender
                        && before[p].out_at_us.is_none()
                        && taken.get(p).is_none_or(|t| t.0 < room)
                })
                .collect();
            let spread = |cands: &[ParticipantId]| {
                cands
                    .iter()
                    .copied()
                    .min_by_key(|p| (before[p].admitted, tie(sender, *p)))
            };
            let target = match seat.targeting {
                Targeting::Spread => spread(&eligible),
                Targeting::Retaliate => seat
                    .last_sender
                    .filter(|p| eligible.contains(p))
                    .or_else(|| spread(&eligible)),
                Targeting::Leader => eligible.iter().copied().min_by_key(|p| {
                    (
                        std::cmp::Reverse(before[p].life_ppm),
                        std::cmp::Reverse(before[p].earned),
                        tie(sender, *p),
                    )
                }),
            };
            let outcome = match target {
                None => Err(BlockReason::NoEligible),
                // A chained attack's section starts after the one
                // before it has cleared.
                Some(t) => self
                    .section_for(
                        at,
                        earliest_next,
                        before[&t].lead_us,
                        taken.get(&t).map(|t| t.1),
                    )
                    .map(|slot| (t, slot))
                    .ok_or(BlockReason::NoSafeSection),
            };
            let seat = self.seats.get_mut(&sender).expect("seated");
            match outcome {
                Err(reason) => {
                    seat.waits[reason as usize] += 1;
                    if seat.blocked != Some(reason) {
                        seat.blocked = Some(reason);
                        events.push(AttackEvent::Blocked { sender, reason });
                    }
                }
                Ok((recipient, slot)) => {
                    seat.blocked = None;
                    seat.power -= self.rules.cost;
                    seat.launched += 1;
                    if let Some(r) = self.seats.get_mut(&recipient) {
                        r.admitted += 1;
                    }
                    let section = self.schedule.slots[slot]
                        .section
                        .clone()
                        .expect("a safe section");
                    let t = taken.entry(recipient).or_insert((0, i64::MIN));
                    t.0 += 1;
                    t.1 = t.1.max(section.clear_us);
                    let id = self.attacks.len() as AttackId + 1;
                    self.attacks.push(Attack {
                        id,
                        sender,
                        recipient,
                        checkpoint: k,
                        slot,
                        cells: section.cells,
                        clear_us: section.clear_us,
                        recover_after_us: section.recover_after_us,
                        state: AttackState::Active,
                        after_clear: 0,
                    });
                    memory.push((recipient, sender));
                    events.push(AttackEvent::Committed {
                        attack: id,
                        sender,
                        recipient,
                    });
                }
            }
        }
        // Retaliate's memory, for the next batch.
        for (recipient, sender) in memory {
            if let Some(r) = self.seats.get_mut(&recipient) {
                r.last_sender = Some(sender);
            }
        }
    }

    /// The first slot with a section that starts no sooner than `lead_us`
    /// after `at` and the next checkpoint (`next`: at least four beats),
    /// nor before `after` (when the attack before it against the same
    /// recipient clears: a chain's sections never overlap), within
    /// [`AttackRules::search_beats`] past that start.
    fn section_for(
        &self,
        at: i64,
        next: Option<i64>,
        lead_us: i64,
        after: Option<i64>,
    ) -> Option<usize> {
        let earliest = (at + lead_us)
            .max(next.unwrap_or(at + 1))
            .max(after.map_or(i64::MIN, |a| a.saturating_add(1)));
        let from = self
            .schedule
            .slots
            .partition_point(|s| s.start_us < earliest);
        let reach = (self.rules.search_beats / self.rules.checkpoint_beats.max(1)) as usize + 1;
        (from..(from + reach).min(self.schedule.slots.len()))
            .find(|&j| self.schedule.slots[j].section.is_some())
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

    /// Its policy.
    pub fn targeting(&self, participant: ParticipantId) -> Targeting {
        self.seats
            .get(&participant)
            .map_or_else(Targeting::default, |s| s.targeting)
    }

    /// Why its full bar waits, since the last checkpoint that could not
    /// launch it (none once it launched, or while it is not full).
    pub fn waiting(&self, participant: ParticipantId) -> Option<BlockReason> {
        self.seats
            .get(&participant)
            .filter(|s| s.power >= self.rules.cost && s.out_at_us.is_none())
            .and_then(|s| s.blocked)
    }

    /// Every attack committed so far, in order.
    pub fn attacks(&self) -> &[Attack] {
        &self.attacks
    }

    pub fn attack(&self, id: AttackId) -> Option<&Attack> {
        self.attacks.get((id as usize).checked_sub(1)?)
    }

    /// The attack against `participant` it has not recovered from
    /// (active or cleared), if any: the first of them where one more
    /// waits behind it ([`AttackRules::chain`]).
    pub fn incoming(&self, participant: ParticipantId) -> Option<&Attack> {
        self.incoming_all(participant).next()
    }

    /// Every attack against `participant` it has not recovered from, in
    /// the order they land: the one on its way or passing, then the one
    /// waiting behind it.
    pub fn incoming_all(&self, participant: ParticipantId) -> impl Iterator<Item = &Attack> {
        self.attacks.iter().filter(move |a| {
            a.recipient == participant
                && matches!(a.state, AttackState::Active | AttackState::Cleared)
        })
    }

    /// Its attacks whose recipients have not recovered.
    pub fn outgoing(&self, participant: ParticipantId) -> impl Iterator<Item = &Attack> {
        self.attacks.iter().filter(move |a| {
            a.sender == participant && matches!(a.state, AttackState::Active | AttackState::Cleared)
        })
    }

    /// The recipients of committed attacks not cleared yet, with when
    /// each clears: what keeps a stage from being settled
    /// ([`SETTLEMENT_V0`]).
    pub fn outstanding(&self) -> impl Iterator<Item = (ParticipantId, i64)> + '_ {
        self.attacks
            .iter()
            .filter(|a| a.state == AttackState::Active)
            .map(|a| (a.recipient, a.clear_us))
    }

    /// Its attacks in numbers.
    pub fn stats(&self, participant: ParticipantId) -> SeatStats {
        let Some(s) = self.seats.get(&participant) else {
            return SeatStats::default();
        };
        SeatStats {
            launched: s.launched,
            received: s.admitted,
            waited_no_eligible: s.waits[BlockReason::NoEligible as usize],
            waited_no_safe_section: s.waits[BlockReason::NoSafeSection as usize],
        }
    }

    /// The ledger after each checkpoint run so far.
    pub fn record(&self) -> &[CheckpointRecord] {
        &self.record
    }
}

/// One competitor's reports, from its play as it is judged: its judgments,
/// its going out and its policy changes in; each checkpoint's [`Report`]
/// out once the play has been judged past it. The same reports the local
/// stage runner hands its ledger (`ddi_session::battle::attacks`), so a
/// device of an online room reports what practice would have counted
/// (`crate::live`).
#[derive(Clone, Debug)]
pub struct Reporter {
    participant: ParticipantId,
    checkpoints_us: Vec<i64>,
    next: usize,
    pending: Vec<Judged>,
    out_at_us: Option<i64>,
    out_told: bool,
    retargets: Vec<(i64, Targeting)>,
}

impl Reporter {
    pub fn new(participant: ParticipantId, checkpoints_us: Vec<i64>) -> Reporter {
        Reporter {
            participant,
            checkpoints_us,
            next: 0,
            pending: Vec::new(),
            out_at_us: None,
            out_told: false,
            retargets: Vec::new(),
        }
    }

    /// A judgment of a note of the chart (in the order judged).
    pub fn judged(&mut self, j: Judged) {
        self.pending.push(j);
    }

    /// It went out (its gauge emptied) at song µs `at_us`.
    pub fn out_at(&mut self, at_us: i64) {
        self.out_at_us.get_or_insert(at_us);
    }

    /// It changed its policy at song µs `at_us`.
    pub fn retarget(&mut self, at_us: i64, targeting: Targeting) {
        self.retargets.push((at_us, targeting));
    }

    /// The next checkpoint to report, if any.
    pub fn next(&self) -> Option<usize> {
        (self.next < self.checkpoints_us.len()).then_some(self.next)
    }

    /// The reports of the checkpoints at or before `judged_to_us` (the song
    /// time its play has been judged to), `life` its life just before a
    /// song µs (millionths). Once out, one report more carries it and its
    /// last judgments, and nothing after (the ledger takes it as out from
    /// then on).
    pub fn due(&mut self, judged_to_us: i64, life: &dyn Fn(i64) -> u32) -> Vec<(usize, Report)> {
        let mut out = Vec::new();
        while let Some(k) = self.next() {
            let at = self.checkpoints_us[k];
            let out_now = self.out_at_us.is_some() && !self.out_told;
            if at > judged_to_us && !out_now {
                break;
            }
            if self.out_told {
                break;
            }
            let split = self.pending.partition_point(|j| j.at_us < at);
            let n = self.retargets.partition_point(|(t, _)| *t < at);
            let targeting = self.retargets.drain(..n).next_back().map(|(_, t)| t);
            out.push((
                k,
                Report {
                    participant: self.participant,
                    judged: self.pending.drain(..split).collect(),
                    life_ppm: life(at),
                    out_at_us: self.out_at_us,
                    targeting,
                },
            ));
            self.next += 1;
            if self.out_at_us.is_some() {
                self.out_told = true;
            }
        }
        out
    }
}

/// How a stage is settled once attacks exist (D6). PROVISIONAL.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Settlement {
    pub id: &'static str,
}

/// `settlement-v0` (PROVISIONAL, D6): a stage is decided once at most one
/// competitor is still in and no committed attack against anyone still in
/// is outstanding (no Stage won while one is); if the last one in goes
/// out while it waits (the final attack, or anything else), they share
/// first with those who went out in the same checkpoint interval as the
/// one before them ("Drawn: final attack").
pub const SETTLEMENT_V0: Settlement = Settlement {
    id: "settlement-v0",
};

/// What a stage waits on while settling: who was left alone, and when
/// the one before them went out (song microseconds).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Settling {
    pub survivor: ParticipantId,
    pub last_out_us: i64,
}

impl Settlement {
    /// Decided: at most one of `entrants` still in, and none of them
    /// still in is a recipient in `outstanding`.
    pub fn decided(&self, entrants: &[Entrant], outstanding: &[ParticipantId]) -> bool {
        crate::battle::decided(entrants)
            && !entrants
                .iter()
                .any(|e| matches!(e.run, Run::Live { .. }) && outstanding.contains(&e.participant))
    }

    /// The ranking of `entrants` (`crate::battle::rank`), and whether it
    /// was drawn by a final attack: `settling`'s survivor out since, they
    /// share first with those out in the same interval of
    /// `checkpoints_us` as `settling.last_out_us`.
    pub fn rank(
        &self,
        entrants: &[Entrant],
        checkpoints_us: &[i64],
        settling: Option<Settling>,
    ) -> (Vec<Vec<Finisher>>, bool) {
        let ranking = crate::battle::rank(entrants);
        let Some(s) = settling else {
            return (ranking, false);
        };
        let fell = entrants
            .iter()
            .any(|e| e.participant == s.survivor && matches!(e.run, Run::Out { .. }));
        if !fell {
            return (ranking, false);
        }
        let interval = |us: i64| checkpoints_us.partition_point(|&c| c <= us);
        let last = interval(s.last_out_us);
        let shares: BTreeSet<ParticipantId> = entrants
            .iter()
            .filter(|e| match e.run {
                Run::Out { at } => {
                    e.participant == s.survivor || interval((at * 1e6).round() as i64) == last
                }
                _ => false,
            })
            .map(|e| e.participant)
            .collect();
        let mut first: Vec<Finisher> = shares
            .iter()
            .map(|&participant| Finisher {
                participant,
                finish: Finish::Eliminated,
            })
            .collect();
        first.sort_by_key(|f| f.participant);
        let mut out = vec![first];
        for group in ranking {
            let rest: Vec<Finisher> = group
                .into_iter()
                .filter(|f| !shares.contains(&f.participant))
                .collect();
            if !rest.is_empty() {
                out.push(rest);
            }
        }
        (out, true)
    }
}

#[cfg(test)]
mod tests;
