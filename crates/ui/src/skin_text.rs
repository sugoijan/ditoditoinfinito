//! The words of a bundled skin's own options (`docs/plans/skin-parameters.md`,
//! "The bundled skins' words"): for a skin the game ships, its parameters'
//! labels, descriptions and choices' labels are the game's messages, in
//! the UI's language and at the level's wording; for any other skin, and
//! where a message is missing, the skin's own text from its `metrics.ini`,
//! as data. What a skin's Lua and the settings know (the parameters'
//! names, the choices' ids) never changes.
//!
//! Each message is named by the skin's key and the parameter's name, in
//! `locales/<tag>/skins.ftl`: `skins-<game>-<skin>-<parameter>` its label,
//! `…-description` its description and `…-choice-<id>` a choice's label,
//! each part in lower case with a hyphen where a capital starts a word
//! (Infinito's `NoteTimedCharge` is
//! `skins-dance-infinito-note-timed-charge`). A bundled skin that declares
//! a parameter without its messages fails this module's tests.

use ddi_l10n::UiText;
use ddi_noteskin::bundled::KEY_PREFIX;
use ddi_noteskin::params::{Choice, Kind, Parameter};

/// The messages' area, the start of their ids.
const AREA: &str = "skins";

/// The id of parameter `name`'s label of the skin keyed `key`, the start
/// of its other messages' ids; `None` for a skin the game does not ship.
pub fn message_id(key: &str, name: &str) -> Option<String> {
    let skin = key.strip_prefix(KEY_PREFIX)?;
    Some(format!("{AREA}-{}-{}", part(skin), part(name)))
}

/// `s` as a part of an id: lower case, a hyphen where a capital follows a
/// small letter or a digit and in the place of what an id cannot hold.
fn part(s: &str) -> String {
    let mut out = String::new();
    let mut after_small = false;
    for c in s.chars() {
        if c.is_ascii_uppercase() {
            if after_small {
                out.push('-');
            }
            out.push(c.to_ascii_lowercase());
            after_small = false;
        } else if c.is_ascii_alphanumeric() || c == '_' {
            out.push(c);
            after_small = c != '_';
        } else {
            if !out.is_empty() && !out.ends_with('-') {
                out.push('-');
            }
            after_small = false;
        }
    }
    out
}

/// Message `id + suffix` where the skin is bundled and the message exists.
fn message(key: &str, p: &Parameter, suffix: &str) -> Option<UiText> {
    ddi_l10n::message(&format!("{}{suffix}", message_id(key, &p.name)?))
}

/// Parameter `p`'s label as skin `key`'s row shows it.
pub fn label(key: &str, p: &Parameter) -> UiText {
    message(key, p, "").unwrap_or_else(|| UiText::data(&p.label))
}

/// Parameter `p`'s description as skin `key`'s row shows it.
pub fn description(key: &str, p: &Parameter) -> UiText {
    message(key, p, "-description").unwrap_or_else(|| UiText::data(&p.description))
}

/// Choice `c` of parameter `p` as skin `key`'s row shows it (a color's
/// name stays the skin's, or the palette's).
pub fn choice(key: &str, p: &Parameter, c: &Choice) -> UiText {
    matches!(p.kind, Kind::Choice(_))
        .then(|| message(key, p, &format!("-choice-{}", part(&c.id))))
        .flatten()
        .unwrap_or_else(|| UiText::data(&c.label))
}

#[cfg(test)]
mod tests {
    use std::path::{Path, PathBuf};

    use ddi_noteskin::params::{Declared, catalog};
    use ddi_noteskin::{Folder, bundled};

    use super::*;

    /// `assets/noteskins/`.
    fn root() -> PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../../assets/noteskins")
    }

    /// Every bundled skin's folder (its metrics alone, which is all its
    /// declarations need), and whether it is another's color scheme
    /// (`variant_of` in its `PROVENANCE.toml`: chosen through that one,
    /// its rows that one's).
    fn bundled_skins() -> Vec<(Folder, bool)> {
        let mut out = Vec::new();
        for game in std::fs::read_dir(root()).unwrap() {
            let game = game.unwrap().path();
            if !game.is_dir() {
                continue;
            }
            for skin in std::fs::read_dir(&game).unwrap() {
                let skin = skin.unwrap().path();
                let metrics = std::fs::read_to_string(skin.join("metrics.ini")).unwrap_or_default();
                let provenance =
                    std::fs::read_to_string(skin.join("PROVENANCE.toml")).unwrap_or_default();
                let variant = provenance
                    .lines()
                    .any(|l| l.trim_start().starts_with("variant_of"));
                let name = |p: &Path| p.file_name().unwrap().to_string_lossy().into_owned();
                out.push((
                    Folder {
                        game: name(&game),
                        name: name(&skin),
                        files: vec!["metrics.ini".into()],
                        texts: vec![("metrics.ini".into(), metrics)],
                    },
                    variant,
                ));
            }
        }
        out.sort_by(|a, b| (&a.0.game, &a.0.name).cmp(&(&b.0.game, &b.0.name)));
        out
    }

    /// The bundled skins one picks and the parameters each declares, as
    /// Options lists them (Infinito's from its own metrics, built in).
    fn declared() -> Vec<(String, Vec<Parameter>)> {
        let skins = bundled_skins();
        let folders: Vec<Folder> = skins.iter().map(|(f, _)| f.clone()).collect();
        let keys: Vec<String> = skins
            .iter()
            .filter(|(_, variant)| !variant)
            .map(|(f, _)| bundled::key(&f.game, &f.name, true))
            .collect();
        let mut out: Vec<(String, Vec<Parameter>)> = catalog(&folders, 0, &keys)
            .into_iter()
            .filter(|(k, _)| k != bundled::INFINITO)
            .collect();
        out.push((
            bundled::INFINITO.to_string(),
            bundled::infinito_parameters().parameters.clone(),
        ));
        out
    }

    /// Every message the bundled skins' parameters are shown by, with the
    /// skin's own text for it: `(id, text)`.
    fn texts() -> Vec<(String, String)> {
        let mut out = Vec::new();
        for (key, parameters) in declared() {
            for p in parameters {
                let id = message_id(&key, &p.name).unwrap();
                out.push((id.clone(), p.label.clone()));
                if !p.description.is_empty() {
                    out.push((format!("{id}-description"), p.description.clone()));
                }
                if let Kind::Choice(choices) = &p.kind {
                    for c in choices {
                        out.push((format!("{id}-choice-{}", part(&c.id)), c.label.clone()));
                    }
                }
            }
        }
        out
    }

    #[test]
    fn ids_are_made_from_the_key_and_the_name() {
        assert_eq!(
            message_id(bundled::INFINITO, "NoteTimedCharge").as_deref(),
            Some("skins-dance-infinito-note-timed-charge")
        );
        assert_eq!(
            message_id(bundled::PETERS_DDR, "Colors").as_deref(),
            Some("skins-dance-peters-ddr-colors")
        );
        assert_eq!(part("CF_ALLOY skin"), "cf_alloy-skin");
        assert_eq!(part("Size2X"), "size2-x");
        // A skin the game does not ship has none.
        assert_eq!(message_id("dance/infinito", "Colors"), None);
    }

    #[test]
    fn the_bundled_skins_declare_what_is_looked_for() {
        let declared = declared();
        let keys: Vec<&str> = declared.iter().map(|(k, _)| k.as_str()).collect();
        assert!(keys.contains(&bundled::INFINITO), "{keys:?}");
        assert!(keys.contains(&bundled::PETERS_DDR), "{keys:?}");
        let infinito = &declared
            .iter()
            .find(|(k, _)| k == bundled::INFINITO)
            .unwrap()
            .1;
        assert_eq!(infinito.len(), 7);
        // No two of a skin's messages share an id.
        let texts = texts();
        let mut ids: Vec<&str> = texts.iter().map(|(id, _)| id.as_str()).collect();
        ids.sort_unstable();
        let count = ids.len();
        ids.dedup();
        assert_eq!(ids.len(), count, "two texts share an id");
    }

    /// Every bundled skin's every parameter has its messages: a skin that
    /// declares a new one, or a new choice, fails here until they are
    /// written (`locales/en/skins.ftl` and every other language's). At
    /// Advanced, in English, each says exactly what the skin's file says.
    #[test]
    fn every_bundled_parameter_has_its_messages_saying_the_skin_s_words_at_advanced() {
        ddi_l10n::set_language("en");
        let missing: Vec<String> = texts()
            .into_iter()
            .filter(|(id, _)| ddi_l10n::message(id).is_none())
            .map(|(id, _)| id)
            .collect();
        assert!(missing.is_empty(), "no message for {missing:?}");
        ddi_l10n::at_level(true, || {
            for (id, text) in texts() {
                assert_eq!(ddi_l10n::message(&id).unwrap().as_str(), text, "{id}");
            }
        });
    }

    /// The rows read through here: the same words at Advanced in English
    /// as the file's, a Basic wording only for a parameter shown at Basic,
    /// and another language's own words.
    #[test]
    fn rows_read_the_messages_and_other_skins_their_own_words() {
        ddi_l10n::set_language("en");
        for (key, parameters) in declared() {
            for p in &parameters {
                ddi_l10n::at_level(true, || {
                    assert_eq!(label(&key, p).as_str(), p.label);
                    assert_eq!(description(&key, p).as_str(), p.description);
                    if let Kind::Choice(choices) = &p.kind {
                        for c in choices {
                            assert_eq!(choice(&key, p, c).as_str(), c.label);
                        }
                    }
                });
                if !p.basic {
                    let at = |advanced| {
                        ddi_l10n::at_level(advanced, || (label(&key, p), description(&key, p)))
                    };
                    assert_eq!(at(true), at(false), "{key} {}: no Basic wording", p.name);
                }
            }
        }
        let colors = bundled::infinito_parameters()
            .get(bundled::infinito::COLORS)
            .unwrap();
        assert!(colors.basic);
        let plain = ddi_l10n::at_level(false, || description(bundled::INFINITO, colors));
        assert_ne!(plain.as_str(), colors.description, "a Basic wording");
        assert!(!plain.contains("timing"), "{plain}");
        // A skin the game does not ship: its own words, at any level and
        // in any language.
        let mine = Declared::from_ini(&ddi_noteskin::ini::Ini::parse(
            "[Parameters]\nList=Colors\n[Parameter Colors]\nType=Choice\n\
             Label=Mine\nDescription=Mine too.\nChoices=Vivid\nChoiceLabels=Mine!\n",
        ));
        let p = &mine.parameters[0];
        let Kind::Choice(cs) = &p.kind else { panic!() };
        ddi_l10n::set_language("ja");
        for key in ["dance/infinito", "dance/mine"] {
            assert_eq!(label(key, p).as_str(), "Mine");
            assert_eq!(description(key, p).as_str(), "Mine too.");
            assert_eq!(choice(key, p, &cs[0]).as_str(), "Mine!");
        }
        // Japanese for the bundled one.
        assert_ne!(label(bundled::INFINITO, colors).as_str(), colors.label);
        assert_ne!(
            description(bundled::INFINITO, colors).as_str(),
            colors.description
        );
        ddi_l10n::set_language("en");
    }
}
