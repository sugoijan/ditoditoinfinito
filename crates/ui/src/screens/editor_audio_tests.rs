//! The editor's audio view (`editor_audio.rs`): each row's seconds as
//! the field maps them, clicks on known beats drawn on their beats' rows
//! in both views at any zoom, and the view on the screen at the three
//! sizes.

use std::cell::{Cell, RefCell};

use ddi_chart::{BpmSegment, StopSegment};
use ddi_engine::scroll::{ScrollAction, ScrollOptions, SpeedMod};

use super::*;
use crate::look::Plain;
use crate::screens::editor::{EditorCall, EditorHost, EditorPlace, EditorView};
use crate::screens::editor_field::{FieldView, SkinField, shown_seconds};
use crate::screens::{COMPACT, DESIGN, Env};
use crate::state::editor::Editor;
use crate::state::editor::audio::Builder;
use crate::testing::Harness;
use crate::{Modifiers, PointerKind, TextMode};

/// An MP3's lead: its first sample heard this late.
const LEAD: f64 = 0.025_1;
const RATE: f64 = 44_100.0;

/// 128 BPM from an offset of 0.123 s (beat 0 at −0.123 s), 170 BPM
/// from beat 16, a 0.4 s stop at beat 24.
fn timing() -> TimingMap {
    let mut t = TimingMap::constant(128.0, 0.123);
    t.bpms.push(BpmSegment {
        tick: Tick::from_beats(16),
        bpm: 170.0,
    });
    t.stops.push(StopSegment {
        tick: Tick::from_beats(24),
        seconds: 0.4,
    });
    t.tidy();
    t
}

/// The beats clicked on.
const BEATS: [i64; 7] = [2, 9, 15, 16, 21, 24, 27];

/// The music: quiet, a click on each of [`BEATS`] at the second it is
/// heard, and one in the middle of the stop (on no beat), its first
/// sample heard at [`LEAD`].
fn music(t: &TimingMap) -> (Analysis, Vec<f64>) {
    let seconds = 16.0;
    let n = (RATE * seconds) as usize;
    let mut s: Vec<f32> = (0..n)
        .map(|i| ((i as f32 * 12.9898).sin() * 43758.547).fract() * 0.003)
        .collect();
    let mut at: Vec<f64> = BEATS
        .iter()
        .map(|b| t.seconds_at(Tick::from_beats(*b)))
        .collect();
    let stop = t.seconds_at(Tick::from_beats(24)) + 0.2;
    for &sec in at.iter().chain([&stop]) {
        // The first sample heard at or after the click's second.
        let i0 = ((sec - LEAD) * RATE).ceil() as usize;
        for i in 0..(RATE * 0.02) as usize {
            let x = i as f64 / RATE;
            s[i0 + i] += ((2.0 * std::f64::consts::PI * 1500.0 * x + 1.2).sin()
                * (-x * 300.0).exp()
                * 0.9) as f32;
        }
    }
    let mut b = Builder::new(RATE, n, LEAD);
    b.add_interleaved(&s, 1);
    at.push(stop);
    (b.finish(), at)
}

/// Each row's waveform (rows `row_h` high from `top`) as the field `m`
/// maps it: the rows where it rises past half after a quieter one.
fn onsets(m: &Mapping, t: &TimingMap, a: &Analysis, field: [f32; 2], row_h: f32) -> Vec<f32> {
    let mut spans = Vec::new();
    let rows = ((field[1] - field[0]) / row_h) as usize;
    row_spans(m, t, &knots(t), field, field[0], row_h, rows, &mut spans);
    let mut level = vec![None; rows];
    for (r, s0, s1) in &spans {
        if let Some(p) = a.wave(*s0, *s1) {
            let l: &mut Option<f32> = &mut level[*r as usize];
            *l = Some(l.unwrap_or(0.0).max(p.max.max(-p.min)));
        }
    }
    // Loud after quiet (past half, after a row under a tenth), in the
    // music's order (up the field under Reverse).
    let mut armed = true;
    let mut out = Vec::new();
    let mut order: Vec<usize> = (0..rows).collect();
    if m.dir() < 0.0 {
        order.reverse();
    }
    for r in order {
        match level[r] {
            Some(l) if l >= 0.5 && armed => {
                out.push(field[0] + r as f32 * row_h);
                armed = false;
            }
            Some(l) if l < 0.1 => armed = true,
            _ => {}
        }
    }
    out.sort_by(f32::total_cmp);
    out
}

/// Every click on a beat in view starts on its beat's row (within a
/// pixel), and nothing else does but the click in the stop, where the
/// view has it (`stop`, its second).
fn check(m: &Mapping, t: &TimingMap, a: &Analysis, stop: f64, what: &str) {
    let field = [0.0, 900.0];
    let line = Line::of(m);
    let in_stop = line.post(line.pre(t, stop)) as f32;
    for row_h in [1.0, 0.5] {
        let rows = onsets(m, t, a, field, row_h);
        let beats: Vec<f32> = BEATS
            .iter()
            .map(|b| m.y(t, Tick::from_beats(*b).0 as f64))
            .filter(|y| *y >= field[0] + 2.0 && *y < field[1] - 2.0)
            .collect();
        assert!(!beats.is_empty(), "{what}: no beat in view");
        for y in &beats {
            assert!(
                rows.iter().any(|r| (r - y).abs() <= 1.0),
                "{what} ({row_h}): the click on y {y} not on its row: {rows:?}"
            );
        }
        for r in &rows {
            assert!(
                beats.iter().chain([&in_stop]).any(|y| (r - y).abs() <= 1.0),
                "{what} ({row_h}): an onset off the beats at {r}: {beats:?}"
            );
        }
    }
}

#[test]
fn clicks_on_beats_are_on_their_rows_in_the_edit_view_at_any_zoom() {
    let t = timing();
    let (a, at) = music(&t);
    let stop = at[at.len() - 1];
    // 22 pixels a beat (zoomed out) to the 192nds' 1152.
    for beat_h in [22.0, 88.0, 352.0, 1152.0] {
        for beat in BEATS {
            let shown = Tick::from_beats(beat).0 as f64 - 0.3 * TICKS_PER_BEAT as f64;
            let m = Mapping::edit(250.0, shown, beat_h, 64.0);
            check(&m, &t, &a, stop, &format!("edit {beat_h} at {beat}"));
        }
    }
}

#[test]
fn clicks_on_beats_are_on_their_rows_in_the_play_view() {
    let t = timing();
    let (a, at) = music(&t);
    let stop = at[at.len() - 1];
    for scroll in [
        ScrollOptions::default(),
        ScrollOptions {
            speed: SpeedMod::XMod(3.0),
            ..Default::default()
        },
        ScrollOptions {
            speed: SpeedMod::CMod(450.0),
            ..Default::default()
        },
        ScrollOptions {
            speed: SpeedMod::XMod(2.0),
            reverse: true,
            scroll_action: ScrollAction::Boost,
        },
        ScrollOptions {
            speed: SpeedMod::XMod(1.5),
            scroll_action: ScrollAction::Brake,
            ..Default::default()
        },
    ] {
        for beat in BEATS {
            let shown = Tick::from_beats(beat).0 as f64 - 0.7 * TICKS_PER_BEAT as f64;
            let at = shown_seconds(&t, shown);
            let m = Mapping::play(scroll, &t, at, [0.0, 900.0], 250.0, shown, 88.0, 64.0);
            check(&m, &t, &a, stop, &format!("play {scroll:?} at {beat}"));
        }
    }
}

#[test]
fn a_stop_has_no_rows_in_the_edit_view_and_its_own_in_a_c_mod() {
    let t = timing();
    let (a, at) = music(&t);
    let stop = at[at.len() - 1];
    let shown = Tick::from_beats(24).0 as f64;
    let edit = Mapping::edit(250.0, shown, 352.0, 64.0);
    let mut spans = Vec::new();
    row_spans(
        &edit,
        &t,
        &knots(&t),
        [0.0, 900.0],
        0.0,
        1.0,
        900,
        &mut spans,
    );
    // The stop's seconds are all in its row, and no other.
    let (s0, s1) = (
        t.seconds_at(Tick::from_beats(24)),
        t.seconds_at(Tick::from_beats(24)) + 0.4,
    );
    let row = edit.y(&t, shown).floor() as u32;
    for (r, a, b) in &spans {
        let inside = *a < s1 - 1e-9 && *b > s0 + 1e-9;
        assert_eq!(inside, *r == row && (*a - s0).abs() < 1e-9, "{r} {a} {b}");
    }
    assert!(
        spans
            .iter()
            .any(|(r, a, b)| *r == row && *a == s0 && *b == s1)
    );
    // In the edit view the click in the stop is folded into its row:
    // no onset of its own.
    let rows = onsets(&edit, &t, &a, [0.0, 900.0], 1.0);
    assert!(
        !rows
            .iter()
            .any(|r| (r - 250.0).abs() > 1.0 && (r - 250.0).abs() < 100.0),
        "{rows:?}"
    );
    // In a c-mod's play view the click in the stop has its own row,
    // where its second is: between the stop's row and the next beat's.
    let scroll = ScrollOptions {
        speed: SpeedMod::CMod(300.0),
        ..Default::default()
    };
    let m = Mapping::play(scroll, &t, s0, [0.0, 900.0], 250.0, shown, 88.0, 64.0);
    let rows = onsets(&m, &t, &a, [0.0, 900.0], 1.0);
    let want = 250.0 + ((stop - s0) * 300.0 / 60.0 * 64.0) as f32;
    assert!(
        rows.iter().any(|r| (r - want).abs() <= 1.0),
        "{want}: {rows:?}"
    );
}

#[test]
fn rows_tile_the_seconds_between_changes() {
    // Rows next to each other cover seconds next to each other.
    let t = timing();
    let m = Mapping::edit(250.0, 20.0 * 48.0, 88.0, 64.0);
    let mut spans = Vec::new();
    row_spans(&m, &t, &knots(&t), [0.0, 900.0], 0.0, 1.0, 900, &mut spans);
    for w in spans.windows(2) {
        let ((r0, _, b0), (r1, a1, _)) = (w[0], w[1]);
        if r1 == r0 + 1 {
            // Or the stop's seconds between, in a row of their own.
            let gap = a1 - b0;
            assert!(gap.abs() < 1e-9 || (gap - 0.4).abs() < 1e-9, "{w:?}");
        }
    }
    // A row is a beat's 88th: at 170 BPM about 4 ms.
    let (_, a, b) = spans[100];
    assert!(((b - a) - 60.0 / 170.0 / 88.0).abs() < 1e-9);
}

/// A shell that has the music's analysis, draws the skin or not, and
/// keeps the pictures it was given.
struct Host {
    analysis: Rc<Analysis>,
    skin: bool,
    pictures: RefCell<Vec<Option<AudioPicture>>>,
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

    fn audio(&self) -> Option<Rc<Analysis>> {
        Some(self.analysis.clone())
    }

    fn skin_field(&self) -> Option<SkinField> {
        self.skin.then(SkinField::default)
    }

    fn show_audio(&self, picture: Option<&AudioPicture>) {
        self.pictures.borrow_mut().push(picture.cloned());
    }
}

struct Setup {
    h: Harness,
    env: Env,
    host: Rc<Host>,
}

/// The editor on [`timing`]'s song with its clicks at `size`.
fn setup(size: [f32; 2], design: [f32; 2], skin: bool) -> Setup {
    let env = Env::new(
        Rc::new(Plain),
        Rc::new(|s| ddi_library::search::Fold::nfkd(&ddi_library::search::Nfkd, s)),
    );
    env.reduced_motion.set(true);
    let t = timing();
    let host = Rc::new(Host {
        analysis: Rc::new(music(&t).0),
        skin,
        pictures: RefCell::default(),
        time: Cell::new(None),
    });
    let mut h = Harness::new(design, size, 1.0, TextMode::Own);
    h.ui.set_design(design);
    let mut editor = Editor::new_song(Some("song.ogg".into()), "Test");
    editor.set_offset(0.123, crate::state::editor::Retime::KeepBeats);
    editor.set_segment(
        Tick::ZERO,
        crate::state::editor::Segment::Bpm(128.0),
        crate::state::editor::Retime::KeepBeats,
    );
    editor.set_segment(
        Tick::from_beats(16),
        crate::state::editor::Segment::Bpm(170.0),
        crate::state::editor::Retime::KeepBeats,
    );
    editor.set_segment(
        Tick::from_beats(24),
        crate::state::editor::Segment::Stop(0.4),
        crate::state::editor::Retime::KeepBeats,
    );
    assert_eq!(editor.timing(), &t);
    editor.mark_saved();
    let place = EditorPlace {
        listen: true,
        playtest: true,
        ..EditorPlace::default()
    };
    let screen = EditorScreen::new(env.clone(), editor, place).with_host(host.clone());
    h.push(screen);
    h.run(0.4);
    Setup { h, env, host }
}

fn view(s: &Setup) -> EditorView {
    s.env.state.borrow().editor.clone().expect("the view")
}

const SHIFT: Modifiers = Modifiers {
    shift: true,
    ctrl: false,
    alt: false,
    meta: false,
};

/// The last picture's rows where the waveform's color starts, as
/// virtual y (the harness draws at a ratio of 1).
fn picture_onsets(s: &Setup) -> Vec<f32> {
    let p = s
        .host
        .pictures
        .borrow()
        .last()
        .cloned()
        .flatten()
        .expect("a picture");
    let [w, h] = p.size;
    let x = (w - 2) as usize; // the waveform's right edge: lit when loud
    let lit = |r: usize| p.rgba[(r * w as usize + x) * 4 + 3] > 0;
    (0..h as usize)
        .filter(|&r| lit(r) && (r == 0 || !lit(r - 1)))
        .map(|r| p.rect[1] + r as f32)
        .collect()
}

#[test]
fn the_column_beside_the_lanes_and_its_keys() {
    let mut s = setup([1280.0, 720.0], DESIGN, true);
    let v = view(&s).audio;
    assert!(v.ready && v.shown, "{v:?}");
    assert_eq!((v.layout.as_str(), v.mode.as_str()), ("column", "both"));
    // Left of the ruler and the lanes, inside the field's rows.
    let column = s.h.ui.bounds("AudioColumn").expect("the column");
    let field = s.h.ui.bounds("Field").unwrap();
    let row = s.h.ui.bounds("Row.0");
    assert!(
        column[0] >= field[0] && column[2] < field[0] + 200.0,
        "{column:?}"
    );
    assert!((column[1] - field[1]).abs() < 1.0 && (column[3] - field[3]).abs() < 1.0);
    assert!(row.is_none_or(|r| r[0] > column[2]));
    // The picture: the column's whole physical pixels.
    let p = s.host.pictures.borrow().last().cloned().flatten().unwrap();
    assert_eq!(p.size, [p.rect[2] as u32, p.rect[3] as u32]);
    assert!((p.rect[0] - column[0]).abs() <= 1.0);
    // The cursor on beat 2: its click starts on its row.
    s.h.key("ArrowDown", "ArrowDown");
    for _ in 0..7 {
        s.h.key("ArrowDown", "ArrowDown");
    }
    s.h.run(0.2);
    assert_eq!(view(&s).cursor, 2 * 48);
    let cursor = s.h.ui.bounds("Cursor").unwrap();
    let y = (cursor[1] + cursor[3]) / 2.0;
    let onsets = picture_onsets(&s);
    assert!(
        onsets.iter().any(|o| (o - y).abs() <= 1.0),
        "{y}: {onsets:?}"
    );
    assert_eq!(view(&s).audio.onsets, onsets);
    // Shift W: the waveform alone, then the spectrogram, then both.
    s.h.key_with("KeyW", "W", SHIFT);
    s.h.frame(1.0 / 60.0);
    assert_eq!(view(&s).audio.mode, "waveform");
    s.h.key_with("KeyW", "W", SHIFT);
    s.h.frame(1.0 / 60.0);
    assert_eq!(view(&s).audio.mode, "spectrogram");
    assert!(view(&s).audio.onsets.is_empty(), "no waveform drawn");
    s.h.key_with("KeyW", "W", SHIFT);
    s.h.frame(1.0 / 60.0);
    assert_eq!(view(&s).audio.mode, "both");
    // W hides it: the lanes take the room, the shell shows none.
    let lanes_before = s.h.ui.bounds("Cursor").unwrap();
    s.h.key("KeyW", "w");
    s.h.run(0.2);
    let v = view(&s).audio;
    assert!(!v.shown && v.layout.is_empty());
    assert!(s.h.ui.bounds("AudioColumn").is_none());
    assert_eq!(s.host.pictures.borrow().last(), Some(&None));
    assert_ne!(s.h.ui.bounds("Cursor").unwrap()[0], lanes_before[0]);
    // And shows it again; the panel's line too.
    s.h.tap(PointerKind::Mouse, "Button.Audio");
    s.h.run(0.2);
    assert!(view(&s).audio.shown);
    assert!(
        s.h.texts()
            .iter()
            .any(|t| t == "Audio: waveform and spectrogram (W)")
    );
    // The menu has both.
    s.h.key("Tab", "Tab");
    s.h.run(0.2);
    let texts = s.h.texts();
    assert!(
        texts.iter().any(|t| t == "Hide the audio view (W)"),
        "{texts:?}"
    );
    assert!(texts.iter().any(|t| t == "Audio view: waveform (Shift W)"));
}

#[test]
fn a_tap_on_the_column_moves_the_cursor_and_the_wheel_scrolls() {
    let mut s = setup([1280.0, 720.0], DESIGN, true);
    let column = s.h.ui.bounds("AudioColumn").unwrap();
    let cursor = s.h.ui.bounds("Cursor").unwrap();
    let line = (cursor[1] + cursor[3]) / 2.0;
    let beat_h = view(&s).beat_h;
    let x = (column[0] + column[2]) / 2.0;
    s.h.tap_at(PointerKind::Mouse, [x, line + 3.0 * beat_h]);
    s.h.run(0.2);
    assert_eq!(view(&s).cursor, 3 * 48);
    assert!(!view(&s).dirty, "no note placed");
    s.h.wheel_lines([x, line], [0.0, 2.0]);
    s.h.run(0.2);
    assert!(view(&s).cursor > 3 * 48);
}

#[test]
fn listening_paints_it_as_the_field_moves() {
    let mut s = setup([1280.0, 720.0], DESIGN, true);
    s.h.run_on_demand(0.5);
    let before = view(&s).audio.painted;
    assert!(s.h.run_on_demand(0.5) == 0, "nothing drawn between changes");
    assert_eq!(view(&s).audio.painted, before);
    s.h.key("Space", " ");
    for i in 1..=10 {
        s.host.time.set(Some(0.1 * f64::from(i)));
        s.h.frame(1.0 / 60.0);
    }
    let v = view(&s);
    assert!(v.listening);
    assert!(
        v.audio.painted >= before + 9,
        "{} {before}",
        v.audio.painted
    );
    // Painted from the rows kept as the field scrolls: still each click
    // on its beat's line (the picture on the nearest whole pixel).
    let cursor = s.h.ui.bounds("Cursor").unwrap();
    let beat0 = (cursor[1] + cursor[3]) / 2.0;
    assert!(!v.audio.onsets.is_empty());
    for o in &v.audio.onsets {
        let k = ((o - beat0) / v.beat_h).round();
        assert!(BEATS.contains(&(k as i64)), "{o}: beat {k}");
        assert!(
            (o - (beat0 + k * v.beat_h)).abs() <= 1.0,
            "{o} {beat0} {}",
            v.beat_h
        );
    }
    let picture = s.host.pictures.borrow().last().cloned().flatten().unwrap();
    assert_eq!(picture.rect[1].fract(), 0.0, "on whole pixels");
    s.h.key("Space", " ");
    s.h.run(0.2);
    // In the play view too.
    s.h.key("KeyV", "v");
    s.h.run(0.2);
    assert_eq!(view(&s).view, FieldView::Play.name());
    assert!(view(&s).audio.shown);
}

#[test]
fn where_there_is_no_room_it_is_behind_the_lanes() {
    for (size, design, skin) in [
        ([844.0, 390.0], COMPACT, true),
        ([390.0, 844.0], COMPACT, true),
        ([390.0, 844.0], COMPACT, false),
    ] {
        let mut s = setup(size, design, skin);
        // Hidden at first: no room beside the lanes.
        let v = view(&s).audio;
        assert!(v.ready && !v.shown, "{size:?}: {v:?}");
        s.h.key("KeyW", "w");
        s.h.run(0.2);
        let v = view(&s).audio;
        assert!(v.shown, "{size:?}");
        assert_eq!(v.layout, "behind", "{size:?}");
        assert!(s.h.ui.bounds("AudioColumn").is_none());
        let picture = s.h.ui.bounds("AudioPicture").expect("the picture");
        let field = s.h.ui.bounds("Field").unwrap();
        assert!(picture[0] > field[0] && picture[2] < field[2], "{size:?}");
        let alpha = s.h.actor("AudioPicture").current().diffuse[0][3];
        assert!(alpha < 0.5, "translucent: {alpha}");
        // A tap there is a lane's: a note.
        let at = [
            (picture[0] + picture[2]) / 2.0,
            (picture[1] + picture[3]) / 2.0,
        ];
        s.h.tap_at(PointerKind::Touch, at);
        s.h.run(0.2);
        assert!(view(&s).dirty, "{size:?}");
    }
}

#[test]
fn without_music_there_is_no_view() {
    let env = Env::new(
        Rc::new(Plain),
        Rc::new(|s| ddi_library::search::Fold::nfkd(&ddi_library::search::Nfkd, s)),
    );
    let mut h = Harness::new(DESIGN, [1280.0, 720.0], 1.0, TextMode::Own);
    h.push(EditorScreen::new(
        env.clone(),
        Editor::new_song(None, "Test"),
        EditorPlace::default(),
    ));
    h.run(0.3);
    h.key("KeyW", "w");
    h.run(0.2);
    let v = env.state.borrow().editor.clone().unwrap().audio;
    assert!(!v.ready && !v.shown);
    assert!(h.texts().iter().any(|t| t == "No music to show"));
}

/// What painting costs (`cargo test -p ddi-ui --lib paint_cost --
/// --ignored --nocapture`): a full-height column at a ratio of 2.
#[test]
#[ignore]
fn paint_cost() {
    use std::time::Instant;
    let t = timing();
    let (a, _) = music(&t);
    let colors = Colors {
        ramp: [[255; 4]; 256],
        wave: [255; 4],
        rms: [255; 4],
    };
    let (w, h) = (320usize, 1168usize);
    let k = knots(&t);
    let mut spans = Vec::new();
    for beat_h in [22.0, 96.0, 1152.0] {
        let (mut t_rows, mut t_paint, n) = (0.0, 0.0, 200);
        for i in 0..n {
            let shown = 4.0 * 48.0 + f64::from(i) * 0.37;
            let m = Mapping::edit(160.0, shown, beat_h, 72.0);
            let t0 = Instant::now();
            row_spans(&m, &t, &k, [80.0, 664.0], 80.0, 0.5, h, &mut spans);
            let t1 = Instant::now();
            let mut rgba = vec![0u8; w * h * 4];
            let mode = [AudioMode::Both, AudioMode::Wave, AudioMode::Spectrogram][i as usize % 3];
            let painter = Painter::new(&a, &colors, mode, w);
            for (r, row) in by_row(&spans) {
                painter.row(row, &mut rgba[r * w * 4..(r + 1) * w * 4]);
            }
            let t2 = Instant::now();
            t_rows += (t1 - t0).as_secs_f64();
            t_paint += (t2 - t1).as_secs_f64();
        }
        eprintln!(
            "beat {beat_h}: rows {:.0} µs, paint {:.0} µs",
            t_rows / f64::from(n) * 1e6,
            t_paint / f64::from(n) * 1e6
        );
    }
}
