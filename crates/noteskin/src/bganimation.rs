//! A song's background animations in the formats StepMania 3.9 and ITG
//! read (`docs/plans/scripted-backgrounds.md`, step 6), built natively:
//! a folder of the song's with a `default.xml` (OpenITG's actor tree, the
//! nodes `crate::legacy` reads), else a `BGAnimation.ini` (`[BGAnimation]`
//! and its `Layer<N>` sections), else StepMania 3.0's folder of layers
//! (each image, `.sprite` or movie in it a layer whose file name picks
//! what it does).
//!
//! Follows StepMania 3.9 (`v390release`, MIT-style): `BGAnimation.cpp`
//! (`LoadFromAniDir`, `AddLayersFromAniDir`), `BGAnimationLayer.cpp`
//! (`LoadFromAniLayerFile`, `LoadFromIni`, `Update`, `GainFocus`),
//! `ActorUtil.cpp` (`LoadFromActorFile`, `MakeActor`), `ActorCollision.h`,
//! with what OpenITG (`f2c129fe`, the same licence) added to the format:
//! `default.xml` before the ini (`ActorUtil::MakeActor`), layers sorted by
//! number (`Layer01`), `LengthSeconds` as an actor sleeping that long on
//! `On` (`BGAnimation::LoadFromNode`). Differences:
//!
//! - Lua (`Condition`, `Cond`, `DrawCond`, `File=@…`) is not run: a layer
//!   with a condition is left out, one with a `DrawCond` always drawn,
//!   with a note. `Player=2` leaves a layer out (the second player is
//!   never enabled here).
//! - The song's background (`songbackground`, `usesongbg`), banners and
//!   movies draw nothing yet, with a note; so does a scroller
//!   (`UseScroller`).
//! - Random placements (particles, `StartOnRandomFrame`) come from a
//!   generator seeded by the folder's name, so every play (and a replay)
//!   draws the same.
//! - A 3.0 layer's effect and hints are read from its path in the song's
//!   folder (StepMania reads its whole path, the pack's and the song's
//!   folder names too).
//! - Tiles move on the animation's own seconds (StepMania 3.9: the
//!   program's).
//!
//! A layer is a frame of its actors (`BGAnimationLayer` is an
//! `ActorFrame`) whose motion ([`Animation::update`]: particles' and
//! tiles' movement, `CommandRepeatSeconds`) the animation keeps.
//! Non-generic layers (the song's folder itself) center a sprite on the
//! screen or stretch it over it; a folder loaded by another (`File=` a
//! folder) is generic, at its parent's origin.

use ddi_actor::commands::{parse, parse_legacy, run};
use ddi_actor::{Actor, Blend, Clocks, Kind, Problems};

use crate::build::{Builder, Images, Textures};
use crate::ini::{Ini, string_to_float, string_to_int};
use crate::legacy::Node;
use crate::skin::{Dir, FileRef, Skin};

/// Most `Layer<N>` sections read of one ini (`MAX_LAYERS`).
const MAX_LAYERS: usize = 1000;
/// Most particles or tiles one layer may have (StepMania 3.0's layers
/// make about 500 on a 16:9 screen at most).
pub const MAX_LAYER_ACTORS: usize = 2048;
/// Most folders deep animations may load or import each other.
const MAX_DEPTH: u32 = 8;
/// What a layer's frame is named by: its index in [`Animation`]'s layers
/// after it (no file can name an actor so).
const MARK: char = '\u{1}';
/// Particles' speed, pixels a second (`PARTICLE_SPEED`).
const PARTICLE_SPEED: f32 = 300.0;

/// A song's background animation, built ([`load`]).
#[derive(Clone, Debug)]
pub struct Animation {
    /// What it draws, on StepMania's screen.
    pub tree: Actor,
    layers: Vec<Layer>,
    screen: [f32; 2],
}

/// What a layer does each frame besides its actors' own updates.
#[derive(Clone, Debug, Default, PartialEq)]
struct Layer {
    motion: Motion,
    /// `CommandRepeatSeconds`: `On` again every so many seconds, and the
    /// seconds left until the next.
    repeat: Option<(f32, f32)>,
}

#[derive(Clone, Debug, Default, PartialEq)]
enum Motion {
    #[default]
    None,
    /// Each particle's velocity, pixels a second; bouncing off the
    /// screen's edges, else wrapping around them.
    Particles {
        velocity: Vec<[f32; 3]>,
        bounce: bool,
    },
    /// A grid `wide` by `high` (child `y * wide + x`) scrolling at
    /// `velocity`, `seconds` into it.
    Tiles {
        wide: usize,
        high: usize,
        start: [f32; 2],
        spacing: [f32; 2],
        velocity: [f32; 2],
        seconds: f32,
    },
}

/// What a [`Builder`] keeps while it builds an animation.
#[derive(Debug, Default)]
pub(crate) struct Building {
    layers: Vec<Layer>,
    screen: [f32; 2],
    rng: Rng,
}

/// `random_up_to`'s generator: a xorshift seeded per animation.
#[derive(Debug, Default)]
struct Rng(u64);

impl Rng {
    fn seeded(name: &str) -> Rng {
        // FNV-1a of the name; never 0.
        let mut h: u64 = 0xcbf2_9ce4_8422_2325;
        for b in name.to_lowercase().bytes() {
            h = (h ^ u64::from(b)).wrapping_mul(0x100_0000_01b3);
        }
        Rng(h | 1)
    }

    fn next(&mut self) -> u64 {
        if self.0 == 0 {
            self.0 = 0x853c_49e6_748f_ea9b;
        }
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        self.0
    }

    /// `randomf(low, high)`.
    fn float(&mut self, low: f32, high: f32) -> f32 {
        let u = (self.next() >> 40) as f32 / (1u64 << 24) as f32;
        low + (high - low) * u
    }
}

/// Builds the animation of the song's folder `folder` (relative to the
/// song's folder of `skin`, a [`Skin::song`]) on StepMania's `screen`
/// (its width and height): the folder itself is not generic (its sprite
/// layers centered or stretched over the screen). What could not be built
/// is noted and left out; an animation that builds nothing draws nothing
/// (StepMania's empty `BGAnimation`).
pub fn load(
    skin: &Skin<'_>,
    folder: &str,
    screen: [f32; 2],
    textures: &mut Textures,
    images: &dyn Images,
) -> (Animation, Vec<String>) {
    let mut b = Builder {
        skin,
        textures,
        images,
        notes: Vec::new(),
        actors: 0,
        bga: Building {
            layers: Vec::new(),
            screen,
            rng: Rng::seeded(folder),
        },
    };
    let path = format!("{}{folder}", skin.dir_path(Dir::Folder(0)));
    let tree = match skin.resolve_path(&path) {
        Ok(f) if f.is_dir => b.animation_dir(&f, false, 0),
        Ok(f) => Err(format!("{} is not a folder", f.path)),
        Err(e) => Err(e),
    };
    let tree = tree.unwrap_or_else(|e| {
        b.note(format!("{folder}: {e}"));
        Actor::new(Kind::Frame(Vec::new()))
    });
    let mut a = Animation {
        tree,
        layers: b.bga.layers,
        screen,
    };
    // Tiles in their places before the first frame.
    a.step_layers(0.0);
    (a, b.notes)
}

/// Whether a folder of a song is a background animation of StepMania 3.9's
/// or ITG's formats rather than a script: it has no `default.lua`.
/// `files` are the song folder's files (relative to it), `folder` the
/// folder's path.
pub fn is_native<S: AsRef<str>>(files: &[S], folder: &str) -> bool {
    let lua = format!("{}/default.lua", folder.trim_end_matches('/')).to_lowercase();
    !files.iter().any(|f| f.as_ref().to_lowercase() == lua)
}

impl Animation {
    /// `Update` for `dt` seconds at `clocks`: the actors' tweens and
    /// effects, then each layer's motion (`BGAnimationLayer::Update`).
    pub fn update(&mut self, dt: f32, clocks: &Clocks) {
        let mut p = Problems::default();
        self.tree.update(dt, clocks, &mut p);
        self.step_layers(dt.max(0.0));
    }

    /// `GainFocus`, as a change shows it (`BGAnimationLayer::GainFocus`):
    /// each layer's actors' tweens stopped (but a repeating layer's), then
    /// `On` and `GainFocus` played on the whole tree.
    pub fn gain_focus(&mut self) {
        let layers = &self.layers;
        each_layer(&mut self.tree, 0, &mut |i, layer| {
            if layers.get(i).is_some_and(|l| l.repeat.is_none()) {
                for c in layer.children_mut() {
                    c.stop_tweening();
                }
            }
        });
        self.play("On");
        self.play("GainFocus");
    }

    /// Plays command `name` on the whole tree (`PlayCommand`).
    pub fn play(&mut self, name: &str) {
        self.tree.play(name, &mut Problems::default());
    }

    /// Each layer's motion for `dt` seconds.
    fn step_layers(&mut self, dt: f32) {
        let (layers, screen) = (&mut self.layers, self.screen);
        each_layer(&mut self.tree, 0, &mut |i, frame| {
            if let Some(layer) = layers.get_mut(i) {
                layer.step(frame, dt, screen);
            }
        });
    }
}

/// Calls `f` on every layer's frame in `a`, with its index.
fn each_layer(a: &mut Actor, depth: u32, f: &mut dyn FnMut(usize, &mut Actor)) {
    if depth > 64 {
        return;
    }
    if let Some(i) = a.name.strip_prefix(MARK).and_then(|n| n.parse().ok()) {
        f(i, a);
    }
    for c in a.children_mut() {
        each_layer(c, depth + 1, f);
    }
}

/// The size `a` is drawn at, zoomed (`GetZoomedWidth`, `GetZoomedHeight`).
fn zoomed(a: &Actor) -> [f32; 2] {
    let size = a.size();
    let s = a.current().scale;
    [
        (size[0] * s[0] * a.base_scale[0]).abs(),
        (size[1] * s[1] * a.base_scale[1]).abs(),
    ]
}

impl Layer {
    fn step(&mut self, frame: &mut Actor, dt: f32, [w, h]: [f32; 2]) {
        match &mut self.motion {
            Motion::None => {}
            Motion::Particles { velocity, bounce } => {
                for (a, v) in frame.children_mut().iter_mut().zip(velocity.iter_mut()) {
                    let mut p = a.current().pos;
                    for i in 0..3 {
                        p[i] += dt * v[i];
                    }
                    let [zw, zh] = zoomed(a);
                    // `ActorCollision.h`.
                    for (axis, size, edge) in [(0, zw, w), (1, zh, h)] {
                        let half = size / 2.0;
                        if *bounce {
                            if p[axis] < half {
                                v[axis] = -v[axis];
                                p[axis] = half;
                            }
                            if p[axis] > edge - half {
                                v[axis] = -v[axis];
                                p[axis] = edge - half;
                            }
                        } else {
                            if v[axis] < 0.0 && p[axis] < -half {
                                p[axis] = edge + half;
                            }
                            if v[axis] > 0.0 && p[axis] > edge + half {
                                p[axis] = -half;
                            }
                        }
                    }
                    a.set_position(p);
                }
            }
            Motion::Tiles {
                wide,
                high,
                start,
                spacing,
                velocity,
                seconds,
            } => {
                *seconds += dt;
                let total = [*wide as f32 * spacing[0], *high as f32 * spacing[1]];
                let children = frame.children_mut();
                for x in 0..*wide {
                    for y in 0..*high {
                        let Some(a) = children.get_mut(y * *wide + x) else {
                            continue;
                        };
                        let at = |i: usize, n: usize| {
                            let mut v = start[i] + spacing[i] * n as f32 + *seconds * velocity[i];
                            v += spacing[i] / 2.0;
                            if total[i] > 0.0 {
                                v %= total[i];
                                if v < 0.0 {
                                    v += total[i];
                                }
                            }
                            v - spacing[i] / 2.0
                        };
                        let z = a.current().pos[2];
                        a.set_position([at(0, x), at(1, y), z]);
                    }
                }
            }
        }
        if let Some((every, left)) = &mut self.repeat {
            *left -= dt;
            if *left <= 0.0 {
                frame.play("On", &mut Problems::default());
                *left = (*left + *every).rem_euclid(*every);
            }
        }
    }
}

/// Runs `commands` (StepMania 5's syntax, numbers only) on `a`.
/// An ini value read as 3.9 reads a `bool` (`atoi` not 0).
fn truth(v: &str) -> bool {
    string_to_int(v) != 0
}

/// The keys of a layer's section StepMania reads, as it spells them: an
/// ini's keys are found whatever their case here, and command names are
/// StepMania 3.9's, lowercased (`PlayCommand` lowercases too).
const KEYS: &[&str] = &[
    "AltText",
    "BaseRotationXDegrees",
    "BaseRotationYDegrees",
    "BaseRotationZDegrees",
    "BaseZoomX",
    "BaseZoomY",
    "BaseZoomZ",
    "Class",
    "CommandRepeatSeconds",
    "Cond",
    "Condition",
    "DrawCond",
    "FOV",
    "File",
    "Import",
    "Lighting",
    "Name",
    "NumParticles",
    "OverrideSpeed",
    "ParticlesBounce",
    "Player",
    "StartOnRandomFrame",
    "Stretch",
    "StretchTexCoordVelocityX",
    "StretchTexCoordVelocityY",
    "TexCoordVelocityX",
    "TexCoordVelocityY",
    "Text",
    "TileVelocityX",
    "TileVelocityY",
    "TilesSpacingX",
    "TilesSpacingY",
    "TilesStartX",
    "TilesStartY",
    "Type",
    "VelocityXMax",
    "VelocityXMin",
    "VelocityYMax",
    "VelocityYMin",
    "VelocityZMax",
    "VelocityZMin",
    "ZoomMax",
    "ZoomMin",
];

/// The commands the game plays, by their lowercased names.
const COMMANDS: &[&str] = &["On", "Off", "Init", "GainFocus", "LoseFocus"];

/// A key of an ini layer as [`KEYS`] spells it; a command's name as the
/// game plays it (`oncommand`: `OnCommand`).
fn canonical(key: &str) -> String {
    if let Some(k) = KEYS.iter().find(|k| k.eq_ignore_ascii_case(key)) {
        return k.to_string();
    }
    let lower = key.to_ascii_lowercase();
    if let Some(name) = lower.strip_suffix("command") {
        if name.is_empty() {
            return "Command".into();
        }
        if let Some(c) = COMMANDS.iter().find(|c| c.eq_ignore_ascii_case(name)) {
            return format!("{c}Command");
        }
        return format!("{}Command", &key[..name.len()]);
    }
    key.to_string()
}

/// Section `name` of `ini` as a node, its keys spelled as StepMania
/// reads them ([`canonical`]).
fn section_node(ini: &Ini, name: &str) -> Node {
    Node {
        name: name.to_string(),
        attrs: ini
            .section(name)
            .map(|(k, v)| (canonical(k), v.to_string()))
            .collect(),
        children: Vec::new(),
    }
}

/// `ini`'s value of `key` in `section`, whatever its case.
fn ini_value<'i>(ini: &'i Ini, section: &str, key: &str) -> Option<&'i str> {
    ini.section(section)
        .find(|(k, _)| k.eq_ignore_ascii_case(key))
        .map(|(_, v)| v)
}

/// What a 3.0 layer's file name makes it (`LoadFromAniLayerFile`'s
/// `EFFECT_STRING`, the last of the names in it).
const EFFECTS: &[&str] = &[
    "center",
    "stretchstill",
    "stretchscrollleft",
    "stretchscrollright",
    "stretchscrollup",
    "stretchscrolldown",
    "stretchwater",
    "stretchbubble",
    "stretchtwist",
    "stretchspin",
    "particlesspiralout",
    "particlesspiralin",
    "particlesfloatup",
    "particlesfloatdown",
    "particlesfloatleft",
    "particlesfloatright",
    "particlesbounce",
    "tilestill",
    "tilescrollleft",
    "tilescrollright",
    "tilescrollup",
    "tilescrolldown",
    "tileflipx",
    "tileflipy",
    "tilepulse",
];

/// The files a 3.0 folder's layers are (`GetDirListing` of each).
const LAYER_FILES: &[&str] = &["png", "jpg", "jpeg", "gif", "avi", "mpg", "mpeg", "sprite"];

/// What a layer's type is.
#[derive(Clone, Copy, PartialEq)]
enum Type {
    Sprite,
    Particles,
    Tiles,
}

impl Builder<'_, '_> {
    /// `MakeActor` of a song's folder: its `default.xml`, else its
    /// `BGAnimation.ini`'s layers, else a 3.0 folder of layers;
    /// `generic` when another file loads it.
    pub(crate) fn animation_dir(
        &mut self,
        dir: &FileRef,
        generic: bool,
        depth: u32,
    ) -> Result<Actor, String> {
        if depth > MAX_DEPTH {
            return Err(format!("{}: folders load each other too deep", dir.path));
        }
        let dir_path = format!("{}/", self.skin.file_path(dir));
        if let Some(xml) = self
            .skin
            .dir_has(dir.dir, &format!("{}/default.xml", dir.path))
        {
            let file = FileRef {
                path: xml,
                is_dir: false,
                virt: None,
                ..dir.clone()
            };
            let node = self.file_node(&file, None)?;
            return self.node_actor(&dir_path, &node, depth);
        }
        if let Some(ini) = self
            .skin
            .dir_has(dir.dir, &format!("{}/BGAnimation.ini", dir.path))
        {
            return self.ini_animation(dir, &ini, generic, depth);
        }
        self.old_layers(dir, depth)
    }

    /// The text of the ini `path` of `dir`'s folder, charged as read.
    fn ini_text(&mut self, dir: Dir, path: &str) -> Result<Ini, String> {
        let file = FileRef {
            dir,
            path: path.to_string(),
            virt: None,
            is_dir: false,
        };
        let skin = self.skin;
        let text = skin
            .file_text(&file)
            .ok_or_else(|| format!("{path} could not be read"))?;
        self.charge(1 + text.len() as u64 / 64)?;
        Ok(Ini::parse(text))
    }

    /// `BGAnimation::LoadFromAniDir` with a `BGAnimation.ini` (`ini`, a
    /// path in `dir`'s skin folder).
    fn ini_animation(
        &mut self,
        dir: &FileRef,
        ini: &str,
        generic: bool,
        depth: u32,
    ) -> Result<Actor, String> {
        let ini = self.ini_text(dir.dir, ini)?;
        let mut children = Vec::new();
        self.ini_layers(dir, &ini, generic, depth, &mut children)?;
        // OpenITG: a hidden actor sleeping `LengthSeconds` on `On`, for
        // the tween time a foreground lasts.
        if let Some(v) = ini_value(&ini, "BGAnimation", "LengthSeconds") {
            let mut a = Actor::new(Kind::Actor);
            a.visible = false;
            a.set_command(
                "On",
                parse(&format!("sleep,{}", string_to_float(v).max(0.0))).unwrap_or_default(),
            );
            children.push(a);
        }
        if ini_value(&ini, "BGAnimation", "UseScroller").is_some_and(truth) {
            self.note(format!("{}: a scroller is not drawn as one", dir.path));
        }
        let mut frame = Actor::new(Kind::Frame(children));
        if let Some(init) = ini_value(&ini, "BGAnimation", "InitCommand") {
            match parse_legacy(init) {
                Ok(calls) => {
                    let mut p = Problems::default();
                    for c in &calls {
                        if let Err(e) = frame.call(c, &mut p, 0) {
                            self.note(format!("{}: InitCommand: {e}", dir.path));
                        }
                    }
                }
                Err(e) => self.note(format!("{}: InitCommand: {e}", dir.path)),
            }
        }
        Ok(frame)
    }

    /// `AddLayersFromAniDir`: `ini`'s `Layer<N>` sections in order of
    /// their numbers, each a layer or a folder's layers imported.
    fn ini_layers(
        &mut self,
        dir: &FileRef,
        ini: &Ini,
        generic: bool,
        depth: u32,
        out: &mut Vec<Actor>,
    ) -> Result<(), String> {
        if depth > MAX_DEPTH {
            return Err(format!("{}: folders import each other too deep", dir.path));
        }
        if ini_value(ini, "BGAnimation", "Condition")
            .or_else(|| ini_value(ini, "BGAnimation", "Cond"))
            .is_some()
        {
            self.note(format!(
                "{}: a Condition is Lua; the animation is left out",
                dir.path
            ));
            return Ok(());
        }
        let dir_path = format!("{}/", self.skin.file_path(dir));
        let mut names: Vec<(i64, &str)> = ini
            .section_names()
            .filter_map(|n| {
                let digits = n.strip_prefix("Layer")?;
                let end = digits
                    .char_indices()
                    .find(|&(i, c)| !(c.is_ascii_digit() || (i == 0 && c == '-')))
                    .map_or(digits.len(), |(i, _)| i);
                Some((digits[..end].parse().ok()?, n))
            })
            .collect();
        names.sort_by_key(|&(n, _)| n);
        for (_, name) in names.into_iter().take(MAX_LAYERS) {
            let node = section_node(ini, name);
            let r = match node.attr("Import") {
                Some(import) => self.import(&dir_path, &node, import, generic, depth, out),
                None => self.ini_layer(&dir_path, &node, generic, depth).map(|a| {
                    out.extend(a);
                }),
            };
            if let Err(e) = r {
                self.note(format!("{}/BGAnimation.ini [{name}]: {e}", dir.path));
            }
        }
        Ok(())
    }

    /// `Import`: another folder's layers, as if they were this one's.
    fn import(
        &mut self,
        dir_path: &str,
        node: &Node,
        import: &str,
        generic: bool,
        depth: u32,
        out: &mut Vec<Actor>,
    ) -> Result<(), String> {
        if node.attr("Condition").is_some() {
            return Err("a Condition is Lua; the import is left out".into());
        }
        let found = self.skin.resolve_path(&format!("{dir_path}{import}"))?;
        if !found.is_dir {
            return Err(format!("{} is not a folder", found.path));
        }
        let ini = self
            .skin
            .dir_has(found.dir, &format!("{}/BGAnimation.ini", found.path))
            .ok_or_else(|| format!("{} has no BGAnimation.ini", found.path))?;
        let ini = self.ini_text(found.dir, &ini)?;
        self.ini_layers(&found, &ini, generic, depth + 1, out)
    }

    /// Registers `children` as a layer doing `motion`: its frame.
    fn layer(&mut self, children: Vec<Actor>, motion: Motion, repeat: Option<f32>) -> Actor {
        let mut frame = Actor::new(Kind::Frame(children));
        frame.name = format!("{MARK}{}", self.bga.layers.len());
        self.bga.layers.push(Layer {
            motion,
            repeat: repeat.map(|r| (r, r)),
        });
        frame
    }

    /// `BGAnimationLayer::LoadFromIni` (`LoadFromNode`): a layer of
    /// `node`'s type; `None` when it is left out.
    fn ini_layer(
        &mut self,
        dir_path: &str,
        node: &Node,
        generic: bool,
        depth: u32,
    ) -> Result<Option<Actor>, String> {
        if let Some(player) = node.attr("Player") {
            match string_to_int(player) {
                1 => {}
                // The second player is never enabled.
                2 => return Ok(None),
                _ => self.note(format!("{}: an invalid Player", node.name)),
            }
        }
        if node
            .attr("Cond")
            .or_else(|| node.attr("Condition"))
            .is_some()
        {
            return Err("a Condition is Lua; the layer is left out".into());
        }
        if node.attr("DrawCond").is_some() {
            self.note(format!(
                "{}: a DrawCond is Lua; the layer always draws",
                node.name
            ));
        }
        let mut stretch = node.attr("Stretch").is_some_and(truth);
        let ty = node.attr("Type").unwrap_or("sprite").to_ascii_lowercase();
        let ty = match ty.as_str() {
            "sprite" => Type::Sprite,
            "particles" => Type::Particles,
            "tiles" => Type::Tiles,
            other => match string_to_int(other) {
                1 => {
                    stretch = true;
                    Type::Sprite
                }
                2 => Type::Particles,
                3 => Type::Tiles,
                _ => Type::Sprite,
            },
        };
        let f = |k: &str, default: f32| node.attr(k).map_or(default, string_to_float);
        let fov = f("FOV", -1.0);
        let mut texcoord = [f("TexCoordVelocityX", 0.0), f("TexCoordVelocityY", 0.0)];
        texcoord[0] = f("StretchTexCoordVelocityX", texcoord[0]);
        texcoord[1] = f("StretchTexCoordVelocityY", texcoord[1]);
        let zoom = [f("ZoomMin", 1.0), f("ZoomMax", 1.0)];
        let velocity = [
            [f("VelocityXMin", 10.0), f("VelocityXMax", 10.0)],
            [f("VelocityYMin", 0.0), f("VelocityYMax", 0.0)],
            [f("VelocityZMin", 0.0), f("VelocityZMax", 0.0)],
        ];
        let repeat = Some(f("CommandRepeatSeconds", -1.0)).filter(|&r| r > 0.0);
        let [w, h] = self.bga.screen;
        let file = node.attr("File").unwrap_or("");
        if ["songbackground", "songbanner", "coursebanner"]
            .iter()
            .any(|s| file.eq_ignore_ascii_case(s))
        {
            return Err(format!(
                "{file}: the song's own art is not drawn in a layer here"
            ));
        }
        let mut children = Vec::new();
        let mut motion = Motion::None;
        match ty {
            Type::Sprite => {
                // The type is the layer's, not the actor's class.
                let mut actor_node = node.clone();
                actor_node.attrs.retain(|(k, _)| k != "Type");
                let mut a = self.node_actor(dir_path, &actor_node, depth)?;
                if !generic {
                    if stretch {
                        run(&mut a, &format!("stretchto,0,0,{w},{h}"));
                    } else {
                        run(&mut a, &format!("xy,{},{}", w / 2.0, h / 2.0));
                    }
                }
                if texcoord != [0.0; 2] {
                    run(
                        &mut a,
                        &format!("texcoordvelocity,{},{}", texcoord[0], texcoord[1]),
                    );
                }
                children.push(a);
            }
            Type::Particles => {
                let target = self.skin.resolve_path(&format!("{dir_path}{file}"))?;
                let mut proto = self.file_actor(&target, depth + 1)?;
                // The layer's commands, on each of its actors.
                self.load_node(&mut proto, node);
                let n = usize::try_from(node.attr("NumParticles").map_or(10, string_to_int))
                    .unwrap_or(0)
                    .min(MAX_LAYER_ACTORS);
                let speed = f("OverrideSpeed", 0.0);
                let mut velocities = Vec::new();
                for i in 0..n {
                    if i > 0 {
                        self.spend(0)?;
                    }
                    let mut a = proto.clone();
                    let rng = &mut self.bga.rng;
                    let (x, y) = (rng.float(0.0, w), rng.float(0.0, h));
                    let z = rng.float(zoom[0], zoom[1]);
                    run(&mut a, &format!("xy,{x},{y};zoom,{z}"));
                    let mut v = velocity.map(|[lo, hi]| rng.float(lo, hi));
                    if speed != 0.0 {
                        let len = v.iter().map(|c| c * c).sum::<f32>().sqrt();
                        if len > 0.0 {
                            v = v.map(|c| c / len * speed);
                        }
                    }
                    velocities.push(v);
                    children.push(a);
                }
                motion = Motion::Particles {
                    velocity: velocities,
                    bounce: node.attr("ParticlesBounce").is_some_and(truth),
                };
            }
            Type::Tiles => {
                let target = self.skin.resolve_path(&format!("{dir_path}{file}"))?;
                let mut proto = self.file_actor(&target, depth + 1)?;
                self.load_node(&mut proto, node);
                let size = proto.size();
                let spacing = [f("TilesSpacingX", -1.0), f("TilesSpacingY", -1.0)];
                let spacing = [0, 1].map(|i| {
                    if spacing[i] == -1.0 {
                        size[i]
                    } else {
                        spacing[i]
                    }
                });
                let (wide, high) = tiles(spacing, [w, h])?;
                for i in 0..wide * high {
                    if i > 0 {
                        self.spend(0)?;
                    }
                    let mut a = proto.clone();
                    let z = self.bga.rng.float(zoom[0], zoom[1]);
                    run(&mut a, &format!("zoom,{z}"));
                    children.push(a);
                }
                motion = Motion::Tiles {
                    wide,
                    high,
                    start: [f("TilesStartX", 0.0), f("TilesStartY", 0.0)],
                    spacing,
                    velocity: [f("TileVelocityX", 0.0), f("TileVelocityY", 0.0)],
                    seconds: 0.0,
                };
            }
        }
        if node.attr("StartOnRandomFrame").is_some_and(truth) {
            self.random_frames(&mut children);
        }
        let mut layer = self.layer(children, motion, repeat);
        if fov != -1.0 {
            layer.fov = fov;
            layer.vanish = Some([w / 2.0, h / 2.0]);
        }
        Ok(Some(layer))
    }

    /// `SetState(rand() % GetNumStates())` on each of `actors`.
    fn random_frames(&mut self, actors: &mut [Actor]) {
        for a in actors {
            if let Kind::Sprite(s) = &mut a.kind {
                let n = s.states.len().max(1) as u64;
                s.set_state((self.bga.rng.next() % n) as i64);
            }
        }
    }

    /// A 3.0 folder without an ini: each image, `.sprite` or movie in it
    /// (sorted, those whose names start with `_` skipped) a layer.
    fn old_layers(&mut self, dir: &FileRef, depth: u32) -> Result<Actor, String> {
        let mut children = Vec::new();
        for file in self.skin.folder_files(dir.dir, &dir.path) {
            let name = file.name();
            if name.starts_with('_')
                || !LAYER_FILES
                    .iter()
                    .any(|e| file.extension().eq_ignore_ascii_case(e))
            {
                continue;
            }
            match self.layer_file(&file, depth) {
                Ok(a) => children.push(a),
                Err(e) => self.note(format!("{}: {e}", file.path)),
            }
        }
        Ok(Actor::new(Kind::Frame(children)))
    }

    /// `BGAnimationLayer::LoadFromAniLayerFile`: a layer of `file`, its
    /// effect and hints from its path.
    fn layer_file(&mut self, file: &FileRef, depth: u32) -> Result<Actor, String> {
        let hint = file.path.to_lowercase();
        if hint.contains("usesongbg") {
            return Err("the song's background is not drawn in a layer here".into());
        }
        let effect = EFFECTS
            .iter()
            .rposition(|e| hint.contains(e))
            .map_or("center", |i| EFFECTS[i]);
        let mut proto = self.file_actor(file, depth + 1)?;
        let [w, h] = self.bga.screen;
        let mut children = Vec::new();
        let mut motion = Motion::None;
        if effect == "center" {
            run(&mut proto, &format!("xy,{},{}", w / 2.0, h / 2.0));
            children.push(proto);
        } else if effect == "stretchspin" {
            run(
                &mut proto,
                &format!(
                    "scaletocover,-200,-200,{},{};spin;effectmagnitude,0,0,60",
                    w + 200.0,
                    h + 200.0
                ),
            );
            children.push(proto);
        } else if effect.starts_with("stretch") {
            run(
                &mut proto,
                &format!("stretchto,0,0,{w},{h};customtexturerect,0,0,1,1"),
            );
            let v = match effect {
                "stretchscrollleft" => [0.5, 0.0],
                "stretchscrollright" => [-0.5, 0.0],
                "stretchscrollup" => [0.0, 0.5],
                "stretchscrolldown" => [0.0, -0.5],
                _ => [0.0; 2],
            };
            if v != [0.0; 2] {
                run(&mut proto, &format!("texcoordvelocity,{},{}", v[0], v[1]));
            }
            children.push(proto);
        } else if effect.starts_with("particles") {
            let [sw, sh] = proto.size();
            let area = (sw * sh) as i64;
            if area <= 0 {
                return Err("particles of no size".into());
            }
            let max = ((w / 32.0 + 2.0) as i64) * ((h / 32.0 + 2.0) as i64);
            let n = ((w * h) as i64 / area)
                .min(max)
                .clamp(0, MAX_LAYER_ACTORS as i64) as usize;
            let bounce = effect == "particlesbounce";
            let mut velocities = Vec::new();
            for i in 0..n {
                if i > 0 {
                    self.spend(0)?;
                }
                let mut a = proto.clone();
                let zoom = if bounce {
                    1.0
                } else {
                    0.7 + 0.6 * i as f32 / n as f32
                };
                let (hw, hh) = (sw * zoom / 2.0, sh * zoom / 2.0);
                let rng = &mut self.bga.rng;
                let (x, y) = (rng.float(hw, w - hw), rng.float(hh, h - hh));
                run(&mut a, &format!("zoom,{zoom};xy,{x},{y}"));
                let speed = PARTICLE_SPEED * zoom;
                velocities.push(match effect {
                    "particlesfloatup" | "particlesspiralout" => [0.0, -speed, 0.0],
                    "particlesfloatdown" | "particlesspiralin" => [0.0, speed, 0.0],
                    "particlesfloatleft" => [-speed, 0.0, 0.0],
                    "particlesfloatright" => [speed, 0.0, 0.0],
                    // `randomf()`'s direction, normalized: a pixel a second.
                    _ => {
                        let v = [rng.float(-1.0, 1.0), rng.float(-1.0, 1.0)];
                        let len = (v[0] * v[0] + v[1] * v[1]).sqrt();
                        if len > 0.0 {
                            [v[0] / len, v[1] / len, 0.0]
                        } else {
                            [0.0; 3]
                        }
                    }
                });
                children.push(a);
            }
            motion = Motion::Particles {
                velocity: velocities,
                bounce,
            };
        } else {
            let size = proto.size();
            if !(size[0] > 0.0 && size[1] > 0.0) {
                return Err("tiles of no size".into());
            }
            // `MAX_TILES_WIDE`, `MAX_TILES_HIGH`.
            let wide = (2 + (w / size[0]) as usize).min((w / 32.0 + 2.0) as usize);
            let high = (2 + (h / size[1]) as usize).min((h / 32.0 + 2.0) as usize);
            let effect_commands = match effect {
                "tileflipx" => "spin;effectmagnitude,2,0,0",
                "tileflipy" => "spin;effectmagnitude,0,2,0",
                "tilepulse" => "pulse;effectperiod,1;effectmagnitude,0.3,1,10",
                _ => "",
            };
            run(&mut proto, effect_commands);
            for i in 0..wide * high {
                if i > 0 {
                    self.spend(0)?;
                }
                children.push(proto.clone());
            }
            let velocity = match effect {
                "tilescrollleft" => [-PARTICLE_SPEED, 0.0],
                "tilescrollright" => [PARTICLE_SPEED, 0.0],
                "tilescrollup" => [0.0, -PARTICLE_SPEED],
                "tilescrolldown" => [0.0, PARTICLE_SPEED],
                _ => [0.0; 2],
            };
            motion = Motion::Tiles {
                wide,
                high,
                start: [size[0] / 2.0, size[1] / 2.0],
                spacing: size,
                velocity,
                seconds: 0.0,
            };
        }
        if hint.contains("cyclecolor") {
            for a in &mut children {
                run(a, "rainbow;effectperiod,5");
            }
        }
        if hint.contains("cyclealpha") {
            for a in &mut children {
                run(
                    a,
                    "diffuseshift;effectperiod,2;effectcolor1,1,1,1,1;effectcolor2,1,1,1,0",
                );
            }
        }
        if hint.contains("startonrandomframe") {
            self.random_frames(&mut children);
        }
        if hint.contains("dontanimate") {
            for a in &mut children {
                run(a, "animate,0");
            }
        }
        if hint.contains("add") {
            for a in &mut children {
                a.blend = Blend::Add;
            }
        }
        Ok(self.layer(children, motion, None))
    }
}

/// How many tiles `spacing` apart cover a screen `[w, h]` with a tile to
/// spare each side (`2 + SCREEN_WIDTH / spacing`).
fn tiles(spacing: [f32; 2], [w, h]: [f32; 2]) -> Result<(usize, usize), String> {
    if !(spacing[0] > 0.0 && spacing[1] > 0.0) {
        return Err("tiles of no size".into());
    }
    let wide = 2 + (w / spacing[0]) as usize;
    let high = 2 + (h / spacing[1]) as usize;
    if wide.saturating_mul(high) > MAX_LAYER_ACTORS {
        return Err(format!("more than {MAX_LAYER_ACTORS} tiles"));
    }
    Ok((wide, high))
}

#[cfg(test)]
mod tests;
