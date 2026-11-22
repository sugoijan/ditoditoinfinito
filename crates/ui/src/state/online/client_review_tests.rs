//! The room's client's fixes from the code review of 2026-11-22 (round
//! RFb), each over the fake room.

use ddi_room::{ClientId, LinkState, Loading, ManualClock, ParticipantId, RoomId, Transport, TransportEvent};

use super::away_tests::Tab;
use super::review_tests::picked_and_prompted;
use super::tests::room::{client, person, room_of, shared_pick, song, tick};
use crate::state::room::RoomClient;

const BOB: ParticipantId = ParticipantId(2);

/// A tab whose socket's messages arrive between the page's runs: what came
/// is read at the next poll, so that a command sent between two ticks may
/// be the first to read a new socket's room.
#[derive(Clone)]
struct Lagged(Tab);

impl Transport for Lagged {
    fn open(&mut self, room: &RoomId, ticket: &str) {
        self.0.open(room, ticket)
    }
    fn send(&mut self, text: String) {
        self.0.send(text)
    }
    fn send_binary(&mut self, bytes: Vec<u8>) {
        self.0.send_binary(bytes)
    }
    fn binary(&self) -> bool {
        true
    }
    fn close(&mut self) {
        self.0.close()
    }
    fn poll(&mut self) -> Vec<TransportEvent> {
        let out = std::mem::take(&mut *self.0.came.borrow_mut());
        self.0.network();
        out
    }
}

/// Bob's client over a [`Lagged`] tab.
fn lagged_bob(server: &ddi_room::fake::SharedFake, tab: &Tab) -> RoomClient {
    RoomClient::online(
        person(server, "bob"),
        "Room000001",
        Box::new(Lagged(tab.clone())),
        ClientId::new("bob-web"),
        1_000,
    )
}

/// room-client 1: a member back from away past the room's patience whose
/// new socket's room is read by a command sent between two ticks (a
/// download's progress, a press) is still told the room again: the song
/// reported loaded again, so that they play. (Before: `connected_again`
/// ran only where a tick saw the link turn Connected, and the stage's
/// song was never reported again.)
#[test]
fn a_reconnect_first_read_by_a_command_still_tells_the_room_again() {
    use ddi_room::attacks::Targeting;
    let clock = ManualClock::new(1_000);
    let server = room_of(&clock, &["bob"]);
    server.borrow_mut().set_grace(Some(2_000));
    let mut alice = client(&server, person(&server, "alice"), "alice", 1_000);
    let tab = Tab::new(&server);
    let mut bob = lagged_bob(&server, &tab);
    for _ in 0..3 {
        tick(&clock, &mut [&mut alice, &mut bob], 400);
    }
    let stage = picked_and_prompted(&clock, &server, &mut alice, &mut bob);
    let loading = |server: &ddi_room::fake::SharedFake| {
        let s = server.borrow();
        s.state().stage(stage).unwrap().prep(BOB).loading
    };
    for _ in 0..3 {
        let s = song();
        for j in bob.take_load_jobs() {
            bob.finish_shared_load(&j, Ok(shared_pick(&s)));
        }
        tick(&clock, &mut [&mut alice, &mut bob], 400);
    }
    assert_eq!(loading(&server), Loading::Loaded);
    server.borrow_mut().drop_socket(BOB);
    tab.network();
    tick(&clock, &mut [&mut alice], 3_000);
    assert_eq!(loading(&server), Loading::NotStarted, "the room forgot it");
    let mut flip = false;
    let mut seen_by_send = false;
    for _ in 0..200 {
        clock.advance(10);
        let now = ddi_room::Clock::now(&clock).0;
        alice.tick(now);
        let before = bob.link_state();
        flip = !flip;
        let _ = bob.set_targeting(if flip { Targeting::Leader } else { Targeting::Spread });
        if before != LinkState::Connected && bob.link_state() == LinkState::Connected {
            seen_by_send = true;
        }
        bob.tick(now);
        if bob.link_state() == LinkState::Connected {
            break;
        }
    }
    assert!(seen_by_send, "the case under test: a send read the new socket's room");
    assert_eq!(bob.link_state(), LinkState::Connected);
    let s = song();
    for _ in 0..5 {
        for j in bob.take_load_jobs() {
            match j.share {
                Some(_) => bob.finish_shared_load(&j, Ok(shared_pick(&s))),
                None => bob.finish_load(&j, Ok(())),
            }
        }
        tick(&clock, &mut [&mut alice, &mut bob], 400);
    }
    assert_eq!(loading(&server), Loading::Loaded, "told the room again");
}

/// room-client 3: Ready pressed, then the stage's prompt left before the
/// room answered (a slow link, Back pressed twice at once): the Ready
/// asked is taken back too, so that the room never starts the stage with
/// a player who left its prompt.
#[test]
fn the_prompt_left_before_the_room_answered_ready_takes_it_back() {
    let clock = ManualClock::new(1_000);
    let server = room_of(&clock, &["bob"]);
    let mut alice = client(&server, person(&server, "alice"), "alice", 1_000);
    let tab = Tab::new(&server);
    let mut bob = lagged_bob(&server, &tab);
    for _ in 0..3 {
        tick(&clock, &mut [&mut alice, &mut bob], 400);
    }
    let stage = picked_and_prompted(&clock, &server, &mut alice, &mut bob);
    for _ in 0..3 {
        let s = song();
        for j in bob.take_load_jobs() {
            bob.finish_shared_load(&j, Ok(shared_pick(&s)));
        }
        tick(&clock, &mut [&mut alice, &mut bob], 400);
    }
    bob.set_ready(true).unwrap();
    assert!(bob.ready_as_asked());
    assert!(!bob.ready(), "the case under test: the room has not answered");
    let _ = bob.leave_prompt(stage);
    for _ in 0..3 {
        tick(&clock, &mut [&mut alice, &mut bob], 100);
    }
    let ready = server.borrow().state().stage(stage).unwrap().prep(BOB).ready;
    assert!(!ready, "Bob left the prompt but the room has him ready");
    assert!(!bob.ready_as_asked());
}

/// A song of three charts in one style, Easy, Medium and Hard (digests 7,
/// 8 and 9).
fn three_charts() -> crate::screens::data::SongEntry {
    use ddi_chart::Difficulty::*;
    let of = |index: usize, difficulty, meter, digest| crate::screens::data::ChartEntry {
        style: "dance-single".into(),
        index,
        difficulty,
        meter,
        digest: Some(ddi_chart::ChartDigest(digest)),
        ..Default::default()
    };
    crate::screens::data::SongEntry {
        id: "bundled:a".into(),
        title: "A".into(),
        charts: vec![of(0, Easy, 3, 7), of(1, Medium, 6, 8), of(2, Hard, 9, 9)],
        ..Default::default()
    }
}

/// room-client 2: under Charts: Adaptive a member's own start (their Easy
/// where the stage's chart is Hard) is told the room again after they
/// were away past its patience (the room forgot what they had told it):
/// every device and their play start them where their Ready card says.
/// (Before: the start was taken as told already and never sent again.)
#[test]
fn a_members_own_start_is_told_again_after_being_away() {
    use super::live_tests::room_with;
    use super::tests::room::upload;
    use crate::state::room::Picked;
    use ddi_room::RoomPolicy;
    let clock = ManualClock::new(1_000);
    let server = room_with(&clock, &["bob"], RoomPolicy::default().with_live());
    server.borrow_mut().set_grace(Some(2_000));
    let mut alice = client(&server, person(&server, "alice"), "alice", 1_000);
    let tab = Tab::new(&server);
    let mut bob = RoomClient::online(
        person(&server, "bob"),
        "Room000001",
        Box::new(tab.clone()),
        ClientId::new("bob-web"),
        1_000,
    );
    for _ in 0..20 {
        tick(&clock, &mut [&mut alice, &mut bob], 40);
    }
    let mut rules = alice.rules().clone();
    rules.charts = ddi_room::Charts::Adaptive;
    alice.save_rules(rules).unwrap();
    tick(&clock, &mut [&mut alice, &mut bob], 400);
    bob.set_preferred(ddi_chart::Difficulty::Easy);
    let s = three_charts();
    alice.pick(&s, &s.charts[2]).unwrap();
    tick(&clock, &mut [&mut alice, &mut bob], 400);
    upload(&server, &mut alice, "abc12345");
    tick(&clock, &mut [&mut alice, &mut bob], 400);
    let stage = alice.stages()[0].id;
    for j in alice.take_load_jobs() {
        alice.finish_load(&j, Ok(()));
    }
    for j in bob.take_load_jobs() {
        let picked = Picked {
            song: crate::screens::data::SongEntry {
                id: "shared:abc12345:0".into(),
                ..s.clone()
            },
            chart: s.charts[2].clone(),
        };
        bob.finish_shared_load(&j, Ok(picked));
    }
    tick(&clock, &mut [&mut alice, &mut bob], 400);
    let easy = Some(ddi_room::ChartKey::new(ddi_chart::ChartDigest(7).to_string()));
    let start = |server: &ddi_room::fake::SharedFake| server.borrow().state().start_of(stage, BOB).cloned();
    assert_eq!(start(&server), easy, "told the room");
    // Away past the room's patience: the room forgets it.
    server.borrow_mut().drop_socket(BOB);
    tab.network();
    tick(&clock, &mut [&mut alice], 3_000);
    assert_eq!(start(&server), None, "the room forgot it");
    // Back: told again.
    for _ in 0..10 {
        tick(&clock, &mut [&mut alice, &mut bob], 400);
    }
    assert_eq!(bob.link_state(), LinkState::Connected);
    assert_eq!(start(&server), easy, "told the room again");
    assert_eq!(
        bob.start_shown(stage, BOB).and_then(|c| c.digest),
        Some(ddi_chart::ChartDigest(7)),
        "his Ready card says the same"
    );
}
