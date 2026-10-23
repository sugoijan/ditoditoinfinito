use ddi_chart::{NoteKind, Tick};

use super::*;
use crate::state::editor::{MEASURE, Retime, SNAPS, Segment};

fn beat(b: i64) -> Tick {
    Tick::from_beats(b)
}

/// A song at 120 BPM, then 180 BPM from beat 8, a stop of 0.5 s on beat
/// 12, a delay of 0.25 s on beat 16, a warp over beat 20 (one beat).
fn timed() -> Editor {
    let mut e = Editor::new_song(Some("song.ogg".into()), "Record");
    assert!(e.set_segment(beat(8), Segment::Bpm(180.0), Retime::KeepBeats));
    assert!(e.set_segment(beat(12), Segment::Stop(0.5), Retime::KeepBeats));
    assert!(e.set_segment(beat(16), Segment::Delay(0.25), Retime::KeepBeats));
    assert!(e.set_segment(beat(20), Segment::Warp(beat(1)), Retime::KeepBeats));
    e.mark_saved();
    e
}

/// The second row `t` is judged at.
fn at(e: &Editor, t: Tick) -> f64 {
    e.timing().seconds_at(t)
}

#[test]
fn presses_land_on_the_nearest_row_of_every_snap_through_the_timing() {
    let mut e = timed();
    for &snap in &SNAPS {
        e.set_snap(snap);
        let step = e.step();
        let take = Take::new(&e, Mode::Replace);
        // Rows of the snap at 120 BPM, after the BPM change, on the stop,
        // on the delay, after the warp.
        for b in [1, 5, 9, 11, 12, 13, 16, 17, 21, 23] {
            for k in 0..(MEASURE / 4 / step).max(1) {
                let row = beat(b) + Tick(k * step);
                if e.timing().in_warp(row) {
                    continue;
                }
                // On it, and heard 0.4 of a snap before and after it.
                let near = |by: f64| {
                    e.timing()
                        .seconds_at_beat(row.beat() + by * step as f64 / 48.0)
                };
                for second in [at(&e, row), near(-0.4), near(0.4)] {
                    assert_eq!(
                        take.row(second),
                        row,
                        "snap {snap}, row {row:?}, second {second}"
                    );
                }
            }
        }
    }
}

#[test]
fn stops_delays_warps_and_ties() {
    let mut e = timed();
    e.set_snap(16);
    let take = Take::new(&e, Mode::Replace);
    // In the stop (0.5 s on beat 12), on its beat; the same in the delay.
    assert_eq!(take.row(at(&e, beat(12)) + 0.3), beat(12));
    assert_eq!(take.row(at(&e, beat(16)) - 0.1), beat(16));
    // Just before the warp's second: its rows are never heard, the first
    // after it.
    let w = at(&e, beat(20));
    assert_eq!(take.row(w - 0.01), beat(21));
    assert_eq!(take.row(w + 0.01), beat(21));
    // Halfway between two rows: the later (StepMania's Quantize).
    let half = (at(&e, beat(1)) + at(&e, beat(1) + Tick(12))) / 2.0;
    assert_eq!(take.row(half), beat(1) + Tick(12));
    // 12ths and 24ths: thirds and sixths of a beat.
    e.set_snap(12);
    let take = Take::new(&e, Mode::Replace);
    assert_eq!(
        take.row(at(&e, beat(2)) + 0.5 / 3.0 + 0.02),
        beat(2) + Tick(16)
    );
    e.set_snap(24);
    let take = Take::new(&e, Mode::Replace);
    assert_eq!(
        take.row(at(&e, beat(2)) + 0.5 / 6.0 * 5.0),
        beat(2) + Tick(40)
    );
}

#[test]
fn a_press_held_past_the_threshold_is_a_hold_a_roll_with_the_roll_tool() {
    let mut e = timed();
    e.set_snap(8);
    let mut take = Take::new(&e, Mode::Replace);
    let s = at(&e, beat(2));
    // Just under the threshold: a tap, even when let go on a later row.
    take.press(0, s);
    take.update(s + HOLD_SECONDS - 0.01);
    assert_eq!(take.notes()[0].kind, NoteKind::Tap);
    take.release(0, s + HOLD_SECONDS - 0.01);
    // Just over: a hold to the row let go on, growing while held.
    take.press(1, s);
    take.update(s + HOLD_SECONDS + 0.01);
    assert_eq!(
        take.notes()[1].kind,
        NoteKind::HoldHead {
            end: beat(2) + Tick(24)
        },
        "at least a snap long"
    );
    take.update(s + 1.0);
    assert_eq!(take.notes()[1].kind.hold_end(), Some(beat(4)));
    take.release(1, s + 1.26);
    assert_eq!(take.notes()[1].kind.hold_end(), Some(beat(4) + Tick(24)));
    // A release stamped before an update saw it held: a tap again.
    take.press(2, s);
    take.update(s + 0.4);
    take.release(2, s + 0.2);
    assert_eq!(take.notes()[2].kind, NoteKind::Tap);
    e.record(take, s + 2.0);
    // The roll tool: rolls. A mine (or lift, fake): taps and holds, as
    // StepMania records.
    for (tool, roll) in [(NoteTool::Roll, true), (NoteTool::Mine, false)] {
        e.set_tool(tool);
        e.set_cursor(beat(4), false);
        let mut take = Take::new(&e, Mode::Replace);
        let s = at(&e, beat(5));
        take.press(3, s);
        take.press(2, s + 0.25);
        take.release(3, s + 0.5);
        take.release(2, s + 0.3);
        e.record(take, s + 1.0);
        let n = e.note_at(3, beat(5)).expect("recorded").kind;
        assert_eq!(
            n,
            if roll {
                NoteKind::RollHead { end: beat(6) }
            } else {
                NoteKind::HoldHead { end: beat(6) }
            }
        );
        assert_eq!(
            e.note_at(2, beat(5) + Tick(24)).unwrap().kind,
            NoteKind::Tap
        );
    }
}

#[test]
fn a_tap_inside_a_recorded_hold_takes_its_place() {
    let mut e = timed();
    e.set_snap(4);
    let mut take = Take::new(&e, Mode::Replace);
    let s = at(&e, beat(2));
    take.press(0, s);
    take.release(0, s + 1.0);
    assert_eq!(take.notes()[0].kind.hold_end(), Some(beat(4)));
    // Pressed again at once: on the hold's tail row.
    take.press(0, s + 1.02);
    take.release(0, s + 1.1);
    assert_eq!(take.notes().len(), 1);
    assert_eq!(take.notes()[0].tick, beat(4));
    assert_eq!(take.notes()[0].kind, NoteKind::Tap);
}

/// A chart with taps on every beat of lanes 0 and 3 from beat 0 to 15.
fn charted() -> Editor {
    let mut e = timed();
    for b in 0..16 {
        e.toggle_at(0, beat(b));
        e.toggle_at(3, beat(b));
    }
    e.mark_saved();
    e
}

#[test]
fn replace_takes_away_the_span_add_keeps_what_is_there() {
    // From the cursor at beat 4, stopped on beat 8.
    let mut e = charted();
    e.set_snap(4);
    e.set_cursor(beat(4), false);
    let mut take = Take::new(&e, Mode::Replace);
    take.press(1, at(&e, beat(5)));
    take.release(1, at(&e, beat(5)) + 0.1);
    take.press(0, at(&e, beat(6)));
    take.release(0, at(&e, beat(6)) + 0.1);
    let r = e.record(take, at(&e, beat(8)));
    assert_eq!(r.span, (beat(4), beat(8)));
    assert_eq!(r.notes, 2);
    assert_eq!(e.cursor(), beat(8));
    for b in 0..16 {
        let kept = !(4..8).contains(&b);
        assert_eq!(e.note_at(3, beat(b)).is_some(), kept, "lane 3 beat {b}");
        assert_eq!(
            e.note_at(0, beat(b)).is_some(),
            kept || b == 6,
            "lane 0 {b}"
        );
    }
    assert!(e.note_at(1, beat(5)).is_some());
    // Add: what is there stays; a note on one is left out; a hold over a
    // later note ends a snap before it.
    let mut e = charted();
    e.set_snap(4);
    e.set_cursor(beat(4), false);
    let mut take = Take::new(&e, Mode::Add);
    take.press(1, at(&e, beat(5)));
    take.release(1, at(&e, beat(5)) + 0.1);
    take.press(0, at(&e, beat(6)));
    take.release(0, at(&e, beat(6)) + 0.1);
    e.toggle_at(2, beat(11));
    take.press(2, at(&e, beat(9)));
    take.press(3, at(&e, beat(9)) - 0.1 * 60.0 / 180.0);
    take.release(2, at(&e, beat(12)));
    take.release(3, at(&e, beat(12)));
    let before = e.note_count();
    let r = e.record(take, at(&e, beat(13)));
    assert_eq!(r.mode, Mode::Add);
    assert_eq!(r.notes, 2, "the one on lane 0's tap and lane 3's left out");
    assert_eq!(e.note_count(), before + 2);
    for b in 0..16 {
        assert!(e.note_at(0, beat(b)).is_some());
        assert_eq!(e.note_at(3, beat(b)).unwrap().tick, beat(b));
    }
    assert_eq!(
        e.note_at(2, beat(9)).unwrap().kind,
        NoteKind::HoldHead { end: beat(10) },
        "cut a snap before the tap on beat 11"
    );
}

#[test]
fn a_selection_bounds_the_take() {
    let mut e = charted();
    e.set_snap(8);
    e.select(beat(4), beat(7));
    let mut take = Take::new(&e, Mode::Replace);
    assert_eq!((take.start(), take.last()), (beat(4), Some(beat(7))));
    // Before and after it: ignored.
    assert_eq!(take.press(1, at(&e, beat(3) + Tick(24))), None);
    assert_eq!(take.press(1, at(&e, beat(7) + Tick(24))), None);
    assert_eq!(take.press(1, at(&e, beat(7))), Some(beat(7)));
    // A hold held past its end: to its last row.
    take.press(2, at(&e, beat(6)));
    take.update(at(&e, beat(9)));
    assert_eq!(take.notes()[1].kind.hold_end(), Some(beat(7)));
    assert!(!take.ended(at(&e, beat(7)) + 0.1));
    assert!(take.ended(at(&e, beat(7) + Tick(12))));
    let r = e.record(take, at(&e, beat(8)));
    assert_eq!(r.span, (beat(4), beat(7) + Tick(1)));
    assert_eq!(e.cursor(), beat(7));
    assert!(e.selection().is_none());
    for b in 0..16 {
        let kept = !(4..=7).contains(&b);
        assert_eq!(e.note_at(3, beat(b)).is_some(), kept, "beat {b}");
    }
}

#[test]
fn presses_in_the_preroll_count_only_within_half_a_snap() {
    let mut e = timed();
    e.set_snap(8);
    e.set_cursor(beat(4), false);
    let mut take = Take::new(&e, Mode::Replace);
    // A measure before beat 4 at 120 BPM: two seconds.
    assert!((take.preroll() - (at(&e, beat(4)) - 2.0)).abs() < 1e-9);
    assert_eq!(take.start_seconds(), at(&e, beat(4)));
    let eighth = 0.25;
    assert_eq!(take.press(0, at(&e, beat(4)) - eighth * 0.4), Some(beat(4)));
    assert_eq!(take.press(1, at(&e, beat(4)) - eighth * 0.6), None);
    assert_eq!(take.press(2, at(&e, beat(2))), None);
    // In 3/4 the pre-roll is three beats; from beat 1 it starts with the
    // song.
    assert!(e.set_segment(beat(0), Segment::TimeSignature(3, 4), Retime::KeepBeats));
    let take = Take::new(&e, Mode::Replace);
    assert!((take.preroll() - (at(&e, beat(4)) - 1.5)).abs() < 1e-9);
    e.set_cursor(beat(1), false);
    assert_eq!(Take::new(&e, Mode::Replace).preroll(), 0.0);
}

#[test]
fn one_undo_takes_the_whole_take_back() {
    let mut e = charted();
    let song = e.song().clone();
    e.set_snap(16);
    e.set_cursor(beat(2), false);
    let mut take = Take::new(&e, Mode::Replace);
    for (i, b) in [2, 3, 4, 5, 6].into_iter().enumerate() {
        take.press(i as u8 % 4, at(&e, beat(b)));
        take.release(
            i as u8 % 4,
            at(&e, beat(b)) + if i == 2 { 0.5 } else { 0.1 },
        );
    }
    e.record(take, at(&e, beat(7)));
    assert!(e.is_dirty());
    assert_ne!(e.song(), &song);
    e.undo();
    assert_eq!(e.song(), &song);
    assert!(!e.is_dirty());
    e.redo();
    assert_eq!(
        e.note_at(2, beat(4)).unwrap().kind.hold_end(),
        Some(beat(5))
    );
}

#[test]
fn the_mean_and_spread_say_how_far_off_the_grid() {
    let mut e = timed();
    e.set_snap(4);
    let mut take = Take::new(&e, Mode::Replace);
    for (lane, (b, ms)) in [(1, 10.0), (2, 20.0), (3, 30.0)].into_iter().enumerate() {
        take.press(lane as u8, at(&e, beat(b)) + ms / 1000.0);
    }
    let (mean, spread) = take.stats();
    assert!((mean.unwrap() - 20.0).abs() < 1e-6);
    assert!((spread.unwrap() - (200.0f64 / 3.0).sqrt()).abs() < 1e-6);
    let r = e.record(take, at(&e, beat(4)));
    assert_eq!(r.presses, 3);
    assert_eq!(
        r.describe(),
        "3 notes recorded · 20 ms late on average, ±8 ms"
    );
    let empty = Take::new(&e, Mode::Add);
    assert_eq!(empty.stats(), (None, None));
    assert_eq!(e.record(empty, 0.0).describe(), "Nothing recorded");
}
