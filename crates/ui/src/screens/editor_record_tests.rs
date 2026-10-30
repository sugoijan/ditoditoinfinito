//! Recording on the screen (`editor_record.rs`).

use std::cell::{Cell, RefCell};

use ddi_chart::{NoteKind, Tick};
use ddi_engine::input::{BindingDevice, Bindings};
use ddi_platform::gamepad::standard;
use ddi_platform::{DeviceId, HostTime, RawInput};

use super::*;
use crate::UiEvent;
use crate::look::Plain;
use crate::screens::{COMPACT, DESIGN};
use crate::testing::Harness;
use crate::{Modifiers, PointerKind, TextMode};

/// Host seconds less this are the song second heard (a fake clock: no
/// latency, no offset).
const CLOCK: f64 = 100.0;

#[derive(Default)]
struct Host {
    calls: RefCell<Vec<EditorCall>>,
    time: Cell<Option<f64>>,
}

impl EditorHost for Host {
    fn call(&self, call: EditorCall, _editor: &Editor, _cx: &mut Cx) {
        match &call {
            EditorCall::Listen { from } | EditorCall::Record { from, .. } => {
                self.time.set(Some(*from))
            }
            EditorCall::Stop => self.time.set(None),
            _ => {}
        }
        self.calls.borrow_mut().push(call);
    }

    fn music_time(&self) -> Option<f64> {
        self.time.get()
    }

    fn heard_at(&self, host: f64) -> Option<f64> {
        self.time.get().map(|_| host - CLOCK)
    }

    fn record_input(&self, _layout: &ddi_chart::Layout) -> Option<RecordInput> {
        // The arrows, and a controller's d-pad left on the first lane.
        let mut lanes = Bindings::default();
        for (lane, code) in ["ArrowLeft", "ArrowDown", "ArrowUp", "ArrowRight"]
            .iter()
            .enumerate()
        {
            lanes.bind(BindingDevice::Keyboard, code, lane as u8);
        }
        lanes.bind(
            BindingDevice::Gamepad("pad".into()),
            &format!("button:{}", standard::DPAD_LEFT),
            0,
        );
        Some(RecordInput {
            lanes,
            controls: Default::default(),
            standard: vec!["pad".into()],
        })
    }
}

struct Setup {
    h: Harness,
    env: Env,
    host: Rc<Host>,
    editor: Rc<RefCell<Editor>>,
}

/// The three sizes the screens are tested at.
const SIZES: [([f32; 2], [f32; 2]); 3] = [
    ([1280.0, 720.0], DESIGN),
    ([844.0, 390.0], COMPACT),
    ([390.0, 844.0], COMPACT),
];

fn setup(size: [f32; 2], design: [f32; 2]) -> Setup {
    let env = Env::new(
        Rc::new(Plain),
        Rc::new(|s| ddi_library::search::Fold::nfkd(&ddi_library::search::Nfkd, s)),
    );
    let host = Rc::new(Host::default());
    let mut h = Harness::new(design, size, 1.0, TextMode::Own);
    h.ui.set_design(design);
    let place = EditorPlace {
        listen: true,
        playtest: true,
        ..EditorPlace::default()
    };
    // 120 BPM from 0 s: a beat each half second.
    let screen = EditorScreen::new(
        env.clone(),
        Editor::new_song(Some("song.ogg".into()), "Test"),
        place,
    )
    .with_host(host.clone());
    let editor = screen.editor();
    editor.borrow_mut().mark_saved();
    h.push(screen);
    h.run(0.4);
    Setup {
        h,
        env,
        host,
        editor,
    }
}

fn view(s: &Setup) -> EditorView {
    s.env.state.borrow().editor.clone().expect("the view")
}

fn beat(b: f64) -> i64 {
    (b * 48.0).round() as i64
}

impl Setup {
    /// The music heard at song second `at`, a frame drawn.
    fn heard(&mut self, at: f64) {
        self.host.time.set(Some(at));
        self.h.now = (CLOCK + at) * 1000.0;
        self.h.frame(1.0 / 60.0);
    }

    /// Key `code` pressed (`down`) or let go, stamped at song second `at`.
    fn key_at(&mut self, code: &str, down: bool, at: f64) {
        self.h.now = (CLOCK + at) * 1000.0;
        if down {
            self.h.key_down(code, code);
        } else {
            self.h.key_up(code, code);
        }
    }

    fn tap_key(&mut self, code: &str, at: f64) {
        self.key_at(code, true, at);
        self.key_at(code, false, at + 0.08);
    }

    /// The controller's `button` pressed or let go at song second `at`.
    fn pad_at(&mut self, button: u16, down: bool, at: f64) {
        self.h.send(UiEvent::Pad(RawInput {
            device: DeviceId::Gamepad("pad".into()),
            control: format!("button:{button}"),
            pressed: down,
            host_time: HostTime(CLOCK + at),
            slot: 0,
        }));
    }

    fn notes(&self) -> Vec<(i64, u8, NoteKind)> {
        self.editor
            .borrow()
            .chart()
            .notes
            .iter()
            .map(|n| (n.tick.0, n.lane, n.kind))
            .collect()
    }

    fn records(&self) -> Vec<(f64, f64)> {
        self.host
            .calls
            .borrow()
            .iter()
            .filter_map(|c| match c {
                EditorCall::Record { from, count_in } => Some((*from, *count_in)),
                _ => None,
            })
            .collect()
    }
}

#[test]
fn stamped_presses_on_the_play_keys_become_notes_on_the_grid() {
    for (size, design) in SIZES {
        let mut s = setup(size, design);
        // At beat 4 (a measure down), 8ths.
        s.h.key("PageDown", "PageDown");
        s.h.key("ArrowLeft", "ArrowLeft");
        s.h.key("ArrowLeft", "ArrowLeft");
        assert_eq!(view(&s).snap, 8);
        assert_eq!(view(&s).cursor, beat(4.0));
        s.h.key("KeyR", "r");
        // The music from a measure before (two seconds), clicking until
        // beat 4's second.
        assert_eq!(s.records(), vec![(0.0, 2.0)], "{size:?}");
        assert!(s.h.ui.wants_raw());
        let v = view(&s);
        assert!(v.listening);
        let r = v.record.expect("recording");
        assert!(r.active);
        assert_eq!((r.mode.as_str(), r.start), ("replace", beat(4.0)));
        s.heard(1.0);
        // In the pre-roll: ignored.
        s.tap_key("ArrowUp", 1.5);
        s.heard(2.0);
        // Beat 4 a little late, 4.5 and 5 early; the right arrow held
        // from beat 5.5 to beat 7.
        s.tap_key("ArrowLeft", 2.02);
        s.tap_key("ArrowDown", 2.25 - 0.03);
        s.tap_key("ArrowUp", 2.5 - 0.006);
        s.key_at("ArrowRight", true, 2.75);
        s.heard(3.2);
        let v = view(&s);
        assert_eq!(v.cursor, beat(4.0), "the arrows are lanes: no move");
        assert_eq!(v.snap, 8, "nor a snap change");
        assert_eq!(v.record.as_ref().unwrap().notes, 4);
        // Shown tinted while recording: the hold growing.
        assert!(s.h.ui.bounds("Record.3").is_some());
        s.key_at("ArrowRight", false, 3.5);
        s.heard(4.1);
        assert!(s.editor.borrow().chart().notes.is_empty(), "not yet");
        // Escape stops at its second (beat 8.2: the row nearest, 8).
        s.key_at("Escape", true, 4.1);
        s.key_at("Escape", false, 4.15);
        s.h.run(0.1);
        assert!(!s.h.ui.wants_raw());
        let v = view(&s);
        assert!(!v.listening);
        assert_eq!(s.h.ui.layers().len(), 1, "the editor stays");
        assert_eq!(
            s.notes(),
            vec![
                (beat(4.0), 0, NoteKind::Tap),
                (beat(4.5), 1, NoteKind::Tap),
                (beat(5.0), 2, NoteKind::Tap),
                (
                    beat(5.5),
                    3,
                    NoteKind::HoldHead {
                        end: Tick(beat(7.0))
                    }
                ),
            ]
        );
        assert_eq!(v.cursor, beat(8.0));
        let r = v.record.expect("the last take");
        assert!(!r.active);
        assert_eq!((r.notes, r.presses), (4, 4));
        assert!((r.mean_ms.unwrap() + 4.0).abs() < 1e-6);
        let status = s.env.state.borrow().status.clone();
        assert!(
            status.starts_with("4 notes recorded · 4 ms early on average, ±"),
            "{status}"
        );
        // One undo takes the take back.
        let cmd = Modifiers {
            meta: true,
            ..Modifiers::default()
        };
        s.h.key_with("KeyZ", "z", cmd);
        assert!(s.notes().is_empty());
        assert!(!view(&s).dirty);
    }
}

#[test]
fn shift_r_adds_a_controller_plays_and_stops_and_a_tap_stops() {
    let mut s = setup([1280.0, 720.0], DESIGN);
    s.h.key("ArrowLeft", "ArrowLeft");
    s.h.key("ArrowLeft", "ArrowLeft");
    // A note there already on beat 1's first lane.
    s.editor.borrow_mut().toggle_at(0, Tick(beat(1.0)));
    s.h.key_with(
        "KeyR",
        "R",
        Modifiers {
            shift: true,
            ..Modifiers::default()
        },
    );
    assert_eq!(view(&s).record.unwrap().mode, "add");
    assert_eq!(s.records(), vec![(0.0, 0.0)], "from the song's start");
    s.heard(0.4);
    // The controller's d-pad left on beat 1 (kept as it was) and 1.5;
    // the keyboard's second lane on beat 1.
    s.pad_at(standard::DPAD_LEFT, true, 0.5);
    s.pad_at(standard::DPAD_LEFT, false, 0.55);
    s.pad_at(standard::DPAD_LEFT, true, 0.75);
    s.pad_at(standard::DPAD_LEFT, false, 0.8);
    s.tap_key("ArrowDown", 0.51);
    s.heard(1.0);
    // Its B is Back: it stops.
    s.pad_at(standard::B, true, 1.0);
    s.h.run(0.1);
    assert!(!view(&s).listening);
    assert_eq!(
        s.notes(),
        vec![
            (beat(1.0), 0, NoteKind::Tap),
            (beat(1.0), 1, NoteKind::Tap),
            (beat(1.5), 0, NoteKind::Tap),
        ]
    );
    let r = view(&s).record.unwrap();
    assert_eq!((r.notes, r.presses), (2, 3));
    // Again, stopped by a tap on the field; then by R.
    s.h.key("KeyR", "r");
    s.heard(0.5);
    assert!(view(&s).listening);
    s.h.tap(PointerKind::Mouse, "FieldLayer");
    assert!(!view(&s).listening);
    s.h.key("KeyR", "r");
    s.heard(0.5);
    s.tap_key("KeyR", 0.6);
    s.h.run(0.1);
    assert!(!view(&s).listening);
    // Other editor keys do nothing while recording.
    s.h.key("KeyR", "r");
    s.heard(0.5);
    let before = view(&s);
    for code in ["KeyN", "KeyV", "Digit1", "PageDown", "KeyK"] {
        s.tap_key(code, 0.6);
    }
    let after = view(&s);
    assert_eq!(
        (after.tool, after.view, after.cursor, after.metronome),
        (before.tool, before.view, before.cursor, before.metronome)
    );
    assert!(after.listening);
}

#[test]
fn a_selection_is_recorded_alone_and_stops_at_its_end() {
    let mut s = setup([1280.0, 720.0], DESIGN);
    s.h.key("ArrowLeft", "ArrowLeft");
    for b in 0..8 {
        s.editor.borrow_mut().toggle_at(3, Tick(beat(f64::from(b))));
    }
    s.editor
        .borrow_mut()
        .select(Tick(beat(2.0)), Tick(beat(4.0)));
    s.h.key("KeyR", "r");
    assert_eq!(s.records(), vec![(0.0, 1.0)]);
    s.heard(1.0);
    s.tap_key("ArrowLeft", 1.5);
    s.tap_key("ArrowLeft", 2.5);
    // 12ths: past its last row by half a snap (a sixth of a beat), over.
    s.heard(2.05);
    assert!(view(&s).listening);
    s.heard(2.09);
    assert!(!view(&s).listening);
    let notes = s.notes();
    assert!(notes.contains(&(beat(3.0), 0, NoteKind::Tap)));
    assert!(!notes.iter().any(|n| n.1 == 0 && n.0 == beat(5.0)));
    for b in 0..8 {
        let kept = !(2..=4).contains(&b);
        assert_eq!(
            notes.iter().any(|n| n.1 == 3 && n.0 == beat(f64::from(b))),
            kept,
            "beat {b}"
        );
    }
    assert_eq!(view(&s).cursor, beat(4.0));
}

#[test]
fn the_menu_records_too() {
    let mut s = setup([844.0, 390.0], COMPACT);
    s.h.key("Tab", "Tab");
    s.h.run(0.3);
    assert!(
        s.h.texts().iter().any(|t| t == "Record from cursor (R)"),
        "{:?}",
        s.h.texts()
    );
    s.h.tap(PointerKind::Mouse, "Item.record-add");
    s.h.run(0.3);
    assert_eq!(view(&s).record.unwrap().mode, "add");
}
