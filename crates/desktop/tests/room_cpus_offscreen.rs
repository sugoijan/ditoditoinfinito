//! CPUs in an online room (`ddi_room::live::cpus`, `online-cpus-v0`,
//! 2026-11-13) on its screens, drawn offscreen on this machine's GPU in
//! the game's theme over the room model's fake room (`room_fake`: no
//! network, no sign-in). Skipped where there is no GPU adapter (CI).
//!
//! The pictures: the owner's Rules pop-up in a room that plays live with
//! the roster's rows (Add CPU, Add random, each CPU's stars and its
//! remove), then the room's song select as the owner and a member see it
//! with the CPUs among the players (stars, never Bot), wide and on a
//! phone, in English and Japanese. `DDI_OFFSCREEN_PPM=<dir>` keeps them.

mod room_fake;

use std::rc::Rc;

use ddi_desktop::gpu::{self, Gpu};
use ddi_room::fake::FakeServer;
use ddi_room::{Identity, Intent, ManualClock, RoomPolicy};
use room_fake::{ROOM, Scene, Songs, drawn, keep, library, person, tap};

/// Opens the Rules pop-up on `ui`: its button in the header, or More's
/// item where the header has no room for it (a phone).
fn open_rules(ui: &mut ddi_ui::Ui) {
    if ui.actor("Room.Rules").is_some() {
        tap(ui, "Room.Rules", 0.6);
    } else {
        tap(ui, "More", 0.6);
        tap(ui, "Item.rules", 0.6);
    }
}

#[test]
fn an_online_rooms_cpus_on_its_rules_and_its_players() {
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
    ] {
        assert!(ddi_l10n::set_language(lang));
        let css = [size[0] as f32, size[1] as f32];
        let clock = ManualClock::new(1_000);
        let server = FakeServer::hosted(
            ROOM,
            Identity::new("dev:alice"),
            "alice",
            RoomPolicy::default().with_live(),
            clock.clone(),
        )
        .shared();
        server
            .borrow_mut()
            .join(Identity::new("dev:bob"), "bob", Intent::Play)
            .unwrap();
        let (a, b) = (library(0, 0x1000), library(9, 0x9000));
        let all: Songs = Rc::new(a.songs.iter().chain(&b.songs).cloned().collect());
        let mut scene = Scene {
            clock,
            server: server.clone(),
            people: vec![
                person(&server, "alice", a, &all, css),
                person(&server, "bob", b, &all, css),
            ],
        };
        scene.run(2.0);
        assert!(scene.people[0].room.borrow().cpus_offered());
        open_rules(&mut scene.people[0].ui);
        assert!(scene.people[0].ui.actor("AddRandom").is_some(), "the roster's rows");
        for _ in 0..3 {
            tap(&mut scene.people[0].ui, "AddRandom", 0.3);
        }
        let pixels = gpu.draw_offscreen(&mut scene.people[0].ui, size, format);
        keep("cpus-rules", size, lang, &pixels);
        assert!(drawn(&pixels, size, [0.3, 0.3, 0.7, 0.7]));
        assert!(scene.people[0].ui.actor("Cpu2.Stars").is_some(), "three CPUs as stars");
        tap(&mut scene.people[0].ui, "Apply", 0.6);
        scene.run(2.0);
        let roster = server.borrow().state().next.rules.cpu.roster.clone();
        assert_eq!(roster.len(), 3);
        for (who, name) in [(0, "cpus-players-host"), (1, "cpus-players-member")] {
            let pixels = gpu.draw_offscreen(&mut scene.people[who].ui, size, format);
            keep(name, size, lang, &pixels);
            // The CPUs among the players, with their levels (stars),
            // never bots.
            let view = scene.people[who].env.state.borrow().room.clone().unwrap();
            let cpus: Vec<_> = view.members.iter().filter(|m| m.cpu).collect();
            assert_eq!(cpus.len(), 3, "{:?}", view.members);
            assert!(cpus.iter().all(|m| m.level.is_some() && !m.bot && m.at_start));
            assert_eq!(cpus[0].name, roster[0].name.clone().unwrap());
        }
    }
    assert!(ddi_l10n::set_language("en"));
}
