//! A stage due while the player is on a screen opened over their room
//! (decisions round D2, item 1; the maintainer, 2026-11-15: "Tell
//! them"): desktop keeps the room's song select under Options and
//! Calibrate on the UI's stack, running the room; when the room hands
//! the player a stage's prompt meanwhile, a notice over the screen says
//! so, and its button asks the screens over the room to leave, each by
//! its own Back, the prompt opening once the room shows again. Nothing
//! is closed unasked. The real library host (the bundled songs) and
//! desktop's own Options and Calibrate, no window, no sound, no GPU.
//!
//! CPU practice's player is the one who picks, so a stage never comes
//! due under Options there by the screens alone: the pick is made on the
//! room's client directly (as another member's pick reaches an online
//! room's member; `ddi_ui`'s `room_tests.rs` shows that over the fake
//! room).

mod common;
use std::cell::RefCell;
use std::rc::Rc;

use ddi_desktop::{shell, songs::Songs};
use ddi_ui::screens::DESIGN;
use ddi_ui::screens::data::Difficulty;
use ddi_ui::screens::room::DUE_NOTICE;
use ddi_ui::{PointerKind, TextMode, testing::Harness};

fn layers(h: &Harness) -> Vec<String> {
    h.ui.layers().iter().map(|l| l.name.clone()).collect()
}

#[test]
fn a_stage_due_under_options_and_calibrate_is_told_and_gone_to() {
    common::indexes();
    let songs = Songs::new(Vec::new());
    songs.skip_offer();
    let (ui, env, _) = shell::theme_ui("start", Rc::new(str::to_string));
    let mut h = Harness::with_gestures(DESIGN, DESIGN, 1.0, TextMode::Own, false);
    h.ui = ui;
    h.run(0.3);
    let entry = common::practice::song_entry("beatucada", "Beatucada", 1, Difficulty::Hard);
    for nested in [false, true] {
        // A fresh practice room, its stages' plays asked through `slot`.
        ddi_desktop::room::discard();
        let room = ddi_desktop::room::current();
        room.borrow_mut().set_rules_seen(true);
        let slot: Rc<RefCell<Option<ddi_desktop::play::Request>>> = Rc::default();
        h.ui.push(ddi_desktop::room::room_screen_with(
            &env,
            Some(songs.clone()),
            None,
            Some(slot.clone()),
        ));
        h.run(0.8);
        assert_eq!(env.state.borrow().screen, "select");
        // Options over the room, and Calibrate over Options.
        h.tap(PointerKind::Mouse, "Link.Options");
        h.run(0.8);
        assert_eq!(env.state.borrow().screen, "options");
        if nested {
            h.tap(PointerKind::Mouse, "Link.Calibrate");
            h.run(0.8);
            assert_eq!(env.state.borrow().screen, "calibration", "{:?}", layers(&h));
        }
        let over = layers(&h);
        // The stage picked meanwhile: prepared under the screens, told
        // over them, nothing handed to the play host.
        room.borrow_mut().pick(&entry, &entry.charts[0]).unwrap();
        h.run(0.8);
        assert_eq!(layers(&h).last().map(String::as_str), Some(DUE_NOTICE), "{:?}", layers(&h));
        assert!(slot.borrow().is_none(), "no prompt opens under the screens");
        assert!(
            h.texts().iter().any(|t| t == "Stage 1 is starting"),
            "{:?}",
            h.texts()
        );
        // Nothing closes by itself.
        h.run(2.0);
        let mut told = over.clone();
        told.push(DUE_NOTICE.into());
        assert_eq!(layers(&h), told);
        // Go to Ready: the screens over the room leave, the prompt is
        // handed to the play host.
        assert_eq!(h.ui.focused(), Some("Go"));
        h.key("Enter", "Enter");
        h.run(2.0);
        assert_eq!(env.state.borrow().screen, "select", "{:?}", layers(&h));
        assert_eq!(layers(&h).last().map(String::as_str), Some("select"));
        let request = slot.borrow_mut().take().expect("the stage's prompt asked for");
        assert!(request.stage.is_some());
        // Back to the start menu for the next round.
        ddi_desktop::room::discard();
        while layers(&h).last().map(String::as_str) == Some("select") {
            h.key("Escape", "Escape");
            h.run(0.8);
        }
    }
}

/// The same over a stage's results (decisions round D4, item C: "Tell
/// them the same way"): the room's screen under the results prepares
/// the next stage's song, the results say it is due, and the notice's
/// button leaves them as their own button does, the prompt asked for
/// once the room shows again.
#[test]
fn a_stage_due_under_a_stages_results_is_told_and_gone_to() {
    use ddi_ui::screens::data::ResultsKind;
    use ddi_ui::screens::results::{ResultsActions, ResultsScreen};
    common::indexes();
    let songs = Songs::new(Vec::new());
    songs.skip_offer();
    let (ui, env, _) = shell::theme_ui("start", Rc::new(str::to_string));
    let mut h = Harness::with_gestures(DESIGN, DESIGN, 1.0, TextMode::Own, false);
    h.ui = ui;
    h.run(0.3);
    let entry = common::practice::song_entry("beatucada", "Beatucada", 1, Difficulty::Hard);
    ddi_desktop::room::discard();
    let room = ddi_desktop::room::current();
    room.borrow_mut().set_rules_seen(true);
    let slot: Rc<RefCell<Option<ddi_desktop::play::Request>>> = Rc::default();
    h.ui.push(ddi_desktop::room::room_screen_with(
        &env,
        Some(songs.clone()),
        None,
        Some(slot.clone()),
    ));
    h.run(0.8);
    assert_eq!(env.state.borrow().screen, "select");
    // A stage's results over the room, as the play host leaves them.
    let mut data = ddi_ui::screens::sample::results();
    data.kind = ResultsKind::Stage;
    h.ui.push(
        ResultsScreen::new(env.clone(), data, ResultsActions::default())
            .with_room(room.clone(), ddi_room::StageId(1)),
    );
    h.run(0.8);
    assert_eq!(layers(&h).last().map(String::as_str), Some("results"));
    // The next stage picked meanwhile: prepared under the results, told
    // over them, nothing handed to the play host.
    room.borrow_mut().pick(&entry, &entry.charts[0]).unwrap();
    h.run(0.8);
    assert_eq!(layers(&h).last().map(String::as_str), Some(DUE_NOTICE), "{:?}", layers(&h));
    assert!(slot.borrow().is_none(), "no prompt opens under the results");
    assert_eq!(h.ui.focused(), Some("Go"));
    h.key("Enter", "Enter");
    h.run(2.0);
    assert_eq!(layers(&h).last().map(String::as_str), Some("select"), "{:?}", layers(&h));
    let request = slot.borrow_mut().take().expect("the stage's prompt asked for");
    assert!(request.stage.is_some());
    ddi_desktop::room::discard();
}
