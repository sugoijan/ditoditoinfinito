//! `metrics.ini` as StepMania reads it (`IniFile::ReadFile`,
//! `src/IniFile.cpp`, 5_1-new), and its number parsing.

use std::collections::HashMap;

/// Sections in file order, each with its keys in order. Names are
/// case-sensitive, as StepMania's `XNode` children and attributes are.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Ini {
    sections: Vec<Section>,
    by_name: HashMap<String, usize>,
}

#[derive(Clone, Debug, Default, PartialEq)]
struct Section {
    name: String,
    keys: Vec<(String, String)>,
    by_key: HashMap<String, usize>,
}

impl Section {
    fn set(&mut self, key: &str, value: &str) {
        match self.by_key.get(key) {
            Some(&i) => self.keys[i].1 = value.to_string(),
            None => self.add(key, value),
        }
    }

    fn add(&mut self, key: &str, value: &str) {
        self.by_key.insert(key.to_string(), self.keys.len());
        self.keys.push((key.to_string(), value.to_string()));
    }
}

impl Ini {
    /// Reads `text` line by line: a trailing `\` joins the next line, a
    /// line starting with `;`, `#`, `//` or `--` is a comment, `[name]` opens
    /// (or reopens) a section, `key=value` sets a key in the current one with
    /// the key trimmed and the value kept as is (a later key wins). Lines
    /// before any section and lines without `=` are ignored.
    pub fn parse(text: &str) -> Ini {
        let mut ini = Ini::default();
        let mut current: Option<usize> = None;
        let mut line = String::new();
        // `str::lines` drops each line's `\r\n`, as `RageFile::GetLine` does.
        for part in text.lines() {
            // `utf8_remove_bom` runs on every line read.
            line.push_str(part.strip_prefix('\u{feff}').unwrap_or(part));
            if line.ends_with('\\') {
                line.pop();
                continue;
            }
            ini.read_line(&line, &mut current);
            line.clear();
        }
        // A continued line cut off by the end of the file is dropped.
        ini
    }

    fn read_line(&mut self, line: &str, current: &mut Option<usize>) {
        let bytes = line.as_bytes();
        let Some(&first) = bytes.first() else {
            return;
        };
        let comment = match first {
            b';' | b'#' => true,
            b'/' | b'-' => bytes.get(1) == Some(&first),
            _ => false,
        };
        if comment {
            return;
        }
        if first == b'[' && line.len() >= 2 && line.ends_with(']') {
            let name = &line[1..line.len() - 1];
            *current = Some(self.section_index(name));
            return;
        }
        let (Some(section), Some(eq)) = (*current, line.find('=')) else {
            return;
        };
        let key = line[..eq].trim_matches(TRIM);
        if !key.is_empty() {
            self.sections[section].set(key, &line[eq + 1..]);
        }
    }

    fn section_index(&mut self, name: &str) -> usize {
        if let Some(&i) = self.by_name.get(name) {
            return i;
        }
        self.by_name.insert(name.to_string(), self.sections.len());
        self.sections.push(Section {
            name: name.to_string(),
            ..Section::default()
        });
        self.sections.len() - 1
    }

    /// The value of `key` in `section`.
    pub fn get(&self, section: &str, key: &str) -> Option<&str> {
        let s = &self.sections[*self.by_name.get(section)?];
        s.by_key.get(key).map(|&i| s.keys[i].1.as_str())
    }

    /// Adds what `other` has and `self` lacks: whole sections, and keys of
    /// sections both have (`XmlFileUtil::MergeIniUnder`, `src/XmlFileUtil.cpp`).
    pub fn merge_under(&mut self, other: &Ini) {
        for theirs in &other.sections {
            let i = self.section_index(&theirs.name);
            let ours = &mut self.sections[i];
            for (k, v) in &theirs.keys {
                if !ours.by_key.contains_key(k) {
                    ours.add(k, v);
                }
            }
        }
    }

    /// Section names in file order.
    pub fn section_names(&self) -> impl Iterator<Item = &str> {
        self.sections.iter().map(|s| s.name.as_str())
    }
}

/// StepMania's `Trim` default: spaces, tabs, carriage returns, newlines.
const TRIM: &[char] = &[' ', '\t', '\r', '\n'];

/// `StringToFloat` (`src/RageUtil.cpp`): `strtof` on the leading number,
/// whatever follows; 0 when there is none or it is not finite.
pub fn string_to_float(s: &str) -> f32 {
    let t = s.trim_start_matches(|c: char| c.is_ascii_whitespace());
    let b = t.as_bytes();
    let mut i = 0;
    if i < b.len() && (b[i] == b'+' || b[i] == b'-') {
        i += 1;
    }
    let digits_start = i;
    while i < b.len() && b[i].is_ascii_digit() {
        i += 1;
    }
    let mut digits = i - digits_start;
    if i < b.len() && b[i] == b'.' {
        i += 1;
        let frac = i;
        while i < b.len() && b[i].is_ascii_digit() {
            i += 1;
        }
        digits += i - frac;
    }
    if digits == 0 {
        // `strtof` also reads "inf", "nan" and hex floats; the first two are
        // not finite (0 here), hex floats do not occur in skins.
        return 0.0;
    }
    if i < b.len() && (b[i] == b'e' || b[i] == b'E') {
        let mut j = i + 1;
        if j < b.len() && (b[j] == b'+' || b[j] == b'-') {
            j += 1;
        }
        let exp = j;
        while j < b.len() && b[j].is_ascii_digit() {
            j += 1;
        }
        if j > exp {
            i = j;
        }
    }
    match t[..i].parse::<f32>() {
        Ok(v) if v.is_finite() => v,
        _ => 0.0,
    }
}

/// `StringToInt` (`std::stoi` with 0 on failure): the leading integer.
pub fn string_to_int(s: &str) -> i32 {
    let t = s.trim_start_matches(|c: char| c.is_ascii_whitespace());
    let b = t.as_bytes();
    let mut i = 0;
    if i < b.len() && (b[i] == b'+' || b[i] == b'-') {
        i += 1;
    }
    let start = i;
    while i < b.len() && b[i].is_ascii_digit() {
        i += 1;
    }
    if i == start {
        return 0;
    }
    // Out of range is an exception in `stoi`, caught as 0.
    t[..i].parse::<i32>().unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_sections_keys_and_comments_as_stepmania_does() {
        let ini = Ini::parse(
            "\u{feff}[Global]\r\nFallbackNoteSkin=default\r\n; c\n# c\n// c\n-- c\n\
             -x=1\n/y=2\n  Key  =  value with spaces \n[NoteDisplay]\nA=1\nA=2\n\
             StopDrawingHoldBodyOffsetFromTail=-32\t// top of tail\n\
             Long=a,\\\nb\nnoequals\n[Global]\nB=3\n",
        );
        assert_eq!(ini.get("Global", "FallbackNoteSkin"), Some("default"));
        // A single `-` or `/` is not a comment.
        assert_eq!(ini.get("Global", "-x"), Some("1"));
        assert_eq!(ini.get("Global", "/y"), Some("2"));
        // Only the key is trimmed.
        assert_eq!(ini.get("Global", "Key"), Some("  value with spaces "));
        assert_eq!(ini.get("NoteDisplay", "A"), Some("2"));
        assert_eq!(
            ini.get("NoteDisplay", "StopDrawingHoldBodyOffsetFromTail"),
            Some("-32\t// top of tail")
        );
        assert_eq!(ini.get("NoteDisplay", "Long"), Some("a,b"));
        // A reopened section adds to the first.
        assert_eq!(ini.get("Global", "B"), Some("3"));
        assert_eq!(
            ini.section_names().collect::<Vec<_>>(),
            ["Global", "NoteDisplay"]
        );
        // Case-sensitive.
        assert_eq!(ini.get("global", "B"), None);
    }

    #[test]
    fn keys_before_a_section_and_blank_lines_are_ignored() {
        let ini = Ini::parse("a=1\n\n\n[S]\n\nb=2");
        assert_eq!(ini.get("S", "b"), Some("2"));
        assert_eq!(ini.get("", "a"), None);
        // `[` without `]` is a key line.
        let ini = Ini::parse("[S]\n[x=1");
        assert_eq!(ini.get("S", "[x"), Some("1"));
        assert_eq!(Ini::parse(""), Ini::default());
        let cut = Ini::parse("[S]\nx=1\\");
        assert_eq!(cut.get("S", "x"), None);
        assert_eq!(cut.section_names().collect::<Vec<_>>(), ["S"]);
    }

    #[test]
    fn merging_under_keeps_what_is_there() {
        let mut skin = Ini::parse("[NoteDisplay]\nA=skin\n");
        skin.merge_under(&Ini::parse("[NoteDisplay]\nA=base\nB=base\n[Other]\nC=1\n"));
        assert_eq!(skin.get("NoteDisplay", "A"), Some("skin"));
        assert_eq!(skin.get("NoteDisplay", "B"), Some("base"));
        assert_eq!(skin.get("Other", "C"), Some("1"));
    }

    #[test]
    fn numbers_are_read_like_strtof_and_stoi() {
        assert_eq!(string_to_float("-32\t// top of tail"), -32.0);
        assert_eq!(string_to_float(" .125"), 0.125);
        assert_eq!(string_to_float("1e2x"), 100.0);
        assert_eq!(string_to_float("1e"), 1.0);
        assert_eq!(string_to_float("abc"), 0.0);
        assert_eq!(string_to_float("1e99"), 0.0);
        assert_eq!(string_to_float("-"), 0.0);
        assert_eq!(string_to_int(" 12px"), 12);
        assert_eq!(string_to_int("-7"), -7);
        assert_eq!(string_to_int("0.9"), 0);
        assert_eq!(string_to_int("99999999999"), 0);
        assert_eq!(string_to_int(""), 0);
    }
}
