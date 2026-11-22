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

/// room-client 4: a member who leaves while their socket is down (a blip,
/// Reconnecting) is still told the room: the shell lets the room's client
/// go, the kept clients drive its link until the socket is back and its
/// Leave is read, and the room has them left at once rather than away
/// until its patience ran out (a host's succession waiting for it).
#[test]
fn a_member_who_leaves_while_reconnecting_is_told_the_room() {
    use super::kept::Kept;
    use std::cell::RefCell;
    use std::rc::Rc;
    let clock = ManualClock::new(1_000);
    let server = room_of(&clock, &["bob"]);
    server.borrow_mut().set_grace(Some(60_000));
    let mut alice = client(&server, person(&server, "alice"), "alice", 1_000);
    let tab = Tab::new(&server);
    let kept = Kept::new();
    let bob = kept.room_or("Room000001", || {
        Rc::new(RefCell::new(RoomClient::online(
            person(&server, "bob"),
            "Room000001",
            Box::new(tab.clone()),
            ClientId::new("bob-web"),
            1_000,
        )))
    });
    for _ in 0..3 {
        tick(&clock, &mut [&mut alice, &mut bob.borrow_mut()], 400);
    }
    assert_eq!(bob.borrow().link_state(), LinkState::Connected);
    server.borrow_mut().drop_socket(BOB);
    tick(&clock, &mut [&mut bob.borrow_mut()], 40);
    assert!(matches!(bob.borrow().link_state(), LinkState::Reconnecting { .. }));
    // Leave room, as the screens do: told, then let go by the shell.
    bob.borrow_mut().leave();
    let left = kept.discard_room().expect("kept");
    assert!(Rc::ptr_eq(&left, &bob));
    drop((left, bob));
    for _ in 0..40 {
        clock.advance(100);
        let now = ddi_room::Clock::now(&clock).0;
        alice.tick(now);
        kept.drive_leaving(now);
    }
    let gone = server.borrow().state().member(BOB).is_some_and(|m| m.left);
    assert!(gone, "the room has Bob left");
    assert!(!kept.drive_leaving(ddi_room::Clock::now(&clock).0), "and lets the link go");
}

/// room-client, note: a member's page loaded again (or joined) during a
/// stage's Ready window after the group's draw was revealed does not
/// reveal it again: its reveal's time on the room's clock is past (as the
/// audience's seat reads it), so its prompt is not held back for it while
/// the window runs. A draw fresh on the room's clock is revealed as
/// before.
#[test]
fn a_draw_revealed_before_a_page_was_loaded_is_not_revealed_again() {
    use super::live_tests::room_with;
    use ddi_room::RoomPolicy;
    let clock = ManualClock::new(5_000);
    let server = room_with(&clock, &["bob", "carol"], RoomPolicy::default().with_live());
    server.borrow_mut().rewrite(|s| {
        s.next.rules.choice = ddi_room::SongChoice::Group;
        let list = std::sync::Arc::make_mut(&mut s.songs);
        for n in 1..=2u64 {
            let id = ddi_room::RoomSongId(list.next.max(1));
            list.next = id.0 + 1;
            list.entries.push(ddi_room::RoomSong {
                id,
                key: ddi_room::SongKey::new(format!("s{n:015x}")),
                card: ddi_room::SongCard {
                    title: format!("Song {n}"),
                    ..Default::default()
                },
                charts: vec![ddi_room::ChartLine {
                    chart: ddi_room::ChartKey::new(format!("d{n}")),
                    style: "dance-single".into(),
                    difficulty: "Hard".into(),
                    meter: 9,
                }],
                by: ParticipantId(1),
                added_at: s.revision,
                status: ddi_room::ContentStatus::Available,
                share: Some(format!("share{n}")),
                bundled: false,
            });
        }
    });
    let mut alice = client(&server, person(&server, "alice"), "alice", 5_000);
    let mut bob = client(&server, person(&server, "bob"), "bob", 5_000);
    let step = |ms: u64, rooms: &mut [&mut RoomClient]| {
        for _ in 0..ms / 50 {
            clock.advance(50);
            server.borrow_mut().pump();
            for r in rooms.iter_mut() {
                r.tick(ddi_room::Clock::now(&clock).0);
            }
        }
    };
    step(2_000, &mut [&mut alice, &mut bob]);
    bob.choose(ddi_room::RoomSongId(2), &ddi_room::ChartKey::new("d2")).unwrap();
    step(200, &mut [&mut alice, &mut bob]);
    alice.choose(ddi_room::RoomSongId(1), &ddi_room::ChartKey::new("d1")).unwrap();
    let mut revealed = false;
    for _ in 0..120 {
        step(50, &mut [&mut alice, &mut bob]);
        if bob.reveal().is_some() {
            revealed = true;
            break;
        }
    }
    assert!(revealed, "a fresh draw is revealed");
    // Its reveal over on the room's clock, Carol's page is loaded.
    step(4_000, &mut [&mut alice, &mut bob]);
    assert!(server.borrow().state().drawn.is_some());
    let mut carol = client(&server, person(&server, "carol"), "carol", ddi_room::Clock::now(&clock).0);
    for _ in 0..60 {
        step(50, &mut [&mut alice, &mut bob, &mut carol]);
        assert!(carol.reveal().is_none(), "revealed again on a page loaded after it");
    }
    assert_eq!(carol.link_state(), LinkState::Connected);
}

/// room-client, note: a room's link let go with its requests in flight
/// (the room's summary asked as it was made, a ticket) leaves none of
/// their answers kept unread in the online state for the session.
#[test]
fn a_link_let_go_leaves_no_answers_unread() {
    let clock = ManualClock::new(1_000);
    let server = room_of(&clock, &["bob"]);
    let online = person(&server, "bob");
    let mut bob = client(&server, online.clone(), "bob", 1_000);
    // Its invite asked (an operator's screen asks while there is none).
    bob.fetch_invite();
    drop(bob);
    for n in 0..4 {
        online.borrow_mut().poll(1_100 + n * 100);
    }
    assert_eq!(online.borrow().unread_answers(), 0);
}
