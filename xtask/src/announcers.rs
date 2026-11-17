//! `gen-announcers`: check the bundled announcers (`assets/announcers/<name>/`,
//! ours, which `tools/announcer-design/kokoro.py pack --bundle` makes, each
//! with a `PROVENANCE.toml`) and write `target/announcers/index.json`, the
//! index the web fetches: every announcer's lines, their sounds and each
//! sound's hash. Desktop reads the folders themselves.

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};
use ddi_library::announcer::{Announcer, Bundled, Index, Kept, find_announcers, pool};
use serde::Deserialize;

use crate::songs::LicenseEntry;

use crate::repo_root;

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
    /// What each sound says, by its path in the folder: every sound's,
    /// or none (the announcer studio's examples).
    #[serde(default)]
    pub(crate) said: BTreeMap<String, String>,
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

/// A bundled announcer as found: its folder's name, each voice's
/// provenance (by its language's tag; `""`: a pack of one voice, its
/// PROVENANCE.toml beside its line folders), what was found in it and
/// its `announcer.ini`'s text.
pub(crate) struct Pack {
    pub(crate) path: String,
    pub(crate) voices: Vec<(String, Provenance)>,
    pub(crate) announcer: Announcer,
    pub(crate) ini: Option<String>,
}

/// A PROVENANCE.toml read and checked: its licences known.
fn provenance(root: &Path, path: &Path, name: &str) -> Result<Provenance> {
    let prov: Provenance = toml::from_str(
        &fs::read_to_string(path).with_context(|| format!("reading {}", path.display()))?,
    )
    .with_context(|| format!("parsing {}", path.display()))?;
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
    Ok(prov)
}

/// Every bundled announcer. Each must be one announcer of known lines
/// and their sounds alone: a pack of one voice beside its
/// PROVENANCE.toml, or a declared pack (`announcer.ini`) whose every
/// declared language is a folder of its own with its PROVENANCE.toml,
/// named there as the declaration names it, every voice with the same
/// lines.
pub(crate) fn scan(root: &Path) -> Result<Vec<Pack>> {
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
        let ini = fs::read_to_string(folder.join(pool::DECLARATION)).ok();
        let declared = ini.as_deref().map(pool::Declaration::parse);
        let mut voices = Vec::new();
        match &declared {
            Some(d) => {
                if !d.warnings.is_empty() {
                    bail!("{name}: announcer.ini: {}", d.warnings.join("; "));
                }
                if d.languages.is_empty() {
                    bail!("{name}: announcer.ini declares no language");
                }
                for lang in &d.languages {
                    let prov = provenance(
                        root,
                        &folder.join(lang).join("PROVENANCE.toml"),
                        &format!("{name}/{lang}"),
                    )?;
                    let named = d.name(lang).unwrap_or(&name);
                    if prov.name != named {
                        bail!("{name}/{lang}: PROVENANCE.toml names it {:?}, announcer.ini {named:?}", prov.name);
                    }
                    voices.push((lang.clone(), prov));
                }
            }
            None => {
                let prov = provenance(root, &folder.join("PROVENANCE.toml"), &name)?;
                if prov.name != name {
                    bail!("{name}: PROVENANCE.toml names it {:?}", prov.name);
                }
                voices.push((String::new(), prov));
            }
        }
        let mut files = Vec::new();
        crate::noteskins::walk(&folder, "", &mut files)?;
        files.retain(|f| !f.ends_with("PROVENANCE.toml"));
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
        let kept = a.kept();
        if declared.is_some() {
            if !kept.lines.is_empty() {
                bail!("{name}: line folders outside its languages' folders");
            }
            let langs: Vec<&String> = kept.voices.keys().collect();
            let mut wanted: Vec<&String> = voices.iter().map(|(l, _)| l).collect();
            wanted.sort();
            if langs != wanted {
                bail!("{name}: its language folders are not those it declares");
            }
            let first = kept.voices.values().next().map(|v| v.keys().collect::<Vec<_>>());
            for (lang, lines) in &kept.voices {
                if Some(lines.keys().collect::<Vec<_>>()) != first {
                    bail!("{name}/{lang}: not the same lines as the other voices");
                }
            }
        }
        for (lang, prov) in &voices {
            if prov.said.is_empty() {
                continue;
            }
            let sounds: std::collections::BTreeSet<String> = if lang.is_empty() {
                kept.lines.values().flatten().map(ToString::to_string).collect()
            } else {
                kept.voices[lang]
                    .values()
                    .flatten()
                    .map(|f| f.as_str().strip_prefix(&format!("{lang}/")).unwrap_or(f.as_str()).to_string())
                    .collect()
            };
            let told: std::collections::BTreeSet<String> = prov.said.keys().cloned().collect();
            if let Some(f) = sounds.symmetric_difference(&told).next() {
                bail!("{name}: [said] and the sounds differ at {f}");
            }
        }
        out.push(Pack {
            path: name,
            voices,
            announcer: a,
            ini,
        });
    }
    Ok(out)
}

/// What the shells keep of a bundled announcer.
fn kept(pack: &Pack) -> Kept {
    Kept {
        bundled: true,
        ini: pack.ini.clone(),
        ..pack.announcer.kept()
    }
}

pub(crate) fn gen_announcers(check: bool) -> Result<()> {
    let root = repo_root();
    let dir = root.join("assets/announcers");
    let index = Index {
        announcers: scan(&root)?
            .into_iter()
            .map(|pack| {
                let kept = kept(&pack);
                let path = pack.path.clone();
                let hashes = kept
                    .sounds()
                    .into_iter()
                    .map(|f| {
                        let file = f
                            .to_path(dir.join(&path))
                            .with_context(|| format!("{path}/{f}"))?;
                        let bytes = fs::read(file)?;
                        Ok((f.to_string(), format!("{:016x}", ddi_library::pack::fnv1a64(&bytes))))
                    })
                    .collect::<Result<_>>()?;
                // What each sound says, by its path in the pack.
                let said = pack
                    .voices
                    .into_iter()
                    .flat_map(|(lang, prov)| {
                        prov.said.into_iter().map(move |(f, text)| {
                            let at = if lang.is_empty() { f } else { format!("{lang}/{f}") };
                            (at, text)
                        })
                    })
                    .collect();
                Ok(Bundled {
                    path,
                    kept,
                    hashes,
                    said,
                })
            })
            .collect::<Result<_>>()?,
    };
    let json = serde_json::to_string(&index)? + "\n";
    let out = root.join("target/announcers/index.json");
    let name = out.display().to_string();
    if crate::generated(&out, &json, check, &name, "gen-announcers")? {
        println!("wrote {name} ({} announcers)", index.announcers.len());
    }
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
        let pack = &found[0];
        let prov = &pack.voices[0].1;
        assert_eq!((pack.path.as_str(), prov.author.as_str()), ("Voice", "a"));
        let kept = kept(pack);
        assert_eq!(kept.key(), "bundled:voice");
        assert_eq!(kept.files, 4);
        assert!(prov.said.is_empty());
        let ready: Vec<String> = kept.lines["gameplay ready"]
            .iter()
            .map(ToString::to_string)
            .collect();
        assert_eq!(
            ready,
            ["gameplay ready/get-ready.mp3", "gameplay ready/ready.mp3"]
        );
    }

    /// A declared pack of two voices (the built-in's shape): one entry,
    /// its sounds and texts by their paths in it; a voice short of a line,
    /// or misnamed, refused.
    #[test]
    fn a_declared_pack_lists_its_voices() {
        const INI: &str = "[Announcer]\nName=Voice\nName.ja=声\n[Parameters]\nList=Language\n\
            [Parameter Language]\nType=Language\nLanguages=en,ja\n";
        let make = |tag: &str, ja_name: &str, ja_lines: &[&str]| -> PathBuf {
            let root = library(tag, "Two", PROVENANCE, &[]);
            let dir = root.join("assets/announcers/Two");
            fs::remove_file(dir.join("PROVENANCE.toml")).unwrap();
            fs::write(dir.join("announcer.ini"), INI).unwrap();
            for (lang, name, lines) in [("en", "Voice", &LINES[..]), ("ja", ja_name, ja_lines)] {
                let v = dir.join(lang);
                fs::create_dir_all(&v).unwrap();
                fs::write(
                    v.join("PROVENANCE.toml"),
                    PROVENANCE.replace("name = \"Voice\"", &format!("name = \"{name}\"")),
                )
                .unwrap();
                for f in lines {
                    let p = v.join(f);
                    fs::create_dir_all(p.parent().unwrap()).unwrap();
                    fs::write(p, f.as_bytes()).unwrap();
                }
            }
            root
        };
        let root = make("declared", "声", &LINES);
        let found = scan(&root);
        fs::remove_dir_all(&root).unwrap();
        let found = found.unwrap();
        let pack = &found[0];
        let langs: Vec<&str> = pack.voices.iter().map(|(l, _)| l.as_str()).collect();
        assert_eq!(langs, ["en", "ja"]);
        let kept = kept(pack);
        assert_eq!(kept.key(), "bundled:two");
        assert_eq!(kept.files, 8);
        assert!(kept.lines.is_empty() && kept.ini.is_some());
        assert_eq!(kept.display_name("ja"), "声");
        for (tag, name, lines) in [("short", "声", &LINES[..3]), ("misnamed", "Voice", &LINES[..])] {
            let root = make(tag, name, lines);
            let result = scan(&root);
            fs::remove_dir_all(&root).unwrap();
            assert!(result.is_err(), "{tag}");
        }
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
            (
                "said",
                "Voice",
                format!("{PROVENANCE}\n[said]\n\"gameplay ready/ready.mp3\" = \"Ready?\"\n"),
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
