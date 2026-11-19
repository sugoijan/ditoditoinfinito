//! The volume on a phone's song select (round AD, 2026-11-19): its
//! layout has no quick volume, so More has a Volume item that opens the
//! same control in a small dialog, in singleplayer and in a room (the
//! follower's More too); a wide layout's More has none.

use std::cell::RefCell;
use std::rc::Rc;

use super::data::{ChartEntry, Difficulty, SelectData, SongEntry};
use super::select::{Feed, OpenFn, PlayFn, SelectCall, SelectScreen, SharedFeed, ShellFn};
use super::select_dialogs::VOLUME_DIALOG_CONTROL;
use super::test_kit::{Orders, env, top};
use super::{COMPACT, DESIGN, Env, sample};
use crate::PointerKind;
use crate::TextMode;
use crate::state::settings::{Settings, slider_to_volume};
use crate::testing::Harness;

type Calls = Rc<RefCell<Vec<SelectCall>>>;

fn songs() -> SelectData {
    let songs = (0..4)
        .map(|k| SongEntry {
            id: format!("s{k}"),
            title: format!("Song {k}"),
            artist: "Artist".into(),
            bpm_text: "150".into(),
            length: 120.0,
            charts: vec![ChartEntry {
                style: "dance-single".into(),
                difficulty: Difficulty::Hard,
                meter: 9,
                notes: 300,
                ..ChartEntry::default()
            }],
            ..SongEntry::default()
        })
        .collect();
    SelectData {
        styles: sample::styles(),
        songs,
        style: Some("dance-single".into()),
        ..SelectData::default()
    }
}

fn shelled(size: [f32; 2]) -> (Harness, Calls) {
    let design = if size[1] < 500.0 { COMPACT } else { DESIGN };
    let mut h = Harness::new(design, size, 1.0, TextMode::Mirror);
    let play: PlayFn = Rc::new(|_, _, _: &Env, _| {});
    let open: OpenFn = Rc::new(|_, _| {});
    let calls: Calls = Rc::default();
    let c = calls.clone();
    let shell: ShellFn = Rc::new(move |call, _| c.borrow_mut().push(call));
    let feed: SharedFeed = Rc::new(RefCell::new(Feed::default()));
    h.push(SelectScreen::new(env(), songs(), None, play, open).with_shell(shell, feed));
    h.run(0.5);
    (h, calls)
}

fn set_volume(gain: f32) {
    let mut s = Settings::load();
    s.volume = gain;
    s.save();
}

fn last_volume(calls: &Calls) -> Option<f32> {
    calls.borrow().iter().rev().find_map(|c| match c {
        SelectCall::Volume(g) => Some(*g),
        _ => None,
    })
}

const BUTTON: &str = "Volume.Button";

/// A phone's More has Volume: its dialog's button mutes (its slider is
/// already up), saying "Sound is muted", and brings the level back; Up
/// and Down step it; the preview follows each change and the settings
/// keep it; Back closes it.
#[test]
fn a_phones_more_opens_the_volume() {
    ddi_l10n::set_language("en");
    set_volume(slider_to_volume(0.5));
    let (mut h, calls) = shelled(COMPACT);
    assert!(h.ui.actor(BUTTON).is_none(), "a phone has no quick volume");
    h.tap(PointerKind::Touch, "More");
    h.run(0.3);
    assert!(h.texts().iter().any(|t| t == "Volume"), "{:?}", h.texts());
    h.tap(PointerKind::Touch, "Item.MoreVolume");
    h.run(0.3);
    assert_eq!(top(&h), "volume");
    assert!(!h.texts().iter().any(|t| t == "Sound is muted"));
    assert!(
        h.ui.actor(&format!("{VOLUME_DIALOG_CONTROL}.Popup"))
            .is_some_and(|p| p.visible),
        "its slider shows at once"
    );
    // A tap on its button mutes; the next brings the level back.
    h.tap(PointerKind::Touch, BUTTON);
    h.run(0.1);
    assert_eq!(Settings::load().volume, 0.0);
    assert_eq!(last_volume(&calls), Some(0.0));
    assert!(h.texts().iter().any(|t| t == "Sound is muted"));
    h.tap(PointerKind::Touch, BUTTON);
    h.run(0.1);
    assert_eq!(Settings::load().volume, slider_to_volume(0.5));
    assert!(!h.texts().iter().any(|t| t == "Sound is muted"));
    // Keys: the first press focuses the control, Up then steps it by 5%.
    if h.ui.focused() != Some(VOLUME_DIALOG_CONTROL) {
        h.key("ArrowUp", "ArrowUp");
        h.run(0.1);
    }
    assert_eq!(h.ui.focused(), Some(VOLUME_DIALOG_CONTROL));
    assert_eq!(Settings::load().volume, slider_to_volume(0.5));
    h.key("ArrowUp", "ArrowUp");
    h.run(0.1);
    assert_eq!(Settings::load().volume, slider_to_volume(0.55));
    assert_eq!(last_volume(&calls), Some(slider_to_volume(0.55)));
    // Start mutes.
    h.key("Enter", "Enter");
    h.run(0.1);
    assert_eq!(Settings::load().volume, 0.0);
    // Back closes it.
    h.key("Escape", "Escape");
    h.run(0.3);
    assert_eq!(top(&h), "select");
}

/// A wide layout keeps its quick volume and has no Volume in More.
#[test]
fn a_wide_layouts_more_has_no_volume() {
    ddi_l10n::set_language("en");
    let (mut h, _) = shelled([1280.0, 720.0]);
    assert!(h.ui.actor(BUTTON).is_some());
    h.tap(PointerKind::Mouse, "More");
    h.run(0.3);
    assert!(!h.texts().iter().any(|t| t == "Volume"), "{:?}", h.texts());
}

/// In a room on a phone, the follower's More has Volume too (they have no
/// search row, and no quick volume on this layout).
#[test]
fn a_rooms_follower_on_a_phone_has_the_volume_in_more() {
    use super::room_tests::Three;
    ddi_l10n::set_language("en");
    set_volume(slider_to_volume(0.5));
    let t = Three::new(false);
    let (ea, eb) = (env(), env());
    let orders = Orders::default();
    let mut ha = super::test_kit::harness();
    ha.push(
        sample::room_select(&ea, t.alice.clone(), songs())
            .with_room(t.alice.clone(), super::test_kit::shell(&t.alice, &orders)),
    );
    let mut hb = Harness::new(COMPACT, COMPACT, 1.0, TextMode::Own);
    hb.push(
        sample::room_select(&eb, t.bob.clone(), songs())
            .with_room(t.bob.clone(), super::test_kit::shell(&t.bob, &orders)),
    );
    t.settle(400, &mut [&mut ha, &mut hb]);
    ha.run(0.5);
    hb.run(0.5);
    assert_eq!(eb.state.borrow().favorite, None, "Bob follows");
    hb.tap(PointerKind::Touch, "More");
    hb.run(0.3);
    assert!(hb.texts().iter().any(|t| t == "Volume"), "{:?}", hb.texts());
    hb.tap(PointerKind::Touch, "Item.MoreVolume");
    hb.run(0.3);
    assert_eq!(top(&hb), "volume");
    hb.tap(PointerKind::Touch, BUTTON);
    hb.run(0.1);
    assert_eq!(Settings::load().volume, 0.0);
}
