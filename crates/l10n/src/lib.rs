//! The UI's languages (`docs/plans/localization.md`).
//!
//! Messages are Fluent files under `locales/<tag>/<area>.ftl`, English
//! the source. Each English message is a function of [`msg`], its
//! variables its arguments (`msg::start::singleplayer()`), returning the
//! text in the current language as a [`UiText`]; what is not ours to
//! translate (a song's title, a path, what the player typed) is
//! [`UiText::data`]. A message missing in a language falls back to its
//! fallback's, then to English.
//!
//! The language is the thread's (the UI's): [`choose`] picks it from the
//! settings' choice or the system's languages ([`set_system_languages`]),
//! and screens built after it speak it. Two pseudo-languages show what
//! passes through here: `qps-ploc` writes each message accented, longer
//! and in « », data in ‹ ›; `qps-wide` in full-width letters.

use std::borrow::Cow;
use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::rc::Rc;
use std::sync::Arc;

use fluent_bundle::types::FluentNumber;
use fluent_bundle::{FluentArgs, FluentResource, FluentValue};

mod number;
mod pseudo;
mod wide;

pub use wide::wide;

include!(concat!(env!("OUT_DIR"), "/messages.rs"));

type Bundle = fluent_bundle::FluentBundle<FluentResource>;

/// A language the UI speaks.
#[derive(Debug, PartialEq, Eq)]
pub struct Language {
    /// Its BCP 47 tag (`en`, `ja`, `en-GB`).
    pub tag: &'static str,
    /// Its name, written in itself ("日本語").
    pub name: &'static str,
    pub status: Status,
    /// The language its missing messages come from before English.
    pub fallback: Option<&'static str>,
    decimal: &'static str,
    group: &'static str,
    min_grouping: u32,
    /// The font families its script needs beyond the theme's text face
    /// (Japanese: Noto Sans CJK JP), each in [`FACES`].
    pub faces: &'static [&'static str],
}

impl Language {
    /// Every weight of each of its [`Language::faces`].
    pub fn face_files(&self) -> impl Iterator<Item = &'static Face> + '_ {
        FACES.iter().filter(|f| self.faces.contains(&f.family))
    }
}

/// One weight of a face a language needs, cut to what its languages'
/// messages use (`cargo run -p xtask -- l10n fonts`).
#[derive(Debug, PartialEq, Eq)]
pub struct Face {
    /// Its family, as font stacks name it.
    pub family: &'static str,
    /// The CSS weight it is registered at, under its family: the theme's
    /// 400, 600 and 700 (others take the nearest, as CSS matches them).
    pub weight: u16,
    /// Its file in `assets/fonts/` (served as `fonts/<file>` on the web).
    pub file: &'static str,
    /// The file (feature `embed-faces`: desktop's, whose text is drawn
    /// with bundled faces; the web fetches them).
    #[cfg(feature = "embed-faces")]
    pub bytes: &'static [u8],
}

/// `stack`, a CSS font stack whose first family is the theme's text face,
/// with the current language's faces after that first family: Latin is
/// drawn by the theme's face, the language's script by its faces (their
/// forms of unified Han characters, Japanese ones in Japanese), the rest
/// by what the stack named after.
pub fn font_stack(stack: &str) -> String {
    let faces = current().faces;
    if faces.is_empty() {
        return stack.to_string();
    }
    let (first, rest) = match stack.split_once(',') {
        Some((first, rest)) => (first.trim(), Some(rest.trim())),
        None => (stack.trim(), None),
    };
    let mut out = first.to_string();
    for f in faces {
        out.push_str(", ");
        out.push_str(f);
    }
    if let Some(rest) = rest.filter(|r| !r.is_empty()) {
        out.push_str(", ");
        out.push_str(rest);
    }
    out
}

/// Whether the cut of `family` has `c`, for a language face and a wide
/// character (`None` otherwise): its base and what its languages write
/// (`xtask l10n fonts` cuts it from the same, `l10n check` keeps it so).
pub fn face_has(family: &str, c: char) -> Option<bool> {
    if !wide(c) {
        return None;
    }
    let (_, ranges) = FACE_CHARS.iter().find(|(f, _)| *f == family)?;
    let c = u32::from(c);
    Some(
        ranges
            .binary_search_by(|&(a, b)| {
                if b < c {
                    std::cmp::Ordering::Less
                } else if a > c {
                    std::cmp::Ordering::Greater
                } else {
                    std::cmp::Ordering::Equal
                }
            })
            .is_ok(),
    )
}

/// The stack `text` is set in, of `stack` (a style's, `font_stack`'s): a
/// language's face drawing a run only where its cut has every wide
/// character the run writes. A song's title with a kanji no message uses
/// is then drawn whole by the system's faces, never half in each; a
/// message always by its language's (`l10n check` sees to it).
pub fn stack_for<'a>(stack: &'a str, text: &str) -> Cow<'a, str> {
    if !text.chars().any(wide) || !FACE_CHARS.iter().any(|(f, _)| stack.contains(f)) {
        return Cow::Borrowed(stack);
    }
    let lacking = |family: &str| text.chars().any(|c| face_has(family, c) == Some(false));
    let families: Vec<&str> = stack.split(',').map(str::trim).collect();
    if !families.iter().any(|f| lacking(f)) {
        return Cow::Borrowed(stack);
    }
    Cow::Owned(
        families
            .into_iter()
            .filter(|f| !lacking(f))
            .collect::<Vec<_>>()
            .join(", "),
    )
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Status {
    /// Offered to everyone, and to the system's choice.
    Shipped,
    /// Offered to everyone in the picker as a draft to try, never chosen
    /// for the system.
    Preview,
    /// A pseudo-language, for tests and for looking: offered with the
    /// debug overlay on.
    Pseudo,
}

/// `qps-ploc` and `qps-wide`.
pub static PSEUDO: &[Language] = &[
    Language {
        tag: "qps-ploc",
        name: "Ƥšéúðó-áççéñţš",
        status: Status::Pseudo,
        fallback: None,
        decimal: ".",
        group: ",",
        min_grouping: 1,
        faces: &[],
    },
    Language {
        tag: "qps-wide",
        name: "Ｐｓｅｕｄｏ－ｗｉｄｅ",
        status: Status::Pseudo,
        fallback: None,
        decimal: ".",
        group: ",",
        min_grouping: 1,
        // Full-width letters and 「」, drawn as Japanese text is.
        faces: &["Noto Sans CJK JP"],
    },
];

/// What a variable of a message is (its comment's declaration).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ArgKind {
    Number,
    String,
}

/// A message's argument, as the generated functions pass it.
#[doc(hidden)]
pub enum Arg<'a> {
    Number(f64),
    Str(&'a str),
}

/// A number a message takes (a count, a size).
pub trait Number {
    fn value(self) -> f64;
}

macro_rules! numbers {
    ($($t:ty),*) => {$(
        impl Number for $t {
            fn value(self) -> f64 {
                self as f64
            }
        }
    )*};
}
numbers!(u8, u16, u32, u64, usize, i8, i16, i32, i64, isize, f32, f64);

/// Text for the screen: a message in the current language, or data that
/// is not ours to translate.
#[derive(Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct UiText(Arc<str>);

impl UiText {
    /// Text that is not ours to translate: a song's title, a pack's name,
    /// a path, what the player typed. The pseudo-languages mark it ‹ ›.
    pub fn data(text: impl AsRef<str>) -> UiText {
        let text = text.as_ref();
        if current().status == Status::Pseudo {
            UiText(format!("‹{text}›").into())
        } else {
            UiText(text.into())
        }
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl std::ops::Deref for UiText {
    type Target = str;
    fn deref(&self) -> &str {
        &self.0
    }
}

impl AsRef<str> for UiText {
    fn as_ref(&self) -> &str {
        &self.0
    }
}

impl std::fmt::Display for UiText {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

impl From<UiText> for String {
    fn from(t: UiText) -> String {
        t.0.to_string()
    }
}

impl From<UiText> for Arc<str> {
    fn from(t: UiText) -> Arc<str> {
        t.0
    }
}

struct State {
    current: &'static Language,
    /// The system's preferred languages, best first (BCP 47 tags).
    system: Vec<String>,
    /// The run's language whatever the settings say ([`force`]).
    forced: Option<&'static Language>,
    bundles: HashMap<&'static str, Rc<Bundle>>,
}

thread_local! {
    static STATE: RefCell<State> = RefCell::new(State {
        current: &LANGUAGES[0],
        system: Vec::new(),
        forced: None,
        bundles: HashMap::new(),
    });
    /// The language a number is being formatted for ([`number::format`]).
    static FORMATTING: Cell<&'static Language> = Cell::new(&LANGUAGES[0]);
}

/// Every language there is: [`LANGUAGES`] and then [`PSEUDO`].
pub fn all() -> impl Iterator<Item = &'static Language> {
    LANGUAGES.iter().chain(PSEUDO)
}

/// The language tagged `tag` (exactly, without regard to case).
pub fn find(tag: &str) -> Option<&'static Language> {
    all().find(|l| l.tag.eq_ignore_ascii_case(tag))
}

/// The languages to offer: the shipped and preview ones, with `pseudo`
/// the pseudo-languages too.
pub fn offered(pseudo: bool) -> Vec<&'static Language> {
    all()
        .filter(|l| pseudo || l.status != Status::Pseudo)
        .collect()
}

/// The language the UI speaks now.
pub fn current() -> &'static Language {
    STATE.with_borrow(|s| s.current)
}

/// The system's preferred languages, best first, as the platform gives
/// them (`ja-JP`, `en_US.UTF-8`, `fil`).
pub fn set_system_languages<S: AsRef<str>>(tags: impl IntoIterator<Item = S>) {
    let system = tags.into_iter().map(|t| t.as_ref().to_string()).collect();
    STATE.with_borrow_mut(|s| s.system = system);
}

/// The shipped language the system's preferences come to, else English.
pub fn system_language() -> &'static Language {
    let system = STATE.with_borrow(|s| s.system.clone());
    let shipped: Vec<&'static Language> = LANGUAGES
        .iter()
        .filter(|l| l.status == Status::Shipped)
        .collect();
    negotiate(&system, &shipped).unwrap_or(&LANGUAGES[0])
}

/// Speaks `setting` (a tag, from the settings), or the system's language
/// without one or for a tag there is no language for; the run's when
/// one is [forced](force).
pub fn choose(setting: Option<&str>) -> &'static Language {
    let language = STATE
        .with_borrow(|s| s.forced)
        .unwrap_or_else(|| setting.and_then(find).unwrap_or_else(system_language));
    STATE.with_borrow_mut(|s| s.current = language);
    language
}

/// Speaks the language tagged `tag` for this run whatever the settings
/// say (desktop's `--lang`, the web's `?lang=`), until [`unforce`];
/// `false` when there is no such language.
pub fn force(tag: &str) -> bool {
    let Some(language) = find(tag) else {
        return false;
    };
    STATE.with_borrow_mut(|s| {
        s.forced = Some(language);
        s.current = language;
    });
    true
}

/// The settings choose again (the player chose in Options).
pub fn unforce() {
    STATE.with_borrow_mut(|s| s.forced = None);
}

/// Speaks the language tagged `tag`, when there is one (a test, `--lang`).
pub fn set_language(tag: &str) -> bool {
    match find(tag) {
        Some(language) => {
            STATE.with_borrow_mut(|s| s.current = language);
            true
        }
        None => false,
    }
}

/// The first of `wanted` that one of `languages` speaks: the same tag,
/// else the same language with no region (`ja-JP` → `ja`), else any of
/// that language (`en-AU` → `en-GB` when there is no `en`).
fn negotiate(wanted: &[String], languages: &[&'static Language]) -> Option<&'static Language> {
    for tag in wanted {
        let tag = normalize(tag);
        let base = tag.split('-').next().unwrap_or_default();
        let found = languages
            .iter()
            .find(|l| l.tag.eq_ignore_ascii_case(&tag))
            .or_else(|| languages.iter().find(|l| l.tag.eq_ignore_ascii_case(base)))
            .or_else(|| {
                languages.iter().find(|l| {
                    l.tag
                        .split('-')
                        .next()
                        .is_some_and(|b| b.eq_ignore_ascii_case(base))
                })
            });
        if let Some(l) = found {
            return Some(l);
        }
    }
    None
}

/// A system's tag as BCP 47 (`en_US.UTF-8` → `en-US`), with the old
/// names some systems still give (`tl` → `fil`, `iw` → `he`).
fn normalize(tag: &str) -> String {
    let tag = tag
        .split(['.', '@'])
        .next()
        .unwrap_or_default()
        .replace('_', "-");
    let (base, rest) = tag
        .split_once('-')
        .map_or((tag.as_str(), ""), |(b, r)| (b, r));
    let base = match base.to_ascii_lowercase().as_str() {
        "tl" => "fil".to_string(),
        "iw" => "he".to_string(),
        "in" => "id".to_string(),
        "no" => "nb".to_string(),
        b => b.to_string(),
    };
    if rest.is_empty() {
        base
    } else {
        format!("{base}-{rest}")
    }
}

/// The bundle of the language tagged `tag` (a pseudo-language's: English
/// with its transform), made once.
fn bundle(language: &'static Language) -> Rc<Bundle> {
    if let Some(b) = STATE.with_borrow(|s| s.bundles.get(language.tag).cloned()) {
        return b;
    }
    let source = if language.status == Status::Pseudo {
        "en"
    } else {
        language.tag
    };
    let id = if language.status == Status::Pseudo {
        "en".parse()
    } else {
        language.tag.parse()
    }
    .unwrap_or_default();
    let mut b = Bundle::new(vec![id]);
    // Unicode's isolating marks drawn as boxes, counted as width.
    b.set_use_isolating(false);
    b.set_formatter(Some(number::format));
    let _ = b.add_builtins();
    match language.tag {
        "qps-ploc" => b.set_transform(Some(pseudo::accents)),
        "qps-wide" => b.set_transform(Some(pseudo::wide)),
        _ => {}
    }
    if let Some((_, files)) = SOURCES.iter().find(|(t, _)| *t == source) {
        for (_, text) in *files {
            let resource =
                FluentResource::try_new((*text).to_string()).unwrap_or_else(|(partial, _)| partial);
            // A message in two files: the first wins (`xtask l10n check`
            // says so).
            let _ = b.add_resource(resource);
        }
    }
    let b = Rc::new(b);
    STATE.with_borrow_mut(|s| s.bundles.insert(language.tag, b.clone()));
    b
}

/// The languages a message is looked for in: the current one, its
/// fallbacks, English.
fn chain(language: &'static Language) -> Vec<&'static Language> {
    let mut chain = vec![language];
    let mut at = language;
    while let Some(next) = at.fallback.and_then(find) {
        if chain.contains(&next) {
            break;
        }
        chain.push(next);
        at = next;
    }
    if !chain.contains(&&LANGUAGES[0]) {
        chain.push(&LANGUAGES[0]);
    }
    chain
}

/// Message `id` in the current language, with `args`.
#[doc(hidden)]
pub fn format(id: &str, args: &[(&str, Arg)]) -> UiText {
    let language = current();
    let mut fluent = FluentArgs::with_capacity(args.len());
    for (name, value) in args {
        match value {
            Arg::Number(n) => fluent.set(*name, FluentValue::Number(FluentNumber::from(*n))),
            Arg::Str(s) => fluent.set(*name, FluentValue::from(*s)),
        }
    }
    for l in chain(language) {
        let b = bundle(l);
        let Some(pattern) = b.get_message(id).and_then(|m| m.value()) else {
            continue;
        };
        FORMATTING.set(l);
        let mut errors = Vec::new();
        let text = b.format_pattern(pattern, Some(&fluent), &mut errors);
        debug_assert!(
            errors.is_empty() || l.tag != "en",
            "{id} in English: {errors:?}"
        );
        return UiText(match language.tag {
            "qps-ploc" => pseudo::wrap(&text).into(),
            "qps-wide" => format!("「{text}」").into(),
            _ => Cow::into_owned(text).into(),
        });
    }
    // Every message is English's (the build makes the functions from it).
    UiText(id.into())
}

#[cfg(test)]
mod tests;
