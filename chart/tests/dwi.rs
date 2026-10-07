//! Tests for the `.dwi` importer. There is no freely licensed DWI corpus,
//! so every fixture is hand-written from the readme and StepMania's
//! `NotesLoaderDWI.cpp`.

use std::path::Path;

use ddi_chart::formats::dwi::parse_dwi;
use ddi_chart::formats::sm::{parse_simfile, parse_sm};
use ddi_chart::{
    BpmSegment, Chart, Difficulty, DisplayBpm, NoteKind, Song, SourceFormat, StopSegment, Tick,
};

fn fixture(name: &str) -> String {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures")
        .join(name);
    std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("{}: {e}", path.display()))
}

fn approx(a: f64, b: f64) -> bool {
    (a - b).abs() < 1e-6
}

fn song(text: &str) -> Song {
    parse_dwi(text).unwrap()
}

/// The only chart of a one-chart file.
fn chart(text: &str) -> Chart {
    let song = song(text);
    assert_eq!(song.charts.len(), 1, "expected exactly one chart");
    song.charts.into_iter().next().unwrap()
}

/// `(tick, lane)` of every tap.
fn taps(chart: &Chart) -> Vec<(i64, u8)> {
    chart
        .notes
        .iter()
        .filter(|n| n.kind == NoteKind::Tap)
        .map(|n| (n.tick.0, n.lane))
        .collect()
}

/// `(tick, lane, end)` of every hold.
fn holds(chart: &Chart) -> Vec<(i64, u8, i64)> {
    chart
        .notes
        .iter()
        .filter_map(|n| match n.kind {
            NoteKind::HoldHead { end } => Some((n.tick.0, n.lane, end.0)),
            _ => None,
        })
        .collect()
}

fn single(steps: &str) -> Chart {
    chart(&format!("#SINGLE:BASIC:1:{steps};"))
}

const L: u8 = 0;
const D: u8 = 1;
const U: u8 = 2;
const R: u8 = 3;

// ---------------------------------------------------------------------------
// End to end: a `.dwi` and the equivalent `.sm`.
// ---------------------------------------------------------------------------

#[test]
fn dwi_and_equivalent_sm_import_identically() {
    let dwi = parse_dwi(&fixture("dwi_equivalent.dwi")).unwrap();
    let sm = parse_sm(&fixture("dwi_equivalent.sm")).unwrap();

    assert_eq!(dwi.source.format, SourceFormat::Dwi);
    assert_eq!(dwi.title, sm.title);
    assert_eq!(dwi.subtitle, sm.subtitle);
    assert_eq!(dwi.artist, sm.artist);
    assert_eq!(dwi.genre, sm.genre);
    assert_eq!(dwi.music, sm.music);
    assert_eq!(dwi.timing, sm.timing);

    assert_eq!(dwi.charts.len(), 1);
    let (d, s) = (&dwi.charts[0], &sm.charts[0]);
    assert_eq!(d.layout, s.layout);
    assert_eq!(d.difficulty, s.difficulty);
    assert_eq!(d.meter, s.meter);
    assert_eq!(d.notes, s.notes);

    // Spot checks against hand-computed values.
    assert_eq!(dwi.title, "Equivalence");
    assert_eq!(dwi.subtitle, "(Test Mix)");
    assert_eq!(d.notes.len(), 37);
    assert_eq!(
        holds(d),
        vec![(96, D, 192), (384, L, 528), (432, U, 480)],
        "holds end at the next note in the column, closed by taps and jumps"
    );
    // The hold on Down at tick 576 is never closed and disappears.
    assert!(!d.notes.iter().any(|n| n.tick == Tick(576)));

    let t = &dwi.timing;
    assert!(approx(t.offset_seconds, -0.12));
    assert_eq!(
        t.bpms,
        vec![
            BpmSegment {
                tick: Tick(0),
                bpm: 150.0
            },
            BpmSegment {
                tick: Tick(120), // DWI beat 10 = beat 2.5
                bpm: 180.0
            },
            BpmSegment {
                tick: Tick(384), // DWI beat 32 = beat 8
                bpm: 200.0
            },
        ]
    );
    assert_eq!(
        t.stops,
        vec![StopSegment {
            tick: Tick(576), // DWI beat 48 = beat 12
            seconds: 0.5
        }]
    );
    assert!(approx(t.seconds_at(Tick(0)), 0.12));
    assert!(approx(t.seconds_at(Tick(120)), 1.12));
    assert!(approx(t.seconds_at(Tick(384)), 1.12 + 5.5 / 3.0));
    let beat12 = 1.12 + 5.5 / 3.0 + 1.2;
    assert!(approx(t.seconds_at(Tick(576)), beat12));
    // The freeze holds beat 12 for half a second before the next 8th.
    assert!(approx(t.seconds_at(Tick(600)), beat12 + 0.5 + 0.15));

    // Ignored tags are kept, unparsed, for reference.
    let unknown: Vec<&str> = dwi
        .source
        .unknown_tags
        .iter()
        .map(|(k, _)| k.as_str())
        .collect();
    assert_eq!(unknown, vec!["MD5", "DISPLAYTITLE", "BACKGROUND", "END"]);
}

// ---------------------------------------------------------------------------
// Note grammar.
// ---------------------------------------------------------------------------

#[test]
fn each_character_is_an_eighth() {
    assert_eq!(
        taps(&single("4268")),
        vec![(0, L), (24, D), (48, R), (72, U)]
    );
}

#[test]
fn bracket_quantizations() {
    // ( ) 16ths
    assert_eq!(
        taps(&single("(8888)8")),
        vec![(0, U), (12, U), (24, U), (36, U), (48, U)]
    );
    // [ ] 24ths
    assert_eq!(
        taps(&single("[888]8")),
        vec![(0, U), (8, U), (16, U), (24, U)]
    );
    // { } 64ths
    assert_eq!(
        taps(&single("{8888}8")),
        vec![(0, U), (3, U), (6, U), (9, U), (12, U)]
    );
    // ` ' 192nds
    assert_eq!(
        taps(&single("`8888'8")),
        vec![(0, U), (1, U), (2, U), (3, U), (4, U)]
    );
    // Old-style 192nds: `<` with a `0` before the next `>`.
    assert_eq!(taps(&single("<0808>8")), vec![(1, U), (3, U), (4, U)]);
}

#[test]
fn brackets_do_not_nest_any_closer_resets_to_eighths() {
    // `(` 16th, `[` 24th, `]` back to 8ths although `(` is still open.
    assert_eq!(
        taps(&single("(8[8]8)8")),
        vec![(0, U), (12, U), (20, U), (44, U)]
    );
    // A closer without an opener just (re)sets 8ths.
    assert_eq!(taps(&single("8)8")), vec![(0, U), (24, U)]);
}

#[test]
fn empty_and_invalid_characters_advance_one_step() {
    // `0`, `5`, unknown characters and lower case all place nothing.
    assert_eq!(taps(&single("05xa8")), vec![(96, U)]);
}

#[test]
fn jumps() {
    assert_eq!(
        taps(&single("<2468>8")),
        vec![(0, L), (0, D), (0, U), (0, R), (24, U)]
    );
    // Two-member jump: both members are placed (StepMania 5.1.0 behaviour).
    assert_eq!(taps(&single("<24>")), vec![(0, L), (0, D)]);
    // A jump keeps the current quantization.
    assert_eq!(taps(&single("(<24>8)")), vec![(0, L), (0, D), (12, U)]);
    // Unterminated jump at the end of the data.
    assert_eq!(taps(&single("8<24")), vec![(0, U), (24, L), (24, D)]);
}

#[test]
fn hold_released_at_next_note_in_column() {
    let c = single("8!808");
    assert_eq!(holds(&c), vec![(0, U, 48)]);
    // The note that closes the hold is removed.
    assert!(taps(&c).is_empty());
}

#[test]
fn show_hold_pairs() {
    // 7!4: show Up+Left, hold only Left.
    let c = single("7!404");
    assert_eq!(holds(&c), vec![(0, L, 48)]);
    assert_eq!(taps(&c), vec![(0, U)]);
}

#[test]
fn hold_closed_by_jump_character_and_bracket_jump() {
    // `A` is Up+Down: Down closes the hold, Up stays a tap.
    let c = single("2!20A");
    assert_eq!(holds(&c), vec![(0, D, 48)]);
    assert_eq!(taps(&c), vec![(48, U)]);
    // A `<...>` jump closes it the same way.
    let c = single("2!20<26>");
    assert_eq!(holds(&c), vec![(0, D, 48)]);
    assert_eq!(taps(&c), vec![(48, R)]);
}

#[test]
fn holds_inside_jumps() {
    let c = single("<8!82>0A");
    assert_eq!(holds(&c), vec![(0, U, 48)]);
    assert_eq!(taps(&c), vec![(0, D), (48, D)]);
}

#[test]
fn hold_closed_by_another_hold_head() {
    // The second head is the first one's tail; the tap after it stays.
    let c = single("8!88!808");
    assert_eq!(holds(&c), vec![(0, U, 24)]);
    assert_eq!(taps(&c), vec![(72, U)]);
}

#[test]
fn unclosed_holds_are_dropped() {
    assert!(single("8!800").notes.is_empty());
    // Only the held panel disappears; the shown-only one stays a tap.
    let c = single("7!400");
    assert!(holds(&c).is_empty());
    assert_eq!(taps(&c), vec![(0, U)]);
    // `!` as the last character: nothing to hold.
    assert_eq!(taps(&single("88!")), vec![(0, U), (24, U)]);
}

#[test]
fn bang_quirks() {
    // A `!` not right after a step is skipped.
    assert_eq!(taps(&single("(8)!8")), vec![(0, U), (12, U)]);
    // The hold character is consumed whatever it is, even a bracket.
    assert_eq!(taps(&single("8!(88)")), vec![(0, U), (24, U), (48, U)]);
    // A later member of the same jump overwrites a head on the same panel.
    let c = single("<7!4B>");
    assert!(holds(&c).is_empty());
    assert_eq!(taps(&c), vec![(0, L), (0, U), (0, R)]);
}

// ---------------------------------------------------------------------------
// Steps types and column maps.
// ---------------------------------------------------------------------------

#[test]
fn solo_column_map() {
    // Solo lanes: L UL D U UR R = 0..5.
    let c = chart("#SOLO:MANIAC:7:4C28D6;");
    assert_eq!(c.layout, "dance-solo");
    assert_eq!(
        taps(&c),
        vec![(0, 0), (24, 1), (48, 2), (72, 3), (96, 4), (120, 5)]
    );
    // Two-panel solo letters, one per 8th.
    let c = chart("#SOLO:MANIAC:7:EFGHIJKLM;");
    let rows: Vec<Vec<u8>> = (0..9)
        .map(|i| {
            c.notes
                .iter()
                .filter(|n| n.tick == Tick(24 * i))
                .map(|n| n.lane)
                .collect()
        })
        .collect();
    assert_eq!(
        rows,
        vec![
            vec![0, 1], // E: L + UL
            vec![1, 2], // F: UL + D
            vec![1, 3], // G: UL + U
            vec![1, 5], // H: UL + R
            vec![0, 4], // I: L + UR
            vec![2, 4], // J: D + UR
            vec![3, 4], // K: U + UR
            vec![4, 5], // L: UR + R
            vec![1, 4], // M: UL + UR
        ]
    );
    // The readme's example: four ways to write L+UL+UR+R.
    for jump in ["<MB>", "<LE>", "<IH>", "<46M>"] {
        let c = chart(&format!("#SOLO:MANIAC:7:{jump}0;"));
        assert_eq!(taps(&c), vec![(0, 0), (0, 1), (0, 4), (0, 5)], "{jump}");
    }
}

#[test]
fn double_and_couple_pads() {
    let song = song("#DOUBLE:ANOTHER:5:4268:B0A;\n#COUPLE:BASIC:2:8:2!202;");
    let d = &song.charts[0];
    assert_eq!(d.layout, "dance-double");
    assert_eq!(d.difficulty, Difficulty::Medium);
    assert_eq!(
        taps(d),
        vec![
            (0, 0),
            (0, 4),
            (0, 7),
            (24, 1),
            (48, 3),
            (48, 5),
            (48, 6),
            (72, 2)
        ]
    );
    // Pad 1 data shorter than two characters is fine when pad 2 has data.
    let c = &song.charts[1];
    assert_eq!(c.layout, "dance-couple");
    assert_eq!(taps(c), vec![(0, 2)]);
    assert_eq!(holds(c), vec![(0, 5, 48)]);
}

#[test]
fn second_pad_read_only_as_last_parameter() {
    // A stray extra parameter: StepMania ignores pad 2 (`iNumParams != 5`).
    let c = chart("#DOUBLE:BASIC:3:44:66:x;");
    assert_eq!(taps(&c), vec![(0, 0), (24, 0)]);
}

#[test]
fn panels_missing_from_the_layout_are_dropped() {
    // Solo letters in a single chart: only the four-panel part survives.
    assert_eq!(taps(&single("CH8")), vec![(24, R), (48, U)]);
    // A second pad on a single chart is ignored.
    let c = chart("#SINGLE:BASIC:1:88:22;");
    assert_eq!(taps(&c), vec![(0, U), (24, U)]);
}

#[test]
fn whitespace_inside_steps_is_ignored() {
    assert_eq!(
        taps(&single("4 2\n\t6\r\n8")),
        vec![(0, L), (24, D), (48, R), (72, U)]
    );
}

// ---------------------------------------------------------------------------
// Chart metadata.
// ---------------------------------------------------------------------------

#[test]
fn difficulty_aliases_and_meter_fallback() {
    let song = song(
        "#SINGLE:BEGINNER:1:88;#SINGLE:BASIC:2:88;#SINGLE:ANOTHER:5:88;\
         #SINGLE:MANIAC:8:88;#SINGLE:SMANIAC:10:88;#SINGLE:light:2:88;\
         #SINGLE:WILD:1:88;#SINGLE:WILD:3:88;#SINGLE:WILD:6:88;#SINGLE:WILD:7:88;\
         #SINGLE:WILD::88;#SINGLE:WILD:0:88;",
    );
    let got: Vec<(Difficulty, u32)> = song
        .charts
        .iter()
        .map(|c| (c.difficulty, c.meter))
        .collect();
    assert_eq!(
        got,
        vec![
            (Difficulty::Beginner, 1),
            (Difficulty::Easy, 2),
            (Difficulty::Medium, 5),
            (Difficulty::Hard, 8),
            (Difficulty::Challenge, 10),
            (Difficulty::Easy, 2),
            (Difficulty::Beginner, 1),
            (Difficulty::Easy, 3),
            (Difficulty::Medium, 6),
            (Difficulty::Hard, 7),
            // An empty meter is 1.
            (Difficulty::Beginner, 1),
            // Meter 0 is "≤ 3".
            (Difficulty::Easy, 0),
        ]
    );
}

#[test]
fn chart_tags_are_case_insensitive() {
    let c = chart("#single:basic:1:88;");
    assert_eq!(c.layout, "dance-single");
    assert_eq!(c.difficulty, Difficulty::Easy);
}

#[test]
fn charts_without_data_are_skipped() {
    let song = song("#SINGLE:BASIC:1:8;#SINGLE:BASIC:1:;#SINGLE:BASIC:1;#DOUBLE:BASIC:1:8:8;");
    assert!(song.charts.is_empty());
}

// ---------------------------------------------------------------------------
// Header tags and timing.
// ---------------------------------------------------------------------------

#[test]
fn header_tags() {
    let song = song(
        "#TITLE:Song -Long Version-;#ARTIST:Someone;#GENRE:Pop,Rock;\
         #CDTITLE:cd.png;#FILE:.\\music\\song.mp3;#DISPLAYBPM:90..180;\
         #STATUS:NEW;#RANDSEED:3;",
    );
    assert_eq!(song.title, "Song");
    assert_eq!(song.subtitle, "-Long Version-");
    assert_eq!(song.artist, "Someone");
    assert_eq!(song.genre, "Pop,Rock");
    assert_eq!(song.cd_title.as_deref(), Some("cd.png"));
    // Backslashes survive: DWI files are read without unescaping.
    assert_eq!(song.music.as_deref(), Some(".\\music\\song.mp3"));
    assert_eq!(song.display_bpm, DisplayBpm::Range(90.0, 180.0));
    assert_eq!(song.source.unknown_tags.len(), 2);
    assert!(song.charts.is_empty());

    assert_eq!(song_display_bpm("*"), DisplayBpm::Random);
    assert_eq!(song_display_bpm("150"), DisplayBpm::Single(150.0));
    assert_eq!(
        parse_dwi("#TITLE:x;").unwrap().display_bpm,
        DisplayBpm::Actual
    );
}

fn song_display_bpm(v: &str) -> DisplayBpm {
    song(&format!("#DISPLAYBPM:{v};")).display_bpm
}

#[test]
fn gap_is_milliseconds_before_beat_zero() {
    // Positive GAP: beat 0 comes later in the music (negative SM offset).
    let t = song("#BPM:120;#GAP:1500;").timing;
    assert!(approx(t.offset_seconds, -1.5));
    assert!(approx(t.seconds_at(Tick(0)), 1.5));
    // Negative GAP: beat 0 before the music starts.
    let t = song("#BPM:120;#GAP:-250;").timing;
    assert!(approx(t.offset_seconds, 0.25));
    assert!(approx(t.seconds_at(Tick(0)), -0.25));
    // Integer milliseconds (`StringToInt`): fractions are cut.
    let t = song("#BPM:120;#GAP:1500.9;").timing;
    assert!(approx(t.offset_seconds, -1.5));
}

#[test]
fn bpm_changes_and_freezes_use_quarter_beats() {
    let t = song("#BPM:120;#GAP:0;#CHANGEBPM:1=240,6=60;#FREEZE:2=250,6=1000;").timing;
    assert_eq!(
        t.bpms,
        vec![
            BpmSegment {
                tick: Tick(0),
                bpm: 120.0
            },
            BpmSegment {
                tick: Tick(12), // quarter beat 1 = a 16th note
                bpm: 240.0
            },
            BpmSegment {
                tick: Tick(72), // quarter beat 6 = beat 1.5
                bpm: 60.0
            },
        ]
    );
    assert_eq!(
        t.stops,
        vec![
            StopSegment {
                tick: Tick(24),
                seconds: 0.25
            },
            StopSegment {
                tick: Tick(72),
                seconds: 1.0
            },
        ]
    );
    // 0.25 beat @120 = 0.125 s; 0.25 beat @240 = 0.0625 s; stop 0.25 s;
    // 1 beat @240 = 0.25 s; stop 1 s; then 60 BPM.
    assert!(approx(t.seconds_at(Tick(12)), 0.125));
    assert!(approx(t.seconds_at(Tick(24)), 0.1875));
    assert!(approx(t.seconds_at(Tick(72)), 0.1875 + 0.25 + 0.25));
    assert!(approx(t.seconds_at(Tick(120)), 0.6875 + 1.0 + 1.0));
}

#[test]
fn bpm_tag_variants() {
    // `#BPMCHANGE` is an alias; a later value on the same row wins; BPMs
    // that are not positive are ignored.
    let t = song("#BPM:150;#BPMCHANGE:0=175,8=0,8=-5,16=200;#BPM:0;").timing;
    assert_eq!(
        t.bpms,
        vec![
            BpmSegment {
                tick: Tick(0),
                bpm: 175.0
            },
            BpmSegment {
                tick: Tick(192),
                bpm: 200.0
            },
        ]
    );
    // No BPM at all: 60 like StepMania's default.
    assert_eq!(song("#TITLE:x;").timing.bpms[0].bpm, 60.0);
    // A freeze set twice on one row keeps the later value; zero deletes it.
    let t = song("#BPM:120;#FREEZE:4=500,4=250,8=100;#FREEZE:8=0;").timing;
    assert_eq!(
        t.stops,
        vec![StopSegment {
            tick: Tick(48),
            seconds: 0.25
        }]
    );
}

#[test]
fn sample_timestamps_in_all_syntaxes() {
    let s = song("#SAMPLESTART:5230;#SAMPLELENGTH:15000;");
    assert!(approx(s.preview_start, 5.23));
    assert!(approx(s.preview_length, 15.0));
    let s = song("#SAMPLESTART:5.23;#SAMPLELENGTH:12.5;");
    assert!(approx(s.preview_start, 5.23));
    assert!(approx(s.preview_length, 12.5));
    let s = song("#SAMPLESTART:1:05.23;#SAMPLELENGTH:0:15;");
    assert!(approx(s.preview_start, 65.23));
    assert!(approx(s.preview_length, 15.0));
    let s = song("#SAMPLESTART:1:02:03.5;");
    assert!(approx(s.preview_start, 3723.5));
    // StepMania ignores the readme's `+` (factor in GAP) prefix.
    let s = song("#GAP:500;#SAMPLESTART:+5230;");
    assert!(approx(s.preview_start, 5.23));
    // Lengths in (0, 1) seconds are multiplied by 1000 (StepMania quirk).
    assert!(approx(song("#SAMPLELENGTH:0.015;").preview_length, 15.0));
    assert!(approx(song("#SAMPLELENGTH:500;").preview_length, 500.0));
}

#[test]
fn simfile_dispatch_routes_dwi() {
    let s = parse_simfile("#TITLE:x;#SINGLE:BASIC:1:88;", ".dwi").unwrap();
    assert_eq!(s.source.format, SourceFormat::Dwi);
    assert_eq!(s.charts.len(), 1);
}
