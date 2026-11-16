//! Feedback round N1 (2026-11-16): members the room counted away (a tab
//! in the background whose socket went) and their way back into a stage,
//! over the fake room.

use std::cell::RefCell;
use std::rc::Rc;

use ddi_room::fake::{FakeSocket, SharedFake};
use ddi_room::{
    ClientId, Connection, LinkState, Loading, ManualClock, ParticipantId, RoomId, Transport,
    TransportEvent,
};

use super::review_tests::picked_and_prompted;
use super::tests::room::{client, person, room_of, shared_pick, song, tick};
use crate::state::room::RoomClient;

const BOB: ParticipantId = ParticipantId(2);

/// A browser tab's socket: the browser reads it whether or not the page
/// runs ([`Tab::network`]), and the page takes what came at its next run.
#[derive(Clone)]
struct Tab {
    socket: Rc<RefCell<FakeSocket>>,
    came: Rc<RefCell<Vec<TransportEvent>>>,
}

impl Tab {
    fn new(server: &SharedFake) -> Tab {
        Tab {
            socket: Rc::new(RefCell::new(FakeSocket::new(server.clone()))),
            came: Rc::default(),
        }
    }

    /// The network runs though the page does not (a hidden tab): what
    /// the socket brings (its close too) is queued for the page.
    fn network(&self) {
        let got = self.socket.borrow_mut().poll();
        self.came.borrow_mut().extend(got);
    }
}

impl Transport for Tab {
    fn open(&mut self, room: &RoomId, ticket: &str) {
        self.socket.borrow_mut().open(room, ticket)
    }
    fn send(&mut self, text: String) {
        self.socket.borrow_mut().send(text)
    }
    fn send_binary(&mut self, bytes: Vec<u8>) {
        self.socket.borrow_mut().send_binary(bytes)
    }
    fn binary(&self) -> bool {
        true
    }
    fn close(&mut self) {
        self.socket.borrow_mut().close()
    }
    fn poll(&mut self) -> Vec<TransportEvent> {
        self.network();
        std::mem::take(&mut *self.came.borrow_mut())
    }
}

/// Bob's client on a tab's socket.
fn tab_client(server: &SharedFake, tab: &Tab) -> RoomClient {
    RoomClient::online(
        person(server, "bob"),
        "Room000001",
        Box::new(tab.clone()),
        ClientId::new("bob-web"),
        1_000,
    )
}

/// A member who had a stage's song loaded goes away past the room's
/// patience while its readiness gathers (the room forgets what an away
/// member had loaded) and comes back: their device says the song is
/// loaded again, so that their Ready counts and the stage starts with
/// them. (Before: the song was never reported again, the room showed them
/// Loading for good and they sat the stage out.)
#[test]
fn a_member_back_from_away_has_the_song_loaded_again_and_plays() {
    let clock = ManualClock::new(1_000);
    let server = room_of(&clock, &["bob"]);
    server.borrow_mut().set_grace(Some(2_000));
    let mut alice = client(&server, person(&server, "alice"), "alice", 1_000);
    let tab = Tab::new(&server);
    let mut bob = tab_client(&server, &tab);
    let stage = picked_and_prompted(&clock, &server, &mut alice, &mut bob);
    let loading = |server: &ddi_room::fake::SharedFake| {
        let s = server.borrow();
        s.state().stage(stage).unwrap().prep(BOB).loading
    };
    assert_eq!(loading(&server), Loading::Loaded);
    // Bob's socket goes and his page does not run (a hidden tab): away.
    server.borrow_mut().drop_socket(BOB);
    tab.network();
    tick(&clock, &mut [&mut alice], 3_000);
    assert_eq!(
        server.borrow().state().member(BOB).unwrap().connection,
        Connection::Disconnected
    );
    assert_eq!(loading(&server), Loading::NotStarted, "the room forgot it");
    // Back: connected again, and the song reported loaded again.
    tick(&clock, &mut [&mut alice, &mut bob], 2_000);
    assert_eq!(bob.link_state(), LinkState::Connected);
    let s = song();
    for _ in 0..3 {
        for j in bob.take_load_jobs() {
            match j.share {
                Some(_) => bob.finish_shared_load(&j, Ok(shared_pick(&s))),
                None => bob.finish_load(&j, Ok(())),
            }
        }
        tick(&clock, &mut [&mut alice, &mut bob], 400);
    }
    assert_eq!(loading(&server), Loading::Loaded, "told the room again");
    // Both ready: the stage starts with Bob in it.
    alice.set_ready(true).unwrap();
    bob.set_ready(true).unwrap();
    tick(&clock, &mut [&mut alice, &mut bob], 400);
    tick(&clock, &mut [&mut alice, &mut bob], 6_000);
    assert!(bob.stage_started(stage), "Bob plays the stage");
    assert!(bob.in_stage(stage));
}

/// A page shown again whose socket went while it stood still does not
/// wait out the reconnect's backoff: it asks for its ticket at once.
#[test]
fn a_page_shown_again_reconnects_at_once() {
    let clock = ManualClock::new(1_000);
    let server = room_of(&clock, &["bob"]);
    let mut alice = client(&server, person(&server, "alice"), "alice", 1_000);
    let tab = Tab::new(&server);
    let mut bob = tab_client(&server, &tab);
    tick(&clock, &mut [&mut alice, &mut bob], 400);
    tick(&clock, &mut [&mut alice, &mut bob], 400);
    assert_eq!(bob.link_state(), LinkState::Connected);
    assert!(!bob.hurry(), "nothing waits while connected");
    // The socket goes; the page's next run finds it closed and waits.
    server.borrow_mut().drop_socket(BOB);
    tick(&clock, &mut [&mut alice, &mut bob], 4);
    assert_eq!(bob.link_state(), LinkState::Reconnecting { attempt: 1 });
    tick(&clock, &mut [&mut alice, &mut bob], 40);
    assert_ne!(bob.link_state(), LinkState::Connected, "the backoff waited out");
    // Shown again: at once.
    assert!(bob.hurry());
    tick(&clock, &mut [&mut alice, &mut bob], 4);
    tick(&clock, &mut [&mut alice, &mut bob], 4);
    assert_eq!(bob.link_state(), LinkState::Connected);
    assert_eq!(
        server.borrow().state().member(BOB).unwrap().connection,
        Connection::Connected
    );
    assert!(!bob.hurry());
}
