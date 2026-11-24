//! Room model revision 24 (round R24, 2026-11-24), through the reducer
//! and the room's live side: judged counts bounded by the battle plan's
//! notes, the rows of people long gone forgotten, the kicked remembered,
//! a battle plan offered by any prepared device; and a room stored by
//! revision 23 read and played on. Every revision's events are applied to
//! a mirror, which must stay equal.

#![allow(dead_code)]

mod common;

use common::Room;
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

// ---- 2. Rows of people long gone are forgotten ----

/// A hosted live room with two songs listed by the host, a two-stage
/// match's rules.
fn hosted() -> Room {
    let mut policy = RoomPolicy::default().with_live().with_audience();
    policy.readiness.window_ms = 15_000;
    let mut rules = MatchRules::hosted();
    rules.stages = 2;
    Room::open("host", "Host", policy, rules, ContentSource::Shared)
}

/// `n` people who came to watch and left, `name0`, `name1`…: their ids.
fn passers(r: &mut Room, name: &str, n: usize) -> Vec<ParticipantId> {
    (0..n)
        .map(|i| {
            let id = r.join_as(&format!("{name}{i}"), Intent::Watch, false).unwrap();
            r.by(id, Command::Leave).unwrap();
            id
        })
        .collect()
}

fn rows(r: &Room) -> usize {
    r.s.members.values().filter(|m| !m.is_cpu()).count()
}

/// Item 2: a room many came and went through keeps the rows of at most
/// `left_rows_kept` (its member bound, 384) people who left and whom
/// nothing names: a newcomer's join forgets the earliest arrivals beyond
/// them (`Event::MembersForgotten`), every mirror alike. Before revision
/// 24 every row was kept, and a newcomer was refused as a full room
/// after 1,552 different people.
#[test]
fn a_newcomers_join_forgets_the_rows_of_people_long_gone() {
    let mut r = hosted();
    let kept = r.s.policy.left_rows_kept();
    assert_eq!(kept, 384);
    let gone = passers(&mut r, "p", kept + 20);
    // Each newcomer's join forgot the earliest beyond the 384: the host
    // and 385 who left (the last one left after their own join).
    assert_eq!(rows(&r), 1 + kept + 1);
    assert!(r.s.member(gone[18]).is_none());
    assert!(r.s.member(gone[19]).is_some());
    let events = r.sys(Command::Join {
        identity: Identity::new("twitch:new"),
        name: "new".into(),
        admin: false,
        intent: Intent::Watch,
    })
    .unwrap();
    assert_eq!(
        events[0],
        Event::MembersForgotten { participants: vec![gone[19]] },
        "the earliest arrival forgotten first"
    );
    assert_eq!(rows(&r), 1 + kept + 1);
    // Past the old stopgap: 2,000 people came and went, a newcomer is
    // still let in.
    passers(&mut r, "q", 1_700);
    let late = r.join_as("late", Intent::Watch, false).unwrap();
    assert!(r.s.member(late).is_some());
    assert!(rows(&r) <= kept + 3, "{}", rows(&r));
    // One forgotten who comes back is a newcomer, a new id.
    let again = r.join_as("p0", Intent::Watch, false).unwrap();
    assert_ne!(again, gone[0]);
    assert!(again.0 > late.0);
}

/// What a forgotten person could lose is never forgotten: a player of a
/// kept match (its standings name them), one whose song is listed, a
/// co-host (their role), the room's picker; the authority's Forget (at
/// a quiet moment) forgets the rest at once, and a member may not send
/// it.
#[test]
fn rows_something_names_are_never_forgotten() {
    let mut policy = RoomPolicy::default().with_live().with_audience();
    policy.max_members = 4;
    let mut rules = MatchRules::hosted();
    rules.stages = 2;
    let mut r = Room::open("host", "Host", policy, rules, ContentSource::Shared);
    let host = r.host;
    assert_eq!(r.s.policy.left_rows_kept(), 4);
    let ana = r.join_as("ana", Intent::Play, false).unwrap();
    let cy = r.join_as("cy", Intent::Watch, false).unwrap();
    r.by(host, Command::SetCoHost { participant: cy, on: true }).unwrap();
    r.by(host, Command::SetWhoAdds { who: songs::WhoAdds::Everyone }).unwrap();
    let dee = r.join_as("dee", Intent::Watch, false).unwrap();
    r.by(dee, Command::AddSong { song: Box::new(common::new_song(1)) }).unwrap();
    let listed = r.s.songs.entries[0].id;
    r.sys(Command::SetSongContent {
        song: listed,
        status: ContentStatus::Available,
        share: Some("listed".into()),
        charts: None,
    })
    .unwrap();
    // Ana plays the first stage, then leaves; so do Cy and Dee.
    r.by(host, Command::Pick { content: common::song(2), card: None }).unwrap();
    let stage = r.s.upcoming_stage().unwrap();
    r.sys(Command::SetContentStatus { stage, status: ContentStatus::Available, share: Some("sh".into()) })
        .unwrap();
    for p in [host, ana] {
        r.load_for(p, stage, Loading::Loaded).unwrap();
        r.ready_for(p, stage, true).unwrap();
    }
    r.wait(r.s.policy.readiness.countdown_ms + 1_000);
    assert!(matches!(r.s.phase, Phase::Stage { .. }), "{:?}", r.s.phase);
    for p in [ana, cy, dee] {
        r.by(p, Command::Leave).unwrap();
    }
    let gone = passers(&mut r, "p", 3);
    for p in [ana, cy, dee] {
        assert!(!r.s.forgettable(p), "{p:?}");
    }
    assert!(r.s.referenced(ana) && r.s.referenced(dee) && !r.s.referenced(cy));
    assert_eq!(r.s.to_forget(0), gone);
    assert_eq!(r.by(host, Command::Forget), Err(Rejection::AuthorityOnly));
    let events = r.sys(Command::Forget).unwrap();
    assert!(events.is_empty(), "three of four kept");
    passers(&mut r, "q", 3);
    // Joins forgot one; the authority's Forget the next beyond four.
    assert!(r.s.member(gone[0]).is_none());
    let events = r.sys(Command::Forget).unwrap();
    assert_eq!(events, vec![Event::MembersForgotten { participants: vec![gone[1]] }]);
    for p in [ana, cy, dee] {
        assert!(r.s.member(p).is_some());
    }
    assert!(r.s.to_forget(4).is_empty());
}
