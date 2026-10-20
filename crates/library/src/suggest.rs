//! Suggestions while a search is typed (`docs/plans/ui-core.md`, step 4
//! and 6.2): song titles, artists and packs that the text typed so far
//! starts, folded and romanised as [`crate::search`] does, so `さく`
//! suggests `桜` transliterated `Sakura` and `sak` suggests `さくら`.
//!
//! Ranked, best first: what starts with the text, then what has a word
//! starting with it, then what merely contains it; within a rank titles
//! before artists before packs, then shorter before longer, then by the
//! folded text, then in the order given. The same input always gives the
//! same list. Matching runs over text prepared once ([`Suggester::new`]),
//! so a keystroke over a large library costs a scan of short strings.

use crate::search::{Fold, fold, romaji, romaji_folded};

/// What a suggestion is.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Kind {
    Title,
    Artist,
    Pack,
}

/// How well it matches.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Rank {
    Prefix,
    WordStart,
    Contains,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Suggestion {
    /// As the library spells it.
    pub text: String,
    pub kind: Kind,
    pub rank: Rank,
}

struct Entry {
    text: String,
    kind: Kind,
    plain: String,
    roman: String,
    /// Characters in `plain` (shorter first).
    len: u32,
}

/// The last query and what it matched, to narrow from as it grows.
struct Last {
    plain: String,
    roman: Option<String>,
    matched: Vec<u32>,
}

/// Candidates prepared for matching.
pub struct Suggester {
    entries: Vec<Entry>,
    last: std::cell::RefCell<Option<Last>>,
}

fn is_word_char(c: char) -> bool {
    c.is_alphanumeric() && !is_cjk(c)
}

/// Kana and kanji: every character starts a word (no spaces between).
fn is_cjk(c: char) -> bool {
    matches!(c, '\u{3040}'..='\u{30FF}' | '\u{3400}'..='\u{9FFF}' | '\u{F900}'..='\u{FAFF}')
}

fn rank(hay: &str, needle: &str) -> Option<Rank> {
    if needle.is_empty() || hay.is_empty() {
        return None;
    }
    let mut found = false;
    for (i, _) in hay.match_indices(needle) {
        if i == 0 {
            return Some(Rank::Prefix);
        }
        found = true;
        if !hay[..i].chars().next_back().is_some_and(is_word_char) {
            return Some(Rank::WordStart);
        }
    }
    found.then_some(Rank::Contains)
}

impl Suggester {
    /// From `(text, transliteration, kind)` candidates (the
    /// transliteration may be empty); the same text of the same kind is
    /// kept once, and folded once.
    pub fn new<'a>(
        candidates: impl IntoIterator<Item = (&'a str, &'a str, Kind)>,
        folder: &impl Fold,
    ) -> Suggester {
        let mut entries: Vec<Entry> = Vec::new();
        let mut seen_text = std::collections::HashSet::new();
        let mut seen_plain = std::collections::HashSet::new();
        for (text, translit, kind) in candidates {
            if !seen_text.insert((kind, text, translit)) {
                continue;
            }
            let plain = fold(text, folder);
            if plain.is_empty() || !seen_plain.insert((kind, plain.clone())) {
                continue;
            }
            let roman = if translit.trim().is_empty() {
                romaji_folded(&plain)
            } else {
                romaji(translit, folder)
            };
            entries.push(Entry {
                text: text.to_string(),
                kind,
                len: plain.chars().count() as u32,
                plain,
                roman,
            });
        }
        Suggester {
            entries,
            last: Default::default(),
        }
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// At most `limit` suggestions for `typed`, best first. Typing on
    /// (the text only growing) searches what the last query matched.
    pub fn suggest(&self, typed: &str, folder: &impl Fold, limit: usize) -> Vec<Suggestion> {
        let plain = fold(typed, folder);
        let plain = plain.trim();
        if plain.is_empty() || limit == 0 {
            return Vec::new();
        }
        let roman = romaji_folded(plain);
        // Too short a romanisation finds everything.
        let roman = (roman.chars().count() >= 2).then_some(roman);
        let mut last = self.last.borrow_mut();
        let narrow = last.as_ref().filter(|l| {
            plain.starts_with(l.plain.as_str())
                && match (&l.roman, &roman) {
                    (Some(a), Some(b)) => b.starts_with(a.as_str()),
                    (None, None) => true,
                    _ => false,
                }
        });
        let candidates: Box<dyn Iterator<Item = u32>> = match narrow {
            Some(l) => Box::new(l.matched.clone().into_iter()),
            None => Box::new(0..self.entries.len() as u32),
        };
        let mut found: Vec<(Rank, u32)> = candidates
            .filter_map(|i| {
                let e = &self.entries[i as usize];
                let a = rank(&e.plain, plain);
                let b = roman.as_deref().and_then(|r| rank(&e.roman, r));
                match (a, b) {
                    (Some(a), Some(b)) => Some((a.min(b), i)),
                    (a, b) => a.or(b).map(|r| (r, i)),
                }
            })
            .collect();
        *last = Some(Last {
            plain: plain.to_string(),
            roman: roman.clone(),
            matched: found.iter().map(|&(_, i)| i).collect(),
        });
        let order = |&(ra, ia): &(Rank, u32), &(rb, ib): &(Rank, u32)| {
            let (a, b) = (&self.entries[ia as usize], &self.entries[ib as usize]);
            ra.cmp(&rb)
                .then(a.kind.cmp(&b.kind))
                .then(a.len.cmp(&b.len))
                .then_with(|| a.plain.cmp(&b.plain))
                .then(ia.cmp(&ib))
        };
        // Only the best `limit` are sorted.
        if found.len() > limit {
            found.select_nth_unstable_by(limit - 1, order);
            found.truncate(limit);
        }
        found.sort_by(order);
        found
            .into_iter()
            .map(|(rank, i)| Suggestion {
                text: self.entries[i as usize].text.clone(),
                kind: self.entries[i as usize].kind,
                rank,
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The search tests' stand-in, enough for these.
    fn nfkd(text: &str) -> String {
        text.chars()
            .flat_map(|c| -> Vec<char> {
                match c {
                    'é' => vec!['e', '\u{0301}'],
                    'ジ' => vec!['シ', '\u{3099}'],
                    'ど' => vec!['と', '\u{3099}'],
                    'Ａ'..='Ｚ' => vec![char::from_u32(c as u32 - 0xFF21 + 'A' as u32).unwrap()],
                    _ => vec![c],
                }
            })
            .collect()
    }

    fn library() -> Suggester {
        Suggester::new(
            [
                ("Sakura Drops", "", Kind::Title),
                ("さくらんぼ", "", Kind::Title),
                ("桜", "Sakura", Kind::Title),
                ("Into My Dream", "", Kind::Title),
                ("Dream Catcher", "", Kind::Title),
                ("Daydream", "", Kind::Title),
                ("Sakuraba", "", Kind::Artist),
                ("Café Sakura", "", Kind::Pack),
                ("Sakura Drops", "", Kind::Title),
                ("ジョジョ", "", Kind::Title),
            ],
            &nfkd,
        )
    }

    fn texts(s: &[Suggestion]) -> Vec<&str> {
        s.iter().map(|s| s.text.as_str()).collect()
    }

    #[test]
    fn prefixes_then_word_starts_then_contains() {
        let l = library();
        assert_eq!(l.len(), 9, "the repeated title kept once");
        let s = l.suggest("dream", &nfkd, 10);
        assert_eq!(
            texts(&s),
            vec!["Dream Catcher", "Into My Dream", "Daydream"]
        );
        assert_eq!(
            s.iter().map(|s| s.rank).collect::<Vec<_>>(),
            vec![Rank::Prefix, Rank::WordStart, Rank::Contains]
        );
    }

    #[test]
    fn kana_and_romaji_meet() {
        let l = library();
        // Titles first (shorter first), then the artist, then the pack
        // (its word "Sakura", accents folded).
        let s = l.suggest("saku", &nfkd, 10);
        assert_eq!(
            texts(&s),
            vec![
                "桜",
                "さくらんぼ",
                "Sakura Drops",
                "Sakuraba",
                "Café Sakura"
            ]
        );
        // Kana typed finds the romanised title and the transliteration.
        let s = l.suggest("さくら", &nfkd, 3);
        assert_eq!(texts(&s), vec!["桜", "さくらんぼ", "Sakura Drops"]);
        // Romanisations people type meet (zyo for じょ).
        assert_eq!(texts(&l.suggest("zyozyo", &nfkd, 5)), vec!["ジョジョ"]);
        // Full-width letters and case fold.
        assert_eq!(texts(&l.suggest("ＤＡＹ", &nfkd, 5)), vec!["Daydream"]);
        assert_eq!(texts(&l.suggest("cafe", &nfkd, 5)), vec!["Café Sakura"]);
    }

    #[test]
    fn capped_deterministic_and_empty_for_nothing() {
        let l = library();
        assert_eq!(l.suggest("sakura", &nfkd, 2).len(), 2);
        assert_eq!(
            l.suggest("sakura", &nfkd, 10),
            l.suggest("sakura", &nfkd, 10)
        );
        assert!(l.suggest("  ", &nfkd, 10).is_empty());
        assert!(l.suggest("zzzz", &nfkd, 10).is_empty());
        assert!(l.suggest("dream", &nfkd, 0).is_empty());
    }
}
