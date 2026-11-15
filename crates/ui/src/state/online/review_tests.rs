//! The room's client's fixes from the code review of 2026-11-15 (fix
//! round G1), each over the fake room.

use std::cell::RefCell;
use std::collections::VecDeque;
use std::rc::Rc;

use ddi_room::fake::SharedFake;
use ddi_room::{LinkState, ManualClock, ParticipantId};

use super::tests::room::{client, handler_of, room_of, tick};
use super::tests::FakeHttp;
use super::*;

/// `who`'s online state as a page loaded on the room's own address has
/// it: nobody asked who is signed in yet (the session is the cookie's).
fn reloaded(server: &SharedFake, who: &'static str) -> SharedOnline {
    let http = FakeHttp {
        handler: Rc::new(RefCell::new(handler_of(server, who))),
        log: Rc::new(RefCell::new(Vec::new())),
        out: VecDeque::new(),
    };
    Online::new(
        Box::new(http),
        OnlineConfig {
            platform: Platform::Web,
            site: "http://localhost:8080/".into(),
        },
    )
    .shared()
}

/// ui-state-1: a room's page reloaded (its `Online` knowing nobody yet)
/// finds who it is once the account is known: its own row, its roles.
#[test]
fn a_reloaded_rooms_page_knows_who_it_is() {
    let clock = ManualClock::new(1_000);
    let server = room_of(&clock, &["bob"]);
    let online = reloaded(&server, "bob");
    assert!(online.borrow().me().is_none());
    let mut bob = client(&server, online.clone(), "bob", 1_000);
    tick(&clock, &mut [&mut bob], 400);
    tick(&clock, &mut [&mut bob], 400);
    assert_eq!(bob.link_state(), LinkState::Connected);
    assert!(online.borrow().me().is_some(), "the account asked for");
    assert_eq!(bob.me(), ParticipantId(2), "bob's own row");
    // The owner too: their roles back.
    let online = reloaded(&server, "alice");
    let mut alice = client(&server, online, "alice", 1_000);
    tick(&clock, &mut [&mut alice], 400);
    tick(&clock, &mut [&mut alice], 400);
    assert_eq!(alice.me(), ParticipantId(1));
    assert!(alice.chooses(), "the picker chooses again");
}

/// Alice picks the room's song and Bob follows; both have it and their
/// prompts are handed over. The stage's id.
fn picked_and_prompted(
    clock: &ManualClock,
    server: &SharedFake,
    alice: &mut crate::state::room::RoomClient,
    bob: &mut crate::state::room::RoomClient,
) -> ddi_room::StageId {
    use super::tests::room::{shared_pick, song, upload};
    tick(clock, &mut [&mut *alice, &mut *bob], 400);
    let s = song();
    alice.pick(&s, &s.charts[0]).unwrap();
    tick(clock, &mut [&mut *alice, &mut *bob], 400);
    upload(server, alice, "abc12345");
    tick(clock, &mut [&mut *alice, &mut *bob], 400);
    for j in alice.take_load_jobs() {
        alice.finish_load(&j, Ok(()));
    }
    for j in bob.take_load_jobs() {
        bob.finish_shared_load(&j, Ok(shared_pick(&s)));
    }
    tick(clock, &mut [&mut *alice, &mut *bob], 400);
    let stage = alice.stages()[0].id;
    assert!(alice.take_stage_order().is_some());
    assert!(bob.take_stage_order().is_some());
    stage
}

/// ui-state-2, gathering: a follower's Back on the stage's prompt
/// withdraws them to the room's song select, where they stay while the
/// window lasts; a new window (the pick made again) opens it again.
#[test]
fn a_followers_back_on_the_prompt_stays_on_song_select() {
    use crate::state::room::{PromptExit, RoomView};
    let clock = ManualClock::new(1_000);
    let server = room_of(&clock, &["bob"]);
    let mut alice = client(&server, super::tests::room::person(&server, "alice"), "alice", 1_000);
    let mut bob = client(&server, super::tests::room::person(&server, "bob"), "bob", 1_000);
    let stage = picked_and_prompted(&clock, &server, &mut alice, &mut bob);
    assert_eq!(bob.leave_prompt(stage), PromptExit::Withdraw);
    tick(&clock, &mut [&mut alice, &mut bob], 400);
    assert_eq!(bob.view(), RoomView::Select);
    assert!(bob.take_stage_order().is_none(), "not handed back");
    // The picker backs out and picks again: a new window, Bob's prompt.
    assert_eq!(alice.leave_prompt(stage), PromptExit::Withdraw);
    tick(&clock, &mut [&mut alice, &mut bob], 400);
    let s = super::tests::room::song();
    alice.pick(&s, &s.charts[0]).unwrap();
    tick(&clock, &mut [&mut alice, &mut bob], 400);
    assert_eq!(bob.view(), RoomView::Prompt(stage));
    assert!(bob.take_stage_order().is_some());
}

/// ui-state-2, once the stage is on: an online player who leaves its
/// prompt forfeits it and is not handed the same stage again.
#[test]
fn a_forfeit_is_not_handed_the_stage_again() {
    use crate::state::room::{PromptExit, RoomView};
    let clock = ManualClock::new(1_000);
    let server = room_of(&clock, &["bob"]);
    let mut alice = client(&server, super::tests::room::person(&server, "alice"), "alice", 1_000);
    let mut bob = client(&server, super::tests::room::person(&server, "bob"), "bob", 1_000);
    let stage = picked_and_prompted(&clock, &server, &mut alice, &mut bob);
    alice.set_ready(true).unwrap();
    bob.set_ready(true).unwrap();
    tick(&clock, &mut [&mut alice, &mut bob], 400);
    tick(&clock, &mut [&mut alice, &mut bob], 6_000);
    assert!(bob.stage_started(stage));
    assert_eq!(bob.leave_prompt(stage), PromptExit::Forfeit);
    bob.report_run(stage, ddi_room::Run::Forfeit, None).unwrap();
    tick(&clock, &mut [&mut alice, &mut bob], 400);
    assert_eq!(alice.state().phase, ddi_room::Phase::Stage { stage });
    assert_eq!(bob.view(), RoomView::Select);
    assert!(bob.take_stage_order().is_none(), "never the forfeited stage again");
}
