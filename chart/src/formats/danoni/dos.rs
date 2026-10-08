//! The dos text format: `|key=value|` fields.
//!
//! As danoniplus's `dosConvert` (`js/lib/dosConverter.js` 521–535): `&` is
//! a field separator too unless the page turns it off, a field without `=`
//! (or starting with one) is ignored, the key runs to the first `=`, nothing
//! is trimmed, and a later field with the same key wins. Several texts (an
//! inline dos, then an external one) merge the same way, later over earlier.

use std::collections::HashMap;

/// How a page asks the texts to be split.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DosFlags {
    /// `enableAmpersandSplit` (default on).
    pub ampersand_split: bool,
    /// `enableDecodeURI` (default off): values are percent-decoded.
    pub decode_uri: bool,
}

impl Default for DosFlags {
    fn default() -> DosFlags {
        DosFlags {
            ampersand_split: true,
            decode_uri: false,
        }
    }
}

/// The fields of one or more dos texts.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Dos {
    map: HashMap<String, String>,
    /// Keys in first-seen order (for reporting).
    order: Vec<String>,
}

impl Dos {
    pub fn parse(text: &str, flags: DosFlags) -> Dos {
        let mut d = Dos::default();
        d.merge(text, flags);
        d
    }

    /// Adds the fields of `text`, replacing earlier values of the same keys.
    pub fn merge(&mut self, text: &str, flags: DosFlags) {
        let joined;
        let text = if flags.ampersand_split {
            joined = text.replace('&', "|");
            joined.as_str()
        } else {
            text
        };
        for field in text.split('|') {
            let Some(pos) = field.find('=') else { continue };
            if pos == 0 {
                continue;
            }
            let (key, value) = (&field[..pos], &field[pos + 1..]);
            let value = if flags.decode_uri {
                percent_decode(value).unwrap_or_else(|| value.to_string())
            } else {
                value.to_string()
            };
            self.set(key, value);
        }
    }

    pub fn set(&mut self, key: &str, value: String) {
        if !self.map.contains_key(key) {
            self.order.push(key.to_string());
        }
        self.map.insert(key.to_string(), value);
    }

    pub fn get(&self, key: &str) -> Option<&str> {
        self.map.get(key).map(String::as_str)
    }

    /// The value when present and not empty (danoniplus `hasVal`).
    pub fn val(&self, key: &str) -> Option<&str> {
        self.get(key).filter(|v| !v.is_empty())
    }

    pub fn keys(&self) -> impl Iterator<Item = &str> {
        self.order.iter().map(String::as_str)
    }

    /// Every field, in first-seen order.
    pub fn fields(&self) -> impl Iterator<Item = (&str, &str)> {
        self.order
            .iter()
            .map(|k| (k.as_str(), self.map[k].as_str()))
    }
}

/// `decodeURIComponent`; `None` on a malformed sequence (which throws in
/// JavaScript).
fn percent_decode(s: &str) -> Option<String> {
    let b = s.as_bytes();
    let mut out = Vec::with_capacity(b.len());
    let mut i = 0;
    while i < b.len() {
        if b[i] == b'%' {
            let hex = s.get(i + 1..i + 3)?;
            out.push(u8::from_str_radix(hex, 16).ok()?);
            i += 3;
        } else {
            out.push(b[i]);
            i += 1;
        }
    }
    String::from_utf8(out).ok()
}

/// danoniplus `splitLF`: lines, with `\r` as a line break too.
pub fn split_lf(s: &str) -> impl Iterator<Item = &str> {
    s.split(['\r', '\n'])
}

/// danoniplus `splitLF2`: non-empty lines joined with `$`, then split on
/// `$` (so `$` and line breaks both separate entries, blank lines do not).
pub fn split_lf2(s: &str) -> Vec<String> {
    split_lf(s)
        .filter(|l| !l.is_empty())
        .collect::<Vec<_>>()
        .join("$")
        .split('$')
        .map(str::to_string)
        .collect()
}

/// JavaScript `parseFloat`: the longest numeric prefix after leading
/// whitespace, `None` when there is none.
pub fn parse_float(s: &str) -> Option<f64> {
    let t = s.trim_start();
    let b = t.as_bytes();
    let mut end = 0;
    if end < b.len() && (b[end] == b'+' || b[end] == b'-') {
        end += 1;
    }
    if t[end..].starts_with("Infinity") {
        let v = if t.starts_with('-') {
            f64::NEG_INFINITY
        } else {
            f64::INFINITY
        };
        return Some(v);
    }
    let digits_start = end;
    while end < b.len() && b[end].is_ascii_digit() {
        end += 1;
    }
    let mut mantissa = end > digits_start;
    if end < b.len() && b[end] == b'.' {
        let mut j = end + 1;
        while j < b.len() && b[j].is_ascii_digit() {
            j += 1;
        }
        if j > end + 1 || mantissa {
            mantissa |= j > end + 1;
            end = j;
        }
    }
    if !mantissa {
        return None;
    }
    if end < b.len() && (b[end] == b'e' || b[end] == b'E') {
        let mut j = end + 1;
        if j < b.len() && (b[j] == b'+' || b[j] == b'-') {
            j += 1;
        }
        let exp_start = j;
        while j < b.len() && b[j].is_ascii_digit() {
            j += 1;
        }
        if j > exp_start {
            end = j;
        }
    }
    t[..end].parse().ok()
}

/// JavaScript `parseInt(s, 10)`.
pub fn parse_int(s: &str) -> Option<i64> {
    let t = s.trim_start();
    let (neg, rest) = match t.as_bytes().first() {
        Some(b'-') => (true, &t[1..]),
        Some(b'+') => (false, &t[1..]),
        _ => (false, t),
    };
    let digits: String = rest.chars().take_while(char::is_ascii_digit).collect();
    let v: i64 = digits.parse().ok()?;
    Some(if neg { -v } else { v })
}

/// JavaScript `Math.round`: halves round up (towards +∞).
pub fn js_round(x: f64) -> f64 {
    (x + 0.5).floor()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fields_split_like_dos_convert() {
        let d = Dos::parse("\n|a=1|b=2=3|\n|c|=x|a=4&d=5\n|e= 6 |", DosFlags::default());
        assert_eq!(d.get("a"), Some("4"));
        assert_eq!(d.get("b"), Some("2=3"));
        assert_eq!(d.get("c"), None);
        assert_eq!(d.get("d"), Some("5\n"));
        assert_eq!(d.get("e"), Some(" 6 "));
        let no_amp = Dos::parse(
            "|a=1&d=5|",
            DosFlags {
                ampersand_split: false,
                decode_uri: false,
            },
        );
        assert_eq!(no_amp.get("a"), Some("1&d=5"));
        let uri = Dos::parse(
            "|t=%E3%81%82%7C|",
            DosFlags {
                ampersand_split: true,
                decode_uri: true,
            },
        );
        assert_eq!(uri.get("t"), Some("あ|"));
    }

    #[test]
    fn js_number_parsing() {
        assert_eq!(parse_float(" 12.5abc"), Some(12.5));
        assert_eq!(parse_float("100+20"), Some(100.0));
        assert_eq!(parse_float(".5"), Some(0.5));
        assert_eq!(parse_float("5."), Some(5.0));
        assert_eq!(parse_float("-3e2x"), Some(-300.0));
        assert_eq!(parse_float("x1"), None);
        assert_eq!(parse_float(""), None);
        assert_eq!(parse_int("07a"), Some(7));
        assert_eq!(parse_int("-2.9"), Some(-2));
        assert_eq!(js_round(2.5), 3.0);
        assert_eq!(js_round(-2.5), -2.0);
        assert_eq!(split_lf2("1$2\n\n3\r\n4"), ["1", "2", "3", "4"]);
    }
}
