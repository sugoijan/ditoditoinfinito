//! An online room's song select uncovered (round SF; the maintainer,
//! 2026-11-20: "I keep seeing a 'Joining the room...' message drawn on
//! top of the song list even though I *am* already connected"). Desktop
//! keeps the room's song select under the screens opened over it (here
//! desktop's own Options; a stage's prompt, play and results the same
//! way, through the play host), and a screen uncovered plays its
//! entrance over its whole tree again. The joining line, whether it
//! showed (a slow first answer) or not, is never drawn again once the
//! room is known. Drawn offscreen on this machine's GPU over the room
//! model's fake room; without a GPU adapter (CI) only the pictures are
//! skipped. `DDI_OFFSCREEN_PPM=<dir>` keeps them
//! (`uncovered-<lag>-<step>-<w>x<h>-en.ppm`).

mod common;
use std::rc::Rc;

use common::online::{ROOM, Scene, Songs, keep, library, person};
use ddi_room::fake::{FakeServer, Fault};
use ddi_room::{Identity, ManualClock, ParticipantId, RoomPolicy};
use ddi_ui::UiEvent;
use ddi_ui::event::KeyEvent;

#[test]
fn a_known_rooms_song_select_never_says_joining_again() {
    common::indexes();
    assert!(ddi_l10n::set_language("en"));
    let mut pictures = common::gpu::Pictures::new();
    let size = [1280u32, 720u32];
    let css = [size[0] as f32, size[1] as f32];
    for lag in [0u64, 1_600] {
        let clock = ManualClock::new(1_000);
        let server = FakeServer::hosted(
            ROOM,
            Identity::new("dev:alice"),
            "alice",
            RoomPolicy::default(),
            clock.clone(),
        )
        .shared();
        if lag > 0 {
            server.borrow_mut().fault(ParticipantId(1), Fault::Delay(lag));
        }
        let mine = library(0, 0x1000);
        let all: Songs = Rc::new(mine.songs.clone());
        let mut scene = Scene {
            clock,
            server: server.clone(),
            people: vec![person(&server, "alice", mine, &all, css)],
        };
        let line = |scene: &Scene| {
            scene.people[0]
                .ui
                .actor("RoomJoining")
                .filter(|a| a.visible)
                .map_or(0.0, |a| a.current().diffuse()[3])
        };
        let screen = |scene: &Scene| scene.people[0].env.state.borrow().screen.clone();
        scene.run(4.0);
        assert!(
            scene.people[0].ui.actor("Wheel").is_some_and(|w| w.visible),
            "{lag}: the room's view"
        );
        assert_eq!(line(&scene), 0.0, "{lag}");
        if let Some(p) = pictures.draw(&mut scene.people[0].ui, size) {
            keep(&format!("uncovered-{lag}-1-known"), size, "en", &p);
        }
        // Desktop's Options over the room, and back by Escape.
        let env = scene.people[0].env.clone();
        let (options, _) = ddi_desktop::options::screen(&env, None, None);
        scene.people[0].ui.push(options);
        scene.run(0.8);
        assert_eq!(screen(&scene), "options", "{lag}");
        let ui = &mut scene.people[0].ui;
        ui.handle(UiEvent::Key(KeyEvent::down("Escape", "Escape")));
        ui.handle(UiEvent::Key(KeyEvent::up("Escape", "Escape")));
        for _ in 0..12 {
            scene.run(0.25);
            assert_eq!(line(&scene), 0.0, "{lag}: joining said again");
        }
        assert_eq!(screen(&scene), "select", "{lag}");
        assert!(scene.people[0].ui.actor("RoomJoining").is_none(), "{lag}");
        if let Some(p) = pictures.draw(&mut scene.people[0].ui, size) {
            keep(&format!("uncovered-{lag}-2-back"), size, "en", &p);
        }
    }
}
