//! Native title/menu flow with the real library host and a silent audio clock.
mod common;
use ddi_desktop::{
    audio::Audio,
    preview::{Player, PreviewSong},
    shell,
    songs::Songs,
};
use ddi_ui::screens::{DESIGN, select::FEED};
use ddi_ui::{PointerKind, TextMode, testing::Harness};
use std::cell::RefCell;
use std::rc::Rc;

#[test]
fn the_title_is_quiet_and_options_returns_to_each_caller() {
    common::indexes();
    let songs = Songs::new(Vec::new());
    songs.skip_offer();
    let sound = Rc::new(RefCell::new(Player::with(
        Audio::silent(0.0),
        PreviewSong::bundled,
    )));
    let (ui, env, _) = shell::library_ui("start", Rc::new(str::to_string), sound.clone(), &songs);
    let mut h = Harness::with_gestures(DESIGN, DESIGN, 1.0, TextMode::Own, false);
    h.ui = ui;
    h.run(0.4);
    assert_eq!(env.state.borrow().screen, "start");
    assert_eq!(sound.borrow().status(), (None, None));
    h.key("ArrowDown", "ArrowDown");
    h.key("ArrowDown", "ArrowDown");
    h.key("Enter", "Enter");
    h.run(0.4);
    assert_eq!(env.state.borrow().screen, "options");
    for _ in 0..3 {
        h.key("Tab", "Tab");
    }
    h.run(0.2);
    assert_eq!(env.state.borrow().section, "Timing");
    h.tap(PointerKind::Mouse, "Option0.Button0");
    h.run(0.4);
    assert_eq!(env.state.borrow().screen, "calibration");
    h.requests.clear();
    h.key("Backspace", "Backspace");
    h.run(0.4);
    assert_eq!(env.state.borrow().screen, "options");
    assert_eq!(env.state.borrow().section, "Timing");
    assert!(
        h.requests
            .iter()
            .any(|r| matches!(r, ddi_ui::UiRequest::FieldPreview(Some(_))))
    );
    h.key("Backspace", "Backspace");
    h.run(0.4);
    assert_eq!(env.state.borrow().screen, "start");
    assert_eq!(h.ui.focused(), Some("Options"));
    h.key("ArrowUp", "ArrowUp");
    h.key("ArrowUp", "ArrowUp");
    h.key("Enter", "Enter");
    h.run(0.4);
    assert_eq!(env.state.borrow().screen, "select");
    let song = env.state.borrow().song_id.clone();
    let chart = env.state.borrow().chart.clone();
    h.tap(PointerKind::Mouse, "Link.Options");
    h.run(0.4);
    assert_eq!(env.state.borrow().screen, "options");
    assert!(sound.borrow().paused());
    h.ui.broadcast(ddi_ui::Message::new(FEED));
    h.run(0.4);
    sound.borrow_mut().tick();
    assert!(sound.borrow().status().1.is_none());
    h.key("Escape", "Escape");
    h.run(0.4);
    assert_eq!(env.state.borrow().screen, "select");
    assert_eq!(env.state.borrow().song_id, song);
    h.key("Backspace", "Backspace");
    h.run(0.4);
    assert_eq!(env.state.borrow().screen, "start");
    assert_eq!(h.ui.focused(), Some("Singleplayer"));
    sound.borrow_mut().tick();
    assert!(sound.borrow().status().1.is_none());
    h.key("Enter", "Enter");
    h.run(0.4);
    assert_eq!(env.state.borrow().song_id, song);
    assert_eq!(env.state.borrow().chart, chart);
    h.key("Escape", "Escape");
    h.run(0.4);
    h.tap(PointerKind::Mouse, "Quit");
    assert_eq!(h.requests.last(), Some(&ddi_ui::UiRequest::Quit));
}
