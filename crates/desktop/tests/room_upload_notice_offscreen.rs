//! Round TU (a draft for the maintainer's approval): an online room's list
//! says under its status line what its songs are, and on My songs, to
//! whoever may add songs, what adding does and the condition, with the
//! terms of use's button at the tabs' row's end; in English and Japanese,
//! on a computer's screen and a phone's. Over the room model's fake room;
//! drawn offscreen where there is a GPU (`DDI_OFFSCREEN_PPM=<dir>` keeps
//! `notice-<who>-<tab>-<w>x<h>-<lang>.ppm`).

mod common;
use std::rc::Rc;

use common::online::{ROOM, Scene, Songs, keep, library, person};
use ddi_room::fake::FakeServer;
use ddi_room::{Identity, Intent, ManualClock, RoomPolicy};
use ddi_ui::screens::select::{TAB_MINE_NAME, TAB_ROOM_NAME, TERMS_NAME};
use ddi_ui::testing::Harness;
use ddi_ui::{PointerKind, TextMode};

#[test]
fn a_rooms_list_says_what_uploading_does_in_both_languages() {
    common::indexes();
    let mut pictures = common::gpu::Pictures::new();
    for lang in ["en", "ja"] {
        assert!(ddi_l10n::set_language(lang));
        for size in [[1280u32, 720u32], [390, 844]] {
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
            server
                .borrow_mut()
                .join(Identity::new("dev:bob"), "bob", Intent::Play)
                .unwrap();
            let (mine, theirs) = (library(0, 0x1000), library(3, 0x9000));
            let all: Songs = Rc::new(mine.songs.iter().chain(&theirs.songs).cloned().collect());
            let mut scene = Scene {
                clock,
                server: server.clone(),
                people: vec![
                    person(&server, "alice", mine, &all, css),
                    person(&server, "bob", theirs, &all, css),
                ],
            };
            scene.run(2.0);
            let tab = |scene: &Scene, i: usize| {
                let state = scene.people[i].env.state.borrow();
                state.room.as_ref().and_then(|r| r.tab.clone())
            };
            let notice = |scene: &Scene, i: usize| {
                scene.people[i].ui.actor("Room.Notice0").is_some()
                    && scene.people[i].ui.actor(TERMS_NAME).is_some()
            };
            // Each one's Room songs, then their My songs, by a tap on
            // the tab (alice adds the song shown first, so the room's list
            // has one).
            for i in 0..2 {
                let who = ["alice", "bob"][i];
                assert!(notice(&scene, i), "{lang} {size:?} {who}: the notice");
                let design = scene.people[i].ui.design();
                let mut h = Harness::with_gestures(design, css, 1.0, TextMode::Own, false);
                std::mem::swap(&mut h.ui, &mut scene.people[i].ui);
                h.tap(PointerKind::Mouse, TAB_MINE_NAME);
                std::mem::swap(&mut h.ui, &mut scene.people[i].ui);
                scene.run(0.5);
                assert_eq!(tab(&scene, i).as_deref(), Some("mine"), "{lang} {who}");
                assert!(notice(&scene, i), "{lang} {size:?} {who}: on My songs");
                if i == 0 {
                    std::mem::swap(&mut h.ui, &mut scene.people[0].ui);
                    h.key("Enter", "Enter");
                    std::mem::swap(&mut h.ui, &mut scene.people[0].ui);
                    scene.run(1.0);
                }
                if let Some(p) = pictures.draw(&mut scene.people[i].ui, size) {
                    keep(&format!("notice-{who}-mine"), size, lang, &p);
                }
                std::mem::swap(&mut h.ui, &mut scene.people[i].ui);
                h.tap(PointerKind::Mouse, TAB_ROOM_NAME);
                std::mem::swap(&mut h.ui, &mut scene.people[i].ui);
                scene.run(0.5);
                if let Some(p) = pictures.draw(&mut scene.people[i].ui, size) {
                    keep(&format!("notice-{who}-room"), size, lang, &p);
                }
            }
        }
    }
    assert!(ddi_l10n::set_language("en"));
}
