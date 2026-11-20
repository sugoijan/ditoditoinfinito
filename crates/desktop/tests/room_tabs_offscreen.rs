//! An online room's tabs pressed (round SF; the maintainer, 2026-11-20:
//! "I think we should support clicking on the tab to switch between
//! them, and not just the 'M' shortcut"), in the game's own look, whose
//! tab leaves its plate unpainted: a click or a tap on a tab's words, or
//! on the plate around them, shows that tab. Over the room model's fake
//! room; drawn offscreen where there is a GPU (`DDI_OFFSCREEN_PPM=<dir>`
//! keeps `tabs-<step>-<w>x<h>-en.ppm`).

mod common;
use std::rc::Rc;

use common::online::{ROOM, Scene, Songs, keep, library, person};
use ddi_room::fake::FakeServer;
use ddi_room::{Identity, ManualClock, RoomPolicy};
use ddi_ui::screens::select::{TAB_MINE_NAME, TAB_ROOM_NAME};
use ddi_ui::testing::Harness;
use ddi_ui::{PointerKind, TextMode};

#[test]
fn a_click_or_a_tap_on_a_rooms_tab_shows_it() {
    common::indexes();
    assert!(ddi_l10n::set_language("en"));
    let mut pictures = common::gpu::Pictures::new();
    let size = [1280u32, 720u32];
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
    let mine = library(0, 0x1000);
    let all: Songs = Rc::new(mine.songs.clone());
    let mut scene = Scene {
        clock,
        server: server.clone(),
        people: vec![person(&server, "alice", mine, &all, css)],
    };
    scene.run(2.0);
    let tab = |scene: &Scene| {
        let state = scene.people[0].env.state.borrow();
        state.room.as_ref().and_then(|r| r.tab.clone())
    };
    assert_eq!(tab(&scene).as_deref(), Some("room"));
    // Taps through a harness over alice's UI (its pointer events).
    let design = scene.people[0].ui.design();
    let mut h = Harness::with_gestures(design, css, 1.0, TextMode::Own, false);
    let mut step = 0;
    for kind in [PointerKind::Mouse, PointerKind::Touch] {
        for words in [true, false] {
            let (name, to) = match tab(&scene).as_deref() {
                Some("room") => (TAB_MINE_NAME, "mine"),
                _ => (TAB_ROOM_NAME, "room"),
            };
            std::mem::swap(&mut h.ui, &mut scene.people[0].ui);
            let b = h.ui.bounds(name).expect("the tab");
            let at = if words {
                h.centre(name)
            } else {
                // The plate's corner, away from the words.
                [b[0] + 3.0, b[3] - 3.0]
            };
            h.tap_at(kind, at);
            std::mem::swap(&mut h.ui, &mut scene.people[0].ui);
            scene.run(0.5);
            assert_eq!(tab(&scene).as_deref(), Some(to), "{kind:?} {name} words {words}");
            step += 1;
            if let Some(p) = pictures.draw(&mut scene.people[0].ui, size) {
                keep(&format!("tabs-{step}-{to}"), size, "en", &p);
            }
        }
    }
}
