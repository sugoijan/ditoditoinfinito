//! `DDI_NOTESKINS=<dir>[:<dir>…] cargo test -p ddi-noteskin --test local_skins -- --nocapture`
//! loads every dance skin under each folder (a `NoteSkins` tree, or any
//! folder holding skins; third-party skins stay local) and resolves every
//! element the engine and the fallbacks' Lua ask for, for all six buttons.
//! Each folder is its own library: its `default` and `common` skins, when
//! present, are the fallbacks instead of the stand-ins (with
//! `DDI_NOTESKINS_STANDINS=1` they are left out, as when a skin is imported
//! on its own). Prints, per skin,
//! how many elements are the skin's images, the stand-ins' (built in), Lua,
//! models, other files, blank or missing.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use ddi_noteskin::build::{Built, Images, Textures, build};
use ddi_noteskin::load::{ENGINE_ELEMENTS, EXPLOSION_PARTS, RECEPTOR_PARTS};
use ddi_noteskin::{Button, Dir, Folder, Resolved, Skin, Source};

/// Every image read as 128 px square: enough to build the actors.
struct AnySize;

impl Images for AnySize {
    fn size(&self, _dir: Dir, _path: &str) -> Option<(u32, u32)> {
        Some((128, 128))
    }

    fn max_size(&self) -> u32 {
        8192
    }
}

/// Skin folders: those holding a `metrics.ini` or a `NoteSkin.lua`.
fn skins(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    let mut subdirs = Vec::new();
    let mut is_skin = false;
    for e in entries.flatten() {
        let p = e.path();
        if p.is_dir() {
            subdirs.push(p);
        } else if let Some(n) = p.file_name().and_then(|n| n.to_str()) {
            is_skin |=
                n.eq_ignore_ascii_case("metrics.ini") || n.eq_ignore_ascii_case("NoteSkin.lua");
        }
    }
    if is_skin {
        out.push(dir.to_path_buf());
    }
    subdirs.sort();
    for s in subdirs {
        // Skins inside skins (`variants/`) are not separate here.
        if !is_skin {
            skins(&s, out);
        }
    }
}

fn files(root: &Path, dir: &Path, out: &mut Vec<String>) {
    for e in std::fs::read_dir(dir).into_iter().flatten().flatten() {
        let p = e.path();
        if p.is_dir() {
            files(root, &p, out);
        } else if let Ok(rel) = p.strip_prefix(root) {
            out.push(rel.to_string_lossy().replace('\\', "/"));
        }
    }
}

fn folder(path: &Path) -> Folder {
    let mut list = Vec::new();
    files(path, path, &mut list);
    list.sort();
    let texts = list
        .iter()
        .filter(|p| {
            let l = p.to_ascii_lowercase();
            l == "metrics.ini" || l.ends_with(".redir")
        })
        .filter_map(|p| {
            let bytes = std::fs::read(path.join(p)).ok()?;
            Some((p.clone(), String::from_utf8_lossy(&bytes).into_owned()))
        })
        .collect();
    let parent = path
        .parent()
        .and_then(|p| p.file_name())
        .map(|n| n.to_string_lossy().to_ascii_lowercase())
        .unwrap_or_default();
    // A skin's game is its parent folder in a `NoteSkins` tree; OutFox's
    // `global` skins are tried as dance skins, and a folder anywhere else
    // is taken to hold dance skins.
    let game = match parent.as_str() {
        "common" => "common",
        "dance" | "global" => "dance",
        g if GAMES.contains(&g) => g,
        _ => "dance",
    };
    Folder {
        game: game.into(),
        name: path.file_name().unwrap().to_string_lossy().into_owned(),
        files: list,
        texts,
    }
}

fn kind(r: &Resolved) -> &'static str {
    match r {
        Resolved::Blank | Resolved::NotASprite(_) => "blank",
        Resolved::Missing(_) => "missing",
        Resolved::File { source, .. } => match source {
            Source::Image(_) => "image",
            Source::BuiltInArt(_) | Source::Native(..) => "builtin",
            Source::Lua(_) => "lua",
            Source::Model(_) => "model",
            Source::Movie(_) | Source::Other(_) => "other",
        },
    }
}

/// StepMania's and OutFox's other game folders.
const GAMES: &[&str] = &[
    "pump",
    "kb7",
    "ez2",
    "para",
    "ds3ddx",
    "beat",
    "maniax",
    "techno",
    "popn",
    "lights",
    "kickbox",
    "taiko",
    "gh",
    "gddm",
    "gdgf",
    "be-mu",
    "po-mu",
    "smx",
    "stepstage",
    "kbx",
];

const KINDS: [&str; 7] = [
    "image", "builtin", "lua", "model", "other", "blank", "missing",
];

#[test]
fn local_skins() {
    let Some(dirs) = std::env::var_os("DDI_NOTESKINS") else {
        return;
    };
    let mut totals: BTreeMap<&str, usize> = BTreeMap::new();
    let (mut skins_seen, mut loaded, mut core_images) = (0, 0, 0);
    for root in std::env::split_paths(&dirs) {
        let mut paths = Vec::new();
        skins(&root, &mut paths);
        if std::env::var_os("DDI_NOTESKINS_STANDINS").is_some() {
            paths.retain(|p| {
                let f = folder(p);
                f.game != "common" && !f.name.eq_ignore_ascii_case("default")
            });
        }
        let folders: Vec<Folder> = paths.iter().map(|p| folder(p)).collect();
        for (path, f) in paths.iter().zip(&folders) {
            if f.game != "dance" || f.name.eq_ignore_ascii_case("default") && folders.len() > 1 {
                // Fallbacks, not skins of their own here.
                continue;
            }
            skins_seen += 1;
            let skin = match Skin::load(&folders, &f.game, &f.name) {
                Ok(s) => s,
                Err(e) => {
                    println!("{}\tnot loaded: {e}", path.display());
                    continue;
                }
            };
            loaded += 1;
            let mut counts: BTreeMap<&str, usize> = BTreeMap::new();
            let mut core = true;
            for b in Button::ALL {
                for e in ENGINE_ELEMENTS
                    .iter()
                    .chain(EXPLOSION_PARTS)
                    .chain(RECEPTOR_PARTS)
                {
                    let r = skin.resolve(b, e);
                    let k = kind(&r);
                    *counts.entry(k).or_default() += 1;
                    *totals.entry(k).or_default() += 1;
                    if matches!(*e, "Tap Note" | "Receptor") && k != "image" {
                        core = false;
                    }
                }
            }
            core_images += usize::from(core);
            // The explosion row and the mine, with their commands.
            let mut textures = Textures::default();
            let mut lua_commands = 0;
            for e in ["Explosion", "Tap Mine"] {
                let built = build(&skin, Button::Down, e, &mut textures, &AnySize);
                lua_commands += built
                    .notes
                    .iter()
                    .filter(|n| n.contains("needs Lua"))
                    .count();
                if matches!(built.built, Built::Actor(_)) {
                    *totals.entry("actors").or_default() += 1;
                }
            }
            *totals.entry("lua commands").or_default() += lua_commands;
            let row: Vec<String> = KINDS
                .iter()
                .map(|k| format!("{k} {}", counts.get(k).copied().unwrap_or(0)))
                .collect();
            let chain: Vec<String> = skin
                .chain
                .iter()
                .map(|d| match skin.folder(*d) {
                    Some(f) => f.name.clone(),
                    None => format!("{d:?}"),
                })
                .collect();
            println!(
                "{}\t{}\tchain {}\t{}{}",
                path.display(),
                row.join(", "),
                chain.join(" > "),
                if core {
                    "taps and receptor are images"
                } else {
                    ""
                },
                if skin.warnings().is_empty() {
                    String::new()
                } else {
                    format!("\t{} warnings", skin.warnings().len())
                }
            );
        }
    }
    let row: Vec<String> = KINDS
        .iter()
        .map(|k| format!("{k} {}", totals.get(k).copied().unwrap_or(0)))
        .collect();
    println!(
        "{loaded} of {skins_seen} skins loaded; {core_images} with image taps and receptors; elements: {}; explosion rows and mines built as actors: {}, metrics commands needing Lua: {}",
        row.join(", "),
        totals.get("actors").copied().unwrap_or(0),
        totals.get("lua commands").copied().unwrap_or(0)
    );
}
