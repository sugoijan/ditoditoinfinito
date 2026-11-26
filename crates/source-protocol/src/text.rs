//! A plugin's own text (`docs/plans/source-plugins.md`, "Text from a
//! plugin"): data the app shows framed by its own messages, never a
//! sentence or a button of the app's, and never in `locales/`.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::limits::{MAX_LANGUAGES, MAX_TAG_BYTES};

/// A label in the languages the plugin gives it: language tags (`en`,
/// `ja`, `pt-BR`) to text, English required.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct Text(pub BTreeMap<String, String>);

impl Text {
    /// English alone.
    pub fn en(text: impl Into<String>) -> Text {
        Text(BTreeMap::from([("en".to_string(), text.into())]))
    }

    /// The same text with `tag`'s too.
    pub fn with(mut self, tag: impl Into<String>, text: impl Into<String>) -> Text {
        self.0.insert(tag.into(), text.into());
        self
    }

    /// The text for `language` (a tag as the app's settings name it): its
    /// own, else its primary language's (`pt` for `pt-BR`), else English.
    pub fn get(&self, language: &str) -> &str {
        let primary = language.split('-').next().unwrap_or(language);
        self.0
            .get(language)
            .or_else(|| self.0.get(primary))
            .or_else(|| self.0.get("en"))
            .map_or("", String::as_str)
    }

    /// Why it is not a label of at most `max_chars` characters a
    /// language, or `None`.
    pub fn problem(&self, max_chars: usize) -> Option<String> {
        if self.0.len() > MAX_LANGUAGES {
            return Some(format!("more than {MAX_LANGUAGES} languages"));
        }
        if !self.0.contains_key("en") {
            return Some("no English text".into());
        }
        for (tag, text) in &self.0 {
            if !is_tag(tag) {
                return Some(format!("not a language tag: {tag:?}"));
            }
            if let Some(p) = text_problem(text, max_chars) {
                return Some(format!("{tag}: {p}"));
            }
        }
        None
    }
}

/// A BCP 47-shaped tag: ASCII letters, digits and `-`, a letter first.
fn is_tag(tag: &str) -> bool {
    !tag.is_empty()
        && tag.len() <= MAX_TAG_BYTES
        && tag.starts_with(|c: char| c.is_ascii_alphabetic())
        && tag
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-')
}

/// Why `text` is not a plugin's text of at most `max_chars` characters
/// (empty after [`clean`] counts as missing), or `None`.
pub fn text_problem(text: &str, max_chars: usize) -> Option<String> {
    if text.chars().count() > max_chars {
        return Some(format!("longer than {max_chars} characters"));
    }
    if clean(text).is_empty() {
        return Some("empty".into());
    }
    None
}

/// Control and bidirectional control characters (which could reorder or
/// hide what the app shows around the text) taken out, and the ends
/// trimmed: how the app shows any text of a plugin's.
pub fn clean(text: &str) -> String {
    let kept: String = text
        .chars()
        .map(|c| if c == '\n' || c == '\t' { ' ' } else { c })
        .filter(|&c| !c.is_control() && !is_bidi_control(c))
        .collect();
    kept.trim().to_string()
}

/// The characters that change the direction text is laid out in
/// (Unicode's `Bidi_Control`).
pub fn is_bidi_control(c: char) -> bool {
    matches!(
        c,
        '\u{061C}' | '\u{200E}' | '\u{200F}' | '\u{202A}'..='\u{202E}' | '\u{2066}'..='\u{2069}'
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_label_falls_back_to_its_primary_language_then_english() {
        let t = Text::en("Songs").with("pt", "Músicas").with("ja", "曲");
        assert_eq!(t.get("pt-BR"), "Músicas");
        assert_eq!(t.get("ja"), "曲");
        assert_eq!(t.get("fr"), "Songs");
        assert!(t.problem(128).is_none());
        assert!(Text::default().problem(128).is_some());
        assert!(Text::en("x".repeat(129)).problem(128).is_some());
        assert!(Text::en("ok").with("not a tag", "x").problem(128).is_some());
        assert!(Text::en("\u{202E}\u{0007}").problem(128).is_some());
    }

    #[test]
    fn control_and_direction_characters_are_taken_out() {
        assert_eq!(clean(" a\u{202E}b\u{0000}c\nd\u{2066} "), "abc d");
    }
}
