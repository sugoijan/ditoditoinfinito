//! `.lrc` lyrics as StepMania 5.1 reads them (`LyricsLoader::LoadFromLRCFile`,
//! `HHMMSSToSeconds` in `RageUtil.cpp`): a song's `#LYRICSPATH`, or the
//! one `.lrc` file in its folder.
//!
//! - A line starting with `//` is a comment. Otherwise a line is `[name]
//!   data`: spaces before the `[`, around the data; a byte-order mark at
//!   the start of any line is dropped; a line without a bracket is
//!   skipped.
//! - `[COLOUR]` or `[COLOR]` (any case) with data `0xRRGGBB` sets the
//!   color of the lyrics after it (each part over 256, as StepMania
//!   divides); an invalid value is skipped. Lyrics before any are the
//!   theme's default, green in StepMania's (`LyricDisplayDefaultColor`).
//! - A name that is a time (`[hh:][mm:]ss[.xx]`, the regular expression
//!   `^([0-9]+:){0,2}[0-9]+(.[0-9]*)?$`, its `.` any byte) starts a lyric
//!   then, its data the text with each `|` a line break; only the first
//!   bracket is a time (`[00:01][00:05]a` is the text `[00:05]a` at 1 s).
//!   Any other name (`[ar:…]`, `[offset:…]`) is ignored: StepMania does not
//!   apply `[offset]` (a TODO in its loader).
//! - The lyrics are sorted by time (lines of one time keep their order).
//!
//! Times are the music's seconds. [`add_lyrics`] keeps them on a [`Song`]
//! as effect events ([`LYRIC_EVENT`]) a play shows
//! (`ddi_engine::lyrics`).

use crate::formats::sm::{sm_float, sm_int};
use crate::model::{EffectEvent, EffectTime, Song};

/// Effect kind of a `.lrc` lyric: `fields = [text, red, green, blue]`,
/// colors in 0..=1, at the music second it starts, layer 0 (every chart).
pub const LYRIC_EVENT: &str = "lrc:lyric";

/// StepMania's default lyric color (`LyricDisplayDefaultColor`, its
/// `_fallback` and `default` themes).
pub const DEFAULT_COLOR: [f32; 3] = [0.0, 1.0, 0.0];

/// One lyric: its start in music seconds, its text (`\n` between lines),
/// its color.
#[derive(Clone, Debug, PartialEq)]
pub struct Lyric {
    pub start: f64,
    pub text: String,
    pub color: [f32; 3],
}

/// The lyrics of an `.lrc` file's text, sorted by time.
pub fn parse(text: &str) -> Vec<Lyric> {
    let mut color = DEFAULT_COLOR;
    let mut out = Vec::new();
    for line in text.split('\n') {
        let line = line.strip_prefix('\u{FEFF}').unwrap_or(line);
        if line.starts_with("//") {
            continue;
        }
        let Some((name, data)) = bracketed(line) else {
            continue;
        };
        let data = data.trim_end_matches(['\r', '\n']);
        if name.eq_ignore_ascii_case("COLOUR") || name.eq_ignore_ascii_case("COLOR") {
            if let Some(c) = hex_color(data) {
                color = c;
            }
            continue;
        }
        if is_timestamp(name.as_bytes()) {
            out.push(Lyric {
                start: hhmmss_to_seconds(name),
                text: data.replace('|', "\n"),
                color,
            });
        }
    }
    out.sort_by(|a, b| a.start.total_cmp(&b.start));
    out
}

/// Adds the lyrics of `.lrc` text to `song` as [`LYRIC_EVENT`]s.
pub fn add_lyrics(song: &mut Song, text: &str) {
    for l in parse(text) {
        if !l.start.is_finite() {
            continue;
        }
        let [r, g, b] = l.color;
        song.effects.push(EffectEvent {
            at: EffectTime::Seconds(l.start),
            kind: LYRIC_EVENT.to_string(),
            layer: 0,
            fields: vec![l.text, r.to_string(), g.to_string(), b.to_string()],
        });
    }
}

/// `^ *\[([^]]+)\] *(.*)$`: the bracket's contents and what follows.
fn bracketed(line: &str) -> Option<(&str, &str)> {
    let rest = line.trim_start_matches(' ').strip_prefix('[')?;
    let end = rest.find(']')?;
    if end == 0 {
        return None;
    }
    Some((&rest[..end], rest[end + 1..].trim_start_matches(' ')))
}

/// `sscanf("0x%2x%2x%2x")`: three hex numbers of up to two digits each
/// after `0x` (white space allowed before each, as `%x` skips it).
fn hex_color(s: &str) -> Option<[f32; 3]> {
    let mut b = s.as_bytes().strip_prefix(b"0x")?;
    let mut out = [0.0; 3];
    for c in &mut out {
        while let [w, rest @ ..] = b
            && w.is_ascii_whitespace()
        {
            b = rest;
        }
        let n = b
            .iter()
            .take(2)
            .take_while(|d| d.is_ascii_hexdigit())
            .count();
        if n == 0 {
            return None;
        }
        let v = u32::from_str_radix(std::str::from_utf8(&b[..n]).ok()?, 16).ok()?;
        *c = v as f32 / 256.0;
        b = &b[n..];
    }
    Some(out)
}

/// `^([0-9]+:){0,2}[0-9]+(.[0-9]*)?$`, on bytes as PCRE matches it.
fn is_timestamp(s: &[u8]) -> bool {
    fn digits(s: &[u8]) -> usize {
        s.iter().take_while(|c| c.is_ascii_digit()).count()
    }
    // The rest after the groups: digits, then optionally any byte and
    // digits, then the end.
    fn tail(s: &[u8]) -> bool {
        let n = digits(s);
        if n == 0 {
            return false;
        }
        let rest = &s[n..];
        rest.is_empty() || digits(&rest[1..]) == rest.len() - 1
    }
    let mut s = s;
    for groups in 0..=2 {
        if tail(s) {
            return true;
        }
        if groups == 2 {
            break;
        }
        let n = digits(s);
        if n == 0 || s.get(n) != Some(&b':') {
            return false;
        }
        s = &s[n + 1..];
    }
    false
}

/// `HHMMSSToSeconds`: the parts between `:`, padded in front to three,
/// hours and minutes as integers, seconds as a float (`atoi`, `strtof`).
fn hhmmss_to_seconds(s: &str) -> f64 {
    let mut bits: Vec<&str> = s.split(':').collect();
    while bits.len() < 3 {
        bits.insert(0, "0");
    }
    sm_int(bits[0]) as f64 * 3600.0 + sm_int(bits[1]) as f64 * 60.0 + sm_float(bits[2])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lines_colors_and_times_as_stepmania_reads_them() {
        let text = "\u{FEFF}[ti:Song]\r\n// a comment\r\n  [00:01.50]  first|line two\r\n\
                    [colour]0x00FF80\r\n[00:03]second\r\n[1:00:02.25]late\r\n\
                    [00:02.00][00:09.00]repeated\r\n[COLOR]0xZZ\r\n[02:00.5x]bad?\r\n\
                    no bracket\r\n[]empty\r\n[offset:500]\r\n[00:04.]dot";
        let l = parse(text);
        let got: Vec<(f64, &str)> = l.iter().map(|l| (l.start, l.text.as_str())).collect();
        assert_eq!(
            got,
            vec![
                (1.5, "first\nline two"),
                (2.0, "[00:09.00]repeated"),
                (3.0, "second"),
                (4.0, "dot"),
                (3602.25, "late"),
            ]
        );
        assert_eq!(l[0].color, DEFAULT_COLOR);
        assert_eq!(l[2].color, [0.0, 255.0 / 256.0, 128.0 / 256.0]);
        // An invalid color keeps the one before.
        assert_eq!(l[4].color, l[2].color);
    }

    #[test]
    fn timestamps_match_stepmanias_expression() {
        for (s, ok) in [
            ("12", true),
            ("00:12.34", true),
            ("1:02:03.5", true),
            ("1:2:3:4", true), // the `.` matches the third `:`
            ("1:2:3:4:5", false),
            ("ar:x", false),
            (":12", false),
            ("12.", true),
            ("12.3.4", false),
            ("", false),
        ] {
            assert_eq!(is_timestamp(s.as_bytes()), ok, "{s}");
        }
        assert_eq!(hhmmss_to_seconds("1:2:3:4"), 3723.0);
        assert_eq!(
            hex_color("0x 1 2 3"),
            Some([1.0 / 256.0, 2.0 / 256.0, 3.0 / 256.0])
        );
        assert_eq!(hex_color("0X112233"), None);
    }

    #[test]
    fn lyrics_are_kept_on_the_song() {
        let mut song = crate::formats::sm::parse_simfile("#BPMS:0=120;", "sm").unwrap();
        add_lyrics(&mut song, "[00:01]a\n[COLOUR]0x800000\n[00:00.5]b");
        let e: Vec<_> = song
            .effects
            .iter()
            .filter(|e| e.kind == LYRIC_EVENT)
            .collect();
        assert_eq!(e.len(), 2);
        assert_eq!(e[0].at, EffectTime::Seconds(0.5));
        assert_eq!(e[0].fields, ["b", "0.5", "0", "0"]);
        assert_eq!(e[1].fields[0], "a");
    }
}
