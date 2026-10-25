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

use ddi_actor::{Actor, Kind, Sprite, State};
use ddi_lua::lunita::{Context, Table, Value};

use super::host::actor_userdata;
use super::{Host, MAX_ACTORS, Registries};
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

/// How a song's scripts' `BitmapText` is set, whatever font it names:
/// StepMania's `_fallback` `Common Normal` is Open Sans Semibold at 24
/// pixels a line; here the platform's sans-serif face, 20 pixels (a line
/// 24 apart, as the shells draw runs: `ddi_session::scripted::TEXT_SIZE`).
pub fn song_text_style() -> ddi_actor::text::TextStyle {
    let mut style = ddi_actor::text::TextStyle::new("sans-serif", 20.0);
    style.weight = 600;
    style
}

pub(crate) struct Converted {
    /// The tree's index in the stage and its root's id; `None` when its
    /// `Condition` is false.
    pub root: Option<(usize, u32)>,
    /// Something in it is not drawn here (text, a movie, a model that did
    /// not load): the element is the built-in skin's.
    pub unsupported: bool,
    pub notes: Vec<String>,
    /// A `Def.Component` instead of actors (`crate::component`).
    pub component: Option<crate::component::Component>,
    /// A `Def.LongNote` (a hold's or roll's body element).
    pub long_note: Option<crate::component::LongNote>,
    /// A `Def.Lift` (a lift's element).
    pub lift: Option<crate::component::Lift>,
}

struct Convert<'a, 'gc> {
    ctx: Context<'gc>,
    host: &'a mut Host,
    actors: Table<'gc>,
    commands: Table<'gc>,
    children: Table<'gc>,
    metatables: [Table<'gc>; 5],
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
        metatables: [0, 1, 2, 3, 4].map(|i| ctx.fetch(&registries.metatables[i])),
        textures,
        images,
        root,
        unsupported: false,
        notes: Vec::new(),
        visits: 0,
        read: HashMap::new(),
        path_tables: Vec::new(),
    };
    match text(c.get(t, "Class")).as_deref() {
        Some("Component") => {
            let component = c.component(t)?;
            return Ok(Converted {
                root: None,
                unsupported: false,
                notes: c.notes,
                component: Some(component),
                long_note: None,
                lift: None,
            });
        }
        Some("LongNote") => {
            let long_note = c.long_note(t)?;
            return Ok(Converted {
                root: None,
                unsupported: false,
                notes: c.notes,
                component: None,
                long_note: Some(long_note),
                lift: None,
            });
        }
        Some("Lift") => {
            let lift = c.lift(t)?;
            return Ok(Converted {
                root: None,
                unsupported: false,
                notes: c.notes,
                component: None,
                long_note: None,
                lift: Some(lift),
            });
        }
        _ => {}
    }
    let tree = c.node(t, Vec::new(), 0)?;
    let (unsupported, notes) = (c.unsupported, c.notes);
    Ok(Converted {
        root: tree.map(|a| {
            let id = a.id;
            (host.stage.add(a), id)
        }),
        unsupported,
        notes,
        component: None,
        long_note: None,
        lift: None,
    })
}

/// `#rgb`, `#rrggbb` or `#rrggbbaa` as sRGB-encoded values, 0 to 1.
fn hex_color(hex: &str) -> Option<[f32; 4]> {
    let h = hex.strip_prefix('#')?;
    let digit = |i: usize, n: usize| u8::from_str_radix(h.get(i..i + n)?, 16).ok();
    let (r, g, b, a) = match h.len() {
        3 => (digit(0, 1)? * 17, digit(1, 1)? * 17, digit(2, 1)? * 17, 255),
        6 => (digit(0, 2)?, digit(2, 2)?, digit(4, 2)?, 255),
        8 => (digit(0, 2)?, digit(2, 2)?, digit(4, 2)?, digit(6, 2)?),
        _ => return None,
    };
    Some([r, g, b, a].map(|v| f32::from(v) / 255.0))
}

/// A number (or a boolean as 1 or 0).
fn number(v: Value<'_>) -> Option<f32> {
    match v {
        Value::Integer(n) => Some(n as f32),
        Value::Number(n) if n.is_finite() => Some(n as f32),
        Value::Boolean(b) => Some(if b { 1.0 } else { 0.0 }),
        _ => None,
    }
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
    /// A `Def.Component`: its SVG's component, each part's images and the
    /// properties and bindings `Parts` gives by part name. While the
    /// shell probes, the file is asked for and nothing drawn.
    fn component(&mut self, t: Table<'gc>) -> Result<crate::component::Component, Failure> {
        let id = text(self.get(t, "Id")).unwrap_or_default();
        self.component_of(t, id, true)
    }

    /// Where table `t` was written, for notes (its `_Source`, short).
    fn source(&self, t: Table<'gc>) -> String {
        text(self.get(t, "_Source"))
            .map(|s| s.trim_start_matches('@').chars().take(200).collect())
            .unwrap_or_default()
    }

    /// The part a long note's or a lift's `Material` paints its band
    /// with, its `Phase` a binding or a number; none without a material.
    fn fill(
        &mut self,
        t: Table<'gc>,
        where_: &str,
    ) -> Result<Option<crate::component::Part>, Failure> {
        use crate::component::{Part, Property};
        let Value::Table(m) = self.get(t, "Material") else {
            return Ok(None);
        };
        let Some(material) = self.material(m, where_)? else {
            return Ok(None);
        };
        let mut part = Part::new(None, None, Vec::new());
        part.material = Some(Box::new(material));
        match self.get(t, "Phase") {
            Value::Table(b) => match self.binding(b, Property::Phase)? {
                Ok(b) => part.bindings.push(b),
                Err(e) => self.note(format!("{where_}: Phase: {e}")),
            },
            v => {
                if let Some(n) = number(v) {
                    part.set(Property::Phase, n);
                }
            }
        }
        Ok(Some(part))
    }

    /// A `Def.LongNote`: its head's, body's and cap's components (the
    /// `Parts` table naming parts of any of them), its pitch and join, and
    /// the material painting its band.
    fn long_note(&mut self, t: Table<'gc>) -> Result<crate::component::LongNote, Failure> {
        use crate::component::{Anchor, LongNote};
        let where_: String = self.source(t);
        let id = |c: &Self, key: &'static str| text(c.get(t, key)).filter(|s| !s.is_empty());
        let (head, body, cap) = (id(self, "Head"), id(self, "Body"), id(self, "Cap"));
        let body_id = body.unwrap_or_else(|| {
            self.note(format!("{where_}: a LongNote needs a Body"));
            String::new()
        });
        let body = self.component_of(t, body_id, false)?;
        let head = match head {
            Some(h) => Some(self.component_of(t, h, false)?),
            None => None,
        };
        let cap = match cap {
            Some(c) => Some(self.component_of(t, c, false)?),
            None => None,
        };
        // A part `Parts` names that none of them has.
        if let Value::Table(parts) = self.get(t, "Parts") {
            for (k, _) in self.entries(parts)? {
                let Some(name) = text(k) else { continue };
                let known = [Some(&body), head.as_ref(), cap.as_ref()]
                    .into_iter()
                    .flatten()
                    .any(|c| {
                        c.parts
                            .iter()
                            .any(|p| p.name.as_deref() == Some(name.as_str()))
                    });
                if !known && !body.parts.is_empty() {
                    self.note(format!("{where_}: the long note has no part \"{name}\""));
                }
            }
        }
        let pitch = number(self.get(t, "Pitch"))
            .filter(|p| *p >= 1.0)
            .unwrap_or_else(|| body.view_box[3].max(1.0));
        let fill = self.fill(t, &where_)?;
        let anchor = match text(self.get(t, "Anchor")).as_deref() {
            None | Some("head") => Anchor::Head,
            Some("tail") => Anchor::Tail,
            Some(other) => {
                self.note(format!("{where_}: no anchor \"{other}\""));
                Anchor::Head
            }
        };
        self.check_layers(&body, &where_, "a long note's body", |_| true);
        if let Some(cap) = &cap {
            self.check_layers(cap, &where_, "a long note's cap", |_| true);
        }
        if let Some(head) = &head {
            // The head's other parts are drawn as a component's.
            self.check_layers(head, &where_, "a long note's head", |p| {
                matches!(p.role.as_deref(), Some("protect" | "outside"))
            });
        }
        Ok(LongNote {
            head,
            body,
            cap,
            pitch,
            join: number(self.get(t, "Join")).unwrap_or(0.0),
            fill,
            anchor,
        })
    }

    /// A `Def.Lift`: its head's component (`Id`), its trail's (`Trail`),
    /// the lead's time and fade, and the material painting the trail.
    fn lift(&mut self, t: Table<'gc>) -> Result<crate::component::Lift, Failure> {
        use crate::component::Lift;
        let where_: String = self.source(t);
        let id = text(self.get(t, "Id")).unwrap_or_default();
        let head = self.component_of(t, id, false)?;
        let trail = match text(self.get(t, "Trail")).filter(|s| !s.is_empty()) {
            Some(trail) => {
                let mut c = self.component_of(t, trail, false)?;
                // The trail is upright: only the head turns with the lane.
                c.rotation = 0.0;
                Some(c)
            }
            None => None,
        };
        let fill = self.fill(t, &where_)?;
        if let Some(trail) = &trail {
            self.check_layers(trail, &where_, "a lift's trail", |_| true);
        }
        Ok(Lift {
            head,
            trail,
            seconds: number(self.get(t, "Seconds"))
                .filter(|s| s.is_finite())
                .unwrap_or(0.28)
                .clamp(0.05, 2.0),
            fade: number(self.get(t, "Fade")).unwrap_or(2.0).clamp(0.0, 8.0),
            opacity: number(self.get(t, "Opacity"))
                .unwrap_or(1.0)
                .clamp(0.0, 1.0),
            fill,
        })
    }

    /// Notes what `c`'s parts (those `layered` picks) set that their layers
    /// do not take: a long note's and a trail's layers take `Frame` and
    /// `Visible` (a pulse its `Opacity` too); placement, materials and
    /// blending belong in the SVG or on a component's parts.
    fn check_layers(
        &mut self,
        c: &crate::component::Component,
        where_: &str,
        what: &str,
        layered: impl Fn(&crate::component::Part) -> bool,
    ) {
        use crate::component::{PartBlend, Property};
        for part in c.parts.iter().filter(|p| layered(p)) {
            let Some(name) = &part.name else { continue };
            let pulse = part.role.as_deref() == Some("pulse");
            let mut ignored: Vec<&str> = Vec::new();
            for b in &part.bindings {
                let taken = matches!(b.property, Property::Frame | Property::Visible)
                    || (pulse && b.property == Property::Opacity);
                if !taken {
                    ignored.push(match b.property {
                        Property::Opacity => "Opacity",
                        Property::X => "X",
                        Property::Y => "Y",
                        Property::Rotation => "Rotation",
                        Property::Zoom => "Zoom",
                        Property::Phase => "Phase",
                        _ => "a property",
                    });
                }
            }
            let [opacity, x, y, rotation, zoom, _, phase, _] = part.base;
            for (set, key) in [
                (!pulse && opacity != 1.0 && opacity > 0.0, "Opacity"),
                (x != 0.0, "X"),
                (y != 0.0, "Y"),
                (rotation != 0.0, "Rotation"),
                (zoom != 1.0, "Zoom"),
                (phase != 0.0, "Phase"),
                (part.material.is_some(), "Material"),
                (part.blend != PartBlend::Normal, "Blend"),
                (part.tint != [[1.0; 4]; 2], "Tint"),
            ] {
                if set {
                    ignored.push(key);
                }
            }
            ignored.dedup();
            if !ignored.is_empty() {
                self.note(format!(
                    "{where_}: {name}: {} has no effect on {what} (its layers take Frame and Visible{})",
                    ignored.join(", "),
                    if pulse { ", and a pulse its Opacity" } else { "" }
                ));
            }
        }
    }

    /// Component `id` of the table's `File`, its parts' properties from
    /// its `Parts` (each part it lacks said in the notes when `strict`).
    fn component_of(
        &mut self,
        t: Table<'gc>,
        id: String,
        strict: bool,
    ) -> Result<crate::component::Component, Failure> {
        use crate::component::{Component, Part, PartBlend, Property, part_path};
        let where_: String = self.source(t);
        let mut component = Component {
            file: String::new(),
            id,
            view_box: [0.0; 4],
            rotation: number(self.get(t, "Rotation")).unwrap_or(0.0),
            zoom: number(self.get(t, "Zoom")).unwrap_or(1.0),
            parts: Vec::new(),
            duration: number(self.get(t, "Duration"))
                .filter(|d| d.is_finite())
                .unwrap_or(1.0)
                .clamp(0.0, 10.0),
            bindings: Vec::new(),
            pivot: [0.0; 2],
        };
        // The whole component's animation: bindings of X, Y, Rotation and
        // Zoom (a number for the last two is its turn and zoom).
        for key in ["X", "Y", "Rotation", "Zoom"] {
            if let Value::Table(b) = self.get(t, key)
                && let Some(property) = Property::parse(key)
            {
                match self.binding(b, property)? {
                    Ok(b) => component.bindings.push(b),
                    Err(e) => self.note(format!("{where_}: {key}: {e}")),
                }
            }
        }
        if let Value::Table(p) = self.get(t, "Pivot") {
            let n: Vec<f32> = self
                .entries(p)?
                .into_iter()
                .filter_map(|(_, v)| number(v))
                .collect();
            component.pivot = [
                n.first().copied().unwrap_or(0.0),
                n.get(1).copied().unwrap_or(0.0),
            ];
        }
        let Some(file) = text(self.get(t, "File")).filter(|f| !f.is_empty()) else {
            self.note(format!("{where_}: a Component needs a File"));
            return Ok(component);
        };
        // Relative to the file that wrote it, else (`Load` runs under a
        // `pcall`, which hides its file) to the skin's own folder.
        let path = if file.starts_with('/') {
            file.clone()
        } else {
            let dir = text(self.get(t, "_Dir"))
                .filter(|d| !d.is_empty())
                .or_else(|| {
                    let skin = &self.host.skin;
                    skin.chain.first().map(|&d| skin.dir_path(d))
                })
                .unwrap_or_default();
            format!("{dir}{file}")
        };
        let found = match self.host.skin.resolve_path(&path) {
            Ok(f) => f,
            Err(e) => {
                self.note(format!("{where_}: {e}"));
                return Ok(component);
            }
        };
        component.file = found.path.clone();
        let Some(read) = self.images.components(found.dir, &found.path) else {
            // Probing: the shell reads the file before the second pass, and
            // the materials' images are asked for now.
            if let Value::Table(parts) = self.get(t, "Parts") {
                for (_, spec) in self.entries(parts)? {
                    if let Value::Table(spec) = spec
                        && let Value::Table(m) = self.get(spec, "Material")
                    {
                        let _ = self.material(m, &where_)?;
                    }
                }
            }
            return Ok(component);
        };
        let svgs = match read {
            Ok(c) => c,
            Err(e) => {
                self.note(format!("{where_}: {}: {e}", found.path));
                return Ok(component);
            }
        };
        let Some(svg) = svgs.into_iter().find(|c| c.id == component.id) else {
            self.note(format!(
                "{where_}: {} has no component \"{}\"",
                found.path, component.id
            ));
            return Ok(component);
        };
        component.view_box = svg.view_box.map(|v| v as f32);
        for (n, info) in svg.parts.iter().enumerate() {
            let frames: Vec<Option<usize>> = if info.frames == 0 {
                vec![None]
            } else {
                (0..info.frames).map(Some).collect()
            };
            let mut textures = Vec::new();
            for frame in frames {
                let image = part_path(&found.path, &svg.id, n, frame);
                // Kept in its place all the same: the frames after it keep
                // theirs, and it shows nothing.
                if self.images.size(found.dir, &image).is_none() {
                    self.note(format!("{where_}: {image} could not be drawn"));
                }
                textures.push(self.textures.id(crate::build::TextureKey::Skin {
                    dir: found.dir,
                    path: image,
                }));
            }
            component
                .parts
                .push(Part::new(info.name.clone(), info.role.clone(), textures));
        }
        let Value::Table(parts) = self.get(t, "Parts") else {
            return Ok(component);
        };
        for (k, v) in self.entries(parts)? {
            let (Some(name), Value::Table(spec)) = (text(k), v) else {
                continue;
            };
            let Some(part) = component
                .parts
                .iter_mut()
                .find(|p| p.name.as_deref() == Some(name.as_str()))
            else {
                if strict {
                    self.note(format!(
                        "{where_}: the component {} has no part \"{name}\"",
                        component.id
                    ));
                }
                continue;
            };
            let mut problems = Vec::new();
            for (key, value) in self.entries(spec)? {
                let Some(key) = text(key) else { continue };
                if key == "Blend" {
                    part.blend = match text(value).as_deref() {
                        Some("add") | Some("Add") => PartBlend::Add,
                        _ => PartBlend::Normal,
                    };
                    continue;
                }
                if key == "Tint" {
                    match self.tint(value)? {
                        Some(tint) => part.tint = tint,
                        None => problems.push(format!(
                            "{name}.Tint: a color (\"#rrggbb\", {{r, g, b}}) or two"
                        )),
                    }
                    continue;
                }
                if key == "When" {
                    part.when = match value {
                        Value::Table(list) => self
                            .entries(list)?
                            .into_iter()
                            .filter_map(|(_, v)| text(v))
                            .collect(),
                        v => text(v).into_iter().collect(),
                    };
                    continue;
                }
                if key == "Bright" {
                    part.bright = match value {
                        Value::Boolean(b) => Some(b),
                        _ => None,
                    };
                    continue;
                }
                if key == "Material" {
                    match value {
                        Value::Table(m) => {
                            part.material = self.material(m, &where_)?.map(Box::new);
                        }
                        _ => problems.push(format!("{name}.Material: not a Def.Material")),
                    }
                    continue;
                }
                let Some(property) = Property::parse(&key) else {
                    problems.push(format!("{name}: no property {key}"));
                    continue;
                };
                match value {
                    Value::Table(b) => match self.binding(b, property)? {
                        Ok(binding) => part.bindings.push(binding),
                        Err(e) => problems.push(format!("{name}.{key}: {e}")),
                    },
                    v => match number(v) {
                        Some(n) => part.set(property, n),
                        None => problems.push(format!("{name}.{key}: not a number")),
                    },
                }
            }
            for p in problems {
                self.note(format!("{where_}: {p}"));
            }
        }
        Ok(component)
    }

    /// A `Def.Material`: its WGSL's text, space, origin, parameters and
    /// images (each asked for, as a sprite's texture is); `None` (said in
    /// the notes) when it cannot be read.
    fn material(
        &mut self,
        t: Table<'gc>,
        where_: &str,
    ) -> Result<Option<crate::component::PartMaterial>, Failure> {
        use crate::component::{PartMaterial, Space, ramp_path};
        if text(self.get(t, "Class")).as_deref() != Some("Material") {
            self.note(format!("{where_}: Material is not a Def.Material"));
            return Ok(None);
        }
        let resolve = |c: &mut Self, file: &str| -> Option<crate::skin::FileRef> {
            let path = if file.starts_with('/') {
                file.to_string()
            } else {
                let dir = text(c.get(t, "_Dir"))
                    .filter(|d| !d.is_empty())
                    .or_else(|| {
                        let skin = &c.host.skin;
                        skin.chain.first().map(|&d| skin.dir_path(d))
                    })
                    .unwrap_or_default();
                format!("{dir}{file}")
            };
            match c.host.skin.resolve_path(&path) {
                Ok(f) => Some(f),
                Err(e) => {
                    c.note(format!("{where_}: {e}"));
                    None
                }
            }
        };
        let Some(file) = text(self.get(t, "File")).filter(|f| !f.is_empty()) else {
            self.note(format!("{where_}: a Material needs a File"));
            return Ok(None);
        };
        let Some(found) = resolve(self, &file) else {
            return Ok(None);
        };
        let Some(source) = self
            .host
            .skin
            .folder(found.dir)
            .and_then(|f| f.text(&found.path))
            .map(str::to_string)
        else {
            self.note(format!("{where_}: {} is not kept as text", found.path));
            return Ok(None);
        };
        let space = match text(self.get(t, "Space")).as_deref() {
            None | Some("note") => Space::Note,
            Some("lane") => Space::Lane,
            Some(other) => {
                self.note(format!("{where_}: no material space \"{other}\""));
                Space::Note
            }
        };
        let numbers = |c: &mut Self, v: Value<'gc>| -> Result<Vec<f32>, Failure> {
            Ok(match v {
                Value::Table(n) => c
                    .entries(n)?
                    .into_iter()
                    .filter(|(k, _)| matches!(k, Value::Integer(_)))
                    .filter_map(|(_, v)| number(v))
                    .collect(),
                v => number(v).into_iter().collect(),
            })
        };
        let origin = numbers(self, self.get(t, "Origin"))?;
        let mut params = [[0.0; 4]; 8];
        if let Value::Table(list) = self.get(t, "Params") {
            for (n, (_, v)) in self
                .entries(list)?
                .into_iter()
                .filter(|(k, _)| matches!(k, Value::Integer(_)))
                .enumerate()
            {
                if n >= params.len() {
                    self.note(format!("{where_}: a material takes at most 8 Params"));
                    break;
                }
                let values = match text(v).filter(|s| s.starts_with('#')) {
                    Some(hex) => match hex_color(&hex) {
                        Some(c) => c.to_vec(),
                        None => {
                            self.note(format!("{where_}: Params: \"{hex}\" is not a color"));
                            continue;
                        }
                    },
                    None => numbers(self, v)?,
                };
                for (i, x) in values.into_iter().take(4).enumerate() {
                    params[n][i] = x;
                }
            }
        }
        let mut images = Vec::new();
        if let Value::Table(list) = self.get(t, "Images") {
            for (_, v) in self
                .entries(list)?
                .into_iter()
                .filter(|(k, _)| matches!(k, Value::Integer(_)))
            {
                let Some(name) = text(v) else { continue };
                if images.len() == 2 {
                    self.note(format!("{where_}: a material takes at most 2 Images"));
                    break;
                }
                let (file, id) = match name.rsplit_once('#') {
                    Some((f, id)) => (f.to_string(), Some(id.to_string())),
                    None => (name.clone(), None),
                };
                let Some(found) = resolve(self, &file) else {
                    continue;
                };
                let path = match &id {
                    Some(id) => ramp_path(&found.path, id),
                    None => found.path.clone(),
                };
                if self.images.size(found.dir, &path).is_none() {
                    self.note(format!("{where_}: {path} could not be drawn"));
                    continue;
                }
                images.push(self.textures.id(crate::build::TextureKey::Skin {
                    dir: found.dir,
                    path,
                }));
            }
        }
        Ok(Some(PartMaterial {
            file: found.path,
            source,
            space,
            origin: [
                origin.first().copied().unwrap_or(0.0),
                origin.get(1).copied().unwrap_or(0.0),
            ],
            params,
            images,
            compiled: None,
        }))
    }

    /// A part's `Tint`: one color (`"#rrggbb"`, `{r, g, b[, a]}`), both its
    /// colors, or two colors `TintMix` mixes; `None` when it is neither.
    fn tint(&mut self, value: Value<'gc>) -> Result<Option<[[f32; 4]; 2]>, Failure> {
        let numbers = |c: &mut Self, t: Table<'gc>| -> Result<Vec<f32>, Failure> {
            Ok(c.entries(t)?
                .into_iter()
                .filter(|(k, _)| matches!(k, Value::Integer(_)))
                .filter_map(|(_, v)| number(v))
                .collect())
        };
        let color = |c: &mut Self, v: Value<'gc>| -> Result<Option<[f32; 4]>, Failure> {
            Ok(match v {
                Value::Table(t) => {
                    let n = numbers(c, t)?;
                    (n.len() >= 3).then(|| [n[0], n[1], n[2], n.get(3).copied().unwrap_or(1.0)])
                }
                v => text(v).and_then(|t| hex_color(&t)),
            })
        };
        if let Value::Table(t) = value {
            let items: Vec<Value<'gc>> = self
                .entries(t)?
                .into_iter()
                .filter(|(k, _)| matches!(k, Value::Integer(_)))
                .map(|(_, v)| v)
                .collect();
            if items.len() == 2
                && items
                    .iter()
                    .all(|v| matches!(v, Value::String(_) | Value::Table(_)))
            {
                return Ok(match (color(self, items[0])?, color(self, items[1])?) {
                    (Some(a), Some(b)) => Some([a, b]),
                    _ => None,
                });
            }
        }
        Ok(color(self, value)?.map(|c| [c, c]))
    }

    /// A `Bind.*` table as a binding of `property`, or what is wrong.
    fn binding(
        &mut self,
        t: Table<'gc>,
        property: crate::component::Property,
    ) -> Result<Result<crate::component::Binding, String>, Failure> {
        use crate::component::{Binding, Clock, Ease, Function};
        let kind = text(self.get(t, "_bind")).unwrap_or_default();
        let clock_name = text(self.get(t, "clock")).unwrap_or_else(|| "beat".into());
        let Some(clock) = Clock::parse(&clock_name) else {
            return Ok(Err(format!("no clock \"{clock_name}\"")));
        };
        let num =
            |c: &Self, key: &'static str, default: f32| number(c.get(t, key)).unwrap_or(default);
        let function = match kind.as_str() {
            "Linear" => Function::Linear {
                length: num(self, "length", 1.0),
                from: num(self, "from", 0.0),
                to: num(self, "to", 1.0),
                offset: num(self, "offset", 0.0),
            },
            "Steps" => Function::Steps {
                length: num(self, "length", 1.0),
                count: num(self, "count", 1.0).clamp(1.0, 4096.0) as u32,
                offset: num(self, "offset", 0.0),
            },
            "Random" => Function::Random {
                min: num(self, "min", 0.0),
                max: num(self, "max", 1.0),
                salt: num(self, "salt", 0.0).clamp(0.0, 1.0e6) as u32,
            },
            "Curve" => {
                let Value::Table(keys) = self.get(t, "keys") else {
                    return Ok(Err("a curve needs keys".into()));
                };
                let flat: Vec<f32> = self
                    .entries(keys)?
                    .into_iter()
                    .filter(|(k, _)| matches!(k, Value::Integer(_)))
                    .filter_map(|(_, v)| number(v))
                    .collect();
                if flat.len() < 2 || !flat.len().is_multiple_of(2) {
                    return Ok(Err("a curve's keys are time, value pairs".into()));
                }
                let keys: Vec<(f32, f32)> = flat.chunks(2).map(|p| (p[0], p[1])).collect();
                if keys.windows(2).any(|w| w[1].0 < w[0].0) {
                    return Ok(Err("a curve's times go up".into()));
                }
                let ease_name = text(self.get(t, "ease")).unwrap_or_else(|| "linear".into());
                let Some(mut ease) = Ease::parse(&ease_name) else {
                    return Ok(Err(format!("no ease \"{ease_name}\"")));
                };
                if let Some(p) = number(self.get(t, "power")) {
                    let p = p.clamp(0.05, 20.0);
                    ease = match ease {
                        Ease::In(_) => Ease::In(p),
                        Ease::Out(_) => Ease::Out(p),
                        _ => return Ok(Err("power goes with ease \"in\" or \"out\"".into())),
                    };
                }
                Function::Curve {
                    keys,
                    ease,
                    length: number(self.get(t, "length")).filter(|l| *l > 0.0),
                }
            }
            _ => {
                return Ok(Err(
                    "not a binding (Bind.Linear, Bind.Steps, Bind.Curve or Bind.Random)".into(),
                ));
            }
        };
        Ok(Ok(Binding {
            property,
            clock,
            function,
        }))
    }

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
        let where_: String = self.source(t);
        let entries = self.entries(t)?;

        let (mut actor, class_index) = match class.as_str() {
            "Actor" => (Actor::new(Kind::Actor), 0),
            "Sprite" => (self.sprite(t, &where_)?, 1),
            // A plain rectangle, a pixel big until sized.
            "Quad" => (Actor::new(Kind::Sprite(Sprite::quad([1.0, 1.0]))), 1),
            "ActorFrame" => (Actor::new(Kind::Frame(Vec::new())), 2),
            "Model" => (self.model(t, &where_)?, 3),
            // A song's scripts' text, in the face songs' text is drawn in
            // (`docs/plans/scripted-backgrounds.md`); skins make none.
            "BitmapText" if self.host.skin.is_song() => {
                let text = text(self.get(t, "Text")).unwrap_or_default();
                let text = ddi_actor::Text::new(text, song_text_style());
                (Actor::new(Kind::Text(text)), 0)
            }
            // A song's render target: a frame drawn into its texture once
            // `Create`d (`ddi_actor::Target`).
            "ActorFrameTexture" if self.host.skin.is_song() => {
                let mut a = Actor::new(Kind::Frame(Vec::new()));
                a.target = Some(Box::default());
                (a, 4)
            }
            "BitmapText" | "ActorMultiVertex" | "ActorFrameTexture" | "ActorProxy"
            | "BGAnimation" | "ActorScroller" | "RollingNumbers" => {
                self.unsupported = true;
                self.note(format!("{where_}: {class} is not drawn here"));
                (Actor::new(Kind::Actor), 0)
            }
            "Sound" => {
                // A song's: its file, for `play` (`ActorSound`).
                // `ActorUtil::GetAttrPath`: relative to the file that
                // wrote it.
                let dir = text(self.get(t, "_Dir")).unwrap_or_default();
                let path = text(self.get(t, "File")).map(|f| {
                    if f.starts_with('/') {
                        f
                    } else {
                        format!("{dir}{f}")
                    }
                });
                if self.host.skin.is_song()
                    && let Some(path) = path
                    && let Ok(found) = self.host.skin.resolve_path(&path)
                {
                    self.host.sounds.insert(id, found.path);
                }
                (Actor::new(Kind::Actor), 0)
            }
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

    /// `Def.Model { Meshes =, Materials =, Bones = }`: each relative to the
    /// file that wrote it; StepMania needs all three (`Model::Load`). One
    /// that cannot be loaded leaves the element to the built-in skin.
    fn model(&mut self, t: Table<'gc>, where_: &str) -> Result<Actor, Failure> {
        let mut files = Vec::with_capacity(3);
        for key in ["Meshes", "Materials", "Bones"] {
            let value = self.get(t, key);
            if too_long(value) {
                self.note(format!("{where_}: the {key} path is too long"));
                self.unsupported = true;
                return Ok(Actor::new(Kind::Actor));
            }
            let Some(name) = text(value).filter(|s| !s.is_empty()) else {
                self.note(format!("{where_}: a Model without {key}"));
                self.unsupported = true;
                return Ok(Actor::new(Kind::Actor));
            };
            self.visit(name.len() / 64)?;
            let path = if name.starts_with('/') {
                name.clone()
            } else {
                match text(self.get(t, "_Dir")) {
                    Some(dir) => format!("{dir}{name}"),
                    None => {
                        self.note(format!(
                            "{where_}: relative path \"{name}\", but path is unknown"
                        ));
                        self.unsupported = true;
                        return Ok(Actor::new(Kind::Actor));
                    }
                }
            };
            match self.host.skin.resolve_path(&path) {
                Ok(f) => files.push(f),
                Err(e) => {
                    self.note(format!("{where_}: {e}"));
                    self.unsupported = true;
                    return Ok(Actor::new(Kind::Actor));
                }
            }
        }
        // Models are as costly as their lines.
        for f in &files {
            let len = self
                .host
                .skin
                .folder(f.dir)
                .and_then(|folder| folder.text(&f.path))
                .map_or(0, str::len);
            self.visit(len / 64)?;
        }
        match crate::build::model_actor(
            &self.host.skin,
            self.textures,
            self.images,
            &files[0],
            &files[1],
            Some(&files[2]),
        ) {
            Ok(actor) => Ok(actor),
            Err(e) => {
                self.note(format!("{where_}: {e}"));
                self.unsupported = true;
                Ok(Actor::new(Kind::Actor))
            }
        }
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
