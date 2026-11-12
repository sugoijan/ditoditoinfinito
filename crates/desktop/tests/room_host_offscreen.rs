//! A room's host on its screens
//! (`docs/plans/multiplayer-prototype-progress.md`, "A room's host"),
//! drawn offscreen on this machine's GPU in the game's theme, over the
//! room model's fake room (`room_fake`: no network, no sign-in). Skipped
//! where there is no GPU adapter (CI).
//!
//! The room has its owner (alice), a co-host (bob) and a member (carol).
//! The pictures: the owner's Players pop-up (the others' rows to tap, the
//! line saying what the room does when its host leaves, Host options),
//! Host options (the setting, each person's buttons), the question
//! before the room is handed over, a member's Players pop-up (no
//! controls), and the room after the handover as its new owner sees it.
//! `DDI_OFFSCREEN_PPM=<dir>` keeps them.

mod room_fake;

use std::rc::Rc;

use ddi_desktop::gpu::{self, Gpu};
use ddi_room::fake::FakeServer;
use ddi_room::{Command, Identity, Intent, ManualClock, ParticipantId, RoomPolicy};
use room_fake::{ROOM, Scene, Songs, drawn, keep, library, person, tap};

/// Opens the Players pop-up on `ui`: its button in the header, or More's
/// item where the header has no room for it (a phone).
fn open_players(ui: &mut ddi_ui::Ui) {
    if ui.actor("Room.Players").is_some() {
        tap(ui, "Room.Players", 0.6);
    } else {
        tap(ui, "More", 0.6);
        tap(ui, "Item.players", 0.6);
    }
}

#[test]
fn the_hosts_pop_ups_and_a_room_handed_over() {
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
        let top = |scene: &Scene, who: usize| {
            scene.people[who]
                .ui
                .layers()
                .last()
                .map(|l| l.name.clone())
                .unwrap_or_default()
        };
        scene.run(2.0);
        let (va, vb, vc) = (view(&scene, 0), view(&scene, 1), view(&scene, 2));
        assert!(va.manages_hosts && !vb.manages_hosts && !vc.manages_hosts);
        assert_eq!(va.host_leaves.as_deref(), Some("pass_on"), "{lang} {size:?}");
        // The owner's Players pop-up, then Host options.
        open_players(&mut scene.people[0].ui);
        assert_eq!(top(&scene, 0), "room-players", "{lang} {size:?}");
        assert!(scene.people[0].ui.actor("HostOptions").is_some());
        assert!(scene.people[0].ui.actor("Person.2").is_some());
        let players = shot(&mut scene, 0, "host-0-players");
        assert!(drawn(&players, size, [0.3, 0.3, 0.7, 0.7]));
        tap(&mut scene.people[0].ui, "HostOptions", 0.6);
        assert_eq!(top(&scene, 0), "room-host", "{lang} {size:?}");
        for name in ["Dec.leaves", "Inc.leaves", "CoHost.2", "MakeHost.2", "CoHost.3", "MakeHost.3"] {
            let b = scene.people[0].ui.bounds(name).unwrap_or_else(|| panic!("{name}"));
            assert!(
                b[0] >= 0.0 && b[1] >= 0.0 && b[2] > b[0] && b[3] > b[1],
                "{name} {b:?} {lang} {size:?}"
            );
        }
        shot(&mut scene, 0, "host-1-options");
        // The other value, said.
        tap(&mut scene.people[0].ui, "Inc.leaves", 0.3);
        scene.run(0.5);
        assert_eq!(view(&scene, 2).host_leaves.as_deref(), Some("close"));
        shot(&mut scene, 0, "host-2-options-close");
        tap(&mut scene.people[0].ui, "Dec.leaves", 0.3);
        scene.run(0.5);
        // A member's Players pop-up: the line, no controls.
        open_players(&mut scene.people[2].ui);
        assert!(scene.people[2].ui.actor("HostOptions").is_none());
        assert!(scene.people[2].ui.actor("Person.1").is_none());
        shot(&mut scene, 2, "host-3-member-players");
        tap(&mut scene.people[2].ui, "Close", 0.5);
        // Make host, asked once more.
        tap(&mut scene.people[0].ui, "MakeHost.3", 0.6);
        assert_eq!(top(&scene, 0), "confirm", "{lang} {size:?}");
        shot(&mut scene, 0, "host-4-make-host");
        tap(&mut scene.people[0].ui, "Confirm", 0.5);
        scene.run(1.0);
        let (va, vc) = (view(&scene, 0), view(&scene, 2));
        assert_eq!(va.online.as_ref().unwrap().role, "co_host");
        assert_eq!(vc.online.as_ref().unwrap().role, "owner");
        assert!(vc.manages_hosts && !va.manages_hosts && !va.host_open);
        // The one before sees the Players pop-up again, without the
        // controls; the new owner has them.
        assert_eq!(top(&scene, 0), "room-players");
        assert!(scene.people[0].ui.actor("HostOptions").is_none());
        shot(&mut scene, 0, "host-5-former-owner");
        open_players(&mut scene.people[2].ui);
        tap(&mut scene.people[2].ui, "HostOptions", 0.6);
        assert_eq!(top(&scene, 2), "room-host");
        shot(&mut scene, 2, "host-6-new-owner-options");
    }
}
