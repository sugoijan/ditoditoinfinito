//! An announcer as a pool of takes (`docs/plans/announcers.md`, "Pools of
//! takes"): a take is one sound file with the events (the lines) it may
//! be said for, its language and its tags. A pack's folders build the
//! pool ([`Pool::plain`]: a line folder is an event, a language folder
//! above it a language); the player's choices filter it
//! ([`Pool::resolve`]: a language, or several mixed; tags switched off),
//! and what is left is the same shape as before, each line's sounds
//! ([`Kept::lines`]), which the session speaks as it always did (its
//! fallbacks, shuffles, precedence and timing).
//!
//! A pack says what it offers in `announcer.ini`, in the format skins'
//! `metrics.ini` uses (`docs/plans/skin-parameters.md`), with a type of
//! its own, `Type=Language`, and its names in each language:
//!
//! ```ini
//! [Announcer]
//! Name=Mr. Clank
//! Name.ja=ガキンさん
//!
//! [Parameters]
//! List=Language
//!
//! [Parameter Language]
//! Type=Language
//! Languages=en,ja
//! Mixed=1
//! ```
//!
//! A pack without it reads as before: every sound of its line folders.

use std::collections::{BTreeMap, BTreeSet};

use ddi_noteskin::ini::Ini;
use ddi_noteskin::params::{Declared, Parameter, Value, Values};

use super::{Kept, Line};
use crate::path::RelPathBuf;

/// The file a pack declares itself in.
pub const DECLARATION: &str = "announcer.ini";
/// Characters of `announcer.ini` read, at most (the rest is left out).
pub const MAX_DECLARATION: usize = 64 << 10;
/// The name the Language parameter's value is kept under, whatever the
/// pack calls it.
pub const LANGUAGE: &str = "Language";
/// The Language parameter's value for every language the pack mixes.
pub const MIXED: &str = "mixed";
/// The languages a pack may offer, at most.
pub const MAX_LANGUAGES: usize = 16;
/// The language a game language no voice speaks falls back to first.
pub const FALLBACK_LANGUAGE: &str = "en";

/// One sound and what it may be said for.
#[derive(Clone, Debug, PartialEq)]
pub struct Take {
    /// Relative to the announcer's folder.
    pub file: RelPathBuf,
    /// The lines it may be said for (one or several), in order.
    pub events: Vec<Line>,
    /// Its language's tag (lower case), if it has one: a take without
    /// one is said in every language.
    pub lang: Option<String>,
    /// The pack's own words for it (a switchable tag leaves it out when
    /// switched off).
    pub tags: Vec<String>,
    /// Its share against the line's other takes when languages are
    /// mixed (1: as any other; 0: never).
    pub weight: f64,
}

/// A tag a pack lets the player switch off (`Profanity: On / Off`).
#[derive(Clone, Debug, PartialEq)]
pub struct Tag {
    pub name: String,
    /// What Options calls it (the pack's words: data).
    pub label: String,
    /// On (its takes said) unless the player switched it off.
    pub default: bool,
}

/// An announcer's takes and what it offers.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Pool {
    pub takes: Vec<Take>,
    /// The languages it offers, in the order offered.
    pub languages: Vec<String>,
    /// It offers its languages mixed.
    pub mixed: bool,
    /// The tags it lets the player switch off.
    pub tags: Vec<Tag>,
}

/// What `announcer.ini` declares: the pack's names, its Language
/// parameter and the rest.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Declaration {
    /// Its name by language tag (`""`: in any other).
    pub names: BTreeMap<String, String>,
    /// The languages its Language parameter offers, in order (none: the
    /// voices' folders, sorted).
    pub languages: Vec<String>,
    /// Each language's name where the pack gives one (`Labels=`).
    pub language_labels: BTreeMap<String, String>,
    /// Its Language parameter offers Mixed.
    pub mixed: bool,
    /// Its other parameters, as a skin's are declared.
    pub parameters: Vec<Parameter>,
    pub warnings: Vec<String>,
}

/// A language tag's primary subtag, in lower case (`ja-JP` → `ja`).
fn primary(tag: &str) -> String {
    tag.split(['-', '_'])
        .next()
        .unwrap_or_default()
        .to_ascii_lowercase()
}

/// Whether a folder's name reads as a language's tag (`en`, `ja`,
/// `pt-BR`, `zh-Hant`): a language folder of a declared pack.
pub fn is_language(name: &str) -> bool {
    let mut parts = name.split('-');
    let first = parts.next().unwrap_or_default();
    (2..=3).contains(&first.len())
        && first.bytes().all(|b| b.is_ascii_alphabetic())
        && parts.all(|p| (2..=8).contains(&p.len()) && p.bytes().all(|b| b.is_ascii_alphanumeric()))
}

/// A language's tag as kept: as written, the primary subtag in lower case.
pub fn language_tag(name: &str) -> String {
    match name.split_once('-') {
        Some((p, rest)) => format!("{}-{rest}", p.to_ascii_lowercase()),
        None => name.to_ascii_lowercase(),
    }
}

/// Of `offered` (tags), the one that speaks for the game's language
/// `game`: the same tag, else the same primary subtag (`ja-JP` → `ja`),
/// else English ([`FALLBACK_LANGUAGE`]), else the first.
pub fn pick_language<'a>(offered: &'a [String], game: &str) -> Option<&'a String> {
    offered
        .iter()
        .find(|t| t.eq_ignore_ascii_case(game))
        .or_else(|| offered.iter().find(|t| primary(t) == primary(game)))
        .or_else(|| offered.iter().find(|t| primary(t) == FALLBACK_LANGUAGE))
        .or_else(|| offered.first())
}

impl Declaration {
    /// `announcer.ini`'s text, read as skins' metrics are.
    pub fn parse(text: &str) -> Declaration {
        let text: String = text.chars().take(MAX_DECLARATION).collect();
        let ini = Ini::parse(&text);
        let mut d = Declaration::default();
        // The names: `Name`, then `Name.<tag>` for each language.
        for (key, value) in ini.section("Announcer") {
            let value = value.trim();
            if value.is_empty() {
                continue;
            }
            let name: String = value.chars().take(ddi_noteskin::params::MAX_LABEL).collect();
            if key.eq_ignore_ascii_case("Name") {
                d.names.insert(String::new(), name);
            } else if let Some(tag) = key
                .split_once('.')
                .filter(|(k, _)| k.eq_ignore_ascii_case("Name"))
                .map(|(_, t)| t.trim())
                .filter(|t| is_language(t))
            {
                d.names.insert(language_tag(tag), name);
            }
        }
        // The Language parameter, ours; the rest as a skin declares them.
        let listed: Vec<String> = ini
            .get("Parameters", "List")
            .map(|l| l.split(',').map(|n| n.trim().to_string()).collect())
            .unwrap_or_default();
        let mut language = None;
        for name in &listed {
            let section = format!("Parameter {name}");
            let ty = ini.get(&section, "Type").map(str::trim);
            if ty.is_some_and(|t| t.eq_ignore_ascii_case("Language")) {
                if language.is_some() {
                    d.warnings
                        .push(format!("parameter {name}: a second Language, left out"));
                    continue;
                }
                language = Some(section);
            }
        }
        if let Some(section) = &language {
            let list = |key: &str| -> Vec<String> {
                ini.get(section, key)
                    .map(|l| l.split(',').map(|p| p.trim().to_string()).collect())
                    .unwrap_or_default()
            };
            let labels = list("Labels");
            for (i, tag) in list("Languages").into_iter().enumerate() {
                if !is_language(&tag) {
                    d.warnings.push(format!("\"{tag}\" is not a language's tag"));
                    continue;
                }
                let tag = language_tag(&tag);
                if d.languages.contains(&tag) || d.languages.len() == MAX_LANGUAGES {
                    continue;
                }
                if let Some(l) = labels.get(i).filter(|l| !l.is_empty()) {
                    d.language_labels.insert(
                        tag.clone(),
                        l.chars().take(ddi_noteskin::params::MAX_LABEL).collect(),
                    );
                }
                d.languages.push(tag);
            }
            d.mixed = ini.get(section, "Mixed").is_some_and(|m| {
                let m = m.trim();
                m == "1" || m.eq_ignore_ascii_case("true")
            });
        }
        let declared = Declared::from_ini(&ini);
        d.parameters = declared.parameters;
        d.warnings.extend(
            declared
                .warnings
                .into_iter()
                // Ours, which a skin's parser does not know.
                .filter(|w| !w.contains("not \"Language\"") && !w.contains("not \"language\"")),
        );
        d
    }

    /// Its name in UI language `ui`: that language's, else its primary
    /// subtag's, else its plain name.
    pub fn name(&self, ui: &str) -> Option<&str> {
        name_in(&self.names, ui)
    }
}

/// Of `names` (by language tag, `""` for any other), the one for UI
/// language `ui`: its own, else its primary subtag's, else the plain one.
fn name_in<'a>(names: &'a BTreeMap<String, String>, ui: &str) -> Option<&'a str> {
    names
        .get(&language_tag(ui))
        .or_else(|| names.get(&primary(ui)))
        .or_else(|| names.get(""))
        .map(String::as_str)
}

/// The Language row's choice, as kept in the announcer's values.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum LanguageChoice {
    /// Same as game language: the default, kept as nothing.
    Game,
    /// One language, by its tag.
    One(String),
    /// Every language the pack offers, mixed.
    Mixed,
}

impl LanguageChoice {
    /// What `values` keep.
    pub fn of(values: &Values) -> LanguageChoice {
        match values.get(LANGUAGE) {
            Some(Value::Text(t)) if t.eq_ignore_ascii_case(MIXED) => LanguageChoice::Mixed,
            Some(Value::Text(t)) if is_language(t) => LanguageChoice::One(language_tag(t)),
            _ => LanguageChoice::Game,
        }
    }

    /// Its value to keep (`None`: the default, kept as nothing).
    pub fn value(&self) -> Option<Value> {
        match self {
            LanguageChoice::Game => None,
            LanguageChoice::One(t) => Some(Value::Text(t.clone())),
            LanguageChoice::Mixed => Some(Value::Text(MIXED.into())),
        }
    }
}

/// The name a switchable tag's value is kept under among the
/// announcer's values (apart from its parameters').
pub fn tag_value_name(tag: &str) -> String {
    format!("tag.{tag}")
}

/// What a pool comes to with the player's choices: each line's sounds,
/// and the languages spoken.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Chosen {
    pub lines: BTreeMap<String, Vec<RelPathBuf>>,
    /// The languages its takes were taken from (none: a pack without
    /// languages).
    pub languages: Vec<String>,
    /// What the choice is called among an announcer's shuffles: the
    /// languages and the tags left out (`""`: all of it, a pack without
    /// choices).
    pub key: String,
}

impl Pool {
    /// The pool a pack's folders make ([`Kept::lines`] and
    /// [`Kept::voices`]): each sound a take of its folder's line, in its
    /// language folder's language; the languages offered those `declared`
    /// lists that have a folder, then the other folders', sorted.
    pub fn plain(kept: &Kept, declared: &Declaration) -> Pool {
        let mut takes = Vec::new();
        let mut add = |lines: &BTreeMap<String, Vec<RelPathBuf>>, lang: Option<&String>| {
            for (folder, files) in lines {
                let Some(line) = Line::parse(folder) else {
                    continue;
                };
                for f in files {
                    takes.push(Take {
                        file: f.clone(),
                        events: vec![line],
                        lang: lang.cloned(),
                        tags: Vec::new(),
                        weight: 1.0,
                    });
                }
            }
        };
        add(&kept.lines, None);
        for (lang, lines) in &kept.voices {
            add(lines, Some(lang));
        }
        let mut languages: Vec<String> = declared
            .languages
            .iter()
            .filter(|l| kept.voices.contains_key(*l))
            .cloned()
            .collect();
        for lang in kept.voices.keys() {
            if !languages.contains(lang) && languages.len() < MAX_LANGUAGES {
                languages.push(lang.clone());
            }
        }
        Pool {
            takes,
            mixed: declared.mixed && languages.len() > 1,
            languages,
            tags: Vec::new(),
        }
    }

    /// The languages `choice` speaks for the game's language `game`.
    pub fn languages_for(&self, choice: &LanguageChoice, game: &str) -> Vec<String> {
        match choice {
            LanguageChoice::Mixed if self.mixed => self.languages.clone(),
            LanguageChoice::One(t) if self.languages.contains(t) => vec![t.clone()],
            _ => pick_language(&self.languages, game)
                .cloned()
                .into_iter()
                .collect(),
        }
    }

    /// The tags switched off by `values` (or off by default).
    pub fn tags_off(&self, values: &Values) -> BTreeSet<String> {
        self.tags
            .iter()
            .filter(|t| match values.get(&tag_value_name(&t.name)) {
                Some(Value::Bool(on)) => !on,
                _ => !t.default,
            })
            .map(|t| t.name.clone())
            .collect()
    }

    /// The takes `values` keep for the game's language `game`, as each
    /// line's sounds: those of the languages chosen (and those of no
    /// language), none tagged with a tag switched off. A take of several
    /// lines is a sound of each; a weight above 1 when languages are
    /// mixed makes a take that many sounds of its line (at most four),
    /// one of 0 none.
    pub fn resolve(&self, values: &Values, game: &str) -> Chosen {
        let choice = LanguageChoice::of(values);
        let languages = self.languages_for(&choice, game);
        let off = self.tags_off(values);
        let mixed = languages.len() > 1;
        let mut lines: BTreeMap<String, Vec<RelPathBuf>> = BTreeMap::new();
        for take in &self.takes {
            if take
                .lang
                .as_ref()
                .is_some_and(|l| !languages.contains(l))
            {
                continue;
            }
            if take.tags.iter().any(|t| off.contains(t)) {
                continue;
            }
            let copies = if mixed {
                take.weight.round().clamp(0.0, 4.0) as usize
            } else {
                usize::from(take.weight > 0.0)
            };
            for &event in &take.events {
                let files = lines.entry(event.folder()).or_default();
                for _ in 0..copies {
                    files.push(take.file.clone());
                }
            }
        }
        lines.retain(|_, f| !f.is_empty());
        let mut key = languages.join("+");
        for t in &off {
            key.push_str(&format!(" -{t}"));
        }
        Chosen {
            lines,
            languages,
            key,
        }
    }
}

impl Kept {
    /// What its `announcer.ini` declares (nothing without one).
    pub fn declaration(&self) -> Declaration {
        self.ini.as_deref().map(Declaration::parse).unwrap_or_default()
    }

    /// Its name in UI language `ui`: its declaration's, else its own.
    pub fn display_name(&self, ui: &str) -> String {
        self.declaration()
            .name(ui)
            .map_or_else(|| self.name.clone(), str::to_string)
    }

    /// Its pool of takes ([`Pool::plain`]).
    pub fn pool(&self) -> Pool {
        Pool::plain(self, &self.declaration())
    }

    /// What it offers the player ([`Offered`]), with `values` chosen, the
    /// game's language `game`.
    pub fn offered(&self, values: &Values, game: &str) -> Offered {
        let declared = self.declaration();
        let pool = Pool::plain(self, &declared);
        Offered::of(&pool, &declared, values, game)
    }

    /// Itself as `values` choose for the game's language `game`: its
    /// lines those the choices keep ([`Pool::resolve`]), its shuffles
    /// apart from another choice's ([`Kept::choice`]).
    pub fn resolve(&self, values: &Values, game: &str) -> Kept {
        if self.voices.is_empty() && self.ini.is_none() {
            // A pack of line folders alone: as it always was.
            return self.clone();
        }
        let chosen = self.pool().resolve(values, game);
        Kept {
            name: self.name.clone(),
            files: chosen.lines.values().map(Vec::len).sum(),
            lines: chosen.lines,
            bytes: self.bytes,
            bundled: self.bundled,
            voices: BTreeMap::new(),
            ini: None,
            choice: chosen.key,
        }
    }
}

/// What an announcer lets the player choose, and what the choices come
/// to, for Options' rows.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Offered {
    /// Its names by language tag (`""`: in any other; its declaration's).
    pub names: BTreeMap<String, String>,
    /// The languages it offers (tags), in order, each with the pack's own
    /// name for it where it gives one.
    pub languages: Vec<(String, Option<String>)>,
    /// It offers them mixed.
    pub mixed: bool,
    /// What Same as game language speaks for the game's language now.
    pub game: Option<String>,
    /// The tags it lets the player switch off.
    pub tags: Vec<Tag>,
    /// Its other parameters.
    pub parameters: Vec<Parameter>,
}

impl Offered {
    fn of(pool: &Pool, declared: &Declaration, _values: &Values, game: &str) -> Offered {
        Offered {
            names: declared.names.clone(),
            languages: pool
                .languages
                .iter()
                .map(|t| (t.clone(), declared.language_labels.get(t).cloned()))
                .collect(),
            mixed: pool.mixed,
            game: pick_language(&pool.languages, game).cloned(),
            tags: pool.tags.clone(),
            parameters: declared.parameters.clone(),
        }
    }

    /// Its name in UI language `ui`, where it declares one
    /// ([`Declaration::name`]).
    pub fn name(&self, ui: &str) -> Option<&str> {
        name_in(&self.names, ui)
    }

    /// Whether it offers a choice of language (two or more).
    pub fn has_languages(&self) -> bool {
        self.languages.len() > 1
    }
}

/// The key an announcer's voice is known by with the player's choices:
/// a voice of another key is loaded anew (song select's, kept for the
/// session).
pub fn voice_key(key: &str, values: &Values, game: &str) -> String {
    let values: Vec<String> = values.iter().map(|(k, v)| format!("{k}={v:?}")).collect();
    format!("{key}|{}|{game}", values.join(","))
}
