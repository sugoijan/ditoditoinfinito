//! StepMania `.ssc` and `.sm` writers: the inverse of [`super::sm`].
//!
//! Written so that reading the text back gives the song again: for every
//! song [`super::sm::parse_ssc`] reads, `parse_ssc(&write_ssc(&song))`
//! equals it (its format then `.ssc` 0.83), and the same for `.sm` when
//! [`sm_losses`] finds nothing `.sm` cannot say. The layout follows
//! StepMania 5.1's writers (`NotesWriterSSC.cpp`, `NotesWriterSM.cpp`):
//! the song's tags, then a block per chart. What the model keeps as
//! written (`SourceInfo::unknown_tags`) goes back as it came: a song's
//! after its own tags, a chart's (`NOTEDATA[i].NAME`) in its block.
//!
//! Numbers are written so they read back exactly: values as the shortest
//! decimal that parses to the same `f64` (whole ones with three
//! decimals, as StepMania writes them), beats with three decimals or six
//! where a tick needs them (the reader rounds to the nearest tick).
//! Generated charts (`Chart::derivation`) are not written: the library
//! makes them again from their original.

use std::fmt::Write as _;

use crate::TICKS_PER_BEAT;
use crate::formats::sm::split_kind;
use crate::formats::sm::{Flavor, is_song_tag, known_lane_count};
use crate::model::{Chart, DisplayBpm, EffectEvent, EffectTime, NoteKind, Song, SourceInfo, Tick};
use crate::timing::{SpeedUnit, TimingMap};

/// The `#VERSION` the writer writes (StepMania 5.1's
/// `STEPFILE_VERSION_NUMBER`).
pub const SSC_VERSION: f32 = 0.83;

/// The `.sm` radar field: five zeros, which StepMania ignores (it reads
/// cached values only when there are as many as it has categories) and
/// stricter readers accept.
const SM_RADAR: &str = "0.000,0.000,0.000,0.000,0.000";
/// Ticks in StepMania's 4-beat note-data measure.
const MEASURE_TICKS: i64 = 4 * TICKS_PER_BEAT;
/// The row counts a measure is written with, the fewest that hold its
/// notes (StepMania's `NoteType`s; each divides 192).
const ROW_COUNTS: [i64; 9] = [4, 8, 12, 16, 24, 32, 48, 64, 192];

/// Something a song has that a `.sm` file cannot say, so writing it as
/// `.sm` would lose it ([`sm_losses`]).
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SmLoss {
    /// Warps, speeds, scrolls, fakes, labels or combos (`.ssc` timing;
    /// `.sm` gimmicks are negative BPMs and stops, read back as warps).
    Timing(&'static str),
    /// A chart with its own timing (`.ssc` split timing).
    SplitTiming { chart: usize },
    /// A chart whose name or credit is not its description (`.sm` has the
    /// one field).
    ChartNames { chart: usize },
    /// A chart's own display BPM.
    ChartDisplayBpm { chart: usize },
    /// A chart's tags kept as written (`#MUSIC`, `#ATTACKS`, …).
    ChartTags { chart: usize },
    /// A Hard chart described as `smaniac` or `challenge`, which `.sm`
    /// reads as Challenge.
    LegacyChallenge { chart: usize },
    /// The jacket (`.ssc`'s `#JACKET`).
    Jacket,
    /// Notes `.sm` has no character for (shock arrows, Dancing☆Onigiri
    /// dummies) or properties it has no place for (colors, speeds).
    Notes { chart: usize },
}

impl SmLoss {
    /// Its text, for the export's warning.
    pub fn describe(&self) -> String {
        match self {
            SmLoss::Timing(what) => format!("{what} (.ssc timing)"),
            SmLoss::SplitTiming { chart } => format!("chart {}'s own timing", chart + 1),
            SmLoss::ChartNames { chart } => format!("chart {}'s name and credit", chart + 1),
            SmLoss::ChartDisplayBpm { chart } => format!("chart {}'s display BPM", chart + 1),
            SmLoss::ChartTags { chart } => format!("chart {}'s own tags", chart + 1),
            SmLoss::LegacyChallenge { chart } => {
                format!("chart {}'s difficulty (read as Challenge)", chart + 1)
            }
            SmLoss::Jacket => "the jacket".to_string(),
            SmLoss::Notes { chart } => format!("notes of chart {} .sm cannot hold", chart + 1),
        }
    }
}

/// What writing `song` as `.sm` would lose; empty when `.sm` holds it all.
pub fn sm_losses(song: &Song) -> Vec<SmLoss> {
    let mut out = Vec::new();
    let t = &song.timing;
    for (what, present) in [
        ("warps", !t.warps.is_empty()),
        ("speeds", !t.speeds.is_empty()),
        ("scrolls", !t.scrolls.is_empty()),
        ("fakes", !t.fakes.is_empty()),
        ("labels", !t.labels.is_empty()),
        ("combos", !t.combos.is_empty()),
    ] {
        if present {
            out.push(SmLoss::Timing(what));
        }
    }
    if song.jacket.is_some() {
        out.push(SmLoss::Jacket);
    }
    for (chart, c) in written_charts(song) {
        if c.timing.is_some() {
            out.push(SmLoss::SplitTiming { chart });
        }
        if c.name != c.description || c.credit != c.description {
            out.push(SmLoss::ChartNames { chart });
        }
        if c.display_bpm.is_some() {
            out.push(SmLoss::ChartDisplayBpm { chart });
        }
        if song.source.chart_tags(chart).next().is_some() {
            out.push(SmLoss::ChartTags { chart });
        }
        if c.difficulty == crate::Difficulty::Hard
            && (c.description.eq_ignore_ascii_case("smaniac")
                || c.description.eq_ignore_ascii_case("challenge"))
        {
            out.push(SmLoss::LegacyChallenge { chart });
        }
        if c.notes
            .iter()
            .any(|n| note_char(n.kind).is_none() || n.color.is_some() || n.speed_mul.is_some())
        {
            out.push(SmLoss::Notes { chart });
        }
    }
    out
}

/// `song` as an `.ssc` file (version [`SSC_VERSION`]).
pub fn write_ssc(song: &Song) -> String {
    let mut w = Writer::default();
    w.tag("VERSION", &format!("{SSC_VERSION:.2}"));
    w.song_header(song, Flavor::Ssc);
    w.timing(&song.timing, Flavor::Ssc);
    w.song_tail(song);
    for (index, chart) in written_charts(song) {
        w.out.push('\n');
        w.tag("NOTEDATA", "");
        w.tag("CHARTNAME", &escape(&chart.name));
        w.tag("STEPSTYPE", &escape(&chart.layout));
        w.tag("DESCRIPTION", &escape(&chart.description));
        w.tag("DIFFICULTY", chart.difficulty.name());
        w.tag("METER", &chart.meter.to_string());
        w.tag("CREDIT", &escape(&chart.credit));
        if let Some(bpm) = chart.display_bpm.filter(|b| *b != DisplayBpm::Actual) {
            w.tag("DISPLAYBPM", &display_bpm(bpm));
        }
        if let Some(timing) = &chart.timing {
            w.timing(timing, Flavor::Ssc);
        }
        for (_, name, value) in song.source.chart_tags(index) {
            w.raw_tag(name, value);
        }
        w.out.push_str("#NOTES:\n");
        w.out.push_str(&note_data(chart));
        w.out.push_str(";\n");
    }
    w.out
}

/// `song` as an `.sm` file. What [`sm_losses`] lists is left out.
pub fn write_sm(song: &Song) -> String {
    let mut w = Writer::default();
    w.song_header(song, Flavor::Sm);
    w.timing(&song.timing, Flavor::Sm);
    w.song_tail(song);
    for (_, chart) in written_charts(song) {
        w.out.push('\n');
        let _ = write!(
            w.out,
            "#NOTES:\n     {}:\n     {}:\n     {}:\n     {}:\n     {SM_RADAR}:\n",
            escape(&chart.layout),
            escape(&chart.description),
            chart.difficulty.name(),
            chart.meter,
        );
        w.out.push_str(&note_data(chart));
        w.out.push_str(";\n");
    }
    w.out
}

/// The charts a file holds, with their indexes: the originals.
fn written_charts(song: &Song) -> impl Iterator<Item = (usize, &Chart)> {
    song.charts
        .iter()
        .enumerate()
        .filter(|(_, c)| c.derivation.is_none())
}

#[derive(Default)]
struct Writer {
    out: String,
}

impl Writer {
    /// `#NAME:value;` with `value` already escaped.
    fn tag(&mut self, name: &str, value: &str) {
        let _ = writeln!(self.out, "#{name}:{value};");
    }

    /// A tag kept as written: its parameters joined by `:` (`MsdTag::
    /// raw_value`), so `:` stays a separator.
    fn raw_tag(&mut self, name: &str, value: &str) {
        let value = escape_with(value, false);
        self.tag(name, &value);
    }

    fn opt(&mut self, name: &str, value: &Option<String>) {
        self.tag(name, &escape(value.as_deref().unwrap_or("")));
    }

    fn song_header(&mut self, song: &Song, flavor: Flavor) {
        for (name, value) in [
            ("TITLE", &song.title),
            ("SUBTITLE", &song.subtitle),
            ("ARTIST", &song.artist),
            ("TITLETRANSLIT", &song.title_translit),
            ("SUBTITLETRANSLIT", &song.subtitle_translit),
            ("ARTISTTRANSLIT", &song.artist_translit),
            ("GENRE", &song.genre),
            ("CREDIT", &song.credit),
        ] {
            self.tag(name, &escape(value));
        }
        self.opt("MUSIC", &song.music);
        self.opt("BANNER", &song.banner);
        self.opt("BACKGROUND", &song.background);
        if flavor == Flavor::Ssc {
            self.opt("JACKET", &song.jacket);
        }
        self.opt("CDTITLE", &song.cd_title);
        self.tag("SAMPLESTART", &number(song.preview_start));
        self.tag("SAMPLELENGTH", &number(song.preview_length));
        if song.display_bpm != DisplayBpm::Actual {
            self.tag("DISPLAYBPM", &display_bpm(song.display_bpm));
        }
        // The song's tags kept as written, those the format would not read
        // as its own (a `.sm`'s `#COMBOS`, which StepMania ignores there
        // and `.ssc` reads).
        for (name, value) in &song.source.unknown_tags {
            if !SourceInfo::is_chart_tag_key(name) && !is_song_tag(name, flavor) {
                self.raw_tag(name, value);
            }
        }
    }

    fn song_tail(&mut self, song: &Song) {
        self.effects(&song.effects);
        if !song.keysounds.is_empty() {
            let list: Vec<String> = song.keysounds.iter().map(|k| escape(k)).collect();
            self.tag("KEYSOUNDS", &list.join(","));
        }
    }

    /// Background and foreground changes as they were written (their
    /// `RAW_*` events; the split ones after them are made again on
    /// reading), and attacks, in the order the song has them.
    fn effects(&mut self, effects: &[EffectEvent]) {
        let mut i = 0;
        while i < effects.len() {
            let e = &effects[i];
            if let Some(split) = split_kind(&e.kind) {
                let name = bg_tag_name(split, e.layer);
                let value = e.fields.first().map_or("", String::as_str);
                self.tag(&name, &escape_with(value, false));
                i += 1;
                while effects
                    .get(i)
                    .is_some_and(|n| n.kind == split && n.layer == e.layer)
                {
                    i += 1;
                }
                continue;
            }
            match e.kind.as_str() {
                kind @ ("bgchange" | "fgchange") => {
                    // Changes made without their written value: joined
                    // again.
                    let layer = e.layer;
                    let mut entries = Vec::new();
                    while let Some(n) = effects
                        .get(i)
                        .filter(|n| n.kind == kind && n.layer == layer)
                    {
                        entries.push(n.fields.join("="));
                        i += 1;
                    }
                    self.tag(
                        &bg_tag_name(kind, layer),
                        &escape_with(&entries.join(","), false),
                    );
                }
                "attack" => {
                    let mut params = Vec::new();
                    while let Some(n) = effects.get(i).filter(|n| n.kind == "attack") {
                        let start = match n.at {
                            EffectTime::Seconds(s) => s,
                            EffectTime::Beat(_) => 0.0,
                        };
                        params.push(format!("TIME={}", number(start)));
                        params.push(format!(
                            "LEN={}",
                            n.fields.get(1).map_or("0", String::as_str)
                        ));
                        params.push(format!(
                            "MODS={}",
                            n.fields.first().map_or("", String::as_str)
                        ));
                        i += 1;
                    }
                    self.raw_tag("ATTACKS", &params.join(":"));
                }
                // Lyrics and other files' effects belong to those files.
                _ => i += 1,
            }
        }
    }

    fn timing(&mut self, t: &TimingMap, flavor: Flavor) {
        self.tag("OFFSET", &number(t.offset_seconds));
        let pairs = |items: &mut dyn Iterator<Item = (Tick, String)>| -> String {
            items
                .map(|(tick, v)| format!("{}={v}", beat(tick)))
                .collect::<Vec<_>>()
                .join(",")
        };
        self.tag(
            "BPMS",
            &pairs(&mut t.bpms.iter().map(|s| (s.tick, number(s.bpm)))),
        );
        self.tag(
            "STOPS",
            &pairs(&mut t.stops.iter().map(|s| (s.tick, number(s.seconds)))),
        );
        self.tag(
            "DELAYS",
            &pairs(&mut t.delays.iter().map(|s| (s.tick, number(s.seconds)))),
        );
        if flavor == Flavor::Ssc {
            self.tag(
                "WARPS",
                &pairs(&mut t.warps.iter().map(|s| (s.tick, beat(s.length)))),
            );
        }
        self.tag(
            "TIMESIGNATURES",
            &pairs(
                &mut t
                    .time_signatures
                    .iter()
                    .map(|s| (s.tick, format!("{}={}", s.numerator, s.denominator))),
            ),
        );
        self.tag(
            "TICKCOUNTS",
            &pairs(
                &mut t
                    .tick_counts
                    .iter()
                    .map(|s| (s.tick, s.ticks_per_beat.to_string())),
            ),
        );
        if flavor == Flavor::Sm {
            return;
        }
        self.tag(
            "COMBOS",
            &pairs(
                &mut t
                    .combos
                    .iter()
                    .map(|s| (s.tick, format!("{}={}", s.combo, s.miss_combo))),
            ),
        );
        self.tag(
            "SPEEDS",
            &pairs(&mut t.speeds.iter().map(|s| {
                let unit = match s.unit {
                    SpeedUnit::Beats => 0,
                    SpeedUnit::Seconds => 1,
                };
                (
                    s.tick,
                    format!("{}={}={unit}", number(s.ratio), number(s.delay)),
                )
            })),
        );
        self.tag(
            "SCROLLS",
            &pairs(&mut t.scrolls.iter().map(|s| (s.tick, number(s.ratio)))),
        );
        self.tag(
            "FAKES",
            &pairs(&mut t.fakes.iter().map(|s| (s.tick, beat(s.length)))),
        );
        // A label's text cannot hold `,` or `=` (the reader splits on
        // them, unescaped, as StepMania's does).
        self.tag(
            "LABELS",
            &pairs(
                &mut t
                    .labels
                    .iter()
                    .map(|s| (s.tick, escape(&s.text.replace([',', '='], " ")))),
            ),
        );
    }
}

fn bg_tag_name(kind: &str, layer: u8) -> String {
    match (kind, layer) {
        ("fgchange", _) => "FGCHANGES".to_string(),
        (_, 0) => "BGCHANGES".to_string(),
        (_, n) => format!("BGCHANGES{}", n + 1),
    }
}

fn display_bpm(bpm: DisplayBpm) -> String {
    match bpm {
        DisplayBpm::Actual => String::new(),
        DisplayBpm::Single(v) => number(v),
        DisplayBpm::Range(a, b) => format!("{}:{}", number(a), number(b)),
        DisplayBpm::Random => "*".to_string(),
    }
}

/// A value the reader takes back exactly: whole numbers with three
/// decimals (StepMania's look), others as their shortest decimal.
pub fn number(v: f64) -> String {
    if v.fract() == 0.0 && v.abs() < 1e15 {
        format!("{v:.3}")
    } else {
        format!("{v}")
    }
}

/// A tick as a beat: three decimals when they say it exactly, else six
/// (a 1/48 beat is 0.0208333…: the reader rounds to the nearest tick).
pub fn beat(tick: Tick) -> String {
    let beats = tick.0 as f64 / TICKS_PER_BEAT as f64;
    if (tick.0 * 1000) % TICKS_PER_BEAT == 0 {
        format!("{beats:.3}")
    } else {
        format!("{beats:.6}")
    }
}

/// Text in a one-parameter tag: `:` escaped too.
fn escape(s: &str) -> String {
    escape_with(s, true)
}

/// Escapes what the MSD reader would take as structure: `\`, `;`, `#`,
/// the second `/` of a `//` comment and, with `colon`, `:`.
fn escape_with(s: &str, colon: bool) -> String {
    let mut out = String::with_capacity(s.len());
    let mut prev = '\0';
    for c in s.chars() {
        let special =
            matches!(c, '\\' | ';' | '#') || (colon && c == ':') || (c == '/' && prev == '/');
        if special {
            out.push('\\');
        }
        out.push(c);
        prev = c;
    }
    out
}

/// A note's character, `None` for kinds `.sm` has none for.
fn note_char(kind: NoteKind) -> Option<char> {
    Some(match kind {
        NoteKind::Tap => '1',
        NoteKind::HoldHead { .. } => '2',
        NoteKind::RollHead { .. } => '4',
        NoteKind::Mine => 'M',
        NoteKind::Lift => 'L',
        NoteKind::Fake => 'F',
        NoteKind::AutoKeysound => 'K',
        NoteKind::Shock | NoteKind::Dummy => return None,
    })
}

/// The columns a chart's rows have: its steps type's, else as many as
/// its notes use.
pub fn lane_count(chart: &Chart) -> usize {
    known_lane_count(&chart.layout).unwrap_or_else(|| {
        chart
            .notes
            .iter()
            .map(|n| n.lane as usize + 1)
            .max()
            .unwrap_or(1)
    })
}

/// A chart's note data: 4-beat measures, each with the fewest rows that
/// hold its notes. Holds and rolls end with a `3` on their tail's row;
/// a cell two notes share keeps the first (a valid chart has none).
fn note_data(chart: &Chart) -> String {
    let lanes = lane_count(chart);
    // (tick, lane) → the cell's text.
    let mut cells: std::collections::BTreeMap<(i64, usize), String> = Default::default();
    for n in &chart.notes {
        let lane = n.lane as usize;
        let Some(c) = note_char(n.kind) else {
            continue;
        };
        if lane >= lanes || n.tick < Tick::ZERO {
            continue;
        }
        let mut cell = c.to_string();
        if let Some(k) = n.keysound {
            let _ = write!(cell, "[{k}]");
        }
        cells.entry((n.tick.0, lane)).or_insert(cell);
        if let Some(end) = n.kind.hold_end() {
            cells
                .entry((end.0, lane))
                .or_insert_with(|| "3".to_string());
        }
    }
    let last = cells.keys().map(|(t, _)| *t).max().unwrap_or(0);
    let measures = last / MEASURE_TICKS + 1;
    let mut out = String::new();
    let mut it = cells.iter().peekable();
    for m in 0..measures {
        let start = m * MEASURE_TICKS;
        let end = start + MEASURE_TICKS;
        let in_measure: Vec<_> =
            std::iter::from_fn(|| it.next_if(|((t, _), _)| *t < end)).collect();
        let rows = ROW_COUNTS
            .into_iter()
            .find(|&r| {
                let step = MEASURE_TICKS / r;
                in_measure.iter().all(|((t, _), _)| (t - start) % step == 0)
            })
            .unwrap_or(192);
        let step = MEASURE_TICKS / rows;
        let mut grid = vec![vec!["0"; lanes]; rows as usize];
        for ((t, lane), cell) in &in_measure {
            grid[((t - start) / step) as usize][*lane] = cell.as_str();
        }
        if m > 0 {
            out.push_str(",\n");
        }
        for row in grid {
            out.push_str(&row.concat());
            out.push('\n');
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::formats::msd::parse_msd;

    #[test]
    fn numbers_and_beats_read_back_exactly() {
        use crate::formats::sm::sm_float;
        for v in [
            0.0,
            -0.0,
            120.0,
            0.611,
            -0.25,
            1.0 / 3.0,
            1e-7,
            123456.789,
            2.5e10,
        ] {
            assert_eq!(sm_float(&number(v)).to_bits(), v.to_bits(), "{v}");
        }
        assert_eq!(number(122.0), "122.000");
        for t in [0, 1, 7, 16, 47, 48, 1000, 4_799_999_952, -12] {
            assert_eq!(
                Tick::from_beat_f64(sm_float(&beat(Tick(t)))),
                Tick(t),
                "{t}"
            );
        }
        assert_eq!(beat(Tick(12)), "0.250");
        assert_eq!(beat(Tick(16)), "0.333333");
    }

    #[test]
    fn escapes_read_back_as_the_text() {
        for s in [
            "a:b",
            "x;y",
            "#1",
            "back\\slash",
            "http://x//y",
            "///",
            "a/b",
        ] {
            let text = format!("#TITLE:{};", escape(s));
            assert_eq!(parse_msd(&text)[0].params, vec![s.to_string()], "{text}");
        }
        let text = format!("#X:{};", escape_with("a:b", false));
        assert_eq!(parse_msd(&text)[0].params, vec!["a", "b"]);
    }
}
