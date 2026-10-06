//! Fixture-based tests for the `.sm`/`.ssc` importers.
//!
//! Set `DDI_EXTRA_SIMFILES=<dir>` to additionally parse every `.sm`/`.ssc`
//! found (recursively) under that directory.

use std::path::{Path, PathBuf};

use ddi_chart::formats::sm::{parse_simfile, parse_sm, parse_ssc};
use ddi_chart::{
    Difficulty, DisplayBpm, EffectTime, Note, NoteKind, Quantization, Song, SourceFormat,
    SpeedUnit, Tick, WarpSegment,
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

fn note(notes: &[Note], tick: Tick, lane: u8) -> &Note {
    notes
        .iter()
        .find(|n| n.tick == tick && n.lane == lane)
        .unwrap_or_else(|| panic!("no note at {tick:?} lane {lane}"))
}

#[test]
fn all_notes_header() {
    let song = parse_sm(&fixture("all_notes.sm")).unwrap();
    assert_eq!(song.title, "All Notes");
    assert_eq!(song.subtitle, "fixture");
    assert_eq!(song.artist, "Nobody");
    assert_eq!(song.title_translit, "All Notes TL");
    assert_eq!(song.subtitle_translit, "fixture TL");
    assert_eq!(song.artist_translit, "Nobody TL");
    assert_eq!(song.genre, "Test");
    assert_eq!(song.credit, "ddi");
    assert_eq!(song.banner.as_deref(), Some("bn.png"));
    assert_eq!(song.background.as_deref(), Some("bg.png"));
    assert_eq!(song.cd_title.as_deref(), Some("cd.png"));
    assert_eq!(song.music.as_deref(), Some("song.ogg"));
    assert_eq!(song.jacket, None);
    assert!(approx(song.preview_start, 12.5));
    assert!(approx(song.preview_length, 10.0));
    assert_eq!(song.display_bpm, DisplayBpm::Range(100.0, 200.0));
    assert_eq!(song.keysounds, vec!["kick.ogg", "snare.ogg", "hat.ogg"]);
    assert_eq!(song.source.format, SourceFormat::Sm);
    assert_eq!(
        song.source.unknown_tags,
        vec![("MENUCOLOR".to_string(), "1,1,1".to_string())]
    );

    // Timing: offset as-is, BPM change, FREEZES alias, delay.
    let t = &song.timing;
    assert!(approx(t.offset_seconds, -0.25));
    assert_eq!(t.bpms.len(), 2);
    assert_eq!(t.bpms[1].tick, Tick::from_beats(8));
    assert_eq!(t.bpms[1].bpm, 240.0);
    assert_eq!(t.stops.len(), 1);
    assert_eq!(t.stops[0].tick, Tick::from_beats(4));
    assert!(approx(t.stops[0].seconds, 0.5));
    assert_eq!(t.delays.len(), 1);
    assert_eq!(t.delays[0].tick, Tick::from_beats(12));
    // beat 0 at 0.25 s; beat 4 at 2.25 s (before its stop); beat 8 at 4.75 s;
    // beat 12 at 5.75 s + 0.25 s delay.
    assert!(approx(t.seconds_at_beat(0.0), 0.25));
    assert!(approx(t.seconds_at_beat(4.0), 2.25));
    assert!(approx(t.seconds_at_beat(8.0), 4.75));
    assert!(approx(t.seconds_at_beat(12.0), 6.0));

    // Effects: 2 bgchanges on layer 0, 1 on layer 1, 1 fgchange, 2 attacks.
    let bg: Vec<_> = song
        .effects
        .iter()
        .filter(|e| e.kind == "bgchange")
        .collect();
    assert_eq!(bg.len(), 3);
    assert_eq!(bg[0].at, EffectTime::Beat(Tick::ZERO));
    assert_eq!(bg[0].layer, 0);
    assert_eq!(bg[0].fields.len(), 11);
    assert_eq!(bg[0].fields[1], "bg1.png");
    assert_eq!(bg[1].fields[6], "StretchNoLoop");
    assert_eq!(bg[2].layer, 1);
    assert_eq!(
        bg[2].fields,
        vec!["4.000", "layer2.png", "1.000", "0", "0", "1"]
    );
    let fg: Vec<_> = song
        .effects
        .iter()
        .filter(|e| e.kind == "fgchange")
        .collect();
    assert_eq!(fg.len(), 1);
    assert_eq!(fg[0].at, EffectTime::Beat(Tick::from_beats(16)));
    let attacks: Vec<_> = song.effects.iter().filter(|e| e.kind == "attack").collect();
    assert_eq!(attacks.len(), 2);
    assert_eq!(attacks[0].at, EffectTime::Seconds(1.0));
    assert_eq!(attacks[0].fields, vec!["drunk", "2"]);
    assert_eq!(attacks[1].at, EffectTime::Seconds(4.0));
    assert_eq!(attacks[1].fields, vec!["*4 dizzy", "2"]);
}

#[test]
fn all_notes_chart() {
    let song = parse_sm(&fixture("all_notes.sm")).unwrap();
    assert_eq!(song.charts.len(), 1);
    let c = &song.charts[0];
    assert_eq!(c.layout, "dance-single");
    assert_eq!(c.difficulty, Difficulty::Hard);
    assert_eq!(c.meter, 9);
    assert_eq!(c.description, "ddi");
    assert_eq!(c.name, "ddi");
    assert_eq!(c.credit, "ddi");
    assert!(c.timing.is_none());
    assert_eq!(c.display_bpm, None);

    let n = &c.notes;
    assert_eq!(n.len(), 15);
    assert!(
        n.windows(2)
            .all(|w| (w[0].tick, w[0].lane) < (w[1].tick, w[1].lane))
    );

    // Measure 0: 4ths.
    assert_eq!(note(n, Tick(0), 0).kind, NoteKind::Tap);
    assert_eq!(note(n, Tick(48), 1).kind, NoteKind::Mine);
    assert_eq!(note(n, Tick(96), 2).kind, NoteKind::Lift);
    assert_eq!(note(n, Tick(144), 3).kind, NoteKind::Fake);
    // Measure 1: 8ths; hold and roll close in measure 2 (12ths).
    assert_eq!(
        note(n, Tick(192), 0).kind,
        NoteKind::HoldHead { end: Tick(384) }
    );
    assert_eq!(
        note(n, Tick(216), 1).kind,
        NoteKind::RollHead {
            end: Tick(384 + 16)
        }
    );
    let k = note(n, Tick(240), 2);
    assert_eq!(k.kind, NoteKind::AutoKeysound);
    assert_eq!(k.keysound, Some(2));
    let t = note(n, Tick(264), 3);
    assert_eq!(t.kind, NoteKind::Tap);
    assert_eq!(t.keysound, Some(1));
    // 'A' (attack note) is empty in StepMania 5.1 and so here.
    assert!(!n.iter().any(|x| x.tick == Tick(288) && x.lane == 0));
    // Measure 2: 12ths.
    let t = note(n, Tick(384 + 11 * 16), 3);
    assert_eq!(t.tick.quantization(), Quantization::N12);
    // Measure 3: 16ths.
    assert_eq!(note(n, Tick(576), 0).kind, NoteKind::Tap);
    assert_eq!(
        note(n, Tick(576 + 15 * 12), 3).tick.quantization(),
        Quantization::N16
    );
    // Measure 4: 24ths.
    assert_eq!(note(n, Tick(768), 1).kind, NoteKind::Tap);
    assert_eq!(
        note(n, Tick(768 + 23 * 8), 2).tick.quantization(),
        Quantization::N24
    );
    // Measure 5: 192nds.
    assert_eq!(note(n, Tick(960), 0).kind, NoteKind::Tap);
    assert_eq!(
        note(n, Tick(960 + 191), 3).tick.quantization(),
        Quantization::N192
    );

    assert_eq!(c.judged_note_count(), 12);
}

#[test]
fn negative_bpm_fixture() {
    let song = parse_sm(&fixture("negative_bpm.sm")).unwrap();
    let t = &song.timing;
    assert_eq!(
        t.warps,
        vec![WarpSegment {
            tick: Tick::from_beats(4),
            length: Tick::from_beats(4),
        }]
    );
    assert_eq!(t.bpms.len(), 1);
    assert_eq!(t.bpms[0].bpm, 120.0);
    // Notes in measure 1 (beats 4..8) are skipped; measure 2 is judged at
    // the same time measure 1 would have started.
    let c = &song.charts[0];
    assert_eq!(c.notes.len(), 12);
    for n in &c.notes[4..8] {
        assert!(
            !t.judgeable(n.tick),
            "{:?} should be inside the warp",
            n.tick
        );
    }
    for n in c.notes[..4].iter().chain(&c.notes[8..]) {
        assert!(t.judgeable(n.tick));
    }
    assert!(approx(t.seconds_at(Tick::from_beats(4)), 2.0));
    assert!(approx(t.seconds_at(Tick::from_beats(8)), 2.0));
    assert!(approx(t.seconds_at(Tick::from_beats(9)), 2.5));
    assert_eq!(c.difficulty, Difficulty::Challenge);
}

#[test]
fn negative_stop_fixture() {
    let song = parse_sm(&fixture("negative_stop.sm")).unwrap();
    let t = &song.timing;
    assert_eq!(
        t.warps,
        vec![WarpSegment {
            tick: Tick::from_beats(4),
            length: Tick::from_beats(2),
        }]
    );
    assert_eq!(t.stops.len(), 1);
    assert_eq!(t.stops[0].tick, Tick::from_beats(8));
    assert!(!t.judgeable(Tick::from_beats(5)));
    assert!(t.judgeable(Tick::from_beats(6)));
    // beat 6 at the same time as beat 4 (2 s), beat 8 at 3 s, beat 9 at 4 s.
    assert!(approx(t.seconds_at_beat(6.0), 2.0));
    assert!(approx(t.seconds_at_beat(8.0), 3.0));
    assert!(approx(t.seconds_at_beat(9.0), 4.0));
}

#[test]
fn dwi_aliases_layouts_and_routine() {
    let song = parse_sm(&fixture("dwi_aliases.sm")).unwrap();
    let diffs: Vec<_> = song
        .charts
        .iter()
        .map(|c| (c.layout.as_str(), c.difficulty))
        .collect();
    assert_eq!(
        diffs,
        vec![
            ("dance-single", Difficulty::Easy),
            ("dance-single", Difficulty::Medium),
            ("dance-single", Difficulty::Hard),
            ("dance-single", Difficulty::Challenge), // Hard + "SMANIAC" description
            ("dance-single", Difficulty::Challenge), // oni
            ("dance-single", Difficulty::Beginner),
            ("dance-single", Difficulty::Edit),
            ("dance-solo", Difficulty::Medium),
            ("dance-routine", Difficulty::Medium),
            ("pump-single", Difficulty::Medium),
        ]
    );
    // Empty meter → 1.
    assert_eq!(song.charts[6].meter, 1);
    // Solo: six lanes.
    let solo = song.chart_by("dance-solo", Difficulty::Medium).unwrap();
    let lanes: Vec<u8> = solo.notes.iter().map(|n| n.lane).collect();
    assert_eq!(lanes, vec![0, 5, 1, 4, 2, 3]);
    // Routine keeps player 1 only.
    let routine = song.chart_by("dance-routine", Difficulty::Medium).unwrap();
    assert_eq!(routine.notes.len(), 1);
    assert_eq!(routine.notes[0].lane, 0);
    // Pump chart kept with its id but not playable.
    let pump = song.chart_by("pump-single", Difficulty::Medium).unwrap();
    assert_eq!(pump.notes.len(), 2);
    assert_eq!(pump.notes[1].lane, 4);
    let playable: Vec<_> = song.playable_charts().map(|c| c.layout.as_str()).collect();
    assert_eq!(playable.len(), 8);
    assert!(!playable.contains(&"pump-single"));
    assert!(!playable.contains(&"dance-routine"));
    assert!(song.chart_by("dance-single", Difficulty::Medium).is_some());
    assert!(song.chart_by("dance-double", Difficulty::Medium).is_none());
}

#[test]
fn ssc_split_timing() {
    let song = parse_ssc(&fixture("split_timing.ssc")).unwrap();
    assert_eq!(song.source.format, SourceFormat::Ssc { version: 0.83 });
    assert_eq!(song.jacket.as_deref(), Some("jacket.png"));
    let t = &song.timing;
    assert!(approx(t.offset_seconds, -0.1));
    assert_eq!(t.bpms.len(), 1);
    assert_eq!(t.stops.len(), 1);
    assert_eq!(t.time_signatures.len(), 2);
    assert_eq!(t.time_signatures[1].numerator, 3);
    assert_eq!(t.tick_counts.len(), 1);
    assert_eq!(t.tick_counts[0].ticks_per_beat, 4);
    assert_eq!(t.combos.len(), 2);
    assert_eq!((t.combos[1].combo, t.combos[1].miss_combo), (2, 1));
    assert_eq!((t.combos[0].combo, t.combos[0].miss_combo), (1, 1));
    assert_eq!(t.speeds.len(), 1);
    assert_eq!(t.scrolls.len(), 2);
    assert_eq!(t.fakes.len(), 1);
    assert_eq!(t.fakes[0].length, Tick::from_beats(1));
    assert_eq!(t.labels.len(), 2);
    assert_eq!(t.labels[1].text, "Drop");
    assert!(approx(t.displayed_beat(12.0), 10.0));
    assert!(!t.judgeable(Tick::from_beats(12)));
    assert!(t.judgeable(Tick::from_beats(13)));
    assert_eq!(song.effects.len(), 1);

    assert_eq!(song.charts.len(), 2);
    let a = &song.charts[0];
    assert_eq!(a.name, "Shared timing");
    assert_eq!(a.description, "desc one");
    assert_eq!(a.credit, "alice");
    assert_eq!(a.difficulty, Difficulty::Easy);
    assert_eq!(a.meter, 3);
    assert!(a.timing.is_none());
    assert!(std::ptr::eq(a.timing(&song), &song.timing));

    let b = &song.charts[1];
    assert_eq!(b.name, "desc two"); // empty #CHARTNAME falls back
    assert_eq!(b.credit, "");
    assert_eq!(b.display_bpm, Some(DisplayBpm::Range(90.0, 180.0)));
    let bt = b.timing.as_ref().expect("own timing");
    assert!(approx(bt.offset_seconds, 0.3));
    assert_eq!(bt.bpms.len(), 2);
    assert_eq!(bt.bpms[1].bpm, 180.0);
    // The song's stop/fakes/scrolls are NOT inherited.
    assert!(bt.stops.is_empty());
    assert!(bt.fakes.is_empty());
    assert!(bt.scrolls.is_empty());
    assert_eq!(
        bt.warps,
        vec![WarpSegment {
            tick: Tick::from_beats(8),
            length: Tick::from_beats(2),
        }]
    );
    assert!(std::ptr::eq(b.timing(&song), bt));
    assert_eq!(
        song.source.unknown_tags,
        vec![("NOTEDATA[1].METERF".to_string(), "12.5".to_string())]
    );
}

#[test]
fn ssc_old_version_absolute_warps_and_no_split_timing() {
    let song = parse_ssc(&fixture("old_warps.ssc")).unwrap();
    assert_eq!(song.source.format, SourceFormat::Ssc { version: 0.6 });
    assert_eq!(
        song.timing.warps,
        vec![WarpSegment {
            tick: Tick::from_beats(4),
            length: Tick::from_beats(2),
        }]
    );
    let c = &song.charts[0];
    assert!(c.timing.is_none(), "pre-0.70 charts have no split timing");
    assert_eq!(c.name, "old chart", "chart name falls back to description");
}

#[test]
fn ssc_speeds_padding() {
    let song = parse_ssc(&fixture("speeds.ssc")).unwrap();
    let s = &song.timing.speeds;
    assert_eq!(s.len(), 3, "negative delay entry is dropped");
    assert_eq!(
        (s[0].ratio, s[0].delay, s[0].unit),
        (1.0, 0.0, SpeedUnit::Beats)
    );
    assert_eq!(
        (s[1].ratio, s[1].delay, s[1].unit),
        (2.0, 1.0, SpeedUnit::Beats)
    );
    assert_eq!(
        (s[2].ratio, s[2].delay, s[2].unit),
        (0.5, 2.0, SpeedUnit::Seconds)
    );
}

fn collect_simfiles(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(rd) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in rd.flatten() {
        let p = entry.path();
        if p.is_dir() {
            collect_simfiles(&p, out);
        } else if let Some(ext) = p.extension().and_then(|e| e.to_str())
            && (ext.eq_ignore_ascii_case("sm") || ext.eq_ignore_ascii_case("ssc"))
        {
            out.push(p);
        }
    }
}

fn describe(song: &Song) -> String {
    let mut s = String::new();
    let (lo, hi) = song.timing.bpm_range();
    s.push_str(&format!(
        "  song BPM {lo}..{hi}, {} charts\n",
        song.charts.len()
    ));
    for c in &song.charts {
        let t = c.timing(song);
        let (lo, hi) = t.bpm_range();
        s.push_str(&format!(
            "  {:<14} {:<9?} meter {:>2} notes {:>5} judged {:>5} bpm {lo}..{hi}{}\n",
            c.layout,
            c.difficulty,
            c.meter,
            c.notes.len(),
            c.judged_note_count(),
            if c.timing.is_some() {
                " (own timing)"
            } else {
                ""
            }
        ));
    }
    let mut unknown: Vec<&str> = song
        .source
        .unknown_tags
        .iter()
        .map(|(k, _)| k.split_once('.').map(|(_, t)| t).unwrap_or(k))
        .collect();
    unknown.sort_unstable();
    unknown.dedup();
    s.push_str(&format!("  unknown tags: {}\n", unknown.join(" ")));
    s
}

/// Parses every simfile under `$DDI_EXTRA_SIMFILES` (if set).
#[test]
fn extra_simfiles_from_env() {
    let Ok(dir) = std::env::var("DDI_EXTRA_SIMFILES") else {
        return;
    };
    let mut files = Vec::new();
    collect_simfiles(Path::new(&dir), &mut files);
    files.sort();
    assert!(!files.is_empty(), "no .sm/.ssc files under {dir}");
    for path in files {
        let text = std::fs::read_to_string(&path).unwrap();
        let ext = path.extension().unwrap().to_str().unwrap();
        let song = parse_simfile(&text, ext).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
        eprintln!("{}\n{}", path.display(), describe(&song));
        assert!(
            song.playable_charts().next().is_some(),
            "{}: no playable chart",
            path.display()
        );
        for c in song.playable_charts() {
            assert!(!c.notes.is_empty(), "{}: empty chart", path.display());
        }
    }
}
