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
