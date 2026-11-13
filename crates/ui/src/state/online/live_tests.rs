//! An online room's live play from the client's side
//! (`docs/plans/multiplayer-online-play.md`), over the fake room
//! (`ddi_room::fake`): the live link on only where the room's policy
//! plays live, the room's clock, the battle plan offered by the picker and
//! compared by everyone, readiness to play live said before Loaded.

use super::tests::room::{client, person, tick};
use crate::state::room::RoomClient;
use ddi_room::fake::{FakeServer, SharedFake};
use ddi_room::{Clock, Identity, Intent, LinkState, ManualClock, RoomPolicy};

pub(crate) fn room_with(clock: &ManualClock, others: &[&str], policy: RoomPolicy) -> SharedFake {
    let server = FakeServer::hosted(
        "Room000001",
        Identity::new("dev:alice"),
        "alice",
        policy,
        clock.clone(),
    )
    .shared();
    for who in others {
        server
            .borrow_mut()
            .join(Identity::new(format!("dev:{who}")), who, Intent::Play)
            .unwrap();
    }
    server
}

/// A room that does not play live (an older server) is never sent a live
/// message: the socket stays open (a live frame there closes it with
/// 4400) and the client says it does not play live. One that does says
/// hello, and the room's clock is known within a few pings.
#[test]
fn the_live_link_is_on_only_where_the_room_plays_live() {
    let clock = ManualClock::new(5_000);
    let old = room_with(&clock, &["bob"], RoomPolicy::default());
    let mut alice = client(&old, person(&old, "alice"), "alice", 5_000);
    let mut bob = client(&old, person(&old, "bob"), "bob", 5_000);
    tick(&clock, &mut [&mut alice, &mut bob], 2_000);
    for r in [&alice, &bob] {
        assert_eq!(r.link_state(), LinkState::Connected);
        assert!(!r.plays_live());
        assert!(!r.live_link().unwrap().is_on());
        assert_eq!(r.room_now(), None);
    }
    assert_eq!(old.borrow().sockets(), 2, "nobody closed with 4400");

    let clock = ManualClock::new(5_000);
    let live = room_with(&clock, &["bob"], RoomPolicy::default().with_live());
    let mut alice = client(&live, person(&live, "alice"), "alice", 5_000);
    let mut bob = client(&live, person(&live, "bob"), "bob", 5_000);
    // Ticked as a frame loop does (every 10 ms): a ping's answer is read
    // on the next tick.
    for _ in 0..50 {
        tick(&clock, &mut [&mut alice, &mut bob], 40);
    }
    for r in [&alice, &bob] {
        assert_eq!(r.link_state(), LinkState::Connected);
        assert!(r.plays_live());
        assert!(r.live_link().unwrap().is_on());
        let now = r.room_now().expect("the room's clock").0;
        assert!(now.abs_diff(clock.now().0) <= 5, "{now} {}", clock.now().0);
    }
    // Practice never plays live.
    assert!(!RoomClient::new(5_000, 1).plays_live());
}
