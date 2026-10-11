//! An actor table, as `Load()` returns it, turned into native actors:
//! `XmlFileUtil::XNodeFromTable` (array entries first, in order, then the
//! other table entries), `ActorUtil::LoadFromNode` (`Condition`, `Class` or
//! `Type`), `Actor::LoadFromNode` (`Name`, `Base*`, `*Command`),
//! `Sprite::LoadFromNode` (`Texture`, `Frames`, `FrameNNNN`/`DelayNNNN`)
//! and `ActorFrame::LoadChildrenFromNode` (`children`, else the array).
//! Each actor gets an id, a userdata and its commands in the VM's
//! registries. The work is bounded by the caps below, not by fuel.

use ddi_lua::Failure;
use std::collections::HashMap;

use ddi_lua::piccolo::{Context, Table, Value};

use super::host::actor_userdata;
use super::{Host, MAX_ACTORS, Registries};
use crate::actor::{Actor, Kind, Sprite, State};
use crate::build::{Images, Textures, builtin_sprite, image_sprite};
use crate::load::Source;

/// Entries read from one table.
const MAX_ENTRIES: usize = 10_000;
/// Table entries and frames read for one element, over all its tables
/// (the converter runs outside the VM's fuel: this bounds its time).
const MAX_VISITS: usize = 200_000;
/// Actors nested in one element.
const MAX_DEPTH: usize = 32;
/// States of one sprite.
const MAX_STATES: usize = 4096;
/// Bytes of a name, path or source read from a table (longer ones are
/// cut: no skin needs them, and they are copied per actor).
const MAX_TEXT: usize = 4096;

pub(crate) struct Converted {
    /// The tree's index in the stage and its root's id; `None` when its
    /// `Condition` is false.
    pub root: Option<(usize, u32)>,
    /// Something in it is not drawn here (a model, text, a movie): the
    /// element is the built-in skin's.
    pub unsupported: bool,
    pub notes: Vec<String>,
}

struct Convert<'a, 'gc> {
    ctx: Context<'gc>,
    host: &'a mut Host,
    actors: Table<'gc>,
    commands: Table<'gc>,
    children: Table<'gc>,
    metatables: [Table<'gc>; 4],
    textures: &'a mut Textures,
    images: &'a dyn Images,
    root: usize,
    unsupported: bool,
    notes: Vec<String>,
    visits: usize,
    /// Tables read so far for this element, by identity: a table used in
    /// many places is read once.
    read: HashMap<Table<'gc>, Vec<(Value<'gc>, Value<'gc>)>>,
    /// The tables from the root to the current one (`XNodeFromTable`
    /// skips a table already on the way, `ProcessedTables`).
    path_tables: Vec<Table<'gc>>,
}

pub(crate) fn element<'gc>(
    ctx: Context<'gc>,
    host: &mut Host,
    registries: &Registries,
    textures: &mut Textures,
    images: &dyn Images,
    t: Table<'gc>,
) -> Result<Converted, Failure> {
    let root = host.stage.next_root();
    let mut c = Convert {
        ctx,
        host,
        actors: ctx.fetch(&registries.actors),
        commands: ctx.fetch(&registries.commands),
        children: ctx.fetch(&registries.children),
        metatables: [0, 1, 2, 3].map(|i| ctx.fetch(&registries.metatables[i])),
        textures,
        images,
        root,
        unsupported: false,
        notes: Vec::new(),
        visits: 0,
        read: HashMap::new(),
        path_tables: Vec::new(),
    };
    let tree = c.node(t, Vec::new(), 0)?;
    let (unsupported, notes) = (c.unsupported, c.notes);
    Ok(Converted {
        root: tree.map(|a| {
            let id = a.id;
            (host.stage.add(a), id)
        }),
        unsupported,
        notes,
    })
}

/// A string or number attribute as text, at most [`MAX_TEXT`] bytes.
fn text(v: Value<'_>) -> Option<String> {
    match v {
        Value::String(s) => {
            let b = s.as_bytes();
            Some(String::from_utf8_lossy(b.get(..MAX_TEXT).unwrap_or(b)).into_owned())
        }
        Value::Integer(_) | Value::Number(_) => Some(v.display().to_string()),
        _ => None,
    }
}

/// Whether a string is longer than [`MAX_TEXT`].
fn too_long(v: Value<'_>) -> bool {
    matches!(v, Value::String(s) if s.as_bytes().len() > MAX_TEXT)
}

impl<'gc> Convert<'_, 'gc> {
    fn note(&mut self, n: String) {
        if self.notes.len() < 32 && !self.notes.contains(&n) {
            self.notes.push(n);
        }
    }

    fn get(&self, t: Table<'gc>, key: &'static str) -> Value<'gc> {
        t.get_value(self.ctx, key)
    }

    /// The entries of `t`: the array from 1 on, then the rest (as
    /// `FOREACH_LUATABLEI` then `FOREACH_LUATABLE`).
    fn visit(&mut self, n: usize) -> Result<(), Failure> {
        self.visits += n;
        if self.visits > MAX_VISITS {
            return Err(Failure::Error(
                "an element's actor tables are too large".into(),
            ));
        }
        Ok(())
    }

    fn entries(&mut self, t: Table<'gc>) -> Result<Vec<(Value<'gc>, Value<'gc>)>, Failure> {
        if let Some(e) = self.read.get(&t) {
            let e = e.clone();
            self.visit(e.len())?;
            return Ok(e);
        }
        let e = self.read_entries(t)?;
        self.read.insert(t, e.clone());
        Ok(e)
    }

    fn read_entries(&mut self, t: Table<'gc>) -> Result<Vec<(Value<'gc>, Value<'gc>)>, Failure> {
        let mut out = Vec::new();
        let mut i = 1i64;
        loop {
            let v = t.get_value(self.ctx, i);
            if v.is_nil() {
                break;
            }
            out.push((Value::Integer(i), v));
            self.visit(1)?;
            i += 1;
            if out.len() > MAX_ENTRIES {
                return Err(Failure::Error("an actor table has too many entries".into()));
            }
        }
        let array = i - 1;
        for (k, v) in t.iter() {
            let in_array = match k {
                Value::Integer(n) => (1..=array).contains(&n),
                Value::Number(n) => n >= 1.0 && n <= array as f64 && n.fract() == 0.0,
                _ => false,
            };
            if in_array {
                continue;
            }
            out.push((k, v));
            self.visit(1)?;
            if out.len() > MAX_ENTRIES {
                return Err(Failure::Error("an actor table has too many entries".into()));
            }
        }
        Ok(out)
    }

    fn node(
        &mut self,
        t: Table<'gc>,
        path: Vec<u16>,
        depth: usize,
    ) -> Result<Option<Actor>, Failure> {
        if depth > MAX_DEPTH {
            return Err(Failure::Error("actors are nested too deeply".into()));
        }
        if self.path_tables.contains(&t) {
            return Ok(None);
        }
        // `Condition`: present and false, the actor is not made.
        if matches!(self.get(t, "Condition"), Value::Boolean(false)) {
            return Ok(None);
        }
        let id = self.host.stage.next_id();
        if id as usize > MAX_ACTORS {
            return Err(Failure::Error(format!(
                "the skin makes more than {MAX_ACTORS} actors"
            )));
        }
        self.host.stage.place(id, self.root, path.clone());
        let class = text(self.get(t, "Class"))
            .or_else(|| text(self.get(t, "Type")))
            .unwrap_or_default();
        // For notes only: short (any table may set `_Source`).
        let where_: String = text(self.get(t, "_Source"))
            .map(|s| s.trim_start_matches('@').chars().take(200).collect())
            .unwrap_or_default();
        let entries = self.entries(t)?;

        let (mut actor, class_index) = match class.as_str() {
            "Actor" => (Actor::new(Kind::Actor), 0),
            "Sprite" => (self.sprite(t, &where_)?, 1),
            "Quad" => {
                // A plain rectangle: the renderer's white.
                let mut a = builtin_sprite(self.textures, "_quad.png");
                if let Kind::Sprite(s) = &mut a.kind {
                    s.size = [1.0, 1.0];
                }
                (a, 1)
            }
            "ActorFrame" => (Actor::new(Kind::Frame(Vec::new())), 2),
            "Model" => {
                self.unsupported = true;
                self.note(format!("{where_}: a 3D model"));
                (Actor::new(Kind::Actor), 3)
            }
            "BitmapText" | "ActorMultiVertex" | "ActorFrameTexture" | "ActorProxy"
            | "BGAnimation" | "ActorScroller" | "RollingNumbers" => {
                self.unsupported = true;
                self.note(format!("{where_}: {class} is not drawn here"));
                (Actor::new(Kind::Actor), 0)
            }
            "Sound" => (Actor::new(Kind::Actor), 0),
            other => {
                // StepMania reports it and makes a dummy actor.
                self.note(format!("{where_}: invalid Class \"{other}\""));
                (Actor::new(Kind::Actor), 0)
            }
        };
        actor.id = id;

        let ud = actor_userdata(self.ctx, id, self.metatables[class_index]);
        let _ = self.actors.set(self.ctx, i64::from(id), ud);

        // Attributes.
        let mut commands: Option<Table<'gc>> = None;
        for &(k, v) in &entries {
            let Value::String(key) = k else { continue };
            let key = key.display_lossy().to_string();
            if let Some(name) = key.strip_suffix("Command") {
                match v {
                    Value::Function(f) => {
                        let table = *commands.get_or_insert_with(|| {
                            let t = Table::new(&self.ctx);
                            let _ = self.commands.set(self.ctx, i64::from(id), t);
                            t
                        });
                        let _ = table.set(self.ctx, self.ctx.intern(name.as_bytes()), f);
                    }
                    _ => self.note(format!("{where_}: {key} is not a function")),
                }
                continue;
            }
            // `GetValue<float>`: 0 for what is not a number.
            let num = || v.to_number().map_or(0.0, |n| n as f32);
            match key.as_str() {
                "Name" => {
                    let name = text(v).unwrap_or_default();
                    self.visit(name.len() / 64)?;
                    actor.name = name;
                }
                "BaseRotationX" => actor.base_rotation[0] = num(),
                "BaseRotationY" => actor.base_rotation[1] = num(),
                "BaseRotationZ" => actor.base_rotation[2] = num(),
                "BaseZoomX" => actor.base_scale[0] = num(),
                "BaseZoomY" => actor.base_scale[1] = num(),
                "BaseZoomZ" => actor.base_scale[2] = num(),
                _ => {}
            }
        }

        // Children of frames: `children`'s tables, else the array's.
        if let Kind::Frame(kids) = &mut actor.kind {
            let mut list = Vec::new();
            match self.get(t, "children") {
                Value::Table(c) => {
                    for (_, v) in self.entries(c)? {
                        if let Value::Table(child) = v {
                            list.push(child);
                        }
                    }
                }
                _ => {
                    for &(k, v) in &entries {
                        if let (Value::Integer(_), Value::Table(child)) = (k, v) {
                            list.push(child);
                        }
                    }
                }
            }
            let mut built = Vec::new();
            let ids = Table::new(&self.ctx);
            self.path_tables.push(t);
            for child in list {
                let mut p = path.clone();
                p.push(u16::try_from(built.len()).unwrap_or(u16::MAX));
                if let Some(a) = self.node(child, p, depth + 1)? {
                    let _ = ids.set(self.ctx, built.len() as i64 + 1, i64::from(a.id));
                    built.push(a);
                }
            }
            self.path_tables.pop();
            if !built.is_empty() {
                let _ = self.children.set(self.ctx, i64::from(id), ids);
            }
            *kids = built;
        }
        Ok(Some(actor))
    }

    /// A `Sprite`'s texture and states.
    fn sprite(&mut self, t: Table<'gc>, where_: &str) -> Result<Actor, Failure> {
        let value = self.get(t, "Texture");
        if too_long(value) {
            self.note(format!("{where_}: the texture path is too long"));
            return Ok(Actor::new(Kind::Actor));
        }
        let Some(texture) = text(value).filter(|s| !s.is_empty()) else {
            // No texture: draws nothing.
            return Ok(Actor::new(Kind::Actor));
        };
        self.visit(texture.len() / 64)?;
        // `ActorUtil::GetAttrPath`: relative to the file that wrote it.
        let path = if texture.starts_with('/') {
            texture.clone()
        } else {
            match text(self.get(t, "_Dir")) {
                Some(dir) => format!("{dir}{texture}"),
                None => {
                    self.note(format!(
                        "{where_}: relative path \"{texture}\", but path is unknown"
                    ));
                    return Ok(Actor::new(Kind::Actor));
                }
            }
        };
        let file = match self.host.skin.resolve_path(&path) {
            Ok(f) => f,
            Err(e) => {
                self.note(format!("{where_}: {e}"));
                return Ok(Actor::new(Kind::Actor));
            }
        };
        let source = self.host.skin.classify(file);
        let mut actor = match &source {
            Source::Image(f) => {
                match image_sprite(&self.host.skin, self.textures, self.images, f) {
                    Some(a) => a,
                    None => {
                        self.note(format!("{where_}: {} could not be read", f.path));
                        self.unsupported = true;
                        return Ok(Actor::new(Kind::Actor));
                    }
                }
            }
            Source::BuiltInArt(f) => builtin_sprite(self.textures, f.name()),
            Source::Movie(f) => {
                self.note(format!("{where_}: {} is a movie", f.path));
                self.unsupported = true;
                return Ok(Actor::new(Kind::Actor));
            }
            other => {
                self.note(format!("{where_}: {} is not a texture", other.file().path));
                return Ok(Actor::new(Kind::Actor));
            }
        };
        if let Kind::Sprite(sprite) = &mut actor.kind {
            self.states(t, sprite, where_)?;
        }
        Ok(actor)
    }

    /// `Frames = { { Frame =, Delay =, {x=,y=}, {x=,y=} }, … }`, else
    /// `FrameNNNN`/`DelayNNNN` pairs; the texture's frames otherwise.
    fn states(&mut self, t: Table<'gc>, sprite: &mut Sprite, where_: &str) -> Result<(), Failure> {
        let states = match self.get(t, "Frames") {
            Value::Table(list) => {
                let states = frames(self.ctx, list, sprite, where_, &mut self.notes)?;
                self.visit(states.len())?;
                states
            }
            _ => {
                let mut states = Vec::new();
                for i in 0..MAX_STATES {
                    let fk = format!("Frame{i:04}");
                    let dk = format!("Delay{i:04}");
                    let Some(frame) = t
                        .get_value(self.ctx, self.ctx.intern(fk.as_bytes()))
                        .to_number()
                    else {
                        break;
                    };
                    let rect = frame_rect(sprite, frame as i64, where_, &mut self.notes);
                    let Some(delay) = t
                        .get_value(self.ctx, self.ctx.intern(dk.as_bytes()))
                        .to_number()
                    else {
                        break;
                    };
                    states.push(State {
                        rect,
                        delay: delay as f32,
                    });
                    self.visit(1)?;
                }
                states
            }
        };
        set_states(sprite, states);
        Ok(())
    }
}

/// The texture's frame `i` (StepMania reports one past the end).
fn frame_rect(sprite: &Sprite, i: i64, where_: &str, notes: &mut Vec<String>) -> [f32; 4] {
    match usize::try_from(i).ok().and_then(|i| sprite.frames.get(i)) {
        Some(r) => *r,
        None => {
            let n = format!(
                "{where_}: frame {i}, but the texture only has {} frames",
                sprite.frames.len()
            );
            if notes.len() < 32 && !notes.contains(&n) {
                notes.push(n);
            }
            sprite
                .frames
                .last()
                .copied()
                .unwrap_or([0.0, 0.0, 1.0, 1.0])
        }
    }
}

/// A `Frames` list (`Sprite::LoadFromNode`, `SetStateProperties`): each
/// entry's `Frame` (else the previous one's), `Delay` (else 0.1) and two
/// optional points that take part of the frame.
pub(crate) fn frames<'gc>(
    ctx: Context<'gc>,
    list: Table<'gc>,
    sprite: &Sprite,
    where_: &str,
    notes: &mut Vec<String>,
) -> Result<Vec<State>, Failure> {
    let mut states = Vec::new();
    let mut index = 0i64;
    for i in 1i64.. {
        let Value::Table(f) = list.get_value(ctx, i) else {
            break;
        };
        if states.len() >= MAX_STATES {
            return Err(Failure::Error("a sprite has too many states".into()));
        }
        let delay = f
            .get_value(ctx, "Delay")
            .to_number()
            .map_or(0.1, |d| d as f32);
        if let Some(n) = f.get_value(ctx, "Frame").to_number() {
            index = n as i64;
        }
        let mut rect = frame_rect(sprite, index, where_, notes);
        if let (Value::Table(a), Value::Table(b)) = (f.get_value(ctx, 1i64), f.get_value(ctx, 2i64))
        {
            let r = rect;
            let point = |p: Table<'gc>| {
                let x = p.get_value(ctx, "x").to_number().unwrap_or(1.0) as f32;
                let y = p.get_value(ctx, "y").to_number().unwrap_or(1.0) as f32;
                (r[0] + x * (r[2] - r[0]), r[1] + y * (r[3] - r[1]))
            };
            let (l, tp) = point(a);
            let (rt, bm) = point(b);
            rect = [l, tp, rt, bm];
        }
        states.push(State { rect, delay });
    }
    Ok(states)
}

/// New states, the size from the first one's rectangle (`m_size`).
pub(crate) fn set_states(sprite: &mut Sprite, states: Vec<State>) {
    let Some(first) = states.first() else {
        return;
    };
    if let Some(full) = sprite.frames.first() {
        let w = (full[2] - full[0]).abs().max(f32::EPSILON);
        let h = (full[3] - full[1]).abs().max(f32::EPSILON);
        let frame = sprite.frame_size;
        sprite.size = [
            frame[0] * (first.rect[2] - first.rect[0]).abs() / w,
            frame[1] * (first.rect[3] - first.rect[1]).abs() / h,
        ];
    }
    sprite.states = states;
    sprite.set_state(0);
}
