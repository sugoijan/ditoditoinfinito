//! A practice room's song select comes back where it was, as
//! singleplayer's does (`docs/plans/song-navigation.md`, "In a room";
//! feedback round E2): a new room (the last one left) starts at the last
//! room's place and at the song picked there, the first at singleplayer's;
//! the room never moves singleplayer's place; Options and Calibrate open
//! over it and come back to it. The real library host
//! (the bundled songs), no window, no sound, no GPU.

mod common;
use std::rc::Rc;

use ddi_desktop::{shell, songs::Songs};
use ddi_ui::screens::DESIGN;
use ddi_ui::state::place::MenuPlace;
use ddi_ui::{PointerKind, TextMode, testing::Harness};

fn song(env: &ddi_ui::screens::Env) -> String {
    env.state.borrow().song_id.clone()
}

#[test]
fn a_new_room_comes_back_where_the_last_one_was_and_singleplayer_stays() {
    common::indexes();
    let songs = Songs::new(Vec::new());
    songs.skip_offer();
    let (ui, env, _) = shell::theme_ui("start", Rc::new(str::to_string));
    let mut h = Harness::with_gestures(DESIGN, DESIGN, 1.0, TextMode::Own, false);
    h.ui = ui;
    h.run(0.3);
    let solo = MenuPlace::load();
    let room = |h: &mut Harness| {
        ddi_desktop::room::current()
            .borrow_mut()
            .set_rules_seen(true);
        h.ui.push(ddi_desktop::room::room_screen_with(
            &env,
            Some(songs.clone()),
            None,
            None,
        ));
        h.run(0.8);
        assert_eq!(env.state.borrow().screen, "select");
    };
    // The first room: singleplayer's place (no song kept: the first).
    room(&mut h);
    let first = song(&env);
    assert!(!first.is_empty());
    h.key("ArrowDown", "ArrowDown");
    h.run(0.8);
    h.key("ArrowDown", "ArrowDown");
    h.run(0.8);
    let moved = song(&env);
    assert_ne!(moved, first);
    // Left between matches (no question in practice), then a new room.
    h.key("Escape", "Escape");
    h.run(0.8);
    assert_ne!(env.state.borrow().screen, "select");
    room(&mut h);
    assert_eq!(song(&env), moved, "where the last room was");
    // Options and Calibrate, the footer's links, open over the room and
    // come back to it where it was (they did nothing in a room before).
    for (link, screen) in [("Link.Options", "options"), ("Link.Calibrate", "calibration")] {
        h.tap(PointerKind::Mouse, link);
        h.run(0.8);
        assert_eq!(env.state.borrow().screen, screen, "{link}");
        h.key("Escape", "Escape");
        h.run(0.8);
        assert_eq!(env.state.borrow().screen, "select", "back from {screen}");
        assert_eq!(song(&env), moved, "back from {screen}");
    }
    // Picked one further on (no library play here: the press alone), left
    // with the match running (forgotten), and a new room: the song picked.
    h.key("ArrowDown", "ArrowDown");
    h.run(0.8);
    let picked = song(&env);
    assert_ne!(picked, moved);
    h.key("Enter", "Enter");
    h.run(0.5);
    while env.state.borrow().screen == "select" {
        h.key("Escape", "Escape");
        h.run(0.8);
    }
    room(&mut h);
    assert_eq!(song(&env), picked, "the song picked");
    assert_eq!(MenuPlace::load(), solo, "singleplayer's place never moved");
}
