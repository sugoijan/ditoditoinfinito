//! `l10n`: the UI's languages (`locales/`, `docs/plans/localization.md`).
//!
//! - `check`: every language has every English message and nothing else,
//!   each parses, uses only English's variables and the terms it defines,
//!   and gives its plural selectors every category CLDR has for it; no
//!   string literal is handed to `UiText::data`; then how many messages
//!   each language has reviewed by a person, by an agent, or unverified.
//! - `report <tag>`: each message with its English, translation, comment
//!   and review state, for a review pass.
//! - `review <tag> --by agent|human <ids…>`: records reviews in
//!   `locales/<tag>/reviewed.toml`, each against hashes of the English and
//!   the translation reviewed. A message not recorded there, or recorded
//!   against other text, is unverified: nobody marks a draft, and a
//!   review lapses by itself when either side changes.
//! - `fonts`: cuts the faces `languages.toml` lists to what their
//!   languages' messages use ([`fonts`]); `check` fails a message using a
//!   character none of its language's faces draws.
//! - `notes`: the check, then each language's review states as the
//!   release's notes say them (`.github/workflows/release.yml`).
//! - `package`: what the desktop packages write for the shipped
//!   languages (`tools/package/package.py`): their tags, which macOS's
//!   `Info.plist` declares, and the Linux desktop entry's lines in each.
//!
//! [`boot_lines`] is the web page's loader's table (`xtask minify-boot`).

use std::collections::{BTreeMap, BTreeSet};
use std::fmt::Write as _;
use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};
use fluent_syntax::ast;
use serde::{Deserialize, Serialize};

mod fonts;

#[derive(clap::Subcommand)]
pub enum Action {
    /// Check every language against English, and the code for literal
    /// data; print the review states.
    Check,
    /// Print each message of a language with its English, comment and
    /// review state.
    Report {
        /// The language (`ja`).
        tag: String,
        /// Only the messages not reviewed.
        #[arg(long)]
        unverified: bool,
        /// Only one area (`start`, for `locales/<tag>/start.ftl`).
        #[arg(long)]
        area: Option<String>,
    },
    /// Record that messages of a language were reviewed as they stand.
    Review {
        /// The language (`ja`).
        tag: String,
        /// Who reviewed them: `agent`, or `human` only at the maintainer's
        /// word.
        #[arg(long)]
        by: Reviewer,
        /// Every message of an area (`start`) instead of naming them.
        #[arg(long)]
        area: Option<String>,
        /// The messages' ids.
        ids: Vec<String>,
    },
    /// Cut each face `locales/languages.toml` lists to what its
    /// languages' messages and names use, into `assets/fonts/` (the
    /// upstream files fetched once into `target/fonts/`; pyftsubset
    /// through uv).
    Fonts,
    /// Check, then print each language's review states in Markdown, for
    /// a release's notes.
    Notes,
    /// Print, as JSON, what the desktop packages write for the shipped
    /// languages: `{"languages": [tags], "entry": {tag: {key: line}}}`,
    /// the Linux desktop entry's localized keys (`Comment`, …).
    Package {
        /// The preview languages too (to look at them before they ship).
        #[arg(long)]
        previews: bool,
    },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, clap::ValueEnum)]
#[serde(rename_all = "lowercase")]
pub enum Reviewer {
    Agent,
    Human,
}

pub fn run(action: Action) -> Result<()> {
    let root = root();
    match action {
        Action::Check => check(&root),
        Action::Report {
            tag,
            unverified,
            area,
        } => report(&root, &tag, unverified, area.as_deref()),
        Action::Review { tag, by, area, ids } => review(&root, &tag, by, area.as_deref(), &ids),
        Action::Fonts => fonts::cut(&root),
        Action::Notes => notes(&root),
        Action::Package { previews } => package(previews),
    }
}

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .to_path_buf()
}

#[derive(Deserialize)]
struct Languages {
    language: Vec<LanguageEntry>,
    #[serde(default)]
    face: Vec<fonts::FaceEntry>,
}

#[derive(Deserialize)]
struct LanguageEntry {
    tag: String,
    name: String,
    status: String,
    #[serde(default)]
    faces: Vec<String>,
}

/// A message as written in one language.
struct Message {
    area: String,
    /// Its text without its comment, as Fluent writes it.
    text: String,
    /// FNV-1a of `text`: what a review is recorded against.
    hash: String,
    comment: Vec<String>,
    variables: BTreeSet<String>,
    terms: BTreeSet<String>,
    /// Each selector's keys, where they are all plural categories or
    /// numbers.
    plurals: Vec<(BTreeSet<String>, bool)>,
}

/// A language's messages and terms, and what is wrong with its files.
struct Catalog {
    messages: BTreeMap<String, Message>,
    /// Ids in the order the files have them, area by area.
    order: Vec<String>,
    terms: BTreeSet<String>,
    /// Every character its messages and terms write, with the first
    /// message (or term) writing it.
    chars: BTreeMap<char, String>,
    problems: Vec<String>,
}

#[derive(Serialize, Deserialize)]
struct Review {
    source: String,
    text: String,
    by: Reviewer,
    date: String,
}

fn listed(root: &Path) -> Result<Languages> {
    let path = root.join("locales/languages.toml");
    let text = fs::read_to_string(&path).with_context(|| path.display().to_string())?;
    toml::from_str::<Languages>(&text).with_context(|| path.display().to_string())
}

fn languages(root: &Path) -> Result<Vec<LanguageEntry>> {
    Ok(listed(root)?.language)
}

fn catalog(root: &Path, tag: &str) -> Result<Catalog> {
    let dir = root.join("locales").join(tag);
    let mut files: Vec<PathBuf> = fs::read_dir(&dir)
        .with_context(|| dir.display().to_string())?
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| p.extension().is_some_and(|x| x == "ftl"))
        .collect();
    files.sort();
    let mut c = Catalog {
        messages: BTreeMap::new(),
        order: Vec::new(),
        terms: BTreeSet::new(),
        chars: BTreeMap::new(),
        problems: Vec::new(),
    };
    for path in files {
        let area = path.file_stem().unwrap().to_string_lossy().into_owned();
        let name = format!("{tag}/{area}.ftl");
        let text = fs::read_to_string(&path)?;
        let resource = match fluent_syntax::parser::parse(text.as_str()) {
            Ok(r) => r,
            Err((r, errors)) => {
                for e in errors {
                    let line = text[..e.pos.start.min(text.len())].lines().count().max(1);
                    c.problems.push(format!("{name}:{line}: {:?}", e.kind));
                }
                r
            }
        };
        for entry in &resource.body {
            match entry {
                ast::Entry::Term(t) => {
                    c.terms.insert(t.id.name.to_string());
                    let mut found = Found::default();
                    found.pattern(&t.value);
                    for ch in found.text.chars() {
                        c.chars
                            .entry(ch)
                            .or_insert_with(|| format!("-{}", t.id.name));
                    }
                }
                ast::Entry::Message(m) => {
                    let id = m.id.name.to_string();
                    if c.messages.contains_key(&id) {
                        c.problems.push(format!("{name}: {id} is written twice"));
                        continue;
                    }
                    let mut alone = m.clone();
                    alone.comment = None;
                    let written = fluent_syntax::serializer::serialize(&ast::Resource {
                        body: vec![ast::Entry::Message(alone)],
                    });
                    let written = written.trim_end().to_string();
                    let mut found = Found::default();
                    if let Some(v) = &m.value {
                        found.pattern(v);
                    }
                    for a in &m.attributes {
                        found.pattern(&a.value);
                    }
                    for ch in found.text.chars() {
                        c.chars.entry(ch).or_insert_with(|| id.clone());
                    }
                    c.order.push(id.clone());
                    c.messages.insert(
                        id,
                        Message {
                            area: area.clone(),
                            hash: hash(&written),
                            text: written,
                            comment: m
                                .comment
                                .as_ref()
                                .map(|c| c.content.iter().map(|l| l.to_string()).collect())
                                .unwrap_or_default(),
                            variables: found.variables,
                            terms: found.terms,
                            plurals: found.plurals,
                        },
                    );
                }
                ast::Entry::Junk { content } => {
                    c.problems
                        .push(format!("{name}: cannot read {:?}", content.lines().next()));
                }
                _ => {}
            }
        }
    }
    Ok(c)
}

const CATEGORIES: [&str; 6] = ["zero", "one", "two", "few", "many", "other"];

#[derive(Default)]
struct Found {
    variables: BTreeSet<String>,
    terms: BTreeSet<String>,
    /// Each plural selector's keys, and whether it selects on ordinal
    /// numbers (`NUMBER($n, type: "ordinal")`: 1st, 2nd, 3rd).
    plurals: Vec<(BTreeSet<String>, bool)>,
    /// The text it writes (its text and string literals, every variant).
    text: String,
}

impl Found {
    fn pattern(&mut self, p: &ast::Pattern<&str>) {
        for e in &p.elements {
            match e {
                ast::PatternElement::Placeable { expression } => self.expression(expression),
                ast::PatternElement::TextElement { value } => self.text.push_str(value),
            }
        }
    }

    fn expression(&mut self, e: &ast::Expression<&str>) {
        match e {
            ast::Expression::Select { selector, variants } => {
                self.inline(selector);
                let keys: BTreeSet<String> = variants
                    .iter()
                    .map(|v| match &v.key {
                        ast::VariantKey::Identifier { name } => name.to_string(),
                        ast::VariantKey::NumberLiteral { value } => format!("={value}"),
                    })
                    .collect();
                if keys
                    .iter()
                    .all(|k| k.starts_with('=') || CATEGORIES.contains(&k.as_str()))
                    && keys.iter().any(|k| !k.starts_with('='))
                {
                    let ordinal = matches!(
                        selector,
                        ast::InlineExpression::FunctionReference { id, arguments }
                            if id.name == "NUMBER"
                                && arguments.named.iter().any(|a| {
                                    a.name.name == "type"
                                        && matches!(
                                            &a.value,
                                            ast::InlineExpression::StringLiteral { value }
                                                if *value == "ordinal"
                                        )
                                })
                    );
                    self.plurals.push((keys, ordinal));
                }
                for v in variants {
                    self.pattern(&v.value);
                }
            }
            ast::Expression::Inline(i) => self.inline(i),
        }
    }

    fn inline(&mut self, e: &ast::InlineExpression<&str>) {
        match e {
            ast::InlineExpression::VariableReference { id } => {
                self.variables.insert(id.name.to_string());
            }
            ast::InlineExpression::FunctionReference { arguments, .. } => self.call(arguments),
            ast::InlineExpression::TermReference { id, arguments, .. } => {
                self.terms.insert(id.name.to_string());
                if let Some(a) = arguments {
                    self.call(a);
                }
            }
            ast::InlineExpression::Placeable { expression } => self.expression(expression),
            ast::InlineExpression::StringLiteral { value } => self.text.push_str(value),
            _ => {}
        }
    }

    fn call(&mut self, a: &ast::CallArguments<&str>) {
        for p in &a.positional {
            self.inline(p);
        }
        for n in &a.named {
            self.inline(&n.value);
        }
    }
}

/// FNV-1a, 64 bits, as hex: stable everywhere, enough to see a change.
fn hash(text: &str) -> String {
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    for b in text.as_bytes() {
        h ^= u64::from(*b);
        h = h.wrapping_mul(0x0000_0100_0000_01b3);
    }
    format!("{h:016x}")
}

/// The plural categories CLDR gives `tag`'s cardinal numbers.
fn categories(tag: &str, ordinal: bool) -> BTreeSet<String> {
    use intl_pluralrules::{PluralCategory, PluralRuleType, PluralRules};
    let Ok(id) = tag.parse::<unic_langid::LanguageIdentifier>() else {
        return BTreeSet::from(["other".to_string()]);
    };
    let kind = if ordinal {
        PluralRuleType::ORDINAL
    } else {
        PluralRuleType::CARDINAL
    };
    let Ok(rules) = PluralRules::create(id, kind) else {
        return BTreeSet::from(["other".to_string()]);
    };
    let mut found = BTreeSet::new();
    let decimals = [
        "0.5", "1.5", "2.5", "0.1", "1.1", "2.1", "5.5", "10.25", "1.0",
    ];
    let numbers = (0..=1000)
        .map(|n| n.to_string())
        .chain(decimals.map(String::from));
    for n in numbers {
        if let Ok(c) = rules.select(n.as_str()) {
            found.insert(
                match c {
                    PluralCategory::ZERO => "zero",
                    PluralCategory::ONE => "one",
                    PluralCategory::TWO => "two",
                    PluralCategory::FEW => "few",
                    PluralCategory::MANY => "many",
                    PluralCategory::OTHER => "other",
                }
                .to_string(),
            );
        }
    }
    found
}

fn reviews(root: &Path, tag: &str) -> Result<BTreeMap<String, Review>> {
    let path = root.join("locales").join(tag).join("reviewed.toml");
    if !path.exists() {
        return Ok(BTreeMap::new());
    }
    let text = fs::read_to_string(&path)?;
    toml::from_str(&text).with_context(|| path.display().to_string())
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum State {
    Human,
    Agent,
    Unverified,
}

fn state(id: &str, en: &Catalog, it: &Catalog, reviews: &BTreeMap<String, Review>) -> State {
    let (Some(source), Some(text)) = (en.messages.get(id), it.messages.get(id)) else {
        return State::Unverified;
    };
    match reviews.get(id) {
        Some(r) if r.source == source.hash && r.text == text.hash => match r.by {
            Reviewer::Human => State::Human,
            Reviewer::Agent => State::Agent,
        },
        _ => State::Unverified,
    }
}

/// A language's review states (`check`, `notes`).
struct Row {
    tag: String,
    name: String,
    status: String,
    messages: usize,
    /// Reviewed by a person, by an agent, unverified.
    counts: [usize; 3],
}

fn check(root: &Path) -> Result<()> {
    let rows = checked(root)?;
    for r in rows.iter().filter(|r| r.tag != "en") {
        println!(
            "{} ({}): {} messages; reviewed by a person {}, by an agent {}, unverified {}",
            r.tag, r.status, r.messages, r.counts[0], r.counts[1], r.counts[2],
        );
    }
    Ok(())
}

/// Every language checked (an error listing what is wrong), with its
/// review states, English first.
fn checked(root: &Path) -> Result<Vec<Row>> {
    let languages = languages(root)?;
    let en = catalog(root, "en")?;
    let mut problems = en.problems.clone();
    problems.extend(collisions("en", &en));
    problems.extend(boot_problems(&en));
    let mut rows = Vec::new();
    for l in &languages {
        if l.tag == "en" {
            rows.push(Row {
                tag: l.tag.clone(),
                name: l.name.clone(),
                status: l.status.clone(),
                messages: en.messages.len(),
                counts: [0; 3],
            });
            continue;
        }
        let it = catalog(root, &l.tag)?;
        problems.extend(it.problems.iter().cloned());
        let tag = &l.tag;
        let cats = categories(tag, false);
        let ordinals = categories(tag, true);
        for (id, source) in &en.messages {
            let Some(m) = it.messages.get(id) else {
                problems.push(format!(
                    "{tag}: {id} is missing ({}/{}.ftl; English: {})",
                    tag,
                    source.area,
                    source.text.trim_start_matches(&format!("{id} =")).trim()
                ));
                continue;
            };
            if m.area != source.area {
                problems.push(format!(
                    "{tag}: {id} is in {}.ftl, English's in {}.ftl",
                    m.area, source.area
                ));
            }
            for v in m.variables.difference(&source.variables) {
                problems.push(format!(
                    "{tag}: {id} uses ${v}, which English does not pass"
                ));
            }
            for t in &m.terms {
                if !it.terms.contains(t) {
                    problems.push(format!(
                        "{tag}: {id} uses the term -{t}, which {tag} does not define"
                    ));
                }
            }
            for (keys, ordinal) in &m.plurals {
                let cats = if *ordinal { &ordinals } else { &cats };
                let missing: Vec<&String> = cats.iter().filter(|c| !keys.contains(*c)).collect();
                if !missing.is_empty() {
                    problems.push(format!(
                        "{tag}: {id} has no variant for the plural categories {missing:?} \
                         ({tag} has {cats:?})"
                    ));
                }
                let extra: Vec<&String> = keys
                    .iter()
                    .filter(|k| !k.starts_with('=') && !cats.contains(*k))
                    .collect();
                if !extra.is_empty() {
                    problems.push(format!(
                        "{tag}: {id} has variants for {extra:?}, which {tag} never uses \
                         ({tag} has {cats:?})"
                    ));
                }
            }
        }
        for id in it.messages.keys() {
            if !en.messages.contains_key(id) {
                problems.push(format!("{tag}: {id} is not an English message"));
            }
        }
        problems.extend(collisions(tag, &it));
        let reviewed = reviews(root, tag)?;
        let mut counts = [0usize; 3];
        for id in en.messages.keys() {
            counts[match state(id, &en, &it, &reviewed) {
                State::Human => 0,
                State::Agent => 1,
                State::Unverified => 2,
            }] += 1;
        }
        rows.push(Row {
            tag: tag.clone(),
            name: l.name.clone(),
            status: l.status.clone(),
            messages: en.messages.len(),
            counts,
        });
    }
    problems.extend(lint(root)?);
    problems.extend(fonts::check(root)?);
    if !problems.is_empty() {
        bail!(
            "the UI's languages (locales/, docs/plans/localization.md):\n  {}",
            problems.join("\n  ")
        );
    }
    Ok(rows)
}

/// The release's notes on its languages: each one's status and review
/// states, after the check passed (a release needs every language
/// complete).
fn notes(root: &Path) -> Result<()> {
    let rows = checked(root)?;
    print!("{}", notes_text(&rows));
    Ok(())
}

fn notes_text(rows: &[Row]) -> String {
    let n = |n: usize| {
        let digits = n.to_string();
        let mut out = String::new();
        for (i, c) in digits.chars().enumerate() {
            if i > 0 && (digits.len() - i).is_multiple_of(3) {
                out.push(',');
            }
            out.push(c);
        }
        out
    };
    let mut out = String::from(
        "## Languages\n\n\
         Every language has every message (`xtask l10n check`). A translation \
         stays unverified until a review of exactly its text and its English \
         is recorded (`locales/<tag>/reviewed.toml`).\n\n\
         | Language | Status | Messages | Reviewed by a person | Reviewed by an agent | Unverified |\n\
         | --- | --- | ---: | ---: | ---: | ---: |\n",
    );
    for r in rows {
        let _ = if r.tag == "en" {
            writeln!(
                out,
                "| {} (`{}`) | {} | {} | the source | | |",
                r.name,
                r.tag,
                r.status,
                n(r.messages)
            )
        } else {
            writeln!(
                out,
                "| {} (`{}`) | {} | {} | {} | {} | {} |",
                r.name,
                r.tag,
                r.status,
                n(r.messages),
                n(r.counts[0]),
                n(r.counts[1]),
                n(r.counts[2])
            )
        };
    }
    out
}

/// The web page's loader's messages (`boot.ftl`) are formatted when the
/// page is built, with nothing to pass: none may take a variable.
fn boot_problems(en: &Catalog) -> Vec<String> {
    en.messages
        .iter()
        .filter(|(_, m)| m.area == "boot" && !m.variables.is_empty())
        .map(|(id, m)| {
            format!(
                "en: {id} takes {}: the page's loader formats its lines when the page \
                 is built (boot.ftl), with nothing to pass",
                m.variables
                    .iter()
                    .map(|v| format!("${v}"))
                    .collect::<Vec<_>>()
                    .join(", ")
            )
        })
        .collect()
}

/// The web page's loader's lines (`site/boot.js`; `xtask minify-boot`
/// makes the page's table and files from them): `{"shipped": [tags],
/// "lines": {tag: {key: line}}}`, each line a message of `boot.ftl` (its
/// key the id without `boot-`) formatted as the game formats it, in
/// every language listed (shipped and preview: either may be chosen),
/// English first, and with `pseudo` (a dev build's) the pseudo-languages
/// too (each language costs the page's first bytes a little).
pub(crate) fn boot_lines(pseudo: bool) -> Result<serde_json::Value> {
    let en = catalog(&root(), "en")?;
    let problems = boot_problems(&en);
    if !problems.is_empty() {
        bail!("{}", problems.join("\n"));
    }
    let ids: Vec<&String> = en
        .order
        .iter()
        .filter(|id| en.messages[*id].area == "boot")
        .collect();
    let mut lines = serde_json::Map::new();
    let mut shipped = Vec::new();
    let was = ddi_l10n::current().tag;
    for l in ddi_l10n::all() {
        if l.status == ddi_l10n::Status::Pseudo && !pseudo {
            continue;
        }
        if l.status == ddi_l10n::Status::Shipped {
            shipped.push(l.tag);
        }
        ddi_l10n::set_language(l.tag);
        let table: serde_json::Map<String, serde_json::Value> = ids
            .iter()
            .map(|id| {
                let key = id.trim_start_matches("boot-").to_string();
                (key, ddi_l10n::format(id, &[]).as_str().into())
            })
            .collect();
        lines.insert(l.tag.to_string(), table.into());
    }
    ddi_l10n::set_language(was);
    Ok(serde_json::json!({ "shipped": shipped, "lines": lines }))
}

/// The Linux desktop entry's keys and the messages giving them.
const ENTRY: [(&str, &str); 3] = [
    ("GenericName", "shell-entry-kind"),
    ("Comment", "shell-entry-comment"),
    ("Keywords", "shell-entry-keywords"),
];

/// What the desktop packages write for the shipped languages (with
/// `previews`, the preview ones too), as JSON on standard output.
fn package(previews: bool) -> Result<()> {
    println!("{}", serde_json::to_string_pretty(&package_json(previews))?);
    Ok(())
}

fn package_json(previews: bool) -> serde_json::Value {
    let was = ddi_l10n::current().tag;
    let languages: Vec<&'static ddi_l10n::Language> = ddi_l10n::all()
        .filter(|l| {
            l.status == ddi_l10n::Status::Shipped
                || (previews && l.status == ddi_l10n::Status::Preview)
        })
        .collect();
    let mut entry = serde_json::Map::new();
    for l in &languages {
        ddi_l10n::set_language(l.tag);
        let keys: serde_json::Map<String, serde_json::Value> = ENTRY
            .iter()
            .map(|(key, id)| ((*key).to_string(), ddi_l10n::format(id, &[]).as_str().into()))
            .collect();
        entry.insert(l.tag.to_string(), keys.into());
    }
    ddi_l10n::set_language(was);
    serde_json::json!({
        "languages": languages.iter().map(|l| l.tag).collect::<Vec<_>>(),
        "entry": entry,
    })
}

/// Terms named as a message is (`-studio-x` beside `studio-x`): a bundle
/// keeps one of the two, and the message then shows its id.
fn collisions(tag: &str, c: &Catalog) -> Vec<String> {
    c.terms
        .iter()
        .filter(|t| c.messages.contains_key(*t))
        .map(|t| format!("{tag}: the term -{t} and the message {t} share a name: rename one"))
        .collect()
}

/// String literals handed to `UiText::data`, which are almost always
/// English slipping past the messages (symbols are fine); tests are left
/// alone.
/// Calls whose text a player reads, and what the check calls them: a
/// literal (or a `format!` of one) with letters in it is text written in
/// code. The screens' text is the coverage walk's (`l10n_tests.rs`);
/// these are what it cannot see: data marked so, the web's plain pages,
/// the desktop's window and file dialogs.
const TEXT_CALLS: &[(&str, &str)] = &[
    ("UiText::data(", "UiText::data"),
    ("set_text_content(Some(", "a page's text"),
    ("set_attribute(\"aria-label\",", "a page's label"),
    ("set_attribute(\"title\",", "a page's tooltip"),
    ("with_title(", "a window's title"),
    ("add_filter(", "a file dialog's kind of file"),
];

/// The developer pages, in English only (`docs/plans/localization.md`,
/// "What stays as it is").
const DEVELOPER_PAGES: &[&str] = &[
    "crates/web/src/components/video_check.rs",
    "crates/web/src/components/ui_test.rs",
    "crates/web/src/components/skin_preview.rs",
];

fn lint(root: &Path) -> Result<Vec<String>> {
    let mut problems = Vec::new();
    let mut stack = vec![root.join("crates")];
    while let Some(dir) = stack.pop() {
        for entry in fs::read_dir(&dir)?.flatten() {
            let path = entry.path();
            let name = entry.file_name().to_string_lossy().into_owned();
            if path.is_dir() {
                if name != "tests" && name != "target" {
                    stack.push(path);
                }
                continue;
            }
            if path.extension().is_none_or(|x| x != "rs")
                || name == "tests.rs"
                || name.ends_with("_tests.rs")
            {
                continue;
            }
            let text = fs::read_to_string(&path)?;
            let relative = path.strip_prefix(root).unwrap_or(&path);
            let developer = DEVELOPER_PAGES.iter().any(|d| relative == Path::new(d));
            for (call, what) in TEXT_CALLS {
                if developer && *call != "UiText::data(" {
                    continue;
                }
                for (at, _) in text.match_indices(call) {
                    let mut rest = text[at + call.len()..].trim_start();
                    // Data may be formatted (readouts, a ruleset's
                    // names); text for the page or the system may not.
                    if *call != "UiText::data(" {
                        rest = rest.strip_prefix('&').unwrap_or(rest);
                        rest = rest.strip_prefix("format!(").unwrap_or(rest);
                    }
                    let rest = rest.strip_prefix('r').unwrap_or(rest);
                    let Some(literal) = rest.strip_prefix('"') else {
                        continue;
                    };
                    let end = literal.find('"').unwrap_or(literal.len());
                    if literal[..end].chars().any(char::is_alphabetic) {
                        let line = text[..at].matches('\n').count() + 1;
                        problems.push(format!(
                            "{}:{line}: a literal given to {what}; make it a message",
                            relative.display()
                        ));
                    }
                }
            }
        }
    }
    Ok(problems)
}

fn report(root: &Path, tag: &str, unverified: bool, area: Option<&str>) -> Result<()> {
    let en = catalog(root, "en")?;
    let it = catalog(root, tag)?;
    let reviewed = reviews(root, tag)?;
    for id in &en.order {
        let source = &en.messages[id];
        if area.is_some_and(|a| a != source.area) {
            continue;
        }
        let s = state(id, &en, &it, &reviewed);
        if unverified && s != State::Unverified {
            continue;
        }
        println!(
            "{id} ({}.ftl): {}",
            source.area,
            match s {
                State::Human => "reviewed by a person",
                State::Agent => "reviewed by an agent",
                State::Unverified => "unverified",
            }
        );
        for line in &source.comment {
            println!("  # {line}");
        }
        for line in source.text.lines() {
            println!("  en: {line}");
        }
        match it.messages.get(id) {
            Some(m) => {
                for line in m.text.lines() {
                    println!("  {tag}: {line}");
                }
            }
            None => println!("  {tag}: (missing)"),
        }
        println!();
    }
    Ok(())
}

fn review(root: &Path, tag: &str, by: Reviewer, area: Option<&str>, ids: &[String]) -> Result<()> {
    if tag == "en" {
        bail!("English is the source; its messages are not reviewed here");
    }
    let en = catalog(root, "en")?;
    let it = catalog(root, tag)?;
    let mut chosen: Vec<String> = ids.to_vec();
    if let Some(a) = area {
        chosen.extend(
            en.order
                .iter()
                .filter(|id| en.messages[*id].area == a)
                .cloned(),
        );
    }
    if chosen.is_empty() {
        bail!("name the messages reviewed, or --area");
    }
    let mut reviewed = reviews(root, tag)?;
    let date = today();
    for id in &chosen {
        let (Some(source), Some(text)) = (en.messages.get(id), it.messages.get(id)) else {
            bail!("{id} is not a message of both English and {tag}");
        };
        reviewed.insert(
            id.clone(),
            Review {
                source: source.hash.clone(),
                text: text.hash.clone(),
                by,
                date: date.clone(),
            },
        );
    }
    let mut out = format!(
        "# Reviews of the {tag} messages (docs/plans/localization.md), each\n\
         # against hashes of the English and the translation reviewed. Written\n\
         # by `cargo run -p xtask -- l10n review`; a message not here, or here\n\
         # with other hashes, is unverified.\n\n"
    );
    out.push_str(&toml::to_string(&reviewed)?);
    let path = root.join("locales").join(tag).join("reviewed.toml");
    fs::write(&path, out)?;
    println!("{} reviews recorded in {}", chosen.len(), path.display());
    Ok(())
}

/// Today's date (UTC), `YYYY-MM-DD`.
fn today() -> String {
    let secs = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_secs());
    // Days to a civil date (Howard Hinnant's algorithm).
    let z = (secs / 86_400) as i64 + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = yoe + era * 400 + i64::from(m <= 2);
    format!("{y:04}-{m:02}-{d:02}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn plural_categories_are_cldr_s() {
        let set = |c: &[&str]| c.iter().map(|s| s.to_string()).collect::<BTreeSet<_>>();
        assert_eq!(categories("en", false), set(&["one", "other"]));
        assert_eq!(categories("ja", false), set(&["other"]));
        assert_eq!(
            categories("ru", false),
            set(&["few", "many", "one", "other"])
        );
        assert_eq!(categories("fil", false), set(&["one", "other"]));
        // Ordinals: 1st, 2nd, 3rd, 4th; Japanese has one form.
        assert_eq!(categories("en", true), set(&["few", "one", "other", "two"]));
        assert_eq!(categories("ja", true), set(&["other"]));
    }

    #[test]
    fn the_notes_list_each_language_with_its_review_states() {
        let row = |tag: &str, name: &str, status: &str, counts| Row {
            tag: tag.into(),
            name: name.into(),
            status: status.into(),
            messages: 1607,
            counts,
        };
        let notes = notes_text(&[
            row("en", "English", "shipped", [0; 3]),
            row("ja", "日本語", "preview", [7, 1200, 400]),
        ]);
        assert!(notes.starts_with("## Languages\n"), "{notes}");
        assert!(
            notes.contains("| English (`en`) | shipped | 1,607 | the source | | |\n"),
            "{notes}"
        );
        assert!(
            notes.contains("| 日本語 (`ja`) | preview | 1,607 | 7 | 1,200 | 400 |\n"),
            "{notes}"
        );
    }

    #[test]
    fn packages_write_the_shipped_languages_and_the_previews_when_asked() {
        let tags = |status: &[ddi_l10n::Status]| -> Vec<&str> {
            ddi_l10n::all()
                .filter(|l| status.contains(&l.status))
                .map(|l| l.tag)
                .collect()
        };
        let shipped = package_json(false);
        let expected = tags(&[ddi_l10n::Status::Shipped]);
        assert_eq!(shipped["languages"], serde_json::json!(expected));
        assert_eq!(shipped["entry"].as_object().unwrap().len(), expected.len());
        assert_eq!(shipped["entry"]["en"]["GenericName"], "Dance rhythm game");
        let previews = package_json(true);
        let expected = tags(&[ddi_l10n::Status::Shipped, ddi_l10n::Status::Preview]);
        assert_eq!(previews["languages"], serde_json::json!(expected));
        assert_eq!(previews["entry"]["ja"]["GenericName"], "ダンスリズムゲーム");
        assert!(
            previews["entry"]["ja"]["Keywords"]
                .as_str()
                .unwrap()
                .ends_with(';')
        );
    }

    #[test]
    fn text_written_in_code_for_pages_dialogs_and_windows_is_found() {
        let root = std::env::temp_dir().join(format!("ddi-l10n-lint-{}", std::process::id()));
        let web = root.join("crates/web/src/components");
        fs::create_dir_all(&web).unwrap();
        fs::write(
            web.join("page.rs"),
            "fn f(p: &str, text: &str) {\n\
             el.set_text_content(Some(\"No graphics\"));\n\
             el.set_text_content(Some(&format!(\"no route `{p}`\")));\n\
             el.set_text_content(Some(&text));\n\
             el.set_text_content(Some(\"← \"));\n\
             rfd::FileDialog::new().add_filter(\"Music\", &[\"ogg\"]);\n\
             Window::default_attributes().with_title(\"Dito\");\n\
             UiText::data(format!(\"{} · {name}\", 1));\n\
             UiText::data(\"Giko\");\n\
             }\n",
        )
        .unwrap();
        // A developer page is English: only its data is held to the rule.
        fs::write(
            web.join("video_check.rs"),
            "el.set_text_content(Some(\"Video decoder check\"));\n",
        )
        .unwrap();
        let found = lint(&root).unwrap();
        fs::remove_dir_all(&root).unwrap();
        let mut lines: Vec<&str> = found
            .iter()
            .map(|p| p.split(": ").next().unwrap())
            .collect();
        lines.sort();
        let page = "crates/web/src/components/page.rs";
        assert_eq!(
            lines,
            [2, 3, 6, 7, 9].map(|l| format!("{page}:{l}")),
            "{found:#?}"
        );
        assert!(
            found.iter().any(|p| p.contains(":7: a literal given to a window's title")),
            "{found:#?}"
        );
    }

    #[test]
    fn hashes_are_stable() {
        assert_eq!(hash(""), "cbf29ce484222325");
        assert_eq!(hash("a"), "af63dc4c8601ec8c");
    }
}
