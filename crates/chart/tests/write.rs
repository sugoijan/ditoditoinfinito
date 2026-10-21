//! The `.ssc`/`.sm` writer against the readers: every song read, written
//! and read again is the same song (`formats::sm_write`).
//!
//! Covers the bundled songs, the chart fixtures, DWI files converted to
//! `.ssc`, and generated songs using every field the writer writes. Set
//! `DDI_WRITE_CORPUS=<dir>[:<dir>…]` to round-trip every `.sm`/`.ssc`
//! under local folders too (one-off checks; third-party files stay local).

use std::path::{Path, PathBuf};

use ddi_chart::formats::dwi::parse_dwi;
use ddi_chart::formats::sm::{parse_sm, parse_ssc};
use ddi_chart::formats::sm_write::{SSC_VERSION, sm_losses, write_sm, write_ssc};
use ddi_chart::{
    BpmSegment, Chart, ComboSegment, Difficulty, DisplayBpm, FakeSegment, Label, Note, NoteKind,
    ScrollSegment, Song, SourceFormat, SourceInfo, SpeedSegment, SpeedUnit, StopSegment, Tick,
    TickCount, TimeSignature, TimingMap, WarpSegment,
};

fn repo() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn read(path: &Path) -> String {
    let bytes = std::fs::read(path).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
    String::from_utf8_lossy(&bytes).into_owned()
}

/// `song` as `.ssc` reads it back: the writer's version, and none of the
/// tags kept as written that `.ssc` reads as its own (a `.sm`'s
/// `#COMBOS`, which StepMania ignores there).
fn as_ssc(mut song: Song) -> Song {
    song.source.format = SourceFormat::Ssc {
        version: SSC_VERSION,
    };
    song.source.unknown_tags.retain(|(k, _)| k != "COMBOS");
    song
}

/// Writes `song` as `.ssc`, reads it back and expects it unchanged.
fn assert_ssc_round_trip(song: &Song, what: &str) {
    let text = write_ssc(song);
    let back = parse_ssc(&text).unwrap_or_else(|e| panic!("{what}: {e}\n{text}"));
    let expected = as_ssc(song.clone());
    if back != expected {
        panic!(
            "{what}: .ssc round trip differs\n{}\n{text}",
            diff(&expected, &back)
        );
    }
    // Writing what was read gives the same text.
    assert_eq!(write_ssc(&back), text, "{what}: second write differs");
}

/// The same for `.sm`, when nothing would be lost.
fn assert_sm_round_trip(song: &Song, what: &str) -> bool {
    if !sm_losses(song).is_empty() {
        return false;
    }
    let text = write_sm(song);
    let back = parse_sm(&text).unwrap_or_else(|e| panic!("{what}: {e}\n{text}"));
    let mut expected = song.clone();
    expected.source.format = SourceFormat::Sm;
    if back != expected {
        panic!(
            "{what}: .sm round trip differs\n{}\n{text}",
            diff(&expected, &back)
        );
    }
    true
}

/// The first field that differs, for a readable failure.
fn diff(a: &Song, b: &Song) -> String {
    macro_rules! field {
        ($($f:ident),*) => {$(
            if a.$f != b.$f {
                return format!("{}: {:?}\n  vs {:?}", stringify!($f), a.$f, b.$f);
            }
        )*};
    }
    field!(
        title,
        subtitle,
        artist,
        title_translit,
        subtitle_translit,
        artist_translit,
        genre
    );
    field!(
        credit,
        music,
        preview_start,
        preview_length,
        banner,
        background,
        jacket,
        cd_title
    );
    field!(timing, display_bpm, effects, keysounds, layouts, source);
    if a.charts.len() != b.charts.len() {
        return format!("chart count {} vs {}", a.charts.len(), b.charts.len());
    }
    for (i, (x, y)) in a.charts.iter().zip(&b.charts).enumerate() {
        if x != y {
            let notes = x
                .notes
                .iter()
                .zip(&y.notes)
                .find(|(n, m)| n != m)
                .map(|(n, m)| format!("first note {n:?}\n  vs {m:?}"))
                .unwrap_or_else(|| format!("notes {} vs {}", x.notes.len(), y.notes.len()));
            let mut x = x.clone();
            let mut y = y.clone();
            x.notes.clear();
            y.notes.clear();
            return format!("chart {i}: {x:?}\n  vs {y:?}\n{notes}");
        }
    }
    "equal".to_string()
}

#[test]
fn bundled_songs_round_trip() {
    let mut count = 0;
    for entry in std::fs::read_dir(repo().join("assets/songs")).unwrap() {
        let dir = entry.unwrap().path();
        for file in std::fs::read_dir(&dir).unwrap() {
            let path = file.unwrap().path();
            if path.extension().is_some_and(|e| e == "ssc") {
                let song = parse_ssc(&read(&path)).unwrap();
                assert_ssc_round_trip(&song, &path.display().to_string());
                count += 1;
            }
        }
    }
    assert!(count >= 4, "bundled songs found: {count}");
}

#[test]
fn fixtures_round_trip() {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures");
    let mut sm_written = 0;
    for file in std::fs::read_dir(&dir).unwrap() {
        let path = file.unwrap().path();
        let what = path.display().to_string();
        let text = read(&path);
        match path.extension().and_then(|e| e.to_str()) {
            Some("sm") => {
                let song = parse_sm(&text).unwrap();
                assert_ssc_round_trip(&song, &what);
                sm_written += usize::from(assert_sm_round_trip(&song, &what));
            }
            Some("ssc") => {
                let song = parse_ssc(&text).unwrap();
                assert_ssc_round_trip(&song, &what);
                assert_sm_round_trip(&song, &what);
            }
            Some("dwi") => {
                // A DWI song converted: its timing and charts survive.
                let song = parse_dwi(&text).unwrap();
                let back = parse_ssc(&write_ssc(&song)).unwrap();
                assert_eq!(back.timing, song.timing, "{what}");
                assert_eq!(back.charts, song.charts, "{what}");
                assert_eq!(back.title, song.title, "{what}");
            }
            _ => {}
        }
    }
    assert!(sm_written >= 3, ".sm fixtures written as .sm: {sm_written}");
}

#[test]
fn sm_losses_name_what_sm_cannot_say() {
    let song = parse_ssc(&read(
        &Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/split_timing.ssc"),
    ))
    .unwrap();
    let losses = sm_losses(&song);
    assert!(
        losses
            .iter()
            .any(|l| matches!(l, ddi_chart::formats::sm_write::SmLoss::SplitTiming { .. })),
        "{losses:?}"
    );
    let song = parse_sm(&read(
        &Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/negative_bpm.sm"),
    ))
    .unwrap();
    assert!(
        sm_losses(&song)
            .iter()
            .any(|l| l.describe().contains("warps")),
        "negative BPMs read as warps"
    );
}

#[test]
fn generated_charts_are_not_written() {
    let mut song = generated(&mut Rng(7), true);
    let original = song.clone();
    let mut derived = song.charts[0].clone();
    derived.derivation = Some(ddi_chart::simplify::Derivation::WithoutOnigiri { source: 0 });
    song.charts.push(derived);
    assert_eq!(write_ssc(&song), write_ssc(&original));
}

// ---------------------------------------------------------------------------
// Generated songs.
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

    fn chance(&mut self, p: f64) -> bool {
        self.unit() < p
    }

    fn unit(&mut self) -> f64 {
        (self.next() >> 11) as f64 / (1u64 << 53) as f64
    }

    /// Any value in `[lo, hi)`, all its bits (not a short decimal).
    fn float(&mut self, lo: f64, hi: f64) -> f64 {
        lo + (hi - lo) * self.unit()
    }

    fn pick<'a, T>(&mut self, items: &'a [T]) -> &'a T {
        &items[self.below(items.len() as u64) as usize]
    }

    /// Text with what files must escape (`:`, `;`, `#`, `//`, `\`), as
    /// the reader keeps it: trimmed.
    fn text(&mut self) -> String {
        const PIECES: [&str; 14] = [
            "a",
            "Song",
            " ",
            ":",
            ";",
            "#",
            "/",
            "//",
            "\\",
            "=",
            ",",
            "ドリーム",
            "(",
            "ä",
        ];
        let n = self.below(8);
        let s: String = (0..n).map(|_| *self.pick(&PIECES)).collect();
        s.trim().to_string()
    }

    fn opt_text(&mut self) -> Option<String> {
        Some(self.text()).filter(|s| !s.is_empty())
    }

    /// Strictly increasing ticks from `first`, each `step` apart at most.
    fn ticks(&mut self, first: i64, count: u64, step: i64) -> Vec<Tick> {
        let mut t = first;
        (0..count)
            .map(|_| {
                let tick = Tick(t);
                t += 1 + self.below(step as u64) as i64;
                tick
            })
            .collect()
    }
}

const LAYOUTS: [&str; 6] = [
    "dance-single",
    "dance-double",
    "pump-single",
    "kb7-single",
    "lights-cabinet",
    "dance-solo",
];

fn display_bpm(rng: &mut Rng) -> DisplayBpm {
    match rng.below(4) {
        0 => DisplayBpm::Actual,
        1 => DisplayBpm::Single(rng.float(60.0, 300.0)),
        2 => DisplayBpm::Range(rng.float(60.0, 150.0), rng.float(150.0, 300.0)),
        _ => DisplayBpm::Random,
    }
}

/// A timing map as the readers make them; `ssc` adds what only `.ssc`
/// says.
fn timing(rng: &mut Rng, ssc: bool) -> TimingMap {
    let mut t = TimingMap::constant(rng.float(60.0, 250.0), rng.float(-1.0, 1.0));
    let n = rng.below(5);
    for (i, tick) in rng.ticks(0, n + 1, 400).into_iter().enumerate() {
        let bpm = rng.float(1.0, 400.0);
        if i == 0 {
            t.bpms[0].bpm = bpm;
        } else {
            t.bpms.push(BpmSegment { tick, bpm });
        }
    }
    let n = rng.below(4);
    let first = rng.below(100) as i64;
    for tick in rng.ticks(first, n, 300) {
        t.stops.push(StopSegment {
            tick,
            seconds: rng.float(0.001, 2.0),
        });
    }
    let n = rng.below(3);
    let first = rng.below(100) as i64;
    for tick in rng.ticks(first, n, 300) {
        t.delays.push(StopSegment {
            tick,
            seconds: rng.float(0.001, 2.0),
        });
    }
    if rng.chance(0.5) {
        t.time_signatures.push(TimeSignature {
            tick: Tick::ZERO,
            numerator: 4,
            denominator: 4,
        });
        let n = rng.below(3);
        let first = 1 + rng.below(100) as i64;
        for tick in rng.ticks(first, n, 400) {
            t.time_signatures.push(TimeSignature {
                tick,
                numerator: 1 + rng.below(12) as u32,
                denominator: *rng.pick(&[2, 4, 8, 16]),
            });
        }
    }
    let n = rng.below(3);
    for tick in rng.ticks(0, n, 400) {
        t.tick_counts.push(TickCount {
            tick,
            ticks_per_beat: rng.below(49) as u32,
        });
    }
    if !ssc {
        return t;
    }
    let n = rng.below(3);
    for tick in rng.ticks(0, n, 400) {
        t.warps.push(WarpSegment {
            tick,
            length: Tick(1 + rng.below(200) as i64),
        });
    }
    let n = rng.below(3);
    for tick in rng.ticks(0, n, 400) {
        t.speeds.push(SpeedSegment {
            tick,
            ratio: rng.float(0.0, 4.0),
            delay: rng.float(0.0, 4.0),
            unit: *rng.pick(&[SpeedUnit::Beats, SpeedUnit::Seconds]),
        });
    }
    let n = rng.below(3);
    for tick in rng.ticks(0, n, 400) {
        t.scrolls.push(ScrollSegment {
            tick,
            ratio: rng.float(-2.0, 4.0),
        });
    }
    let n = rng.below(3);
    for tick in rng.ticks(0, n, 400) {
        t.fakes.push(FakeSegment {
            tick,
            length: Tick(1 + rng.below(200) as i64),
        });
    }
    let n = rng.below(3);
    for tick in rng.ticks(0, n, 400) {
        let combo = rng.below(5) as u32;
        t.combos.push(ComboSegment {
            tick,
            combo,
            miss_combo: rng.below(5) as u32,
        });
    }
    let n = rng.below(3);
    for tick in rng.ticks(0, n, 400) {
        let text = rng.text().replace([',', '='], "").trim().to_string();
        t.labels.push(Label { tick, text });
    }
    t
}

/// Notes as a chart holds them: sorted, one per cell, nothing inside a
/// hold's span in its lane but its tail.
fn notes(rng: &mut Rng, lanes: u8, keysounds: usize) -> Vec<Note> {
    let mut out = Vec::new();
    for lane in 0..lanes {
        let mut t = rng.below(200) as i64;
        for _ in 0..rng.below(40) {
            let kind = match rng.below(10) {
                0..=4 => NoteKind::Tap,
                5 => NoteKind::HoldHead {
                    end: Tick(t + 1 + rng.below(300) as i64),
                },
                6 => NoteKind::RollHead {
                    end: Tick(t + 1 + rng.below(300) as i64),
                },
                7 => NoteKind::Mine,
                8 => *rng.pick(&[NoteKind::Lift, NoteKind::Fake]),
                _ => NoteKind::AutoKeysound,
            };
            let mut note = Note::new(Tick(t), lane, kind);
            if keysounds > 0 && rng.chance(0.2) {
                note.keysound = Some(rng.below(keysounds as u64) as u16);
            }
            let end = note.end_tick().0;
            out.push(note);
            t = end + 1 + rng.below(100) as i64;
        }
    }
    out.sort_by_key(|n| (n.tick, n.lane));
    out
}

fn generated(rng: &mut Rng, ssc: bool) -> Song {
    let keysounds: Vec<String> = (0..rng.below(3)).map(|i| format!("k{i}.ogg")).collect();
    let mut charts = Vec::new();
    for _ in 0..1 + rng.below(4) {
        let layout = rng.pick(&LAYOUTS).to_string();
        let lanes = ddi_chart::Layout::builtin(&layout).map_or(6, |l| l.lane_count()) as u8;
        let description = rng.text();
        let (name, credit) = if ssc && rng.chance(0.7) {
            (rng.text(), rng.text())
        } else {
            (description.clone(), description.clone())
        };
        // `.ssc` falls back to the description for an empty name.
        let name = if name.is_empty() {
            description.clone()
        } else {
            name
        };
        let mut difficulty = *rng.pick(&Difficulty::ALL);
        if difficulty == Difficulty::Hard
            && (description.eq_ignore_ascii_case("smaniac")
                || description.eq_ignore_ascii_case("challenge"))
        {
            difficulty = Difficulty::Challenge;
        }
        charts.push(Chart {
            derivation: None,
            layout,
            difficulty,
            meter: rng.below(20) as u32,
            name,
            description,
            credit,
            notes: notes(rng, lanes, keysounds.len()),
            timing: (ssc && rng.chance(0.3)).then(|| timing(rng, true)),
            display_bpm: if ssc && rng.chance(0.3) {
                Some(display_bpm(rng)).filter(|b| *b != DisplayBpm::Actual)
            } else {
                None
            },
            danoni: None,
        });
    }
    let mut unknown_tags = Vec::new();
    if rng.chance(0.5) {
        // Kept as written: parameters joined by `:`, each trimmed.
        let mut param = || rng.text().replace(':', "").trim().to_string();
        let value = format!("{}:{}", param(), param());
        unknown_tags.push(("ORIGIN".to_string(), value));
    }
    if ssc && rng.chance(0.5) {
        unknown_tags.push(("NOTEDATA[0].MUSIC".to_string(), "other.ogg".to_string()));
    }
    Song {
        title: rng.text(),
        subtitle: rng.text(),
        artist: rng.text(),
        title_translit: rng.text(),
        subtitle_translit: rng.text(),
        artist_translit: rng.text(),
        genre: rng.text(),
        credit: rng.text(),
        music: rng.opt_text(),
        preview_start: rng.float(0.0, 100.0),
        preview_length: rng.float(0.0, 30.0),
        banner: rng.opt_text(),
        background: rng.opt_text(),
        jacket: if ssc { rng.opt_text() } else { None },
        cd_title: rng.opt_text(),
        timing: timing(rng, ssc),
        display_bpm: display_bpm(rng),
        charts,
        effects: Vec::new(),
        keysounds,
        layouts: Vec::new(),
        source: SourceInfo {
            format: SourceFormat::Ssc {
                version: SSC_VERSION,
            },
            unknown_tags,
        },
    }
}

#[test]
fn generated_songs_round_trip() {
    let mut sm_written = 0;
    for seed in 1..=400u64 {
        let mut rng = Rng(seed.wrapping_mul(0x9e37_79b9_7f4a_7c15) | 1);
        let ssc = seed % 2 == 0;
        let song = generated(&mut rng, ssc);
        assert_ssc_round_trip(&song, &format!("seed {seed}"));
        sm_written += usize::from(assert_sm_round_trip(&song, &format!("seed {seed}")));
    }
    assert!(
        sm_written >= 150,
        "generated songs written as .sm: {sm_written}"
    );
}

#[test]
fn local_corpus_round_trips() {
    let Ok(dirs) = std::env::var("DDI_WRITE_CORPUS") else {
        return;
    };
    let mut files = Vec::new();
    for dir in std::env::split_paths(&dirs) {
        collect(&dir, &mut files);
    }
    let (mut ok, mut sm) = (0, 0);
    for path in &files {
        let what = path.display().to_string();
        let text = read(path);
        let song = match path
            .extension()
            .and_then(|e| e.to_str())
            .map(str::to_ascii_lowercase)
        {
            Some(e) if e == "sm" => parse_sm(&text),
            _ => parse_ssc(&text),
        };
        let Ok(song) = song else {
            continue;
        };
        assert_ssc_round_trip(&song, &what);
        sm += usize::from(assert_sm_round_trip(&song, &what));
        ok += 1;
    }
    eprintln!(
        "{ok} of {} files round-tripped ({sm} as .sm too)",
        files.len()
    );
}

fn collect(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            collect(&path, out);
        } else if path
            .extension()
            .and_then(|e| e.to_str())
            .is_some_and(|e| e.eq_ignore_ascii_case("sm") || e.eq_ignore_ascii_case("ssc"))
        {
            out.push(path);
        }
    }
}
