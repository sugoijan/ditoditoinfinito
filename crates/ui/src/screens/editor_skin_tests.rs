//! The editor screen with a shell that draws the field in the skin: what
//! it is told to draw ([`FieldPicture`]) and the overlay over it, the
//! two views, the rows found under a pointer, the zoom at dense snaps
//! and the compact field.

use std::cell::{Cell, RefCell};
use std::rc::Rc;

use ddi_chart::Tick;
use ddi_engine::scroll::{ScrollOptions, SpeedMod};

use super::*;
use crate::look::Plain;
use crate::screen::Cx;
use crate::screens::DESIGN;
use crate::screens::Env;
use crate::screens::editor::{EditorCall, EditorHost, EditorPlace, EditorScreen, EditorView};
use crate::state::editor::Editor;
use crate::testing::Harness;
use crate::{PointerKind, TextMode};

/// A shell drawing the skin: the pictures it was given.
struct Host {
    scroll: ScrollOptions,
    pictures: RefCell<Vec<Option<FieldPicture>>>,
    time: Cell<Option<f64>>,
}

impl EditorHost for Host {
    fn call(&self, call: EditorCall, _editor: &Editor, _cx: &mut Cx) {
        match call {
            EditorCall::Listen { from } => self.time.set(Some(from)),
            EditorCall::Stop => self.time.set(None),
            _ => {}
        }
    }

    fn music_time(&self) -> Option<f64> {
        self.time.get()
    }

    fn skin_field(&self) -> Option<SkinField> {
        Some(SkinField {
            scroll: self.scroll,
        })
    }

    fn show_field(&self, picture: Option<&FieldPicture>, _editor: &Editor) {
        self.pictures.borrow_mut().push(picture.cloned());
    }
}

impl Host {
    fn last(&self) -> Option<FieldPicture> {
        self.pictures.borrow().last().cloned().flatten()
    }
}

struct Setup {
    h: Harness,
    env: Env,
    host: Rc<Host>,
    editor: Rc<RefCell<Editor>>,
}

/// The editor at 1280×720 on a song at 120 BPM going to 240 at beat 4,
/// a half-second stop at beat 6.
fn setup(scroll: ScrollOptions) -> Setup {
    let env = Env::new(
        Rc::new(Plain),
        Rc::new(|s| ddi_library::search::Fold::nfkd(&ddi_library::search::Nfkd, s)),
    );
    env.reduced_motion.set(true);
    let host = Rc::new(Host {
        scroll,
        pictures: RefCell::default(),
        time: Cell::new(None),
    });
    let mut h = Harness::new(DESIGN, [1280.0, 720.0], 1.0, TextMode::Own);
    let mut editor = Editor::new_song(Some("song.ogg".into()), "Test");
    editor.set_segment(
        Tick::from_beats(4),
        crate::state::editor::Segment::Bpm(240.0),
        crate::state::editor::Retime::KeepBeats,
    );
    editor.set_segment(
        Tick::from_beats(6),
        crate::state::editor::Segment::Stop(0.5),
        crate::state::editor::Retime::KeepBeats,
    );
    let screen = EditorScreen::new(
        env.clone(),
        editor,
        EditorPlace {
            listen: true,
            ..EditorPlace::default()
        },
    )
    .with_host(host.clone());
    let editor = screen.editor();
    editor.borrow_mut().mark_saved();
    h.push(screen);
    h.run(0.3);
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

/// The screen's y (virtual pixels, one to a physical one here) of the
/// picture's texture's `y`.
fn screen_y(p: &FieldPicture, y: f32) -> f32 {
    p.rect[1] + y
}

/// Every note's overlay tick at the picture's row of that note (those
/// whose row is on the field: the picture holds the notes a lane's width
/// beyond too, whose art reaches in).
fn rows_agree(s: &Setup, what: &str) {
    let p = s.host.last().expect("a picture");
    let on_field = |n: &&ddi_engine::frame::NoteSprite| (0.0..=p.rect[3]).contains(&p.y(n.y));
    assert!(
        p.frame.notes.iter().filter(on_field).count() > 1,
        "{what}: notes in view"
    );
    for n in p.frame.notes.iter().filter(on_field) {
        let b =
            s.h.ui
                .bounds(&format!("Row.{}", n.note_index))
                .unwrap_or_else(|| panic!("{what}: note {}'s tick", n.note_index));
        let tick_y = (b[1] + b[3]) / 2.0;
        let row = screen_y(&p, p.y(n.y));
        assert!(
            (tick_y - row).abs() < 0.6,
            "{what}: note {} drawn at {row}, its tick at {tick_y}",
            n.note_index
        );
        // In its lane: the tick at the lane's left edge.
        let lane_left = p.rect[0] + p.left + f32::from(n.lane) * p.arrow;
        assert!(
            (b[0] - lane_left).abs() < 0.6,
            "{what}: {} {lane_left}",
            b[0]
        );
    }
    let c = s.h.ui.bounds("Cursor").expect("the cursor line");
    let cursor = s.editor.borrow().cursor();
    let at_cursor = p
        .frame
        .notes
        .iter()
        .find(|n| s.editor.borrow().chart().notes[n.note_index].tick == cursor);
    if let Some(n) = at_cursor {
        assert!(((c[1] + c[3]) / 2.0 - screen_y(&p, p.y(n.y))).abs() < 0.6);
    }
}

/// Places notes on beats 0, 1, 4.5, 5, 6 and 7 in lanes by turn.
fn place(s: &mut Setup) {
    let ticks = [0, 48, 216, 240, 288, 336];
    for (i, t) in ticks.iter().enumerate() {
        s.editor.borrow_mut().set_cursor(Tick(*t), false);
        s.h.key(&format!("Digit{}", i % 4 + 1), "1");
    }
    s.editor.borrow_mut().set_cursor(Tick(48), false);
    s.h.key("ArrowDown", "ArrowDown");
    s.h.key("ArrowUp", "ArrowUp");
    s.h.run(0.1);
    assert_eq!(s.editor.borrow().note_count(), 6);
}

#[test]
fn the_overlay_marks_the_rows_the_skin_draws() {
    let scroll = ScrollOptions {
        speed: SpeedMod::CMod(150.0),
        ..Default::default()
    };
    let mut s = setup(scroll);
    place(&mut s);
    let v = view(&s);
    assert_eq!(v.view, "edit");
    assert!(!v.plates);
    let p = s.host.last().expect("a picture");
    assert_eq!(p.layout, "dance-single");
    assert!(!p.running && !p.reverse);
    // The edit view: a beat the same height everywhere, BPM change or
    // stop.
    let beat = |t: i64| {
        p.frame
            .notes
            .iter()
            .find(|n| s.editor.borrow().chart().notes[n.note_index].tick.0 == t)
            .map(|n| n.y)
    };
    // Beats 1 and 5, a change to 240 BPM between them: four beats.
    let (a, b) = (beat(48).unwrap(), beat(240).unwrap());
    assert!(
        ((b - a) * p.arrow - 4.0 * v.beat_h).abs() < 0.01,
        "{a} {b} {}",
        v.beat_h
    );
    rows_agree(&s, "edit");
    // The play view: where play has them (the speed, the tempo doubled).
    s.h.key("KeyV", "v");
    s.h.run(0.1);
    let v = view(&s);
    assert_eq!(v.view, "play");
    let p = s.host.last().expect("a picture");
    let timing = s.editor.borrow().timing().clone();
    let cursor = s.editor.borrow().cursor();
    for n in &p.frame.notes {
        let tick = s.editor.borrow().chart().notes[n.note_index].tick;
        let want =
            ddi_engine::scroll::note_y(scroll, &timing, tick, timing.seconds_before_delay(cursor));
        assert!((n.y - want).abs() < 1e-4, "{tick:?}: {} {want}", n.y);
    }
    rows_agree(&s, "play");
    assert!(
        s.h.texts()
            .iter()
            .any(|t| t.starts_with("Play view · C150")),
        "{:?}",
        s.h.texts()
    );
    // A tap on a drawn row in a lane places a note there.
    let before = s.editor.borrow().note_count();
    let row = |t: i64| {
        let m = p
            .frame
            .notes
            .iter()
            .find(|n| s.editor.borrow().chart().notes[n.note_index].tick.0 == t)
            .expect("drawn");
        screen_y(&p, p.y(m.y))
    };
    let lane_x = |l: f32| p.rect[0] + p.lane_x(l);
    s.h.tap_at(PointerKind::Mouse, [lane_x(0.0), row(240)]);
    s.h.run(0.1);
    let e = s.editor.borrow();
    assert_eq!(e.note_count(), before + 1);
    assert!(
        e.chart()
            .notes
            .iter()
            .any(|n| n.lane == 0 && n.tick == Tick(240)),
        "placed on the row under the pointer"
    );
}

#[test]
fn reverse_puts_the_line_at_the_bottom_in_the_play_view() {
    let mut s = setup(ScrollOptions {
        speed: SpeedMod::XMod(1.0),
        reverse: true,
        ..Default::default()
    });
    place(&mut s);
    let edit = s.host.last().unwrap();
    assert!(!edit.reverse, "the edit view keeps later notes below");
    s.h.key("KeyV", "v");
    s.h.run(0.1);
    let p = s.host.last().unwrap();
    assert!(p.reverse);
    assert!(p.receptor_y > p.rect[3] / 2.0, "{}", p.receptor_y);
    rows_agree(&s, "reverse");
    // Later notes above the line.
    let later = p.frame.notes.iter().filter(|n| n.y > 0.0).count();
    assert!(later > 0);
    assert!(
        p.frame
            .notes
            .iter()
            .filter(|n| n.y > 0.0)
            .all(|n| p.y(n.y) < p.receptor_y)
    );
}

#[test]
fn dense_snaps_raise_the_zoom_and_the_compact_field_does_not() {
    let mut s = setup(ScrollOptions::default());
    // The 16th's rows: a third of a 72-pixel note is 24, a beat 96.
    assert_eq!(view(&s).beat_h, 96.0);
    let base = 88.0;
    // To the 192nd: each row a third of a note apart.
    for _ in 0..8 {
        s.h.key("ArrowRight", "ArrowRight");
    }
    s.h.run(0.1);
    let v = view(&s);
    assert_eq!(v.snap, 192);
    let p = s.host.last().unwrap();
    let lane = p.arrow;
    assert!(
        v.beat_h / 48.0 >= lane / 3.0 - 0.01,
        "rows {} apart, notes {lane}",
        v.beat_h / 48.0
    );
    assert!(v.beat_h > base);
    // The compact field: the shell draws nothing, plates at the zoom.
    s.h.key("KeyF", "f");
    s.h.run(0.1);
    let v = view(&s);
    assert!(v.plates);
    assert_eq!(s.host.pictures.borrow().last(), Some(&None));
    assert_eq!(v.beat_h, base, "the plates keep the zoom");
    assert!(s.h.ui.bounds("Row.0").is_none());
    s.h.key("KeyF", "f");
    s.h.run(0.1);
    assert!(!view(&s).plates);
    assert!(s.host.last().is_some());
}

#[test]
fn listening_runs_the_skin_and_passes_notes() {
    let mut s = setup(ScrollOptions::default());
    place(&mut s);
    s.editor.borrow_mut().set_cursor(Tick::ZERO, false);
    s.h.run(0.1);
    s.h.key("Space", " ");
    s.host.time.set(Some(0.6));
    s.h.frame(1.0 / 60.0);
    let p = s.host.last().unwrap();
    assert!(p.running);
    // Beats 0 and 1 passed: their taps hit.
    let hits = p
        .events
        .iter()
        .filter(|e| matches!(e.kind, ddi_engine::frame::SkinEventKind::Step(_)))
        .count();
    assert_eq!(hits, 2, "{:?}", p.events);
    s.h.key("Space", " ");
    s.h.run(0.1);
    let p = s.host.last().unwrap();
    assert!(!p.running && p.events.is_empty());
}

#[test]
fn a_shell_without_the_skin_gets_plates() {
    // The harness's own host draws no skin: no pictures, plates.
    struct Plain2;
    impl EditorHost for Plain2 {
        fn call(&self, _: EditorCall, _: &Editor, _: &mut Cx) {}
    }
    let env = Env::new(
        Rc::new(Plain),
        Rc::new(|s| ddi_library::search::Fold::nfkd(&ddi_library::search::Nfkd, s)),
    );
    env.reduced_motion.set(true);
    let mut h = Harness::new(DESIGN, [1280.0, 720.0], 1.0, TextMode::Own);
    h.push(
        EditorScreen::new(
            env.clone(),
            Editor::new_song(None, "T"),
            EditorPlace::default(),
        )
        .with_host(Rc::new(Plain2)),
    );
    h.run(0.5);
    let v = env.state.borrow().editor.clone().unwrap();
    assert!(v.plates);
    // F does nothing there; V still changes the view.
    h.key("KeyF", "f");
    h.key("KeyV", "v");
    h.run(0.1);
    let v = env.state.borrow().editor.clone().unwrap();
    assert!(v.plates && v.view == "play", "{v:?}");
}

#[test]
fn leaving_takes_the_picture_away() {
    let mut s = setup(ScrollOptions::default());
    assert!(s.host.last().is_some());
    s.h.key("Escape", "Escape");
    s.h.run(0.3);
    assert_eq!(s.host.pictures.borrow().last(), Some(&None));
}
