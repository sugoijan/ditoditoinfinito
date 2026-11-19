//! An online room's song select (room model revision 17, round RL1:
//! `docs/plans/multiplayer-room-songs.md`), drawn offscreen on this
//! machine's GPU in the game's theme, over the room model's fake room
//! (`ddi_room::fake`: the server's room in process; no network, no
//! sign-in). Without a GPU adapter (CI) only the pictures are skipped:
//! what each shows is checked all the same.
//!
//! The room has its owner (alice), a co-host (bob) and a member (carol),
//! each with a client and a song select of their own. The pictures: the
//! screen before the room answers (nothing of a view), the owner's empty
//! Room songs, the member's, the owner's My songs, then the room's list
//! once she added three songs from there (uploaded as the server ends
//! them) for the owner, the co-host (with Pick songs) and the member, and
//! the member's My songs (nothing to add). `DDI_OFFSCREEN_PPM=<dir>` keeps
//! them (`<scene>-<w>x<h>-<lang>.ppm`).

mod common;
use std::rc::Rc;

use ddi_room::fake::{FakeServer, Fault};
use ddi_room::{Command, Identity, Intent, ManualClock, ParticipantId, RoomPolicy};
use common::gpu::seen;
use common::online::{ROOM, Scene, Songs, drawn, keep, library, person};

#[test]
fn the_rooms_song_select_has_the_rooms_songs_and_ones_own() {
    let mut pictures = common::gpu::Pictures::new();
    for (size, lang) in [
        ([1280u32, 720u32], "en"),
        ([1280, 720], "ja"),
        ([844, 390], "en"),
        ([844, 390], "ja"),
    ] {
        assert!(ddi_l10n::set_language(lang));
        let css = [size[0] as f32, size[1] as f32];
        let clock = ManualClock::new(1_000);
        let server = FakeServer::hosted(
            ROOM,
            Identity::new("dev:alice"),
            "alice",
            RoomPolicy::default(),
            clock.clone(),
        )
        .shared();
        for who in ["bob", "carol"] {
            server
                .borrow_mut()
                .join(Identity::new(format!("dev:{who}")), who, Intent::Play)
                .unwrap();
        }
        server
            .borrow_mut()
            .authority(Command::SetCoHost {
                participant: ParticipantId(2),
                on: true,
            })
            .unwrap();
        // The room's first answer to its owner is a moment on its way.
        server.borrow_mut().fault(ParticipantId(1), Fault::Delay(900));
        let (mut a, b, c) = (library(0, 0x1000), library(9, 0x9000), library(20, 0x20000));
        // Alice's songs imported (a bundled song is listed with nothing
        // uploaded, every device having it).
        for s in &mut a.songs {
            s.pack.get_or_insert_with(|| "Alice's Pack".into());
        }
        let all: Songs = Rc::new(a.songs.iter().chain(&b.songs).cloned().collect());
        let mut scene = Scene {
            clock,
            server: server.clone(),
            people: vec![
                person(&server, "alice", a, &all, css),
                person(&server, "bob", b, &all, css),
                person(&server, "carol", c, &all, css),
            ],
        };
        let mut shot = |scene: &mut Scene, who: usize, name: &str| -> Option<Vec<u8>> {
            let pixels = pictures.draw(&mut scene.people[who].ui, size)?;
            keep(name, size, lang, &pixels);
            Some(pixels)
        };
        let view = |scene: &Scene, who: usize| {
            scene.people[who]
                .env
                .state
                .borrow()
                .room
                .clone()
                .expect("the room said what it shows")
        };
        let list = |scene: &Scene, who: usize| {
            scene.people[who]
                .ui
                .actor("Wheel")
                .is_some_and(|w| w.visible)
        };
        let button = |scene: &Scene, who: usize| scene.people[who].ui.actor("Room.TakePick").is_some();
        // Before the room answers: nothing of a view, the bars and a line.
        scene.run(0.7);
        assert!(!list(&scene, 0) && scene.people[0].ui.actor("RoomJoining").is_some());
        let joining = shot(&mut scene, 0, "0-joining");
        assert!(seen(&joining, |p| !drawn(p, size, [0.55, 0.3, 0.95, 0.42])), "no list yet");
        // Known: Room songs for everyone, empty, said to each as they may
        // act on it.
        scene.run(5.0);
        let (va, vb, vc) = (view(&scene, 0), view(&scene, 1), view(&scene, 2));
        assert!(va.host && !vb.host && !vc.host, "{lang} {size:?}");
        assert!(vb.can_take_pick && !vc.can_take_pick && !va.can_take_pick);
        assert!(list(&scene, 0) && list(&scene, 1) && list(&scene, 2));
        for v in [&va, &vb, &vc] {
            assert_eq!(v.tab.as_deref(), Some("room"));
            assert!(v.songs.is_empty());
        }
        assert!(va.may_add && vb.may_add && !vc.may_add);
        assert!(button(&scene, 1) && !button(&scene, 0) && !button(&scene, 2));
        shot(&mut scene, 0, "1-owner-room-empty");
        shot(&mut scene, 2, "2-member-room-empty");
        // The owner's My songs: Enter adds; three songs added.
        let key = |scene: &mut Scene, who: usize, code: &str, k: &str| {
            let ui = &mut scene.people[who].ui;
            ui.handle(ddi_ui::UiEvent::Key(ddi_ui::event::KeyEvent::down(code, k)));
            ui.handle(ddi_ui::UiEvent::Key(ddi_ui::event::KeyEvent::up(code, k)));
        };
        key(&mut scene, 0, "KeyM", "m");
        scene.run(0.5);
        assert_eq!(view(&scene, 0).tab.as_deref(), Some("mine"));
        shot(&mut scene, 0, "3-owner-my-songs");
        for _ in 0..3 {
            key(&mut scene, 0, "Enter", "Enter");
            scene.run(0.6);
            key(&mut scene, 0, "ArrowDown", "ArrowDown");
            scene.run(0.6);
        }
        scene.run(2.0);
        key(&mut scene, 0, "KeyM", "m");
        scene.run(1.0);
        let (va, vb, vc) = (view(&scene, 0), view(&scene, 1), view(&scene, 2));
        assert_eq!(va.songs.len(), 3, "{va:?}");
        assert!(va.songs.iter().all(|e| e.state == "ready" && e.by == "alice"), "{:?}", va.songs);
        assert!(vc.songs.iter().all(|e| e.share.is_some()), "{:?}", vc.songs);
        assert_eq!(vb.songs.len(), 3);
        let owner = shot(&mut scene, 0, "4-owner-room-songs");
        assert!(seen(&owner, |p| drawn(p, size, [0.55, 0.3, 0.95, 0.6])), "the list");
        shot(&mut scene, 1, "5-cohost-room-songs");
        shot(&mut scene, 2, "6-member-room-songs");
        // The member's My songs: theirs to browse, nothing to add.
        key(&mut scene, 2, "KeyM", "m");
        scene.run(0.5);
        assert_eq!(view(&scene, 2).tab.as_deref(), Some("mine"));
        shot(&mut scene, 2, "7-member-my-songs");
    }
}
