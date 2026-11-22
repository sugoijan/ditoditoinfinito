//! The room model's fixes from the code review of 2026-11-22 (round RFa):
//! each case as the review described it, reproduced first, every
//! revision's events applied to a mirror, which must stay equal.

#![allow(dead_code)]

mod common;

use common::{Room, song};
use ddi_room::*;

fn hosted() -> Room {
    let mut policy = RoomPolicy::default().with_live().with_audience();
    policy.readiness.window_ms = 15_000;
    let mut rules = MatchRules::hosted();
    rules.stages = 3;
    Room::open("host", "Host", policy, rules, ContentSource::Shared)
}

fn join(r: &mut Room, name: &str) -> ParticipantId {
    r.join_as(name, Intent::Play, false).unwrap()
}

/// Finding 1 (a): a member's loading failure is kept with a code of a
/// bounded size, whatever they send (two 700 KB codes once made the
/// running match's row 1.4 MB). An honest client's longest code,
/// `battle_plan_mismatch`, is kept as it is.
#[test]
fn a_loading_failures_code_is_kept_bounded() {
    let mut r = hosted();
    let ana = join(&mut r, "ana");
    let bob = join(&mut r, "bob");
    let host = r.host;
    r.by(host, Command::Pick { content: song(1), card: None }).unwrap();
    let s1 = r.s.upcoming_stage().unwrap();
    r.load_for(ana, s1, Loading::Failed { code: "x".repeat(700_000) }).unwrap();
    // Cut between characters, never inside one.
    r.load_for(bob, s1, Loading::Failed { code: "é".repeat(100) }).unwrap();
    for who in [ana, bob] {
        let Loading::Failed { code } = &r.s.stage(s1).unwrap().prep(who).loading else {
            panic!("failed")
        };
        assert!(code.len() <= Loading::MAX_CODE, "{}", code.len());
    }
    let bytes = serde_json::to_vec(&r.s.current).unwrap().len();
    assert!(bytes < 20_000, "{bytes}");
    // The same report again changes nothing.
    assert_eq!(r.load_for(ana, s1, Loading::Failed { code: "x".repeat(700_000) }), Ok(vec![]));
    // An honest client's code, as it is.
    let honest = "battle_plan_mismatch";
    r.load_for(ana, s1, Loading::Failed { code: honest.into() }).unwrap();
    assert_eq!(
        r.s.stage(s1).unwrap().prep(ana).loading,
        Loading::Failed { code: honest.into() }
    );
}

/// Finding 1 (c): the next match's rules an operator saves are bounded:
/// the battle's id at 64 bytes (as a stage's rules already were,
/// `PlayRules::check`) and the CPU roster at the room's competitor cap
/// whatever the CPU mode (it was read only under `CpuMode::Roster`, so
/// a co-host could park any number of entries in the draft). An honest
/// roster seats its CPUs beside one person at least: at most the cap
/// less one.
#[test]
fn the_next_rules_are_bounded() {
    let mut r = hosted();
    let host = r.host;
    let mut rules = r.s.next.rules.clone();
    rules.battle = "x".repeat(700_000);
    let got = r.by(host, Command::SaveNextRules { rules });
    assert!(
        matches!(got, Err(Rejection::InvalidRules(RulesProblem::BattleId))),
        "the battle's id refused"
    );

    let mut rules = r.s.next.rules.clone();
    rules.cpu.mode = CpuMode::Fill;
    rules.cpu.roster = vec![rules.cpu.opponent.clone(); 10_000];
    let got = r.by(host, Command::SaveNextRules { rules: rules.clone() });
    assert!(
        matches!(got, Err(Rejection::InvalidRules(RulesProblem::OverCap(_)))),
        "the roster refused"
    );
    // Within the cap it is saved as before.
    rules.cpu.roster.truncate(usize::from(r.s.policy.max_competitors) - 1);
    r.by(host, Command::SaveNextRules { rules }).unwrap();
    // A practice roster as the client makes it (`cpu_roster::with_roster`).
    let mut rules = MatchRules::practice(OpponentSpec { level: 5, seed: 1, personality: None, name: None, targeting: None });
    rules.cpu.roster = vec![rules.cpu.opponent.clone(); 3];
    rules.cpu.mode = CpuMode::Roster;
    rules.seats = 4;
    assert_eq!(rules.validate(&RoomPolicy::default(), 1), Ok(()));
}

/// Finding 1 (b): rows of people who left are kept (results name them by
/// id), and a room bounded only the people present, so a room joined
/// over its life by thousands of sign-ins grew its stored row without
/// bound. A new person is refused as a full room once the room keeps
/// `RoomPolicy::member_rows` people's rows, present or not (four times
/// its member bound and administrators' reserve: 1,552 for a room of
/// today's bound); someone with a row comes back as before. Proven with
/// a member bound of 4 (rows: 20).
#[test]
fn a_room_keeps_a_bounded_number_of_peoples_rows() {
    let mut policy = RoomPolicy::default().with_live().with_audience();
    policy.max_members = 4;
    policy.admin_reserve = 1;
    assert_eq!(policy.member_rows(), 20);
    assert_eq!(RoomPolicy::default().member_rows(), 1_552);
    let mut r = Room::open("host", "Host", policy, MatchRules::hosted(), ContentSource::Shared);
    let first = r.join_as("p0", Intent::Watch, false).unwrap();
    r.by(first, Command::Leave).unwrap();
    for i in 1..18 {
        let id = r.join_as(&format!("p{i}"), Intent::Watch, false).unwrap();
        r.by(id, Command::Leave).unwrap();
    }
    assert_eq!(r.s.members.len(), 19, "the host and 18 who left");
    // The twentieth row is taken; a twenty-first person is refused.
    let last = r.join_as("p18", Intent::Watch, false).unwrap();
    r.by(last, Command::Leave).unwrap();
    assert_eq!(r.join_as("p19", Intent::Watch, false), Err(Rejection::RoomFull));
    // An administrator too: the bound is the room's stored size.
    assert_eq!(r.join_as("pat", Intent::Watch, true), Err(Rejection::RoomFull));
    // Someone who has a row comes back.
    assert_eq!(r.join_as("p0", Intent::Watch, false), Ok(first));
    assert!(!r.s.member(first).unwrap().left);
    // A room stored with more rows than that (before the bound) loads,
    // and its people come back.
    let mut big = r.s.clone();
    let json = {
        let mut room = r.s.clone();
        for i in 0..30u64 {
            let id = ParticipantId(1_000 + i);
            let mut m = room.members[&first].clone();
            m.id = id;
            m.kind = MemberKind::Human { identity: Identity::new(format!("twitch:old{i}")), name: format!("old{i}") };
            m.left = true;
            room.members.insert(id, m);
        }
        room.counters.participant = 1_030;
        serde_json::to_string(&room).unwrap()
    };
    big = serde_json::from_str(&json).unwrap_or(big);
    assert_eq!(big.members.len(), 50);
    let mut back = Room { mirror: big.clone(), s: big, now: r.now, host: r.host };
    assert_eq!(back.join_as("old3", Intent::Watch, false), Ok(ParticipantId(1_003)));
    assert_eq!(back.join_as("newcomer", Intent::Watch, false), Err(Rejection::RoomFull));
}
