//! Rooms of 1+72 players and the members who watch them
//! (`docs/plans/multiplayer-prototype-progress.md`, "Rooms of 1+72
//! players"): the caps, the bound on members refused plainly, and what a
//! room's state costs at 73 players and a few hundred members
//! (`what_a_large_room_costs`, ignored: `cargo test --release -p ddi-room
//! --test scale -- --ignored --nocapture`).

use std::time::Instant;

use ddi_room::fake::FakeServer;
use ddi_room::{
    Command, Connection, Identity, Intent, ManualClock, MatchRules, Rejection, RoomPolicy,
};

const ROOM: &str = "r_scale";

fn server(policy: RoomPolicy) -> FakeServer {
    FakeServer::hosted(
        ROOM,
        Identity::new("p:1"),
        "P1",
        policy,
        ManualClock::new(1_000_000),
    )
}

#[test]
fn seventy_three_play_and_more_watch_up_to_the_member_bound() {
    let policy = RoomPolicy::default();
    assert_eq!(policy.max_competitors, 73);
    let mut s = server(policy.clone());
    // 72 more players: every one plays.
    for i in 2..=73 {
        let id = s
            .join(Identity::new(format!("p:{i}")), &format!("P{i}"), Intent::Play)
            .unwrap();
        assert_eq!(s.state().members[&id].intent, Intent::Play, "{i}");
    }
    assert!(s.state().players_full());
    // The 74th asking to play joins to watch, and members go on joining
    // to watch up to the bound.
    let members = usize::from(policy.max_members);
    for i in 74..=members {
        let id = s
            .join(Identity::new(format!("p:{i}")), &format!("P{i}"), Intent::Play)
            .unwrap();
        assert_eq!(s.state().members[&id].intent, Intent::Watch, "{i}");
    }
    // One more is refused plainly: the room is full.
    assert_eq!(
        s.join(Identity::new("p:late"), "Late", Intent::Watch),
        Err(Rejection::RoomFull)
    );
    // Rules for the whole field validate; one more seat does not.
    let mut rules = MatchRules::hosted_for(&policy);
    assert_eq!(rules.seats, 73);
    assert!(rules.validate(&policy, 73).is_ok());
    rules.seats = 74;
    assert!(rules.validate(&policy, 73).is_err());
}

#[test]
fn a_hosted_rooms_seats_follow_its_authoritys_cap() {
    // An authority one build behind seats nine: Reset there asks for nine.
    let older = RoomPolicy {
        max_competitors: 9,
        points_bias: false,
        ..RoomPolicy::default()
    };
    let rules = MatchRules::hosted_for(&older);
    assert_eq!(rules.seats, 9);
    assert!(rules.validate(&older, 1).is_ok());
}

/// The numbers the member bound was chosen from.
#[test]
#[ignore]
fn what_a_large_room_costs() {
    println!(
        "| Players | Watching | State JSON | Join (reducer + state written) | A member's connection change |"
    );
    println!("| --- | --- | --- | --- | --- |");
    for watching in [0usize, 100, 200, 300, 500, 1000] {
        let policy = RoomPolicy {
            max_members: 2_000,
            ..RoomPolicy::default()
        };
        let mut s = server(policy);
        let total = 73 + watching;
        let mut joins = Vec::new();
        for i in 2..=total {
            let t = Instant::now();
            let id = s
                .join(Identity::new(format!("p:{i}")), &format!("Player {i}"), Intent::Play)
                .unwrap();
            // The server writes the room's state as one row on every change.
            let row = serde_json::to_string(s.state()).unwrap();
            std::hint::black_box(row.len());
            if i + 20 > total {
                joins.push(t.elapsed().as_secs_f64() * 1e6);
            }
            let _ = id;
        }
        let state = serde_json::to_string(s.state()).unwrap();
        let mut changes = Vec::new();
        for i in 2..22 {
            let t = Instant::now();
            let id = s
                .state()
                .member_by_identity(&Identity::new(format!("p:{i}")))
                .unwrap()
                .id;
            s.authority(Command::SetConnection {
                participant: id,
                connection: Connection::Reconnecting,
            })
            .unwrap();
            let row = serde_json::to_string(s.state()).unwrap();
            std::hint::black_box(row.len());
            changes.push(t.elapsed().as_secs_f64() * 1e6);
        }
        let mean = |v: &[f64]| v.iter().sum::<f64>() / v.len() as f64;
        println!(
            "| 73 | {watching} | {:.1} KB | {:.0} µs | {:.0} µs |",
            state.len() as f64 / 1024.0,
            mean(&joins),
            mean(&changes)
        );
    }
}
