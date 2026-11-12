//! One picker at a time on an online room's song select
//! (`docs/plans/multiplayer-prototype-progress.md`, "One picker at a
//! time"), drawn offscreen on this machine's GPU in the game's theme, over
//! the room model's fake room (`ddi_room::fake`: the server's room in
//! process; no network, no sign-in). Skipped where there is no GPU adapter
//! (CI).
//!
//! The room has its owner (alice), a co-host (bob) and a member (carol),
//! each with a client and a song select of their own. The pictures: the
//! screen before the room answers (nothing of a view), the owner's list,
//! the co-host's following view with Pick songs, the member's following
//! view without it, then both sides after the co-host took the pick.
//! `DDI_OFFSCREEN_PPM=<dir>` keeps them (`<scene>-<w>x<h>-<lang>.ppm`).

mod room_fake;

use std::rc::Rc;

use ddi_desktop::gpu::{self, Gpu};
use ddi_room::fake::{FakeServer, Fault};
use ddi_room::{Command, Identity, Intent, ManualClock, ParticipantId, RoomPolicy};
use room_fake::{ROOM, Scene, Songs, drawn, keep, library, person};

#[test]
fn the_rooms_song_select_changes_hands() {
    let instance = gpu::instance();
    let Some(adapter) = gpu::adapter(&instance, None) else {
        eprintln!("no GPU adapter: skipped");
        return;
    };
    let (device, queue) = gpu::device(&adapter).expect("device");
    let format = wgpu::TextureFormat::Rgba8Unorm;
    let mut gpu = Gpu::new(device, queue, format);
    for (size, lang) in [
        ([1280u32, 720u32], "en"),
        ([1280, 720], "ja"),
        ([844, 390], "en"),
        ([844, 390], "ja"),
    ] {
        assert!(ddi_l10n::set_language(lang));
        let css = [size[0] as f32, size[1] as f32];
        let wide = size[0] >= 1000;
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
        let (a, b, c) = (library(0, 0x1000), library(9, 0x9000), library(20, 0x20000));
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
        let mut shot = |scene: &mut Scene, who: usize, name: &str| -> Vec<u8> {
            let pixels = gpu.draw_offscreen(&mut scene.people[who].ui, size, format);
            keep(name, size, lang, &pixels);
            pixels
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
        assert!(!drawn(&joining, size, [0.55, 0.3, 0.95, 0.42]), "no list yet");
        // The owner picks; their song rests and the others see it.
        scene.run(5.0);
        let (va, vb, vc) = (view(&scene, 0), view(&scene, 1), view(&scene, 2));
        assert!(va.host && !vb.host && !vc.host, "{lang} {size:?}");
        assert!(vb.can_take_pick && !vc.can_take_pick && !va.can_take_pick);
        assert!(list(&scene, 0) && !list(&scene, 1) && !list(&scene, 2));
        assert!(vb.spotlight.is_some() && vb.spotlight_song.is_some(), "{vb:?}");
        assert_eq!(button(&scene, 1), wide, "a phone's is in More");
        assert!(!button(&scene, 0) && !button(&scene, 2));
        let owner = shot(&mut scene, 0, "1-owner-list");
        assert!(drawn(&owner, size, [0.55, 0.3, 0.95, 0.6]), "the list");
        shot(&mut scene, 1, "2-cohost-following");
        shot(&mut scene, 2, "3-member-following");
        // The co-host takes the pick: their own list, the owner follows
        // their resting song, with the action.
        scene.people[1].room.borrow_mut().take_pick().unwrap();
        scene.run(5.0);
        let (va, vb, vc) = (view(&scene, 0), view(&scene, 1), view(&scene, 2));
        assert!(!va.host && vb.host && !vc.host, "{lang} {size:?}");
        assert!(va.can_take_pick && !vb.can_take_pick && !vc.can_take_pick);
        assert_eq!((va.picker, vc.picker), (Some(2), Some(2)));
        assert!(list(&scene, 1) && !list(&scene, 0));
        assert!(va.spotlight.is_some() && va.spotlight_song.is_some(), "{va:?}");
        assert_eq!(button(&scene, 0), wide);
        shot(&mut scene, 1, "4-cohost-list");
        shot(&mut scene, 0, "5-owner-following");
        shot(&mut scene, 2, "6-member-following");
    }
}
