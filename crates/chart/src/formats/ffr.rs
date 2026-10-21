//! FlashFlashRevolution charts (`docs/research/formats-ffr-danoni.md`,
//! part A; `docs/plans/phase-10.md`, step 2): a level's `beatBox`, the
//! ActionScript array a level SWF assigns to `_root.beatBox`, one entry a
//! note: `[frame, direction, color]`, or `[frame, direction, color, ms]`.
//!
//! What R^3 does with it (rCubed `ChartFFRLegacy.parseChart`, read for
//! its behavior): a note's time is `(frame + sync) / 30` seconds from
//! the start of the level's music, or `ms / 1000` when the entry has a
//! fourth value; its direction is exactly `L`, `D`, `U` or `R` (anything
//! else is left out); its color is the third value, `blue` when it is
//! missing or empty. `sync` is the playlist's per-song frame offset (not
//! in the level itself; 0 here unless given).
//!
//! Here: one `dance-single` chart (left, down, up, right), on a synthetic
//! tempo of [`SYNTHETIC_BPM`], where a tick is 1/1200 s: a frame is 40
//! ticks exactly, and a note timed in milliseconds is within 0.42 ms of
//! its time. The colors are kept on the notes (`Note::color`, shown with
//! "songs' own colors"). Two notes at one time in one lane are one.
//!
//! The array comes as values ([`Value`]): from a level SWF
//! (`ddi_library::swf` runs the ActionScript that builds it) or written
//! as text ([`parse_beatbox_text`]: what a decompiler shows, or JSON).
//! Nothing here fetches anything: a level is read from what the player
//! has.

use super::ParseError;
use crate::TICKS_PER_BEAT;
use crate::model::{
    Chart, Color, Difficulty, DisplayBpm, Note, NoteKind, Song, SourceFormat, Tick,
};
use crate::timing::TimingMap;

/// The synthetic tempo of FFR charts: 1,200 ticks a second.
pub const SYNTHETIC_BPM: f64 = 1500.0;
/// FFR's frame rate: frames are 1/30 s.
pub const FRAME_RATE: f64 = 30.0;
/// How long song select's sample of a level plays, seconds.
const PREVIEW_SECONDS: f64 = 15.0;
/// The most notes taken from one level.
pub const MAX_NOTES: usize = 200_000;
/// The latest time a note may have, seconds (a day).
const MAX_SECONDS: f64 = 86_400.0;
/// The deepest nesting [`parse_beatbox_text`] reads.
const MAX_DEPTH: usize = 16;
/// The most values [`parse_beatbox_text`] reads.
const MAX_VALUES: usize = 4 * MAX_NOTES;

/// An ActionScript value as far as a `beatBox` needs: numbers, text,
/// arrays, and what else there is.
#[derive(Clone, Debug, PartialEq)]
pub enum Value {
    Number(f64),
    Text(String),
    Bool(bool),
    /// `null` or `undefined`.
    Null,
    List(Vec<Value>),
    /// Anything else (an object).
    Other,
}

impl Value {
    fn number(&self) -> Option<f64> {
        match self {
            Value::Number(n) => Some(*n),
            // ActionScript turns text into a number where one is wanted.
            Value::Text(t) => t.trim().parse().ok(),
            _ => None,
        }
    }

    /// The value as JSON, which [`parse_beatbox_text`] reads back (a
    /// number that is not finite, and an object, as `null`).
    pub fn to_json(&self) -> String {
        let mut out = String::new();
        self.write_json(&mut out);
        out
    }

    fn write_json(&self, out: &mut String) {
        match self {
            Value::Number(n) if n.is_finite() => out.push_str(&n.to_string()),
            Value::Text(t) => {
                out.push('"');
                for c in t.chars() {
                    match c {
                        '"' => out.push_str("\\\""),
                        '\\' => out.push_str("\\\\"),
                        '\n' => out.push_str("\\n"),
                        '\r' => out.push_str("\\r"),
                        '\t' => out.push_str("\\t"),
                        c if (c as u32) < 0x20 => out.push_str(&format!("\\u{:04x}", c as u32)),
                        c => out.push(c),
                    }
                }
                out.push('"');
            }
            Value::Bool(b) => out.push_str(if *b { "true" } else { "false" }),
            Value::List(items) => {
                out.push('[');
                for (i, v) in items.iter().enumerate() {
                    if i > 0 {
                        out.push(',');
                    }
                    v.write_json(out);
                }
                out.push(']');
            }
            Value::Number(_) | Value::Null | Value::Other => out.push_str("null"),
        }
    }

    /// Falsy as ActionScript 2 has it (`beat[2] || "blue"`).
    fn falsy(&self) -> bool {
        match self {
            Value::Null => true,
            Value::Other | Value::List(_) => false,
            Value::Bool(b) => !b,
            Value::Number(n) => *n == 0.0 || n.is_nan(),
            Value::Text(t) => t.is_empty(),
        }
    }
}

/// What a level does not say about itself.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct LevelInfo {
    pub title: String,
    pub artist: String,
    /// The stepartist.
    pub credit: String,
    /// The playlist's offset, frames (added to every note's frame).
    pub sync: i64,
    /// The music's file, relative to the chart's.
    pub music: Option<String>,
    /// FFR's difficulty number, when known.
    pub meter: u32,
}

/// The color FFR names `name` as ours: its palette (`noteColors`: red,
/// blue, purple, yellow, pink, orange, cyan, green, white; by FFR's
/// convention 4th, 8th, 12th, 16th, 24th, 32nd, 48th, 64th, 192nd notes),
/// in the hues our quantization colors use. A name it does not have: none
/// (the note takes its quantization's).
pub fn color(name: &str) -> Option<Color> {
    let rgb = |r: u8, g: u8, b: u8| Color {
        r: f32::from(r) / 255.0,
        g: f32::from(g) / 255.0,
        b: f32::from(b) / 255.0,
        a: 1.0,
    };
    Some(match name.trim().to_ascii_lowercase().as_str() {
        "red" => rgb(0xff, 0x3b, 0x30),
        "blue" => rgb(0x2f, 0x7d, 0xff),
        "purple" => rgb(0xa8, 0x4c, 0xff),
        "yellow" => rgb(0xff, 0xd6, 0x1f),
        "pink" => rgb(0xff, 0x5c, 0xc8),
        "orange" => rgb(0xff, 0x8a, 0x1f),
        "cyan" => rgb(0x1f, 0xd8, 0xe8),
        "green" => rgb(0x34, 0xd0, 0x58),
        "white" => rgb(0xff, 0xff, 0xff),
        _ => return None,
    })
}

/// The lane of a direction, as R^3 takes them: exactly `L`, `D`, `U`, `R`.
fn lane(direction: &Value) -> Option<u8> {
    match direction {
        Value::Text(t) => match t.as_str() {
            "L" => Some(0),
            "D" => Some(1),
            "U" => Some(2),
            "R" => Some(3),
            _ => None,
        },
        _ => None,
    }
}

/// A note's time, seconds from the music's start, and its color's name.
fn note_of(entry: &Value, sync: i64) -> Option<(f64, u8, String)> {
    let Value::List(parts) = entry else {
        return None;
    };
    let lane = lane(parts.get(1)?)?;
    let time = if parts.len() >= 4 {
        parts[3].number()? / 1000.0
    } else {
        (parts.first()?.number()? + sync as f64) / FRAME_RATE
    };
    if !time.is_finite() || time.abs() > MAX_SECONDS {
        return None;
    }
    let color = match parts.get(2) {
        Some(v) if !v.falsy() => match v {
            Value::Text(t) => t.clone(),
            Value::Number(n) => n.to_string(),
            _ => String::new(),
        },
        _ => "blue".to_string(),
    };
    Some((time, lane, color))
}

/// A song of one `beatBox`: its notes on [`SYNTHETIC_BPM`], beat 0 at the
/// music's start (or at the first note, when one comes before it).
/// Entries that are not notes are left out; none at all is an error.
pub fn song_from_beatbox(beatbox: &Value, info: &LevelInfo) -> Result<Song, ParseError> {
    let Value::List(entries) = beatbox else {
        return Err(ParseError::Malformed("the beatBox is not an array".into()));
    };
    let mut notes: Vec<(f64, u8, String)> = entries
        .iter()
        .filter_map(|e| note_of(e, info.sync))
        .take(MAX_NOTES)
        .collect();
    if notes.is_empty() {
        return Err(ParseError::Malformed("the beatBox has no notes".into()));
    }
    let ticks_per_second = SYNTHETIC_BPM / 60.0 * TICKS_PER_BEAT as f64;
    // Beat 0 at the music's start; earlier when a note is (a negative
    // sync): ticks are never negative.
    let start = notes.iter().map(|n| n.0).fold(0.0f64, f64::min);
    notes.sort_by(|a, b| a.0.total_cmp(&b.0).then(a.1.cmp(&b.1)));
    let notes_start = notes[0].0;
    let mut chart_notes: Vec<Note> = Vec::with_capacity(notes.len());
    for (time, lane, name) in notes {
        let tick = Tick(((time - start) * ticks_per_second).round() as i64);
        if chart_notes
            .iter()
            .rev()
            .take_while(|n| n.tick == tick)
            .any(|n| n.lane == lane)
        {
            continue;
        }
        let mut note = Note::new(tick, lane, NoteKind::Tap);
        note.color = color(&name);
        chart_notes.push(note);
    }
    chart_notes.sort_by_key(|n| (n.tick, n.lane));
    let mut song = super::sm::empty_song(SourceFormat::Ffr);
    song.title = info.title.clone();
    song.artist = info.artist.clone();
    song.credit = info.credit.clone();
    song.music = info.music.clone();
    song.timing = TimingMap::constant(SYNTHETIC_BPM, -start);
    // No tempo of its own: the scroll speed takes FFR's reference
    // (`ddi_engine::scroll::speed_for_song`) and none is shown.
    song.display_bpm = DisplayBpm::Random;
    // Song select's sample: from the first note (a level names none).
    song.preview_start = notes_start.max(0.0);
    song.preview_length = PREVIEW_SECONDS;
    song.charts.push(Chart {
        derivation: None,
        layout: "dance-single".into(),
        difficulty: Difficulty::Medium,
        meter: info.meter,
        name: String::new(),
        description: String::new(),
        credit: info.credit.clone(),
        notes: chart_notes,
        timing: None,
        display_bpm: None,
        danoni: None,
    });
    Ok(song)
}

/// A `beatBox` written as text: `_root.beatBox = [[30, "L", "red"], …];`
/// as a decompiler shows a level's ActionScript, `beatBox = […]`, or the
/// bare array (JSON is one). Strings in single or double quotes; numbers
/// as ActionScript writes them; `true`, `false`, `null`, `undefined`.
pub fn parse_beatbox_text(text: &str) -> Result<Value, ParseError> {
    let bad = |why: &str| ParseError::Malformed(format!("beatBox: {why}"));
    let text = text.trim_start_matches('\u{feff}');
    // What is assigned, if it is an assignment.
    let body = match text.find('[') {
        Some(at) => {
            let before = text[..at].trim();
            let ok = before.is_empty()
                || before
                    .strip_suffix('=')
                    .map(str::trim)
                    .is_some_and(|name| name.rsplit('.').next() == Some("beatBox"));
            if !ok {
                return Err(bad("not a beatBox"));
            }
            &text[at..]
        }
        None => return Err(bad("no array")),
    };
    let mut r = TextReader {
        bytes: body.as_bytes(),
        at: 0,
        values: 0,
    };
    let value = r.value(0).map_err(|e| bad(&e))?;
    r.space();
    let rest = &body[r.at..];
    if !rest.trim().trim_start_matches(';').trim().is_empty() {
        return Err(bad("text after the array"));
    }
    Ok(value)
}

struct TextReader<'a> {
    bytes: &'a [u8],
    at: usize,
    values: usize,
}

impl TextReader<'_> {
    fn space(&mut self) {
        while self.at < self.bytes.len() {
            match self.bytes[self.at] {
                b' ' | b'\t' | b'\r' | b'\n' => self.at += 1,
                // Comments a decompiler keeps.
                b'/' if self.bytes.get(self.at + 1) == Some(&b'/') => {
                    while self.at < self.bytes.len() && self.bytes[self.at] != b'\n' {
                        self.at += 1;
                    }
                }
                _ => break,
            }
        }
    }

    fn value(&mut self, depth: usize) -> Result<Value, String> {
        self.values += 1;
        if self.values > MAX_VALUES {
            return Err("too many values".into());
        }
        self.space();
        let Some(&c) = self.bytes.get(self.at) else {
            return Err("cut short".into());
        };
        match c {
            b'[' => {
                if depth >= MAX_DEPTH {
                    return Err("nested too deep".into());
                }
                self.at += 1;
                let mut items = Vec::new();
                loop {
                    self.space();
                    if self.bytes.get(self.at) == Some(&b']') {
                        self.at += 1;
                        return Ok(Value::List(items));
                    }
                    items.push(self.value(depth + 1)?);
                    self.space();
                    match self.bytes.get(self.at) {
                        Some(b',') => self.at += 1,
                        Some(b']') => {}
                        _ => return Err(format!("expected , or ] at byte {}", self.at)),
                    }
                }
            }
            b'"' | b'\'' => self.string(c).map(Value::Text),
            _ => {
                let start = self.at;
                while self.at < self.bytes.len()
                    && (self.bytes[self.at].is_ascii_alphanumeric()
                        || matches!(self.bytes[self.at], b'.' | b'-' | b'+' | b'_'))
                {
                    self.at += 1;
                }
                let word = std::str::from_utf8(&self.bytes[start..self.at]).unwrap_or("");
                match word {
                    "" => Err(format!("unexpected {:?} at byte {start}", c as char)),
                    "true" => Ok(Value::Bool(true)),
                    "false" => Ok(Value::Bool(false)),
                    "null" | "undefined" => Ok(Value::Null),
                    w => w
                        .parse::<f64>()
                        .ok()
                        .filter(|n| n.is_finite())
                        .or_else(|| {
                            w.strip_prefix("0x")
                                .and_then(|h| i64::from_str_radix(h, 16).ok())
                                .map(|n| n as f64)
                        })
                        .map(Value::Number)
                        .ok_or_else(|| format!("not a value: {w}")),
                }
            }
        }
    }

    fn string(&mut self, quote: u8) -> Result<String, String> {
        self.at += 1;
        let mut out = Vec::new();
        while let Some(&c) = self.bytes.get(self.at) {
            self.at += 1;
            match c {
                c if c == quote => return String::from_utf8(out).map_err(|e| e.to_string()),
                b'\\' => {
                    let Some(&e) = self.bytes.get(self.at) else {
                        break;
                    };
                    self.at += 1;
                    match e {
                        b'n' => out.push(b'\n'),
                        b't' => out.push(b'\t'),
                        b'r' => out.push(b'\r'),
                        b'u' => {
                            let hex = self
                                .bytes
                                .get(self.at..self.at + 4)
                                .and_then(|h| std::str::from_utf8(h).ok())
                                .and_then(|h| u32::from_str_radix(h, 16).ok())
                                .ok_or("a bad \\u escape")?;
                            self.at += 4;
                            let ch = char::from_u32(hex).unwrap_or('\u{fffd}');
                            out.extend_from_slice(ch.to_string().as_bytes());
                        }
                        other => out.push(other),
                    }
                }
                c => out.push(c),
            }
        }
        Err("a string not closed".into())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn notes(song: &Song) -> Vec<(i64, u8)> {
        song.charts[0]
            .notes
            .iter()
            .map(|n| (n.tick.0, n.lane))
            .collect()
    }

    #[test]
    fn frames_are_thirtieths_of_a_second_from_the_music() {
        let v = parse_beatbox_text(
            r#"_root.beatBox = [[30, "L", "red"], [45, 'D', "blue"], [45, "R"], [60, "U", ""]];"#,
        )
        .unwrap();
        let song = song_from_beatbox(&v, &LevelInfo::default()).unwrap();
        // 40 ticks a frame.
        assert_eq!(
            notes(&song),
            vec![(1200, 0), (1800, 1), (1800, 3), (2400, 2)]
        );
        let timing = &song.timing;
        for (n, secs) in song.charts[0].notes.iter().zip([1.0, 1.5, 1.5, 2.0]) {
            assert!((timing.seconds_at(n.tick) - secs).abs() < 1e-9);
        }
        // Colors as named; missing or empty: blue.
        let c: Vec<_> = song.charts[0].notes.iter().map(|n| n.color).collect();
        assert_eq!(c[0], color("red"));
        assert_eq!(c[2], color("blue"));
        assert_eq!(c[3], color("blue"));
        assert_eq!(song.source.format, SourceFormat::Ffr);
        assert_eq!(song.charts[0].layout, "dance-single");
    }

    #[test]
    fn milliseconds_win_over_frames_and_sync_moves_frames() {
        let v = parse_beatbox_text("[[30,\"L\",\"red\",1003.4],[30,\"R\",\"red\"]]").unwrap();
        let info = LevelInfo {
            sync: -3,
            ..LevelInfo::default()
        };
        let song = song_from_beatbox(&v, &info).unwrap();
        let t: Vec<f64> = song.charts[0]
            .notes
            .iter()
            .map(|n| song.timing.seconds_at(n.tick))
            .collect();
        // 27 frames for the right; 1.0034 s for the left, to the tick.
        assert!((t[0] - 0.9).abs() < 1e-9, "{t:?}");
        assert!((t[1] - 1.0034).abs() <= 0.5 / 1200.0, "{t:?}");
        assert_eq!(song.charts[0].notes[0].lane, 3);
    }

    #[test]
    fn notes_before_the_music_move_beat_zero_back() {
        let v = parse_beatbox_text("[[3,\"L\"],[30,\"D\"]]").unwrap();
        let song = song_from_beatbox(
            &v,
            &LevelInfo {
                sync: -6,
                ..LevelInfo::default()
            },
        )
        .unwrap();
        assert_eq!(notes(&song)[0], (0, 0));
        assert!((song.timing.seconds_at(Tick(0)) + 0.1).abs() < 1e-9);
        assert!((song.timing.seconds_at(song.charts[0].notes[1].tick) - 0.8).abs() < 1e-9);
    }

    #[test]
    fn what_is_not_a_note_is_left_out() {
        let v = parse_beatbox_text(
            "[[10,'l'],[10,'X'],[10],'text',[null,'L'],[20,'L','red',\"x\"],\
             [30,'U'],[30,'U','red'],[1e9,'L']]",
        )
        .unwrap();
        let song = song_from_beatbox(&v, &LevelInfo::default()).unwrap();
        // Lowercase and unknown directions, no frame, a bad ms value, a
        // note a day past: out; a note twice: once.
        assert_eq!(notes(&song), vec![(1200, 2)]);
        assert!(
            song_from_beatbox(&parse_beatbox_text("[]").unwrap(), &LevelInfo::default()).is_err()
        );
        assert!(song_from_beatbox(&Value::Number(1.0), &LevelInfo::default()).is_err());
    }

    /// Text damaged anywhere reads or is refused; none panics.
    #[test]
    fn damaged_text_never_panics() {
        let good = "_root.beatBox = [[30, \"L\", \"red\"], [45, 'D', 'blué', 1500.5], // ノート\n[60,\"R\"]];";
        let mut seed = 0x9e37_79b9_7f4a_7c15u64;
        let mut next = || {
            seed ^= seed << 13;
            seed ^= seed >> 7;
            seed ^= seed << 17;
            seed
        };
        for _ in 0..5000 {
            let mut bytes = good.as_bytes().to_vec();
            for _ in 0..1 + next() % 3 {
                let at = (next() as usize) % bytes.len();
                bytes[at] = b"[]',\"\\/\n0x-.L\xc3"[(next() % 14) as usize];
            }
            if let Ok(text) = String::from_utf8(bytes)
                && let Ok(v) = parse_beatbox_text(&text)
            {
                let _ = song_from_beatbox(&v, &LevelInfo::default());
            }
        }
    }

    #[test]
    fn the_text_form_reads_what_decompilers_and_json_write() {
        let as2 = "// frame 1\n_root.beatBox = [\n  [12, \"L\", \"red\"], // a note\n  [0x10, 'R', 'blue']\n];\n";
        let v = parse_beatbox_text(as2).unwrap();
        assert_eq!(
            v,
            Value::List(vec![
                Value::List(vec![
                    Value::Number(12.0),
                    Value::Text("L".into()),
                    Value::Text("red".into())
                ]),
                Value::List(vec![
                    Value::Number(16.0),
                    Value::Text("R".into()),
                    Value::Text("blue".into())
                ]),
            ])
        );
        assert!(parse_beatbox_text("beatBox=[[1,\"\\u004c\",true,false,undefined]]").is_ok());
        for bad in [
            "",
            "x = [[1,'L']]",
            "[[1,'L']",
            "[[1,'L']] trailing",
            "[[1,'L' 'D']]",
            "[\"open",
            "[[1,'L'],]x",
        ] {
            assert!(parse_beatbox_text(bad).is_err(), "{bad}");
        }
        // JSON written back reads the same.
        let v = parse_beatbox_text(r#"[[1,"L","r\"e\nd",2.5],[true,null,'é']]"#).unwrap();
        assert_eq!(parse_beatbox_text(&v.to_json()).unwrap(), v);
        // Nesting and size are bounded.
        let deep = format!("{}{}", "[".repeat(100), "]".repeat(100));
        assert!(parse_beatbox_text(&deep).is_err());
    }
}
