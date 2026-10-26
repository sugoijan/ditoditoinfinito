//! The skins bundled with the game (`assets/noteskins/<game>/<name>/`, see
//! `docs/plans/bundled-skins.md`) load through their Lua with every
//! element the engine asks for, for all six dance buttons and the button
//! a Dancing☆Onigiri onigiri lane takes (`docs/plans/danoni-skins.md`),
//! without a warning, and their taps and receptors are their own (all of
//! Infinito's, Alpha-Arrows', Dake's and Peter Lawrence's CF skins' elements
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

    fn components(
        &self,
        _dir: Dir,
        path: &str,
    ) -> Option<Result<Vec<ddi_noteskin::component::SvgComponent>, String>> {
        (path.starts_with("_spiral")
            && (path.ends_with("components.svg") || path == "_spiral explosions.svg"))
            .then(|| {
                let file = Path::new(env!("CARGO_MANIFEST_DIR"))
                    .join("../../assets/noteskins/dance/Infinito")
                    .join(path);
                ddi_noteskin::component::components(&std::fs::read_to_string(file).unwrap())
            })
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

/// Like the renderer, ask for bodies first and omit the legacy pieces
/// owned by a complete long note. Other skins still exercise every piece.
/// Infinito's Lua with its parameters set to `params`.
fn infinito(folders: &[Folder], params: ddi_noteskin::params::Values) -> LuaSkin {
    LuaSkin::new(
        Skin::load(folders, "dance", "Infinito")
            .unwrap()
            .into_owned(),
        Env {
            params,
            ..Env::default()
        },
        ddi_lua::Settings::default(),
        LoadBudget::default(),
        Box::new(|| 0.0),
    )
    .unwrap()
}

fn engine_elements(
    lua: &mut LuaSkin,
    button: Button,
    textures: &mut Textures,
) -> Vec<&'static str> {
    let mut owned = std::collections::HashSet::new();
    for name in ENGINE_ELEMENTS
        .iter()
        .filter(|name| name.contains(" Body "))
    {
        let element = lua.element(button, name, textures, &AnySize).unwrap();
        if matches!(element.built, Built::LongNote(_)) {
            for part in ["Head", "Tail", "Topcap", "Bottomcap"] {
                owned.insert(name.replace(" Body ", &format!(" {part} ")));
            }
        }
    }
    ENGINE_ELEMENTS
        .iter()
        .copied()
        .filter(|name| !owned.contains(*name))
        .collect()
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
        // The onigiri: Infinito's and DanOni's own, the others' center.
        let seven = ddi_chart::Layout::builtin("danoni-7").unwrap();
        let onigiri = seven
            .lanes
            .iter()
            .find(|l| l.glyph == ddi_chart::Glyph::Onigiri)
            .and_then(|l| skin.lane_button(l))
            .unwrap()
            .button;
        let expected = if ["DanOni", "Infinito"].contains(&f.name.as_str()) {
            Button::Onigiri
        } else {
            Button::Center
        };
        assert_eq!(onigiri, expected, "{}", f.name);
        // A pump pad's center: every skin's own (docs/plans/pump.md).
        let centre = ddi_chart::Layout::pump_single().lanes[2].clone();
        let centre = skin.lane_button(&centre).unwrap().button;
        assert_eq!(centre, Button::Center, "{}", f.name);
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
            for e in engine_elements(&mut lua, b, &mut textures) {
                let el = lua
                    .element(b, e, &mut textures, &AnySize)
                    .unwrap_or_else(|err| panic!("{} {b:?} {e}: {err}", f.name));
                // Infinito, Alpha-Arrows, Dake and the CF skins draw everything
                // themselves.
                if f.name == "Infinito"
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
                if ["Tap Note", "Receptor"].contains(&e) {
                    assert!(
                        matches!(el.built, Built::Actor(_) | Built::Component(_))
                            && el.notes.is_empty(),
                        "{} {b:?} {e}: {:?}",
                        f.name,
                        el.notes
                    );
                }
            }
        }
        assert_eq!(lua.warnings(), Vec::<String>::new(), "{}", f.name);
        // The center's receptor is drawn from the skin's center art (its
        // onigiri's, DanOni's), not Down's.
        let mut receptor = Textures::default();
        lua.element(Button::Center, "Receptor", &mut receptor, &AnySize)
            .unwrap();
        let own = if f.name == "DanOni" {
            "onigiri"
        } else {
            "center"
        };
        assert!(
            receptor.keys().iter().any(|k| matches!(k,
                ddi_noteskin::build::TextureKey::Skin { path, .. } if path.to_lowercase().contains(own))),
            "{}: {:?}",
            f.name,
            receptor.keys()
        );
    }
}

#[test]
fn spiral_components_keep_shapes_clocks_parameters_and_actor_fallbacks() {
    use ddi_noteskin::component::{Clocks, Component};
    let folders = bundled();
    let skin = Skin::load(&folders, "dance", "Infinito").unwrap();
    assert_eq!(skin.declared_buttons().count(), 10);
    let own = folders.iter().find(|f| f.name == "Infinito").unwrap();
    for button in ["Down", "Center", "Onigiri"] {
        for element in ["Tap Note", "Receptor"] {
            assert!(
                !own.files.contains(&format!("{button} {element}.lua")),
                "button availability comes from metrics, without a forwarding file"
            );
        }
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
    let mut component = |button, name| -> Component {
        let built = lua.element(button, name, &mut textures, &AnySize).unwrap();
        assert!(
            built.notes.is_empty(),
            "{button:?} {name}: {:?}",
            built.notes
        );
        let Built::Component(c) = built.built else {
            panic!("{button:?} {name} is not a component");
        };
        assert_eq!(c.parts.len(), 3);
        c
    };
    for (button, shape, rotation) in [
        (Button::Down, "vertical", 180.0),
        (Button::Up, "vertical", 0.0),
        (Button::Left, "arrow", 270.0),
        (Button::Right, "arrow", 90.0),
        (Button::UpLeft, "arrow", 315.0),
        (Button::UpRight, "arrow", 45.0),
        (Button::DownLeft, "arrow", 225.0),
        (Button::DownRight, "arrow", 135.0),
        (Button::Center, "center", 0.0),
        (Button::Onigiri, "onigiri", 0.0),
    ] {
        let tap = component(button, "Tap Note");
        assert_eq!(tap.id, format!("tap-{shape}"));
        assert_eq!(tap.rotation, rotation);
        assert_eq!(component(button, "Tap Fake"), tap, "fakes remain taps");
        let clocks = Clocks {
            beat: 1.125,
            note: Some(4.5),
            ..Default::default()
        };
        let pose: Vec<_> = tap.pose(&clocks).collect();
        // The half-beat note displaces a four-beat Vivid cycle by two beats.
        assert!((pose[1].phase - (0.626 - 3.125 / 4.0)).abs() < 1e-5);
        assert!(pose[1].part.material.is_some());
        for (beat, stage) in [(0.0, 0), (0.25, 1), (0.5, 2), (0.75, 3), (1.0, 0)] {
            let clocks = Clocks {
                beat: 4.5 + beat,
                note: Some(4.5),
                ..Default::default()
            };
            let pose: Vec<_> = tap.pose(&clocks).collect();
            assert_eq!(pose[2].texture, tap.parts[2].textures[stage]);
        }
        let receptor = component(button, "Receptor");
        assert_eq!(receptor.id, format!("receptor-{shape}"));
        for (beat, frame) in [(0.0, 0), (0.199, 0), (0.2, 1), (0.99, 1), (1.0, 0)] {
            let clocks = Clocks {
                beat,
                ..Default::default()
            };
            let pose: Vec<_> = receptor.pose(&clocks).collect();
            assert_eq!(pose[2].texture, receptor.parts[2].textures[frame]);
        }
        let offset = match shape {
            "vertical" => 1.0,
            "onigiri" => -7.0,
            _ => 0.0,
        };
        assert_eq!(receptor.pivot, [0.0, offset]);
        assert_eq!(receptor.placement(&Clocks::default()).zoom, 1.0);
        for (hit, zoom) in [(0.0, 0.9), (1.0 / 60.0, 0.9), (0.05, 0.95), (0.1, 1.0)] {
            let clocks = Clocks {
                hit: Some(hit),
                ..Default::default()
            };
            let placement = receptor.placement(&clocks);
            assert!((placement.zoom - zoom).abs() < 1e-5);
            // The renderer applies P + zoom * (point - P), then the lane turn.
            let aligned_y = receptor.pivot[1] + placement.zoom * (offset - receptor.pivot[1]);
            assert!((aligned_y - offset).abs() < 1e-5);
            for p in receptor.pose(&clocks) {
                assert_eq!(
                    (p.x, p.y, p.zoom),
                    (0.0, 0.0, 1.0),
                    "parts need no compensation"
                );
            }
        }
    }
    {
        let element = "Tap Mine";
        let built = lua
            .element(Button::Down, element, &mut textures, &AnySize)
            .unwrap();
        assert!(built.notes.is_empty(), "{element}: {:?}", built.notes);
        assert!(
            matches!(built.built, Built::Actor(_)),
            "{element} keeps its actors"
        );
    }
    assert!(
        !textures.keys().iter().any(|k| matches!(k,
        ddi_noteskin::build::TextureKey::Skin {path, ..}
            if path.starts_with("_spiral color") || path.starts_with("_spiral center color")
                || path.starts_with("_spiral onigiri color"))),
        "no baked tap color sheets"
    );

    let mut values = ddi_noteskin::params::Values::new();
    values.insert("Onigiri".into(), ddi_noteskin::params::Value::Bool(false));
    values.insert(
        "ReceptorFill".into(),
        ddi_noteskin::params::Value::Bool(false),
    );
    let mut lua = infinito(&folders, values);
    let built = lua
        .element(Button::Onigiri, "Receptor", &mut textures, &AnySize)
        .unwrap();
    let Built::Component(c) = built.built else {
        panic!("center receptor component")
    };
    assert_eq!(c.id, "receptor-center");
    assert!(
        !c.parts[1].visible,
        "the optional receptor interior is hidden"
    );
}

#[test]
fn infinito_color_modes_preserve_charge_and_encode_timing_on_the_full_tick_grid() {
    use ddi_noteskin::component::{Clocks, Property};
    use ddi_noteskin::params::Value;
    let folders = bundled();
    for color in ["Vivid", "Rainbow", "Flat"] {
        let mut lua = infinito(
            &folders,
            [
                ("Colors".into(), Value::Text(color.into())),
                ("NoteTimedCharge".into(), Value::Bool(false)),
            ]
            .into(),
        );
        let mut textures = Textures::default();
        for button in Button::ALL.into_iter().chain([
            Button::DownLeft,
            Button::DownRight,
            Button::Center,
            Button::Onigiri,
        ]) {
            let mut load = |element: &str| {
                let el = lua
                    .element(button, element, &mut textures, &AnySize)
                    .unwrap();
                assert!(
                    el.notes.is_empty(),
                    "{color} {button:?} {element}: {:?}",
                    el.notes
                );
                el.built
            };
            let Built::Component(tap) = load("Tap Note") else {
                panic!("tap component")
            };
            let Built::Component(fake) = load("Tap Fake") else {
                panic!("fake component")
            };
            assert_eq!(tap, fake);
            let Built::Lift(lift) = load("Tap Lift") else {
                panic!("lift component")
            };
            let band = &tap.parts[1];
            let charge = &tap.parts[2];
            let lift_charge = lift
                .head
                .parts
                .iter()
                .find(|p| p.name.as_deref() == Some("head-charge"))
                .unwrap();
            for part in [band, lift_charge] {
                assert_eq!(
                    part.material.as_ref().unwrap().params[3],
                    [0.021, 1.35, 0.3, 2.0]
                );
                assert_eq!(
                    part.material.as_ref().unwrap().params[2][1],
                    if color == "Rainbow" { 1.0 } else { 0.0 }
                );
                for beat in [-3.25, 0.0, 1.125, 47.999, 48.0, 1000.25] {
                    for tick in 0..48 {
                        let note = 8.0 + tick as f32 / 48.0;
                        let clocks = Clocks {
                            beat,
                            note: Some(note),
                            ..Default::default()
                        };
                        let phase = part.value(Property::Phase, &clocks);
                        if color == "Rainbow" {
                            // Same endpoints as the game's Rainbow bands, including triplets.
                            let encoded = (phase - beat + 0.25).rem_euclid(64.0).floor();
                            let family = ((encoded / 16.0).ceil() - 1.0).rem_euclid(4.0) as u32;
                            let expected = match tick {
                                1..=12 => 0,
                                13..=24 => 1,
                                25..=36 => 2,
                                _ => 3,
                            };
                            assert_eq!(family, expected, "{button:?} {beat} {tick}");
                        } else {
                            let offset = if color == "Flat" {
                                0.0
                            } else {
                                (note.fract() * 4.0).floor()
                            };
                            let expected = 0.626 - (beat + offset).rem_euclid(4.0) / 4.0;
                            assert!((phase - expected).abs() < 1e-5, "{color} {beat} {tick}");
                        }
                        assert_eq!(
                            charge.value(Property::Frame, &clocks),
                            (beat.rem_euclid(1.0) * 4.0).floor()
                        );
                    }
                }
            }
        }
        assert!(lua.warnings().is_empty(), "{:?}", lua.warnings());
    }
}

#[test]
fn infinito_note_timed_charge_aligns_each_note_without_changing_its_colors() {
    use ddi_noteskin::component::{Clocks, Property};
    use ddi_noteskin::params::{Declared, Value};
    let folders = bundled();
    let declared = Declared::of_skin(&folders, "dance", "Infinito");
    assert_eq!(
        declared.get("NoteTimedCharge").unwrap().default,
        Value::Bool(true)
    );
    for color in ["Vivid", "Rainbow", "Flat"] {
        let params = [
            ("Colors".into(), Value::Text(color.into())),
            ("NoteTimedCharge".into(), Value::Bool(false)),
        ]
        .into();
        let mut original = infinito(&folders, params);
        let mut timed = infinito(
            &folders,
            [("Colors".into(), Value::Text(color.into()))].into(),
        );
        let mut textures = Textures::default();
        for button in Button::ALL.into_iter().chain([
            Button::DownLeft,
            Button::DownRight,
            Button::Center,
            Button::Onigiri,
        ]) {
            for element in [
                "Receptor",
                "Tap Note",
                "Tap Fake",
                "Tap Lift",
                "Hold Body",
                "Roll Body",
            ] {
                let mut load = |lua: &mut LuaSkin| {
                    let el = lua
                        .element(button, element, &mut textures, &AnySize)
                        .unwrap();
                    assert!(el.notes.is_empty(), "{button:?} {element}: {:?}", el.notes);
                    el.built
                };
                let old = load(&mut original);
                let new = load(&mut timed);
                if element == "Receptor" {
                    assert_eq!(old, new);
                    continue;
                }
                let head = |built: Built| match built {
                    Built::Component(c) => c,
                    Built::Lift(lift) => lift.head,
                    Built::LongNote(note) => note.head.unwrap(),
                    _ => panic!("{element}: component head"),
                };
                let old = head(old);
                let new = head(new);
                for (old_part, part) in old.parts.iter().zip(&new.parts) {
                    if !part.name.as_deref().is_some_and(|n| n.ends_with("-charge")) {
                        assert_eq!(old_part, part, "{color} {button:?} {element}");
                        continue;
                    }
                    assert_eq!(part.textures, old_part.textures);
                    // Frame zero is full for taps and long heads, empty for lifts.
                    // This samples the approach across every subdivision of the chart grid.
                    for tick in 0..48 {
                        let note = 8.0 + tick as f32 / 48.0;
                        for (delta, expected) in
                            [(-0.625, 1.0), (-0.375, 2.0), (-0.125, 3.0), (0.0, 0.0)]
                        {
                            for cycle in [-2.0, -1.0, 0.0] {
                                let clocks = Clocks {
                                    beat: note + delta + cycle,
                                    note: Some(note),
                                    ..Default::default()
                                };
                                assert_eq!(
                                    part.value(Property::Frame, &clocks),
                                    expected,
                                    "{color} {button:?} {element}: {note} {delta} {cycle}"
                                );
                                assert_eq!(
                                    part.value(Property::Phase, &clocks),
                                    old_part.value(Property::Phase, &clocks)
                                );
                            }
                        }
                    }
                    let now = |note| Clocks {
                        beat: 8.0,
                        note: Some(note),
                        ..Default::default()
                    };
                    assert_eq!(part.value(Property::Frame, &now(8.0)), 0.0);
                    assert_eq!(part.value(Property::Frame, &now(8.5)), 2.0);
                }
            }
        }
        assert!(timed.warnings().is_empty(), "{:?}", timed.warnings());
    }
}

#[test]
fn spiral_shared_mines_default_on_every_lane_and_shock_arrows_remain_optional() {
    use ddi_noteskin::params::{Declared, Value, Values};
    let folders = bundled();
    let declared = Declared::of_skin(&folders, "dance", "Infinito");
    assert!(declared.warnings.is_empty(), "{:?}", declared.warnings);
    assert_eq!(
        declared.get("ShockArrows").unwrap().default,
        Value::Bool(false)
    );
    for arrows in [None, Some(false), Some(true)] {
        for onigiri in [false, true] {
            let mut params = Values::from([("Onigiri".into(), Value::Bool(onigiri))]);
            if let Some(arrows) = arrows {
                params.insert("ShockArrows".into(), Value::Bool(arrows));
            }
            let mut lua = infinito(&folders, params);
            let mut textures = Textures::default();
            for (button, name) in [
                (Button::Down, "down"),
                (Button::Up, "up"),
                (Button::Left, "left"),
                (Button::Right, "right"),
                (Button::DownLeft, "downleft"),
                (Button::DownRight, "downright"),
                (Button::UpLeft, "upleft"),
                (Button::UpRight, "upright"),
                (Button::Center, "center"),
                (Button::Onigiri, if onigiri { "onigiri" } else { "center" }),
            ] {
                let built = lua
                    .element(button, "Tap Mine", &mut textures, &AnySize)
                    .unwrap();
                assert!(built.notes.is_empty(), "{button:?}: {:?}", built.notes);
                let Built::Actor(mut actor) = built.built else {
                    panic!("linked mine actor")
                };
                assert_eq!(
                    actor
                        .children()
                        .iter()
                        .map(|c| c.name.as_str())
                        .collect::<Vec<_>>(),
                    [
                        "MineShadow",
                        "MineCore",
                        "MineRimLeft",
                        "MineRimRight",
                        "MineLink"
                    ]
                );
                let expected = if arrows == Some(true) {
                    [
                        format!("_spiral roll {name} head shadow (res 96x96).svg"),
                        format!("_spiral mine {name} base.svg"),
                    ]
                } else {
                    [
                        "_spiral mine shared shadow.svg".into(),
                        "_spiral mine shared base.svg".into(),
                    ]
                };
                for (child, expected) in actor.children().iter().take(2).zip(expected) {
                    let image = if child.name == "MineCore" && arrows != Some(true) {
                        &child.children()[0]
                    } else {
                        child
                    };
                    let ddi_actor::Kind::Sprite(sprite) = &image.kind else {
                        panic!("mine sprite")
                    };
                    assert!(
                        matches!(&textures.keys()[sprite.texture as usize],
                        ddi_noteskin::build::TextureKey::Skin {path,..} if path == &expected),
                        "{button:?}, arrows={arrows:?}: {expected}"
                    );
                    assert_eq!(child.current().pos[..2], [0.0; 2]);
                    assert_eq!(child.current().rotation[2], 0.0);
                }
                let prefix = if arrows == Some(true) {
                    "_spiral mine"
                } else {
                    "_spiral mine shared"
                };
                let suffixes = if arrows == Some(true) {
                    [
                        "rim left 16x1 (res 1536x96).svg",
                        "rim right 16x1 (res 1536x96).svg",
                        "link 16x1 (res 1024x96).svg",
                    ]
                } else {
                    [
                        "rim left 8x8 (res 768x768).svg",
                        "rim right 8x8 (res 768x768).svg",
                        "link 16x1 (res 1024x96).svg",
                    ]
                };
                for (i, (child, suffix)) in
                    actor.children().iter().skip(2).zip(suffixes).enumerate()
                {
                    let ddi_actor::Kind::Sprite(sprite) = &child.kind else {
                        panic!("electric sprite")
                    };
                    let expected = format!("{prefix} {suffix}");
                    assert!(matches!(&textures.keys()[sprite.texture as usize],
                        ddi_noteskin::build::TextureKey::Skin {path,..} if path == &expected));
                    assert_eq!(
                        sprite.states.len(),
                        if arrows != Some(true) && i < 2 {
                            64
                        } else {
                            16
                        }
                    );
                }
                if arrows != Some(true) {
                    let mut problems = ddi_actor::Problems::default();
                    for (seconds, beat, state) in [(1.25, 2.8, 1), (1.5, 3.2, 0)] {
                        actor.update(
                            0.0,
                            &ddi_actor::Clocks {
                                music_seconds: seconds,
                                beat,
                                ..Default::default()
                            },
                            &mut problems,
                        );
                        for child in actor.children().iter().take(2) {
                            assert!(
                                (child.current().rotation[2] - (-22.5 * seconds).rem_euclid(360.0))
                                    .abs()
                                    < 1e-4
                            );
                        }
                        for child in actor.children().iter().skip(2) {
                            assert_eq!(
                                child.current().rotation[2],
                                0.0,
                                "electric geometry never spins"
                            );
                        }
                        let charge = &actor.children()[1].children()[1];
                        let ddi_actor::Kind::Sprite(sprite) = &charge.kind else {
                            panic!("charge sprite")
                        };
                        assert_eq!(
                            sprite.state(),
                            state,
                            "full and almost-full follow the beat"
                        );
                        assert_eq!(sprite.states.len(), 2);
                    }
                    assert!(problems.messages.is_empty(), "{problems:?}");
                }
            }
            assert!(lua.warnings().is_empty(), "{:?}", lua.warnings());
        }
    }
}

#[test]
fn spiral_long_notes_and_lifts_use_roles_and_the_active_roll_window() {
    use ddi_noteskin::component::{Clocks, Property};
    let folders = bundled();
    for onigiri in [true, false] {
        let mut lua = infinito(
            &folders,
            [("Onigiri".into(), ddi_noteskin::params::Value::Bool(onigiri))].into(),
        );
        let mut textures = Textures::default();
        for button in Button::ALL.into_iter().chain([
            Button::DownLeft,
            Button::DownRight,
            Button::Center,
            Button::Onigiri,
        ]) {
            let load = |lua: &mut LuaSkin, textures: &mut Textures, element: &str| {
                let el = lua.element(button, element, textures, &AnySize).unwrap();
                assert!(el.notes.is_empty(), "{button:?} {element}: {:?}", el.notes);
                el.built
            };
            let expected = if button == Button::Onigiri && !onigiri {
                "center".into()
            } else {
                button.name().to_lowercase()
            };
            for roll in [false, true] {
                let kind = if roll { "Roll" } else { "Hold" };
                for state in ["Inactive", "Active"] {
                    let Built::LongNote(n) =
                        load(&mut lua, &mut textures, &format!("{kind} Body {state}"))
                    else {
                        panic!("{button:?} {kind} {state}: whole long note")
                    };
                    assert!(n.body.file.ends_with(&format!("{expected} components.svg")));
                    assert_eq!(n.pitch, if roll { 25.0 } else { 12.5 });
                    let head = n.head.as_ref().unwrap();
                    for role in ["band", "protect", "outside", "shadow"] {
                        assert!(head.parts.iter().any(|p| p.role.as_deref() == Some(role)));
                    }
                    for c in [&n.body, n.cap.as_ref().unwrap()] {
                        for role in ["band", "envelope", "shadow"] {
                            assert!(c.parts.iter().any(|p| p.role.as_deref() == Some(role)));
                        }
                        if roll {
                            let pulse = c
                                .parts
                                .iter()
                                .find(|p| p.role.as_deref() == Some("pulse"))
                                .unwrap();
                            for window in [0.175, 0.35, 0.7] {
                                for i in 0..=100 {
                                    let phase = i as f32 / 100.0;
                                    let seconds = phase * window * 0.75;
                                    let expected = 0.08
                                        + 0.77
                                            * (1.0_f32 - phase.rem_euclid(1.0) / 0.25)
                                                .max(0.0)
                                                .powf(0.65);
                                    for beat in [1.0, 8.125] {
                                        let clocks = Clocks {
                                            seconds,
                                            beat,
                                            roll_window: window,
                                            ..Default::default()
                                        };
                                        assert!(
                                            (pulse.value(Property::Opacity, &clocks) - expected)
                                                .abs()
                                                < 1e-5
                                        );
                                    }
                                }
                            }
                        }
                    }
                    let fill = n.fill.as_ref().unwrap();
                    let material = fill.material.as_ref().unwrap();
                    assert!(material.file.ends_with("_spiral wave.wgsl"));
                    assert_eq!(material.params[0][2], if roll { 171.0 } else { 121.0 });
                    assert_eq!(material.params[6], [0.08, 0.22, 0.0, 0.0]);
                    let a = fill.value(
                        Property::Phase,
                        &Clocks {
                            seconds: 0.125,
                            ..Default::default()
                        },
                    );
                    let b = fill.value(
                        Property::Phase,
                        &Clocks {
                            seconds: 1.125,
                            ..Default::default()
                        },
                    );
                    assert!(
                        (a - b).abs() < 1e-5,
                        "the palette phase loops while the field uses unwrapped song seconds"
                    );
                }
            }
            let Built::Lift(lift) = load(&mut lua, &mut textures, "Tap Lift") else {
                panic!("{button:?}: lift component")
            };
            assert!(
                lift.head
                    .file
                    .ends_with(&format!("{expected} components.svg"))
            );
            assert_eq!((lift.seconds, lift.fade, lift.opacity), (0.28, 2.0, 1.0));
            assert_eq!(
                lift.trail.as_ref().unwrap().view_box,
                [-48.0, -308.0, 96.0, 360.0]
            );
            let charge = lift
                .head
                .parts
                .iter()
                .find(|p| p.name.as_deref() == Some("head-charge"))
                .unwrap();
            assert_eq!(charge.textures.len(), 4);
            assert!(
                charge
                    .material
                    .as_ref()
                    .unwrap()
                    .file
                    .ends_with("_spiral material.wgsl")
            );
            assert_eq!(charge.material.as_ref().unwrap().params[1][3], 34.848);
            let band = lift
                .head
                .parts
                .iter()
                .find(|p| p.name.as_deref() == Some("head-band"))
                .unwrap();
            let band_material = band.material.as_ref().unwrap();
            assert_eq!(band_material.params[5][0], 0.7);
            for part in [band, lift.fill.as_ref().unwrap()] {
                assert_eq!(
                    part.material.as_ref().unwrap().params[6],
                    [0.08, 0.22, 0.0, 0.0]
                );
            }
        }
        assert!(lua.warnings().is_empty(), "{:?}", lua.warnings());
        assert!(!textures.keys().iter().any(|k| matches!(k,
            ddi_noteskin::build::TextureKey::Skin {path, ..} if path.starts_with("_spiral lift color"))),
            "no baked lift color sheets");
    }
}

#[test]
fn spiral_explosions_preserve_flash_timing_colors_variation_and_hold_pulses() {
    use ddi_noteskin::component::{Clocks, PartBlend, Property};
    let effect_file = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../assets/noteskins/dance/Infinito/_spiral explosions.svg");
    let source = std::fs::read_to_string(effect_file).unwrap();
    ddi_noteskin::component::part_documents("_spiral explosions.svg", &source)
        .expect("the complete framed effect stays within the derived-document budget");
    let folders = bundled();
    for onigiri in [true, false] {
        for vary in [true, false] {
            let mut lua = infinito(
                &folders,
                [
                    ("Onigiri".into(), ddi_noteskin::params::Value::Bool(onigiri)),
                    (
                        "HitVariation".into(),
                        ddi_noteskin::params::Value::Bool(vary),
                    ),
                ]
                .into(),
            );
            for button in Button::ALL.into_iter().chain([
                Button::DownLeft,
                Button::DownRight,
                Button::Center,
                Button::Onigiri,
            ]) {
                let el = lua
                    .element(button, "Explosion", &mut Textures::default(), &AnySize)
                    .unwrap();
                assert!(el.notes.is_empty(), "{:?}", el.notes);
                let Built::Component(c) = el.built else {
                    panic!("{button:?}: explosion component")
                };
                let shape = if button == Button::Center || button == Button::Onigiri && !onigiri {
                    "center"
                } else if button == Button::Onigiri {
                    "onigiri"
                } else {
                    "arrow"
                };
                assert_eq!(c.id, format!("explosion-{shape}"));
                assert_eq!(c.parts.len(), 19);
                assert_eq!(c.duration, 0.5);
                let offset = match button {
                    Button::Up => 1.0,
                    Button::Down => -1.0,
                    Button::Onigiri if onigiri => -7.0,
                    _ => 0.0,
                };
                for p in &c.parts {
                    assert_eq!(p.blend, PartBlend::Add);
                    let circular =
                        matches!(p.role.as_deref(), Some("rim-left" | "rim-right" | "link"));
                    assert_eq!(
                        p.value(Property::Y, &Clocks::default()),
                        if circular { 0.0 } else { offset }
                    );
                    if let Some(m) = &p.material {
                        if m.params[1][0] == 1.0 {
                            assert_eq!(m.params[1][1], u8::from(vary) as f32);
                        }
                    } else {
                        assert!(p.name.as_deref().unwrap_or("").contains("-mine-"));
                    }
                    assert!(!p.shows_for("W5", false));
                }
                let value = |name: &str, property, t| {
                    c.parts
                        .iter()
                        .find(|p| p.name.as_deref() == Some(&format!("{shape}-{name}")))
                        .unwrap()
                        .value(
                            property,
                            &Clocks {
                                hit: Some(t),
                                ..Default::default()
                            },
                        )
                };
                for (t, gain) in [(0.0, 1.8), (0.025, 1.8), (0.1, 0.9), (0.175, 0.0)] {
                    let sum = value("burst-1", Property::Opacity, t)
                        + value("burst-2", Property::Opacity, t);
                    assert!((sum - gain).abs() < 1e-5, "accepted W1 gain at {t}: {sum}");
                }
                assert_eq!(value("burst-1", Property::Zoom, 0.0), 0.85);
                assert_eq!(value("burst-1", Property::Zoom, 0.175), 1.25);
                assert_eq!(value("best-core", Property::Opacity, 0.045), 0.0);
                assert_eq!(value("perfect-core", Property::Opacity, 5.0 / 60.0), 0.0);
                let rgb = |name: &str| {
                    c.parts
                        .iter()
                        .find(|p| p.name.as_deref() == Some(&format!("{shape}-{name}")))
                        .unwrap()
                        .material
                        .as_ref()
                        .unwrap()
                        .params[0]
                };
                assert_eq!(rgb("tap-echo"), [1.0, 240.0 / 255.0, 100.0 / 255.0, 1.0]);
                assert_eq!(rgb("tap-echo-w3"), [63.0 / 255.0, 1.0, 63.0 / 255.0, 1.0]);
                for p in c
                    .parts
                    .iter()
                    .filter(|p| p.name.as_deref().unwrap_or("").contains("-mine-"))
                {
                    assert!(p.shows_for("HitMine", false));
                    assert!(
                        !p.shows_for("W1", false),
                        "damage layers never tint a success"
                    );
                }
                for t in [0.0, 0.05, 0.15, 0.3, 0.44] {
                    for name in [
                        "mine-glow-left",
                        "mine-glow-right",
                        "mine-glow-link",
                        "mine-wire-left",
                        "mine-wire-right",
                        "mine-wire-link",
                    ] {
                        assert_eq!(
                            value(name, Property::Zoom, t),
                            1.0,
                            "damage stays on the receptor"
                        );
                    }
                }
                assert_eq!(value("mine-core", Property::Zoom, 0.0), 0.9);
                assert_eq!(value("mine-core", Property::Zoom, 5.0 / 60.0), 1.0);
                for name in [
                    "mine-glow-left",
                    "mine-glow-right",
                    "mine-glow-link",
                    "mine-wire-left",
                    "mine-wire-right",
                    "mine-wire-link",
                ] {
                    let part = c
                        .parts
                        .iter()
                        .find(|p| p.name.as_deref() == Some(&format!("{shape}-{name}")))
                        .unwrap();
                    assert_eq!(part.textures.len(), 16);
                    assert_eq!(value(name, Property::Opacity, 0.45), 0.0);
                }
                let pulse = |t| {
                    let clocks = Clocks {
                        hit: Some(t),
                        ..Default::default()
                    };
                    c.pose(&clocks)
                        .filter(|p| p.part.shows_for("Holding", false))
                        .map(|p| p.opacity)
                        .sum::<f32>()
                };
                assert!(
                    (pulse(0.0) - 0.9).abs() < 1e-5,
                    "no pulses before the hold starts"
                );
                assert!(
                    (pulse(0.18) - 0.84).abs() < 1e-4,
                    "overlapping echoes retain their cadence"
                );
                for p in c.parts.iter().filter(|p| p.shows_for("Holding", false)) {
                    assert_eq!(
                        p.value(
                            Property::TintMix,
                            &Clocks {
                                hit: Some(1.3),
                                ended: Some(1.0),
                                ..Default::default()
                            }
                        ),
                        1.0,
                        "longer mine life must not prolong the previous held tail"
                    );
                }
            }
            assert!(lua.warnings().is_empty(), "{:?}", lua.warnings());
        }
    }
    let mut lua = infinito(
        &folders,
        [(
            "Explosions".into(),
            ddi_noteskin::params::Value::Bool(false),
        )]
        .into(),
    );
    let el = lua
        .element(
            Button::Down,
            "Explosion",
            &mut Textures::default(),
            &AnySize,
        )
        .unwrap();
    let Built::Actor(a) = el.built else {
        panic!("explosions disabled")
    };
    assert!(a.children().is_empty());
}

#[test]
fn infinito_is_the_component_default_with_no_retired_variants() {
    use ddi_noteskin::params::{Declared, Value, Values, head};
    let folders = bundled();
    for name in [
        "Spiral Base",
        "Dito",
        "Dito Vivid",
        "Dito Rainbow",
        "Dito Flat",
        "Dito Single",
    ] {
        assert!(!folders.iter().any(|f| f.name == name));
    }
    assert_eq!(
        head(&folders, "dance", "Infinito", &Values::new())
            .unwrap()
            .name,
        "Infinito"
    );
    assert_eq!(
        &Declared::of_skin(&folders, "dance", "Infinito"),
        ddi_noteskin::bundled::infinito_parameters()
    );
    for on in [true, false] {
        let mut lua = infinito(&folders, [("Explosions".into(), Value::Bool(on))].into());
        let el = lua
            .element(
                Button::Down,
                "Explosion",
                &mut Textures::default(),
                &AnySize,
            )
            .unwrap();
        if on {
            assert!(matches!(el.built, Built::Component(_)));
        } else {
            assert!(matches!(el.built, Built::Actor(a) if a.children().is_empty()));
        }
        assert!(el.notes.is_empty() && lua.warnings().is_empty());
    }
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
                                matches!(el.built, Built::Actor(_) | Built::Component(_))
                                    && el.notes.is_empty()
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
