//! The DDI ruleset's judgment words in the player's language
//! (`docs/plans/ddi-ruleset.md`, "Names in other languages"; round 4):
//! the messages of `locales/<tag>/judgments.ftl`, shown where the option
//! is on (Options → Rules → Judgment words, `Settings::judgment_words`,
//! on by default), else the ruleset's English data as before.
//!
//! The option is the thread's, as the language is: the shells set it
//! from the settings ([`set_localized`]) where they choose the language.

use std::cell::Cell;

use crate::UiText;

thread_local! {
    static LOCALIZED: Cell<bool> = const { Cell::new(true) };
}

/// One of DDI's words: the tiers' (W1 to W5 by side), W0's, a Miss's and
/// a Boo's, the results' label for the Boos counted, and a hold's held to
/// its end and dropped.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Word {
    W0,
    /// Tier 1 to 5 (W1 … W5), early.
    Early(u8),
    /// Tier 1 to 5, late: each language's late words carry its hint (a G
    /// in English, a voicing mark in Japanese).
    Late(u8),
    Miss,
    Boo,
    Boos,
    /// A hold or roll held to its end.
    Held,
    /// A hold or roll let go before its end ("Dropped", since 2026-11-14;
    /// "Off" before: one message, so that a rename is one line).
    Dropped,
}

impl Word {
    /// Every word, W0 first, each tier's early then late.
    pub const ALL: [Word; 16] = [
        Word::W0,
        Word::Early(1),
        Word::Late(1),
        Word::Early(2),
        Word::Late(2),
        Word::Early(3),
        Word::Late(3),
        Word::Early(4),
        Word::Late(4),
        Word::Early(5),
        Word::Late(5),
        Word::Miss,
        Word::Boo,
        Word::Boos,
        Word::Held,
        Word::Dropped,
    ];

    /// The message, in the current language.
    pub fn message(self) -> UiText {
        use crate::msg::judgments as m;
        match self {
            Word::W0 => m::w0(),
            Word::Early(1) => m::w1_early(),
            Word::Late(1) => m::w1_late(),
            Word::Early(2) => m::w2_early(),
            Word::Late(2) => m::w2_late(),
            Word::Early(3) => m::w3_early(),
            Word::Late(3) => m::w3_late(),
            Word::Early(4) => m::w4_early(),
            Word::Late(4) => m::w4_late(),
            Word::Early(_) => m::w5_early(),
            Word::Late(_) => m::w5_late(),
            Word::Miss => m::miss(),
            Word::Boo => m::boo(),
            Word::Boos => m::boos(),
            Word::Held => m::held(),
            Word::Dropped => m::dropped(),
        }
    }
}

/// Shows DDI's judgment words in the player's language (`true`, the
/// default) or as the ruleset's English data.
pub fn set_localized(localized: bool) {
    LOCALIZED.with(|l| l.set(localized));
}

/// Whether DDI's judgment words are shown in the player's language.
pub fn localized() -> bool {
    LOCALIZED.with(Cell::get)
}

/// `word` as shown: its message where the option is on, `None` where the
/// ruleset's English data is shown instead.
pub fn word(word: Word) -> Option<UiText> {
    localized().then(|| word.message())
}

/// [`word`] by DDI's tier number, as text: 0 W0, 1 to 5 W1 to W5 (by
/// `late`), 6 Miss, 7 Boo, 8 a hold held, 9 one dropped. What
/// `ddi_engine`'s `JudgeNames::shown` asks.
pub fn ddi_word(tier: u8, late: bool) -> Option<String> {
    let w = match (tier, late) {
        (0, _) => Word::W0,
        (1..=5, false) => Word::Early(tier),
        (1..=5, true) => Word::Late(tier),
        (6, _) => Word::Miss,
        (7, _) => Word::Boo,
        (8, _) => Word::Held,
        _ => Word::Dropped,
    };
    word(w).map(String::from)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn with<T>(tag: &str, f: impl FnOnce() -> T) -> T {
        assert!(crate::force(tag));
        let out = f();
        crate::unforce();
        crate::choose(Some("en"));
        out
    }

    /// The hint: every late word has its language's mark, no early word
    /// does (a G in English; a voicing mark, ゛ or ゜'s voiced kin, in
    /// Japanese: ガ–ド, バ–ボ, ヴ).
    #[test]
    fn every_late_word_has_its_hint_and_no_early_word_does() {
        let voiced = |s: &str| {
            s.chars().any(|c| {
                // Katakana with a dakuten, composed, or the mark alone.
                matches!(c, 'ガ' | 'ギ' | 'グ' | 'ゲ' | 'ゴ' | 'ザ' | 'ジ' | 'ズ' | 'ゼ' | 'ゾ'
                    | 'ダ' | 'ヂ' | 'ヅ' | 'デ' | 'ド' | 'バ' | 'ビ' | 'ブ' | 'ベ' | 'ボ'
                    | 'ヴ' | 'ヷ' | 'ヸ' | 'ヹ' | 'ヺ' | '\u{3099}' | '゛')
            })
        };
        let g = |s: &str| s.chars().any(|c| c == 'g' || c == 'G');
        // English's W5 says it plainly ("Too early", "Too late": the plan), so
        // its hint is W1 to W4's; Japanese keeps it on all five.
        type Hint<'a> = (&'a str, &'a dyn Fn(&str) -> bool, u8);
        let hints: [Hint; 2] = [("en", &g, 4), ("ja", &voiced, 5)];
        for (tag, hint, tiers) in hints {
            with(tag, || {
                for tier in 1..=tiers {
                    let early = Word::Early(tier).message();
                    let late = Word::Late(tier).message();
                    assert!(hint(&late), "{tag}: late {tier} {late}");
                    assert!(!hint(&early), "{tag}: early {tier} {early}");
                    assert_ne!(early.as_str(), late.as_str());
                }
            });
        }
    }

    #[test]
    fn english_is_the_ruleset_s_own_words() {
        with("en", || {
            let words: Vec<String> = Word::ALL.iter().map(|w| w.message().to_string()).collect();
            assert_eq!(
                words,
                [
                    "Absolute", "Awesome", "Amazing", "Fantastic", "Glorious", "Nice", "Great",
                    "OK", "Good", "Too early", "Too late", "Miss", "Boo", "Boos", "Held",
                    "Dropped"
                ]
            );
        });
    }

    #[test]
    fn the_option_turns_the_words_off() {
        with("ja", || {
            assert_eq!(word(Word::Late(3)).unwrap().as_str(), "グレート");
            set_localized(false);
            assert_eq!(word(Word::Late(3)), None);
            set_localized(true);
        });
    }
}
