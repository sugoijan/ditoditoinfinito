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
pub(super) fn picked_and_prompted(
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

/// First pass 3: joined to watch a full room that plays live, the line
/// that says so keeps its time from the connection when a stage's prompt
/// opens for the watcher (the rule that clears notices at a prompt is for
/// the others).
#[test]
fn joined_to_watch_is_said_through_a_prompt_opening() {
    use super::link::LinkNotice;
    use super::tests::INVITE;
    use super::tests::room::{person, shared_pick, song, upload};
    use crate::state::room::RoomView;
    use ddi_room::fake::FakeServer;
    use ddi_room::{Identity, RoomPolicy};
    let clock = ManualClock::new(1_000);
    let server = FakeServer::hosted(
        "Room000001",
        Identity::new("dev:alice"),
        "alice",
        RoomPolicy {
            max_competitors: 2,
            ..RoomPolicy::default().with_live()
        },
        clock.clone(),
    )
    .shared();
    let mut alice = client(&server, person(&server, "alice"), "alice", 1_000);
    let mut bob = client(&server, person(&server, "bob"), "bob", 1_000);
    server
        .borrow_mut()
        .join(ddi_room::Identity::new("dev:bob"), "bob", ddi_room::Intent::Play)
        .unwrap();
    let carol_online = person(&server, "carol");
    carol_online.borrow_mut().open_invite(INVITE);
    carol_online.borrow_mut().poll(3);
    carol_online.borrow_mut().join_invite();
    carol_online.borrow_mut().poll(4);
    let mut carol = client(&server, carol_online, "carol", 1_000);
    tick(&clock, &mut [&mut alice, &mut bob, &mut carol], 400);
    assert!(carol.watching());
    assert!(carol.watches(), "a live room's stages are watched");
    assert_eq!(carol.link_notice(), Some(LinkNotice::JoinedToWatch));
    let s = song();
    alice.pick(&s, &s.charts[0]).unwrap();
    tick(&clock, &mut [&mut alice, &mut bob, &mut carol], 400);
    upload(&server, &mut alice, "abc12345");
    tick(&clock, &mut [&mut alice, &mut bob, &mut carol], 400);
    for j in carol.take_load_jobs() {
        carol.finish_shared_load(&j, Ok(shared_pick(&s)));
    }
    tick(&clock, &mut [&mut alice, &mut bob, &mut carol], 400);
    let stage = alice.stages()[0].id;
    assert_eq!(carol.view(), RoomView::Prompt(stage));
    assert_eq!(
        carol.link_notice(),
        Some(LinkNotice::JoinedToWatch),
        "still said, its 12 s not over"
    );
    tick(&clock, &mut [&mut alice, &mut bob, &mut carol], 12_000);
    assert_eq!(carol.link_notice(), None, "gone at its time");
}

/// First pass 5: the picker's song select resting on no song (a search
/// that finds nothing) clears the room's spotlight after the same rest,
/// so the others no longer preview a song the picker left.
#[test]
fn resting_on_no_song_clears_the_spotlight() {
    use super::tests::room::song;
    let clock = ManualClock::new(1_000);
    let server = room_of(&clock, &["bob"]);
    let mut alice = client(&server, super::tests::room::person(&server, "alice"), "alice", 1_000);
    let mut bob = client(&server, super::tests::room::person(&server, "bob"), "bob", 1_000);
    tick(&clock, &mut [&mut alice, &mut bob], 400);
    let s = song();
    alice.hover(Some((&s, &s.charts[0])));
    tick(&clock, &mut [&mut alice, &mut bob], 3_600);
    assert!(bob.state().spotlight.is_some());
    alice.hover(None);
    tick(&clock, &mut [&mut alice, &mut bob], 400);
    assert!(bob.state().spotlight.is_some(), "not rested yet");
    tick(&clock, &mut [&mut alice, &mut bob], 3_600);
    assert!(bob.state().spotlight.is_none(), "cleared");
    // Back on a song: told again.
    alice.hover(Some((&s, &s.charts[0])));
    tick(&clock, &mut [&mut alice, &mut bob], 3_600);
    assert!(bob.state().spotlight.is_some());
}

/// ui-state-4: Retry sharing on a song the room refused (no chosen chart
/// in what was shared) asks for a grant and uploads it again; nothing is
/// uploaded again by itself.
#[test]
fn retry_sharing_on_a_refused_song_uploads_it_again() {
    use super::tests::room::{person, song};
    use ddi_room::{Command, ContentStatus};
    let clock = ManualClock::new(1_000);
    let server = room_of(&clock, &["bob"]);
    let mut alice = client(&server, person(&server, "alice"), "alice", 1_000);
    let mut bob = client(&server, person(&server, "bob"), "bob", 1_000);
    tick(&clock, &mut [&mut alice, &mut bob], 400);
    let s = song();
    alice.pick(&s, &s.charts[0]).unwrap();
    tick(&clock, &mut [&mut alice, &mut bob], 400);
    let stage = alice.stages()[0].id;
    let jobs = alice.take_upload_jobs();
    assert_eq!(jobs.len(), 1);
    server
        .borrow_mut()
        .authority(Command::SetContentStatus {
            stage,
            status: ContentStatus::Unavailable {
                code: "digest_mismatch".into(),
            },
            share: None,
        })
        .unwrap();
    alice.upload_finished(stage, Ok("abc12345".into()));
    tick(&clock, &mut [&mut alice, &mut bob], 400);
    assert!(alice.may_retry_sharing(stage));
    assert!(alice.take_upload_jobs().is_empty(), "not again by itself");
    alice.retry_sharing(stage);
    tick(&clock, &mut [&mut alice, &mut bob], 400);
    let again = alice.take_upload_jobs();
    assert_eq!(again.len(), 1, "uploaded again when asked");
    assert_eq!(again[0].grant, "grant-alice-2");
    assert!(!alice.may_retry_sharing(stage), "not offered while under way");
    tick(&clock, &mut [&mut alice, &mut bob], 400);
    assert!(alice.take_upload_jobs().is_empty(), "once");
}

/// Song `n` of the fake library: its own id and chart digest.
fn song_n(n: u8) -> crate::screens::data::SongEntry {
    let mut s = super::tests::room::song();
    s.id = format!("bundled:{n}");
    s.title = format!("Song {n}");
    s.charts[0].digest = Some(ddi_chart::ChartDigest(u128::from(n)));
    // An imported song's pack (a bundled song has none; both are uploaded
    // since round DB).
    s.pack = Some("Pack".into());
    s
}

/// Revision 17 (round RL1): songs added to the room's list are uploaded
/// one at a time, each under its own entry's grant; an upload whose entry
/// was removed meanwhile ends changing nothing, and the next one goes on.
#[test]
fn songs_added_are_uploaded_one_at_a_time_and_a_removed_ones_end_changes_nothing() {
    use super::tests::room::person;
    use ddi_room::{Command, ContentStatus};
    let clock = ManualClock::new(1_000);
    let server = room_of(&clock, &["bob"]);
    let mut alice = client(&server, person(&server, "alice"), "alice", 1_000);
    let mut bob = client(&server, person(&server, "bob"), "bob", 1_000);
    tick(&clock, &mut [&mut alice, &mut bob], 400);
    let (a, b) = (song_n(7), song_n(8));
    alice.set_library(&[a.clone(), b.clone()]);
    alice.add_song(&a).unwrap();
    alice.add_song(&b).unwrap();
    tick(&clock, &mut [&mut alice, &mut bob], 400);
    tick(&clock, &mut [&mut alice, &mut bob], 400);
    let ids: Vec<_> = alice.room_songs().iter().map(|e| e.id).collect();
    assert_eq!(ids.len(), 2);
    let jobs = alice.take_upload_jobs();
    assert_eq!(jobs.len(), 1, "one at a time");
    assert_eq!(jobs[0].entry, Some(ids[0]));
    // A is removed while its upload runs; its end changes nothing.
    alice.remove_song(ids[0]).unwrap();
    tick(&clock, &mut [&mut alice, &mut bob], 400);
    assert_eq!(bob.room_songs().len(), 1);
    alice.song_upload_finished(ids[0], Ok("sharea01".into()));
    tick(&clock, &mut [&mut alice, &mut bob], 400);
    tick(&clock, &mut [&mut alice, &mut bob], 400);
    let jobs = alice.take_upload_jobs();
    assert_eq!(jobs.len(), 1, "B's upload next");
    assert_eq!(jobs[0].entry, Some(ids[1]));
    assert_eq!(jobs[0].song.id, b.id);
    server
        .borrow_mut()
        .authority(Command::SetSongContent {
            song: ids[1],
            status: ContentStatus::Available,
            share: Some("shareb01".into()),
            charts: None,
        })
        .unwrap();
    alice.song_upload_finished(ids[1], Ok("shareb01".into()));
    tick(&clock, &mut [&mut alice, &mut bob], 400);
    assert!(alice.take_upload_jobs().is_empty());
    assert_eq!(bob.room_songs()[0].share.as_deref(), Some("shareb01"));
}


/// Round DB (revision 19): a bundled song (no pack) added to the room's
/// list is uploaded as any song, under its entry's grant; a member whose
/// library has it plays their own copy once it is picked (a load from the
/// library, nothing downloaded). An entry a room stored before revision 19
/// listed as bundled (no share) plays the same from a device that has it;
/// one that has not reports it unreadable.
#[test]
fn a_bundled_song_is_uploaded_and_a_device_that_has_it_plays_its_own_copy() {
    use super::tests::room::person;
    use ddi_room::{Command, ContentStatus, Loading};
    let clock = ManualClock::new(1_000);
    let server = room_of(&clock, &["bob", "carol"]);
    let mut alice = client(&server, person(&server, "alice"), "alice", 1_000);
    let mut bob = client(&server, person(&server, "bob"), "bob", 1_000);
    let mut carol = client(&server, person(&server, "carol"), "carol", 1_000);
    tick(&clock, &mut [&mut alice, &mut bob, &mut carol], 400);
    let mut bundled = song_n(7);
    bundled.pack = None;
    alice.set_library(std::slice::from_ref(&bundled));
    bob.set_library(std::slice::from_ref(&bundled));
    alice.add_song(&bundled).unwrap();
    tick(&clock, &mut [&mut alice, &mut bob, &mut carol], 400);
    tick(&clock, &mut [&mut alice, &mut bob, &mut carol], 400);
    let entry = alice.room_songs()[0].clone();
    assert!(!entry.bundled && entry.status == ContentStatus::Pending);
    let jobs = alice.take_upload_jobs();
    assert_eq!(jobs.len(), 1, "uploaded as any song");
    assert_eq!((jobs[0].entry, jobs[0].song.id.as_str()), (Some(entry.id), "bundled:7"));
    server
        .borrow_mut()
        .authority(Command::SetSongContent {
            song: entry.id,
            status: ContentStatus::Available,
            share: Some("share007".into()),
            charts: None,
        })
        .unwrap();
    alice.song_upload_finished(entry.id, Ok("share007".into()));
    tick(&clock, &mut [&mut alice, &mut bob, &mut carol], 400);
    let entry = bob.room_songs()[0].clone();
    assert_eq!(bob.entry_state(&entry), crate::state::room::EntryState::Ready);
    // A room stored before revision 19: a second bundled song listed by
    // its content alone, nothing uploaded.
    let mut stored = song_n(9);
    stored.pack = None;
    alice.set_library(&[bundled.clone(), stored.clone()]);
    bob.set_library(&[bundled.clone(), stored.clone()]);
    let mut old = entry.clone();
    old.id = ddi_room::RoomSongId(entry.id.0 + 1);
    old.key = ddi_room::SongKey::new(crate::state::room::song_key(&stored).unwrap());
    old.charts[0].chart = ddi_room::ChartKey::new(stored.charts[0].digest.unwrap().to_string());
    old.share = None;
    old.bundled = true;
    old.status = ContentStatus::Available;
    server.borrow_mut().rewrite(|s| {
        let list = std::sync::Arc::make_mut(&mut s.songs);
        list.next = old.id.0 + 1;
        list.entries.push(old.clone());
    });
    tick(&clock, &mut [&mut alice, &mut bob, &mut carol], 400);
    let chart = old.charts[0].chart.clone();
    alice.pick_listed(old.id, &chart).unwrap();
    for _ in 0..3 {
        tick(&clock, &mut [&mut alice, &mut bob, &mut carol], 400);
    }
    let jobs = bob.take_load_jobs();
    assert_eq!(jobs.len(), 1);
    assert_eq!((jobs[0].share.as_deref(), jobs[0].song.as_str()), (None, "bundled:9"), "its own copy");
    assert!(carol.take_load_jobs().is_empty(), "nothing to read it from");
    let st = server.borrow().state().clone();
    let stage = st.upcoming_stage().unwrap();
    let carol_id = carol.me();
    assert!(
        matches!(st.stage(stage).unwrap().prep(carol_id).loading, Loading::Failed { .. }),
        "{:?}",
        st.stage(stage).unwrap().prep(carol_id).loading
    );
}

/// `who`'s online state over the fake room whose invite endpoint is
/// `invite`'s: read as it stands, replaced by any operator, failing while
/// `down` says so.
fn with_invite(
    server: &SharedFake,
    who: &'static str,
    invite: Rc<RefCell<String>>,
    down: Rc<RefCell<bool>>,
) -> SharedOnline {
    use super::api::Method;
    use super::tests::{err, ok};
    let mut rest = handler_of(server, who);
    let handler: super::tests::Handler = Box::new(move |req| match (req.method, req.path.as_str()) {
        (Method::Get, "/rooms/Room000001/invite") if *down.borrow() => err(503, "unavailable"),
        (Method::Get, "/rooms/Room000001/invite") => ok(&format!(
            r#"{{"invite":"{}","expires_at_ms":9,"admission":"open"}}"#,
            invite.borrow()
        )),
        (Method::Post, "/rooms/Room000001/invite") => {
            let next = format!("{}X", invite.borrow());
            *invite.borrow_mut() = next.clone();
            ok(&format!(r#"{{"invite":"{next}","expires_at_ms":9,"admission":"open"}}"#))
        }
        _ => rest(req),
    });
    let online = Online::new(
        Box::new(FakeHttp {
            handler: Rc::new(RefCell::new(handler)),
            log: Rc::new(RefCell::new(Vec::new())),
            out: VecDeque::new(),
        }),
        OnlineConfig {
            platform: Platform::Web,
            site: "http://localhost:8080/".into(),
        },
    )
    .shared();
    online.borrow_mut().check();
    online.borrow_mut().poll(0);
    online
}

/// ui-state-7: one operator replaces the invite; the other's copy of it
/// is read again (More opened), not the dead link; and a failed read of
/// the invite is asked again after a while.
#[test]
fn an_invite_replaced_by_another_operator_and_a_failed_read_are_read_again() {
    use ddi_room::Command;
    let clock = ManualClock::new(1_000);
    let server = room_of(&clock, &["bob"]);
    server
        .borrow_mut()
        .authority(Command::SetCoHost {
            participant: ParticipantId(2),
            on: true,
        })
        .unwrap();
    let invite = Rc::new(RefCell::new("Room000001.abcdefghijABCDEFGHIJ01".to_string()));
    let down = Rc::new(RefCell::new(true));
    let mut alice = client(
        &server,
        with_invite(&server, "alice", invite.clone(), down.clone()),
        "alice",
        1_000,
    );
    let mut bob = client(
        &server,
        with_invite(&server, "bob", invite.clone(), Rc::new(RefCell::new(false))),
        "bob",
        1_000,
    );
    tick(&clock, &mut [&mut alice, &mut bob], 400);
    // Alice's first read fails; asked again, after a while, as the
    // screen asks each frame while there is none.
    alice.fetch_invite();
    bob.fetch_invite();
    tick(&clock, &mut [&mut alice, &mut bob], 400);
    assert_eq!(alice.invite_link(), None);
    *down.borrow_mut() = false;
    for _ in 0..40 {
        alice.fetch_invite();
        tick(&clock, &mut [&mut alice, &mut bob], 400);
    }
    let first = alice.invite_link().expect("read again after the failure");
    assert_eq!(bob.invite_link().as_deref(), Some(first.as_str()));
    // Bob replaces it: Alice's More reads it again.
    bob.rotate_invite();
    tick(&clock, &mut [&mut alice, &mut bob], 400);
    let replaced = bob.invite_link().unwrap();
    assert_ne!(replaced, first);
    alice.refresh_invite();
    tick(&clock, &mut [&mut alice, &mut bob], 400);
    assert_eq!(alice.invite_link(), Some(replaced));
}
