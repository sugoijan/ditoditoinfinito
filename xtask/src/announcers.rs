//! `gen-announcers`: check the bundled announcers (`assets/announcers/<name>/`,
//! ours, which `tools/announcer-design/kokoro.py pack --bundle` makes, each
//! with a `PROVENANCE.toml`) and write `target/announcers/index.json`, the
//! index the web fetches: every announcer's lines, their sounds and each
//! sound's hash. Desktop reads the folders themselves.

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};
use ddi_library::announcer::{Announcer, Bundled, Index, Kept, find_announcers};
use serde::Deserialize;

use crate::songs::LicenseEntry;

fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("xtask lives in the workspace root")
        .to_path_buf()
}

/// `assets/announcers/<name>/PROVENANCE.toml`, as kokoro.py writes it.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Provenance {
    /// Its folder's name.
    pub(crate) name: String,
    /// Who holds it.
    pub(crate) author: String,
    pub(crate) language: String,
    /// When its last sound was made.
    pub(crate) made: String,
    /// One line: the model and the voice.
    pub(crate) made_with: String,
    pub(crate) kokoro: Made,
    pub(crate) licenses: Vec<LicenseEntry>,
    /// What its voice learned from, where that asks for credit.
    #[serde(default)]
    pub(crate) trained_on: Vec<TrainedOn>,
}

/// What made it: `[kokoro]`.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
#[allow(dead_code)] // checked and kept with the pack, not listed
pub(crate) struct Made {
    /// The pack file it was made from.
    pub(crate) pack: String,
    pub(crate) model: String,
    pub(crate) revision: String,
    pub(crate) voice: String,
    pub(crate) speed: f64,
    pub(crate) sound: String,
    pub(crate) versions: BTreeMap<String, String>,
}

/// `[[trained_on]]`: recordings a voice learned from.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct TrainedOn {
    pub(crate) voice: String,
    pub(crate) what: String,
    pub(crate) url: String,
    pub(crate) spdx: String,
    pub(crate) holders: Vec<String>,
}

/// Every bundled announcer: its folder's name, provenance and what was
/// found in it. Each must be one announcer of known lines and their
/// sounds alone, beside its PROVENANCE.toml.
pub(crate) fn scan(root: &Path) -> Result<Vec<(String, Provenance, Announcer)>> {
    let dir = root.join("assets/announcers");
    let mut out = Vec::new();
    if !dir.is_dir() {
        return Ok(out);
    }
    let mut folders: Vec<PathBuf> = fs::read_dir(&dir)?
        .filter_map(|e| e.ok())
        .map(|e| e.path())
        .filter(|p| p.is_dir())
        .collect();
    folders.sort();
    for folder in folders {
        let name = folder
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default();
        let prov_path = folder.join("PROVENANCE.toml");
        let prov: Provenance = toml::from_str(
            &fs::read_to_string(&prov_path)
                .with_context(|| format!("reading {}", prov_path.display()))?,
        )
        .with_context(|| format!("parsing {}", prov_path.display()))?;
        if prov.name != name {
            bail!("{name}: PROVENANCE.toml names it {:?}", prov.name);
        }
        if prov.licenses.is_empty() {
            bail!("{name}: PROVENANCE.toml has no [[licenses]] entries");
        }
        for spdx in prov
            .licenses
            .iter()
            .map(|l| &l.spdx)
            .chain(prov.trained_on.iter().map(|t| &t.spdx))
        {
            if !root.join("LICENSES").join(format!("{spdx}.txt")).is_file() {
                bail!("{name}: {spdx} is not in LICENSES/");
            }
        }
        let mut files = Vec::new();
        crate::noteskins::walk(&folder, "", &mut files)?;
        files.retain(|f| f != "PROVENANCE.toml");
        let listed: Vec<(String, u64)> = files
            .iter()
            .map(|f| Ok((format!("{name}/{f}"), fs::metadata(folder.join(f))?.len())))
            .collect::<Result<_>>()?;
        let mut found = find_announcers(&listed, &name).announcers;
        if found.len() != 1 || found[0].dir.as_str() != name {
            bail!("{name}: not one announcer at its folder");
        }
        let a = found.remove(0);
        let r = &a.report;
        let mut stray: Vec<String> = a.unused.keys().chain(a.unknown.keys()).cloned().collect();
        stray.extend(r.empty.iter().cloned());
        if !stray.is_empty() {
            bail!("{name}: folders no line of ours: {}", stray.join(", "));
        }
        if r.ignored != 0 || r.left_out != 0 || !r.nested.is_empty() {
            bail!(
                "{name}: {} files that are no line's sound, {} left out, {} nested",
                r.ignored,
                r.left_out,
                r.nested.len()
            );
        }
        out.push((name, prov, a));
    }
    Ok(out)
}

/// What the shells keep of a bundled announcer.
fn kept(a: &Announcer) -> Kept {
    Kept {
        bundled: true,
        ..a.kept()
    }
}

pub(crate) fn gen_announcers(check: bool) -> Result<()> {
    let root = repo_root();
    let dir = root.join("assets/announcers");
    let index = Index {
        announcers: scan(&root)?
            .into_iter()
            .map(|(path, _, a)| {
                let kept = kept(&a);
                let hashes = kept
                    .lines
                    .values()
                    .flatten()
                    .map(|f| {
                        let file = f
                            .to_path(dir.join(&path))
                            .with_context(|| format!("{path}/{f}"))?;
                        let bytes = fs::read(file)?;
                        Ok((f.to_string(), ddi_noteskin::report::hash(&bytes)))
                    })
                    .collect::<Result<_>>()?;
                Ok(Bundled { path, kept, hashes })
            })
            .collect::<Result<_>>()?,
    };
    let json = serde_json::to_string(&index)? + "\n";
    let out_dir = root.join("target/announcers");
    let out = out_dir.join("index.json");
    if check {
        if fs::read_to_string(&out).unwrap_or_default() != json {
            bail!(
                "{} is stale; run `cargo run -p xtask -- gen-announcers`",
                out.display()
            );
        }
        return Ok(());
    }
    fs::create_dir_all(&out_dir)?;
    if fs::read_to_string(&out).is_ok_and(|c| c == json) {
        return Ok(());
    }
    fs::write(&out, json)?;
    println!(
        "wrote {} ({} announcers)",
        out.display(),
        index.announcers.len()
    );
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    const PROVENANCE: &str = r#"name = "Voice"
author = "a"
language = "British English"
made = "2026-11-06"
made_with = "Kokoro-82M (hexgrad, Apache-2.0), voice bm_george"

[kokoro]
pack = "tools/announcer-design/pack.toml"
model = "https://huggingface.co/onnx-community/Kokoro-82M-v1.0-ONNX"
revision = "0"
voice = "bm_george"
speed = 1
sound = "MP3"
versions = { "misaki" = "0.9.4" }

[[licenses]]
part = "all"
spdx = "CC0-1.0"
holders = ["a"]
"#;

    /// A repository under a fresh temporary folder with one announcer
    /// holding `files` beside its PROVENANCE.toml.
    fn library(tag: &str, name: &str, provenance: &str, files: &[&str]) -> PathBuf {
        let root =
            std::env::temp_dir().join(format!("ddi-gen-announcers-{tag}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        let dir = root.join("assets/announcers").join(name);
        fs::create_dir_all(&dir).unwrap();
        fs::write(dir.join("PROVENANCE.toml"), provenance).unwrap();
        for f in files {
            let p = dir.join(f);
            fs::create_dir_all(p.parent().unwrap()).unwrap();
            fs::write(p, f.as_bytes()).unwrap();
        }
        fs::create_dir_all(root.join("LICENSES")).unwrap();
        fs::write(root.join("LICENSES/CC0-1.0.txt"), "").unwrap();
        root
    }

    const LINES: [&str; 4] = [
        "gameplay ready/ready.mp3",
        "gameplay ready/get-ready.mp3",
        "gameplay cleared/cleared.mp3",
        "evaluation aa/excellent.mp3",
    ];

    #[test]
    fn an_announcer_lists_its_lines_sounds() {
        let root = library("ok", "Voice", PROVENANCE, &LINES);
        let found = scan(&root);
        fs::remove_dir_all(&root).unwrap();
        let found = found.unwrap();
        let (path, prov, a) = &found[0];
        assert_eq!((path.as_str(), prov.author.as_str()), ("Voice", "a"));
        let kept = kept(a);
        assert_eq!(kept.key(), "bundled:voice");
        assert_eq!(kept.files, 4);
        let ready: Vec<String> = kept.lines["gameplay ready"]
            .iter()
            .map(ToString::to_string)
            .collect();
        assert_eq!(
            ready,
            ["gameplay ready/get-ready.mp3", "gameplay ready/ready.mp3"]
        );
    }

    #[test]
    fn strays_and_bad_records_are_refused() {
        let mut stray = LINES.to_vec();
        stray.push("notes.txt");
        let mut unknown = LINES.to_vec();
        unknown.push("screen bonus/x.mp3");
        for (tag, name, prov, files) in [
            ("stray", "Voice", PROVENANCE.to_string(), stray),
            ("unknown", "Voice", PROVENANCE.to_string(), unknown),
            ("renamed", "Other", PROVENANCE.to_string(), LINES.to_vec()),
            (
                "key",
                "Voice",
                format!("color = \"red\"\n{PROVENANCE}"),
                LINES.to_vec(),
            ),
            (
                "licence",
                "Voice",
                PROVENANCE.replace("CC0-1.0", "CC-BY-4.0"),
                LINES.to_vec(),
            ),
        ] {
            let root = library(tag, name, &prov, &files);
            let result = scan(&root);
            fs::remove_dir_all(&root).unwrap();
            assert!(result.is_err(), "{tag}");
        }
    }
}
