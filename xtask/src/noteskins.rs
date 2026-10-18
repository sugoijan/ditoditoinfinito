//! `gen-noteskins`: check the bundled skins (`assets/noteskins/<game>/<name>/`,
//! each with a `PROVENANCE.toml`) and write `target/noteskins/index.json`,
//! the index the app fetches: every skin's files and the texts of its
//! `metrics.ini`, redirects and Lua.

use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};
use ddi_noteskin::bundled::{Bundled, Index};
use ddi_noteskin::import::{kept_game, kept_text};
use ddi_noteskin::{Folder, Skin};
use serde::Deserialize;

use crate::songs::{LicenseEntry, Source};

fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("xtask lives in the workspace root")
        .to_path_buf()
}

/// `assets/noteskins/<game>/<name>/PROVENANCE.toml`.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Provenance {
    /// Who made the skin.
    pub(crate) author: String,
    /// One line for the Options list, e.g. "shakesoda · CC BY 3.0".
    pub(crate) credit: String,
    /// This repository's own (MIT, like the code): not in the credits.
    #[serde(default)]
    pub(crate) ours: bool,
    /// The skin this one is a variant of (a color scheme of it): not
    /// listed under Options on its own.
    #[serde(default)]
    pub(crate) variant_of: Option<String>,
    #[serde(default)]
    pub(crate) excluded: Vec<String>,
    #[serde(default)]
    pub(crate) modifications: Vec<String>,
    pub(crate) source: Source,
    pub(crate) licenses: Vec<LicenseEntry>,
}

/// The largest text kept with a skin (as for imported skins).
const MAX_TEXT: u64 = 1 << 20;

/// Every bundled skin: its path under `assets/noteskins/`, provenance and
/// folder.
pub(crate) fn scan(root: &Path) -> Result<Vec<(String, Provenance, Folder)>> {
    let dir = root.join("assets/noteskins");
    let mut out = Vec::new();
    if !dir.is_dir() {
        return Ok(out);
    }
    for game in sorted_dirs(&dir)? {
        let game_name = name_of(&game);
        if !kept_game(&game_name) {
            bail!("assets/noteskins/{game_name}: not a game whose skins are kept");
        }
        for skin in sorted_dirs(&game)? {
            let name = name_of(&skin);
            let path = format!("{game_name}/{name}");
            if name.eq_ignore_ascii_case("default") || name.eq_ignore_ascii_case("common") {
                bail!("{path}: would hide StepMania's stand-in of that name");
            }
            let prov_path = skin.join("PROVENANCE.toml");
            let prov: Provenance = toml::from_str(
                &fs::read_to_string(&prov_path)
                    .with_context(|| format!("reading {}", prov_path.display()))?,
            )
            .with_context(|| format!("parsing {}", prov_path.display()))?;
            if prov.licenses.is_empty() {
                bail!("{path}: PROVENANCE.toml has no [[licenses]] entries");
            }
            let mut files = Vec::new();
            walk(&skin, "", &mut files)?;
            files.retain(|f| f != "PROVENANCE.toml");
            files.sort();
            let mut texts = Vec::new();
            for f in files.iter().filter(|f| kept_text(f)) {
                let p = skin.join(f);
                if fs::metadata(&p)?.len() > MAX_TEXT {
                    bail!("{path}/{f}: larger than {MAX_TEXT} bytes");
                }
                let bytes = fs::read(&p)?;
                texts.push((f.clone(), String::from_utf8_lossy(&bytes).into_owned()));
            }
            out.push((
                path,
                prov,
                Folder {
                    game: game_name.clone(),
                    name,
                    files,
                    texts,
                },
            ));
        }
    }
    Ok(out)
}

fn sorted_dirs(dir: &Path) -> Result<Vec<PathBuf>> {
    let mut dirs: Vec<PathBuf> = fs::read_dir(dir)?
        .filter_map(|e| e.ok())
        .map(|e| e.path())
        .filter(|p| p.is_dir())
        .collect();
    dirs.sort();
    Ok(dirs)
}

fn name_of(path: &Path) -> String {
    path.file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default()
}

/// Every file under `dir`, `/`-separated relative to it.
fn walk(dir: &Path, prefix: &str, out: &mut Vec<String>) -> Result<()> {
    for entry in fs::read_dir(dir)? {
        let entry = entry?;
        let name = entry.file_name().to_string_lossy().into_owned();
        let kind = entry.file_type()?;
        // Hidden files (`.DS_Store`, editors') are not the skin's.
        if name.starts_with('.') {
            continue;
        }
        if kind.is_symlink() {
            bail!("{}: a link; bundle the file itself", entry.path().display());
        }
        let rel = format!("{prefix}{name}");
        if kind.is_dir() {
            walk(&entry.path(), &format!("{rel}/"), out)?;
        } else {
            out.push(rel);
        }
    }
    Ok(())
}

pub(crate) fn gen_noteskins(check: bool) -> Result<()> {
    let root = repo_root();
    let skins = scan(&root)?;
    let folders: Vec<Folder> = skins.iter().map(|(_, _, f)| f.clone()).collect();
    for (path, prov, f) in &skins {
        Skin::load(&folders, &f.game, &f.name).map_err(|e| anyhow::anyhow!("{path}: {e}"))?;
        if let Some(of) = &prov.variant_of
            && !skins
                .iter()
                .any(|(_, _, g)| g.game == f.game && &g.name == of)
        {
            bail!("{path}: a variant of {of}, which is not bundled");
        }
    }
    let index = Index {
        skins: skins
            .into_iter()
            .map(|(path, prov, folder)| Bundled {
                path,
                folder,
                credit: prov.credit,
                variant_of: prov.variant_of,
            })
            .collect(),
    };
    let json = serde_json::to_string(&index)? + "\n";
    let out_dir = root.join("target/noteskins");
    let out = out_dir.join("index.json");
    if check {
        let current = fs::read_to_string(&out).unwrap_or_default();
        if current != json {
            bail!(
                "{} is stale; run `cargo run -p xtask -- gen-noteskins`",
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
    println!("wrote {} ({} skins)", out.display(), index.skins.len());
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    const PROVENANCE: &str = "author = \"a\"\ncredit = \"a · CC0\"\n\n[source]\nurl = \"https://example.test\"\n\n[[licenses]]\npart = \"all\"\nspdx = \"CC0-1.0\"\nholders = [\"a\"]\n";

    /// A library under a fresh temporary folder with one skin.
    fn library(tag: &str, name: &str, provenance: &str) -> PathBuf {
        let root =
            std::env::temp_dir().join(format!("ddi-gen-noteskins-{tag}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        let skin = root.join("assets/noteskins/dance").join(name);
        fs::create_dir_all(skin.join("sub")).unwrap();
        fs::write(skin.join("PROVENANCE.toml"), provenance).unwrap();
        fs::write(skin.join("metrics.ini"), "[Global]\n").unwrap();
        fs::write(skin.join("Down Tap Note.lua"), "return Def.Actor{}").unwrap();
        fs::write(skin.join("sub/x.png"), [0u8]).unwrap();
        root
    }

    #[test]
    fn skins_list_their_files_and_texts() {
        let root = library("ok", "Mine", PROVENANCE);
        let skins = scan(&root).unwrap();
        fs::remove_dir_all(&root).unwrap();
        let (path, prov, folder) = &skins[0];
        assert_eq!(path, "dance/Mine");
        assert_eq!(prov.credit, "a · CC0");
        assert_eq!(
            folder.files,
            ["Down Tap Note.lua", "metrics.ini", "sub/x.png"]
        );
        let texts: Vec<&str> = folder.texts.iter().map(|(p, _)| p.as_str()).collect();
        assert_eq!(texts, ["Down Tap Note.lua", "metrics.ini"]);
    }

    #[test]
    fn bad_records_and_names_are_refused() {
        for (tag, name, prov) in [
            ("unknown", "Mine", format!("color = \"red\"\n{PROVENANCE}")),
            (
                "nolicence",
                "Mine",
                PROVENANCE.split("[[licenses]]").next().unwrap().to_string(),
            ),
            ("default", "Default", PROVENANCE.to_string()),
        ] {
            let root = library(tag, name, &prov);
            let result = scan(&root);
            fs::remove_dir_all(&root).unwrap();
            assert!(result.is_err(), "{tag}");
        }
    }
}
