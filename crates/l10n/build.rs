//! The UI's messages made Rust functions (`docs/plans/localization.md`).
//!
//! Reads `locales/languages.toml` and the English files, `locales/en/<area>.ftl`,
//! and writes `messages.rs`: the languages, every language's files
//! embedded, and a module per area with a function per message
//! (`select-songs-match` is `msg::select::songs_match`), its variables
//! its arguments, typed as the message's comment declares them:
//!
//! ```ftl
//! # The search's result line.
//! # Variables:
//! #   $count (Number) - how many songs match
//! select-songs-match = { $count -> … }
//! ```
//!
//! An English message that does not parse, is not named for its file,
//! uses a variable its comment does not declare (or declares one it does
//! not use) fails the build. The other languages are checked by
//! `cargo run -p xtask -- l10n check`, not here: a missing translation
//! falls back to English at run time.

use std::collections::{BTreeMap, BTreeSet};
use std::fmt::Write as _;
use std::path::{Path, PathBuf};

use fluent_syntax::ast;

#[derive(serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct Languages {
    language: Vec<LanguageEntry>,
    #[serde(default)]
    face: Vec<FaceEntry>,
}

/// A face the languages name: its family and its cut files (the cut's
/// recipe, `base`, `features` and each file's `source` and `sha256`, is
/// xtask's, `l10n fonts`).
#[derive(serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct FaceEntry {
    family: String,
    base: String,
    #[allow(dead_code)]
    features: String,
    file: Vec<FaceFile>,
}

#[derive(serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct FaceFile {
    weight: u16,
    file: String,
    #[allow(dead_code)]
    source: String,
    #[allow(dead_code)]
    sha256: String,
}

#[derive(serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct LanguageEntry {
    tag: String,
    name: String,
    status: String,
    #[serde(default)]
    fallback: Option<String>,
    #[serde(default = "dot")]
    decimal: String,
    #[serde(default = "comma")]
    group: String,
    #[serde(default = "one")]
    min_grouping: u32,
    #[serde(default)]
    faces: Vec<String>,
}

fn dot() -> String {
    ".".into()
}
fn comma() -> String {
    ",".into()
}
fn one() -> u32 {
    1
}

#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum Kind {
    Number,
    String,
}

const KEYWORDS: &[&str] = &[
    "as", "async", "await", "box", "break", "const", "continue", "crate", "do", "dyn", "else",
    "enum", "extern", "false", "final", "fn", "for", "gen", "if", "impl", "in", "let", "loop",
    "macro", "match", "mod", "move", "mut", "override", "priv", "pub", "ref", "return", "self",
    "static", "struct", "super", "trait", "true", "try", "type", "typeof", "unsafe", "unsized",
    "use", "virtual", "where", "while", "yield",
];

fn main() {
    let manifest = PathBuf::from(std::env::var("CARGO_MANIFEST_DIR").unwrap());
    let locales = manifest.join("../../locales");
    let locales = locales.canonicalize().unwrap_or(locales);
    println!("cargo:rerun-if-changed={}", locales.display());
    let mut problems = Vec::new();

    let listed = read(&locales.join("languages.toml"));
    let languages: Languages =
        toml::from_str(&listed).unwrap_or_else(|e| panic!("locales/languages.toml: {e}"));
    check_languages(&languages, &locales, &mut problems);

    let mut out = String::from("// Written by build.rs from `locales/`.\n\n");
    write_languages(&mut out, &languages);
    write_faces(&mut out, &languages, &locales.join("../assets/fonts"));
    write_face_chars(&mut out, &languages, &locales);
    write_sources(&mut out, &languages, &locales);

    // The English messages, area by area.
    let mut areas = BTreeMap::new();
    for path in ftl_files(&locales.join("en")) {
        let area = path.file_stem().unwrap().to_string_lossy().into_owned();
        let text = read(&path);
        let messages = english(&area, &text, &mut problems);
        areas.insert(area, messages);
    }
    if !problems.is_empty() {
        panic!(
            "the English messages (locales/en/):\n  {}",
            problems.join("\n  ")
        );
    }
    write_messages(&mut out, &areas);

    let dest = PathBuf::from(std::env::var("OUT_DIR").unwrap()).join("messages.rs");
    std::fs::write(dest, out).unwrap();
}

fn read(path: &Path) -> String {
    std::fs::read_to_string(path).unwrap_or_else(|e| panic!("{}: {e}", path.display()))
}

/// A language's `.ftl` files, sorted.
fn ftl_files(dir: &Path) -> Vec<PathBuf> {
    let mut files: Vec<PathBuf> = std::fs::read_dir(dir)
        .unwrap_or_else(|e| panic!("{}: {e}", dir.display()))
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| p.extension().is_some_and(|x| x == "ftl"))
        .collect();
    files.sort();
    files
}

fn check_languages(languages: &Languages, locales: &Path, problems: &mut Vec<String>) {
    let tags: Vec<&str> = languages.language.iter().map(|l| l.tag.as_str()).collect();
    if tags.first() != Some(&"en") {
        problems.push("languages.toml: English (`en`), the source, comes first".into());
    }
    let unique: BTreeSet<&str> = tags.iter().copied().collect();
    if unique.len() != tags.len() {
        problems.push("languages.toml: a tag is listed twice".into());
    }
    for l in &languages.language {
        if !matches!(l.status.as_str(), "shipped" | "preview") {
            problems.push(format!(
                "languages.toml: {}'s status is `shipped` or `preview`, not `{}`",
                l.tag, l.status
            ));
        }
        if let Some(f) = &l.fallback
            && !unique.contains(f.as_str())
        {
            problems.push(format!(
                "languages.toml: {} falls back to {f}, which is not listed",
                l.tag
            ));
        }
        if !locales.join(&l.tag).is_dir() {
            problems.push(format!("locales/{}/ is missing", l.tag));
        }
        for f in &l.faces {
            if !languages.face.iter().any(|face| face.family == *f) {
                problems.push(format!(
                    "languages.toml: {} names the face {f}, which no [[face]] is",
                    l.tag
                ));
            }
        }
    }
    for face in &languages.face {
        let weights: BTreeSet<u16> = face.file.iter().map(|f| f.weight).collect();
        if face.file.is_empty() || weights.len() != face.file.len() {
            problems.push(format!(
                "languages.toml: {} has no file, or two of one weight",
                face.family
            ));
        }
    }
    for entry in std::fs::read_dir(locales).into_iter().flatten().flatten() {
        let name = entry.file_name().to_string_lossy().into_owned();
        if entry.path().is_dir() && !unique.contains(name.as_str()) {
            problems.push(format!(
                "locales/{name}/ is not in languages.toml (list it as `preview` to start it)"
            ));
        }
    }
}

fn write_languages(out: &mut String, languages: &Languages) {
    out.push_str("/// The languages `locales/languages.toml` lists, English first.\n");
    out.push_str("pub static LANGUAGES: &[Language] = &[\n");
    for l in &languages.language {
        let _ = writeln!(
            out,
            "    Language {{ tag: {:?}, name: {:?}, status: Status::{}, fallback: {:?}, \
             decimal: {:?}, group: {:?}, min_grouping: {}, faces: &{:?} }},",
            l.tag,
            l.name,
            if l.status == "shipped" {
                "Shipped"
            } else {
                "Preview"
            },
            l.fallback.as_deref(),
            l.decimal,
            l.group,
            l.min_grouping,
            l.faces,
        );
    }
    out.push_str("];\n\n");
}

fn write_faces(out: &mut String, languages: &Languages, fonts: &Path) {
    out.push_str("/// Every weight of every face `locales/languages.toml` lists.\n");
    out.push_str("pub static FACES: &[Face] = &[\n");
    for face in &languages.face {
        for f in &face.file {
            let path = fonts.join(&f.file);
            let path = path.canonicalize().unwrap_or(path);
            let _ = writeln!(
                out,
                "    Face {{ family: {:?}, weight: {}, file: {:?}, \
                 #[cfg(feature = \"embed-faces\")] bytes: include_bytes!({:?}) }},",
                face.family,
                f.weight,
                f.file,
                path.display().to_string(),
            );
        }
    }
    out.push_str("];\n\n");
}

/// Whether `c` is wide (CJK, kana, Hangul, full-width forms): a full em
/// in the theme's metrics (`ddi_ui::look::mono_width`), never the theme's
/// face's, a language face's to draw.
fn wide(c: char) -> bool {
    matches!(c as u32, 0x1100..=0x115F | 0x2E80..=0xA4CF | 0xAC00..=0xD7A3 | 0xF900..=0xFAFF | 0xFE30..=0xFE4F | 0xFF00..=0xFF60 | 0xFFE0..=0xFFE6 | 0x20000..=0x3FFFD)
}

/// Every text a resource writes (its messages' and terms' text and string
/// literals, every variant).
fn texts(pattern: &ast::Pattern<&str>, out: &mut String) {
    fn inline(e: &ast::InlineExpression<&str>, out: &mut String) {
        match e {
            ast::InlineExpression::StringLiteral { value } => out.push_str(value),
            ast::InlineExpression::Placeable { expression } => expression_texts(expression, out),
            _ => {}
        }
    }
    fn expression_texts(e: &ast::Expression<&str>, out: &mut String) {
        match e {
            ast::Expression::Select { variants, .. } => {
                for v in variants {
                    texts(&v.value, out);
                }
            }
            ast::Expression::Inline(i) => inline(i, out),
        }
    }
    for e in &pattern.elements {
        match e {
            ast::PatternElement::TextElement { value } => out.push_str(value),
            ast::PatternElement::Placeable { expression } => expression_texts(expression, out),
        }
    }
}

/// Each face's wide characters, as `xtask l10n fonts` cuts it: its base
/// and what the languages naming it write (their messages, their names).
fn write_face_chars(out: &mut String, languages: &Languages, locales: &Path) {
    out.push_str(
        "/// Each language face's wide characters, ranges sorted: its cut's \
         base and what its languages write.\n",
    );
    out.push_str("static FACE_CHARS: &[(&str, &[(u32, u32)])] = &[\n");
    for face in &languages.face {
        let mut set = std::collections::BTreeSet::new();
        for range in face.base.split(',') {
            let range = range.trim().trim_start_matches("U+");
            let (a, b) = range.split_once('-').unwrap_or((range, range));
            let (Ok(a), Ok(b)) = (u32::from_str_radix(a, 16), u32::from_str_radix(b, 16)) else {
                panic!("languages.toml: {}'s base: {range}", face.family);
            };
            set.extend(a..=b);
        }
        for l in languages
            .language
            .iter()
            .filter(|l| l.faces.contains(&face.family))
        {
            let mut written = l.name.clone();
            for path in ftl_files(&locales.join(&l.tag)) {
                let text = read(&path);
                let resource = match fluent_syntax::parser::parse(text.as_str()) {
                    Ok(r) => r,
                    Err((r, _)) => r,
                };
                for entry in &resource.body {
                    match entry {
                        ast::Entry::Message(m) => {
                            if let Some(v) = &m.value {
                                texts(v, &mut written);
                            }
                        }
                        ast::Entry::Term(t) => texts(&t.value, &mut written),
                        _ => {}
                    }
                }
            }
            set.extend(written.chars().filter(|&c| wide(c)).map(u32::from));
        }
        let mut ranges: Vec<(u32, u32)> = Vec::new();
        for c in set {
            match ranges.last_mut() {
                Some(r) if r.1 + 1 == c => r.1 = c,
                _ => ranges.push((c, c)),
            }
        }
        let _ = writeln!(out, "    ({:?}, &{:?}),", face.family, ranges);
    }
    out.push_str("];\n\n");
}

fn write_sources(out: &mut String, languages: &Languages, locales: &Path) {
    out.push_str("/// Each language's files, as written.\n");
    out.push_str("pub(crate) static SOURCES: &[(&str, &[(&str, &str)])] = &[\n");
    for l in &languages.language {
        let _ = writeln!(out, "    ({:?}, &[", l.tag);
        let dir = locales.join(&l.tag);
        if dir.is_dir() {
            for path in ftl_files(&dir) {
                let _ = writeln!(
                    out,
                    "        ({:?}, include_str!({:?})),",
                    path.file_name().unwrap().to_string_lossy(),
                    path.display().to_string(),
                );
            }
        }
        out.push_str("    ]),\n");
    }
    out.push_str("];\n\n");
}

/// One English message, as its function needs it.
struct Msg {
    id: String,
    function: String,
    variables: Vec<(String, Kind)>,
    doc: Vec<String>,
    english: String,
}

fn english(area: &str, text: &str, problems: &mut Vec<String>) -> Vec<Msg> {
    let at = |offset: usize| text[..offset.min(text.len())].lines().count().max(1);
    let file = format!("{area}.ftl");
    if !area
        .chars()
        .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-')
        || !area.starts_with(|c: char| c.is_ascii_lowercase())
    {
        problems.push(format!(
            "{file}: an area is named in lower case, a-z, 0-9 and -"
        ));
    }
    let resource = match fluent_syntax::parser::parse(text) {
        Ok(r) => r,
        Err((r, errors)) => {
            for e in errors {
                problems.push(format!("{file}:{}: {:?}", at(e.pos.start), e.kind));
            }
            r
        }
    };
    let prefix = format!("{area}-");
    let mut messages = Vec::new();
    for entry in &resource.body {
        let ast::Entry::Message(m) = entry else {
            continue;
        };
        let id = m.id.name;
        let Some(rest) = id.strip_prefix(&prefix) else {
            problems.push(format!("{file}: {id} is not named {prefix}…"));
            continue;
        };
        let function = rest.replace('-', "_");
        if KEYWORDS.contains(&function.as_str()) {
            problems.push(format!(
                "{file}: {id} makes the function `{function}`, a Rust keyword: rename it"
            ));
            continue;
        }
        let Some(value) = &m.value else {
            problems.push(format!("{file}: {id} has no value"));
            continue;
        };
        if !m.attributes.is_empty() {
            problems.push(format!(
                "{file}: {id} has attributes, which the generated functions do not take yet"
            ));
        }
        let mut used = BTreeSet::new();
        let mut references = Vec::new();
        pattern_variables(value, &mut used, &mut references);
        for r in references {
            problems.push(format!(
                "{file}: {id} refers to the message {r}; write it out, or use a term (-name)"
            ));
        }
        let comment: Vec<String> = m
            .comment
            .as_ref()
            .map(|c| c.content.iter().map(|l| l.to_string()).collect())
            .unwrap_or_default();
        let (declared, doc) = declarations(&comment);
        let declared_names: BTreeSet<&str> = declared.iter().map(|(n, _)| n.as_str()).collect();
        for v in &used {
            if !declared_names.contains(v.as_str()) {
                problems.push(format!(
                    "{file}: {id} uses ${v}, which its comment does not declare \
                     (`#   ${v} (Number|String) - what it is`)"
                ));
            }
        }
        for (v, _) in &declared {
            if !used.contains(v) {
                problems.push(format!(
                    "{file}: {id} declares ${v} in its comment but does not use it"
                ));
            }
        }
        // The English as a translator sees it, for the function's doc.
        let mut alone = m.clone();
        alone.comment = None;
        let english = fluent_syntax::serializer::serialize(&ast::Resource {
            body: vec![ast::Entry::Message(alone)],
        });
        messages.push(Msg {
            id: id.to_string(),
            function,
            variables: declared,
            doc,
            english: english.trim_end().to_string(),
        });
    }
    messages
}

/// The comment's declared variables (`$name (Number) - …` lines after
/// `Variables:`) and the rest of it.
fn declarations(comment: &[String]) -> (Vec<(String, Kind)>, Vec<String>) {
    let mut declared = Vec::new();
    let mut doc = Vec::new();
    for line in comment {
        let trimmed = line.trim();
        if trimmed == "Variables:" {
            continue;
        }
        if let Some(rest) = trimmed.strip_prefix('$') {
            let name: String = rest
                .chars()
                .take_while(|c| c.is_ascii_alphanumeric() || *c == '_' || *c == '-')
                .collect();
            let after = rest[name.len()..].trim_start();
            let kind = if after.starts_with("(Number)") {
                Some(Kind::Number)
            } else if after.starts_with("(String)") {
                Some(Kind::String)
            } else {
                None
            };
            if let Some(kind) = kind {
                declared.push((name, kind));
                continue;
            }
        }
        doc.push(line.clone());
    }
    (declared, doc)
}

fn pattern_variables(p: &ast::Pattern<&str>, used: &mut BTreeSet<String>, refs: &mut Vec<String>) {
    for e in &p.elements {
        if let ast::PatternElement::Placeable { expression } = e {
            expression_variables(expression, used, refs);
        }
    }
}

fn expression_variables(
    e: &ast::Expression<&str>,
    used: &mut BTreeSet<String>,
    refs: &mut Vec<String>,
) {
    match e {
        ast::Expression::Select { selector, variants } => {
            inline_variables(selector, used, refs);
            for v in variants {
                pattern_variables(&v.value, used, refs);
            }
        }
        ast::Expression::Inline(i) => inline_variables(i, used, refs),
    }
}

fn inline_variables(
    e: &ast::InlineExpression<&str>,
    used: &mut BTreeSet<String>,
    refs: &mut Vec<String>,
) {
    match e {
        ast::InlineExpression::VariableReference { id } => {
            used.insert(id.name.to_string());
        }
        ast::InlineExpression::FunctionReference { arguments, .. } => {
            call_variables(arguments, used, refs)
        }
        ast::InlineExpression::TermReference {
            arguments: Some(a), ..
        } => call_variables(a, used, refs),
        ast::InlineExpression::MessageReference { id, .. } => refs.push(id.name.to_string()),
        ast::InlineExpression::Placeable { expression } => {
            expression_variables(expression, used, refs)
        }
        _ => {}
    }
}

fn call_variables(
    a: &ast::CallArguments<&str>,
    used: &mut BTreeSet<String>,
    refs: &mut Vec<String>,
) {
    for p in &a.positional {
        inline_variables(p, used, refs);
    }
    for n in &a.named {
        inline_variables(&n.value, used, refs);
    }
}

fn write_messages(out: &mut String, areas: &BTreeMap<String, Vec<Msg>>) {
    out.push_str("/// The messages, a module an area (`locales/en/<area>.ftl`).\n");
    out.push_str("pub mod msg {\n");
    for (area, messages) in areas {
        let module = area.replace('-', "_");
        let _ = writeln!(out, "    /// `locales/en/{area}.ftl`.");
        let _ = writeln!(out, "    pub mod {module} {{");
        for m in messages {
            for line in &m.doc {
                let _ = writeln!(out, "        /// {}", line.trim_end());
            }
            if !m.doc.is_empty() {
                out.push_str("        ///\n");
            }
            out.push_str("        /// ```ftl\n");
            for line in m.english.lines() {
                let _ = writeln!(out, "        /// {line}");
            }
            out.push_str("        /// ```\n");
            let params: Vec<String> = m
                .variables
                .iter()
                .map(|(n, k)| {
                    format!(
                        "{}: {}",
                        n.replace('-', "_"),
                        match k {
                            Kind::Number => "impl crate::Number",
                            Kind::String => "impl AsRef<str>",
                        }
                    )
                })
                .collect();
            let args: Vec<String> = m
                .variables
                .iter()
                .map(|(n, k)| {
                    let rust = n.replace('-', "_");
                    match k {
                        Kind::Number => format!("({n:?}, crate::Arg::Number({rust}.value()))"),
                        Kind::String => format!("({n:?}, crate::Arg::Str({rust}.as_ref()))"),
                    }
                })
                .collect();
            let _ = writeln!(
                out,
                "        pub fn {}({}) -> crate::UiText {{\n            crate::format({:?}, &[{}])\n        }}",
                m.function,
                params.join(", "),
                m.id,
                args.join(", "),
            );
        }
        out.push_str("    }\n");
    }
    out.push_str("}\n\n");
    out.push_str(
        "/// Every English message and its variables, for the checks that format them all.\n",
    );
    out.push_str("pub static MESSAGES: &[(&str, &[(&str, ArgKind)])] = &[\n");
    for messages in areas.values() {
        for m in messages {
            let vars: Vec<String> = m
                .variables
                .iter()
                .map(|(n, k)| {
                    format!(
                        "({n:?}, ArgKind::{})",
                        match k {
                            Kind::Number => "Number",
                            Kind::String => "String",
                        }
                    )
                })
                .collect();
            let _ = writeln!(out, "    ({:?}, &[{}]),", m.id, vars.join(", "));
        }
    }
    out.push_str("];\n");
}
