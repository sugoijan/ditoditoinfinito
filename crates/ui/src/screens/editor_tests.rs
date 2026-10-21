use std::cell::{Cell, RefCell};
use std::rc::Rc;

use ddi_chart::{NoteKind, Tick};

use super::*;
use crate::look::Plain;
use crate::screens::{COMPACT, DESIGN};
use crate::testing::Harness;
use crate::{Modifiers, PointerKind, TextMode};

#[derive(Default)]
struct Host {
    calls: RefCell<Vec<EditorCall>>,
    time: Cell<Option<f64>>,
    /// Saves fail with this.
    fail: RefCell<Option<String>>,
}

impl EditorHost for Host {
    fn call(&self, call: EditorCall, _editor: &Editor, cx: &mut Cx) {
        if let EditorCall::Save { revision } = &call {
            let m = match self.fail.borrow().clone() {
                Some(e) => Message::new(SAVED).with("Error", Param::Str(e)),
                None => Message::new(SAVED)
                    .with("Revision", Param::Int(revision.map_or(-1, |r| r as i64))),
            };
            cx.broadcast(m);
        }
        if let EditorCall::Listen { from } = &call {
            self.time.set(Some(*from));
        }
        if call == EditorCall::Stop {
            self.time.set(None);
        }
        self.calls.borrow_mut().push(call);
    }

    fn music_time(&self) -> Option<f64> {
        self.time.get()
    }
}

fn env() -> Env {
    Env::new(
        Rc::new(Plain),
        Rc::new(|s| ddi_library::search::Fold::nfkd(&ddi_library::search::Nfkd, s)),
    )
}

fn place() -> EditorPlace {
    EditorPlace {
        save: Some("Songs/Pack/Song/song.ssc".into()),
        overwrites: true,
        export: Some("Download".into()),
        listen: true,
        playtest: true,
    }
}

/// The three sizes the screens are tested at: the design, a phone
/// sideways, a phone upright.
const SIZES: [([f32; 2], [f32; 2]); 3] = [
    ([1280.0, 720.0], DESIGN),
    ([844.0, 390.0], COMPACT),
    ([390.0, 844.0], COMPACT),
];

struct Setup {
    h: Harness,
    env: Env,
    host: Rc<Host>,
    editor: Rc<RefCell<Editor>>,
}

fn setup(size: [f32; 2], design: [f32; 2]) -> Setup {
    let env = env();
    let host = Rc::new(Host::default());
    let mut h = Harness::new(design, size, 1.0, TextMode::Own);
    h.ui.set_design(design);
    let screen = EditorScreen::new(
        env.clone(),
        Editor::new_song(Some("song.ogg".into()), "Test"),
        place(),
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
    s.env
        .state
        .borrow()
        .editor
        .clone()
        .expect("the editor's view")
}

#[test]
fn it_fits_at_every_size() {
    for (size, design) in SIZES {
        let s = setup(size, design);
        assert_eq!(s.env.state.borrow().screen, "editor");
        let vp = s.h.ui.viewport().screen;
        for name in [
            "Field",
            "PanelPlate",
            "Button.Menu",
            "Button.Save",
            "Button.Play",
            "Cursor",
        ] {
            let [x0, y0, x1, y1] = s.h.ui.bounds(name).unwrap_or_else(|| panic!("{name}"));
            assert!(
                x0 >= -1.0 && y0 >= -1.0 && x1 <= vp.width + 1.0 && y1 <= vp.height + 1.0,
                "{name} at {size:?}: {:?} in {}x{}",
                [x0, y0, x1, y1],
                vp.width,
                vp.height
            );
        }
        // The panel says where the cursor is.
        let texts = s.h.texts();
        assert!(
            texts.iter().any(|t| t.starts_with("Beat 0")),
            "{size:?}: {texts:?}"
        );
        assert!(texts.iter().any(|t| t == "120 BPM"), "{size:?}: {texts:?}");
    }
}

#[test]
fn keys_place_notes_holds_and_undo() {
    let mut s = setup([1280.0, 720.0], DESIGN);
    s.h.key("Digit1", "1");
    s.h.key("ArrowDown", "ArrowDown");
    s.h.key("ArrowDown", "ArrowDown");
    s.h.key_down("Digit3", "3");
    for _ in 0..4 {
        s.h.key("ArrowDown", "ArrowDown");
    }
    s.h.key_up("Digit3", "3");
    s.h.run(0.3);
    {
        let e = s.editor.borrow();
        let notes = &e.chart().notes;
        assert_eq!(notes.len(), 2);
        assert_eq!(notes[0].kind, NoteKind::Tap);
        assert_eq!(notes[1].lane, 2);
        assert_eq!(
            notes[1].kind,
            NoteKind::HoldHead {
                end: Tick::from_beats(1) + Tick(24)
            },
            "16ths: the hold from beat 0.5 to 1.5"
        );
    }
    // Released: moving no longer grows it.
    s.h.key("ArrowDown", "ArrowDown");
    assert_eq!(s.editor.borrow().note_count(), 2);
    let v = view(&s);
    assert!(v.dirty && v.can_undo);
    let ctrl = Modifiers {
        ctrl: true,
        ..Modifiers::default()
    };
    s.h.key_with("KeyZ", "z", ctrl);
    assert_eq!(s.editor.borrow().note_count(), 1);
    s.h.key_with(
        "KeyZ",
        "Z",
        Modifiers {
            shift: true,
            ..ctrl
        },
    );
    assert_eq!(s.editor.borrow().note_count(), 2);
    // Snap and tool.
    s.h.key("ArrowRight", "ArrowRight");
    assert_eq!(view(&s).snap, 24);
    s.h.key("ArrowLeft", "ArrowLeft");
    s.h.key("ArrowLeft", "ArrowLeft");
    assert_eq!(view(&s).snap, 12);
    s.h.key("KeyN", "n");
    assert_eq!(view(&s).tool, "Roll");
    // Shift selects; Delete takes the selection's notes.
    s.h.key("Home", "Home");
    s.h.key_down("ShiftLeft", "Shift");
    s.h.key("PageDown", "PageDown");
    s.h.key_up("ShiftLeft", "Shift");
    assert_eq!(view(&s).selection, Some((0, 192)));
    s.h.key("Delete", "Delete");
    assert_eq!(s.editor.borrow().note_count(), 0);
}

#[test]
fn the_field_glides_unless_motion_is_reduced() {
    let mut s = setup([1280.0, 720.0], DESIGN);
    s.h.key("PageDown", "PageDown");
    s.h.frame(1.0 / 60.0);
    let shown = view(&s).shown;
    assert!(shown > 0.0 && shown < 192.0, "gliding: {shown}");
    s.h.run(0.5);
    assert_eq!(view(&s).shown, 192.0);
    s.env.reduced_motion.set(true);
    s.h.key("PageDown", "PageDown");
    s.h.frame(1.0 / 60.0);
    assert_eq!(view(&s).shown, 384.0, "jumps with reduced motion");
}

#[test]
fn listening_follows_the_music_and_stops_on_the_grid() {
    let mut s = setup([1280.0, 720.0], DESIGN);
    s.h.key("Space", " ");
    assert_eq!(
        s.host.calls.borrow().last(),
        Some(&EditorCall::Listen { from: 0.0 })
    );
    assert!(view(&s).listening);
    // 1.26 s at 120 BPM: beat 2.52.
    s.host.time.set(Some(1.26));
    s.h.frame(1.0 / 60.0);
    assert!((view(&s).shown - 2.52 * 48.0).abs() < 1e-6);
    s.h.key("Space", " ");
    assert_eq!(s.host.calls.borrow().last(), Some(&EditorCall::Stop));
    assert_eq!(view(&s).cursor, 120, "the 16th nearest beat 2.52");
    // A pad's Start does the same.
    s.h.pad(MenuAction::Start, true);
    s.h.pad(MenuAction::Start, false);
    assert!(view(&s).listening);
    s.host.time.set(None);
    s.h.frame(1.0 / 60.0);
    assert!(!view(&s).listening, "the music ended");
}

#[test]
fn playtests_and_saves_go_to_the_shell() {
    let mut s = setup([1280.0, 720.0], DESIGN);
    s.h.key("KeyP", "p");
    s.h.key_with(
        "KeyP",
        "P",
        Modifiers {
            shift: true,
            ..Modifiers::default()
        },
    );
    assert_eq!(
        s.host.calls.borrow()[..],
        [
            EditorCall::Playtest {
                from_cursor: true,
                auto: false
            },
            EditorCall::Playtest {
                from_cursor: true,
                auto: true
            }
        ]
    );
    s.h.key("Digit2", "2");
    // Saving over the file asks first; Cancel saves nothing.
    s.h.tap(PointerKind::Mouse, "Button.Save");
    s.h.run(0.2);
    assert!(
        s.h.texts()
            .iter()
            .any(|t| t == "Save over the song's file?")
    );
    s.h.tap(PointerKind::Mouse, "Cancel");
    s.h.run(0.2);
    assert!(view(&s).dirty);
    let cmd = Modifiers {
        meta: true,
        ..Modifiers::default()
    };
    s.h.key_with("KeyS", "s", cmd);
    s.h.run(0.2);
    s.h.tap(PointerKind::Mouse, "Confirm");
    s.h.run(0.2);
    assert!(!view(&s).dirty, "saved");
    assert_eq!(s.env.state.borrow().status, "Saved");
    // A save that fails says why and stays unsaved.
    *s.host.fail.borrow_mut() = Some("disk full".into());
    s.h.key("Digit3", "3");
    s.h.key_with("KeyS", "s", cmd);
    s.h.run(0.2);
    s.h.tap(PointerKind::Mouse, "Confirm");
    s.h.run(0.2);
    assert!(view(&s).dirty);
    assert_eq!(s.env.state.borrow().status, "Not saved: disk full");
}

#[test]
fn leaving_unsaved_asks() {
    let mut s = setup([844.0, 390.0], COMPACT);
    s.h.key("Escape", "Escape");
    s.h.run(0.3);
    assert_eq!(s.h.ui.layers().len(), 0, "nothing to save: it leaves");
    assert_eq!(s.host.calls.borrow().last(), Some(&EditorCall::Leave));
    assert!(s.env.state.borrow().editor.is_none());

    let mut s = setup([844.0, 390.0], COMPACT);
    s.h.key("Digit1", "1");
    s.h.key("Escape", "Escape");
    s.h.run(0.3);
    assert!(s.h.texts().iter().any(|t| t == "Leave without saving?"));
    s.h.key("Escape", "Escape");
    s.h.run(0.3);
    assert_eq!(s.h.ui.layers().last().unwrap().name, "editor");
    s.h.key("Escape", "Escape");
    s.h.run(0.3);
    s.h.tap(PointerKind::Touch, "Confirm");
    s.h.run(0.3);
    assert_eq!(s.h.ui.layers().len(), 0);
}

#[test]
fn the_menu_and_the_timing_dialog() {
    for (size, design) in SIZES {
        let mut s = setup(size, design);
        // Tab (a pad's Select) opens the menu; every item fits.
        s.h.key("Tab", "Tab");
        s.h.run(0.3);
        assert_eq!(s.h.ui.layers().last().unwrap().name, "editor-actions");
        let vp = s.h.ui.viewport().screen;
        for item in [
            "Item.play",
            "Item.timing",
            "Item.save",
            "Item.leave",
            "Close",
        ] {
            let [x0, y0, x1, y1] = s.h.ui.bounds(item).unwrap_or_else(|| panic!("{item}"));
            assert!(
                x0 >= 0.0 && y0 >= 0.0 && x1 <= vp.width && y1 <= vp.height,
                "{item} at {size:?}"
            );
        }
        s.h.tap(PointerKind::Touch, "Item.timing");
        s.h.run(0.3);
        assert!(
            s.h.texts().iter().any(|t| t == "BPM: 120"),
            "{size:?}: {:?}",
            s.h.texts()
        );
        // A BPM at the cursor, typed.
        s.h.tap(PointerKind::Touch, "Item.seg:bpm");
        s.h.run(0.3);
        assert_eq!(s.h.ui.layers().last().unwrap().name, "editor-value");
        for _ in 0..3 {
            s.h.key("Backspace", "Backspace");
        }
        type_text(&mut s.h, "150");
        s.h.key("Enter", "Enter");
        s.h.run(0.3);
        assert_eq!(s.h.ui.layers().last().unwrap().name, "editor");
        assert_eq!(s.editor.borrow().timing().bpms[0].bpm, 150.0, "{size:?}");
        assert_eq!(view(&s).segments, vec!["150 BPM".to_string()]);
        // What a file cannot hold is refused, saying why.
        s.h.key("KeyB", "b");
        s.h.run(0.3);
        for _ in 0..3 {
            s.h.key("Backspace", "Backspace");
        }
        type_text(&mut s.h, "0");
        s.h.key("Enter", "Enter");
        s.h.run(0.2);
        assert_eq!(s.h.ui.layers().last().unwrap().name, "editor-value");
        assert!(s.h.texts().iter().any(|t| t.contains("cannot be kept")));
        s.h.key("Escape", "Escape");
        s.h.run(0.3);
        assert_eq!(s.h.ui.layers().last().unwrap().name, "editor");
        assert_eq!(s.editor.borrow().timing().bpms[0].bpm, 150.0);
    }
}

#[test]
fn song_and_chart_fields() {
    let mut s = setup([1280.0, 720.0], DESIGN);
    s.h.key("KeyC", "c");
    s.h.run(0.3);
    s.h.tap(PointerKind::Mouse, "Item.add");
    s.h.run(0.3);
    s.h.tap(PointerKind::Mouse, "Item.add:pump-single");
    s.h.run(0.3);
    let v = view(&s);
    assert_eq!(
        (v.charts, v.chart, v.layout.as_str()),
        (2, 1, "pump-single")
    );
    // Five lanes: the fifth key places, the sixth does nothing.
    s.h.key("Digit5", "5");
    s.h.key("ArrowDown", "ArrowDown");
    s.h.key("Digit6", "6");
    assert_eq!(view(&s).notes, 1);
    s.h.key("KeyC", "c");
    s.h.run(0.3);
    s.h.tap(PointerKind::Mouse, "Item.meter");
    s.h.run(0.3);
    s.h.key("Backspace", "Backspace");
    type_text(&mut s.h, "9");
    s.h.key("Enter", "Enter");
    s.h.run(0.3);
    assert_eq!(s.editor.borrow().chart().meter, 9);
    s.h.key("KeyI", "i");
    s.h.run(0.3);
    s.h.tap(PointerKind::Mouse, "Item.artist");
    s.h.run(0.3);
    type_text(&mut s.h, "Ab");
    s.h.key("Enter", "Enter");
    s.h.run(0.3);
    assert_eq!(s.editor.borrow().song().artist, "Ab");
}

#[test]
fn pointers_place_notes_and_holds() {
    for (size, design) in SIZES {
        let mut s = setup(size, design);
        let g = s.h.ui.bounds("Cursor").unwrap();
        let line_y = (g[1] + g[3]) / 2.0;
        let [lx0, _, lx1, _] = g;
        // The cursor line spans the lanes and 8 px either side.
        let z = zoom(&s);
        let lane_w = (lx1 - lx0 - 16.0 * z) / 4.0;
        let lane = |i: f32| lx0 + 8.0 * z + lane_w * (i + 0.5);
        s.h.tap_at(PointerKind::Touch, [lane(1.0), line_y]);
        s.h.run(0.2);
        assert_eq!(s.editor.borrow().note_count(), 1, "{size:?}");
        assert_eq!(s.editor.borrow().chart().notes[0].lane, 1);
        // A drag down along a lane: a hold.
        let beat = beat_height(&s);
        s.h.drag(
            PointerKind::Mouse,
            [lane(3.0), line_y + beat],
            [lane(3.0), line_y + 3.0 * beat],
            0.2,
            6,
        );
        s.h.run(0.3);
        let e = s.editor.borrow();
        let hold = e
            .chart()
            .notes
            .iter()
            .find(|n| n.lane == 3)
            .expect("a hold");
        assert_eq!(hold.tick, Tick::from_beats(1), "{size:?}");
        assert_eq!(
            hold.kind,
            NoteKind::HoldHead {
                end: Tick::from_beats(3)
            }
        );
    }
}

#[test]
fn song_select_opens_the_editor_with_e() {
    use crate::screens::select::{SelectCall, SelectScreen, SharedFeed};
    let env = env();
    let calls = Rc::new(RefCell::new(Vec::new()));
    let select = |editing: bool| {
        let sink = calls.clone();
        let s = SelectScreen::new(
            env.clone(),
            crate::screens::sample::select(),
            Some("some-things-must"),
            Rc::new(|_, _, _, _| {}),
            Rc::new(|_, _| {}),
        )
        .with_shell(
            Rc::new(move |c, _| {
                if matches!(c, SelectCall::Edit { .. }) {
                    sink.borrow_mut().push(c);
                }
            }),
            SharedFeed::default(),
        );
        if editing { s.with_editing() } else { s }
    };
    let mut h = Harness::new(DESIGN, [1280.0, 720.0], 1.0, TextMode::Mirror);
    h.push(select(false));
    h.run(0.5);
    h.key("KeyE", "e");
    assert!(calls.borrow().is_empty(), "not offered: nothing");
    assert!(!env.state.borrow().hints.iter().any(|t| t == "E Edit"));
    let mut h = Harness::new(DESIGN, [1280.0, 720.0], 1.0, TextMode::Mirror);
    h.push(select(true));
    h.run(0.5);
    assert!(env.state.borrow().hints.iter().any(|t| t == "E Edit"));
    h.key("KeyE", "e");
    let id = env.state.borrow().song_id.clone();
    match &calls.borrow()[..] {
        [SelectCall::Edit { song, .. }] => assert_eq!(*song, id),
        other => panic!("{other:?}"),
    }
}

/// Typed into the field being typed into (desktop's way: committed
/// text).
fn type_text(h: &mut Harness, text: &str) {
    h.send(crate::UiEvent::Text(crate::event::TextEvent::Commit(
        text.into(),
    )));
}

fn zoom(s: &Setup) -> f32 {
    s.h.ui.viewport().screen.width / {
        let vp = s.h.ui.viewport().screen;
        super::super::narrow_fit(&s.env, vp).0[0]
    }
}

/// A beat's height on the screen (virtual pixels), from the measure
/// lines drawn.
fn beat_height(s: &Setup) -> f32 {
    let compact = s.env.state.borrow().compact;
    (if compact { 56.0 } else { 88.0 }) * zoom(s)
}
