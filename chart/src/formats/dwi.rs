//! DWI (Dance With Intensity) `.dwi` importer.
//!
//! Follows StepMania's `NotesLoaderDWI.cpp` as documented in
//! `docs/research/formats-sm-ssc-dwi.md` §3:
//!
//! - The file is lexed without unescaping ([`parse_msd_raw`]).
//! - `#GAP` is integer milliseconds before beat 0: `offset = -GAP / 1000`.
//! - `#BPM` sets the BPM at beat 0; `#CHANGEBPM`/`#BPMCHANGE` and `#FREEZE`
//!   take `BBB=value` lists whose positions are in quarter beats (16th
//!   notes), freezes in milliseconds. BPMs must be positive.
//! - `#SAMPLESTART`/`#SAMPLELENGTH` use `ParseBrokenDWITimestamp`: a value
//!   with a `.` is seconds, otherwise milliseconds; two or three
//!   colon-separated parts are `m:s` or `h:m:s`. A length in (0, 1) is
//!   multiplied by 1000. The readme's `+` prefix ("factor in the GAP") is
//!   ignored, as StepMania does.
//! - `#TITLE` is split into title and subtitle like
//!   `NotesLoader::GetMainAndSubTitlesFromFullTitle`.
//! - `#SINGLE|DOUBLE|COUPLE|SOLO:difficulty:meter:pad1[:pad2];` map to
//!   `dance-single`, `dance-double`, `dance-couple` and `dance-solo`.
//!   Difficulty uses the DWI-compatible aliases; an unknown name falls back
//!   to the meter (1 Beginner, ≤3 Easy, ≤6 Medium, else Hard) like
//!   `Steps::TidyUpData`. An empty meter is 1.
//! - Note data: whitespace is removed; each character is an 8th note,
//!   `(` 16ths, `[` 24ths, `{` 64ths, `` ` `` 192nds, and any of
//!   `)` `]` `}` `'` `>` returns to 8ths. `<...>` is a jump (all codes on
//!   one row), unless a `0` appears before the next `>`, in which case the
//!   `<` is an old-style 192nd marker. `X!Y` shows `X` and turns the panels
//!   of `Y` into hold heads; each hold ends at the next note in its column,
//!   which is removed; holds never closed are dropped.
//! - Everything else (`#MD5`, `#STATUS`, `#DISPLAYTITLE`, `#BACKGROUND` …
//!   `#END`, …) has no effect and is kept in `source.unknown_tags`.
//!
//! Deliberate deviations from StepMania 5.1 (more forgiving, or not
//! reproducing a crash or an accident):
//!
//! - Jumps follow the loader StepMania shipped up to 5.1.0. The current
//!   `5_1-new` branch carries an ITGmania change (`829f49f622`, "Fix .dwi
//!   support") that reads one extra character per jump member, so `<24>`
//!   would place only Down; we do not reproduce that.
//! - Steps-type tags are matched case-insensitively. StepMania accepts
//!   `#single:` as a chart tag but then asserts in `GetTypeFromMode`.
//! - Panels the layout lacks (solo letters in a single/double chart, a
//!   second pad on a single or solo chart) are dropped. StepMania looks them
//!   up with `std::map::operator[]`, which silently maps them to column 0.
//! - `#DISPLAYBPM` numbers are read as decimal integers. StepMania uses
//!   `sscanf("%i")`, which reads `090` as octal (0) and `0x..` as hex.
//! - Freezes that are not positive are dropped, as for every other format.
//!   StepMania keeps one only when it happens to be the first stop added.
//! - Note positions are accumulated in exact ticks rather than a `double`
//!   beat converted through `float`; the results agree for every song of
//!   realistic length.
//! - A chart whose pad strings are both shorter than two characters is
//!   skipped (ITGmania's guard). StepMania 5.1 asserts on such charts.
//! - Text decoding is the caller's job: StepMania re-reads title, artist
//!   and genre as Windows-1252 when they are not valid UTF-8.

use std::collections::BTreeMap;
use std::ops::Bound::{Excluded, Unbounded};

use crate::TICKS_PER_BEAT;
use crate::formats::ParseError;
use crate::formats::msd::{MsdTag, parse_msd_raw};
use crate::formats::sm::{empty_song, opt_string, parse_difficulty, sm_float, sm_int};
use crate::model::{Chart, Difficulty, DisplayBpm, Note, NoteKind, Song, SourceFormat, Tick};
use crate::timing::{BpmSegment, StopSegment, TimingMap};

/// Parse a DWI file.
///
/// Never fails in practice: like StepMania, malformed entries are skipped.
/// The `Result` keeps the signature in line with the other importers.
pub fn parse_dwi(text: &str) -> Result<Song, ParseError> {
    let mut song = empty_song(SourceFormat::Dwi);
    let mut offset = 0.0;
    let mut bpms: Vec<BpmSegment> = Vec::new();
    let mut stops: Vec<StopSegment> = Vec::new();

    for tag in &parse_msd_raw(text) {
        let name = tag.name.trim().to_ascii_uppercase();
        let value = tag.param(0);
        match name.as_str() {
            "FILE" => song.music = opt_string(value),
            "TITLE" => {
                let (title, subtitle) = split_full_title(value);
                song.title = title.to_string();
                song.subtitle = subtitle.to_string();
            }
            "ARTIST" => song.artist = value.to_string(),
            "GENRE" => song.genre = value.to_string(),
            "CDTITLE" => song.cd_title = opt_string(value),
            "BPM" => {
                let bpm = sm_float(value);
                if bpm > 0.0 {
                    bpms.push(BpmSegment {
                        tick: Tick::ZERO,
                        bpm,
                    });
                }
            }
            "DISPLAYBPM" => song.display_bpm = parse_display_bpm(value),
            "GAP" => offset = -(sm_int(value) as f64) / 1000.0,
            "SAMPLESTART" => song.preview_start = parse_timestamp(tag),
            "SAMPLELENGTH" => {
                let mut length = parse_timestamp(tag);
                // "There were multiple versions of this tag allegedly."
                if length > 0.0 && length < 1.0 {
                    length *= 1000.0;
                }
                song.preview_length = length;
            }
            "FREEZE" => {
                for (position, ms) in entries(value) {
                    add_stop(
                        &mut stops,
                        quarter_beats_to_tick(position),
                        sm_float(ms) / 1000.0,
                    );
                }
            }
            "CHANGEBPM" | "BPMCHANGE" => {
                for (position, bpm) in entries(value) {
                    let bpm = sm_float(bpm);
                    if bpm > 0.0 {
                        bpms.push(BpmSegment {
                            tick: quarter_beats_to_tick(position),
                            bpm,
                        });
                    }
                }
            }
            "SINGLE" | "DOUBLE" | "COUPLE" | "SOLO" => {
                if let Some(chart) = build_chart(&name, tag) {
                    song.charts.push(chart);
                }
            }
            _ => song
                .source
                .unknown_tags
                .push((name.clone(), tag.raw_value())),
        }
    }

    let mut timing = TimingMap::constant(60.0, offset);
    // Pushed in file order: `tidy` keeps the last BPM per tick, which is
    // what `TimingData::AddSegment` does when a row is set twice.
    timing.bpms = bpms;
    timing.stops = stops;
    timing.tidy();
    song.timing = timing;
    Ok(song)
}

// ---------------------------------------------------------------------------
// Header helpers.
// ---------------------------------------------------------------------------

/// `NotesLoader::GetMainAndSubTitlesFromFullTitle`: the first separator found
/// (tried in this order) splits the title; the subtitle keeps the separator
/// minus its leading blank, e.g. `Song (Remix)` → `Song`, `(Remix)`.
fn split_full_title(full: &str) -> (&str, &str) {
    for sep in ["\t", " -", " ~", " (", " ["] {
        if let Some(i) = full.find(sep) {
            return (&full[..i], &full[i + 1..]);
        }
    }
    (full, "")
}

/// `#DISPLAYBPM:a..b|a|*`: anything that is not one or two integers
/// (including `*` and an empty value) is a cycling display.
fn parse_display_bpm(value: &str) -> DisplayBpm {
    let Some((min, rest)) = scan_int(value) else {
        return DisplayBpm::Random;
    };
    let max = rest.strip_prefix("..").and_then(scan_int).map(|(v, _)| v);
    match max {
        Some(max) if max != min => DisplayBpm::Range(min as f64, max as f64),
        _ => DisplayBpm::Single(min as f64),
    }
}

/// `sscanf("%d")`-style integer: leading whitespace, optional sign, at least
/// one digit. Returns the value and the unread rest.
fn scan_int(s: &str) -> Option<(i64, &str)> {
    let s = s.trim_start();
    let b = s.as_bytes();
    let mut i = 0;
    if i < b.len() && (b[i] == b'+' || b[i] == b'-') {
        i += 1;
    }
    let digits = i;
    while i < b.len() && b[i].is_ascii_digit() {
        i += 1;
    }
    if i == digits {
        return None;
    }
    Some((s[..i].parse().unwrap_or(0), &s[i..]))
}

/// `ParseBrokenDWITimestamp` over a tag's first three parameters (the MSD
/// lexer splits `m:ss.ss` at the colons).
fn parse_timestamp(tag: &MsdTag) -> f64 {
    let (a, b, c) = (tag.param(0), tag.param(1), tag.param(2));
    if a.is_empty() {
        0.0
    } else if b.is_empty() {
        if a.contains('.') {
            sm_float(a)
        } else {
            sm_float(a) / 1000.0
        }
    } else if c.is_empty() {
        // `HHMMSSToSeconds("a:b")` pads to `0:a:b`.
        sm_int(a) as f64 * 60.0 + sm_float(b)
    } else {
        sm_int(a) as f64 * 3600.0 + sm_int(b) as f64 * 60.0 + sm_float(c)
    }
}

/// `BBB=vvv,...` with StepMania's `split` semantics (empty pieces ignored);
/// entries without exactly two fields are skipped.
fn entries(value: &str) -> impl Iterator<Item = (&str, &str)> {
    value.split(',').filter(|e| !e.is_empty()).filter_map(|e| {
        let mut fields = e.split('=').filter(|f| !f.is_empty());
        match (fields.next(), fields.next(), fields.next()) {
            (Some(a), Some(b), None) => Some((a, b)),
            _ => None,
        }
    })
}

/// `BeatToNoteRow(StringToFloat(s) / 4)`: DWI positions count quarter beats;
/// the conversion is in `float` with `lrint` (ties to even).
fn quarter_beats_to_tick(s: &str) -> Tick {
    let beat = sm_float(s) as f32 / 4.0;
    Tick((beat * TICKS_PER_BEAT as f32).round_ties_even() as i64)
}

/// `TimingData::AddSegment` for stops: a stop on an existing row replaces it
/// (a non-positive one deletes it); otherwise only positive stops are added,
/// except that the very first one is always pushed (`tidy` then drops it if
/// it is not positive).
fn add_stop(stops: &mut Vec<StopSegment>, tick: Tick, seconds: f64) {
    if let Some(i) = stops.iter().position(|s| s.tick == tick) {
        if seconds > 0.0 {
            stops[i].seconds = seconds;
        } else {
            stops.remove(i);
        }
    } else if seconds > 0.0 || stops.is_empty() {
        stops.push(StopSegment { tick, seconds });
    }
}

// ---------------------------------------------------------------------------
// Charts.
// ---------------------------------------------------------------------------

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Panel {
    Left,
    UpLeft,
    Down,
    Up,
    UpRight,
    Right,
}

/// `DWIcharToNote`: the panels a step character stands for. `0`, `5` and
/// unknown characters (lower case included) stand for none.
fn char_panels(c: u8) -> [Option<Panel>; 2] {
    use Panel::*;
    let (a, b) = match c {
        b'1' => (Some(Down), Some(Left)),
        b'2' => (Some(Down), None),
        b'3' => (Some(Down), Some(Right)),
        b'4' => (Some(Left), None),
        b'6' => (Some(Right), None),
        b'7' => (Some(Up), Some(Left)),
        b'8' => (Some(Up), None),
        b'9' => (Some(Up), Some(Right)),
        b'A' => (Some(Up), Some(Down)),
        b'B' => (Some(Left), Some(Right)),
        b'C' => (Some(UpLeft), None),
        b'D' => (Some(UpRight), None),
        b'E' => (Some(Left), Some(UpLeft)),
        b'F' => (Some(UpLeft), Some(Down)),
        b'G' => (Some(UpLeft), Some(Up)),
        b'H' => (Some(UpLeft), Some(Right)),
        b'I' => (Some(Left), Some(UpRight)),
        b'J' => (Some(Down), Some(UpRight)),
        b'K' => (Some(Up), Some(UpRight)),
        b'L' => (Some(UpRight), Some(Right)),
        b'M' => (Some(UpLeft), Some(UpRight)),
        _ => (None, None),
    };
    [a, b]
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Mode {
    Single,
    Double,
    Couple,
    Solo,
}

impl Mode {
    fn from_tag(name: &str) -> Option<Mode> {
        Some(match name {
            "SINGLE" => Mode::Single,
            "DOUBLE" => Mode::Double,
            "COUPLE" => Mode::Couple,
            "SOLO" => Mode::Solo,
            _ => return None,
        })
    }

    fn layout_id(self) -> &'static str {
        match self {
            Mode::Single => "dance-single",
            Mode::Double => "dance-double",
            Mode::Couple => "dance-couple",
            Mode::Solo => "dance-solo",
        }
    }

    fn lane_count(self) -> usize {
        match self {
            Mode::Single => 4,
            Mode::Double | Mode::Couple => 8,
            Mode::Solo => 6,
        }
    }

    /// The loader's `g_mapDanceNoteToNoteDataColumn` for this steps type.
    fn column(self, pad: usize, panel: Panel) -> Option<u8> {
        let four = match panel {
            Panel::Left => Some(0),
            Panel::Down => Some(1),
            Panel::Up => Some(2),
            Panel::Right => Some(3),
            Panel::UpLeft | Panel::UpRight => None,
        };
        match (self, pad) {
            (Mode::Single, 0) => four,
            (Mode::Double | Mode::Couple, 0 | 1) => four.map(|c| c + 4 * pad as u8),
            (Mode::Solo, 0) => Some(match panel {
                Panel::Left => 0,
                Panel::UpLeft => 1,
                Panel::Down => 2,
                Panel::Up => 3,
                Panel::UpRight => 4,
                Panel::Right => 5,
            }),
            _ => None,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Cell {
    Tap,
    HoldHead,
    Hold { end: i64 },
}

/// One `NoteData` track: tick → cell, at most one cell per tick.
type Track = BTreeMap<i64, Cell>;

/// `LoadFromDWITokens` + `ParseNoteData` + the difficulty part of
/// `Steps::TidyUpData`.
fn build_chart(name: &str, tag: &MsdTag) -> Option<Chart> {
    let mode = Mode::from_tag(name)?;
    let step1 = tag.param(2);
    // `iNumParams == 5`: the second pad is read only when it is the last
    // parameter.
    let step2 = if tag.params.len() == 4 {
        tag.param(3)
    } else {
        ""
    };
    if step1.len() < 2 && step2.len() < 2 {
        return None;
    }

    let meter_text = tag.param(1);
    let meter = if meter_text.is_empty() {
        1
    } else {
        sm_int(meter_text)
    };
    let difficulty = parse_difficulty(tag.param(0)).unwrap_or(if meter == 1 {
        Difficulty::Beginner
    } else if meter <= 3 {
        Difficulty::Easy
    } else if meter <= 6 {
        Difficulty::Medium
    } else {
        Difficulty::Hard
    });

    let mut tracks: Vec<Track> = vec![Track::new(); mode.lane_count()];
    for (pad, data) in [step1, step2].into_iter().enumerate() {
        if !data.is_empty() {
            parse_pad(mode, pad, data, &mut tracks);
        }
    }
    close_holds(&mut tracks);

    let mut notes: Vec<Note> = Vec::new();
    for (lane, track) in tracks.iter().enumerate() {
        for (&tick, &cell) in track {
            let kind = match cell {
                Cell::Tap => NoteKind::Tap,
                Cell::Hold { end } => NoteKind::HoldHead { end: Tick(end) },
                Cell::HoldHead => continue,
            };
            notes.push(Note::new(Tick(tick), lane as u8, kind));
        }
    }
    notes.sort_by_key(|n| (n.tick, n.lane));

    Some(Chart {
        layout: mode.layout_id().to_string(),
        difficulty,
        meter: meter.max(0) as u32,
        name: String::new(),
        description: String::new(),
        credit: String::new(),
        notes,
        timing: None,
        display_bpm: None,
    })
}

/// `Is192`: a `<` is an old-style 192nd marker when a `0` comes before the
/// next `>`.
fn is_192(data: &[u8], from: usize) -> bool {
    for &c in &data[from.min(data.len())..] {
        match c {
            b'>' => return false,
            b'0' => return true,
            _ => {}
        }
    }
    false
}

/// Place one pad's step string into `tracks` (the loop of `ParseNoteData`,
/// as shipped through StepMania 5.1.0).
fn parse_pad(mode: Mode, pad: usize, data: &str, tracks: &mut [Track]) {
    let data: Vec<u8> = data
        .bytes()
        .filter(|c| !matches!(c, b'\n' | b'\r' | b'\t' | b' '))
        .collect();
    const EIGHTH: i64 = TICKS_PER_BEAT / 2;

    let mut place = |c: u8, tick: i64, cell: Cell| {
        for panel in char_panels(c).into_iter().flatten() {
            if let Some(col) = mode.column(pad, panel) {
                tracks[col as usize].insert(tick, cell);
            }
        }
    };

    let mut tick: i64 = 0;
    let mut step = EIGHTH;
    let mut i = 0;
    while i < data.len() {
        let c = data[i];
        i += 1;
        match c {
            b'(' => step = TICKS_PER_BEAT / 4,
            b'[' => step = TICKS_PER_BEAT / 6,
            b'{' => step = TICKS_PER_BEAT / 16,
            b'`' => step = TICKS_PER_BEAT / 48,
            b')' | b']' | b'}' | b'\'' | b'>' => step = EIGHTH,
            // A `!` not directly after a step: StepMania logs and skips it.
            b'!' => {}
            _ => {
                let jump = c == b'<';
                if jump && is_192(&data, i) {
                    step = TICKS_PER_BEAT / 48;
                    continue;
                }
                if !jump {
                    // Re-read `c` inside the loop below.
                    i -= 1;
                }
                while let Some(&c) = data.get(i) {
                    i += 1;
                    if jump && c == b'>' {
                        break;
                    }
                    place(c, tick, Cell::Tap);
                    if i >= data.len() {
                        break;
                    }
                    if data[i] == b'!' {
                        if let Some(&hold) = data.get(i + 1) {
                            place(hold, tick, Cell::HoldHead);
                        }
                        i += 2;
                    }
                    if !jump {
                        break;
                    }
                }
                tick += step;
            }
        }
    }
}

/// The "fill in iDuration" pass: walking each track in order, a hold head
/// ends at the next note in its track, which is removed; a head with
/// nothing after it is removed.
fn close_holds(tracks: &mut [Track]) {
    for track in tracks {
        let mut cursor: Option<i64> = None;
        loop {
            let next = match cursor {
                None => track.iter().next(),
                Some(r) => track.range((Excluded(r), Unbounded)).next(),
            };
            let Some((&row, &cell)) = next else { break };
            cursor = Some(row);
            if cell != Cell::HoldHead {
                continue;
            }
            let tail = track
                .range((Excluded(row), Unbounded))
                .next()
                .map(|(&t, _)| t);
            match tail {
                Some(end) => {
                    track.remove(&end);
                    track.insert(row, Cell::Hold { end });
                }
                None => {
                    track.remove(&row);
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::formats::msd::parse_msd_raw;

    fn approx(a: f64, b: f64) -> bool {
        (a - b).abs() < 1e-9
    }

    fn timestamp(text: &str) -> f64 {
        parse_timestamp(&parse_msd_raw(text)[0])
    }

    #[test]
    fn title_split_matches_stepmania() {
        assert_eq!(split_full_title("Song (Remix)"), ("Song", "(Remix)"));
        assert_eq!(
            split_full_title("Song -Long Ver.-"),
            ("Song", "-Long Ver.-")
        );
        assert_eq!(split_full_title("Song ~extended~"), ("Song", "~extended~"));
        assert_eq!(split_full_title("Song [EX]"), ("Song", "[EX]"));
        assert_eq!(split_full_title("Song\tsub"), ("Song", "sub"));
        // Separators are tried in a fixed order, not by position.
        assert_eq!(split_full_title("A (B) -C-"), ("A (B)", "-C-"));
        assert_eq!(split_full_title("Plain"), ("Plain", ""));
        assert_eq!(split_full_title("Hy-phen"), ("Hy-phen", ""));
    }

    #[test]
    fn timestamps_in_every_syntax() {
        assert!(approx(timestamp("#SAMPLESTART:5230;"), 5.23));
        assert!(approx(timestamp("#SAMPLESTART:5.23;"), 5.23));
        assert!(approx(timestamp("#SAMPLESTART:1:05.5;"), 65.5));
        assert!(approx(timestamp("#SAMPLESTART:1:01:05.5;"), 3665.5));
        assert!(approx(timestamp("#SAMPLESTART:+5230;"), 5.23));
        assert!(approx(timestamp("#SAMPLESTART:;"), 0.0));
    }

    #[test]
    fn display_bpm_forms() {
        assert_eq!(parse_display_bpm("*"), DisplayBpm::Random);
        assert_eq!(parse_display_bpm(""), DisplayBpm::Random);
        assert_eq!(parse_display_bpm("150"), DisplayBpm::Single(150.0));
        assert_eq!(parse_display_bpm("150.5"), DisplayBpm::Single(150.0));
        assert_eq!(parse_display_bpm("90..180"), DisplayBpm::Range(90.0, 180.0));
        assert_eq!(
            parse_display_bpm("090..180"),
            DisplayBpm::Range(90.0, 180.0)
        );
        assert_eq!(parse_display_bpm("120..120"), DisplayBpm::Single(120.0));
        assert_eq!(parse_display_bpm("100..x"), DisplayBpm::Single(100.0));
    }

    #[test]
    fn entries_ignore_empty_pieces_and_need_two_fields() {
        let got: Vec<_> = entries("4=500,,8==250,12=1=2,16").collect();
        assert_eq!(got, vec![("4", "500"), ("8", "250")]);
    }

    #[test]
    fn quarter_beat_rounding_is_ties_to_even() {
        assert_eq!(quarter_beats_to_tick("32"), Tick::from_beats(8));
        assert_eq!(quarter_beats_to_tick("1"), Tick(12));
        // 0.375 quarter beats = 4.5 ticks → 4 (lrint), not 5.
        assert_eq!(quarter_beats_to_tick("0.375"), Tick(4));
        assert_eq!(quarter_beats_to_tick("0.125"), Tick(2));
    }

    #[test]
    fn stops_replace_and_delete_on_the_same_row() {
        let mut stops = Vec::new();
        add_stop(&mut stops, Tick(48), 0.5);
        add_stop(&mut stops, Tick(96), 0.0);
        add_stop(&mut stops, Tick(48), 0.25);
        assert_eq!(
            stops,
            vec![StopSegment {
                tick: Tick(48),
                seconds: 0.25
            }]
        );
        add_stop(&mut stops, Tick(48), 0.0);
        assert!(stops.is_empty());
    }

    #[test]
    fn is_192_looks_for_zero_before_close() {
        assert!(is_192(b"<0808>", 1));
        assert!(!is_192(b"<48>0", 1));
        assert!(!is_192(b"<48", 1));
    }
}
