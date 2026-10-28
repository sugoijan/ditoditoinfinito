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
    fn a_broken_script_is_refused() {
        assert!(minify("(() => {").is_err());
    }
}
