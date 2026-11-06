//! What an announcer might say on each line, for the studio
//! (`docs/plans/announcer-studio.md`, decision 3): two sets, English and
//! Japanese. Most examples are our packs' own sounds, each with its text
//! (`Mr. Clank` and `ガキンさん`, the texts their `PROVENANCE.toml` carries
//! and the web's `announcers/index.json` lists); more, as text, are in
//! `examples.toml` beside this file (turned into a table at build time,
//! `build.rs`), such as for the lines our packs say only two ways.
//! Suggestions only: nothing checks a recording against them.

use super::Line;

/// A set of examples.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Set {
    #[default]
    English,
    Japanese,
}

impl Set {
    pub const ALL: [Set; 2] = [Set::English, Set::Japanese];

    /// Its name, in its own language.
    pub fn name(self) -> &'static str {
        match self {
            Set::English => "English",
            Set::Japanese => "日本語",
        }
    }

    /// The key a setting keeps it under.
    pub fn key(self) -> &'static str {
        match self {
            Set::English => "en",
            Set::Japanese => "ja",
        }
    }

    pub fn from_key(key: &str) -> Option<Set> {
        Set::ALL.into_iter().find(|s| s.key() == key)
    }

    /// The name of our pack whose sounds are this set's examples.
    pub fn pack(self) -> &'static str {
        match self {
            Set::English => "Mr. Clank",
            Set::Japanese => "ガキンさん",
        }
    }
}

/// A hundred's combo in words: "Two hundred", "Fifteen hundred", "Two
/// thousand" (digits past 9,900).
fn english(n: u32) -> String {
    const ONES: [&str; 20] = [
        "",
        "One",
        "Two",
        "Three",
        "Four",
        "Five",
        "Six",
        "Seven",
        "Eight",
        "Nine",
        "Ten",
        "Eleven",
        "Twelve",
        "Thirteen",
        "Fourteen",
        "Fifteen",
        "Sixteen",
        "Seventeen",
        "Eighteen",
        "Nineteen",
    ];
    const TENS: [&str; 10] = [
        "", "", "Twenty", "Thirty", "Forty", "Fifty", "Sixty", "Seventy", "Eighty", "Ninety",
    ];
    let hundreds = n / 100;
    if n.is_multiple_of(1000) && (1..10).contains(&(n / 1000)) {
        format!("{} thousand", ONES[(n / 1000) as usize])
    } else if hundreds < 20 {
        format!("{} hundred", ONES[hundreds as usize])
    } else if hundreds < 100 {
        let (t, o) = (hundreds / 10, hundreds % 10);
        let tens = TENS[t as usize];
        if o == 0 {
            format!("{tens} hundred")
        } else {
            format!("{tens}-{} hundred", ONES[o as usize].to_lowercase())
        }
    } else {
        n.to_string()
    }
}

/// A hundred's combo in kanji: 百, 二百, 千五百, 二千 (digits past 9,900).
fn japanese(n: u32) -> String {
    const DIGITS: [&str; 10] = ["", "", "二", "三", "四", "五", "六", "七", "八", "九"];
    if n >= 10_000 {
        return n.to_string();
    }
    let place = |d: u32, unit: &str| match d {
        0 => String::new(),
        d => format!("{}{unit}", DIGITS[d as usize]),
    };
    place(n / 1000, "千") + &place(n / 100 % 10, "百")
}

include!(concat!(env!("OUT_DIR"), "/examples.rs"));

/// A combo in `set`'s words: "One hundred", 百.
fn in_words(set: Set, n: u32) -> String {
    match set {
        Set::English => english(n),
        Set::Japanese => japanese(n),
    }
}

/// More examples of `line` in `set`, beside our pack's: those
/// `examples.toml` gives it.
pub fn more(set: Set, line: Line) -> Vec<String> {
    let mut out = Vec::new();
    for (key, folder, examples) in MORE {
        if *key != set.key() {
            continue;
        }
        let words = match line {
            Line::Combo(n) if folder.contains("{n}") => {
                if Line::parse(&folder.replace("{n}", &n.to_string())) != Some(line) {
                    continue;
                }
                Some(in_words(set, n))
            }
            _ if Line::parse(folder) == Some(line) => None,
            _ => continue,
        };
        out.extend(examples.iter().map(|e| match &words {
            Some(w) => e.replace("{n}", w),
            None => e.to_string(),
        }));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn combos_are_said_in_words() {
        let en: Vec<String> = [100, 900, 1000, 1100, 1500, 2000, 2500, 9900, 12_000]
            .map(english)
            .into();
        assert_eq!(
            en,
            [
                "One hundred",
                "Nine hundred",
                "One thousand",
                "Eleven hundred",
                "Fifteen hundred",
                "Two thousand",
                "Twenty-five hundred",
                "Ninety-nine hundred",
                "12000"
            ]
        );
        let ja: Vec<String> = [100, 200, 1000, 1500, 2000, 9900].map(japanese).into();
        assert_eq!(ja, ["百", "二百", "千", "千五百", "二千", "九千九百"]);
    }

    /// The lines our packs say only two ways have more, in both sets.
    #[test]
    fn the_thin_lines_have_more() {
        for line in [
            Line::Combo(300),
            Line::FullCombo,
            Line::FullComboW(2),
            Line::FullComboW(3),
            Line::EvaluationFullCombo(1),
            Line::EvaluationFullCombo(4),
        ] {
            for set in Set::ALL {
                assert!(more(set, line).len() >= 2, "{set:?} {line:?}");
            }
        }
        assert!(more(Set::English, Line::Ready).is_empty());
        assert_eq!(Set::from_key("ja"), Some(Set::Japanese));
        assert_eq!(
            more(Set::English, Line::Combo(1500))[0],
            "Fifteen hundred and counting!"
        );
        assert_eq!(more(Set::Japanese, Line::Combo(200))[0], "二百コンボ突破！");
    }

    /// Every set in `examples.toml` is one, every folder a line's (`{n}`
    /// a combo's), and `{n}` in examples only where it stands for one.
    #[test]
    fn the_examples_file_names_sets_and_lines() {
        assert!(!MORE.is_empty());
        for (set, folder, examples) in MORE {
            assert!(Set::from_key(set).is_some(), "set {set}");
            let combo = folder.contains("{n}");
            let line = Line::parse(&folder.replace("{n}", "100"));
            assert!(
                line.is_some_and(|l| combo == matches!(l, Line::Combo(_))),
                "{set}: {folder} is no line"
            );
            assert!(!examples.is_empty(), "{set}: {folder}");
            for e in *examples {
                assert!(!e.trim().is_empty(), "{set}: {folder}");
                assert!(combo || !e.contains("{n}"), "{set}: {folder}: {e}");
            }
        }
    }
}
