//! Actors for a column's elements, without running a skin's Lua: images
//! become sprites, 3D models models, and common's `Fallback Explosion.lua`
//! and `Fallback Tap Mine.lua` are rebuilt natively with the commands their
//! metrics give (`NoteSkins/common/common/`, 5_1-new). Whatever needs the
//! skin's Lua is drawn by the built-in skin, with a note why.

use crate::actor::{Actor, Kind, Model, ModelMaterial, Problems, Sprite, State};
use crate::builtin::NativeLua;
use crate::commands::{Arg, Call, parse};
use crate::load::{Button, Resolved, Source};
use crate::skin::{Dir, FileRef, Skin};

/// A texture an actor draws.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum TextureKey {
    /// An image of an imported skin.
    Skin { dir: Dir, path: String },
    /// A stand-in's image, by file name: the renderer draws its own.
    BuiltIn(String),
}

/// The textures actors refer to, by the id their sprites carry.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Textures {
    keys: Vec<TextureKey>,
    /// Per key: a model draws it, so it gets mipmaps.
    mipmaps: Vec<bool>,
}

impl Textures {
    pub fn id(&mut self, key: TextureKey) -> u32 {
        match self.keys.iter().position(|k| *k == key) {
            Some(i) => i as u32,
            None => {
                self.keys.push(key);
                self.mipmaps.push(false);
                self.keys.len() as u32 - 1
            }
        }
    }

    /// [`Textures::id`] for a model's texture: StepMania makes mipmaps for
    /// those (`AnimatedTexture::Load`, `bMipMaps`), not for sprites'.
    pub fn model_id(&mut self, key: TextureKey) -> u32 {
        let id = self.id(key);
        self.mipmaps[id as usize] = true;
        id
    }

    pub fn keys(&self) -> &[TextureKey] {
        &self.keys
    }

    /// Whether texture `id` gets mipmaps.
    pub fn mipmaps(&self, id: usize) -> bool {
        self.mipmaps.get(id).copied().unwrap_or(false)
    }
}

/// What the builder needs to know about a skin's images (decoded by the
/// shell).
pub trait Images {
    /// The image's size in pixels; `None` when it could not be read.
    fn size(&self, dir: Dir, path: &str) -> Option<(u32, u32)>;
    /// The GPU's largest texture side.
    fn max_size(&self) -> u32;
}

/// A built element.
#[derive(Clone, Debug, PartialEq)]
#[allow(
    clippy::large_enum_variant,
    reason = "built once per element when a skin loads"
)]
pub enum Built {
    /// Drawn by these actors.
    Actor(Actor),
    /// Drawn by the built-in skin.
    BuiltIn,
    /// Nothing is drawn.
    Blank,
}

/// An element and what kept parts of it from being the skin's.
#[derive(Clone, Debug, PartialEq)]
pub struct Element {
    pub built: Built,
    pub notes: Vec<String>,
}

/// The stand-in art an explosion part shows when the skin's cannot load.
fn fallback_art(element: &str) -> &'static str {
    match element {
        "Hold Explosion" | "Roll Explosion" => "Fallback Hold Explosion 2x1.png",
        "HitMine Explosion" => "Fallback HitMine Explosion.png",
        "Tap Explosion Dim" => "Fallback Tap Explosion Dim.png",
        _ => "Fallback Tap Explosion Bright.png",
    }
}

struct Builder<'s, 'a> {
    skin: &'s Skin<'a>,
    textures: &'s mut Textures,
    images: &'s dyn Images,
    notes: Vec<String>,
}

/// Builds `element` of `button`'s column, its `Init` commands played.
pub fn build(
    skin: &Skin<'_>,
    button: Button,
    element: &str,
    textures: &mut Textures,
    images: &dyn Images,
) -> Element {
    let mut b = Builder {
        skin,
        textures,
        images,
        notes: Vec::new(),
    };
    let built = b.element(button, element, 0);
    Element {
        built,
        notes: b.notes,
    }
}

impl Builder<'_, '_> {
    fn note(&mut self, n: String) {
        if !self.notes.contains(&n) {
            self.notes.push(n);
        }
    }

    fn element(&mut self, button: Button, element: &str, depth: u32) -> Built {
        if depth > 4 {
            self.note(format!("{element}: elements load each other without end"));
            return Built::BuiltIn;
        }
        match self.skin.resolve(button, element) {
            Resolved::Blank => Built::Blank,
            Resolved::Missing(e) => {
                self.note(format!("{element}: {e}"));
                Built::BuiltIn
            }
            Resolved::NotASprite(source) => {
                self.note(format!("{element}: {} is not a sprite", source.file().path));
                Built::Blank
            }
            Resolved::File { source, rotation } => {
                let built = self.source(button, element, &source, depth);
                match built {
                    Built::Actor(mut a) => {
                        a.base_rotation[2] = rotation;
                        Built::Actor(a)
                    }
                    other => other,
                }
            }
        }
    }

    fn source(&mut self, button: Button, element: &str, source: &Source, depth: u32) -> Built {
        match source {
            Source::Image(f) => match self.image_sprite(f) {
                Some(a) => Built::Actor(a),
                None => {
                    self.note(format!("{element}: {} could not be read", f.path));
                    Built::BuiltIn
                }
            },
            Source::BuiltInArt(f) => Built::Actor(self.builtin_sprite(f.name())),
            Source::Native(NativeLua::Explosion, _) => Built::Actor(self.explosion(button, depth)),
            Source::Native(NativeLua::TapMine, _) => Built::Actor(self.mine()),
            Source::Native(_, _) => Built::BuiltIn,
            Source::Lua(f) => {
                self.note(format!("{element}: {} is Lua", f.path));
                Built::BuiltIn
            }
            Source::Model(f) => match model_actor(self.skin, self.textures, self.images, f, f, f) {
                Ok(a) => Built::Actor(a),
                Err(e) => {
                    self.note(format!("{element}: {e}"));
                    Built::BuiltIn
                }
            },
            Source::Movie(f) => {
                self.note(format!("{element}: {} is a movie", f.path));
                Built::BuiltIn
            }
            Source::Other(f) => {
                self.note(format!("{element}: {} cannot be drawn", f.path));
                Built::BuiltIn
            }
        }
    }

    fn image_sprite(&mut self, f: &FileRef) -> Option<Actor> {
        image_sprite(self.skin, self.textures, self.images, f)
    }

    fn builtin_sprite(&mut self, name: &str) -> Actor {
        builtin_sprite(self.textures, name)
    }

    /// `NOTESKIN:GetMetricA(section, name)`; a metric that is not there
    /// reads as an empty command (StepMania reports it).
    fn metric_command(&mut self, section: &str, name: &str) -> Vec<Call> {
        let Some(text) = self.skin.metric(section, name) else {
            return Vec::new();
        };
        match parse(text) {
            Ok(calls) => calls,
            Err(e) => {
                self.note(format!("[{section}] {name}: {e}"));
                Vec::new()
            }
        }
    }

    fn calls(text: &str) -> Vec<Call> {
        parse(text).expect("built-in commands parse")
    }

    /// `NOTESKIN:LoadActor(Var "Button", part)`, or the stand-in's art when
    /// the skin's cannot be shown.
    fn part(&mut self, button: Button, element: &str, depth: u32) -> Actor {
        match self.element(button, element, depth + 1) {
            Built::Actor(a) => a,
            Built::Blank => Actor::new(Kind::Actor),
            Built::BuiltIn => self.builtin_sprite(fallback_art(element)),
        }
    }

    /// common's `Fallback Explosion.lua`.
    fn explosion(&mut self, button: Button, depth: u32) -> Actor {
        let mut hold = self.part(button, "Hold Explosion", depth);
        let on = self.metric_command("HoldGhostArrow", "HoldingOnCommand");
        let off = self.metric_command("HoldGhostArrow", "HoldingOffCommand");
        hold.add_command("HoldingOn", on);
        hold.add_command("HoldingOff", off);
        hold.add_command(
            "Init",
            Self::calls("playcommand,\"HoldingOff\";finishtweening"),
        );

        let mut roll = self.part(button, "Roll Explosion", depth);
        let on = self.metric_command("HoldGhostArrow", "RollOnCommand");
        let off = self.metric_command("HoldGhostArrow", "RollOffCommand");
        roll.add_command("RollOn", on);
        roll.add_command("RollOff", off);
        roll.add_command(
            "Init",
            Self::calls("playcommand,\"RollOff\";finishtweening"),
        );

        let mut flashes = Vec::new();
        for (element, section, bright) in [
            ("Tap Explosion Dim", "GhostArrowDim", false),
            ("Tap Explosion Bright", "GhostArrowBright", true),
        ] {
            let mut a = self.part(button, element, depth);
            a.add_command("Init", Self::calls("diffusealpha,0"));
            for c in ["W5", "W4", "W3", "W2", "W1", "Held"] {
                let calls = self.metric_command(section, &format!("{c}Command"));
                a.add_command(c, calls);
            }
            a.add_command("Judgment", Self::calls("finishtweening"));
            a.add_command("Bright", vec![visible(bright)]);
            a.add_command("Dim", vec![visible(!bright)]);
            flashes.push(a);
        }

        let mut mine = self.part(button, "HitMine Explosion", depth);
        mine.add_command(
            "Init",
            Self::calls("blend,\"BlendMode_Add\";diffusealpha,0"),
        );
        let hit = self.metric_command("GhostArrowBright", "HitMineCommand");
        mine.add_command("HitMine", hit);

        let mut children = vec![hold, roll];
        children.extend(flashes);
        children.push(mine);
        self.frame(children)
    }

    /// common's `Fallback Tap Mine.lua`: `_mine` in a frame ("commands don't
    /// behave if this isn't wrapped in a frame").
    fn mine(&mut self) -> Actor {
        let mut sprite = match self.skin.get_path("_mine", "") {
            Ok(f) => match self.source(Button::Down, "_mine", &self.skin.classify(f), 0) {
                Built::Actor(a) => a,
                _ => self.builtin_sprite("_mine (doubleres).png"),
            },
            Err(e) => {
                self.note(format!("Tap Mine: {e}"));
                self.builtin_sprite("_mine (doubleres).png")
            }
        };
        let init = self.metric_command("Mine", "InitCommand");
        sprite.add_command("Init", init);
        self.frame(vec![sprite])
    }

    /// A frame of `children`, each one's `Init` played (StepMania plays it
    /// once per actor as it is loaded, children first).
    fn frame(&mut self, mut children: Vec<Actor>) -> Actor {
        let mut problems = Problems::default();
        for c in &mut children {
            c.play_own("Init", &mut problems);
        }
        let mut f = Actor::new(Kind::Frame(children));
        f.play_own("Init", &mut problems);
        for m in problems.messages {
            self.note(m);
        }
        f
    }
}

/// `LoadActor` of a bitmap: a sprite of the texture's frames, 0.1 s each
/// (`Sprite::LoadStatesFromTexture`); `None` when the image cannot be read.
pub(crate) fn image_sprite(
    skin: &Skin<'_>,
    textures: &mut Textures,
    images: &dyn Images,
    f: &FileRef,
) -> Option<Actor> {
    let (w, h) = images.size(f.dir, &f.path)?;
    let sheet = skin.hints(f).sheet(w, h, images.max_size());
    let count = sheet.frame_count().min(4096);
    let states = (0..count)
        .map(|i| State {
            rect: sheet.frame_rect(i),
            delay: 0.1,
        })
        .collect();
    let (fw, fh) = sheet.frame_size();
    let id = textures.id(TextureKey::Skin {
        dir: f.dir,
        path: f.path.clone(),
    });
    let mut sprite = Sprite::new(id, states, [fw, fh]);
    sprite.padded_to_image = sheet.from_padded([1.0, 1.0]);
    Some(Actor::new(Kind::Sprite(sprite)))
}

/// A 3D model from its meshes, materials and bones (one file for all three
/// when an element is a `.txt`; `Model::Load`, `src/Model.cpp:81-124`),
/// its textures found relative to the materials file and decoded like a
/// sprite's; why it could not be loaded (StepMania fails the element too).
pub(crate) fn model_actor(
    skin: &Skin<'_>,
    textures: &mut Textures,
    images: &dyn Images,
    meshes: &FileRef,
    materials: &FileRef,
    bones: &FileRef,
) -> Result<Actor, String> {
    use crate::model::{self, AnimatedTexture};
    let text = |f: &FileRef| {
        skin.folder(f.dir)
            .and_then(|folder| folder.text(&f.path))
            .ok_or_else(|| format!("{} could not be read", f.path))
    };
    let geometry = model::meshes(text(meshes)?).map_err(|e| format!("{}: {e}", meshes.path))?;
    let mats =
        model::materials(text(materials)?).map_err(|e| format!("{}: {e}", materials.path))?;
    let animation = model::bones(text(bones)?).map_err(|e| format!("{}: {e}", bones.path))?;
    if let Some(i) = geometry
        .meshes
        .iter()
        .find_map(|m| m.material.filter(|&i| i >= mats.len()))
    {
        return Err(format!(
            "{}: a mesh's material {i} is not there",
            meshes.path
        ));
    }
    let dir_of = |f: &FileRef| {
        let p = skin.file_path(f);
        p.rfind('/').map_or(String::new(), |i| p[..=i].to_string())
    };
    let image = |f: &FileRef, textures: &mut Textures| -> Result<u32, String> {
        images
            .size(f.dir, &f.path)
            .ok_or_else(|| format!("{} could not be read", f.path))?;
        Ok(textures.model_id(TextureKey::Skin {
            dir: f.dir,
            path: f.path.clone(),
        }))
    };
    let mut out = Vec::with_capacity(mats.len());
    for m in &mats {
        let (frames, texture) = if m.texture.is_empty() {
            (Vec::new(), AnimatedTexture::image(""))
        } else {
            let f = skin.resolve_path(&format!("{}{}", dir_of(materials), m.texture))?;
            if f.path.to_ascii_lowercase().ends_with(".ini") {
                let animated = AnimatedTexture::parse(text(&f)?);
                let dir = dir_of(&f);
                let mut ids = Vec::with_capacity(animated.frames.len());
                for (frame, _) in &animated.frames {
                    let img = skin.resolve_path(&format!("{dir}{frame}"))?;
                    ids.push(image(&img, textures)?);
                }
                (ids, animated)
            } else {
                (vec![image(&f, textures)?], AnimatedTexture::image(&f.path))
            }
        };
        out.push(ModelMaterial {
            color: m.unlit_color(),
            frames,
            texture,
        });
    }
    Ok(Actor::new(Kind::Model(Box::new(Model::new(
        geometry, animation, out,
    )))))
}

/// A stand-in's image: its frames from its name, each shown at 64 px.
pub(crate) fn builtin_sprite(textures: &mut Textures, name: &str) -> Actor {
    let hints = crate::hints::Hints::parse(name);
    let (cols, rows) = hints.frames.unwrap_or((1, 1));
    let k = if hints.doubleres { 128 } else { 64 };
    let sheet = hints.sheet(cols * k, rows * k, u32::MAX);
    let states = (0..sheet.frame_count())
        .map(|i| State {
            rect: sheet.frame_rect(i),
            delay: 0.1,
        })
        .collect();
    let id = textures.id(TextureKey::BuiltIn(name.to_string()));
    let (fw, fh) = sheet.frame_size();
    Actor::new(Kind::Sprite(Sprite::new(id, states, [fw, fh])))
}

fn visible(on: bool) -> Call {
    Call {
        method: "visible".into(),
        args: vec![Arg::Bool(on)],
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::skin::tests::folder;

    struct Sizes(Vec<(&'static str, (u32, u32))>);

    impl Images for Sizes {
        fn size(&self, _dir: Dir, path: &str) -> Option<(u32, u32)> {
            self.0.iter().find(|(p, _)| *p == path).map(|(_, s)| *s)
        }

        fn max_size(&self) -> u32 {
            8192
        }
    }

    #[test]
    fn images_become_sprites_of_their_frames() {
        let folders = [folder(
            "dance",
            "s",
            &[
                ("metrics.ini", "[Global]\nFallbackNoteSkin=default\n"),
                ("Down Tap Note 4x2 (doubleres).png", ""),
                ("Down Receptor.lua", ""),
            ],
        )];
        let skin = Skin::load(&folders, "dance", "s").unwrap();
        let mut t = Textures::default();
        let images = Sizes(vec![("Down Tap Note 4x2 (doubleres).png", (512, 256))]);
        let e = build(&skin, Button::Left, "Tap Note", &mut t, &images);
        let Built::Actor(a) = e.built else {
            panic!("{e:?}")
        };
        assert_eq!(a.base_rotation[2], 90.0);
        let Kind::Sprite(s) = &a.kind else { panic!() };
        assert_eq!(s.states.len(), 8);
        assert_eq!(s.size, [64.0, 64.0]);
        assert_eq!(s.states[5].rect, [0.25, 0.5, 0.5, 1.0]);
        assert_eq!(t.keys().len(), 1);
        // The skin's Lua: built in, with a note.
        let r = build(&skin, Button::Down, "Receptor", &mut t, &images);
        assert_eq!(r.built, Built::BuiltIn);
        assert_eq!(r.notes, ["Receptor: Down Receptor.lua is Lua"]);
        // An image that could not be read.
        let folders = [folder("dance", "s", &[("Down Tap Note.png", "")])];
        let skin = Skin::load(&folders, "dance", "s").unwrap();
        let e = build(&skin, Button::Down, "Tap Note", &mut t, &Sizes(vec![]));
        assert_eq!(e.built, Built::BuiltIn);
    }

    #[test]
    fn the_explosion_row_is_commons_with_the_metrics_commands() {
        let folders = [folder(
            "dance",
            "s",
            &[
                (
                    "metrics.ini",
                    "[Global]\nFallbackNoteSkin=default\n[GhostArrowBright]\nW1Command=diffuse,1,1,1,1;linear,0.5;diffusealpha,0\n",
                ),
                ("Down Tap Explosion Bright.png", ""),
            ],
        )];
        let skin = Skin::load(&folders, "dance", "s").unwrap();
        let mut t = Textures::default();
        let images = Sizes(vec![("Down Tap Explosion Bright.png", (64, 64))]);
        let e = build(&skin, Button::Up, "Explosion", &mut t, &images);
        let Built::Actor(mut row) = e.built else {
            panic!("{e:?}")
        };
        assert!(e.notes.is_empty(), "{:?}", e.notes);
        // No rotation for the row; its flashes turn with the column.
        assert_eq!(row.base_rotation[2], 0.0);
        assert_eq!(row.children().len(), 5);
        let bright = &row.children()[3];
        assert_eq!(bright.base_rotation[2], 180.0);
        // Init left it transparent.
        assert_eq!(bright.current().diffuse[3], 0.0);
        let mut p = Problems::default();
        // A bright W1: Judgment, Bright, W1 as GhostArrowRow sends them.
        for c in ["Judgment", "Bright", "W1"] {
            row.play(c, &mut p);
        }
        assert!(p.messages.is_empty(), "{:?}", p.messages);
        let bright = &row.children()[3];
        assert!(bright.visible);
        assert_eq!(bright.current().diffuse[3], 1.0);
        assert!(!row.children()[2].visible);
        // dance/default blanks the hold explosion: an empty actor.
        assert_eq!(row.children()[0].kind, Kind::Actor);
        // The dim flash goes through dance/default's redirect to `Down Tap
        // Explosion Bright`, which the skin, first in the search order, has.
        let Kind::Sprite(s) = &row.children()[2].kind else {
            panic!()
        };
        assert_eq!(
            t.keys()[s.texture as usize],
            TextureKey::Skin {
                dir: Dir::Folder(0),
                path: "Down Tap Explosion Bright.png".into()
            }
        );
        // Without it, dance/default's `_glow`, built in.
        let plain = Skin::load(&[], "dance", "default").unwrap();
        let e = build(&plain, Button::Up, "Explosion", &mut t, &images);
        let Built::Actor(row) = e.built else { panic!() };
        let Kind::Sprite(s) = &row.children()[2].kind else {
            panic!()
        };
        assert_eq!(
            t.keys()[s.texture as usize],
            TextureKey::BuiltIn("_glow (doubleres).png".into())
        );
    }

    #[test]
    fn the_mine_spins_on_the_beat() {
        let skin = Skin::load(&[], "dance", "default").unwrap();
        let mut t = Textures::default();
        let e = build(&skin, Button::Down, "Tap Mine", &mut t, &Sizes(vec![]));
        let Built::Actor(mut m) = e.built else {
            panic!("{e:?}")
        };
        let mut p = Problems::default();
        let clocks = |beat| crate::actor::Clocks {
            beat,
            ..Default::default()
        };
        m.update(0.0, &clocks(1.0), &mut p);
        m.update(0.0, &clocks(1.5), &mut p);
        assert_eq!(m.children()[0].current().rotation[2], 270.0);
        assert_eq!(m.children()[0].effect(), crate::actor::Effect::Spin);
        // The tap note is dance/default's own look: built in.
        assert_eq!(
            build(&skin, Button::Down, "Tap Note", &mut t, &Sizes(vec![])).built,
            Built::BuiltIn
        );
    }
}
