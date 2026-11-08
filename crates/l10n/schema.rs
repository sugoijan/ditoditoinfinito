//! What `locales/` holds, read the same way by the build script (the
//! messages made functions) and by `xtask l10n` (the checks, the reviews
//! and the faces' cuts): `languages.toml`'s schema, a language's `.ftl`
//! files, and one walk over a Fluent pattern. Both include this file with
//! `#[path]`: a build script cannot depend on xtask.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

use fluent_syntax::ast;

/// `locales/languages.toml`.
#[derive(serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Languages {
    pub language: Vec<LanguageEntry>,
    #[serde(default)]
    pub face: Vec<FaceEntry>,
}

/// A language: its tag, its name in itself, whether it ships, and how
/// it writes numbers.
#[derive(serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LanguageEntry {
    pub tag: String,
    pub name: String,
    pub status: String,
    #[serde(default)]
    pub fallback: Option<String>,
    #[serde(default = "dot")]
    pub decimal: String,
    #[serde(default = "comma")]
    pub group: String,
    #[serde(default = "one")]
    pub min_grouping: u32,
    #[serde(default)]
    pub faces: Vec<String>,
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

/// A face the languages name: its family, the cut's recipe (`base`,
/// `features`) and its cut files.
#[derive(serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FaceEntry {
    pub family: String,
    pub base: String,
    pub features: String,
    pub file: Vec<FaceFile>,
}

/// One weight of a face: its cut file, and the upstream file it is cut
/// from, pinned by its hash.
#[derive(serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FaceFile {
    pub weight: u16,
    pub file: String,
    pub source: String,
    pub sha256: String,
}

/// A language's `.ftl` files, sorted.
pub fn ftl_files(dir: &Path) -> std::io::Result<Vec<PathBuf>> {
    let mut files: Vec<PathBuf> = std::fs::read_dir(dir)?
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| p.extension().is_some_and(|x| x == "ftl"))
        .collect();
    files.sort();
    Ok(files)
}

/// The plural categories CLDR names.
pub const CATEGORIES: [&str; 6] = ["zero", "one", "two", "few", "many", "other"];

/// What a pattern uses and writes, every variant walked.
#[derive(Default)]
pub struct Found {
    pub variables: BTreeSet<String>,
    pub terms: BTreeSet<String>,
    /// The messages it refers to, in order.
    pub messages: Vec<String>,
    /// Each plural selector's keys, and whether it selects on ordinal
    /// numbers (`NUMBER($n, type: "ordinal")`: 1st, 2nd, 3rd).
    pub plurals: Vec<(BTreeSet<String>, bool)>,
    /// The text it writes (its text and string literals, every variant).
    pub text: String,
}

impl Found {
    pub fn pattern(&mut self, p: &ast::Pattern<&str>) {
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
            ast::InlineExpression::MessageReference { id, .. } => {
                self.messages.push(id.name.to_string())
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
