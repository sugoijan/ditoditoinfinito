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
pub(super) struct Tab {
    socket: Rc<RefCell<FakeSocket>>,
    pub(super) came: Rc<RefCell<Vec<TransportEvent>>>,
}

impl Tab {
    pub(super) fn new(server: &SharedFake) -> Tab {
        Tab {
            socket: Rc::new(RefCell::new(FakeSocket::new(server.clone()))),
            came: Rc::default(),
        }
    }

    /// The network runs though the page does not (a hidden tab): what
    /// the socket brings (its close too) is queued for the page.
    pub(super) fn network(&self) {
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
    // Shown again: at once, still Reconnecting (the link never reads
    // Joining for a moment) with its attempts counted on.
    assert!(bob.hurry());
    assert_eq!(bob.link_state(), LinkState::Reconnecting { attempt: 1 });
    tick(&clock, &mut [&mut alice, &mut bob], 4);
    tick(&clock, &mut [&mut alice, &mut bob], 4);
    assert_eq!(bob.link_state(), LinkState::Connected);
    assert_eq!(
        server.borrow().state().member(BOB).unwrap().connection,
        Connection::Connected
    );
    assert!(!bob.hurry());
}

/// Item 4: an administrator in a room as a plain member (their room
/// passed to someone else and they came back) is offered what a co-host
/// is, wherever the room has them as an administrator. (The room does not
/// yet: its record of a returning member keeps the flag it had,
/// `crates/room/tests/n1_proposals.rs`; here the room is put as the next
/// round's authority will have it.)
#[test]
fn an_administrator_who_is_a_member_is_offered_a_co_hosts_controls() {
    use ddi_room::{Capability, Role};
    let clock = ManualClock::new(1_000);
    let server = room_of(&clock, &["bob"]);
    let mut alice = client(&server, person(&server, "alice"), "alice", 1_000);
    let mut bob = client(&server, person(&server, "bob"), "bob", 1_000);
    tick(&clock, &mut [&mut alice, &mut bob], 400);
    tick(&clock, &mut [&mut alice, &mut bob], 400);
    assert!(!bob.may(Capability::EditNextRules));
    assert!(!bob.may_take_pick());
    server.borrow_mut().rewrite(|s| {
        s.members.get_mut(&BOB).unwrap().admin = true;
    });
    tick(&clock, &mut [&mut alice, &mut bob], 400);
    assert_eq!(bob.my_role(), Some((Role::Member, true)));
    for capability in [
        Capability::ChooseContent,
        Capability::EditNextRules,
        Capability::TakePick,
        Capability::LetPlay,
        Capability::ReplaceInvite,
    ] {
        assert!(bob.may(capability), "{capability:?}");
    }
    assert!(bob.may_take_pick(), "Pick songs is offered");
    // The owner's alone stay the owner's.
    assert!(!bob.manages_hosts());
    // And the lists say what they are.
    let row = crate::screens::room::player_rows(&alice)
        .into_iter()
        .find(|p| p.name == "bob")
        .unwrap();
    assert_eq!(row.role, "Admin");
    // The room takes it: the pick changes hands.
    bob.take_pick().unwrap();
    tick(&clock, &mut [&mut alice, &mut bob], 400);
    assert!(bob.chooses());
}

/// What a hidden tab's title says: the stage that waits for this
/// player's Ready, while it does.
#[test]
fn a_stage_waiting_for_the_players_ready_is_told() {
    let clock = ManualClock::new(1_000);
    let server = room_of(&clock, &["bob"]);
    let mut alice = client(&server, person(&server, "alice"), "alice", 1_000);
    let mut bob = client(&server, person(&server, "bob"), "bob", 1_000);
    tick(&clock, &mut [&mut alice, &mut bob], 400);
    assert_eq!(bob.ready_wanted(), None, "nothing picked");
    let stage = picked_and_prompted(&clock, &server, &mut alice, &mut bob);
    assert_eq!(bob.ready_wanted(), Some(1));
    bob.set_ready(true).unwrap();
    tick(&clock, &mut [&mut alice, &mut bob], 400);
    assert_eq!(bob.ready_wanted(), None, "ready");
    // Alice leaves the prompt: it is not asked of her again.
    assert_eq!(alice.ready_wanted(), Some(1));
    alice.leave_prompt(stage);
    assert_eq!(alice.ready_wanted(), None);
}

/// A member who was away through a stage's pick, or whose page was loaded
/// after the stage started, has not its song: where the room plays live
/// it is read from the room's share and the stage handed to them to
/// watch, with nothing reported to the room. (Before: they stayed on the
/// room's song select, told they sat the stage out, with nothing to see.)
#[test]
fn a_member_who_missed_a_stages_start_is_handed_it_within_the_late_join() {
    use super::live_tests::{beatucada, entry_of, room_with};
    use crate::state::room::{Picked, prepare_stage};
    use ddi_room::{Command, ContentStatus, Phase, RoomPolicy, SitOutReason};
    let clock = ManualClock::new(1_000);
    let server = room_with(&clock, &["bob", "carol"], RoomPolicy::default().with_live());
    let mut alice = client(&server, person(&server, "alice"), "alice", 1_000);
    let mut bob = client(&server, person(&server, "bob"), "bob", 1_000);
    for _ in 0..20 {
        tick(&clock, &mut [&mut alice, &mut bob], 40);
    }
    let (song, chart) = beatucada();
    let s = entry_of(&song, chart);
    alice.pick(&s, &s.charts[0]).unwrap();
    tick(&clock, &mut [&mut alice, &mut bob], 400);
    let stage = alice.stages()[0].id;
    for j in alice.take_upload_jobs() {
        server
            .borrow_mut()
            .authority(Command::SetContentStatus {
                stage: j.stage,
                status: ContentStatus::Available,
                share: Some("abc12345".into()),
            })
            .unwrap();
        alice.upload_finished(j.stage, Ok("abc12345".into()));
    }
    tick(&clock, &mut [&mut alice, &mut bob], 400);
    let picked = Picked {
        song: crate::screens::data::SongEntry {
            id: "shared:abc12345:0".into(),
            ..s.clone()
        },
        chart: s.charts[0].clone(),
    };
    // Alice and Bob load it and are ready; Carol's page is not there.
    let ja = alice.take_load_jobs();
    let live = prepare_stage(&ja[0], &song, &mut |_| {}).unwrap().live;
    alice.finish_load_live(&ja[0], Ok(live.clone()));
    let jb = bob.take_load_jobs();
    bob.finish_shared_load_live(&jb[0], Ok((picked.clone(), live)));
    for _ in 0..4 {
        tick(&clock, &mut [&mut alice, &mut bob], 200);
    }
    alice.set_ready(true).unwrap();
    bob.set_ready(true).unwrap();
    for _ in 0..40 {
        tick(&clock, &mut [&mut alice, &mut bob], 2_000);
        if alice.state().phase == (Phase::Stage { stage }) {
            break;
        }
    }
    assert_eq!(alice.state().phase, Phase::Stage { stage });
    // Carol's page, loaded now: she sat the stage out and has no song.
    let mut carol = client(&server, person(&server, "carol"), "carol", 1_000);
    for _ in 0..10 {
        tick(&clock, &mut [&mut alice, &mut bob, &mut carol], 40);
    }
    let play = carol.state().stage(stage).unwrap().play.clone().unwrap();
    assert!(matches!(
        play.sat_out.get(&carol.me()),
        Some(SitOutReason::NotConnected | SitOutReason::NotReady)
    ));
    let jobs = carol.take_load_jobs();
    assert_eq!(jobs.len(), 1, "the song to watch the stage by");
    assert_eq!(jobs[0].share.as_deref(), Some("abc12345"));
    assert!(carol.take_stage_order().is_none(), "not before its song is here");
    let revision = server.borrow().state().revision;
    carol.finish_shared_load_live(&jobs[0], Ok((picked, None)));
    tick(&clock, &mut [&mut alice, &mut bob, &mut carol], 400);
    let order = carol.take_stage_order().expect("handed the stage");
    assert_eq!(order.stage, stage);
    // Round P1b: her page loaded within the late join of the song's time
    // zero, she is handed its Ready screen, not a watcher's (she may still
    // enter it: her song reported loaded); past the late join it is
    // watched as before (the room's no-show ends the others' streams
    // here, so that is `the_screens`' test,
    // `a_member_who_missed_the_window_may_still_ready_within_the_late_join`).
    assert!(carol.may_enter_late(stage));
    assert!(!order.watching);
    assert!(server.borrow().state().revision > revision, "her load told");
    assert!(carol.refusal().is_none());
    assert!(carol.take_load_jobs().is_empty(), "asked for once");
}

/// A stage's song's load is told the room as it comes, in tenths: the
/// lists say "Loading 40%" (they said "Loading" alone: no shell told how
/// far it was).
#[test]
fn a_songs_load_is_told_the_room_as_it_comes() {
    use super::tests::room::upload;
    let clock = ManualClock::new(1_000);
    let server = room_of(&clock, &["bob"]);
    let mut alice = client(&server, person(&server, "alice"), "alice", 1_000);
    let mut bob = client(&server, person(&server, "bob"), "bob", 1_000);
    tick(&clock, &mut [&mut alice, &mut bob], 400);
    let s = song();
    alice.pick(&s, &s.charts[0]).unwrap();
    tick(&clock, &mut [&mut alice, &mut bob], 400);
    upload(&server, &mut alice, "abc12345");
    tick(&clock, &mut [&mut alice, &mut bob], 400);
    let stage = alice.stages()[0].id;
    let jobs = bob.take_load_jobs();
    let says = |alice: &RoomClient| {
        crate::screens::room::player_rows(alice)
            .into_iter()
            .find(|p| p.name == "bob")
            .unwrap()
            .state
    };
    assert_eq!(says(&alice), "Loading");
    let revision = |server: &SharedFake| server.borrow().state().revision;
    // 4% of it: nothing said yet; 43%: said once, whatever comes within
    // the same tenth.
    bob.load_progress(&jobs[0], 40.0, 1_000.0);
    tick(&clock, &mut [&mut alice, &mut bob], 400);
    assert_eq!(says(&alice), "Loading");
    let before = revision(&server);
    for done in [430.0, 440.0, 499.0] {
        bob.load_progress(&jobs[0], done, 1_000.0);
    }
    tick(&clock, &mut [&mut alice, &mut bob], 400);
    assert_eq!(says(&alice), "Loading 40%");
    assert_eq!(revision(&server).0, before.0 + 1, "one report a tenth");
    // The whole of it is not Loaded: its preparation says that.
    bob.load_progress(&jobs[0], 1_000.0, 1_000.0);
    tick(&clock, &mut [&mut alice, &mut bob], 400);
    assert_eq!(says(&alice), "Loading 90%");
    assert_eq!(
        server.borrow().state().stage(stage).unwrap().prep(BOB).loading,
        Loading::Loading { permille: 900 }
    );
    bob.finish_shared_load(&jobs[0], Ok(shared_pick(&s)));
    tick(&clock, &mut [&mut alice, &mut bob], 400);
    assert_eq!(says(&alice), "Not ready");
    // Nothing more is said of a song that is loaded, nor without a whole.
    let before = revision(&server);
    bob.load_progress(&jobs[0], 950.0, 1_000.0);
    bob.load_progress(&jobs[0], 500.0, 0.0);
    tick(&clock, &mut [&mut alice, &mut bob], 400);
    assert_eq!(revision(&server), before);
}
