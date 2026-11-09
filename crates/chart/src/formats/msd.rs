//! MSD lexical layer shared by `.sm`, `.ssc` and `.dwi`:
//! `#TAG:param:param;` with `//` comments.
//!
//! Mirrors StepMania's `MsdFile::ReadBuf`:
//!
//! - `//` starts a comment anywhere (even inside a value or a URL) that runs
//!   to the end of the line.
//! - `\x` yields `x` literally for any `x` (`\:` `\;` `\#` `\\` `\/` ...)
//!   in unescaping mode ([`parse_msd`], used for `.sm`/`.ssc`). In raw mode
//!   ([`parse_msd_raw`], used for `.dwi`) the backslash is kept and only stops
//!   the next character from acting as a separator.
//! - A `#` that is the first non-blank character of a line while a value is
//!   still open terminates that value: many files lack the final `;`.
//! - An unterminated value at EOF is kept.
//! - Text outside `#...;` is ignored.
//!
//! Unlike StepMania, every parameter is trimmed at both edges; this matches
//! what the loaders do with each parameter they use (note data included).

/// One `#NAME:p1:p2:...;` tag. `name` is the text before the first `:`
/// as written (not upper-cased).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct MsdTag {
    pub name: String,
    pub params: Vec<String>,
}

impl MsdTag {
    /// Parameter `i` (0-based, excluding the name) or `""`.
    pub fn param(&self, i: usize) -> &str {
        self.params.get(i).map(String::as_str).unwrap_or("")
    }

    /// All parameters re-joined with `:` (the "raw value" of the tag).
    pub fn raw_value(&self) -> String {
        self.params.join(":")
    }
}

fn is_blank(c: char) -> bool {
    matches!(c, ' ' | '\t' | '\r' | '\n')
}

/// `s` without blanks at its edges, in place.
fn trim_edges(mut s: String) -> String {
    s.truncate(s.trim_end_matches(is_blank).len());
    let start = s.len() - s.trim_start_matches(is_blank).len();
    s.drain(..start);
    s
}

/// Lex with unescaping (`MsdFile::ReadFile(path, true)`), as the `.sm` and
/// `.ssc` loaders do.
pub fn parse_msd(text: &str) -> Vec<MsdTag> {
    lex(text, true)
}

/// Lex without unescaping (`MsdFile::ReadFile(path, false)`), as the `.dwi`
/// loader does: `\x` stays `\x` but `x` is still not a separator.
pub fn parse_msd_raw(text: &str) -> Vec<MsdTag> {
    lex(text, false)
}

fn lex(text: &str, unescape: bool) -> Vec<MsdTag> {
    let text = text.strip_prefix('\u{feff}').unwrap_or(text);
    // Every delimiter is ASCII, so it is walked byte by byte and whole
    // runs are copied; a byte of a longer character is never one of them.
    let bytes = text.as_bytes();
    let len = bytes.len();
    // The character at `i`, which starts one, and how long it is.
    let char_at = |i: usize| text[i..].chars().next().map(|c| (c, c.len_utf8()));

    let mut tags: Vec<MsdTag> = Vec::new();
    // Parameters of the value being read, including the name as element 0.
    let mut params: Vec<String> = Vec::new();
    // Current parameter buffer; `None` when no parameter has been started.
    let mut cur: Option<String> = None;
    let mut reading = false;

    let finish_value = |params: &mut Vec<String>, tags: &mut Vec<MsdTag>| {
        let mut it = std::mem::take(params).into_iter();
        let name = it.next().unwrap_or_default();
        tags.push(MsdTag {
            name,
            params: it.collect(),
        });
    };

    let mut i = 0;
    while i < len {
        let c = bytes[i];

        // Comments are stripped everywhere.
        if c == b'/' && bytes.get(i + 1) == Some(&b'/') {
            i += bytes[i..]
                .iter()
                .position(|&b| b == b'\n')
                .unwrap_or(len - i);
            continue;
        }

        if reading && c == b'#' {
            // Missing `;` tolerance: a `#` as the first non-blank character
            // of a line ends the open value.
            let buf = cur.as_deref().unwrap_or("");
            let first_on_line = buf
                .chars()
                .rev()
                .take_while(|&ch| ch != '\r' && ch != '\n')
                .all(|ch| ch == ' ' || ch == '\t');
            if !first_on_line {
                cur.get_or_insert_with(String::new).push('#');
                i += 1;
                continue;
            }
            params.push(trim_edges(cur.take().unwrap_or_default()));
            finish_value(&mut params, &mut tags);
            reading = false;
        }

        if !reading && c == b'#' {
            reading = true;
            params.clear();
            cur = Some(String::new());
            i += 1;
            continue;
        }

        if !reading {
            // Outside of a value an escape still consumes two characters
            // (only when unescaping, like StepMania).
            i += 1;
            if unescape && c == b'\\' {
                i += char_at(i).map_or(0, |(_, n)| n);
            }
            continue;
        }

        match c {
            b':' => {
                params.push(trim_edges(cur.take().unwrap_or_default()));
                cur = Some(String::new());
                i += 1;
            }
            b';' => {
                if let Some(p) = cur.take() {
                    params.push(trim_edges(p));
                }
                finish_value(&mut params, &mut tags);
                reading = false;
                i += 1;
            }
            b'\\' => {
                let buf = cur.get_or_insert_with(String::new);
                if !unescape {
                    buf.push('\\');
                }
                i += 1;
                if let Some((escaped, n)) = char_at(i) {
                    buf.push(escaped);
                    i += n;
                }
            }
            _ => {
                // Up to whatever could end the plain text. `i` may be a
                // single `/`, which is plain text here.
                let end = bytes[i + 1..]
                    .iter()
                    .position(|b| matches!(b, b'/' | b'#' | b':' | b';' | b'\\'))
                    .map_or(len, |n| i + 1 + n);
                cur.get_or_insert_with(String::new).push_str(&text[i..end]);
                i = end;
            }
        }
    }

    if reading {
        if let Some(p) = cur.take() {
            params.push(trim_edges(p));
        }
        finish_value(&mut params, &mut tags);
    }

    tags
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tag(name: &str, params: &[&str]) -> MsdTag {
        MsdTag {
            name: name.to_string(),
            params: params.iter().map(|s| s.to_string()).collect(),
        }
    }

    #[test]
    fn basic_tags() {
        let t = parse_msd("#TITLE:Foo;\n#ARTIST:Bar;#OFFSET:-0.5;");
        assert_eq!(
            t,
            vec![
                tag("TITLE", &["Foo"]),
                tag("ARTIST", &["Bar"]),
                tag("OFFSET", &["-0.5"]),
            ]
        );
    }

    #[test]
    fn multiple_params_and_empty_params() {
        let t = parse_msd("#DISPLAYBPM:100:200;#STOPS:;#X;");
        assert_eq!(
            t,
            vec![
                tag("DISPLAYBPM", &["100", "200"]),
                tag("STOPS", &[""]),
                tag("X", &[]),
            ]
        );
    }

    #[test]
    fn comments_are_stripped_everywhere() {
        let t = parse_msd(
            "#TITLE:Foo // comment ; here\n;\n// whole line #IGNORED:x;\n#URL:http://a.b/c;",
        );
        // `:` opens a new (empty) parameter before `//...` is dropped.
        assert_eq!(t, vec![tag("TITLE", &["Foo"]), tag("URL", &["http", ""])]);
    }

    #[test]
    fn escapes() {
        let t = parse_msd(r"#TITLE:a\:b\;c\#d\\e\/f;");
        assert_eq!(t, vec![tag("TITLE", &[r"a:b;c#d\e/f"])]);
    }

    #[test]
    fn bom_and_crlf() {
        let t = parse_msd("\u{feff}#TITLE:Foo;\r\n#NOTES:a:\r\n0000\r\n0000\r\n;\r\n");
        assert_eq!(
            t,
            vec![tag("TITLE", &["Foo"]), tag("NOTES", &["a", "0000\r\n0000"])]
        );
    }

    #[test]
    fn missing_semicolon_before_next_tag_and_at_eof() {
        let t = parse_msd("#BPMS:0.000=160.000\n#STOPS:;\n#TITLE:End");
        assert_eq!(
            t,
            vec![
                tag("BPMS", &["0.000=160.000"]),
                tag("STOPS", &[""]),
                tag("TITLE", &["End"]),
            ]
        );
    }

    #[test]
    fn hash_inside_a_line_is_literal() {
        let t = parse_msd("#TITLE:C# is #1;");
        assert_eq!(t, vec![tag("TITLE", &["C# is #1"])]);
    }

    #[test]
    fn params_trimmed_but_inner_whitespace_kept() {
        let t = parse_msd("#NOTES:  dance-single :\n  desc  :\n\n  1000\n0100\n,\n0010\n0001\n  ;");
        assert_eq!(
            t,
            vec![tag(
                "NOTES",
                &["dance-single", "desc", "1000\n0100\n,\n0010\n0001"]
            )]
        );
    }

    #[test]
    fn text_outside_values_is_ignored() {
        let t = parse_msd("garbage\n#A:1;trailing\\;junk;#B:2;");
        assert_eq!(t, vec![tag("A", &["1"]), tag("B", &["2"])]);
    }

    #[test]
    fn raw_mode_keeps_backslashes_but_not_separators() {
        let t = parse_msd_raw(r"#FILE:.\music\a\:b.mp3;#Y:2;");
        assert_eq!(
            t,
            vec![tag("FILE", &[r".\music\a\:b.mp3"]), tag("Y", &["2"])]
        );
        // `\#` outside a value: unescaping mode skips both characters, raw
        // mode starts a value at the `#`.
        let t = parse_msd_raw(r"\#X:1;");
        assert_eq!(t, vec![tag("X", &["1"])]);
        assert!(parse_msd(r"\#X:1;").is_empty());
    }

    #[test]
    fn helpers() {
        let t = parse_msd("#A:x:y;");
        assert_eq!(t[0].param(0), "x");
        assert_eq!(t[0].param(5), "");
        assert_eq!(t[0].raw_value(), "x:y");
    }
}
