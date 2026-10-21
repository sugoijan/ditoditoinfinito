use std::path::Path;

use ddi_chart::formats::sm::parse_ssc;
use ddi_chart::formats::sm_write::{SSC_VERSION, write_ssc};
use ddi_chart::{Difficulty, NoteKind, Song, SourceFormat, SpeedUnit, Tick};
use ddi_engine::transform::Turn;

use super::*;

fn bundled() -> Vec<Song> {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../assets/songs");
    let mut songs = Vec::new();
    for entry in std::fs::read_dir(dir).unwrap() {
        for file in std::fs::read_dir(entry.unwrap().path()).unwrap() {
            let path = file.unwrap().path();
            if path.extension().is_some_and(|e| e == "ssc") {
                songs.push(parse_ssc(&std::fs::read_to_string(&path).unwrap()).unwrap());
            }
        }
    }
    songs.sort_by(|a, b| a.title.cmp(&b.title));
    songs
}

fn beat(b: i64) -> Tick {
    Tick::from_beats(b)
}

/// What a file keeps of a chart: sorted, one note to a cell, nothing
/// inside a hold in its lane.
fn assert_tidy(e: &Editor, what: &str) {
    for (i, chart) in e.song().charts.iter().enumerate() {
        let notes = &chart.notes;
        assert!(
            notes
                .windows(2)
                .all(|w| (w[0].tick, w[0].lane) < (w[1].tick, w[1].lane)),
            "{what}: chart {i} not sorted or two notes in a cell"
        );
        for lane in 0..64u8 {
            let mut last_end = Tick(-1);
            for n in notes.iter().filter(|n| n.lane == lane) {
                assert!(
                    n.tick > last_end,
                    "{what}: chart {i} lane {lane} overlaps at {n:?}"
                );
                if let Some(end) = n.kind.hold_end() {
                    assert!(end > n.tick, "{what}: empty hold {n:?}");
                }
                last_end = n.end_tick();
            }
        }
    }
}

/// The song as a file reads it back.
fn assert_round_trips(song: &Song, what: &str) {
    let text = write_ssc(song);
    let mut back = parse_ssc(&text).unwrap();
    back.source.format = song.source.format.clone();
    let mut expected = song.clone();
    expected.source.format = SourceFormat::Ssc {
        version: SSC_VERSION,
    };
    back.source.format = expected.source.format.clone();
    assert!(
        back == expected,
        "{what}: the edited song does not read back\n{text}"
    );
}

#[test]
fn a_press_places_and_takes_away() {
    let mut e = Editor::new_song(Some("song.ogg".into()), " New ");
    assert_eq!(e.song().title, "New");
    assert!(e.is_dirty(), "a new song is not saved");
    e.mark_saved();
    assert!(!e.is_dirty());
    e.set_cursor(beat(2), false);
    e.press(1);
    e.release(1);
    assert_eq!(e.chart().notes.len(), 1);
    assert_eq!(e.chart().notes[0].tick, beat(2));
    assert!(e.is_dirty());
    e.press(1);
    e.release(1);
    assert!(e.chart().notes.is_empty());
    e.undo();
    assert_eq!(e.chart().notes.len(), 1);
    e.undo();
    assert!(!e.is_dirty(), "undone back to what was saved");
    e.redo();
    e.redo();
    assert!(e.chart().notes.is_empty());
    assert!(!e.can_redo());
    // Out of the layout's lanes: nothing.
    e.press(9);
    assert!(e.chart().notes.is_empty());
}

#[test]
fn held_lanes_grow_holds_and_rolls() {
    let mut e = Editor::new_song(None, "x");
    e.press(0);
    e.move_snaps(4, false);
    e.move_snaps(4, false);
    e.release(0);
    assert_eq!(
        e.chart().notes[0].kind,
        NoteKind::HoldHead { end: beat(2) },
        "16ths: eight steps are two beats"
    );
    // One undo takes the whole hold back.
    e.undo();
    assert!(e.chart().notes.is_empty());

    e.set_tool(NoteTool::Roll);
    e.set_cursor(beat(4), false);
    e.press(2);
    e.move_measures(1, false);
    e.move_snaps(-4, false);
    e.release(2);
    assert_eq!(e.chart().notes[0].kind, NoteKind::RollHead { end: beat(7) });
    // Back to the press: a tap again.
    e.set_cursor(beat(12), false);
    e.press(3);
    e.move_snaps(1, false);
    e.move_snaps(-1, false);
    e.release(3);
    assert_eq!(e.chart().notes[1].kind, NoteKind::Tap);
    // Pressing inside the roll takes it away.
    e.set_cursor(beat(5), false);
    e.press(2);
    e.release(2);
    assert_eq!(e.chart().notes.len(), 1);
    // A hold placed over notes replaces them.
    e.set_tool(NoteTool::Tap);
    e.place_long(3, beat(14), beat(10));
    assert_eq!(e.chart().notes.len(), 1);
    assert_eq!(
        e.chart().notes[0].kind,
        NoteKind::HoldHead { end: beat(14) }
    );
    assert_tidy(&e, "holds");
}

#[test]
fn the_cursor_moves_on_the_snap_grid() {
    let mut e = Editor::new_song(None, "x");
    assert_eq!(e.snap(), 16);
    e.set_cursor(Tick(5), false);
    e.move_snaps(1, false);
    assert_eq!(e.cursor(), Tick(12));
    e.set_cursor(Tick(5), false);
    e.move_snaps(-1, false);
    assert_eq!(e.cursor(), Tick(0));
    e.move_snaps(-1, false);
    assert_eq!(e.cursor(), Tick(0), "not before the start");
    e.change_snap(-10);
    assert_eq!(e.snap(), 4);
    e.change_snap(100);
    assert_eq!(e.snap(), 192);
    e.set_snap(12);
    e.set_cursor(beat(1) + Tick(3), false);
    e.move_measures(-1, false);
    assert_eq!(e.cursor(), Tick(0));
    e.move_measures(2, true);
    assert_eq!(e.cursor(), beat(8));
    assert_eq!(e.selection(), Some((Tick(0), beat(8))));
    e.move_snaps(1, false);
    assert_eq!(e.selection(), None);
    // 120 BPM from 0: a beat is half a second.
    assert!((e.cursor_seconds() - 4.0 - 1.0 / 6.0).abs() < 1e-9);
    assert_eq!(e.tick_at_seconds(1.01), beat(2));
}

#[test]
fn selections_are_cut_pasted_shifted_and_mirrored() {
    let mut e = Editor::new_song(None, "x");
    for (b, lane) in [(0, 0), (1, 1), (2, 2), (3, 3)] {
        e.toggle_at(lane, beat(b));
    }
    e.place_long(0, beat(1), beat(2));
    e.select(beat(0), beat(1));
    e.cut();
    let clip = e.clip().unwrap().clone();
    assert_eq!(clip.length, beat(1));
    assert_eq!(clip.notes.len(), 3, "the hold starting in it too");
    assert_eq!(e.chart().notes.len(), 2);
    e.set_cursor(beat(8), false);
    e.paste();
    assert_eq!(e.chart().notes.len(), 5);
    assert!(
        e.chart()
            .notes
            .iter()
            .any(|n| n.tick == beat(9) && n.kind == NoteKind::HoldHead { end: beat(10) })
    );
    // Pasting again over itself replaces, not doubles.
    e.paste();
    assert_eq!(e.chart().notes.len(), 5);

    e.select(beat(8), beat(9));
    e.turn_selection(Turn::Mirror);
    let lanes: Vec<u8> = e
        .chart()
        .notes
        .iter()
        .filter(|n| n.tick >= beat(8))
        .map(|n| n.lane)
        .collect();
    assert_eq!(lanes, vec![3, 2, 3], "0 and 1 to 3 and 2, sorted again");
    e.shift_selection(4);
    assert_eq!(e.selection(), Some((beat(9), beat(10))));
    assert!(e.chart().notes.iter().all(|n| n.tick != beat(8)));
    e.select(beat(0), beat(1));
    e.shift_selection(-4);
    assert_eq!(
        e.selection(),
        Some((beat(0), beat(1))),
        "not before the start"
    );
    e.select_all();
    e.delete_selection();
    assert!(e.chart().notes.is_empty());
    assert_tidy(&e, "selection");
}

#[test]
fn timing_keeps_beats_or_times() {
    let mut e = Editor::new_song(None, "x");
    e.toggle_at(0, beat(8));
    e.place_long(1, beat(4), beat(12));
    // A BPM change keeps the notes on their beats…
    assert!(e.set_segment(beat(4), Segment::Bpm(240.0), Retime::KeepBeats));
    assert_eq!(e.chart().notes[1].tick, beat(8));
    assert!((e.timing().seconds_at(beat(8)) - 3.0).abs() < 1e-9);
    e.undo();
    // …or their times.
    assert!(e.set_segment(beat(4), Segment::Bpm(240.0), Retime::KeepTimes));
    let at8 = e.chart().notes.iter().find(|n| n.lane == 0).unwrap().tick;
    assert_eq!(at8, beat(12), "4 s in: beat 4 at 2 s, then 8 beats at 240");
    assert_eq!(
        e.chart().notes[0].kind,
        NoteKind::HoldHead { end: beat(20) }
    );
    e.undo();
    assert_eq!(
        e.chart().notes[1].tick,
        beat(8),
        "one undo for timing and notes"
    );

    // Refused: what a file cannot hold.
    assert!(!e.set_segment(beat(1), Segment::Bpm(0.0), Retime::KeepBeats));
    assert!(!e.set_segment(beat(1), Segment::TimeSignature(0, 4), Retime::KeepBeats));
    assert!(!e.set_segment(Tick(-1), Segment::Stop(1.0), Retime::KeepBeats));
    // The first BPM stays.
    e.remove_segment(SegmentKind::Bpm, Tick::ZERO, Retime::KeepBeats);
    assert_eq!(e.timing().bpms.len(), 1);
    // A stop of 0 is none; one set again replaces.
    e.set_segment(beat(2), Segment::Stop(0.5), Retime::KeepBeats);
    e.set_segment(beat(2), Segment::Stop(0.25), Retime::KeepBeats);
    assert_eq!(e.segments_at(beat(2)), vec![Segment::Stop(0.25)]);
    e.set_segment(beat(2), Segment::Stop(0.0), Retime::KeepBeats);
    assert!(e.segments_at(beat(2)).is_empty());
    // A later time signature brings a 4/4 at the start, as files read.
    e.set_segment(beat(8), Segment::TimeSignature(3, 4), Retime::KeepBeats);
    assert_eq!(e.timing().time_signatures.len(), 2);
    e.set_segment(beat(3), Segment::Label("a,b=c ".into()), Retime::KeepBeats);
    assert_eq!(e.segments_at(beat(3)), vec![Segment::Label("a b c".into())]);
    e.set_segment(beat(3), Segment::TickCount(99), Retime::KeepBeats);
    assert_eq!(e.timing().tick_counts[0].ticks_per_beat, 48);
    let markers = e.segments();
    assert!(
        markers
            .iter()
            .any(|(t, s)| *t == beat(8) && s.kind() == SegmentKind::TimeSignature)
    );

    // Own timing: edits stay in the chart.
    e.set_offset(0.5, Retime::KeepBeats);
    e.set_own_timing(true);
    e.set_offset(-0.25, Retime::KeepBeats);
    assert_eq!(e.song().timing.offset_seconds, 0.5);
    assert_eq!(e.timing().offset_seconds, -0.25);
    e.set_own_timing(false);
    assert_eq!(e.timing().offset_seconds, 0.5);
    assert_round_trips(e.song(), "timing");
}

#[test]
fn charts_and_song_fields() {
    let mut e = Editor::open(bundled().remove(0), 0);
    let charts = e.song().charts.len();
    let tags = e.song().source.unknown_tags.clone();
    e.select_chart(1);
    e.remove_chart();
    assert_eq!(e.song().charts.len(), charts - 1);
    assert_round_trips(e.song(), "a chart removed");
    e.add_chart("pump-single", Difficulty::Hard);
    assert_eq!(e.chart_index(), charts - 1);
    assert_eq!(e.lanes(), 5);
    e.toggle_at(4, beat(1));
    let mut info = e.chart_info();
    info.meter = 12;
    info.name = "  ".into();
    info.description = " Mine ".into();
    e.set_chart_info(info);
    assert_eq!(e.chart().name, "Mine", "an empty name is the description");
    let mut song = e.song_info();
    song.title = " Edited ".into();
    song.banner = Some("  ".into());
    song.display_bpm = DisplayBpm::Range(150.0, 150.0);
    e.set_song_info(song);
    assert_eq!(e.song().title, "Edited");
    assert_eq!(e.song().banner, None);
    assert_eq!(e.song().display_bpm, DisplayBpm::Single(150.0));
    assert_round_trips(e.song(), "charts and fields");
    while e.can_undo() {
        e.undo();
    }
    assert_eq!(e.song().charts.len(), charts);
    assert_eq!(
        e.song().source.unknown_tags,
        tags,
        "chart tags back in place"
    );
    assert_eq!(
        e.song(),
        &Editor::open(bundled().remove(0), 0).song().clone()
    );
}

#[test]
fn generated_charts_are_left_out() {
    let mut song = bundled().remove(0);
    let mut derived = song.charts[0].clone();
    derived.derivation = Some(ddi_chart::simplify::Derivation::WithoutOnigiri { source: 0 });
    song.charts.push(derived);
    let count = song.charts.len();
    let e = Editor::open(song, count - 1);
    assert_eq!(e.song().charts.len(), count - 1);
    assert_eq!(e.chart_index(), count - 2);
    assert!(!e.is_dirty());
}

// ---------------------------------------------------------------------------
// Random edits.
// ---------------------------------------------------------------------------

/// xorshift64*: deterministic, no dependency.
struct Rng(u64);

impl Rng {
    fn next(&mut self) -> u64 {
        self.0 ^= self.0 >> 12;
        self.0 ^= self.0 << 25;
        self.0 ^= self.0 >> 27;
        self.0.wrapping_mul(0x2545_f491_4f6c_dd1d)
    }

    fn below(&mut self, n: u64) -> u64 {
        self.next() % n
    }

    fn tick(&mut self, e: &Editor) -> Tick {
        e.snap_tick(Tick(self.below(e.end().0 as u64 + 400) as i64))
    }
}

/// One random edit (or move), as a user would make it.
fn random_edit(rng: &mut Rng, e: &mut Editor) {
    let lanes = u64::from(e.lanes());
    let retime = if rng.below(4) == 0 {
        Retime::KeepTimes
    } else {
        Retime::KeepBeats
    };
    match rng.below(24) {
        0..=2 => {
            let t = rng.tick(e);
            e.set_cursor(t, rng.below(3) == 0);
        }
        3 => e.move_snaps(rng.below(9) as i64 - 4, rng.below(2) == 0),
        4 => e.change_snap(rng.below(3) as i32 - 1),
        5..=7 => {
            let lane = rng.below(lanes) as u8;
            e.press(lane);
            if rng.below(2) == 0 {
                e.move_snaps(1 + rng.below(6) as i64, false);
            }
            e.release(lane);
        }
        8 => {
            let (a, b) = (rng.tick(e), rng.tick(e));
            e.place_long(rng.below(lanes) as u8, a, b);
        }
        9 => e.set_tool(NoteTool::ALL[rng.below(5) as usize]),
        10 => e.copy(),
        11 => e.cut(),
        12 => e.paste(),
        13 => e.shift_selection(rng.below(5) as i64 - 2),
        14 => e.turn_selection([Turn::Mirror, Turn::Left, Turn::Right][rng.below(3) as usize]),
        15 => e.delete_selection(),
        16 => {
            let t = rng.tick(e);
            let segment = match rng.below(11) {
                0 => Segment::Bpm(60.0 + rng.below(20000) as f64 / 97.0),
                1 => Segment::Stop(rng.below(2000) as f64 / 1000.0),
                2 => Segment::Delay(rng.below(2000) as f64 / 1000.0),
                3 => Segment::Warp(Tick(rng.below(200) as i64)),
                4 => Segment::Speed {
                    ratio: rng.below(400) as f64 / 100.0,
                    delay: rng.below(8) as f64 / 2.0,
                    unit: if rng.below(2) == 0 {
                        SpeedUnit::Beats
                    } else {
                        SpeedUnit::Seconds
                    },
                },
                5 => Segment::Scroll(rng.below(400) as f64 / 100.0 - 1.0),
                6 => Segment::Fake(Tick(rng.below(200) as i64)),
                7 => Segment::TimeSignature(1 + rng.below(7) as u32, 4),
                8 => Segment::TickCount(rng.below(60) as u32),
                9 => Segment::Combo(rng.below(4) as u32, rng.below(4) as u32),
                _ => Segment::Label(format!("l{}", rng.below(10))),
            };
            e.set_segment(t, segment, retime);
        }
        17 => {
            let markers = e.segments();
            if !markers.is_empty() {
                let (t, s) = markers[rng.below(markers.len() as u64) as usize].clone();
                e.remove_segment(s.kind(), t, retime);
            }
        }
        18 => e.set_offset(rng.below(2000) as f64 / 1000.0 - 1.0, retime),
        19 => e.set_own_timing(rng.below(2) == 0),
        20 => {
            let mut info = e.chart_info();
            info.meter = rng.below(20) as u32;
            info.description = format!(" d{} ", rng.below(5));
            e.set_chart_info(info);
        }
        21 => match rng.below(3) {
            0 => e.add_chart(
                ddi_chart::BUILTIN_LAYOUTS[rng.below(7) as usize],
                Difficulty::ALL[rng.below(6) as usize],
            ),
            1 => e.remove_chart(),
            _ => {
                let n = e.song().charts.len() as u64;
                e.select_chart(rng.below(n) as usize);
            }
        },
        22 => e.undo(),
        _ => e.redo(),
    }
}

#[test]
fn random_edits_undo_to_the_song_and_read_back() {
    let songs: Vec<Song> = bundled()
        .into_iter()
        .chain(std::iter::once(
            Editor::new_song(None, "new").song().clone(),
        ))
        .collect();
    for (s, song) in songs.iter().enumerate() {
        for seed in 1..=12u64 {
            let mut rng = Rng((seed * 1000 + s as u64).wrapping_mul(0x9e37_79b9_7f4a_7c15) | 1);
            let mut e = Editor::open(song.clone(), 0);
            let start = e.song().clone();
            for step in 0..60 {
                random_edit(&mut rng, &mut e);
                assert_tidy(&e, &format!("song {s} seed {seed} step {step}"));
            }
            assert_round_trips(e.song(), &format!("song {s} seed {seed}"));
            let end = e.song().clone();
            let mut undone = 0;
            while e.can_undo() {
                e.undo();
                undone += 1;
            }
            assert!(e.song() == &start, "song {s} seed {seed}: undoing all");
            for _ in 0..undone {
                e.redo();
            }
            assert!(e.song() == &end, "song {s} seed {seed}: redoing all");
        }
    }
}

#[test]
fn a_song_read_back_edits_the_same() {
    for (s, song) in bundled().into_iter().enumerate() {
        let mut read_back = parse_ssc(&write_ssc(&song)).unwrap();
        read_back.source.format = song.source.format.clone();
        for seed in 1..=6u64 {
            let mut a = Editor::open(song.clone(), 0);
            let mut b = Editor::open(read_back.clone(), 0);
            let (mut ra, mut rb) = (Rng(seed * 7 + 1), Rng(seed * 7 + 1));
            for _ in 0..40 {
                random_edit(&mut ra, &mut a);
                random_edit(&mut rb, &mut b);
            }
            assert_eq!(
                write_ssc(a.song()),
                write_ssc(b.song()),
                "song {s} seed {seed}"
            );
        }
    }
}
