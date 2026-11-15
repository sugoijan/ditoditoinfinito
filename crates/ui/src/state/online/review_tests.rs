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
    s
}

/// ui-state-6, a follower: the spotlight's songs read from the room's
/// share answered out of order: each song kept as its own, never one
/// under the other's card.
#[test]
fn the_spotlights_reads_answered_out_of_order_keep_each_song_its_own() {
    use super::tests::room::{person, shared_pick, upload};
    let clock = ManualClock::new(1_000);
    let server = room_of(&clock, &["bob"]);
    let mut alice = client(&server, person(&server, "alice"), "alice", 1_000);
    let mut bob = client(&server, person(&server, "bob"), "bob", 1_000);
    tick(&clock, &mut [&mut alice, &mut bob], 400);
    let (a, b) = (song_n(7), song_n(8));
    alice.hover(Some((&a, &a.charts[0])));
    tick(&clock, &mut [&mut alice, &mut bob], 3_600);
    tick(&clock, &mut [&mut alice, &mut bob], 400);
    upload(&server, &mut alice, "sharea01");
    tick(&clock, &mut [&mut alice, &mut bob], 400);
    let job_a = bob.take_load_jobs();
    assert_eq!(job_a.len(), 1);
    alice.hover(Some((&b, &b.charts[0])));
    tick(&clock, &mut [&mut alice, &mut bob], 3_600);
    tick(&clock, &mut [&mut alice, &mut bob], 400);
    upload(&server, &mut alice, "shareb01");
    tick(&clock, &mut [&mut alice, &mut bob], 400);
    let job_b = bob.take_load_jobs();
    assert_eq!(job_b.len(), 1);
    // B's read comes back first, A's after it.
    bob.finish_shared_load(&job_b[0], Ok(shared_pick(&b)));
    bob.finish_shared_load(&job_a[0], Ok(shared_pick(&a)));
    let (card, picked) = bob.spotlight().unwrap();
    assert_eq!(card.title, "Song 8");
    assert_eq!(
        picked.map(|p| p.chart.digest),
        Some(Some(ddi_chart::ChartDigest(8))),
        "B's own song under B's card"
    );
}

/// ui-state-6, the picker: the spotlight moves on while the last song's
/// upload still runs; that upload's end is not the new song's, and the
/// new song is shared once it is done.
#[test]
fn the_spotlights_upload_ending_late_is_not_the_next_songs() {
    use super::tests::room::person;
    use crate::state::room::SPOTLIGHT;
    use ddi_room::Command;
    let clock = ManualClock::new(1_000);
    let server = room_of(&clock, &["bob"]);
    let mut alice = client(&server, person(&server, "alice"), "alice", 1_000);
    let mut bob = client(&server, person(&server, "bob"), "bob", 1_000);
    tick(&clock, &mut [&mut alice, &mut bob], 400);
    let (a, b) = (song_n(7), song_n(8));
    alice.hover(Some((&a, &a.charts[0])));
    tick(&clock, &mut [&mut alice, &mut bob], 3_600);
    tick(&clock, &mut [&mut alice, &mut bob], 400);
    let jobs_a = alice.take_upload_jobs();
    assert_eq!(jobs_a.len(), 1, "A's upload under way");
    // The picker moves on to B before A's upload ends: B's grant asked
    // for, not answered yet, as A's upload ends.
    alice.hover(Some((&b, &b.charts[0])));
    tick(&clock, &mut [&mut alice, &mut bob], 3_600);
    // A's upload ends: the room hears of A's share.
    server
        .borrow_mut()
        .authority(Command::SetSpotlightShare {
            chart: jobs_a[0].content.chart.clone(),
            share: "sharea01".into(),
        })
        .ok();
    alice.upload_finished(SPOTLIGHT, Ok("sharea01".into()));
    for _ in 0..4 {
        tick(&clock, &mut [&mut alice, &mut bob], 400);
    }
    let jobs_b = alice.take_upload_jobs();
    assert_eq!(jobs_b.len(), 1, "B shared too");
    assert_eq!(jobs_b[0].content.chart, b_chart(&b));
}

fn b_chart(s: &crate::screens::data::SongEntry) -> ddi_room::ChartKey {
    crate::state::room::content_id(s, &s.charts[0]).chart
}
