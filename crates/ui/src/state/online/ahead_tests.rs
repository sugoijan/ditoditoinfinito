//! Stage 2 of the room's song list (`docs/plans/multiplayer-room-songs.md`):
//! the listed songs downloaded ahead (`room-preload-v1`) and who has what
//! (`room-have-v0`), from the client's side over the fake room.

use super::live_tests::room_with;
use super::tests::room::{client, person, song, tick};
use crate::screens::data::SongEntry;
use crate::state::room::{EntryState, Listing, PreloadJob, PreloadProblem, RoomClient};
use crate::state::settings::RoomAhead;
use ddi_room::fake::SharedFake;
use ddi_room::{Command, ContentStatus, LinkState, ManualClock, RoomPolicy, RoomSongId};

/// A song of a pack (uploaded when added) with chart digest `digest`.
fn packed(id: &str, title: &str, digest: u128) -> SongEntry {
    let mut s = song();
    s.id = id.into();
    s.title = title.into();
    s.pack = Some("Pack".into());
    s.charts[0].digest = Some(ddi_chart::ChartDigest(digest));
    s
}

/// The server's word on each listed song's upload: there, share `sh<id>`.
fn uploaded(server: &SharedFake, room: &RoomClient) {
    for e in room.room_songs() {
        if e.status == ContentStatus::Pending {
            server
                .borrow_mut()
                .authority(Command::SetSongContent {
                    song: e.id,
                    status: ContentStatus::Available,
                    share: Some(format!("sh{}", e.id.0)),
                    charts: None,
                })
                .unwrap();
        }
    }
}

/// A listing as the shell reads it: the share's song, its banner.
fn listing(share: &str, bytes: u64) -> Listing {
    let mut entry: ddi_library::manifest::ManifestEntry = serde_json::from_value(serde_json::json!({
        "id": format!("shared:{share}:0"),
        "title": format!("Song {share}"),
        "artist": "",
        "chart": "s.ssc",
        "music": "s.ogg",
        "length": 95.0,
        "charts": [{
            "index": 0,
            "layout": "dance-single",
            "difficulty": "Hard",
            "meter": 9,
            "notes": 321,
        }],
    }))
    .unwrap();
    entry.charts[0].digest = Some(ddi_chart::ChartDigest(7));
    Listing {
        entry,
        pack: "Pack".into(),
        banner: Some(format!("files:room/x/{share}/bn.png")),
        bytes,
    }
}

fn songs(jobs: &[PreloadJob]) -> Vec<RoomSongId> {
    jobs.iter()
        .filter_map(|j| match j {
            PreloadJob::Song { song, .. } => Some(*song),
            _ => None,
        })
        .collect()
}

fn listings(jobs: &[PreloadJob]) -> Vec<RoomSongId> {
    jobs.iter()
        .filter_map(|j| match j {
            PreloadJob::Listing { song, .. } => Some(*song),
            _ => None,
        })
        .collect()
}

/// A member downloads every listed song it does not have, banners first
/// and the newest first, two at once, into the room's store; each one
/// here is its own row (its record's entry) and is said to the room, whose
/// count every member reads ("2 of 3 have it"). A song removed is deleted
/// from the store; a stage's countdown stops what is under way, taken up
/// again after; Off downloads nothing.
#[test]
fn listed_songs_are_downloaded_ahead_and_who_has_them_is_counted() {
    let clock = ManualClock::new(1_000);
    let server = room_with(&clock, &["bob", "carol"], RoomPolicy::default().with_live());
    let mut alice = client(&server, person(&server, "alice"), "alice", 1_000);
    let mut bob = client(&server, person(&server, "bob"), "bob", 1_000);
    let mut carol = client(&server, person(&server, "carol"), "carol", 1_000);
    bob.set_ahead(Some(RoomAhead::Everything));
    carol.set_ahead(Some(RoomAhead::Off));
    for _ in 0..20 {
        tick(&clock, &mut [&mut alice, &mut bob, &mut carol], 100);
    }
    assert_eq!(bob.link_state(), LinkState::Connected);
    assert!(bob.plays_live());
    let (a, b, c) = (packed("a", "A", 7), packed("b", "B", 8), packed("c", "C", 9));
    alice.set_library(&[a.clone(), b.clone(), c.clone()]);
    for s in [&a, &b, &c] {
        alice.add_song(s).unwrap();
        tick(&clock, &mut [&mut alice, &mut bob, &mut carol], 200);
    }
    uploaded(&server, &alice);
    tick(&clock, &mut [&mut alice, &mut bob, &mut carol], 400);
    let ids: Vec<RoomSongId> = bob.room_songs().iter().map(|e| e.id).collect();
    assert_eq!(ids.len(), 3);
    assert_eq!(bob.store(), "Room000001");
    // Banners first, the newest first, four at once: no song's bytes yet.
    let jobs = bob.take_preload_jobs();
    assert_eq!(listings(&jobs), vec![ids[2], ids[1], ids[0]]);
    assert!(songs(&jobs).is_empty());
    assert!(carol.take_preload_jobs().is_empty(), "Off: nothing");
    let _ = alice.take_preload_jobs();
    assert!(
        alice.preload_debug().is_empty(),
        "the host has every song in her library"
    );
    for id in &ids {
        let share = format!("sh{}", id.0);
        bob.preload_listed(*id, Ok(listing(&share, 4_000_000)));
    }
    // The banner shows before the song is here; nothing else of it.
    let e0 = bob.room_songs()[0].clone();
    assert!(bob.ahead_banner(&e0).is_some());
    assert!(bob.ahead_song(&e0).is_none());
    assert_eq!(bob.entry_state(&e0), EntryState::NotLoaded);
    tick(&clock, &mut [&mut alice, &mut bob, &mut carol], 100);
    // Then the songs, two at once, the newest first.
    let jobs = bob.take_preload_jobs();
    assert_eq!(songs(&jobs), vec![ids[2], ids[1]]);
    let PreloadJob::Song { store, share, .. } = &jobs[0] else {
        panic!("a song")
    };
    assert_eq!((store.as_str(), share.as_str()), ("Room000001", "sh3"));
    bob.preload_progress(ids[2], 1_000_000.0, 4_000_000.0);
    let e2 = bob.room_songs()[2].clone();
    assert_eq!(
        bob.entry_state(&e2),
        EntryState::Downloading {
            done: 1_000_000.0,
            total: 4_000_000.0
        }
    );
    bob.preload_done(ids[2], Ok(()));
    assert_eq!(bob.entry_state(&e2), EntryState::Ready);
    let row = bob.ahead_song(&e2).expect("its row");
    assert_eq!(row.id, "shared:sh3:0");
    assert_eq!((row.length, row.charts[0].notes), (95.0, 321));
    // A failed download is tried again after a wait.
    bob.preload_done(ids[1], Err(PreloadProblem::Download));
    tick(&clock, &mut [&mut alice, &mut bob, &mut carol], 100);
    assert_eq!(songs(&bob.take_preload_jobs()), vec![ids[0]]);
    tick(&clock, &mut [&mut alice, &mut bob, &mut carol], 2_400);
    assert_eq!(songs(&bob.take_preload_jobs()), vec![ids[1]]);
    // Who has what: said over the live side within the link's few
    // seconds, read by everyone (Carol has none, Alice all).
    for _ in 0..30 {
        tick(&clock, &mut [&mut alice, &mut bob, &mut carol], 400);
    }
    for r in [&alice, &bob, &carol] {
        assert_eq!(r.have_count(&r.room_songs()[2]), Some((2, 3)), "{}", r.me());
        assert_eq!(r.have_count(&r.room_songs()[1]), Some((1, 3)));
        assert_eq!(r.missing_count(&r.room_songs()[1]), 2);
    }
    // A song removed: deleted from the store, its download stopped.
    alice.remove_song(ids[2]).unwrap();
    tick(&clock, &mut [&mut alice, &mut bob, &mut carol], 400);
    let jobs = bob.take_preload_jobs();
    assert!(
        jobs.iter().any(|j| matches!(j, PreloadJob::Forget { share, .. } if share == "sh3")),
        "{jobs:?}"
    );
    assert!(bob.preload_debug().iter().all(|(id, _, _)| *id != ids[2]));
}

/// A stage's countdown stops the downloads under way (no download
/// competes with a play); they are taken up again once it is over. Out of
/// storage, the plan falls back to Likely next and says so once.
#[test]
fn a_stage_stops_the_downloads_and_running_out_of_room_falls_back() {
    let clock = ManualClock::new(1_000);
    let server = room_with(&clock, &["bob"], RoomPolicy::default().with_live());
    let mut alice = client(&server, person(&server, "alice"), "alice", 1_000);
    let mut bob = client(&server, person(&server, "bob"), "bob", 1_000);
    bob.set_ahead(Some(RoomAhead::Everything));
    for _ in 0..20 {
        tick(&clock, &mut [&mut alice, &mut bob], 100);
    }
    let all: Vec<SongEntry> = (0..12)
        .map(|i| packed(&format!("s{i}"), &format!("S{i}"), 20 + i))
        .collect();
    alice.set_library(&all);
    for s in &all {
        alice.add_song(s).unwrap();
        tick(&clock, &mut [&mut alice, &mut bob], 100);
    }
    uploaded(&server, &alice);
    tick(&clock, &mut [&mut alice, &mut bob], 400);
    let ids: Vec<RoomSongId> = bob.room_songs().iter().map(|e| e.id).collect();
    for _ in 0..3 {
        for j in bob.take_preload_jobs() {
            if let PreloadJob::Listing { song, share, .. } = j {
                bob.preload_listed(song, Ok(listing(&share, 6_000_000)));
            }
        }
        tick(&clock, &mut [&mut alice, &mut bob], 100);
    }
    let jobs = bob.take_preload_jobs();
    let mut running: Vec<(RoomSongId, crate::state::room::Stop)> = jobs
        .into_iter()
        .filter_map(|j| match j {
            PreloadJob::Song { song, stop, .. } => Some((song, stop)),
            _ => None,
        })
        .collect();
    assert_eq!(running.len(), 2);
    // The host picks the oldest: while the stage gathers readiness only
    // its song downloads; the others are stopped.
    let first = alice.room_songs()[0].clone();
    alice
        .pick_listed(first.id, &first.charts[0].chart)
        .unwrap();
    tick(&clock, &mut [&mut alice, &mut bob], 200);
    assert!(running.iter().all(|(_, s)| s.stopped()));
    for (song, _) in running.drain(..) {
        bob.preload_done(song, Err(PreloadProblem::Stopped));
    }
    // (Bob's stage load reads the picked song: nothing ahead for it.)
    for job in bob.take_load_jobs() {
        assert_eq!(job.store.as_deref(), Some("Room000001"));
        bob.finish_shared_load(&job, Ok(super::tests::room::shared_pick(&all[0])));
    }
    for job in alice.take_load_jobs() {
        alice.finish_load(&job, Ok(()));
    }
    tick(&clock, &mut [&mut alice, &mut bob], 200);
    assert!(songs(&bob.take_preload_jobs()).is_empty());
    alice.set_ready(true).unwrap();
    bob.set_ready(true).unwrap();
    for _ in 0..10 {
        tick(&clock, &mut [&mut alice, &mut bob], 400);
    }
    assert!(matches!(
        bob.state().phase,
        ddi_room::Phase::Countdown { .. } | ddi_room::Phase::Stage { .. }
    ));
    assert!(
        songs(&bob.take_preload_jobs()).is_empty(),
        "nothing from the countdown on"
    );
    // Over (stopped by its host): taken up again.
    alice.stop_match().unwrap();
    tick(&clock, &mut [&mut alice, &mut bob], 400);
    let jobs = bob.take_preload_jobs();
    let next = songs(&jobs);
    assert_eq!(next.len(), 2, "{jobs:?}");
    assert_eq!(next[0], ids[11]);
    // Out of room: Likely next from then, said once on the status line.
    bob.preload_done(next[0], Err(PreloadProblem::Quota));
    assert!(bob.ahead_quota());
    assert_eq!(bob.ahead_rule(), RoomAhead::LikelyNext);
    assert_eq!(
        bob.link_notice(),
        Some(crate::state::online::link::LinkNotice::AheadQuota)
    );
    bob.preload_done(next[1], Err(PreloadProblem::Stopped));
    tick(&clock, &mut [&mut alice, &mut bob], 400);
    // Likely next: the newest few up to the budget (eight of 6 MB, of
    // eleven not here).
    let mut planned = Vec::new();
    for _ in 0..8 {
        for j in bob.take_preload_jobs() {
            if let PreloadJob::Song { song, .. } = j {
                planned.push(song);
                bob.preload_done(song, Ok(()));
            }
        }
        tick(&clock, &mut [&mut alice, &mut bob], 400);
    }
    planned.sort();
    assert_eq!(planned, ids[4..].to_vec());
}

/// Likely next (round DB, `room-preload-v1`): the newest songs up to eight
/// and 128 MB in all: three of 40 MB, the fourth past the budget.
#[test]
fn likely_next_keeps_to_its_budget_of_bytes() {
    let clock = ManualClock::new(1_000);
    let server = room_with(&clock, &["bob"], RoomPolicy::default().with_live());
    let mut alice = client(&server, person(&server, "alice"), "alice", 1_000);
    let mut bob = client(&server, person(&server, "bob"), "bob", 1_000);
    bob.set_ahead(Some(RoomAhead::LikelyNext));
    for _ in 0..20 {
        tick(&clock, &mut [&mut alice, &mut bob], 100);
    }
    let all: Vec<SongEntry> = (0..6)
        .map(|i| packed(&format!("b{i}"), &format!("B{i}"), 60 + i))
        .collect();
    alice.set_library(&all);
    for s in &all {
        alice.add_song(s).unwrap();
        tick(&clock, &mut [&mut alice, &mut bob], 100);
    }
    uploaded(&server, &alice);
    tick(&clock, &mut [&mut alice, &mut bob], 400);
    let ids: Vec<RoomSongId> = bob.room_songs().iter().map(|e| e.id).collect();
    let mut planned = Vec::new();
    for _ in 0..8 {
        for j in bob.take_preload_jobs() {
            match j {
                PreloadJob::Listing { song, share, .. } => {
                    bob.preload_listed(song, Ok(listing(&share, 40 * 1024 * 1024)));
                }
                PreloadJob::Song { song, .. } => {
                    planned.push(song);
                    bob.preload_done(song, Ok(()));
                }
                _ => {}
            }
        }
        tick(&clock, &mut [&mut alice, &mut bob], 400);
    }
    planned.sort();
    assert_eq!(planned, ids[3..].to_vec());
}

/// More's Add this pack: every song of the pack not in the room yet, as
/// many as the person may add (a host past half the list's size, round
/// DB) and the list has room for, uploaded one at a time; its progress
/// said until the last is there.
#[test]
fn a_pack_is_added_up_to_the_list_s_room_and_its_uploads_are_counted() {
    let clock = ManualClock::new(1_000);
    let server = room_with(&clock, &["bob"], RoomPolicy::default().with_live());
    let mut alice = client(&server, person(&server, "alice"), "alice", 1_000);
    let mut bob = client(&server, person(&server, "bob"), "bob", 1_000);
    for _ in 0..10 {
        tick(&clock, &mut [&mut alice, &mut bob], 100);
    }
    alice
        .set_song_list(ddi_room::ListRules {
            size: 4,
            when_full: ddi_room::WhenFull::Refuse,
        })
        .unwrap();
    tick(&clock, &mut [&mut alice, &mut bob], 200);
    let pack: Vec<SongEntry> = (0..5)
        .map(|i| packed(&format!("p{i}"), &format!("P{i}"), 40 + i))
        .collect();
    alice.set_library(&pack);
    alice.add_song(&pack[0]).unwrap();
    tick(&clock, &mut [&mut alice, &mut bob], 200);
    // A list of four refusing new songs, one added: the host may add
    // three more (past half of it, two).
    assert_eq!(alice.pack_addable(&pack).len(), 3);
    assert!(bob.pack_addable(&pack).is_empty(), "a member may not add");
    assert_eq!(alice.add_pack(&pack), Ok(3));
    tick(&clock, &mut [&mut alice, &mut bob], 200);
    assert_eq!(bob.room_songs().len(), 4);
    assert_eq!(alice.pack_progress(), Some((0, 3)));
    uploaded(&server, &alice);
    tick(&clock, &mut [&mut alice, &mut bob], 200);
    assert_eq!(alice.pack_progress(), None);
    assert!(alice.pack_addable(&pack).is_empty());
}

/// Review 2026-11-22, room-client 5: the song being downloaded ahead is
/// picked: the stage's own load reads it (into the same store), so the
/// download ahead of it stops rather than fetch the same files a second
/// time at once, on the link the stage's loading wait counts on; nothing
/// ahead is asked for it again while the stage's load reads it.
#[test]
fn a_song_picked_while_downloaded_ahead_is_fetched_once() {
    let clock = ManualClock::new(1_000);
    let server = room_with(&clock, &["bob"], RoomPolicy::default().with_live());
    let mut alice = client(&server, person(&server, "alice"), "alice", 1_000);
    let mut bob = client(&server, person(&server, "bob"), "bob", 1_000);
    bob.set_ahead(Some(RoomAhead::Everything));
    for _ in 0..20 {
        tick(&clock, &mut [&mut alice, &mut bob], 100);
    }
    let all: Vec<SongEntry> = (0..3)
        .map(|i| packed(&format!("t{i}"), &format!("T{i}"), 80 + i))
        .collect();
    alice.set_library(&all);
    for s in &all {
        alice.add_song(s).unwrap();
        tick(&clock, &mut [&mut alice, &mut bob], 100);
    }
    uploaded(&server, &alice);
    tick(&clock, &mut [&mut alice, &mut bob], 400);
    let mut running: Vec<(RoomSongId, crate::state::room::Stop)> = Vec::new();
    for _ in 0..3 {
        for j in bob.take_preload_jobs() {
            match j {
                PreloadJob::Listing { song, share, .. } => {
                    bob.preload_listed(song, Ok(listing(&share, 6_000_000)));
                }
                PreloadJob::Song { song, stop, .. } => running.push((song, stop)),
                _ => {}
            }
        }
        tick(&clock, &mut [&mut alice, &mut bob], 100);
    }
    assert_eq!(running.len(), 2);
    // The host picks the newest, downloading here.
    let (newest, stop) = running[0].clone();
    let entry = alice.room_songs().iter().find(|e| e.id == newest).unwrap().clone();
    alice.pick_listed(entry.id, &entry.charts[0].chart).unwrap();
    for _ in 0..3 {
        tick(&clock, &mut [&mut alice, &mut bob], 100);
    }
    let loads = bob.take_load_jobs();
    assert_eq!(loads.len(), 1, "the stage's own load");
    assert_eq!(loads[0].store.as_deref(), Some("Room000001"));
    assert!(stop.stopped(), "the download ahead of the same song stopped");
    bob.preload_done(newest, Err(PreloadProblem::Stopped));
    for _ in 0..3 {
        tick(&clock, &mut [&mut alice, &mut bob], 100);
    }
    assert!(
        !songs(&bob.take_preload_jobs()).contains(&newest),
        "not asked again while the stage's load reads it"
    );
}

/// Review 2026-11-22, room-client 6: the room left (or its client let go)
/// while songs download ahead: each download stops, so that none writes
/// into the room's store after the shell deleted it, nor adds the left
/// room's songs to the library.
#[test]
fn leaving_the_room_stops_its_downloads_ahead() {
    for drop_only in [false, true] {
        let clock = ManualClock::new(1_000);
        let server = room_with(&clock, &["bob"], RoomPolicy::default().with_live());
        let mut alice = client(&server, person(&server, "alice"), "alice", 1_000);
        let mut bob = client(&server, person(&server, "bob"), "bob", 1_000);
        bob.set_ahead(Some(RoomAhead::Everything));
        for _ in 0..20 {
            tick(&clock, &mut [&mut alice, &mut bob], 100);
        }
        let all: Vec<SongEntry> = (0..3)
            .map(|i| packed(&format!("u{i}"), &format!("U{i}"), 90 + i))
            .collect();
        alice.set_library(&all);
        for s in &all {
            alice.add_song(s).unwrap();
            tick(&clock, &mut [&mut alice, &mut bob], 100);
        }
        uploaded(&server, &alice);
        tick(&clock, &mut [&mut alice, &mut bob], 400);
        let mut running = Vec::new();
        for _ in 0..3 {
            for j in bob.take_preload_jobs() {
                match j {
                    PreloadJob::Listing { song, share, .. } => {
                        bob.preload_listed(song, Ok(listing(&share, 6_000_000)));
                    }
                    PreloadJob::Song { stop, .. } => running.push(stop),
                    _ => {}
                }
            }
            tick(&clock, &mut [&mut alice, &mut bob], 100);
        }
        assert_eq!(running.len(), 2);
        assert!(running.iter().all(|s| !s.stopped()));
        if drop_only {
            drop(bob);
        } else {
            bob.leave();
        }
        assert!(running.iter().all(|s| s.stopped()), "dropped only: {drop_only}");
    }
}
