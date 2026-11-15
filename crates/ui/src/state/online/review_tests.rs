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
