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
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../assets/noteskins");
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
fn danoni_default_art_and_lua_behavior() {
    use ddi_actor::{Actor, Clocks};
    use ddi_noteskin::lua::{FrameBudget, Param, Position};

    let folders = bundled();
    let skin = Skin::load(&folders, "dance", "DanOni").unwrap();
    let mut lua = LuaSkin::new(
        skin.into_owned(),
        Env::default(),
        ddi_lua::Settings::default(),
        LoadBudget::default(),
        Box::new(|| 0.0),
    )
    .unwrap();
    let mut textures = Textures::default();
    let mut element = |lua: &mut LuaSkin, button, name| -> Box<Actor> {
        let built = lua.element(button, name, &mut textures, &AnySize).unwrap();
        assert!(
            built.notes.is_empty(),
            "{button:?} {name}: {:?}",
            built.notes
        );
        let Built::Actor(actor) = built.built else {
            panic!("{button:?} {name}")
        };
        let mut actor = Box::new(actor);
        assert!(lua.reclaim(&mut actor));
        actor
    };
    let near = |actual: [f32; 4], expected: [f32; 4]| {
        assert!(
            actual
                .iter()
                .zip(expected)
                .all(|(a, e)| (a - e).abs() < 1e-5),
            "{actual:?} != {expected:?}"
        );
    };
    let command = |lua: &mut LuaSkin, actor: &mut Box<Actor>, name, params: &[(&str, Param)]| {
        lua.begin_frame(Position::default(), FrameBudget::default());
        let id = actor.id;
        assert!(lua.lend(actor));
        lua.play(id, name, params).unwrap();
        assert!(lua.reclaim(actor));
    };
    let update = |lua: &mut LuaSkin, actor: &mut Box<Actor>, seconds| {
        lua.begin_frame(Position::default(), FrameBudget::default());
        let id = actor.id;
        assert!(lua.lend(actor));
        lua.update(id, seconds, &Clocks::default()).unwrap();
        assert!(lua.reclaim(actor));
    };
    for (button, rgb, rotation) in [
        (Button::Left, [0.4, 0.4, 1.0, 1.0], 0.0),
        (Button::Down, [0.4, 0.4, 1.0, 1.0], -90.0),
        (Button::Up, [0.4, 0.4, 1.0, 1.0], 90.0),
        (Button::Right, [0.4, 0.4, 1.0, 1.0], 180.0),
        (Button::DownLeft, [0.6, 1.0, 1.0, 1.0], -45.0),
        (Button::DownRight, [0.6, 1.0, 1.0, 1.0], -135.0),
        (Button::Onigiri, [1.0; 4], 0.0),
    ] {
        let tap = element(&mut lua, button, "Tap Note");
        near(tap.children()[0].current().diffuse(), rgb);
        assert_eq!(tap.base_rotation[2], rotation);
        // Dedicated diagonal buttons also cover freeze tails without the
        // Down fallback's extra turn (which does not turn tails).
        let tail = element(&mut lua, button, "Hold Tail Inactive");
        assert_eq!(tail.base_rotation[2], rotation);
        near(tail.children()[1].current().diffuse(), [0.4, 1.0, 1.0, 1.0]);
        for name in ENGINE_ELEMENTS {
            let built = lua
                .element(button, name, &mut Textures::default(), &AnySize)
                .unwrap();
            assert!(
                !matches!(built.built, Built::BuiltIn) && built.notes.is_empty(),
                "{button:?} {name}: {:?}",
                built.notes
            );
        }
    }
    for button in [Button::Down, Button::Onigiri] {
        let mut receptor = element(&mut lua, button, "Receptor");
        near(
            receptor.children()[0].current().diffuse(),
            [0.6, 0.6, 0.6, 1.0],
        );
        assert!(!receptor.children()[1].visible);
        command(&mut lua, &mut receptor, "Press", &[]);
        assert!(receptor.children()[1].visible);
        near(
            receptor.children()[1].current().diffuse(),
            [0.4, 1.0, 1.0, 1.0],
        );
        command(&mut lua, &mut receptor, "W3", &[]);
        near(
            receptor.children()[0].current().diffuse(),
            [0.6, 0.6, 0.6, 1.0],
        );
        command(&mut lua, &mut receptor, "Lift", &[]);
        assert!(!receptor.children()[1].visible);
        assert_eq!(receptor.current().scale, [1.0; 3]);

        let mut explosion = element(&mut lua, button, "Explosion");
        assert!(!explosion.children()[0].visible);
        for (score, rgb) in [
            ("W1", [0.4, 1.0, 1.0, 0.75]),
            ("W2", [0.4, 1.0, 1.0, 0.75]),
            ("W3", [0.6, 1.0, 0.6, 0.75]),
            ("W4", [1.0, 0.6, 0.4, 0.75]),
            ("W5", [0.8, 0.8, 1.0, 0.75]),
        ] {
            let params = [
                ("TapNoteScore", Param::Str(format!("TapNoteScore_{score}"))),
                ("Bright", Param::Bool(true)),
            ];
            command(&mut lua, &mut explosion, "ColumnJudgmentMessage", &params);
            near(explosion.children()[1].current().diffuse(), rgb);
            assert_eq!(explosion.children()[1].current().scale[..2], [1.6; 2]);
            update(&mut lua, &mut explosion, 0.04);
            near(explosion.children()[1].current().diffuse(), rgb);
            // A second hit resets the full duration, even while the first sleeps.
            command(&mut lua, &mut explosion, "ColumnJudgmentMessage", &params);
            update(&mut lua, &mut explosion, 0.04);
            near(explosion.children()[1].current().diffuse(), rgb);
            update(&mut lua, &mut explosion, 0.03);
            assert_eq!(explosion.children()[1].current().diffuse()[3], 0.0);
        }
        command(
            &mut lua,
            &mut explosion,
            "ColumnJudgmentMessage",
            &[("TapNoteScore", Param::Str("TapNoteScore_Miss".into()))],
        );
        assert_eq!(explosion.children()[1].current().diffuse()[3], 0.0);
        for (on, off) in [("HoldingOn", "HoldingOff"), ("RollOn", "RollOff")] {
            command(&mut lua, &mut explosion, on, &[]);
            assert!(explosion.children()[0].visible);
            assert_eq!(explosion.children()[0].current().diffuse()[3], 0.9);
            command(&mut lua, &mut explosion, off, &[]);
            assert!(!explosion.children()[0].visible);
        }
        command(&mut lua, &mut explosion, "Held", &[]);
        assert_eq!(explosion.children()[1].current().diffuse()[3], 0.0);
        let head = element(&mut lua, button, "Hold Head Active");
        assert!(!head.children()[1].visible);
        let tail = element(&mut lua, button, "Hold Tail Active");
        near(tail.children()[1].current().diffuse(), [0.8, 0.8, 0.2, 1.0]);
        for part in [
            "Hold Head Dropped",
            "Hold Tail Dropped",
            "Roll Head Dropped",
            "Roll Tail Dropped",
        ] {
            let dropped = element(&mut lua, button, part);
            near(
                dropped.children()[0].current().diffuse(),
                [0.2, 0.2, 0.2, 1.0],
            );
            near(
                dropped.children()[1].current().diffuse(),
                [0.8, 0.8, 0.8, 1.0],
            );
            assert!(dropped.children()[1].visible);
        }
        command(
            &mut lua,
            &mut receptor,
            "Press",
            &[("Autoplay", Param::Bool(true))],
        );
        assert!(!receptor.children()[1].visible);
        let auto = [
            ("TapNoteScore", Param::Str("TapNoteScore_W3".into())),
            ("Autoplay", Param::Bool(true)),
            ("HitX", Param::Float(13.0)),
            ("HitY", Param::Float(-7.0)),
        ];
        command(&mut lua, &mut explosion, "ColumnJudgmentMessage", &auto);
        near(
            explosion.children()[1].current().diffuse(),
            [0.6, 0.6, 0.6, 1.0],
        );
        assert_eq!(explosion.children()[1].current().pos[..2], [0.0, 0.0]);
        update(&mut lua, &mut explosion, 0.04);
        assert_eq!(explosion.children()[1].current().diffuse()[3], 1.0);
        update(&mut lua, &mut explosion, 0.03);
        assert_eq!(explosion.children()[1].current().diffuse()[3], 0.0);
        let manual = [
            ("TapNoteScore", Param::Str("TapNoteScore_W3".into())),
            ("Autoplay", Param::Bool(false)),
            ("HitX", Param::Float(13.0)),
            ("HitY", Param::Float(-7.0)),
        ];
        command(&mut lua, &mut explosion, "ColumnJudgmentMessage", &manual);
        assert_eq!(explosion.children()[1].current().pos[..2], [13.0, -7.0]);
        for (score, rgb) in [
            ("W1", [0.4, 1.0, 1.0, 0.75]),
            ("W2", [0.6, 1.0, 0.6, 0.75]),
            ("W3", [1.0, 0.6, 0.4, 0.75]),
            ("W4", [0.8, 0.8, 1.0, 0.75]),
        ] {
            command(
                &mut lua,
                &mut explosion,
                "ColumnJudgmentMessage",
                &[
                    ("TapNoteScore", Param::Str(format!("TapNoteScore_{score}"))),
                    ("DanoniJudgments", Param::Bool(true)),
                ],
            );
            near(explosion.children()[1].current().diffuse(), rgb);
        }
    }
    let bodies = [
        ("Hold Body Inactive", "_bar inactive.svg"),
        ("Hold Body Active", "_bar active.svg"),
        ("Hold Body Dropped", "_bar dropped.svg"),
    ]
    .map(|(name, path)| (element(&mut lua, Button::Down, name), path));
    for (body, path) in bodies {
        let ddi_actor::Kind::Sprite(sprite) = &body.kind else {
            panic!("body must be a strip sprite")
        };
        assert!(
            matches!(&textures.keys()[sprite.texture as usize], ddi_noteskin::build::TextureKey::Skin {path: p, ..} if p == path)
        );
    }
    assert!(textures.keys().iter().any(|k| matches!(k, ddi_noteskin::build::TextureKey::Skin {path, ..} if path == "_onigiri shape.svg")));
    assert!(lua.warnings().is_empty(), "{:?}", lua.warnings());
}

#[test]
fn bundled_skins_build_every_element_through_their_lua() {
    let folders = bundled();
    assert!(!folders.is_empty());
    for f in folders.iter().filter(|f| f.game == "dance") {
        let skin = Skin::load(&folders, &f.game, &f.name).unwrap();
        // The onigiri: Dito's own, CF/Peters' center, the others' Down.
        let seven = ddi_chart::Layout::builtin("danoni-7").unwrap();
        let onigiri = seven
            .lanes
            .iter()
            .find(|l| l.glyph == ddi_chart::Glyph::Onigiri)
            .and_then(|l| skin.lane_button(l))
            .unwrap()
            .button;
        let expected = if f.name.starts_with("Dito") || f.name == "DanOni" {
            Button::Onigiri
        } else if f.name.starts_with("CF_") || f.name.starts_with("Peters-DDR") {
            Button::Center
        } else {
            Button::Down
        };
        assert_eq!(onigiri, expected, "{}", f.name);
        // A pump pad's centre: Dito's, CF's and Peters' own, the others'
        // Down.
        let centre = ddi_chart::Layout::pump_single().lanes[2].clone();
        let centre = skin.lane_button(&centre).unwrap().button;
        let expected = if f.name.starts_with("Dito")
            || f.name.starts_with("CF_")
            || f.name.starts_with("Peters-DDR")
        {
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
                    || f.name == "DanOni"
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
fn ditos_parameters_pick_its_colors_skin_and_its_explosions() {
    use ddi_noteskin::bundled::dito;
    use ddi_noteskin::params::{Declared, Value, Values, head};
    let folders: Vec<Folder> = bundled()
        .into_iter()
        .filter(|f| f.game == "dance")
        .collect();
    let values = |pairs: &[(&str, Value)]| -> Values {
        pairs
            .iter()
            .map(|(k, v)| (k.to_string(), v.clone()))
            .collect()
    };
    // The color schemes: the default's skin, each choice's, and Dito
    // itself for Quantized.
    let colors = |c: &str| values(&[(dito::COLORS, Value::Text(c.into()))]);
    assert_eq!(
        head(&folders, "dance", "Dito", &Values::new())
            .unwrap()
            .name,
        "Dito Vivid"
    );
    for (scheme, skin) in [
        ("Vivid", "Dito Vivid"),
        ("Rainbow", "Dito Rainbow"),
        ("Quantized", "Dito"),
        ("Flat", "Dito Flat"),
        ("single", "Dito Single"),
    ] {
        assert_eq!(
            head(&folders, "dance", "Dito", &colors(scheme))
                .unwrap()
                .name,
            skin
        );
    }
    // Every scheme declares what Dito does (merged from its metrics).
    for f in folders.iter().filter(|f| f.name.starts_with("Dito")) {
        let d = Declared::of_skin(&folders, "dance", &f.name);
        assert_eq!(&d, ddi_noteskin::bundled::dito_parameters(), "{}", f.name);
    }
    // The explosion's Lua reads its toggle: off, an empty frame.
    let explosion = |on: bool| {
        let skin = Skin::load(&folders, "dance", "Dito Vivid").unwrap();
        let start = std::time::Instant::now();
        let mut lua = LuaSkin::new(
            skin.into_owned(),
            Env {
                params: values(&[(dito::EXPLOSIONS, Value::Bool(on))]),
                ..Env::default()
            },
            ddi_lua::Settings::default(),
            LoadBudget::default(),
            Box::new(move || start.elapsed().as_secs_f64()),
        )
        .unwrap();
        let el = lua
            .element(
                Button::Down,
                "Explosion",
                &mut Textures::default(),
                &AnySize,
            )
            .unwrap();
        assert!(lua.warnings().is_empty(), "{:?}", lua.warnings());
        match el.built {
            Built::Actor(a) => a.children().len(),
            other => panic!("{other:?}"),
        }
    };
    assert!(explosion(true) > 0);
    assert_eq!(explosion(false), 0);
}

#[test]
fn peters_center_loads_approved_raster_art_and_charge_animation() {
    let folders = bundled();
    for name in ["Peters-DDR", "Peters-DDR-Vivid", "Peters-DDR-Note"] {
        let skin = Skin::load(&folders, "dance", name).unwrap();
        for id in ["danoni-5", "danoni-7", "pump-single"] {
            let layout = ddi_chart::Layout::builtin(id).unwrap();
            let lane = layout
                .lanes
                .iter()
                .find(|l| {
                    matches!(
                        l.glyph,
                        ddi_chart::Glyph::Center | ddi_chart::Glyph::Onigiri
                    )
                })
                .unwrap();
            assert_eq!(skin.lane_button(lane).unwrap().button, Button::Center);
        }
        let mut lua = LuaSkin::new(
            skin.into_owned(),
            Env::default(),
            ddi_lua::Settings::default(),
            LoadBudget::default(),
            Box::new(|| 0.0),
        )
        .unwrap();
        let mut textures = Textures::default();
        for part in ENGINE_ELEMENTS {
            let element = lua
                .element(Button::Center, part, &mut textures, &AnySize)
                .unwrap();
            assert!(
                element.notes.is_empty(),
                "{name} {part}: {:?}",
                element.notes
            );
            if matches!(*part, "Tap Note" | "Tap Fake" | "Tap Lift") {
                let Built::Actor(a) = element.built else {
                    panic!("{name} {part}")
                };
                let ddi_actor::Kind::Sprite(center) = a.kind else {
                    panic!("{name} {part} must use the approved raster center")
                };
                assert_eq!(center.frame_size, [64.0, 64.0]);
                assert_eq!(
                    center.frames.len(),
                    if name == "Peters-DDR-Vivid" { 32 } else { 64 }
                );
                assert_eq!(
                    center.states.len(),
                    if name == "Peters-DDR-Vivid" { 32 } else { 4 }
                );
                assert!(textures.keys().iter().any(|key| matches!(key,
                    ddi_noteskin::build::TextureKey::Skin { path, .. }
                        if path.starts_with(if name == "Peters-DDR-Vivid" {
                            "_center vivid tap"
                        } else {
                            "_center raster tap"
                        })
                )));
            }
        }
        assert!(lua.warnings().is_empty(), "{name}: {:?}", lua.warnings());
    }
}

#[test]
fn peters_bundled_extensions_load_for_every_mine_and_long_note_direction() {
    use ddi_noteskin::metrics::NoteDisplay;
    let folders = bundled();
    for name in ["Peters-DDR", "Peters-DDR-Vivid", "Peters-DDR-Note"] {
        let skin = Skin::load(&folders, "dance", name).unwrap();
        assert!(NoteDisplay::read(&skin, "Center").mine_connect_rows);
        assert_eq!(
            NoteDisplay::read(&skin, "Down").roll_animation_timing_window_ratio,
            0.75
        );
        let mut lua = LuaSkin::new(
            skin.into_owned(),
            Env::default(),
            ddi_lua::Settings::default(),
            LoadBudget::default(),
            Box::new(|| 0.0),
        )
        .unwrap();
        let mut textures = Textures::default();
        for button in Button::ALL.into_iter().chain([
            Button::DownLeft,
            Button::DownRight,
            Button::Center,
            Button::Onigiri,
        ]) {
            for part in [
                "Tap Mine",
                "Hold Head Active",
                "Hold Body Active",
                "Hold Bottomcap Active",
                "Roll Head Active",
                "Roll Body Active",
                "Roll Bottomcap Active",
            ] {
                let element = lua.element(button, part, &mut textures, &AnySize).unwrap();
                assert!(
                    element.notes.is_empty(),
                    "{name} {button:?} {part}: {:?}",
                    element.notes
                );
                let Built::Actor(actor) = element.built else {
                    panic!("{name} {button:?} {part}")
                };
                if part == "Tap Mine" {
                    let children: Vec<_> =
                        actor.children().iter().map(|a| a.name.as_str()).collect();
                    assert_eq!(
                        children,
                        ["MineCore", "LocalElectricity", "MineLink"],
                        "{name} {button:?}"
                    );
                }
            }
        }
        assert!(lua.warnings().is_empty(), "{name}: {:?}", lua.warnings());
    }
}

#[test]
fn peters_parameters_preserve_both_looks_and_control_explosions() {
    use ddi_noteskin::metrics::{NoteDisplay, Part};
    use ddi_noteskin::params::{Declared, Value, Values, head};
    let folders = bundled();
    let declared = Declared::of_skin(&folders, "dance", "Peters-DDR");
    assert!(declared.warnings.is_empty(), "{:?}", declared.warnings);
    assert_eq!(declared.get("Colors").unwrap().label, "Note colors");
    assert_eq!(
        head(&folders, "dance", "Peters-DDR", &Values::new())
            .unwrap()
            .name,
        "Peters-DDR-Vivid"
    );
    // Old names still load their original looks for external fallback chains.
    assert_eq!(
        head(&folders, "dance", "Peters-DDR-Note", &Values::new())
            .unwrap()
            .name,
        "Peters-DDR-Note"
    );
    assert_eq!(
        head(&folders, "dance", "Peters-DDR-Vivid", &Values::new())
            .unwrap()
            .name,
        "Peters-DDR-Vivid"
    );
    for (color, expected, vivid) in [
        ("Vivid", "Peters-DDR-Vivid", true),
        ("Quantized", "Peters-DDR", false),
    ] {
        for on in [true, false] {
            let params = Values::from([
                ("Colors".into(), Value::Text(color.into())),
                ("Explosions".into(), Value::Bool(on)),
            ]);
            let skin = head(&folders, "dance", "Peters-DDR", &params).unwrap();
            assert_eq!(skin.name, expected);
            let metrics = NoteDisplay::read(&skin, "Down");
            for part in [Part::TapNote, Part::TapFake] {
                assert_eq!(metrics.part(part).animation_is_vivid, vivid);
                assert_eq!(
                    metrics.part(part).color_spacing,
                    [if vivid { 0.0 } else { 0.0625 }, 0.0]
                );
            }
            let mut lua = LuaSkin::new(
                skin.into_owned(),
                Env {
                    params,
                    ..Env::default()
                },
                ddi_lua::Settings::default(),
                LoadBudget::default(),
                Box::new(|| 0.0),
            )
            .unwrap();
            let mut textures = Textures::default();
            for button in Button::ALL {
                for name in ["Tap Note", "Tap Fake", "Tap Lift", "Receptor", "Explosion"] {
                    let built = lua.element(button, name, &mut textures, &AnySize).unwrap();
                    assert!(
                        built.notes.is_empty(),
                        "{color} {button:?} {name}: {:?}",
                        built.notes
                    );
                    let Built::Actor(actor) = built.built else {
                        panic!("{name}")
                    };
                    if name == "Explosion" {
                        assert_eq!(!actor.children().is_empty(), on);
                    }
                    if name == "Receptor" {
                        assert!(!actor.children().is_empty());
                    }
                }
            }
            assert!(lua.warnings().is_empty(), "{:?}", lua.warnings());
            // The selected tap model must still use the original color textures.
            let expected = if vivid {
                "textures/colours.png"
            } else {
                "textures/Tap Note parts (mipmaps).png"
            };
            assert!(
                textures.keys().iter().any(|key| matches!(key,
                    ddi_noteskin::build::TextureKey::Skin { path, .. } if path == expected
                )),
                "{color}: {:?}",
                textures.keys()
            );
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
