//! The editor's sound tools on the screen (`editor_sound.rs`).

use std::cell::{Cell, RefCell};
use std::rc::Rc;

use ddi_chart::Tick;

use super::*;
use crate::look::Plain;
use crate::screens::{COMPACT, DESIGN};
use crate::state::editor::overview::Builder;
use crate::testing::Harness;
use crate::{Modifiers, PointerKind, TextMode};

#[derive(Default)]
struct Host {
    calls: RefCell<Vec<EditorCall>>,
    time: Cell<Option<f64>>,
    overview: RefCell<Option<Rc<Overview>>>,
    coming: Cell<bool>,
}

impl EditorHost for Host {
    fn call(&self, call: EditorCall, _editor: &Editor, _cx: &mut Cx) {
        match &call {
            EditorCall::Listen { from } => self.time.set(Some(*from)),
            EditorCall::Stop => self.time.set(None),
            _ => {}
        }
        self.calls.borrow_mut().push(call);
    }

    fn music_time(&self) -> Option<f64> {
        self.time.get()
    }

    fn overview(&self) -> Option<Rc<Overview>> {
        self.overview.borrow().clone()
    }

    fn overview_coming(&self) -> bool {
        self.coming.get()
    }
}

struct Setup {
    h: Harness,
    env: Env,
    host: Rc<Host>,
    editor: Rc<RefCell<Editor>>,
}

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

fn cues(s: &Setup) -> usize {
    s.host
        .calls
        .borrow()
        .iter()
        .filter(|c| **c == EditorCall::Cues)
        .count()
}

const SHIFT: Modifiers = Modifiers {
    shift: true,
    ctrl: false,
    alt: false,
    meta: false,
};

#[test]
fn the_metronome_and_the_clap_are_toggled_and_told_while_listening() {
    let mut s = setup([1280.0, 720.0], DESIGN);
    assert!(
        s.h.texts().iter().any(|t| t == "Metronome off · clap off"),
        "{:?}",
        s.h.texts()
    );
    s.h.key("KeyK", "k");
    s.h.key_with("KeyK", "K", SHIFT);
    s.h.frame(1.0 / 60.0);
    let v = view(&s);
    assert!(v.metronome && v.clap);
    assert!(s.h.texts().iter().any(|t| t == "Metronome on · clap on"));
    assert!(s.editor.borrow().sounds().metronome);
    assert!(!s.editor.borrow().is_dirty(), "not an edit");
    // Not listening: the shell reads them when asked to listen.
    s.h.run(0.2);
    assert_eq!(cues(&s), 0);
    s.h.key("Space", " ");
    s.h.run(0.2);
    assert_eq!(cues(&s), 0, "read with the listen");
    // Changed while listening: told once.
    s.h.key("KeyK", "k");
    s.h.run(0.2);
    assert_eq!(cues(&s), 1);
    assert!(!view(&s).metronome);
    // So is a note placed (the claps' rows).
    s.h.key("Digit1", "1");
    s.h.run(0.2);
    assert_eq!(cues(&s), 2);
    s.h.run(0.5);
    assert_eq!(cues(&s), 2);
    // A tap on the panel's line toggles the metronome; the menu has both.
    s.h.tap(PointerKind::Mouse, "Panel.Sounds");
    assert!(view(&s).metronome);
    s.h.key("Tab", "Tab");
    let items = s.env.state.borrow().page_items.clone();
    assert!(
        items.iter().any(|i| i == "Item.metronome") && items.iter().any(|i| i == "Item.clap"),
        "{items:?}"
    );
}

#[test]
fn taps_estimate_the_tempo_and_shift_y_applies_it_at_the_cursor() {
    let mut s = setup([1280.0, 720.0], DESIGN);
    // At 150 BPM: 400 ms apart, a few ms off.
    for (i, off) in [0.0, 3.0, -4.0, 2.0, 0.0, -2.0, 4.0, 1.0]
        .iter()
        .enumerate()
    {
        s.h.now = 10_000.0 + 400.0 * i as f64 + off;
        s.h.key("KeyY", "y");
    }
    let v = view(&s);
    assert_eq!(v.taps, 8);
    let bpm = v.tapped.expect("a tempo");
    assert!((bpm - 150.0).abs() < 0.5, "{bpm}");
    s.h.frame(1.0 / 60.0);
    assert!(
        s.h.texts()
            .iter()
            .any(|t| t.starts_with("Tapped 1") && t.ends_with("(8 taps)")),
        "{:?}",
        s.h.texts()
    );
    // Applied at beat 4 as a BPM change, undone in one step.
    s.h.key("PageDown", "PageDown");
    s.h.key_with("KeyY", "Y", SHIFT);
    {
        let e = s.editor.borrow();
        let t = e.timing();
        assert_eq!(t.bpms.len(), 2);
        assert_eq!(t.bpms[1].tick, Tick::from_beats(4));
        assert_eq!(t.bpms[1].bpm, (bpm * 100.0).round() / 100.0);
        assert_eq!(t.offset_seconds, 0.0, "no offset without the music");
    }
    let ctrl = Modifiers {
        ctrl: true,
        ..Modifiers::default()
    };
    s.h.key_with("KeyZ", "z", ctrl);
    assert_eq!(s.editor.borrow().timing().bpms.len(), 1);
    // A pause: the taps start again.
    s.h.now += 3_000.0;
    s.h.key("KeyY", "y");
    assert_eq!(view(&s).taps, 1);
    assert_eq!(view(&s).tapped, None);
}

#[test]
fn taps_while_listening_set_the_offset_too() {
    let mut s = setup([1280.0, 720.0], DESIGN);
    s.h.key("Space", " ");
    // The music's beats at 0.2 + n·0.5 s (120 BPM, 0.2 s after the
    // song's grid): tapped on them, the music's second read as each key
    // is handled.
    for i in 0..10 {
        let at = 0.2 + 0.5 * f64::from(i);
        s.host.time.set(Some(at));
        s.h.now = 50_000.0 + at * 1000.0;
        s.h.key("KeyY", "y");
    }
    assert!((view(&s).tapped.unwrap() - 120.0).abs() < 1e-6);
    s.h.frame(1.0 / 60.0);
    let text = s.h.texts().into_iter().find(|t| t.starts_with("Tapped"));
    assert_eq!(
        text.as_deref(),
        Some("Tapped 120.00 BPM (10 taps) · offset -0.2 s")
    );
    // The menu's "BPM and offset".
    s.h.key("Space", " ");
    s.h.key("Home", "Home");
    s.h.run(0.3);
    s.h.ui
        .broadcast(Message::new(ACTION).with("Id", Param::Str("tap-offset".into())));
    s.h.run(0.1);
    let e = s.editor.borrow();
    assert_eq!(e.timing().bpms[0].bpm, 120.0);
    assert_eq!(e.timing().offset_seconds, -0.2);
    assert!((e.timing().seconds_at(Tick::from_beats(1)) - 0.7).abs() < 1e-12);
}

fn overview() -> Overview {
    // 30 s at 100 Hz: quiet, loud from 10 s.
    let mut b = Builder::new(100.0, 3000, 0.0);
    let data: Vec<f32> = (0..3000)
        .map(|i| if i < 1000 { 0.01 } else { 0.5 })
        .collect();
    b.add(0, &data);
    b.finish()
}

#[test]
fn the_overview_comes_from_the_shell_and_jumps_the_cursor() {
    for (size, design) in [
        ([1280.0, 720.0], DESIGN),
        ([844.0, 390.0], COMPACT),
        ([390.0, 844.0], COMPACT),
    ] {
        let mut s = setup(size, design);
        s.host.coming.set(true);
        s.h.run(0.1);
        assert_eq!(view(&s).overview, 0);
        let wait = s.h.ui.wake_in().expect("woken for the overview");
        assert!(wait <= 0.25, "{wait}");
        *s.host.overview.borrow_mut() = Some(Rc::new(overview()));
        s.host.coming.set(false);
        s.h.run_on_demand(0.5);
        assert_eq!(view(&s).overview, 3000);
        assert_eq!(s.h.ui.wake_in(), None, "{size:?}");
        // The strip at the field's right, within the screen.
        let strip = s.h.ui.bounds("OverviewPlate").unwrap();
        let field = s.h.ui.bounds("Field").unwrap();
        let vp = s.h.ui.viewport().screen;
        assert!(strip[0] >= field[2] && strip[2] <= vp.width, "{size:?}");
        assert!((strip[1] - field[1]).abs() < 1.0 && (strip[3] - field[3]).abs() < 1.0);
        // Its middle: 15 s, beat 30 at 120 BPM.
        let at = [(strip[0] + strip[2]) / 2.0, (strip[1] + strip[3]) / 2.0];
        s.h.tap_at(PointerKind::Touch, at);
        s.h.run(0.3);
        let cursor = view(&s).cursor;
        assert!((cursor - 30 * 48).abs() <= 12, "{size:?}: {cursor}");
        // Dragged to its end.
        s.h.drag(PointerKind::Mouse, at, [at[0], strip[3] - 1.0], 0.2, 4);
        s.h.run(0.3);
        assert!(view(&s).cursor > 55 * 48, "{size:?}");
    }
}

#[test]
fn without_a_shell_overview_nothing_is_awaited() {
    let mut s = setup([1280.0, 720.0], DESIGN);
    s.h.run_on_demand(1.0);
    assert_eq!(s.h.ui.wake_in(), None);
    assert!(s.h.ui.bounds("OverviewPlate").is_some());
}

#[test]
fn the_panels_lines_stay_above_its_buttons() {
    for (size, design) in [
        ([1280.0, 720.0], DESIGN),
        ([844.0, 390.0], COMPACT),
        ([390.0, 844.0], COMPACT),
    ] {
        let mut s = setup(size, design);
        s.h.key("KeyK", "k");
        for i in 0..6 {
            s.h.now = 20_000.0 + 500.0 * f64::from(i);
            s.h.key("KeyY", "y");
        }
        s.h.frame(1.0 / 60.0);
        let menu = s.h.ui.bounds("Button.Menu").unwrap();
        let compact = size != [1280.0, 720.0];
        assert_eq!(s.h.ui.bounds("Panel.Sounds").is_none(), compact, "{size:?}");
        for name in ["Panel.Sounds", "Panel.Tap", "Panel.Saved"] {
            let Some(b) = s.h.ui.bounds(name) else {
                continue;
            };
            assert!(
                b[3] <= menu[1] + 1.0,
                "{size:?}: {name} {b:?} over {menu:?}"
            );
        }
        assert!(s.h.ui.bounds("Panel.Tap").is_some());
    }
}
