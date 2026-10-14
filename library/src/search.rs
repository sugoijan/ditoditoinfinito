//! Text search over the song list: a query's words each found somewhere
//! in a song's text (title, artist, pack, credits, chart names and step
//! artists), in any order.
//!
//! Both sides are folded the same way so that what a player types matches
//! however the simfile spells it: Unicode compatibility decomposition
//! (full-width letters become ASCII, `é` becomes `e` plus an accent),
//! accents dropped, lower case, katakana as hiragana. The decomposition is
//! passed in ([`Fold`]): the app uses the browser's
//! `String.prototype.normalize("NFKD")`, which keeps Unicode's tables out of
//! the wasm; the tests use a stand-in.

/// Unicode NFKD of a string.
pub trait Fold {
    fn nfkd(&self, text: &str) -> String;
}

impl<F: Fn(&str) -> String> Fold for F {
    fn nfkd(&self, text: &str) -> String {
        self(text)
    }
}

/// `text` as searched: decomposed, without combining marks, lower case,
/// katakana as hiragana, runs of white space as one space.
pub fn fold(text: &str, folder: &impl Fold) -> String {
    let mut out = String::with_capacity(text.len());
    let mut space = false;
    for c in folder.nfkd(text).chars() {
        if is_combining(c) {
            continue;
        }
        if c.is_whitespace() {
            space = !out.is_empty();
            continue;
        }
        if space {
            out.push(' ');
            space = false;
        }
        match c {
            // Katakana ァ … ヶ to hiragana ぁ … ゖ.
            '\u{30A1}'..='\u{30F6}' => {
                out.push(char::from_u32(c as u32 - 0x60).unwrap_or(c));
            }
            _ => out.extend(c.to_lowercase()),
        }
    }
    out
}

/// Combining marks NFKD leaves after a base letter: the general diacritics
/// and the kana voicing marks (`が` is `か` plus U+3099).
fn is_combining(c: char) -> bool {
    matches!(c,
        '\u{0300}'..='\u{036F}'
        | '\u{1AB0}'..='\u{1AFF}'
        | '\u{1DC0}'..='\u{1DFF}'
        | '\u{20D0}'..='\u{20FF}'
        | '\u{3099}'..='\u{309A}'
        | '\u{FE20}'..='\u{FE2F}')
}

/// A search: the folded words of what was typed.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Query {
    words: Vec<String>,
}

impl Query {
    pub fn new(text: &str, folder: &impl Fold) -> Query {
        Query {
            words: fold(text, folder)
                .split(' ')
                .filter(|w| !w.is_empty())
                .map(str::to_string)
                .collect(),
        }
    }

    /// Nothing typed (or only spaces): everything matches.
    pub fn is_empty(&self) -> bool {
        self.words.is_empty()
    }

    /// Whether every word is in `haystack`, a [`haystack`].
    pub fn matches(&self, haystack: &str) -> bool {
        self.matches_in(&[haystack])
    }

    /// Whether every word is in one of `haystacks` (a song's own text and
    /// the text of the charts shown, say).
    pub fn matches_in(&self, haystacks: &[&str]) -> bool {
        self.words
            .iter()
            .all(|w| haystacks.iter().any(|h| h.contains(w.as_str())))
    }
}

/// A song's searchable text, folded once: its fields joined by line
/// breaks, so that no word matches across two of them.
pub fn haystack<'a>(fields: impl IntoIterator<Item = &'a str>, folder: &impl Fold) -> String {
    fields
        .into_iter()
        .filter(|f| !f.trim().is_empty())
        .map(|f| fold(f, folder))
        .collect::<Vec<_>>()
        .join("\n")
}

#[cfg(test)]
mod tests {
    use super::*;

    /// NFKD for the characters these tests use.
    fn nfkd(text: &str) -> String {
        text.chars()
            .flat_map(|c| -> Vec<char> {
                match c {
                    'é' => vec!['e', '\u{0301}'],
                    'Ö' => vec!['O', '\u{0308}'],
                    'ö' => vec!['o', '\u{0308}'],
                    'ド' => vec!['ト', '\u{3099}'],
                    'バ' => vec!['ハ', '\u{3099}'],
                    'ば' => vec!['は', '\u{3099}'],
                    'ど' => vec!['と', '\u{3099}'],
                    'Ａ'..='Ｚ' => vec![char::from_u32(c as u32 - 0xFF21 + 'A' as u32).unwrap()],
                    '\u{3000}' => vec![' '],
                    _ => vec![c],
                }
            })
            .collect()
    }

    fn song(fields: &[&str]) -> String {
        haystack(fields.iter().copied(), &nfkd)
    }

    #[test]
    fn every_word_in_any_field_any_order() {
        let s = song(&[
            "Bad Apple!!",
            "Alstroemeria Records",
            "Touhou Fujinya",
            "",
            "nomu",
        ]);
        for q in [
            "bad",
            "APPLE bad",
            "touhou apple",
            "nomu",
            "records alst",
            "  bad   apple ",
        ] {
            assert!(Query::new(q, &nfkd).matches(&s), "{q}");
        }
        for q in ["bad orange", "apple!!!", "applealst"] {
            assert!(!Query::new(q, &nfkd).matches(&s), "{q}");
        }
        // A word is not found across two fields.
        assert!(!Query::new("xst", &nfkd).matches(&song(&["xs", "tx"])));
    }

    #[test]
    fn accents_width_and_kana_fold() {
        let s = song(&["Pokémon", "ＤＪ　ÖSTER", "バッド"]);
        for q in [
            "pokemon",
            "Pokémon",
            "dj öster",
            "dj oster",
            "ばっど",
            "バッド",
            "はっと",
        ] {
            assert!(Query::new(q, &nfkd).matches(&s), "{q}");
        }
    }

    #[test]
    fn words_may_be_in_different_haystacks() {
        let (a, b) = (song(&["Bad Apple!!"]), song(&["nomu"]));
        assert!(Query::new("apple nomu", &nfkd).matches_in(&[&a, &b]));
        assert!(!Query::new("apple nomu", &nfkd).matches_in(&[&a]));
        assert!(Query::new("", &nfkd).matches_in(&[]));
    }

    #[test]
    fn an_empty_query_is_empty() {
        assert!(Query::new("", &nfkd).is_empty());
        assert!(Query::new("  \u{3000} ", &nfkd).is_empty());
        assert!(Query::new("", &nfkd).matches(""));
    }
}
