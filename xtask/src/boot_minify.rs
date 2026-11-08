//! `minify-boot`: the page's inline scripts (`site/boot.js`, the boot
//! script; `site/boot-art.js`, the loader's art) minified with swc into
//! `target/boot/`, which `site/index.html` inlines. Trunk inlines a
//! `rel="inline"` script verbatim and its HTML pass leaves JavaScript's
//! comments and names alone (its swc pass serves `data-trunk` script
//! assets only, which come out as files of their own, and a script with
//! a source blocks the first paint); so the page's first round trip
//! (`check_boot`) gets them minified here, as hard as swc goes: several
//! compress passes, top-level names mangled (both scripts are closures),
//! no comments. A dev build (Trunk's profile not release) copies them as
//! they are. Run by Trunk's pre_build hook; by hand with `--release`.
//!
//! The boot script's lines are each language's `locales/<tag>/boot.ftl`
//! (`crate::l10n::boot_lines`): English's and each language's first ones
//! (read as the page opens: [`FIRST`]) put in for its
//! `__DDI_BOOT_LINES__`, the rest of each language's written to
//! `target/boot/lines/<tag>-<hash>.json` (the page's `boot/`), which the
//! script fetches for the language it speaks: every language's lines in
//! the page would not fit its first round trip. Every key the script
//! names (the literals in `L(…)`, `lacks(…)`, `fail(…)`,
//! `showCrashOverlay(…)`) must be one of them, and every one of them
//! named.

use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};
use swc_common::{FileName, GLOBALS, Globals, Mark, SourceMap, sync::Lrc};
use swc_ecma_ast::EsVersion;
use swc_ecma_codegen::{Emitter, text_writer::JsWriter};
use swc_ecma_minifier::{
    optimize,
    option::{CompressOptions, ExtraOptions, MangleOptions, MinifyOptions},
};
use swc_ecma_parser::{EsSyntax, Syntax, parse_file_as_program};
use swc_ecma_transforms_base::{fixer::fixer, resolver};
use swc_ecma_visit::VisitMutWith;

const SCRIPTS: [&str; 2] = ["boot.js", "boot-art.js"];

pub(crate) fn minify_boot(release: Option<bool>) -> Result<()> {
    let release =
        release.unwrap_or_else(|| std::env::var("TRUNK_PROFILE").is_ok_and(|p| p == "release"));
    let out = PathBuf::from("target/boot");
    std::fs::create_dir_all(&out).with_context(|| format!("creating {}", out.display()))?;
    for name in SCRIPTS {
        let source = Path::new("site").join(name);
        let js = std::fs::read_to_string(&source)
            .with_context(|| format!("reading {}", source.display()))?;
        let js = if name == "boot.js" {
            let lines = crate::l10n::boot_lines(!release)?;
            let table = page_lines(&lines, &out.join("lines"))?;
            with_lines(&js, &table)
                .with_context(|| format!("putting the page's lines in {}", source.display()))?
        } else {
            js
        };
        let written = if release {
            let m = minify(&js).with_context(|| format!("minifying {}", source.display()))?;
            eprintln!(
                "minify-boot: {name} {} bytes to {} ({}%)",
                js.len(),
                m.len(),
                m.len() * 100 / js.len().max(1)
            );
            m
        } else {
            eprintln!("minify-boot: {name} as it is (not a release build)");
            js
        };
        let dest = out.join(name);
        std::fs::write(&dest, written).with_context(|| format!("writing {}", dest.display()))?;
    }
    Ok(())
}

/// What stands for the lines in the boot script.
const LINES: &str = "__DDI_BOOT_LINES__";

/// The lines each language needs before anything is fetched: the
/// loader's label, set as the page opens.
const FIRST: &[&str] = &["loading"];

/// The page's table from `lines` (`crate::l10n::boot_lines`):
/// `{"shipped": [tags], "en": {key: line}, "first": {tag: {key: line}},
/// "files": {tag: "boot/<tag>-<hash>.json"}}`, each language's lines but
/// English's written to its file in `dir` (emptied first: old files go).
fn page_lines(lines: &serde_json::Value, dir: &Path) -> Result<serde_json::Value> {
    use sha2::Digest;
    match std::fs::remove_dir_all(dir) {
        Err(e) if e.kind() != std::io::ErrorKind::NotFound => {
            return Err(e).with_context(|| format!("emptying {}", dir.display()));
        }
        _ => {}
    }
    std::fs::create_dir_all(dir).with_context(|| format!("creating {}", dir.display()))?;
    let all = lines["lines"]
        .as_object()
        .context("the lines are by language")?;
    let mut first = serde_json::Map::new();
    let mut files = serde_json::Map::new();
    for (tag, table) in all {
        let table = table.as_object().context("a language's lines are by key")?;
        first.insert(
            tag.clone(),
            FIRST
                .iter()
                .filter_map(|k| Some(((*k).to_string(), table.get(*k)?.clone())))
                .collect::<serde_json::Map<_, _>>()
                .into(),
        );
        if tag == "en" {
            continue;
        }
        let json = serde_json::to_string(table)?;
        let hash: String = sha2::Sha256::digest(json.as_bytes())
            .iter()
            .take(4)
            .map(|b| format!("{b:02x}"))
            .collect();
        let name = format!("{tag}-{hash}.json");
        std::fs::write(dir.join(&name), json)
            .with_context(|| format!("writing {}", dir.join(&name).display()))?;
        files.insert(tag.clone(), format!("boot/{name}").into());
    }
    Ok(serde_json::json!({
        "shipped": lines["shipped"],
        "en": all.get("en").context("English's lines")?,
        "first": first,
        "files": files,
    }))
}

/// `js` with `table` ([`page_lines`]) for its [`LINES`]; an error where
/// it names a key English's lines lack or they have a key it never names.
fn with_lines(js: &str, table: &serde_json::Value) -> Result<String> {
    if js.matches(LINES).count() != 1 {
        bail!("the script must have {LINES} once, where its lines go");
    }
    let keys: Vec<&String> = table["en"]
        .as_object()
        .map(|o| o.keys().collect())
        .unwrap_or_default();
    let named = named_keys(js);
    let mut problems = Vec::new();
    for k in &named {
        if !keys.contains(&k) {
            problems.push(format!(
                "it names \"{k}\", which no message boot-{k} gives (locales/en/boot.ftl)"
            ));
        }
    }
    for k in keys {
        if !named.contains(k) {
            problems.push(format!(
                "boot-{k} (locales/en/boot.ftl) is never named: shown nowhere"
            ));
        }
    }
    if !problems.is_empty() {
        bail!("{}", problems.join("\n"));
    }
    // A `</script>` in a line would end the inlined script.
    let table = table.to_string().replace('<', "\\u003c");
    Ok(js.replacen(LINES, &table, 1))
}

/// The keys `js` names: the string literals in the arguments of the
/// calls taking them.
fn named_keys(js: &str) -> Vec<String> {
    let mut keys = Vec::new();
    for call in ["L(", "lacks(", "fail(", "showCrashOverlay("] {
        let mut rest = js;
        while let Some(at) = rest.find(call) {
            // A whole name only (not `URL(`, `blacks(`).
            let whole = rest[..at]
                .chars()
                .next_back()
                .is_none_or(|c| !(c.is_ascii_alphanumeric() || c == '_' || c == '$'));
            rest = &rest[at + call.len()..];
            if !whole {
                continue;
            }
            // Every literal among its arguments (`L(hint || "update")`).
            let args = &rest[..rest.find(')').unwrap_or(rest.len())];
            keys.extend(args.split('"').skip(1).step_by(2).map(str::to_string));
        }
    }
    keys.sort();
    keys.dedup();
    keys
}

/// `js`, a global script, minified; an error where swc cannot parse it
/// (the page must not ship a script it could not read).
pub(crate) fn minify(js: &str) -> Result<String> {
    let cm: Lrc<SourceMap> = Default::default();
    let fm = cm.new_source_file(Lrc::new(FileName::Anon), js.to_string());
    let syntax = Syntax::Es(EsSyntax {
        jsx: false,
        ..Default::default()
    });
    let mut errors = vec![];
    let program = match parse_file_as_program(&fm, syntax, EsVersion::latest(), None, &mut errors) {
        Ok(p) => p,
        Err(err) => bail!("parsing: {err:?}"),
    };
    if !errors.is_empty() {
        bail!("parsing: {errors:?}");
    }
    GLOBALS.set(&Globals::new(), || {
        let unresolved_mark = Mark::new();
        let top_level_mark = Mark::new();
        let mut program = program;
        program.visit_mut_with(&mut resolver(unresolved_mark, top_level_mark, false));
        let options = MinifyOptions {
            compress: Some(CompressOptions {
                passes: 3,
                top_level: Some(swc_ecma_minifier::option::TopLevelOptions { functions: true }),
                ..Default::default()
            }),
            mangle: Some(MangleOptions {
                top_level: Some(true),
                ..Default::default()
            }),
            ..Default::default()
        };
        let extra = ExtraOptions {
            unresolved_mark,
            top_level_mark,
            mangle_name_cache: None,
        };
        let mut program = optimize(program, cm.clone(), None, None, &options, &extra);
        program.visit_mut_with(&mut fixer(None));
        let mut buf = vec![];
        {
            let mut emitter = Emitter {
                cfg: swc_ecma_codegen::Config::default().with_minify(true),
                cm: cm.clone(),
                comments: None,
                wr: JsWriter::new(cm.clone(), "\n", &mut buf, None),
            };
            emitter.emit_program(&program).context("emitting")?;
        }
        String::from_utf8(buf).context("the minified script is not UTF-8")
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_closure_is_minified_with_its_names_and_comments_gone() {
        let js =
            "// A comment.\n(() => {\n  const longName = 1;\n  window.__X = { longName };\n})();\n";
        let m = minify(js).unwrap();
        assert!(!m.contains("comment") && !m.contains("longName ="), "{m}");
        assert!(m.contains("__X") && m.contains("longName:"), "{m}");
        assert!(m.len() < js.len());
    }

    #[test]
    fn the_lines_go_in_where_every_key_is_one_of_them() {
        let table = serde_json::json!({
            "shipped": ["en"],
            "en": {"loading": "Loading", "no-wasm": "No </script>", "update": "Update", "reload": "Reload"},
            "first": {"en": {"loading": "Loading"}},
            "files": {},
        });
        let js = "const B = __DDI_BOOT_LINES__; L(\"loading\"); lacks(\"no-wasm\", \"update\"); \
                  showCrashOverlay(\"reload\", \"update\", d); new URL(\"x\");";
        let out = with_lines(js, &table).unwrap();
        assert!(out.starts_with("const B = {\"en\""), "{out}");
        assert!(
            !out.contains("</script>") && out.contains("\\u003c/script>"),
            "{out}"
        );
        // A key no message gives, and messages never named.
        let wrong = "const B = __DDI_BOOT_LINES__; L(\"loading\"); L(\"lodaing\");";
        let e = with_lines(wrong, &table).unwrap_err().to_string();
        assert!(
            e.contains("\"lodaing\"") && e.contains("boot-no-wasm") && e.contains("boot-update"),
            "{e}"
        );
        assert!(with_lines("L(\"loading\")", &table).is_err());
    }

    #[test]
    fn the_page_has_english_and_first_lines_and_a_file_for_the_rest() {
        let lines = crate::l10n::boot_lines(true).unwrap();
        let shipped: Vec<&str> = ddi_l10n::all()
            .filter(|l| l.status == ddi_l10n::Status::Shipped)
            .map(|l| l.tag)
            .collect();
        assert_eq!(lines["shipped"], serde_json::json!(shipped));
        let dir = std::env::temp_dir().join(format!("ddi-boot-lines-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("old-00000000.json"), "{}").unwrap();
        let table = page_lines(&lines, &dir).unwrap();
        assert_eq!(
            table["en"]["blocked"],
            "This browser cannot run Dito Dito Infinito."
        );
        assert_eq!(
            table["first"]["ja"],
            serde_json::json!({"loading": "Dito Dito Infinito、読み込み中"})
        );
        assert!(
            table["first"]["qps-ploc"]["loading"]
                .as_str()
                .unwrap()
                .starts_with('«')
        );
        assert!(table["files"].get("en").is_none());
        // Each language's file has all its lines, named by its contents;
        // the old ones are gone.
        let file = table["files"]["ja"].as_str().unwrap();
        let name = file.strip_prefix("boot/").unwrap();
        assert!(name.starts_with("ja-") && name.ends_with(".json"), "{file}");
        let ja: serde_json::Value =
            serde_json::from_str(&std::fs::read_to_string(dir.join(name)).unwrap()).unwrap();
        assert_eq!(
            ja["reload"],
            "問題が発生しました。ページを再読み込みしてください。"
        );
        assert_eq!(
            ja.as_object().unwrap().len(),
            table["en"].as_object().unwrap().len()
        );
        assert!(!dir.join("old-00000000.json").exists());
        std::fs::remove_dir_all(&dir).unwrap();
        // A release build's page has no pseudo-language.
        let release = crate::l10n::boot_lines(false).unwrap();
        assert!(release["lines"].get("ja").is_some() && release["lines"].get("qps-ploc").is_none());
        // The script names exactly the lines there are.
        let js = std::fs::read_to_string(
            std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../site/boot.js"),
        )
        .unwrap();
        with_lines(&js, &table).unwrap();
    }

    #[test]
    fn a_broken_script_is_refused() {
        assert!(minify("(() => {").is_err());
    }
}
