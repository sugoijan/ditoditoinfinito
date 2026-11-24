//! Room model revision 24 (round R24, 2026-11-24), through the reducer
//! and the room's live side: judged counts bounded by the battle plan's
//! notes, the rows of people long gone forgotten, the kicked remembered,
//! a battle plan offered by any prepared device; and a room stored by
//! revision 23 read and played on. Every revision's events are applied to
//! a mirror, which must stay equal.

#![allow(dead_code)]

mod common;


use ddi_room::attacks::{
    ATTACKS, AttackEvent, Judged, POWER_CLASSES_V0, Report, RosterSeat, StageBattle, StageRoster,
    Targeting,
};
use ddi_room::*;

// ---- 1. Judged counts bounded by the chart ----

/// Ana's and Bob's ledger over `n` checkpoints of 2 s (the common plan),
/// each checkpoint's notes `notes` (none: unbounded, as before revision
/// 24).
fn ledger(n: usize, notes: Option<Vec<u32>>) -> StageBattle {
    let plan = common::plan_under(ATTACKS.id, n, 7, |j| j % 2 == 1);
    let seats = [ParticipantId(2), ParticipantId(3)]
        .map(|participant| RosterSeat { participant, targeting: Targeting::Spread, lead_us: 0 })
        .to_vec();
    let battle = StageBattle::new(
        plan.rules().unwrap(),
        POWER_CLASSES_V0,
        "ddi",
        plan.checkpoints_us.clone(),
        StageRoster { seats, seed: plan.seed },
        plan.schedule.clone(),
    );
    match notes {
        Some(notes) => battle.with_notes(notes),
        None => battle,
    }
}

/// `who`'s report of checkpoint `k` (at `(k + 1) × 2 s`): `count`
/// judgments of the best tier spread over the 2 s before it.
fn report(who: ParticipantId, k: usize, count: usize) -> Report {
    let end = (k as i64 + 1) * 2_000_000;
    let judged = (0..count)
        .map(|i| Judged { at_us: end - 2_000_000 + (i as i64 * 2_000_000 / count.max(1) as i64), tier: 0 })
        .collect();
    Report { participant: who, judged, life_ppm: 1_000_000, out_at_us: None, targeting: None, score: 0 }
}

/// What a stage of `n` checkpoints comes to when Ana reports `ana(k)`
/// judgments at each and Bob ten: every checkpoint's events.
fn run(battle: &mut StageBattle, n: usize, ana: impl Fn(usize) -> usize) -> Vec<Vec<AttackEvent>> {
    (0..n)
        .map(|k| {
            battle.checkpoint(k, &[report(ParticipantId(2), k, ana(k)), report(ParticipantId(3), k, 10)])
        })
        .collect()
}

/// Item 1: a modified client's reports of impossible counts (the wire's
/// most, 4,096 judgments a report, every one the best tier) are credited
/// no more than a play of full accuracy: the plan's notes say what the
/// chart has by each checkpoint (here twelve notes every 2 s, none in the
/// second after a checkpoint), and the room credits no more judgments by
/// then. Without notes (a plan stored before revision 24) the forger
/// charged a full bar at every checkpoint.
#[test]
fn a_forged_report_of_impossible_counts_earns_no_more_than_full_accuracy() {
    const N: usize = 30;
    let notes: Vec<u32> = (1..=N as u32).map(|k| 12 * k).collect();
    let honest = run(&mut ledger(N, Some(notes.clone())), N, |_| 12);
    let mut forged_ledger = ledger(N, Some(notes.clone()));
    let forged = run(&mut forged_ledger, N, |_| 4_096);
    assert_eq!(forged, honest, "the forger's stage is the honest full accuracy one");
    let mut a = ledger(N, Some(notes));
    run(&mut a, N, |_| 12);
    assert_eq!(forged_ledger.stats(ParticipantId(2)), a.stats(ParticipantId(2)));
    assert_eq!(forged_ledger.power(ParticipantId(2)), a.power(ParticipantId(2)));
    // Unbounded, the forger launched far more.
    let mut unbounded = ledger(N, None);
    run(&mut unbounded, N, |_| 4_096);
    assert!(
        unbounded.stats(ParticipantId(2)).launched > a.stats(ParticipantId(2)).launched,
        "{:?} against {:?}",
        unbounded.stats(ParticipantId(2)),
        a.stats(ParticipantId(2))
    );
    // A forger who reports nothing for a while and then everything at
    // once is credited what the chart had by then, never more.
    let mut late = ledger(N, Some((1..=N as u32).map(|k| 12 * k).collect()));
    run(&mut late, 10, |k| if k == 9 { 4_096 } else { 0 });
    let cost = late.rules().cost;
    let a = ParticipantId(2);
    let credited = late.power(a) + late.overflow(a) + late.stats(a).launched * cost;
    assert_eq!(credited, 2 * 12 * 10, "two a judgment of the best tier, 120 judgments");
}

/// An honest play below the notes is credited every judgment it reports:
/// a late judgment (a miss judged after its checkpoint) counts at the
/// next, the count of the notes being cumulative.
#[test]
fn an_honest_play_is_credited_every_judgment() {
    const N: usize = 12;
    let notes: Vec<u32> = (1..=N as u32).map(|k| 12 * k).collect();
    let mut bounded = ledger(N, Some(notes));
    let mut free = ledger(N, None);
    // Eleven, then thirteen (one carried over), as a late judgment would.
    let ana = |k: usize| if k.is_multiple_of(2) { 11 } else { 13 };
    assert_eq!(run(&mut bounded, N, ana), run(&mut free, N, ana));
}
