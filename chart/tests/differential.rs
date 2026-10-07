//! Differential tests: the `.sm` importer against two independent parsers.
//!
//! - `danceparser` reads every chart of a file: note grid, steps type,
//!   difficulty, meter, `#BPMS` and `#STOPS`. It is the structural oracle.
//!   Its `NoteKind` names `3` and `4` the wrong way round (`3` is a hold/roll
//!   tail and `4` a roll head in StepMania); only the characters are compared.
//! - `rgchart` computes note times (whole milliseconds, `f32` beats) through
//!   BPM changes and stops, but reads only the last `#NOTES` block, assumes
//!   four columns, and applies a stop *before* a note on the stop's own beat
//!   (StepMania pauses after it: that is a delay). It is the timing oracle;
//!   notes on a stop row are expected to differ by exactly the stop.
//!
//! Neither reads `.ssc`, delays, warps or negative BPMs, so the generated
//! files stay inside what all three understand; those features have their
//! own fixture tests (`simfiles.rs`).
//!
//! Set `DDI_DIFF_CORPUS=<dir>` to also compare every `.sm` under a directory
//! (recursively), e.g. a local song folder; files outside the generated
//! subset (negative BPMs, warps, delays) skip the timing comparison.

use std::collections::BTreeMap;
use std::io::Cursor;
use std::path::{Path, PathBuf};

use danceparser::{NoteKind as DpKind, SMChart};
use ddi_chart::formats::sm::parse_sm;
use ddi_chart::{NoteKind, Song, TICKS_PER_BEAT, Tick};
use rgchart::KeyType;

// ---------------------------------------------------------------------------
// Generated simfiles.
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
        (self.next() >> 11) as f64 / (1u64 << 53) as f64 <= p
    }

    fn pick<'a, T>(&mut self, items: &'a [T]) -> &'a T {
        &items[self.below(items.len() as u64) as usize]
    }

    /// Uniform in `[lo, hi)`, rounded to `decimals`.
    fn decimal(&mut self, lo: f64, hi: f64, decimals: i32) -> f64 {
        let x = lo + (hi - lo) * (self.next() >> 11) as f64 / (1u64 << 53) as f64;
        let scale = 10f64.powi(decimals);
        (x * scale).round() / scale
    }
}

/// Every row count StepMania writes.
const ROW_COUNTS: [usize; 9] = [4, 8, 12, 16, 24, 32, 48, 64, 192];
const DIFFICULTIES: [&str; 6] = ["Beginner", "Easy", "Medium", "Hard", "Challenge", "Edit"];

fn beat_text(tick: i64) -> String {
    format!("{:.6}", tick as f64 / TICKS_PER_BEAT as f64)
}

/// A random but well-formed `.sm`: positive BPMs and stops on the 48-tick
/// grid, closed holds and rolls with nothing else in their column until the
/// tail. `doubles` allows `dance-double` charts; `charts` is the count.
fn generate(rng: &mut Rng, charts: usize, doubles: bool) -> String {
    let measures = 6 + rng.below(10) as i64;
    let last_tick = measures * 4 * TICKS_PER_BEAT;
    let mut bpms = vec![format!("0.000000={:.3}", rng.decimal(80.0, 220.0, 3))];
    let mut tick = 0;
    for _ in 0..rng.below(6) {
        tick += 4 * (1 + rng.below(40) as i64);
        if tick >= last_tick {
            break;
        }
        bpms.push(format!(
            "{}={:.3}",
            beat_text(tick),
            rng.decimal(60.0, 300.0, 3)
        ));
    }
    let mut stops = Vec::new();
    let mut tick = 0;
    for _ in 0..rng.below(5) {
        // Quarter beats, so stops often share a row with notes.
        tick += 12 * (1 + rng.below(24) as i64);
        if tick >= last_tick {
            break;
        }
        stops.push(format!(
            "{}={:.3}",
            beat_text(tick),
            rng.decimal(0.05, 1.2, 3)
        ));
    }
    let mut text = format!(
        "#TITLE:Generated;\n#ARTIST:Differential;\n#OFFSET:{:.3};\n#BPMS:{};\n#STOPS:{};\n",
        rng.decimal(-0.5, 0.5, 3),
        bpms.join(","),
        stops.join(",")
    );
    for _ in 0..charts {
        let lanes = if doubles && rng.chance(0.3) { 8 } else { 4 };
        let style = if lanes == 8 {
            "dance-double"
        } else {
            "dance-single"
        };
        text.push_str(&format!(
            "#NOTES:\n     {style}:\n     generated:\n     {}:\n     {}:\n     0,0,0,0,0:\n",
            rng.pick(&DIFFICULTIES),
            1 + rng.below(15)
        ));
        let mut open = vec![false; lanes];
        let mut blocks = Vec::new();
        for m in 0..measures {
            let rows = *rng.pick(&ROW_COUNTS);
            let last_measure = m == measures - 1;
            let mut lines = Vec::new();
            for r in 0..rows {
                let last_row = last_measure && r == rows - 1;
                let line: String = open
                    .iter_mut()
                    .map(|open| {
                        if *open {
                            if last_row || rng.chance(0.08) {
                                *open = false;
                                '3'
                            } else {
                                '0'
                            }
                        } else {
                            let c = cell(rng, !last_measure);
                            *open = c == '2' || c == '4';
                            c
                        }
                    })
                    .collect();
                lines.push(line);
            }
            blocks.push(lines.join("\n"));
        }
        text.push_str(&blocks.join("\n,\n"));
        text.push_str("\n;\n");
    }
    text
}

fn cell(rng: &mut Rng, may_open: bool) -> char {
    let r = rng.below(100);
    match r {
        0..=74 => '0',
        75..=86 => '1',
        87..=89 if may_open => '2',
        90..=91 if may_open => '4',
        92..=94 => 'M',
        95..=96 => 'L',
        97..=98 => 'F',
        99 => 'K',
        _ => '1',
    }
}

// ---------------------------------------------------------------------------
// Comparisons.
// ---------------------------------------------------------------------------

/// `(tick, lane) → note character` as StepMania writes it.
type Grid = BTreeMap<(i64, u8), char>;

/// Our notes as the characters that produced them; tails come from the
/// hold/roll ends.
fn our_grid(notes: &[ddi_chart::Note]) -> Grid {
    let mut grid = Grid::new();
    for n in notes {
        let c = match n.kind {
            NoteKind::Tap => '1',
            NoteKind::HoldHead { end } => {
                grid.insert((end.0, n.lane), '3');
                '2'
            }
            NoteKind::RollHead { end } => {
                grid.insert((end.0, n.lane), '3');
                '4'
            }
            NoteKind::Mine => 'M',
            NoteKind::Lift => 'L',
            NoteKind::Fake => 'F',
            NoteKind::AutoKeysound => 'K',
            other => panic!("unexpected note kind from .sm: {other:?}"),
        };
        grid.insert((n.tick.0, n.lane), c);
    }
    grid
}

/// danceparser's rows as characters. Its `RollHead` is StepMania's `3`
/// and its `Tail` is `4` (see the module docs).
fn oracle_grid(notes: &danceparser::NotesData) -> Grid {
    let mut grid = Grid::new();
    for (m, measure) in notes.measures.iter().enumerate() {
        let rows = measure.rows.len() as i64;
        for (r, row) in measure.rows.iter().enumerate() {
            let tick = m as i64 * 4 * TICKS_PER_BEAT + r as i64 * 4 * TICKS_PER_BEAT / rows;
            for (lane, kind) in row.columns.iter().enumerate() {
                let c = match kind {
                    DpKind::Empty => continue,
                    DpKind::Tap => '1',
                    DpKind::HoldHead => '2',
                    DpKind::RollHead => '3',
                    DpKind::Tail => '4',
                    DpKind::Mine => 'M',
                    DpKind::AutoKeysounds => 'K',
                    DpKind::Lift => 'L',
                    DpKind::Fake => 'F',
                };
                grid.insert((tick, lane as u8), c);
            }
        }
    }
    grid
}

fn ticks(beat: f64) -> i64 {
    (beat * TICKS_PER_BEAT as f64).round() as i64
}

/// Compares charts, notes, BPMs and stops against danceparser. Returns the
/// differences, empty when they agree.
fn compare_structure(ours: &Song, oracle: &SMChart, exact_names: bool) -> Vec<String> {
    let mut diffs = Vec::new();
    let theirs: Vec<_> = oracle
        .notes
        .iter()
        .filter(|n| ours.charts.iter().any(|c| c.layout == n.style.trim()))
        .collect();
    if theirs.len() != ours.charts.len() {
        diffs.push(format!(
            "chart count: ours {} vs danceparser {}",
            ours.charts.len(),
            theirs.len()
        ));
        return diffs;
    }
    for (i, (c, t)) in ours.charts.iter().zip(theirs).enumerate() {
        if c.layout != t.style.trim() {
            diffs.push(format!("chart {i}: layout {} vs {}", c.layout, t.style));
        }
        if exact_names && format!("{:?}", c.difficulty) != t.difficulty.trim() {
            diffs.push(format!(
                "chart {i}: difficulty {:?} vs {}",
                c.difficulty, t.difficulty
            ));
        }
        if c.meter != u32::from(t.chart_meter) {
            diffs.push(format!("chart {i}: meter {} vs {}", c.meter, t.chart_meter));
        }
        let (a, b) = (our_grid(&c.notes), oracle_grid(t));
        if a != b {
            let only_ours: Vec<_> = a
                .iter()
                .filter(|(k, v)| b.get(k) != Some(v))
                .take(5)
                .collect();
            let only_theirs: Vec<_> = b
                .iter()
                .filter(|(k, v)| a.get(k) != Some(v))
                .take(5)
                .collect();
            diffs.push(format!(
                "chart {i} ({}): notes differ; ours only {only_ours:?}, danceparser only {only_theirs:?}",
                c.layout
            ));
        }
    }
    let our_bpms: Vec<_> = ours.timing.bpms.iter().map(|b| (b.tick.0, b.bpm)).collect();
    let their_bpms: Vec<_> = oracle.bpms.iter().map(|b| (ticks(b.beat), b.bpm)).collect();
    if !same_segments(&our_bpms, &their_bpms) {
        diffs.push(format!("bpms: ours {our_bpms:?} vs {their_bpms:?}"));
    }
    let our_stops: Vec<_> = ours
        .timing
        .stops
        .iter()
        .map(|s| (s.tick.0, s.seconds))
        .collect();
    let their_stops: Vec<_> = oracle
        .stops
        .iter()
        .map(|s| (ticks(s.beat), s.length.as_secs_f64()))
        .collect();
    if !same_segments(&our_stops, &their_stops) {
        diffs.push(format!("stops: ours {our_stops:?} vs {their_stops:?}"));
    }
    diffs
}

fn same_segments(a: &[(i64, f64)], b: &[(i64, f64)]) -> bool {
    a.len() == b.len()
        && a.iter()
            .zip(b)
            .all(|(x, y)| x.0 == y.0 && (x.1 - y.1).abs() < 1e-6)
}

/// rgchart truncates to whole milliseconds and computes in `f32`.
const TIMING_TOLERANCE_MS: f64 = 2.0;

/// What a timing comparison covered.
#[derive(Default)]
struct Timed {
    notes: usize,
    on_stop_rows: usize,
}

/// Compares a chart's note times against rgchart, which reads only the
/// last `#NOTES` of `text` (four columns only).
fn compare_timing(
    ours: &Song,
    chart: &ddi_chart::Chart,
    text: &str,
    covered: &mut Timed,
) -> Vec<String> {
    let oracle = match rgchart::parse::from_sm_generic(text) {
        Ok(c) => c,
        Err(e) => return vec![format!("rgchart failed: {e}")],
    };
    let timing = chart.timing(ours);
    let grid = our_grid(&chart.notes);
    let stop_at = |tick: i64| {
        timing
            .stops
            .iter()
            .filter(|s| s.tick.0 == tick)
            .map(|s| s.seconds)
            .sum::<f64>()
    };
    let mut diffs = Vec::new();
    let mut seen = 0;
    for o in oracle.hitobjects.iter() {
        let tick = ticks(f64::from(o.beat));
        let lane = o.lane - 1;
        let Some(&c) = grid.get(&(tick, lane)) else {
            diffs.push(format!(
                "rgchart note at tick {tick} lane {lane} not in ours"
            ));
            continue;
        };
        seen += 1;
        let expected_kind = match c {
            '1' => KeyType::Normal,
            '2' | '4' => KeyType::SliderStart,
            '3' => KeyType::SliderEnd,
            'M' => KeyType::Mine,
            'F' => KeyType::Fake,
            _ => KeyType::Unknown,
        };
        if o.key.key_type != expected_kind {
            diffs.push(format!(
                "tick {tick} lane {lane}: kind {:?} vs ours {c}",
                o.key.key_type
            ));
        }
        covered.notes += 1;
        if stop_at(tick) > 0.0 {
            covered.on_stop_rows += 1;
        }
        // rgchart pauses for a stop before the note on its row.
        let ours_ms = (timing.seconds_at(Tick(tick)) + stop_at(tick)) * 1000.0;
        if (ours_ms - f64::from(o.time)).abs() > TIMING_TOLERANCE_MS {
            diffs.push(format!(
                "tick {tick} lane {lane}: {ours_ms:.3} ms vs rgchart {} ms",
                o.time
            ));
        }
    }
    if seen != grid.len() {
        diffs.push(format!(
            "note count: ours {} vs rgchart {seen} matched",
            grid.len()
        ));
    }
    diffs
}

fn parse_oracle(text: &str) -> Result<SMChart, String> {
    SMChart::from_sm(Cursor::new(text.as_bytes())).map_err(|e| format!("{e:?}"))
}

// ---------------------------------------------------------------------------
// Tests.
// ---------------------------------------------------------------------------

const GENERATED_FILES: u64 = 300;

#[test]
fn generated_structure_matches_danceparser() {
    let mut kinds = BTreeMap::<char, usize>::new();
    let mut doubles = 0;
    for seed in 1..=GENERATED_FILES {
        let mut rng = Rng(seed.wrapping_mul(0x9e37_79b9_7f4a_7c15) | 1);
        let charts = 1 + rng.below(4) as usize;
        let text = generate(&mut rng, charts, true);
        let ours = parse_sm(&text).unwrap_or_else(|e| panic!("seed {seed}: {e}"));
        let oracle =
            parse_oracle(&text).unwrap_or_else(|e| panic!("seed {seed}: danceparser: {e}"));
        let diffs = compare_structure(&ours, &oracle, true);
        assert!(
            diffs.is_empty(),
            "seed {seed}:\n{}\n\n{text}",
            diffs.join("\n")
        );
        for c in &ours.charts {
            doubles += usize::from(c.layout == "dance-double");
            for k in our_grid(&c.notes).into_values() {
                *kinds.entry(k).or_default() += 1;
            }
        }
    }
    // The comparison is only as good as what the generator produced.
    for k in ['1', '2', '3', '4', 'M', 'L', 'F', 'K'] {
        assert!(
            kinds.get(&k).copied().unwrap_or(0) >= 50,
            "too few `{k}`: {kinds:?}"
        );
    }
    assert!(doubles >= 50, "too few doubles charts: {doubles}");
}

#[test]
fn generated_timing_matches_rgchart() {
    let mut covered = Timed::default();
    for seed in 1..=GENERATED_FILES {
        let mut rng = Rng(seed.wrapping_mul(0xd1b5_4a32_d192_ed03) | 1);
        // rgchart reads the last chart only and four columns only.
        let text = generate(&mut rng, 1, false);
        let ours = parse_sm(&text).unwrap_or_else(|e| panic!("seed {seed}: {e}"));
        let chart = ours.charts.last().expect("one chart");
        let diffs = compare_timing(&ours, chart, &text, &mut covered);
        assert!(
            diffs.is_empty(),
            "seed {seed}:\n{}\n\n{text}",
            diffs.join("\n")
        );
    }
    assert!(
        covered.notes >= 10_000,
        "only {} notes timed",
        covered.notes
    );
    assert!(
        covered.on_stop_rows >= 50,
        "only {} notes on stop rows",
        covered.on_stop_rows
    );
}

/// The one repository `.sm` fixture inside the oracles' subset. Left out:
/// `all_notes.sm` (`#ATTACKS`, `#DELAYS` and a `#DISPLAYBPM` range, which
/// danceparser rejects), `dwi_aliases.sm` (empty meters, which StepMania
/// reads as 1 and danceparser rejects) and the negative BPM/stop fixtures
/// (warps).
#[test]
fn fixture_matches_danceparser() {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/dwi_equivalent.sm");
    let text = std::fs::read_to_string(&path).unwrap();
    let ours = parse_sm(&text).unwrap();
    let oracle = parse_oracle(&text).unwrap();
    let diffs = compare_structure(&ours, &oracle, false);
    assert!(diffs.is_empty(), "{}", diffs.join("\n"));
}

/// Opt-in: compares every `.sm` under `DDI_DIFF_CORPUS`. Files danceparser
/// rejects are counted, not failed (it is strict about malformed numbers).
#[test]
fn local_corpus() {
    let Some(dir) = std::env::var_os("DDI_DIFF_CORPUS") else {
        return;
    };
    let mut files = Vec::new();
    collect(Path::new(&dir), "sm", &mut files);
    files.sort();
    let (mut compared, mut timed, mut oracle_failed) = (0, 0, 0);
    let mut covered = Timed::default();
    let mut failures = Vec::new();
    for path in &files {
        let bytes = std::fs::read(path).unwrap();
        let text = String::from_utf8_lossy(&bytes).replace("\r\n", "\n");
        let ours = match parse_sm(&text) {
            Ok(s) => s,
            Err(e) => {
                failures.push(format!("{}: ours failed: {e}", path.display()));
                continue;
            }
        };
        match parse_oracle(&text) {
            Ok(oracle) => {
                compared += 1;
                let diffs = compare_structure(&ours, &oracle, false);
                if !diffs.is_empty() {
                    failures.push(format!("{}:\n  {}", path.display(), diffs.join("\n  ")));
                }
            }
            Err(e) => {
                oracle_failed += 1;
                eprintln!("{}: danceparser failed: {e}", path.display());
            }
        }
        if let Some((chart, single)) = first_single_chart(&ours, &text) {
            timed += 1;
            let diffs = compare_timing(&ours, chart, &single, &mut covered);
            if !diffs.is_empty() {
                failures.push(format!(
                    "{} (timing):\n  {}",
                    path.display(),
                    diffs.into_iter().take(8).collect::<Vec<_>>().join("\n  ")
                ));
            }
        }
    }
    eprintln!(
        "{} files: {compared} structure-compared, {timed} timing-compared ({} notes, {} on stop rows), {oracle_failed} rejected by danceparser",
        files.len(),
        covered.notes,
        covered.on_stop_rows
    );
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

/// The first `dance-single` chart and a copy of `text` with only its
/// `#NOTES` block (rgchart reads the last one), when the song is inside
/// rgchart's subset: positive BPMs and stops, no delays, warps or split
/// timing.
fn first_single_chart<'a>(song: &'a Song, text: &str) -> Option<(&'a ddi_chart::Chart, String)> {
    let t = &song.timing;
    let upper = text.to_ascii_uppercase();
    if upper.contains("#DELAYS:")
        || upper.contains("#WARPS:")
        || !t.warps.is_empty()
        || !t.delays.is_empty()
        || t.bpms.iter().any(|b| b.bpm <= 0.0)
        || t.stops.iter().any(|s| s.seconds <= 0.0)
    {
        return None;
    }
    let chart = song.charts.iter().find(|c| c.layout == "dance-single")?;
    if chart.timing.is_some() {
        return None;
    }
    // `#NOTES:` blocks run to the next `;`.
    let starts: Vec<usize> = upper.match_indices("#NOTES:").map(|(i, _)| i).collect();
    let block = starts.iter().find_map(|&i| {
        let end = text[i..].find(';').map_or(text.len(), |e| i + e + 1);
        let block = &text[i..end];
        let style = block["#NOTES:".len()..].split(':').next()?.trim();
        (style == "dance-single").then_some(block)
    })?;
    Some((chart, format!("{}{block}\n", &text[..*starts.first()?])))
}

/// Files with extension `ext` under `dir`, recursively.
fn collect(dir: &Path, ext: &str, out: &mut Vec<PathBuf>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for e in entries.flatten() {
        let p = e.path();
        if p.is_dir() {
            collect(&p, ext, out);
        } else if p.extension().is_some_and(|x| x.eq_ignore_ascii_case(ext)) {
            out.push(p);
        }
    }
}

/// Opt-in: parses every `.dwi` under `DDI_DIFF_CORPUS`, and where a song
/// folder also holds a `.sm` (usually a conversion of the same song),
/// compares the two imports chart by chart: the same notes on the same
/// ticks, and BPMs, stops and offset in the same places with the same
/// values up to the converter's rounding. No third-party parser reads DWI, so the `.sm`
/// importer serves as the oracle here.
#[test]
fn local_dwi_against_sm() {
    let Some(dir) = std::env::var_os("DDI_DIFF_CORPUS") else {
        return;
    };
    let mut dwis = Vec::new();
    collect(Path::new(&dir), "dwi", &mut dwis);
    dwis.sort();
    /// Half the last digit of a three-decimal value.
    const ROUNDING: f64 = 0.0005 + 1e-9;
    let (mut parsed, mut pairs, mut charts) = (0, 0, 0);
    let mut failures = Vec::new();
    let mut edited = Vec::new();
    for path in &dwis {
        let read =
            |p: &Path| String::from_utf8_lossy(&std::fs::read(p).unwrap()).replace("\r\n", "\n");
        let dwi = match ddi_chart::formats::dwi::parse_dwi(&read(path)) {
            Ok(s) => s,
            Err(e) => {
                failures.push(format!("{}: {e}", path.display()));
                continue;
            }
        };
        parsed += 1;
        let mut sms = Vec::new();
        collect(path.parent().unwrap(), "sm", &mut sms);
        let Some(sm_path) = sms.first() else {
            continue;
        };
        let Ok(sm) = parse_sm(&read(sm_path)) else {
            continue;
        };
        pairs += 1;
        for d in &dwi.charts {
            let Some(s) = sm
                .charts
                .iter()
                .find(|s| s.layout == d.layout && s.difficulty == d.difficulty)
            else {
                failures.push(format!(
                    "{}: {} {:?} missing from the .sm",
                    path.display(),
                    d.layout,
                    d.difficulty
                ));
                continue;
            };
            charts += 1;
            let (a, b) = (our_grid(&d.notes), our_grid(&s.notes));
            if a != b {
                let only_dwi: Vec<_> = a
                    .iter()
                    .filter(|(k, v)| b.get(k) != Some(v))
                    .take(4)
                    .collect();
                let only_sm: Vec<_> = b
                    .iter()
                    .filter(|(k, v)| a.get(k) != Some(v))
                    .take(4)
                    .collect();
                failures.push(format!(
                    "{}: {} {:?}: notes differ ({} vs {}); dwi only {only_dwi:?}, sm only {only_sm:?}",
                    path.display(),
                    d.layout,
                    d.difficulty,
                    a.len(),
                    b.len()
                ));
                continue;
            }
            // Positions must agree exactly (quarter beats, GAP sign, ms → s);
            // values may differ by the converter's three-decimal rounding.
            // Larger value differences are edits to the converted file:
            // reported, not failed.
            let (td, ts) = (d.timing(&dwi), s.timing(&sm));
            let segs = |t: &ddi_chart::TimingMap| {
                let bpms: Vec<_> = t.bpms.iter().map(|b| (b.tick.0, b.bpm)).collect();
                let stops: Vec<_> = t.stops.iter().map(|s| (s.tick.0, s.seconds)).collect();
                (bpms, stops)
            };
            let ((db, dst), (sb, sst)) = (segs(td), segs(ts));
            let label = format!("{}: {} {:?}", path.display(), d.layout, d.difficulty);
            for (what, a, b) in [("bpms", &db, &sb), ("stops", &dst, &sst)] {
                if a.len() != b.len() || a.iter().zip(b.iter()).any(|(x, y)| x.0 != y.0) {
                    failures.push(format!(
                        "{label}: {what} positions differ: dwi {a:?} vs sm {b:?}"
                    ));
                } else if a
                    .iter()
                    .zip(b.iter())
                    .any(|(x, y)| (x.1 - y.1).abs() > ROUNDING)
                {
                    edited.push(format!(
                        "{label}: {what} values differ: dwi {a:?} vs sm {b:?}"
                    ));
                }
            }
            if (td.offset_seconds - ts.offset_seconds).abs() > ROUNDING {
                edited.push(format!(
                    "{label}: offset {} vs {}",
                    td.offset_seconds, ts.offset_seconds
                ));
            }
        }
    }
    for e in &edited {
        eprintln!("content differs (not a failure): {e}");
    }
    eprintln!(
        "{} .dwi files: {parsed} parsed, {pairs} with a .sm beside them, {charts} charts compared",
        dwis.len()
    );
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}
