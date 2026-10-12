//! The skins bundled with the game (`assets/noteskins/<game>/<name>/`, see
//! `docs/plans/bundled-skins.md`) load through their Lua with every
//! element the engine asks for, for all six dance buttons, without a
//! warning, and their taps and receptors are their own (all of Dito's,
//! Alpha-Arrows' and Dake's elements are).

use std::path::Path;

use ddi_noteskin::build::{Built, Images, Textures};
use ddi_noteskin::load::ENGINE_ELEMENTS;
use ddi_noteskin::lua::{Env, LoadBudget, LuaSkin};
use ddi_noteskin::{Button, Dir, Folder, Skin};

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

/// The bundled skins, as `xtask gen-noteskins` indexes them.
fn bundled() -> Vec<Folder> {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../assets/noteskins");
    let mut out = Vec::new();
    let mut games: Vec<_> = std::fs::read_dir(&root).unwrap().flatten().collect();
    games.sort_by_key(|e| e.file_name());
    for game in games {
        let mut skins: Vec<_> = std::fs::read_dir(game.path()).unwrap().flatten().collect();
        skins.sort_by_key(|e| e.file_name());
        for skin in skins {
            let path = skin.path();
            let mut list = Vec::new();
            files(&path, &path, &mut list);
            list.retain(|f| f != "PROVENANCE.toml" && !f.split('/').any(|p| p.starts_with('.')));
            list.sort();
            let texts = list
                .iter()
                .filter(|p| ddi_noteskin::import::kept_text(p))
                .map(|p| {
                    let bytes = std::fs::read(path.join(p)).unwrap();
                    (p.clone(), String::from_utf8_lossy(&bytes).into_owned())
                })
                .collect();
            out.push(Folder {
                game: game.file_name().to_string_lossy().into_owned(),
                name: skin.file_name().to_string_lossy().into_owned(),
                files: list,
                texts,
            });
        }
    }
    out
}

#[test]
fn bundled_skins_build_every_element_through_their_lua() {
    let folders = bundled();
    assert!(!folders.is_empty());
    for f in folders.iter().filter(|f| f.game == "dance") {
        let skin = Skin::load(&folders, &f.game, &f.name).unwrap();
        let start = std::time::Instant::now();
        let mut lua = LuaSkin::new(
            skin.into_owned(),
            Env::default(),
            ddi_lua::Settings::default(),
            LoadBudget::default(),
            Box::new(move || start.elapsed().as_secs_f64()),
        )
        .unwrap_or_else(|e| panic!("{}: {e}", f.name));
        let mut textures = Textures::default();
        for b in Button::ALL {
            for e in ENGINE_ELEMENTS {
                let el = lua
                    .element(b, e, &mut textures, &AnySize)
                    .unwrap_or_else(|err| panic!("{} {b:?} {e}: {err}", f.name));
                // Dito, Alpha-Arrows and Dake draw everything themselves.
                if f.name.starts_with("Dito") || ["Alpha-Arrows", "Dake"].contains(&f.name.as_str())
                {
                    assert!(
                        !matches!(el.built, Built::BuiltIn) && el.notes.is_empty(),
                        "{} {b:?} {e}: {:?}",
                        f.name,
                        el.notes
                    );
                }
                if ["Tap Note", "Receptor"].contains(e) {
                    assert!(
                        matches!(el.built, Built::Actor(_)) && el.notes.is_empty(),
                        "{} {b:?} {e}: {:?}",
                        f.name,
                        el.notes
                    );
                }
            }
        }
        assert_eq!(lua.warnings(), Vec::<String>::new(), "{}", f.name);
        // Dito's lifts and fakes are drawn from sheets of their own.
        if f.name.starts_with("Dito") {
            let paths: Vec<String> = textures
                .keys()
                .iter()
                .filter_map(|k| match k {
                    ddi_noteskin::build::TextureKey::Skin { path, .. } => Some(path.to_lowercase()),
                    _ => None,
                })
                .collect();
            for part in ["tap note", "tap lift", "tap fake"] {
                assert!(
                    paths.iter().any(|p| p.contains(part)),
                    "{}: {part} {paths:?}",
                    f.name
                );
            }
        }
    }
}
