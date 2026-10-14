//! The skins bundled with the game (`assets/noteskins/<game>/<name>/`, see
//! `docs/plans/bundled-skins.md`) load through their Lua with every
//! element the engine asks for, for all six dance buttons and the button
//! a Dancing☆Onigiri onigiri lane takes (`docs/plans/danoni-skins.md`),
//! without a warning, and their taps and receptors are their own (all of
//! Dito's, Alpha-Arrows', Dake's and Peter Lawrence's CF skins' elements
//! are).

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
        // The onigiri: Dito's own, the CF skins' centre, the others' Down.
        let seven = ddi_chart::Layout::builtin("danoni-7").unwrap();
        let onigiri = seven
            .lanes
            .iter()
            .find(|l| l.glyph == ddi_chart::Glyph::Onigiri)
            .and_then(|l| skin.lane_button(l))
            .unwrap()
            .button;
        let expected = if f.name.starts_with("Dito") {
            Button::Onigiri
        } else if f.name.starts_with("CF_") {
            Button::Center
        } else {
            Button::Down
        };
        assert_eq!(onigiri, expected, "{}", f.name);
        // A pump pad's centre: Dito's and the CF skins' own, the others'
        // Down.
        let centre = ddi_chart::Layout::pump_single().lanes[2].clone();
        let centre = skin.lane_button(&centre).unwrap().button;
        let expected = if f.name.starts_with("Dito") || f.name.starts_with("CF_") {
            Button::Center
        } else {
            Button::Down
        };
        assert_eq!(centre, expected, "{}", f.name);
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
        for b in Button::ALL.into_iter().chain([onigiri, centre]) {
            for e in ENGINE_ELEMENTS {
                let el = lua
                    .element(b, e, &mut textures, &AnySize)
                    .unwrap_or_else(|err| panic!("{} {b:?} {e}: {err}", f.name));
                // Dito, Alpha-Arrows, Dake and the CF skins draw everything
                // themselves.
                if f.name.starts_with("Dito")
                    || f.name.starts_with("CF_")
                    || ["Alpha-Arrows", "Dake"].contains(&f.name.as_str())
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
        // Dito's onigiri bursts in its star's outline, its centre in its
        // square's.
        if f.name.starts_with("Dito") {
            for (button, ring) in [
                (Button::Onigiri, "_onigiri receptor.svg"),
                (Button::Center, "_center receptor.svg"),
            ] {
                let mut symbol = Textures::default();
                let el = lua
                    .element(button, "Explosion", &mut symbol, &AnySize)
                    .unwrap();
                assert!(matches!(el.built, Built::Actor(_)));
                assert!(
                    symbol.keys().iter().any(|k| matches!(k,
                        ddi_noteskin::build::TextureKey::Skin { path, .. } if path == ring)),
                    "{}: {:?}",
                    f.name,
                    symbol.keys()
                );
                let mut tap = Textures::default();
                lua.element(button, "Tap Note", &mut tap, &AnySize).unwrap();
                let prefix = &ring[..ring.find(' ').unwrap()];
                assert!(
                    tap.keys().iter().any(|k| matches!(k,
                        ddi_noteskin::build::TextureKey::Skin { path, .. } if path.starts_with(prefix))),
                    "{}: {:?}",
                    f.name,
                    tap.keys()
                );
            }
        }
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

#[test]
fn bundled_skins_draw_every_layout_through_their_lua() {
    // Every built-in layout's lanes as the play builds them (the first of
    // a lane's choices whose tap note and receptor build), with the style
    // the skin is told (Dake reads the game and the style: a dance skin is
    // told `dance` on pump layouts too).
    let folders = bundled();
    for f in folders.iter().filter(|f| f.game == "dance") {
        for id in ddi_chart::Layout::builtin_ids() {
            let layout = ddi_chart::Layout::builtin(&id).unwrap();
            let skin = Skin::load(&folders, &f.game, &f.name).unwrap();
            let choices: Vec<_> = layout.lanes.iter().map(|l| skin.lane_buttons(l)).collect();
            let start = std::time::Instant::now();
            let mut lua = LuaSkin::new(
                skin.into_owned(),
                Env {
                    style: ddi_noteskin::lua::dance_style(&layout).into(),
                    columns: layout.lane_count() as u32,
                    ..Env::default()
                },
                ddi_lua::Settings::default(),
                LoadBudget::default(),
                Box::new(move || start.elapsed().as_secs_f64()),
            )
            .unwrap_or_else(|e| panic!("{} {id}: {e}", f.name));
            let mut textures = Textures::default();
            for (lane, choices) in layout.lanes.iter().zip(choices) {
                let built = choices.iter().find(|c| {
                    ["Tap Note", "Receptor"].iter().all(|e| {
                        lua.element(c.button, e, &mut textures, &AnySize)
                            .is_ok_and(|el| {
                                matches!(el.built, Built::Actor(_)) && el.notes.is_empty()
                            })
                    })
                });
                assert!(
                    built.is_some(),
                    "{} {id} {}: {choices:?}",
                    f.name,
                    lane.name
                );
            }
            assert_eq!(lua.warnings(), Vec::<String>::new(), "{} {id}", f.name);
        }
    }
}
