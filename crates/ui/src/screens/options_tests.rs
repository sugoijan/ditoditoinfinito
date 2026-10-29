//! The options screen on the player's settings, through the shell every
//! platform shares (`options_shell`), with the plain look and a platform
//! that writes down what it is asked.

use std::cell::RefCell;
use std::collections::HashSet;
use std::rc::Rc;

use ddi_engine::ConnectedPad;
use ddi_platform::storage::{self, Area, MemoryStore};
use ddi_platform::{DeviceId, HostTime, RawInput};

use super::options::{OptionsFeed, OptionsScreen};
use super::options_model::{OptionsModel, Reset, Sample, World};
use super::options_shell::{self, Leave, OptionsPlatform};
use super::{DESIGN, Env, SharedTheme};
use crate::dev_widgets::FoldFn;
use crate::look::Plain;
use crate::screen::Cx;
use crate::state::settings::{KEY, Settings};
use crate::testing::Harness;
use crate::{MenuAction, PointerKind, TextMode, UiEvent, UiRequest};

#[derive(Default)]
struct Fake {
    log: RefCell<Vec<String>>,
}

impl OptionsPlatform for Fake {
    fn sample(&self, play: bool, volume: f32, cx: &mut Cx) -> bool {
        self.log
            .borrow_mut()
            .push(format!("sample {play} {volume:.2} {}", cx.in_gesture()));
        play
    }
    fn volume(&self, v: f32) {
        self.log.borrow_mut().push(format!("volume {v:.3}"));
    }
    fn export(&self) {
        self.log.borrow_mut().push("export".into());
    }
    fn import_plays(&self, cx: &mut Cx) {
        self.log
            .borrow_mut()
            .push(format!("import {}", cx.in_gesture()));
    }
    fn remove(&self, reset: &Reset) {
        self.log.borrow_mut().push(format!("remove {reset:?}"));
    }
    fn held(&self, _id: &str) -> HashSet<String> {
        HashSet::new()
    }
    fn leave(&self, to: Leave, _cx: &mut Cx) {
        self.log.borrow_mut().push(format!("leave {to:?}"));
    }
}

struct Rig {
    h: Harness,
    state: SharedTheme,
    model: Rc<RefCell<OptionsModel>>,
    fake: Rc<Fake>,
}

fn rig(size: [f32; 2], design: [f32; 2]) -> Rig {
    storage::install(Rc::new(MemoryStore::default()));
    let fold: FoldFn =
        Rc::new(|s: &str| ddi_library::search::Fold::nfkd(&ddi_library::search::Nfkd, s));
    let env = Env::new(Rc::new(Plain), fold);
    let state = env.state.clone();
    let world = World {
        pads_supported: true,
        sample: Sample::Ready,
        connected: vec![ConnectedPad {
            vendor_product: None,
            id: "Pad".into(),
            index: 0,
            standard: true,
        }],
        imported: 2,
        plays: 5,
        ..World::default()
    };
    let model = Rc::new(RefCell::new(OptionsModel::new(Settings::load(), world)));
    let feed: OptionsFeed = Rc::new(RefCell::new(None));
    let fake = Rc::new(Fake::default());
    let shell = options_shell::shell(env.clone(), model.clone(), feed.clone(), fake.clone());
    let data = model.borrow().data();
    let mut h = Harness::new(DESIGN, size, 1.0, TextMode::Mirror);
    h.ui.set_design(design);
    h.push(OptionsScreen::new(env, data, None).with_shell(shell, feed));
    h.run(0.3);
    Rig {
        h,
        state,
        model,
        fake,
    }
}

impl Rig {
    fn section(&mut self, name: &str) {
        for _ in 0..12 {
            if self.state.borrow().section == name {
                return;
            }
            self.h.key("Tab", "Tab");
            self.h.run(0.05);
        }
        panic!("no section {name}");
    }

    /// Down (or up) the rows to `id`.
    fn row(&mut self, id: &str) {
        for key in ["ArrowDown", "ArrowUp"] {
            for _ in 0..60 {
                if self.state.borrow().option_id == id {
                    return;
                }
                self.h.key(key, key);
            }
        }
        panic!("no row {id}: {:?}", self.state.borrow().option_rows);
    }

    fn raw(&mut self, code: &str) {
        self.h.key_down(code, code);
        self.h.key_up(code, code);
        self.h.run(0.02);
    }

    fn top(&self) -> String {
        self.h
            .ui
            .layers()
            .into_iter()
            .rfind(|l| l.phase != "out")
            .map(|l| l.name.clone())
            .unwrap_or_default()
    }

    fn logged(&self, what: &str) -> bool {
        self.fake.log.borrow().iter().any(|l| l.starts_with(what))
    }
}

fn stored() -> Settings {
    storage::load(Area::Local, KEY).unwrap_or_default()
}

#[test]
fn rows_change_the_settings_and_save_them() {
    let mut r = rig([1280.0, 720.0], DESIGN);
    assert_eq!(r.state.borrow().option_id, "speed");
    r.h.key("ArrowRight", "ArrowRight");
    assert_eq!(r.state.borrow().value, "×2.75");
    assert_eq!(stored().speed, 2.75);
    // A tap on a choice.
    r.row("reverse");
    r.h.run(0.05);
    let i = r
        .state
        .borrow()
        .option_rows
        .iter()
        .position(|o| o == "reverse")
        .unwrap();
    r.h.tap(PointerKind::Mouse, &format!("Option{i}.Choice1"));
    assert!(stored().reverse);
    assert!(r.model.borrow().data().preview.ends_with("Reverse"));
    // The volume: changed by a key, the sample plays (in the key's
    // gesture) at the volume chosen; its row stops it.
    r.section("Sound");
    r.h.key("ArrowLeft", "ArrowLeft");
    assert!(r.logged("volume"));
    assert!(r.logged("sample true"), "{:?}", r.fake.log.borrow());
    assert!(
        r.fake
            .log
            .borrow()
            .iter()
            .any(|l| l.ends_with(" true") && l.starts_with("sample"))
    );
    r.row("sample");
    assert_eq!(r.state.borrow().value, "Stop");
    r.h.key("Enter", "Enter");
    assert!(r.logged("sample false"));
    assert_eq!(r.state.borrow().value, "Play a sample");
    r.h.key("Enter", "Enter");
    assert_eq!(r.state.borrow().value, "Stop");
    r.h.key("Enter", "Enter");
    // The play history's export, in Data.
    r.section("Data");
    r.row("history.export");
    r.h.key("Enter", "Enter");
    assert!(r.logged("export"));
    // Reduced motion and the rest, by pad.
    r.section("Display");
    r.row("debug");
    r.h.pad(MenuAction::Right, true);
    r.h.pad(MenuAction::Right, false);
    assert!(stored().debug);
    // Back leaves, and the preview goes with the screen.
    r.h.key("Escape", "Escape");
    assert!(r.logged("leave Back"));
    assert_eq!(
        r.h.requested(|q| matches!(q, UiRequest::FieldPreview(None))),
        1
    );
}

#[test]
fn the_preview_is_told_once_and_again_when_it_moves() {
    let mut r = rig([1280.0, 720.0], DESIGN);
    r.h.run(0.5);
    let told = |h: &Harness| h.requested(|q| matches!(q, UiRequest::FieldPreview(Some(_))));
    assert_eq!(told(&r.h), 1);
    let rect = r.state.borrow().preview.unwrap();
    assert!(
        rect[0] > 640.0 && rect[2] > 200.0 && rect[3] > 200.0,
        "{rect:?}"
    );
    // A change does not move it.
    r.h.key("ArrowRight", "ArrowRight");
    r.h.run(0.2);
    assert_eq!(told(&r.h), 1);
    // A new size does.
    r.h.send(UiEvent::Resize {
        size: [1600.0, 900.0],
        ratio: 2.0,
    });
    r.h.run(0.2);
    assert_eq!(told(&r.h), 2);
    let big = r.state.borrow().preview.unwrap();
    assert!(big[2] > rect[2] * 2.0, "{big:?} {rect:?}");
}

#[test]
fn the_preview_layout_is_available_in_every_section_without_changing_play() {
    for (size, design) in [
        ([1280.0, 720.0], DESIGN),
        ([844.0, 390.0], super::COMPACT),
        ([390.0, 844.0], super::COMPACT),
    ] {
        let mut r = rig(size, design);
        let settings = r.model.borrow().settings.clone();
        let sections = r.model.borrow().data().sections.len();
        for _ in 0..sections {
            let section = r.state.borrow().section.clone();
            assert!(
                !r.state
                    .borrow()
                    .option_rows
                    .iter()
                    .any(|id| id == "preview_layout")
            );
            let selector = r.h.ui.bounds("PreviewLayout").unwrap();
            let preview = r.h.ui.bounds("Preview").unwrap();
            assert!(
                selector[3] < preview[1],
                "{size:?}: {selector:?} {preview:?}"
            );
            r.h.tap(PointerKind::Touch, "PreviewLayout.Up");
            assert_eq!(r.model.borrow().preview_settings().style, "dance-solo");
            assert_eq!(r.state.borrow().section, section);
            assert_eq!(r.state.borrow().option_id, "preview_layout");
            r.h.pad(MenuAction::Left, true);
            r.h.pad(MenuAction::Left, false);
            assert_eq!(r.model.borrow().preview_settings().style, "dance-single");
            r.h.key("ArrowUp", "ArrowUp");
            assert_ne!(r.state.borrow().option_id, "preview_layout");
            // P cycles the preview's layout from anywhere, the focus
            // staying on the row; Shift+P backwards, past the ends.
            let row = r.state.borrow().option_id.clone();
            r.h.key("KeyP", "p");
            assert_eq!(r.model.borrow().preview_settings().style, "dance-solo");
            assert_eq!(r.state.borrow().option_id, row);
            let shift = crate::event::Modifiers {
                shift: true,
                ..Default::default()
            };
            r.h.key_with("KeyP", "P", shift);
            assert_eq!(r.model.borrow().preview_settings().style, "dance-single");
            r.h.key_with("KeyP", "P", shift);
            let last = r.model.borrow().bind_layouts().last().unwrap().id.clone();
            assert_eq!(r.model.borrow().preview_settings().style, last);
            r.h.key("KeyP", "p");
            assert_eq!(r.model.borrow().preview_settings().style, "dance-single");
            assert_eq!(r.model.borrow().settings, settings);
            assert_eq!(stored(), settings);
            r.h.key("Tab", "Tab");
            r.h.run(0.05);
        }
        // A controller reaches the selector after the section's last row.
        let last = r.state.borrow().option_rows.last().unwrap().clone();
        r.row(&last);
        r.h.pad(MenuAction::Down, true);
        r.h.pad(MenuAction::Down, false);
        assert_eq!(r.state.borrow().option_id, "preview_layout");
    }
}

#[test]
fn the_keyboard_is_bound_by_keys_through_the_dialog() {
    let mut r = rig([1280.0, 720.0], DESIGN);
    r.section("Controls");
    r.row("keys");
    r.h.key("Enter", "Enter");
    r.h.run(0.2);
    assert_eq!(r.top(), "bind");
    assert!(r.h.ui.wants_raw());
    // Arrows bind, they do not move the options' focus.
    for k in ["KeyD", "KeyF", "KeyJ", "KeyK"] {
        r.raw(k);
    }
    assert_eq!(r.state.borrow().option_id, "keys");
    assert!(r.state.borrow().status.starts_with("bind review"));
    // A lane lights while held; Enter saves.
    r.h.key_down("KeyJ", "j");
    r.h.run(0.02);
    r.h.key_up("KeyJ", "j");
    r.raw("Enter");
    r.h.run(0.3);
    assert_ne!(r.top(), "bind");
    let keys = stored()
        .controls
        .keyboard(&ddi_chart::Layout::dance_single());
    assert_eq!(keys[0], ["KeyD"]);
    assert_eq!(keys[3], ["KeyK"]);
    assert_eq!(r.state.borrow().notice, "The keyboard is bound for Single.");
    // Escape cancels, nothing saved.
    r.h.key("Enter", "Enter");
    r.h.run(0.2);
    r.raw("KeyQ");
    r.raw("Escape");
    r.h.run(0.3);
    assert_ne!(r.top(), "bind");
    assert_eq!(
        stored()
            .controls
            .keyboard(&ddi_chart::Layout::dance_single())[0],
        ["KeyD"]
    );
    // One more key for a lane, taken from the lane it played.
    r.row("keys.lane.0");
    r.h.key("Enter", "Enter");
    r.h.run(0.2);
    assert_eq!(r.top(), "key");
    r.raw("KeyF");
    r.h.run(0.3);
    let keys = stored()
        .controls
        .keyboard(&ddi_chart::Layout::dance_single());
    assert_eq!(keys[0], ["KeyD", "KeyF"]);
    assert!(keys[1].is_empty());
    assert_eq!(r.state.borrow().notice, "KeyF moved from Down to Left.");
}

#[test]
fn a_controller_is_bound_with_its_own_presses() {
    let mut r = rig([1280.0, 720.0], DESIGN);
    r.section("Controls");
    r.row("pad.0");
    r.h.key("Enter", "Enter");
    r.h.run(0.2);
    assert_eq!(r.top(), "bind");
    let edge = |control: &str, pressed: bool| {
        UiEvent::Raw(RawInput {
            device: DeviceId::Gamepad("Pad".into()),
            control: control.into(),
            pressed,
            host_time: HostTime(0.0),
            slot: 0,
        })
    };
    for b in [
        "button:14",
        "button:13",
        "button:12",
        "button:15",
        "button:9",
        "button:8",
    ] {
        // Arm capture and release its trigger before the candidate.
        r.h.send(edge("button:0", true));
        r.h.send(edge("button:0", false));
        r.h.send(edge(b, true));
        r.h.send(edge(b, false));
    }
    r.h.run(0.1);
    assert!(r.state.borrow().status.starts_with("bind review"));
    // The pad's new Start saves.
    r.h.send(edge("button:9", true));
    r.h.run(0.3);
    assert_ne!(r.top(), "bind");
    let pad = stored().controls.pad("Pad", true).unwrap();
    assert_eq!(pad.start.as_deref(), Some("button:9"));
    assert_eq!(
        pad.table("dance-single").unwrap()[0],
        vec!["button:14".to_string()]
    );
}

#[test]
fn resets_ask_first() {
    let mut r = rig([1280.0, 720.0], DESIGN);
    r.h.key("ArrowRight", "ArrowRight");
    assert_eq!(stored().speed, 2.75);
    r.section("Data");
    r.row("reset.settings");
    r.h.key("Enter", "Enter");
    r.h.run(0.2);
    assert_eq!(r.top(), "confirm");
    // Start on the dialog's first button keeps everything.
    r.h.key("Enter", "Enter");
    r.h.run(0.3);
    assert_ne!(r.top(), "confirm");
    assert_eq!(stored().speed, 2.75);
    r.h.key("Enter", "Enter");
    r.h.run(0.2);
    r.h.tap(PointerKind::Mouse, "Confirm");
    r.h.run(0.3);
    assert_eq!(stored().speed, 2.5);
    assert_eq!(r.state.borrow().notice, "Settings reset.");
    assert!(r.logged("remove Settings"));
    // Removing the imported songs: the platform's part too.
    r.row("reset.songs");
    r.h.key("Enter", "Enter");
    r.h.run(0.2);
    r.h.tap(PointerKind::Mouse, "Confirm");
    r.h.run(0.3);
    assert!(r.logged("remove Songs"));
}

#[test]
fn rows_scroll_to_the_focused_one_and_by_wheel_and_drag() {
    let mut r = rig([1280.0, 720.0], DESIGN);
    r.section("Controls");
    let last = r.state.borrow().option_rows.last().cloned().unwrap();
    r.row(&last);
    r.h.run(0.1);
    let scroll = r.state.borrow().scroll;
    assert!(scroll > 0.0);
    let n = r.state.borrow().option_rows.len() - 1;
    let row = r.h.ui.bounds(&format!("Option{n}")).unwrap();
    let clip = r.h.ui.bounds("RowsClip").unwrap();
    assert!(row[3] <= clip[3] + 0.5, "{row:?} {clip:?}");
    // The wheel and a drag move them back.
    let at = r.h.centre("RowsClip");
    r.h.wheel_lines(at, [0.0, -3.0]);
    r.h.run(0.1);
    assert!(r.state.borrow().scroll < scroll);
    r.h.drag(PointerKind::Touch, at, [at[0], at[1] + 400.0], 0.3, 10);
    r.h.run(0.1);
    assert_eq!(r.state.borrow().scroll, 0.0);
}

#[test]
fn a_tap_on_a_section_opens_it() {
    let mut r = rig([1280.0, 720.0], DESIGN);
    r.h.tap(PointerKind::Mouse, "Section3");
    r.h.run(0.1);
    assert_eq!(r.state.borrow().section, "Timing");
    // Keys still move through its rows.
    r.h.key("ArrowDown", "ArrowDown");
    assert_eq!(r.state.borrow().option_id, "audio_offset");
}

#[test]
fn a_controller_alone_gets_out_of_every_dialog() {
    let mut r = rig([1280.0, 720.0], DESIGN);
    let edge = |control: &str, pressed: bool| {
        UiEvent::Raw(RawInput {
            device: DeviceId::Gamepad("Pad".into()),
            control: control.into(),
            pressed,
            host_time: HostTime(0.0),
            slot: 0,
        })
    };
    r.section("Controls");
    // The key dialog: the pad's B (it is standard) cancels.
    r.row("keys.lane.0");
    r.h.key("Enter", "Enter");
    r.h.run(0.2);
    assert_eq!(r.top(), "key");
    r.h.send(edge("button:1", true));
    r.h.send(edge("button:1", false));
    r.h.run(0.3);
    assert_ne!(r.top(), "key");
    // Binding the keyboard: the pad's B cancels.
    r.row("keys");
    r.h.key("Enter", "Enter");
    r.h.run(0.2);
    assert_eq!(r.top(), "bind");
    r.h.send(edge("button:1", true));
    r.h.run(0.3);
    assert_ne!(r.top(), "bind");
    r.h.send(edge("button:1", false));
    // Binding the pad itself: holding any button a second gets out.
    r.row("pad.0");
    r.h.key("Enter", "Enter");
    r.h.run(0.2);
    assert_eq!(r.top(), "bind");
    r.h.send(edge("button:5", true));
    r.h.run(0.5);
    assert_eq!(r.top(), "bind");
    r.h.run(0.7);
    assert_ne!(r.top(), "bind");
}

#[test]
fn the_volume_is_a_slider() {
    for (size, design) in [([1280.0, 720.0], DESIGN), ([390.0, 844.0], super::COMPACT)] {
        let mut r = rig(size, design);
        r.section("Sound");
        r.row("volume");
        let start = r.state.borrow().value.clone();
        // Keys step it, and the sample starts in the key's gesture.
        r.h.key("ArrowRight", "ArrowRight");
        assert_ne!(r.state.borrow().value, start);
        assert!(r.logged("sample true"), "{:?}", r.fake.log.borrow());
        let track = (0..12)
            .find_map(|i| r.h.ui.bounds(&format!("Option{i}.Track")))
            .expect("the slider's track");
        let y = (track[1] + track[3]) / 2.0;
        // A tap at its left end: muted; at its right end: full.
        r.h.tap_at(PointerKind::Mouse, [track[0] + 2.0, y]);
        r.h.run(0.1);
        assert_eq!(r.state.borrow().value, "0%");
        assert_eq!(stored().volume, 0.0);
        r.h.tap_at(PointerKind::Mouse, [track[2] - 2.0, y]);
        r.h.run(0.1);
        assert_eq!(r.state.borrow().value, "100%");
        // A drag follows the pointer; the rows do not scroll.
        let scroll = r.state.borrow().scroll;
        let mid = (track[0] + track[2]) / 2.0;
        r.h.drag(
            PointerKind::Touch,
            [track[2] - 8.0, y],
            [mid, y + 40.0],
            0.3,
            6,
        );
        r.h.run(0.1);
        let v: f32 = r
            .state
            .borrow()
            .value
            .trim_end_matches('%')
            .parse()
            .unwrap();
        assert!((40.0..=60.0).contains(&v), "{v} at {size:?}");
        assert_eq!(r.state.borrow().scroll, scroll);
    }
}

#[test]
fn the_play_history_is_reset_after_asking() {
    let mut r = rig([1280.0, 720.0], DESIGN);
    r.section("Data");
    r.row("history.export");
    assert_eq!(r.state.borrow().value, "Export");
    r.h.key("ArrowRight", "ArrowRight");
    assert_eq!(r.state.borrow().value, "Import…");
    r.h.key("ArrowRight", "ArrowRight");
    assert_eq!(r.state.borrow().value, "Reset");
    r.h.key("Enter", "Enter");
    r.h.run(0.3);
    assert_eq!(r.top(), "confirm");
    // How many, and Export offered first; Cancel has the focus.
    let texts = r.h.texts().join(" ");
    assert!(texts.contains("the 5 plays"), "{texts}");
    assert_eq!(r.h.ui.focused(), Some("Cancel"));
    r.h.tap(PointerKind::Mouse, "Extra");
    r.h.run(0.1);
    assert!(r.logged("export"));
    assert_eq!(r.top(), "confirm");
    assert!(!r.logged("remove"));
    r.h.tap(PointerKind::Mouse, "Confirm");
    r.h.run(0.3);
    assert_ne!(r.top(), "confirm");
    assert!(r.logged("remove History"));
}

#[test]
fn a_tap_on_the_rows_empty_space_presses_nothing() {
    let mut r = rig([1280.0, 720.0], DESIGN);
    r.section("Data");
    r.row("history.export");
    assert_eq!(r.state.borrow().value, "Export");
    let clip = r.h.ui.bounds("RowsClip").expect("the rows");
    let bottom = (0..40)
        .filter_map(|i| r.h.ui.bounds(&format!("Option{i}")))
        .map(|b| b[3])
        .fold(clip[1], f32::max);
    assert!(
        clip[3] - bottom > 40.0,
        "room under the rows: {clip:?} {bottom}"
    );
    // Under the last row, and far to the right of the focused one's
    // buttons: the rows keep the focus, and nothing is pressed.
    r.h.tap_at(
        PointerKind::Mouse,
        [(clip[0] + clip[2]) / 2.0, (bottom + clip[3]) / 2.0],
    );
    r.h.run(0.1);
    assert!(!r.logged("export"), "{:?}", r.fake.log.borrow());
    assert_eq!(r.h.ui.focused(), Some("RowsClip"));
    assert_eq!(r.state.borrow().option_id, "history.export");
    // The button itself still exports.
    let i = r
        .state
        .borrow()
        .option_rows
        .iter()
        .position(|id| id == "history.export")
        .unwrap();
    r.h.tap(PointerKind::Mouse, &format!("Option{i}.Button0"));
    assert!(r.logged("export"), "{:?}", r.fake.log.borrow());
}

/// The last cursor `requests` set, if any.
fn cursor_in(requests: &[UiRequest]) -> Option<crate::request::Cursor> {
    requests.iter().rev().find_map(|r| match r {
        UiRequest::SetCursor(c) => Some(*c),
        _ => None,
    })
}

#[test]
fn the_mouse_shows_a_pointer_over_what_a_tap_acts_on() {
    use crate::request::Cursor;
    let mut r = rig([1280.0, 720.0], DESIGN);
    r.section("Data");
    let i = r
        .state
        .borrow()
        .option_rows
        .iter()
        .position(|id| id == "history.export")
        .unwrap();
    let button = r.h.centre(&format!("Option{i}.Button0"));
    assert_eq!(cursor_in(&r.h.hover(button)), Some(Cursor::Pointer));
    // Still: nothing more to tell.
    assert_eq!(cursor_in(&r.h.hover([button[0] + 1.0, button[1]])), None);
    // The rows' empty space: the default again.
    let clip = r.h.ui.bounds("RowsClip").unwrap();
    assert_eq!(
        cursor_in(&r.h.hover(clip_mid(&clip))),
        Some(Cursor::Default)
    );
    // A section's tab (focusable): a pointer.
    assert_eq!(
        cursor_in(&r.h.hover(r.h.centre("Section0"))),
        Some(Cursor::Pointer)
    );
    // Back over the empty space, then a link.
    r.h.hover(clip_mid(&clip));
    assert_eq!(
        cursor_in(&r.h.hover(r.h.centre("Link.Calibrate"))),
        Some(Cursor::Pointer)
    );
    // Gone from the surface: the default.
    let left = r.h.send(crate::UiEvent::Pointer(crate::PointerEvent {
        id: 1,
        kind: PointerKind::Mouse,
        phase: crate::PointerPhase::Leave,
        pos: [0.0, 0.0],
        buttons: 0,
        modifiers: Default::default(),
        time: 0.0,
    }));
    assert_eq!(cursor_in(&left), Some(Cursor::Default));
}

fn clip_mid(clip: &[f32; 4]) -> [f32; 2] {
    [(clip[0] + clip[2]) / 2.0, clip[3] - 10.0]
}

#[test]
fn plays_are_imported_from_the_history_row_inside_the_press() {
    let mut r = rig([1280.0, 720.0], DESIGN);
    r.section("Data");
    r.row("history.export");
    r.h.key("ArrowRight", "ArrowRight");
    assert_eq!(r.state.borrow().value, "Import…");
    r.h.key("Enter", "Enter");
    assert!(r.logged("import true"));
    // With no plays kept, Import alone; while one runs, it is not offered.
    let mut m = OptionsModel::new(Settings::default(), World::default());
    let row = |m: &OptionsModel| {
        m.data()
            .sections
            .iter()
            .flat_map(|s| s.options.iter())
            .find(|o| o.id == "history.export")
            .cloned()
            .unwrap()
    };
    assert_eq!(row(&m).note, "no plays kept yet");
    assert_eq!(
        m.action("history.export", 0),
        super::options_model::Act::ImportPlays
    );
    m.world.importing_plays = true;
    assert_eq!(row(&m).note, "importing plays…");
    assert_eq!(
        m.action("history.export", 0),
        super::options_model::Act::Nothing
    );
    m.world.plays = 3;
    assert_eq!(
        m.action("history.export", 0),
        super::options_model::Act::Export
    );
    m.world.importing_plays = false;
    assert_eq!(
        m.action("history.export", 2),
        super::options_model::Act::Confirm(Reset::History)
    );
}

#[test]
fn the_footer_calibrates_from_any_section() {
    let mut r = rig([1280.0, 720.0], DESIGN);
    r.section("Data");
    assert!(r.h.ui.bounds("Link.Calibrate").is_some());
    r.h.tap(PointerKind::Mouse, "Link.Calibrate");
    assert!(r.logged("leave Calibrate"), "{:?}", r.fake.log.borrow());
}

#[test]
fn a_controller_lane_takes_one_more_control_as_the_keyboard_does() {
    let mut r = rig([1280.0, 720.0], DESIGN);
    let edge = |control: &str, pressed: bool| {
        UiEvent::Raw(RawInput {
            device: DeviceId::Gamepad("Pad".into()),
            control: control.into(),
            pressed,
            host_time: HostTime(0.0),
            slot: 0,
        })
    };
    let press = |r: &mut Rig, control: &str| {
        r.h.send(edge(control, true));
        r.h.send(edge(control, false));
        r.h.run(0.3);
    };
    let single = ddi_chart::Layout::dance_single();
    let table = || {
        stored()
            .controls
            .pad_table("Pad", true, &ddi_chart::Layout::dance_single())
            .unwrap()
    };
    r.section("Controls");
    // The standard pad's Left: the d-pad's and X.
    r.row("pad.0.lane.0");
    r.h.key("Enter", "Enter");
    r.h.run(0.2);
    assert_eq!(r.top(), "key");
    // Its Start is refused, a key asks for the controller.
    press(&mut r, "button:9");
    assert_eq!(r.top(), "key");
    r.raw("KeyQ");
    assert_eq!(r.top(), "key");
    // A shoulder button: added to Left, kept with the others.
    press(&mut r, "button:5");
    assert_ne!(r.top(), "key");
    assert_eq!(
        table()[0],
        ["button:4", "button:14", "button:2", "button:5"]
    );
    assert!(
        r.state
            .borrow()
            .notice
            .ends_with("moved from Right to Left."),
        "{}",
        r.state.borrow().notice
    );
    // A play reads both: the first and the added one play Left.
    let b = stored().controls.resolve(
        &single,
        &[ConnectedPad {
            vendor_product: None,
            id: "Pad".into(),
            index: 0,
            standard: true,
        }],
    );
    let pad = DeviceId::Gamepad("Pad".into());
    assert_eq!(b.lane_of(&pad, "button:14"), Some(0));
    assert_eq!(b.lane_of(&pad, "button:5"), Some(0));
    // B is a lane's, not Back, on the controller bound: it moves.
    r.row("pad.0.lane.0");
    r.h.key("Enter", "Enter");
    r.h.run(0.2);
    press(&mut r, "button:1");
    assert_eq!(
        table()[0],
        ["button:4", "button:14", "button:2", "button:5", "button:1"]
    );
    assert!(table()[3].iter().all(|c| c != "button:1"));
    assert!(
        r.state
            .borrow()
            .notice
            .ends_with("moved from Right to Left."),
        "{}",
        r.state.borrow().notice
    );
    // Its Back cancels, nothing changed.
    r.row("pad.0.lane.1");
    r.h.key("Enter", "Enter");
    r.h.run(0.2);
    assert_eq!(r.top(), "key");
    press(&mut r, "button:8");
    assert_ne!(r.top(), "key");
    assert_eq!(table()[1], ["button:6", "button:13", "button:0"]);
    // Clear empties the lane.
    r.h.key("ArrowRight", "ArrowRight");
    r.h.key("Enter", "Enter");
    r.h.run(0.2);
    assert!(table()[1].is_empty());
    assert_eq!(table()[0].len(), 5);
}

#[test]
fn controller_capture_can_bind_face_buttons_skip_and_redo() {
    fn press(h: &mut Harness, n: u16) {
        let control = format!("button:{n}");
        for down in [true, false] {
            h.controller(&Default::default(), "Pad", true, &control, down);
        }
        h.run(0.02);
    }
    for (size, design) in [
        ([1280.0, 720.0], DESIGN),
        ([844.0, 390.0], super::COMPACT),
        ([390.0, 844.0], super::COMPACT),
    ] {
        let mut r = rig(size, design);
        r.section("Controls");
        r.row("pad.0");
        r.h.key("Enter", "Enter");
        r.h.run(0.3);
        for n in [2, 0, 3, 1] {
            press(&mut r.h, 0); // arm; the release must never become a binding
            press(&mut r.h, n);
            assert_eq!(r.top(), "bind", "East is capturable");
        }
        assert!(r.state.borrow().status.starts_with("bind step 4"));
        for _ in 0..2 {
            press(&mut r.h, 13);
            assert_eq!(r.h.ui.focused(), Some("Skip"));
            press(&mut r.h, 0);
        }
        assert!(r.state.borrow().status.starts_with("bind review"));
        press(&mut r.h, 13);
        assert_eq!(r.h.ui.focused(), Some("Redo"));
        press(&mut r.h, 0);
        assert!(r.state.borrow().status.starts_with("bind step 0"));
        press(&mut r.h, 1);
        r.h.run(0.3);
        assert_ne!(r.top(), "bind");
        assert!(
            stored().controls.pads.is_empty(),
            "cancelled capture saves nothing"
        );
    }
}
