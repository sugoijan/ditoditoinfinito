//! An imported noteskin during play: each lane's actors (receptor,
//! explosion, note parts), the commands the engine's events play on them,
//! and what they draw, as StepMania's `NoteField`, `ReceptorArrow`,
//! `GhostArrowRow` and `NoteDisplay` do (5_1-new; `docs/research/noteskins.md`
//! sections 4–7). Elements the skin cannot provide yet are drawn by the
//! built-in skin ([`NoteLayer::draws`]).
//!
//! A skin built through its Lua ([`NoteskinPlay::with_lua`]) plays its
//! commands, queued commands and update functions through that Lua each
//! frame, within [`FrameBudget`]; if the Lua stops (its budget, memory or
//! a host limit), every element is the built-in skin's for the rest of
//! the play ([`NoteskinPlay::lua_failure`]).

mod hold_material;
mod judgment_links;
mod lift_material;

use ddi_actor::{Actor, Clocks, Drawn, Kind, ModelDraw, Problems, Quad};
use ddi_chart::Layout;
use ddi_engine::frame::{Frame, HoldKind, NoteSprite, SkinEvent, SkinEventKind, SpriteKind};
use ddi_engine::rules::Judgement;
use ddi_lua::Failure;
use ddi_noteskin::build::{Built, Images, TextureKey, Textures, build, build_colored};
use ddi_noteskin::lua::{
    Env, FrameBudget, LoadBudget, LuaSkin, NOTE_COLORS, Param, Pause, Position,
};
use ddi_noteskin::metrics::{NoteDisplay, Part, PartMetrics};
use ddi_noteskin::notes::{animation_percent, color_offset, note_type};
use ddi_noteskin::{Button, Folder, LaneButton, Skin};

use crate::actors2d;
use crate::color::to_target;
use crate::model::{Camera, Corners, Seen, triangles_with};
use crate::scene::FieldGeometry;
use crate::stencil::Mask;
use crate::textured::{self, QuadBlend, Shape, TexQuad, UNTEXTURED};

/// What the built-in skin draws when the noteskin does not.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LayerPart {
    Receptor,
    Tap,
    Lift,
    Fake,
    Mine,
    /// A hold's body and caps.
    Hold,
    Roll,
    /// A hold's head.
    HoldHead,
    RollHead,
}

/// The noteskin's side of the scene.
pub trait NoteLayer {
    /// Whether the noteskin draws `part` of lane `lane`.
    fn draws(&self, lane: usize, part: LayerPart) -> bool;
    fn receptors(
        &mut self,
        frame: &Frame,
        layout: &Layout,
        geo: &FieldGeometry,
        out: &mut Vec<TexQuad>,
    );
    fn holds(
        &mut self,
        frame: &Frame,
        layout: &Layout,
        geo: &FieldGeometry,
        out: &mut Vec<TexQuad>,
    );
    fn notes(
        &mut self,
        frame: &Frame,
        layout: &Layout,
        geo: &FieldGeometry,
        out: &mut Vec<TexQuad>,
    );
    fn explosions(
        &mut self,
        frame: &Frame,
        layout: &Layout,
        geo: &FieldGeometry,
        out: &mut Vec<TexQuad>,
    );
    /// The materials' parameters the quads given this frame name
    /// ([`crate::material::MaterialQuad::slot`]).
    fn materials(&self) -> &[crate::material::MaterialParams] {
        &[]
    }

    /// The layer's own materials, which its quads name with
    /// [`crate::material::LOCAL`] and their index.
    fn skin_materials(&self) -> &[crate::material::Compiled] {
        &[]
    }

    /// Registered textures its quads draw whole or by frame, never
    /// repeating them: the renderer may pack them into an atlas
    /// ([`crate::textured::TexturedPipeline::pack`]).
    fn atlas_textures(&self) -> &[u32] {
        &[]
    }
}

/// One element of a lane.
#[derive(Clone, Debug)]
enum Element {
    Actor(Box<Actor>),
    BuiltIn,
    Blank,
    /// A component of the skin's SVG (`docs/plans/noteskin-scene.md`).
    Component(Box<ddi_noteskin::component::Component>),
    /// A hold's or roll's body drawing the whole long note (`Def.LongNote`).
    LongNote(Box<ddi_noteskin::component::LongNote>),
    /// A lift drawn by components (`Def.Lift`).
    Lift(Box<ddi_noteskin::component::Lift>),
}

impl Element {
    fn is_skin(&self) -> bool {
        !matches!(self, Element::BuiltIn)
    }
}

/// A hold's or roll's parts: inactive, active, then optionally dropped.
#[derive(Clone, Debug)]
struct HoldParts {
    head: Vec<Element>,
    tail: Vec<Element>,
    topcap: Vec<Element>,
    body: Vec<Element>,
    bottomcap: Vec<Element>,
}

impl HoldParts {
    /// The skin draws the body and caps (with its body).
    fn drawn(&self) -> bool {
        self.body.iter().all(Element::is_skin)
    }

    /// The skin draws the heads.
    fn heads(&self) -> bool {
        self.head.iter().all(Element::is_skin)
    }
}

/// A lane's note elements in one note type's color, for a skin whose
/// Lua reads `Var "Color"` (`None`: as the 4ths'). ITGmania loads taps,
/// lifts, fakes, mines and every hold and roll part per color; a hold is
/// drawn in its head's.
#[derive(Clone, Debug, Default)]
struct ColorSet {
    tap: Option<Element>,
    lift: Option<Element>,
    fake: Option<Element>,
    mine: Option<Element>,
    /// Head, tail, top cap, body, bottom cap; inactive, active, dropped (as
    /// [`HoldParts::all`]).
    hold: [[Option<Element>; 3]; 5],
    roll: [[Option<Element>; 3]; 5],
}

/// A tap-like note element of a lane.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Slot {
    Tap,
    Lift,
    Fake,
    Mine,
}

impl ColorSet {
    /// Where an element of [`LANE_ELEMENTS`] goes, for those loaded per
    /// color.
    fn place(&mut self, name: &str) -> Option<&mut Option<Element>> {
        match name {
            "Tap Note" => return Some(&mut self.tap),
            "Tap Lift" => return Some(&mut self.lift),
            "Tap Fake" => return Some(&mut self.fake),
            "Tap Mine" => return Some(&mut self.mine),
            _ => {}
        }
        let mut words = name.split(' ');
        let parts = match words.next()? {
            "Hold" => &mut self.hold,
            "Roll" => &mut self.roll,
            _ => return None,
        };
        let part_name = words.next()?;
        let part = ["Head", "Tail", "Topcap", "Body", "Bottomcap"]
            .iter()
            .position(|p| *p == part_name)?;
        let state = match words.next()? {
            "Inactive" => 0,
            "Active" => 1,
            "Dropped" => 2,
            _ => return None,
        };
        Some(&mut parts[part][state])
    }

    fn get_mut(&mut self, slot: Slot) -> &mut Option<Element> {
        match slot {
            Slot::Tap => &mut self.tap,
            Slot::Lift => &mut self.lift,
            Slot::Fake => &mut self.fake,
            Slot::Mine => &mut self.mine,
        }
    }

    fn all(&self) -> impl Iterator<Item = &Element> {
        [&self.tap, &self.lift, &self.fake, &self.mine]
            .into_iter()
            .chain(self.hold.iter().flatten())
            .chain(self.roll.iter().flatten())
            .filter_map(Option::as_ref)
    }

    fn all_mut(&mut self) -> impl Iterator<Item = &mut Element> {
        [
            &mut self.tap,
            &mut self.lift,
            &mut self.fake,
            &mut self.mine,
        ]
        .into_iter()
        .chain(self.hold.iter_mut().flatten())
        .chain(self.roll.iter_mut().flatten())
        .filter_map(Option::as_mut)
    }
}

#[derive(Clone, Debug)]
struct Lane {
    metrics: NoteDisplay,
    receptor: Element,
    explosion: Element,
    tap: Element,
    lift: Element,
    fake: Element,
    mine: Element,
    hold: HoldParts,
    roll: HoldParts,
    holding: Option<HoldKind>,
    /// Since when (song seconds) it is held, and its last judgments: an
    /// explosion component's instances (`docs/plans/noteskin-scene.md`).
    holding_since: Option<f64>,
    /// The last hold's instance after it ended (its name, start and end,
    /// song seconds), drawn on for its component's `Duration`.
    ended: Option<(&'static str, f64, f64)>,
    hits: std::collections::VecDeque<Hit>,
    /// What the lane's last press did, for its receptor component (`W1`
    /// … `W5`, `None` for nothing hit, `HitMine`), as StepMania's
    /// receptor commands are played.
    step: Option<Hit>,
    /// Base orientation before lending the explosion tree to Lua.
    explosion_rotation: f32,
    /// The layout's scroll sign for the lane (-1: a lower-row lane).
    scroll_sign: i8,
    /// The note elements of 8ths to 192nds, when the skin's Lua colors
    /// notes by `Var "Color"` (empty otherwise: every note is as the 4ths).
    colors: Vec<ColorSet>,
}

/// The parts' documents a shell draws, each once: a part whose document
/// an earlier part's is is not drawn, its image that one's
/// ([`NoteskinPlay::take_images`]). Parts of two components are often the
/// same document (`ddi_noteskin::component::part_svg` keeps only what a
/// part reaches).
#[derive(Debug, Default)]
pub struct SameDocuments {
    first: std::collections::HashMap<(usize, String), String>,
    /// Each part not drawn, by `(folder, path)`: the path of the part of
    /// that folder whose image it is.
    pub same: std::collections::HashMap<(usize, String), String>,
}

impl SameDocuments {
    /// The part of folder `folder` drawn from `doc` already, if one is
    /// (`path`'s image then that one's); else `path` is noted as its first.
    pub fn first_of(&mut self, folder: usize, path: &str, doc: &str) -> Option<String> {
        match self.first.get(&(folder, doc.to_string())) {
            Some(first) if first == path => None,
            Some(first) => {
                self.same.insert((folder, path.to_string()), first.clone());
                Some(first.clone())
            }
            None => {
                self.first
                    .insert((folder, doc.to_string()), path.to_string());
                None
            }
        }
    }

    /// Whether `path` of `folder` is a part not drawn.
    pub fn is_copy(&self, folder: usize, path: &str) -> bool {
        self.same.contains_key(&(folder, path.to_string()))
    }
}

/// A noteskin playing.
pub struct NoteskinPlay {
    pub name: String,
    lanes: Vec<Lane>,
    /// The lanes as built, for a retry (StepMania loads the actors again).
    initial: Vec<Lane>,
    /// Positions actually drawn last frame, keyed by transformed note identity.
    last_note_positions: std::collections::HashMap<usize, f32>,
    /// Quads drawn in the last frame (debug).
    quads: std::cell::Cell<usize>,
    /// Models' triangles drawn this frame (at most [`MAX_FRAME_TRIANGLES`]).
    triangles: std::cell::Cell<usize>,
    /// What an actor drew, reused by each [`NoteskinPlay::emit`].
    drawn: std::cell::Cell<Vec<Drawn>>,
    /// A model's triangles and meshes, reused by each
    /// [`NoteskinPlay::push_model`].
    model_scratch: std::cell::Cell<ModelScratch>,
    /// The field's perspective this frame, for models.
    camera: Camera,
    textures: Textures,
    /// The renderer's id for each of [`NoteskinPlay::textures`].
    texture_ids: Vec<Option<u32>>,
    /// Per texture, the earlier one whose image it shares (the same
    /// pixels: [`NoteskinPlay::share_textures`]), given its id with it.
    shared: Vec<Option<usize>>,
    next_seq: u64,
    started: bool,
    last_song_time: Option<f64>,
    /// This frame's song seconds, beat and roll timing window
    /// ([`NoteskinPlay::update`]), for components' clocks.
    frame_time: (f64, f64, f64),
    timer: f32,
    /// Built-in substitutions and command errors, for the import report
    /// and the console.
    pub notes: Vec<String>,
    problems: Problems,
    srgb: bool,
    /// The skin's Lua, when its elements were built through it.
    lua: Option<LuaSkin>,
    /// Lua warnings already copied to the problems.
    lua_seen: usize,
    /// Why the skin's Lua stopped during the play, and whether only the
    /// clock stopped it (the backstop: a stalled machine, not the skin).
    lua_failure: Option<(String, bool)>,
    /// [`NoteskinPlay::take_lua_stop`] has told of the stop.
    lua_stop_taken: bool,
    /// Fuel the skin's Lua spent in the last frame, and at most in one.
    lua_fuel: (u64, u64),
    /// A skin in StepMania 3.9's format: its explosion rows take the
    /// score with the row's name (`ddi_noteskin::legacy`).
    legacy: bool,
    /// This frame's materials' parameters, each once
    /// ([`NoteskinPlay::material_slot`]).
    materials: Vec<crate::material::MaterialParams>,
    /// Each lane's last press and release, in song seconds, and whether
    /// it is down: components' `press` and `release` clocks.
    presses: Vec<LanePresses>,
    /// The skin's materials, compiled (`PartMaterial::compiled` indexes them).
    skin_materials: Vec<crate::material::Compiled>,
    /// [`NoteLayer::atlas_textures`], once its textures are registered.
    atlas: std::cell::OnceCell<Vec<u32>>,
}

/// A judgment an explosion component shows an instance for.
#[derive(Clone, Copy, Debug)]
struct Hit {
    /// When, in song seconds.
    at: f64,
    /// `W1` … `W5`, `Held`, `HitMine` (a miss sets off none).
    name: &'static str,
    bright: bool,
}

/// The judgments a lane keeps for its explosion's instances.
const MAX_HITS: usize = 8;

/// A lane's last press and release ([`NoteskinPlay::presses`]).
#[derive(Clone, Copy, Debug, Default)]
struct LanePresses {
    down: bool,
    pressed: Option<f64>,
    released: Option<f64>,
}

/// What [`NoteskinPlay::push_model`] reuses: a model's triangles seen,
/// its vertices seen, and each mesh's texture and color.
#[derive(Default)]
struct ModelScratch {
    seen: Vec<Seen>,
    corners: Corners,
    meshes: Vec<(Option<u32>, [f32; 4])>,
}

/// The most triangles a frame's models draw (a model has at most
/// [`ddi_noteskin::model::MAX_TRIANGLES`]; the corpus's, under 2,000).
const MAX_FRAME_TRIANGLES: usize = 1 << 18;

/// The elements of a lane, in the order they are built.
const LANE_ELEMENTS: [&str; 36] = [
    "Hold Head Inactive",
    "Hold Head Active",
    "Hold Head Dropped",
    "Hold Tail Inactive",
    "Hold Tail Active",
    "Hold Tail Dropped",
    "Hold Topcap Inactive",
    "Hold Topcap Active",
    "Hold Topcap Dropped",
    "Hold Body Inactive",
    "Hold Body Active",
    "Hold Body Dropped",
    "Hold Bottomcap Inactive",
    "Hold Bottomcap Active",
    "Hold Bottomcap Dropped",
    "Roll Head Inactive",
    "Roll Head Active",
    "Roll Head Dropped",
    "Roll Tail Inactive",
    "Roll Tail Active",
    "Roll Tail Dropped",
    "Roll Topcap Inactive",
    "Roll Topcap Active",
    "Roll Topcap Dropped",
    "Roll Body Inactive",
    "Roll Body Active",
    "Roll Body Dropped",
    "Roll Bottomcap Inactive",
    "Roll Bottomcap Active",
    "Roll Bottomcap Dropped",
    "Receptor",
    "Explosion",
    "Tap Note",
    "Tap Lift",
    "Tap Fake",
    "Tap Mine",
];

fn lane_elements(dropped: bool) -> impl Iterator<Item = &'static str> {
    LANE_ELEMENTS
        .into_iter()
        .filter(move |name| dropped || !name.ends_with(" Dropped"))
}

/// The part whose metrics a lane element of a StepMania 3.9 skin takes,
/// for its per-note-type files (fakes as tap notes; lifts are drawn built
/// in).
fn legacy_part(name: &str) -> Option<Part> {
    let mut words = name.split(' ');
    Some(match (words.next()?, words.next()?) {
        ("Tap", "Note" | "Fake") => Part::TapNote,
        ("Tap", "Mine") => Part::TapMine,
        ("Hold" | "Roll", "Head") => Part::HoldHead,
        ("Hold" | "Roll", "Tail") => Part::HoldTail,
        ("Hold" | "Roll", "Topcap") => Part::HoldTopCap,
        ("Hold" | "Roll", "Body") => Part::HoldBody,
        ("Hold" | "Roll", "Bottomcap") => Part::HoldBottomCap,
        _ => return None,
    })
}

/// The engine's tier as StepMania names it (`TapNoteScoreNames`).
fn score_name(j: Judgement) -> &'static str {
    match j {
        Judgement::W1 => "W1",
        Judgement::W2 => "W2",
        Judgement::W3 => "W3",
        Judgement::W4 => "W4",
        Judgement::W5 => "W5",
        Judgement::Miss => "Miss",
    }
}

/// What a built element is to the play, its notes added to `notes`.
fn to_element(
    built: ddi_noteskin::build::Element,
    button: Button,
    name: &str,
    textures: &Textures,
    notes: &mut Vec<String>,
) -> Element {
    for n in built.notes {
        let n = format!("{}: {n}", button.name());
        if !notes.contains(&n) {
            notes.push(n);
        }
    }
    // The stand-ins' art an actor draws, when it draws only that.
    let standins = |a: &Actor| -> Option<Vec<String>> {
        let ts = a.textures();
        (!ts.is_empty())
            .then(|| {
                ts.iter()
                    .map(|&t| match &textures.keys()[t as usize] {
                        TextureKey::BuiltIn(n) => Some(n.to_ascii_lowercase()),
                        TextureKey::Skin { .. } => None,
                    })
                    .collect()
            })
            .flatten()
    };
    match built.built {
        Built::Actor(a) if name != "Explosion" => match standins(&a) {
            // StepMania's `_blank` (a tail or cap a skin leaves out): nothing.
            Some(n) if n.iter().all(|n| n == "_blank.png") => Element::Blank,
            // A note made of the stand-ins' art only: the built-in skin's
            // own shape, as for the rest.
            Some(_) => Element::BuiltIn,
            None => Element::Actor(Box::new(a)),
        },
        Built::Actor(a) => Element::Actor(Box::new(a)),
        Built::BuiltIn => Element::BuiltIn,
        Built::Blank => Element::Blank,
        Built::Component(c) => Element::Component(Box::new(c)),
        Built::LongNote(n) => Element::LongNote(Box::new(n)),
        Built::Lift(l) => Element::Lift(Box::new(l)),
    }
}

impl Lane {
    /// A lane from its elements, built in [`LANE_ELEMENTS`]' order.
    fn assemble(elements: Vec<Element>, metrics: NoteDisplay, scroll_sign: i8) -> Lane {
        let mut e = elements.into_iter();
        let mut next = || e.next().unwrap_or(Element::BuiltIn);
        let count = if metrics.hold_use_dropped_state { 3 } else { 2 };
        let mut pair = || (0..count).map(|_| next()).collect();
        let mut parts = || HoldParts {
            head: pair(),
            tail: pair(),
            topcap: pair(),
            body: pair(),
            bottomcap: pair(),
        };
        let hold = parts();
        let roll = parts();
        Lane {
            metrics,
            receptor: next(),
            explosion: next(),
            tap: next(),
            lift: next(),
            fake: next(),
            mine: next(),
            hold,
            roll,
            holding: None,
            holding_since: None,
            ended: None,
            hits: std::collections::VecDeque::new(),
            step: None,
            explosion_rotation: 0.0,
            scroll_sign,
            colors: Vec::new(),
        }
    }

    fn base_slot(&mut self, slot: Slot) -> &mut Element {
        match slot {
            Slot::Tap => &mut self.tap,
            Slot::Lift => &mut self.lift,
            Slot::Fake => &mut self.fake,
            Slot::Mine => &mut self.mine,
        }
    }

    /// The element drawing a note of `note_type` (`note_type()`: 0 for
    /// 4ths … 8 for 192nds) in `slot`.
    fn note_slot(&mut self, slot: Slot, note_type: u32) -> &mut Element {
        let colored = (note_type as usize).checked_sub(1).filter(|&i| {
            self.colors
                .get_mut(i)
                .is_some_and(|c| c.get_mut(slot).is_some())
        });
        match colored {
            Some(i) => self.colors[i]
                .get_mut(slot)
                .as_mut()
                .expect("checked above"),
            None => self.base_slot(slot),
        }
    }

    /// Exchanges the hold's (or roll's) parts with note type `note_type`'s
    /// copies: before drawing a hold in its head's color, and after.
    fn swap_hold_colors(&mut self, note_type: u32, roll: bool) {
        let Lane {
            hold,
            roll: rolls,
            colors,
            ..
        } = self;
        let Some(set) = (note_type as usize)
            .checked_sub(1)
            .and_then(|i| colors.get_mut(i))
        else {
            return;
        };
        let (parts, copies) = if roll {
            (rolls, &mut set.roll)
        } else {
            (hold, &mut set.hold)
        };
        for (part, copies) in parts.parts_mut().into_iter().zip(copies) {
            for (base, copy) in part.iter_mut().zip(copies) {
                if let Some(copy) = copy {
                    std::mem::swap(base, copy);
                }
            }
        }
    }

    /// Whether its tap note and receptor are drawn by the skin (or are
    /// blank), not by the built-in skin: a lane built with a button the
    /// skin cannot draw is not.
    fn can_stand(&self) -> bool {
        self.tap.is_skin() && self.receptor.is_skin()
    }

    /// Turns the elements StepMania turns with their button `degrees` more
    /// (a lane drawn with another button's elements, `LaneButton::turn`).
    fn turn(&mut self, degrees: f32) {
        if degrees == 0.0 {
            return;
        }
        let Lane {
            receptor,
            explosion,
            tap,
            lift,
            fake,
            hold,
            roll,
            colors,
            ..
        } = self;
        let heads = hold.head.iter_mut().chain(roll.head.iter_mut());
        let copies = colors.iter_mut().flat_map(|set| {
            let [hold_head, ..] = &mut set.hold;
            let [roll_head, ..] = &mut set.roll;
            [&mut set.tap, &mut set.lift, &mut set.fake]
                .into_iter()
                .chain(hold_head.iter_mut())
                .chain(roll_head.iter_mut())
                .filter_map(Option::as_mut)
        });
        for e in [receptor, explosion, tap, lift, fake]
            .into_iter()
            .chain(heads)
            .chain(copies)
        {
            if let Element::Actor(a) = e {
                a.base_rotation[2] += degrees;
            }
        }
    }

    /// Drops the copies that are not drawn as their 4ths' are (the skin's
    /// or the built-in skin's): a copy that failed to load falls back to
    /// the 4ths'.
    fn settle_colors(&mut self) {
        let Lane {
            tap,
            lift,
            fake,
            mine,
            hold,
            roll,
            colors,
            ..
        } = self;
        for set in colors.iter_mut() {
            for (base, copy) in [
                (&*tap, &mut set.tap),
                (&*lift, &mut set.lift),
                (&*fake, &mut set.fake),
                (&*mine, &mut set.mine),
            ]
            .into_iter()
            .chain(hold.all().zip(set.hold.iter_mut().flatten()))
            .chain(roll.all().zip(set.roll.iter_mut().flatten()))
            {
                if copy.as_ref().is_some_and(|c| c.is_skin() != base.is_skin()) {
                    *copy = None;
                }
            }
        }
    }
}

/// Plays commands and updates on elements: natively, or through the
/// skin's Lua (its trees lent) until it fails.
struct Commands<'a> {
    lua: Option<&'a mut LuaSkin>,
    failed: Option<Failure>,
}

impl Commands<'_> {
    fn play(&mut self, e: &mut Element, name: &str, params: &[(&str, Param)], p: &mut Problems) {
        let Element::Actor(a) = e else {
            return;
        };
        match &mut self.lua {
            Some(lua) if self.failed.is_none() => {
                if let Err(f) = lua.play(a.id, name, params) {
                    self.failed = Some(f);
                }
            }
            Some(_) => {}
            None => a.play(name, p),
        }
    }

    fn update(&mut self, e: &mut Element, dt: f32, clocks: &Clocks, p: &mut Problems) {
        let Element::Actor(a) = e else {
            return;
        };
        match &mut self.lua {
            Some(lua) if self.failed.is_none() => {
                if let Err(f) = lua.update(a.id, dt, clocks) {
                    self.failed = Some(f);
                }
            }
            Some(_) => {}
            None => a.update(dt, clocks, p),
        }
    }
}

impl NoteskinPlay {
    /// Builds every lane's elements; `None` when the layout's lanes are not
    /// dance buttons.
    pub fn new(
        skin: &Skin<'_>,
        layout: &Layout,
        images: &dyn Images,
        srgb: bool,
    ) -> Option<NoteskinPlay> {
        let mut textures = Textures::default();
        let mut notes = Vec::new();
        let mut lanes = Vec::new();
        for lane in &layout.lanes {
            let candidates = skin.lane_buttons(lane);
            if candidates.is_empty() {
                return None;
            }
            let mut chosen = None;
            for (k, lane_button) in candidates.iter().enumerate() {
                let mut lane_notes = Vec::new();
                let assembled = Self::native_lane(
                    skin,
                    lane,
                    *lane_button,
                    images,
                    &mut textures,
                    &mut lane_notes,
                );
                // The next candidate when this one has no tap or receptor.
                if k + 1 == candidates.len() || assembled.can_stand() {
                    notes.append(&mut lane_notes);
                    chosen = Some(assembled);
                    break;
                }
            }
            lanes.extend(chosen);
        }
        let mut play =
            NoteskinPlay::from_lanes(skin.name.clone(), lanes, textures, notes, srgb, None);
        play.legacy = skin.legacy;
        Some(play)
    }

    /// One lane of [`NoteskinPlay::new`], with `lane_button`'s elements.
    fn native_lane(
        skin: &Skin<'_>,
        lane: &ddi_chart::Lane,
        lane_button: LaneButton,
        images: &dyn Images,
        textures: &mut Textures,
        notes: &mut Vec<String>,
    ) -> Lane {
        {
            let button = lane_button.button;
            let metrics = NoteDisplay::read(skin, button.name());
            let elements = lane_elements(metrics.hold_use_dropped_state)
                .map(|name| {
                    let built = build(skin, button, name, textures, images);
                    to_element(built, button, name, textures, notes)
                })
                .collect();
            // A StepMania 3.9 skin's note parts with a file per note type.
            let colored: Vec<&str> = lane_elements(metrics.hold_use_dropped_state)
                .filter(|name| {
                    ColorSet::default().place(name).is_some()
                        && legacy_part(name).is_some_and(|p| metrics.part(p).note_color)
                })
                .collect();
            let mut assembled = Lane::assemble(elements, metrics, lane.scroll_sign);
            if !colored.is_empty() {
                for color in &NOTE_COLORS[1..] {
                    let mut set = ColorSet::default();
                    for &name in &colored {
                        let built = build_colored(skin, button, name, color, textures, images);
                        if let Some(place) = set.place(name) {
                            *place = Some(to_element(built, button, name, textures, notes));
                        }
                    }
                    assembled.colors.push(set);
                }
                assembled.settle_colors();
            }
            assembled.turn(lane_button.turn);
            assembled
        }
    }

    /// Builds skin `name` among `folders` (its fallbacks' too) through its
    /// Lua for `layout`, both shells' way: a VM of its own with hash seed
    /// `seed` (a play's two passes share one, so tables iterate alike) and
    /// the load's budget ([`LoadBudget::default`]), the prelude and the
    /// `NoteSkin.lua` chain run, then [`NoteskinPlay::with_lua`]. `clock`
    /// is the host's for the Lua's deadlines, in seconds; `pause` gives
    /// the host its turn between slices (none: the load runs straight
    /// through and never waits, so a caller may poll it to its end at
    /// once). `Ok(None)` when the layout's lanes are not the skin's
    /// buttons; why it could not otherwise (the caller then builds it
    /// natively, [`NoteskinPlay::new`]).
    #[allow(clippy::too_many_arguments)]
    pub async fn load_lua(
        folders: &[Folder],
        game: &str,
        name: &str,
        env: Env,
        seed: u64,
        layout: &Layout,
        images: &dyn Images,
        clock: Box<dyn FnMut() -> f64>,
        mut pause: Option<&mut Pause>,
    ) -> Result<Option<NoteskinPlay>, String> {
        let loaded = Skin::load(folders, game, name).map_err(|e| e.to_string())?;
        // Lanes no skin draws (`Skin::lane_button`).
        if layout.lanes.iter().any(|l| loaded.lane_button(l).is_none()) {
            return Ok(None);
        }
        let settings = ddi_lua::Settings {
            hash_seed: seed,
            ..ddi_lua::Settings::default()
        };
        let lua = LuaSkin::new_sliced(
            loaded.into_owned(),
            env,
            settings,
            LoadBudget::default(),
            clock,
            pause.as_deref_mut(),
        )
        .await
        .map_err(|e| e.to_string())?;
        NoteskinPlay::with_lua(lua, layout, images, true, pause)
            .await
            .map_err(|e| e.to_string())
    }

    /// Builds every lane's elements through the skin's Lua, giving the host
    /// a turn between slices of it. `Ok(None)` when the layout's lanes are
    /// not dance buttons; an error when the skin's Lua stopped (the caller
    /// builds it natively instead, [`NoteskinPlay::new`]).
    pub async fn with_lua(
        mut lua: LuaSkin,
        layout: &Layout,
        images: &dyn Images,
        srgb: bool,
        mut pause: Option<&mut Pause>,
    ) -> Result<Option<NoteskinPlay>, Failure> {
        let candidates: Vec<Vec<LaneButton>> = layout
            .lanes
            .iter()
            .map(|l| lua.skin().lane_buttons(l))
            .collect();
        if candidates.iter().any(Vec::is_empty) {
            return Ok(None);
        }
        let mut textures = Textures::default();
        let mut notes = Vec::new();
        let mut lanes = Vec::new();
        let mut lane_buttons = Vec::new();
        for (lane, candidates) in layout.lanes.iter().zip(&candidates) {
            for (k, lane_button) in candidates.iter().enumerate() {
                let mut lane_notes = Vec::new();
                let assembled = Self::lua_lane(
                    &mut lua,
                    lane,
                    *lane_button,
                    images,
                    &mut textures,
                    &mut lane_notes,
                    pause.as_deref_mut(),
                )
                .await?;
                // The next candidate when the skin's Lua cannot build this
                // one's tap or receptor (no redirect for `Center`).
                if k + 1 == candidates.len() || assembled.can_stand() {
                    notes.append(&mut lane_notes);
                    lanes.push(assembled);
                    lane_buttons.push(*lane_button);
                    break;
                }
            }
        }
        // Every element is loaded: the play takes the trees it draws.
        for (lane, lane_button) in lanes.iter_mut().zip(&lane_buttons) {
            for e in lane.elements_mut() {
                if let Element::Actor(a) = e
                    && !lua.reclaim(a)
                {
                    *e = Element::BuiltIn;
                }
            }
            lane.settle_colors();
            // On the trees taken (the ones lent to Lua again later).
            lane.turn(lane_button.turn);
        }
        let name = lua.skin().name.clone();
        let mut play = NoteskinPlay::from_lanes(name, lanes, textures, notes, srgb, Some(lua));
        if let Some(lua) = &play.lua {
            for w in lua.warnings() {
                if !play.notes.contains(&w) {
                    play.notes.push(w);
                }
            }
            play.lua_seen = lua.warnings_since(0).len();
        }
        Ok(Some(play))
    }

    /// One lane of [`NoteskinPlay::with_lua`], with `lane_button`'s
    /// elements built through the skin's Lua (their trees still the
    /// skin's).
    async fn lua_lane(
        lua: &mut LuaSkin,
        lane: &ddi_chart::Lane,
        lane_button: LaneButton,
        images: &dyn Images,
        textures: &mut Textures,
        notes: &mut Vec<String>,
        mut pause: Option<&mut Pause>,
    ) -> Result<Lane, Failure> {
        {
            let button = lane_button.button;
            let mut elements = Vec::new();
            // The note elements whose Lua read their color.
            let mut colored = Vec::new();
            let metrics = NoteDisplay::read(&lua.skin(), button.name());
            // The bodies first: a `Def.LongNote` draws the whole long note,
            // and its head, tail and caps are not asked for.
            let mut bodies = std::collections::HashMap::new();
            for name in
                lane_elements(metrics.hold_use_dropped_state).filter(|n| n.contains(" Body "))
            {
                let loaded = lua
                    .element_colored_sliced(
                        button,
                        name,
                        NOTE_COLORS[0],
                        textures,
                        images,
                        pause.as_deref_mut(),
                    )
                    .await?;
                bodies.insert(name, loaded);
            }
            let owned = |name: &str| {
                let mut words = name.split(' ');
                let (kind, part, state) = (words.next(), words.next(), words.next());
                part != Some("Body")
                    && matches!(part, Some("Head" | "Tail" | "Topcap" | "Bottomcap"))
                    && bodies.iter().any(|(body, (built, _))| {
                        matches!(built.built, Built::LongNote(_))
                            && *body
                                == format!("{} Body {}", kind.unwrap_or(""), state.unwrap_or(""))
                    })
            };
            let skipped: Vec<&str> = lane_elements(metrics.hold_use_dropped_state)
                .filter(|n| owned(n))
                .collect();
            for name in lane_elements(metrics.hold_use_dropped_state) {
                if skipped.contains(&name) {
                    elements.push(Element::Blank);
                    continue;
                }
                let (built, read) = match bodies.remove(name) {
                    Some(loaded) => loaded,
                    None => {
                        lua.element_colored_sliced(
                            button,
                            name,
                            NOTE_COLORS[0],
                            textures,
                            images,
                            pause.as_deref_mut(),
                        )
                        .await?
                    }
                };
                if read && ColorSet::default().place(name).is_some() {
                    colored.push(name);
                }
                elements.push(to_element(built, button, name, textures, notes));
            }
            let mut assembled = Lane::assemble(elements, metrics, lane.scroll_sign);
            if !colored.is_empty() {
                for color in &NOTE_COLORS[1..] {
                    let mut set = ColorSet::default();
                    for &name in &colored {
                        let (built, _) = lua
                            .element_colored_sliced(
                                button,
                                name,
                                color,
                                textures,
                                images,
                                pause.as_deref_mut(),
                            )
                            .await?;
                        if let Some(place) = set.place(name) {
                            *place = Some(to_element(built, button, name, textures, notes));
                        }
                    }
                    assembled.colors.push(set);
                }
            }
            Ok(assembled)
        }
    }

    fn from_lanes(
        name: String,
        lanes: Vec<Lane>,
        textures: Textures,
        notes: Vec<String>,
        srgb: bool,
        lua: Option<LuaSkin>,
    ) -> NoteskinPlay {
        let count = textures.keys().len();
        let (mut lanes, mut notes) = (lanes, notes);
        let skin_materials = compile_materials(&mut lanes, &mut notes);
        let mut play = NoteskinPlay {
            name,
            quads: std::cell::Cell::new(0),
            triangles: std::cell::Cell::new(0),
            drawn: std::cell::Cell::default(),
            model_scratch: std::cell::Cell::default(),
            camera: Camera {
                vanish: [0.0; 2],
                distance: 1.0,
            },
            initial: lanes.clone(),
            last_note_positions: Default::default(),
            lanes,
            textures,
            texture_ids: vec![None; count],
            shared: vec![None; count],
            next_seq: 0,
            started: false,
            last_song_time: None,
            timer: 0.0,
            notes,
            problems: Problems::default(),
            srgb,
            lua,
            lua_seen: 0,
            lua_failure: None,
            lua_stop_taken: false,
            lua_fuel: (0, 0),
            legacy: false,
            materials: Vec::new(),
            presses: Vec::new(),
            frame_time: (0.0, 0.0, 0.0),
            skin_materials,
            atlas: std::cell::OnceCell::new(),
        };
        play.notes.append(&mut play.problems.messages);
        play
    }

    /// How many of lane `lane`'s elements the skin draws (or blanks)
    /// rather than the built-in skin.
    pub fn own_elements(&self, lane: usize) -> usize {
        self.lanes
            .get(lane)
            .map_or(0, |l| l.elements().filter(|e| e.is_skin()).count())
    }

    /// Whether the skin's Lua runs in this play.
    pub fn runs_lua(&self) -> bool {
        self.lua.is_some()
    }

    /// The values its Lua reads for the skin's parameters
    /// (`ddi_noteskin::params`), as its declarations took them; none
    /// without Lua.
    pub fn parameters(&self) -> ddi_noteskin::params::Values {
        self.lua
            .as_ref()
            .map(LuaSkin::parameters)
            .unwrap_or_default()
    }

    /// Why the skin's Lua stopped during the play, if it did.
    pub fn lua_failure(&self) -> Option<&str> {
        self.lua_failure.as_ref().map(|(m, _)| m.as_str())
    }

    /// Whether the skin's Lua stopped on the clock alone (not its fuel,
    /// memory or a host limit): a slow or stalled machine, not the skin.
    pub fn lua_stopped_on_clock(&self) -> bool {
        self.lua_failure.as_ref().is_some_and(|(_, clock)| *clock)
    }

    /// Why the skin's Lua stopped, and whether on the clock alone
    /// ([`NoteskinPlay::lua_stopped_on_clock`]), the first time it is
    /// asked once it has: a shell reports a stop once per play it
    /// installs, a retry's included.
    pub fn take_lua_stop(&mut self) -> Option<(&str, bool)> {
        let (why, clock) = self.lua_failure.as_ref()?;
        if std::mem::replace(&mut self.lua_stop_taken, true) {
            return None;
        }
        Some((why.as_str(), *clock))
    }

    /// Fuel the skin's Lua spent in the last frame, and the most in one.
    pub fn lua_fuel(&self) -> (u64, u64) {
        self.lua_fuel
    }

    /// The textures the actors draw, by index.
    pub fn texture_keys(&self) -> &[TextureKey] {
        self.textures.keys()
    }

    /// Each texture's image, taken from `images` (a skin's images as the
    /// shell decoded them, by `(folder, path)`), by texture index: `None`
    /// for the built-in art and an image not decoded. `same` gives, for an
    /// image not decoded because its document is another's (a part the
    /// same as another component's), that one's: textures of one image are
    /// given one id ([`NoteskinPlay::set_texture`]), the first of them
    /// taking it. What no texture takes stays in `images`. Returns each
    /// `(texture, the texture it shares)`, as
    /// [`NoteskinPlay::share_textures`] does.
    pub fn take_images<T: Clone>(
        &mut self,
        images: &mut std::collections::HashMap<(usize, String), T>,
        same: &std::collections::HashMap<(usize, String), String>,
    ) -> (Vec<Option<T>>, Vec<(usize, usize)>) {
        let mut first: std::collections::HashMap<((usize, String), bool), usize> =
            Default::default();
        let mut took: std::collections::HashMap<(usize, String), usize> = Default::default();
        let mut pairs = Vec::new();
        let mut out: Vec<Option<T>> = Vec::with_capacity(self.texture_ids.len());
        for (i, k) in self.textures.keys().iter().enumerate() {
            let TextureKey::Skin {
                dir: ddi_noteskin::Dir::Folder(f),
                path,
            } = k
            else {
                out.push(None);
                continue;
            };
            let key = (*f, path.clone());
            let source = match same.get(&key) {
                Some(first) => (key.0, first.clone()),
                None => key,
            };
            match first.entry((source.clone(), self.textures.mipmaps(i))) {
                std::collections::hash_map::Entry::Occupied(j) => {
                    self.shared[i] = Some(*j.get());
                    pairs.push((i, *j.get()));
                    out.push(None);
                }
                std::collections::hash_map::Entry::Vacant(v) => {
                    v.insert(i);
                    // An image a texture of another mipmapping took
                    // already: copied.
                    let image = match took.get(&source) {
                        Some(&j) => out[j].clone(),
                        None => images.remove(&source),
                    };
                    took.entry(source).or_insert(i);
                    out.push(image);
                }
            }
        }
        (out, pairs)
    }

    /// Actual textures referenced by a lane's built actors, for authoring inspection. This
    /// follows Lua redirects and shared sprites rather than guessing filenames. An unknown
    /// lane or element returns an empty list; procedural and blank actors have no texture.
    pub fn element_texture_keys(&self, lane: usize, name: &str) -> Vec<TextureKey> {
        let Some(lane) = self.lanes.get(lane) else {
            return Vec::new();
        };
        let mut elements = Vec::new();
        match name {
            "Receptor" => elements.push(&lane.receptor),
            "Explosion" => elements.push(&lane.explosion),
            "Tap Note" => {
                elements.push(&lane.tap);
                elements.extend(lane.colors.iter().filter_map(|c| c.tap.as_ref()));
            }
            "Tap Lift" => {
                elements.push(&lane.lift);
                elements.extend(lane.colors.iter().filter_map(|c| c.lift.as_ref()));
            }
            "Tap Fake" => {
                elements.push(&lane.fake);
                elements.extend(lane.colors.iter().filter_map(|c| c.fake.as_ref()));
            }
            "Tap Mine" => {
                elements.push(&lane.mine);
                elements.extend(lane.colors.iter().filter_map(|c| c.mine.as_ref()));
            }
            _ => {
                let mut words = name.split(' ');
                let kind = words.next();
                let Some(parts) = (match kind {
                    Some("Hold") => Some(&lane.hold),
                    Some("Roll") => Some(&lane.roll),
                    _ => None,
                }) else {
                    return Vec::new();
                };
                let part = words.next();
                let state = match words.next() {
                    Some("Active") => 1,
                    Some("Inactive") => 0,
                    Some("Dropped") => 2,
                    _ => return Vec::new(),
                };
                let (entries, index) = match part {
                    Some("Head") => (&parts.head, 0),
                    Some("Tail") => (&parts.tail, 1),
                    Some("Topcap") => (&parts.topcap, 2),
                    Some("Body") => (&parts.body, 3),
                    Some("Bottomcap") => (&parts.bottomcap, 4),
                    _ => return Vec::new(),
                };
                if let Some(element) = entries.get(state) {
                    elements.push(element);
                }
                for color in &lane.colors {
                    let slots = if kind == Some("Hold") {
                        &color.hold
                    } else {
                        &color.roll
                    };
                    if let Some(element) = slots
                        .get(index)
                        .and_then(|s| s.get(state))
                        .and_then(Option::as_ref)
                    {
                        elements.push(element);
                    }
                }
            }
        }
        let mut out = Vec::new();
        for element in elements {
            if let Element::Actor(actor) = element {
                for id in actor.textures() {
                    if let Some(key) = self.textures.keys().get(id as usize)
                        && !out.contains(key)
                    {
                        out.push(key.clone());
                    }
                }
            }
        }
        out
    }

    /// Whether texture `index` gets mipmaps (a model's).
    pub fn texture_mipmaps(&self, index: usize) -> bool {
        self.textures.mipmaps(index)
    }

    /// The id texture `t` was given ([`NoteskinPlay::set_texture`]).
    fn texture_id(&self, t: u32) -> Option<u32> {
        self.texture_ids.get(t as usize).copied().flatten()
    }

    /// Tells which renderer texture stands for texture `index` (and for
    /// those sharing its image).
    pub fn set_texture(&mut self, index: usize, id: u32) {
        for (i, slot) in self.texture_ids.iter_mut().enumerate() {
            if i == index || self.shared.get(i) == Some(&Some(index)) {
                *slot = Some(id);
            }
        }
        self.atlas = std::cell::OnceCell::new();
    }

    /// Finds the textures whose images an earlier texture's has (`key`:
    /// a hash of a texture's pixels, `None` for one not to share; `same`:
    /// whether two are the same pixels), each then given the earlier
    /// one's id by [`NoteskinPlay::set_texture`]: its own image need not
    /// be uploaded. Only textures mipmapped alike share. Returns each
    /// `(texture, the texture it shares)`.
    pub fn share_textures(
        &mut self,
        key: impl Fn(usize) -> Option<u64>,
        same: impl Fn(usize, usize) -> bool,
    ) -> Vec<(usize, usize)> {
        let mut first: std::collections::HashMap<(u64, bool), Vec<usize>> = Default::default();
        let mut pairs = Vec::new();
        for i in 0..self.texture_ids.len() {
            let Some(k) = key(i) else {
                continue;
            };
            let earlier = first.entry((k, self.textures.mipmaps(i))).or_default();
            match earlier.iter().find(|&&j| same(j, i)) {
                Some(&j) => {
                    // Those sharing this one's share that one's.
                    for s in &mut self.shared {
                        if *s == Some(i) {
                            *s = Some(j);
                        }
                    }
                    self.shared[i] = Some(j);
                    pairs.push((i, j));
                }
                None => earlier.push(i),
            }
        }
        pairs
    }

    /// Its components' parts' images, as registered: drawn whole (a long
    /// note's body tiles, which repeat, materials' images, sampled whole,
    /// and models' mipmapped textures left out).
    fn packable(&self) -> Vec<u32> {
        let mut skin: Vec<u32> = Vec::new();

        let parts = |c: &ddi_noteskin::component::Component, out: &mut Vec<u32>| {
            for p in &c.parts {
                out.extend(&p.textures);
            }
        };
        let mut visit = |e: &Element| match e {
            Element::Component(c) => parts(c, &mut skin),
            Element::LongNote(n) => {
                for c in [n.head.as_ref(), n.cap.as_ref()].into_iter().flatten() {
                    parts(c, &mut skin);
                }
            }
            Element::Lift(l) => {
                parts(&l.head, &mut skin);
                if let Some(t) = &l.trail {
                    parts(t, &mut skin);
                }
            }
            _ => {}
        };
        for e in self.lanes.iter().flat_map(Lane::every) {
            visit(e);
        }
        // A texture also sampled whole by a material, or repeated as a
        // long note's body, stays alone.
        fn images_of(c: &ddi_noteskin::component::Component, alone: &mut Vec<u32>) {
            for p in &c.parts {
                if let Some(m) = &p.material {
                    alone.extend(&m.images);
                }
            }
        }
        fn keep_alone(e: &Element, alone: &mut Vec<u32>) {
            match e {
                Element::Component(c) => images_of(c, alone),
                Element::LongNote(n) => {
                    for p in &n.body.parts {
                        alone.extend(&p.textures);
                    }
                    for c in [n.head.as_ref(), n.cap.as_ref()].into_iter().flatten() {
                        images_of(c, alone);
                    }
                    if let Some(m) = n.fill.as_ref().and_then(|f| f.material.as_ref()) {
                        alone.extend(&m.images);
                    }
                }
                Element::Lift(l) => {
                    images_of(&l.head, alone);
                    if let Some(m) = l.fill.as_ref().and_then(|f| f.material.as_ref()) {
                        alone.extend(&m.images);
                    }
                }
                _ => {}
            }
        }
        let mut alone: Vec<u32> = Vec::new();
        for e in self.lanes.iter().flat_map(Lane::every) {
            keep_alone(e, &mut alone);
        }
        skin.sort_unstable();
        skin.dedup();
        // A sprite hold's or roll's body repeats its texture (drawn
        // wrapped), and an image of it may be a part's too
        // ([`NoteskinPlay::share_textures`]): it stays whole. (Models'
        // textures, the other wrapped, get mipmaps, shared with no part.)
        for lane in &self.lanes {
            let bodies = lane.hold.body.iter().chain(&lane.roll.body).chain(
                lane.colors
                    .iter()
                    .flat_map(|c| c.hold[3].iter().chain(&c.roll[3]).flatten()),
            );
            for e in bodies {
                if let Element::Actor(a) = e {
                    alone.extend(a.textures());
                }
            }
        }
        skin.retain(|t| !alone.contains(t) && !self.textures.mipmaps(*t as usize));
        // A shared image stays whole when any of its textures does.
        let id = |t: &u32| self.texture_id(*t);
        let alone: Vec<u32> = alone.iter().filter_map(id).collect();
        let mut ids: Vec<u32> = skin.iter().filter_map(id).collect();
        ids.sort_unstable();
        ids.dedup();
        ids.retain(|i| !alone.contains(i));
        ids
    }

    /// Whether the target is sRGB (colors are converted for it).
    pub fn set_srgb(&mut self, srgb: bool) {
        self.srgb = srgb;
    }

    /// The slot of a material's parameters this frame
    /// ([`crate::material::MaterialQuad::slot`]): the same parameters the
    /// same slot; `None` once every slot is taken.
    fn material_slot(&mut self, params: crate::material::MaterialParams) -> Option<u16> {
        if let Some(i) = self.materials.iter().position(|p| *p == params) {
            return Some(i as u16);
        }
        if self.materials.len() >= crate::material::SLOTS {
            return None;
        }
        self.materials.push(params);
        Some(self.materials.len() as u16 - 1)
    }

    /// Quads the last frame drew.
    pub fn last_quads(&self) -> usize {
        self.quads.get()
    }

    /// The first engine event not seen yet.
    pub fn next_seq(&self) -> u64 {
        self.next_seq
    }

    /// Starts over for a new play of the same layout: commands from the
    /// start, no events seen.
    pub fn restart(&mut self) {
        self.lanes = self.initial.clone();
        self.started = false;
        self.next_seq = 0;
        self.last_song_time = None;
        self.last_note_positions.clear();
        self.timer = 0.0;
    }

    /// Actor commands that went wrong while playing (capped).
    pub fn problems(&self) -> &[String] {
        &self.problems.messages
    }

    /// Plays the commands of the events since the last update and of the
    /// lanes' hold states, then advances every actor to `frame`.
    pub fn update<'e>(
        &mut self,
        frame: &Frame,
        events: impl Iterator<Item = &'e SkinEvent>,
        reverse: bool,
    ) {
        self.materials.clear();
        self.frame_time = (frame.song_time, frame.beat, frame.roll_window_seconds);
        // A seek back forgets presses that have not happened yet.
        if self
            .last_song_time
            .is_some_and(|last| frame.song_time < last)
        {
            self.presses.clear();
            for lane in &mut self.lanes {
                lane.hits.clear();
                lane.ended = None;
                lane.step = None;
            }
        }
        self.presses
            .resize(frame.receptors.len(), LanePresses::default());
        for (p, r) in self.presses.iter_mut().zip(&frame.receptors) {
            if r.pressed != p.down {
                if r.pressed {
                    p.pressed = Some(frame.song_time);
                } else {
                    p.released = Some(frame.song_time);
                }
                p.down = r.pressed;
            }
        }
        for lane in &mut self.lanes {
            if let Element::Actor(a) = &lane.explosion {
                lane.explosion_rotation = a.base_rotation[2];
            }
        }
        if self
            .last_song_time
            .is_some_and(|last| frame.song_time < last)
        {
            self.last_note_positions.clear();
        }
        if let Some(lua) = &mut self.lua {
            lua.begin_frame(
                Position {
                    seconds: frame.song_time,
                    beat: frame.beat,
                },
                FrameBudget::default(),
            );
            for lane in &mut self.lanes {
                for e in lane.elements_mut() {
                    if let Element::Actor(a) = e {
                        let lent = lua.lend(a);
                        debug_assert!(lent);
                    }
                }
            }
        }
        let failed = self.play_frame(frame, events, reverse);
        self.last_note_positions.clear();
        if let Some(lua) = &mut self.lua {
            for lane in &mut self.lanes {
                for e in lane.elements_mut() {
                    if let Element::Actor(a) = e {
                        let back = lua.reclaim(a);
                        debug_assert!(back);
                    }
                }
            }
            let fuel = lua.frame_fuel();
            self.lua_fuel = (fuel, self.lua_fuel.1.max(fuel));
            for w in lua.warnings_since(self.lua_seen) {
                self.lua_seen += 1;
                if self.problems.messages.len() < 64 {
                    self.problems.messages.push(w);
                }
            }
        }
        if let Some(f) = failed {
            self.stop_lua(&f);
        }
    }

    /// The skin's Lua stopped: it is dropped, and every element is the
    /// built-in skin's for the rest of the play (a retry too).
    fn stop_lua(&mut self, f: &Failure) {
        self.lua = None;
        let message = format!("the skin's Lua stopped ({f}); the built-in skin is drawn");
        self.problems.messages.push(message.clone());
        self.lua_failure = Some((message, matches!(f, Failure::Deadline)));
        for lane in self.lanes.iter_mut().chain(self.initial.iter_mut()) {
            for e in lane.elements_mut() {
                *e = Element::BuiltIn;
            }
        }
    }

    /// [`NoteskinPlay::update`]'s commands and updates, the trees lent to
    /// the Lua when there is one; the failure that stopped the Lua.
    fn play_frame<'e>(
        &mut self,
        frame: &Frame,
        events: impl Iterator<Item = &'e SkinEvent>,
        reverse: bool,
    ) -> Option<Failure> {
        let NoteskinPlay {
            lanes,
            lua,
            problems: p,
            started,
            next_seq,
            legacy,
            last_note_positions,
            ..
        } = self;
        let legacy = *legacy;
        let mut cmd = Commands {
            lua: lua.as_mut(),
            failed: None,
        };
        if !*started {
            *started = true;
            // OpenITG plays no command on a receptor as a play starts.
            for lane in lanes.iter_mut().filter(|_| !legacy) {
                // Each column's own reverse (`ReceptorArrow::Update`); a
                // play's scroll does not change once started.
                let reversed = reverse != (lane.scroll_sign < 0);
                cmd.play(
                    &mut lane.receptor,
                    if reversed { "ReverseOn" } else { "ReverseOff" },
                    &[],
                    p,
                );
                cmd.play(&mut lane.receptor, "On", &[], p);
            }
        }
        for e in events {
            if e.seq < *next_seq {
                continue;
            }
            *next_seq = e.seq + 1;
            let column = i64::from(e.lane);
            let Some(lane) = lanes.get_mut(usize::from(e.lane)) else {
                continue;
            };
            let bright = |b: bool| if b { "Bright" } else { "Dim" };
            // `GhostArrowRow::DidTapNote`/`DidHoldNote`: the message, then
            // the commands.
            let mut explode = |score: (&str, String),
                               b: bool,
                               names: &[&'static str; 3],
                               cmd: &mut Commands,
                               p: &mut Problems| {
                // An explosion component's instance, by its score's name.
                if lane.hits.len() == MAX_HITS {
                    lane.hits.pop_front();
                }
                lane.hits.push_back(Hit {
                    at: e.song_time,
                    name: names[2],
                    bright: b,
                });
                let mut params = vec![
                    (score.0, Param::Str(score.1)),
                    ("Column", Param::Int(column)),
                    ("Autoplay", Param::Bool(e.autoplay)),
                    ("DanoniJudgments", Param::Bool(e.danoni_judgments)),
                ];
                if let Some(hit) = e.note {
                    let y = last_note_positions
                        .get(&hit.index)
                        .copied()
                        .unwrap_or(hit.y);
                    let y = y
                        * 64.0
                        * if reverse != (lane.scroll_sign < 0) {
                            -1.0
                        } else {
                            1.0
                        };
                    let (sin, cos) = lane.explosion_rotation.to_radians().sin_cos();
                    params.push(("HitX", Param::Float(f64::from(y * sin))));
                    params.push(("HitY", Param::Float(f64::from(y * cos))));
                }
                if b {
                    params.push(("Bright", Param::Bool(true)));
                }
                cmd.play(&mut lane.explosion, "ColumnJudgmentMessage", &params, p);
                for n in names {
                    cmd.play(&mut lane.explosion, n, &[], p);
                }
                // `GhostArrowRow::DidTapNote`: the score to one row.
                if legacy && let [_, row, score] = names {
                    cmd.play(&mut lane.explosion, &format!("{row}{score}"), &[], p);
                }
            };
            let tap = |s: &str| ("TapNoteScore", format!("TapNoteScore_{s}"));
            match e.kind {
                SkinEventKind::Press => cmd.play(
                    &mut lane.receptor,
                    "Press",
                    &[("Autoplay", Param::Bool(e.autoplay))],
                    p,
                ),
                SkinEventKind::Lift => cmd.play(
                    &mut lane.receptor,
                    "Lift",
                    &[("Autoplay", Param::Bool(e.autoplay))],
                    p,
                ),
                SkinEventKind::Step(_) | SkinEventKind::StepMine => {
                    let name = match e.kind {
                        SkinEventKind::Step(Some(j)) => score_name(j),
                        SkinEventKind::Step(None) => "None",
                        _ => "HitMine",
                    };
                    lane.step = Some(Hit {
                        at: e.song_time,
                        name,
                        bright: false,
                    });
                    cmd.play(&mut lane.receptor, name, &[], p)
                }
                SkinEventKind::Tap {
                    judgement,
                    bright: b,
                }
                | SkinEventKind::NoteTap {
                    judgement,
                    bright: b,
                } if matches!(e.kind, SkinEventKind::NoteTap { .. })
                    == lane.metrics.tap_judgments_per_note =>
                {
                    explode(
                        tap(score_name(judgement)),
                        b,
                        &["Judgment", bright(b), score_name(judgement)],
                        &mut cmd,
                        p,
                    )
                }
                SkinEventKind::Tap { .. } | SkinEventKind::NoteTap { .. } => {}
                SkinEventKind::Held { bright: b } => explode(
                    ("HoldNoteScore", "HoldNoteScore_Held".into()),
                    b,
                    &["Judgment", bright(b), "Held"],
                    &mut cmd,
                    p,
                ),
                SkinEventKind::HitMine => explode(
                    tap("HitMine"),
                    false,
                    &["Judgment", "Dim", "HitMine"],
                    &mut cmd,
                    p,
                ),
            }
        }
        // `GhostArrowRow::Update`: HoldingOn/Off and RollOn/Off on change.
        for (i, lane) in lanes.iter_mut().enumerate() {
            let now = frame.receptors.get(i).and_then(|r| r.holding);
            if now != lane.holding {
                match lane.holding {
                    Some(HoldKind::Hold) => cmd.play(&mut lane.explosion, "HoldingOff", &[], p),
                    Some(HoldKind::Roll) => cmd.play(&mut lane.explosion, "RollOff", &[], p),
                    None => {}
                }
                match now {
                    Some(HoldKind::Hold) => cmd.play(&mut lane.explosion, "HoldingOn", &[], p),
                    Some(HoldKind::Roll) => cmd.play(&mut lane.explosion, "RollOn", &[], p),
                    None => {}
                }
                if let Some(since) = lane.holding_since {
                    let name = held_name(lane.holding);
                    lane.ended = name.map(|name| (name, since, frame.song_time));
                }
                lane.holding = now;
                lane.holding_since = now.map(|_| frame.song_time);
            }
        }
        let dt = match self.last_song_time {
            Some(t) => (frame.song_time - t).clamp(0.0, 0.25) as f32,
            None => 0.0,
        };
        self.last_song_time = Some(frame.song_time);
        self.timer += dt;
        let clocks = Clocks {
            timer: self.timer,
            music_seconds: frame.song_time as f32,
            beat: frame.beat as f32,
            music_seconds_no_offset: frame.song_time as f32,
            beat_no_offset: frame.beat as f32,
        };
        for lane in &mut self.lanes {
            for e in lane.elements_mut() {
                cmd.update(e, dt, &clocks, &mut self.problems);
            }
        }
        cmd.failed
    }

    /// Appends `actor`'s quads at `transform`, colored, with `offset` added
    /// to its texture coordinates (a note's color).
    fn emit(
        &self,
        actor: &mut Actor,
        transform: [f32; 6],
        offset: [f32; 2],
        out: &mut Vec<TexQuad>,
    ) {
        let mut drawn = self.drawn.take();
        let view = self.camera.view();
        actor.draw_in(&view, transform, [1.0; 4], [1.0, 1.0, 1.0, 0.0], &mut drawn);
        for d in drawn.drain(..) {
            match d {
                Drawn::Quad(q) => self.push(q, offset, out),
                Drawn::Model(m) => self.push_model(&m, offset, out),
                // Skins make no text, clipping frames or render targets.
                Drawn::Text(_) | Drawn::ClipPush(_) | Drawn::ClipPop | Drawn::Target(_) => {}
            }
        }
        self.drawn.set(drawn);
    }

    /// A model's triangles (`Model::DrawPrimitives`): every mesh's diffuse
    /// pass, then every mesh's glow pass.
    fn push_model(&self, m: &ModelDraw, offset: [f32; 2], out: &mut Vec<TexQuad>) {
        self.quads.set(self.quads.get() + 1);
        if m.diffuse[3] < 0.001 && m.glow[3] < 0.001 {
            return;
        }
        let mut scratch = self.model_scratch.take();
        let ModelScratch {
            seen,
            corners,
            meshes,
        } = &mut scratch;
        seen.clear();
        // An `fov` frame's camera, or the field's.
        triangles_with(m, &self.camera, offset, seen, corners);
        // A frame's budget: models past it are not drawn.
        let passes = 1 + usize::from(m.glow[3] > 0.0001);
        let total = self.triangles.get() + seen.len() * passes;
        if total > MAX_FRAME_TRIANGLES {
            self.model_scratch.set(scratch);
            return;
        }
        self.triangles.set(total);
        // `DrawPrimitives` sets the blend back to normal after each mesh:
        // the actor's blends the first mesh, and the glow pass only when
        // no diffuse pass came before it.
        let blend = actors2d::blend(m.blend);
        let diffuse = m.diffuse[3] > 0.0;
        let mesh_blend = |mesh: usize| {
            if mesh == 0 { blend } else { QuadBlend::Normal }
        };
        let glow_blend = if diffuse { QuadBlend::Normal } else { blend };
        // A mesh's texture id (`UNTEXTURED`: its color alone; `None`: not
        // loaded) and color; a mesh without a material is grey
        // (`DrawPrimitives`' fallback material, unlit).
        meshes.clear();
        meshes.extend(m.geometry.meshes.iter().map(|mesh| {
            match mesh.material.and_then(|i| m.materials.get(i)) {
                Some(&(Some(t), color, _)) => (self.texture_id(t), color),
                Some(&(None, color, _)) => (Some(UNTEXTURED), color),
                None => (Some(UNTEXTURED), [0.9, 0.9, 0.9, 1.0]),
            }
        }));
        let triangle = |s: &Seen, texture: u32, color: [f32; 4], glow: bool, blend| TexQuad {
            glow,
            blend,
            wrap: true,
            ..TexQuad::new(
                Shape::Triangle {
                    points: s.points,
                    w: s.w,
                    uv: s.uv,
                },
                to_target(color, self.srgb),
                texture,
            )
        };
        if diffuse {
            for s in seen.iter() {
                if let (Some(texture), color) = meshes[s.mesh] {
                    out.push(triangle(s, texture, color, false, mesh_blend(s.mesh)));
                }
            }
        }
        if m.glow[3] > 0.0001 {
            for s in seen.iter() {
                if let (Some(texture), _) = meshes[s.mesh] {
                    out.push(triangle(s, texture, m.glow, true, glow_blend));
                }
            }
        }
        self.model_scratch.set(scratch);
    }

    /// A quad's diffuse and glow passes (`Sprite::DrawTexture`), its
    /// corners' colors, its perspective and its mask.
    fn push(&self, q: Quad, offset: [f32; 2], out: &mut Vec<TexQuad>) {
        self.quads.set(self.quads.get() + 1);
        if q.depth.clear {
            // `clearzbuffer`: the mask cleared over the whole target.
            out.push(clear_quad());
        }
        let Some(mask) = actors2d::drawn_mask(q.depth, q.blend) else {
            return;
        };
        let Some((shape, texture)) = textured::actor_quad(&q, &self.texture_ids, offset) else {
            return;
        };
        let base = TexQuad {
            blend: actors2d::blend(q.blend),
            mask,
            ..TexQuad::new(shape, [1.0; 4], texture)
        };
        textured::push_passes(base, q.diffuse, q.glow, self.srgb, out);
    }

    /// A note part drawn as StepMania's `DrawActor` does: placed by the
    /// field, its frame from the beat, its color by texture translation,
    /// its diffuse and glow from the appearance mods and the hold's life.
    #[allow(clippy::too_many_arguments)]
    fn note_actor(
        &self,
        element: &mut Element,
        metrics: &NoteDisplay,
        part: Part,
        beat: f32,
        (alpha, glow): (f32, f32),
        frame: &Frame,
        at: [f32; 2],
        k: f32,
        color_scale: f32,
        out: &mut Vec<TexQuad>,
    ) {
        let Element::Actor(actor) = element else {
            return;
        };
        let m = metrics.part(part);
        let mut offset = [0.0; 2];
        let mut padded = [1.0; 2];
        let percent = animation_percent(
            m,
            metrics.animation_is_beat_based,
            frame.beat as f32,
            frame.song_time as f32,
            beat,
        );
        set_note_animation(actor, percent, m.animation_on_children);
        if let Kind::Sprite(s) = &actor.kind {
            padded = s.padded_to_image;
        }
        if let Some(o) = color_offset(m, beat, false) {
            offset = [o[0] * padded[0], o[1] * padded[1]];
        }
        if alpha <= 0.0 && glow <= 0.0 {
            return;
        }
        let alpha = alpha.max(0.0);
        let glow = glow.clamp(0.0, 1.0);
        actor.set_note_colors(
            [color_scale, color_scale, color_scale, alpha],
            [1.0, 1.0, 1.0, glow],
        );
        actor.reset_placement();
        self.emit(actor, [k, 0.0, 0.0, k, at[0], at[1]], offset, out);
    }
}

impl Lane {
    fn elements(&self) -> impl Iterator<Item = &Element> {
        [
            &self.receptor,
            &self.explosion,
            &self.tap,
            &self.lift,
            &self.fake,
            &self.mine,
        ]
        .into_iter()
        .chain(self.hold.all())
        .chain(self.roll.all())
    }

    /// Every element, the colors' too.
    fn every(&self) -> impl Iterator<Item = &Element> {
        self.elements()
            .chain(self.colors.iter().flat_map(ColorSet::all))
    }

    /// Every element, the colors' too.
    fn elements_mut(&mut self) -> impl Iterator<Item = &mut Element> {
        [
            &mut self.receptor,
            &mut self.explosion,
            &mut self.tap,
            &mut self.lift,
            &mut self.fake,
            &mut self.mine,
        ]
        .into_iter()
        .chain(self.hold.all_mut())
        .chain(self.roll.all_mut())
        .chain(self.colors.iter_mut().flat_map(ColorSet::all_mut))
    }
}

impl HoldParts {
    fn parts_mut(&mut self) -> [&mut Vec<Element>; 5] {
        [
            &mut self.head,
            &mut self.tail,
            &mut self.topcap,
            &mut self.body,
            &mut self.bottomcap,
        ]
    }

    fn all(&self) -> impl Iterator<Item = &Element> {
        self.head
            .iter()
            .chain(self.tail.iter())
            .chain(self.topcap.iter())
            .chain(self.body.iter())
            .chain(self.bottomcap.iter())
    }

    fn all_mut(&mut self) -> impl Iterator<Item = &mut Element> {
        self.head
            .iter_mut()
            .chain(self.tail.iter_mut())
            .chain(self.topcap.iter_mut())
            .chain(self.body.iter_mut())
            .chain(self.bottomcap.iter_mut())
    }
}

/// A note's seed for its materials, 0 to 1, the same every frame.
fn note_seed(index: usize) -> f32 {
    ((index as u64).wrapping_mul(0x9e37_79b9_7f4a_7c15) >> 40) as f32 / (1u64 << 24) as f32
}

/// The explosion a hold of `kind` shows while it lasts.
fn held_name(kind: Option<HoldKind>) -> Option<&'static str> {
    match kind? {
        HoldKind::Hold => Some("Holding"),
        HoldKind::Roll => Some("Rolling"),
    }
}

/// An effect keeps its variation when older judgments leave the bounded queue.
/// Fold both halves before converting to usize, also on 32-bit wasm.
fn hit_seed(lane: usize, at: f64) -> f32 {
    let bits = at.to_bits();
    note_seed(((bits ^ (bits >> 32)) as u32 as usize) ^ lane.wrapping_mul(MAX_HITS))
}

/// Compiles the materials of the lanes' components
/// ([`crate::material::compile`]), each source once, and points their parts
/// at them; one that does not compile is said in `notes`, and its parts
/// draw their own images.
fn compile_materials(
    lanes: &mut [Lane],
    notes: &mut Vec<String>,
) -> Vec<crate::material::Compiled> {
    let mut compiled: Vec<crate::material::Compiled> = Vec::new();
    let mut sources: Vec<(String, Option<u32>)> = Vec::new();
    let mut visit = |e: &mut Element| {
        let parts: Vec<&mut ddi_noteskin::component::Part> = match e {
            Element::Component(c) => c.parts.iter_mut().collect(),
            Element::LongNote(n) => {
                let n = &mut **n;
                let mut parts: Vec<_> = n.body.parts.iter_mut().collect();
                for c in [n.head.as_mut(), n.cap.as_mut()].into_iter().flatten() {
                    parts.extend(c.parts.iter_mut());
                }
                parts.extend(n.fill.as_mut());
                parts
            }
            Element::Lift(l) => {
                let l = &mut **l;
                let mut parts: Vec<_> = l.head.parts.iter_mut().collect();
                if let Some(t) = l.trail.as_mut() {
                    parts.extend(t.parts.iter_mut());
                }
                parts.extend(l.fill.as_mut());
                parts
            }
            _ => return,
        };
        for part in parts {
            let Some(m) = part.material.as_deref_mut() else {
                continue;
            };
            let index = match sources.iter().find(|(s, _)| *s == m.source) {
                Some((_, index)) => *index,
                None => {
                    let index = match crate::material::compile(&m.source) {
                        Ok(c) => {
                            compiled.push(c);
                            Some(compiled.len() as u32 - 1)
                        }
                        Err(e) => {
                            notes.push(format!("{}: {e}", m.file));
                            None
                        }
                    };
                    sources.push((m.source.clone(), index));
                    index
                }
            };
            m.compiled = index;
        }
    };
    for e in lanes.iter_mut().flat_map(Lane::elements_mut) {
        visit(e);
    }
    compiled
}

/// `a` after `b`, as 2D affine maps `[m0, m1, m2, m3, m4, m5]` (`x' = m0 x
/// + m2 y + m4`, `y' = m1 x + m3 y + m5`).
fn affine_mul(a: [f32; 6], b: [f32; 6]) -> [f32; 6] {
    ddi_actor::mul(&a, &b)
}

/// A turn of `degrees`, clockwise on the screen (its y down), as actors
/// turn.
fn rotation(degrees: f32) -> [f32; 6] {
    let (sin, cos) = degrees.to_radians().sin_cos();
    [cos, sin, -sin, cos, 0.0, 0.0]
}

/// The blend of a long note's or a lift's layer drawn through `mask`:
/// into the mask only (no color) when it writes or erases it.
fn mask_blend(mask: Mask) -> QuadBlend {
    if matches!(mask, Mask::Write | Mask::Erase) {
        QuadBlend::NoEffect
    } else {
        QuadBlend::Normal
    }
}

/// `q` turned over about the field's row `head` (a layer drawn down the
/// lane, scrolling up) when `dir` is negative; a quad's shape only.
fn mirror_about(mut q: TexQuad, head: f32, dir: f32) -> TexQuad {
    if dir < 0.0
        && let Shape::Quad {
            ref mut transform, ..
        } = q.shape
    {
        transform[1] *= -1.0;
        transform[3] *= -1.0;
        transform[5] = head - (transform[5] - head);
    }
    q
}

/// A quad clearing the mask over the whole target (opaque, so the alpha
/// test keeps it; it draws no color). [`bound_clears`] shrinks it to what
/// was drawn.
fn clear_quad() -> TexQuad {
    TexQuad {
        mask: Mask::Clear,
        blend: QuadBlend::NoEffect,
        ..TexQuad::new(
            Shape::Quad {
                transform: [1e6, 0.0, 0.0, 1e6, 0.0, 0.0],
                uv: [0.0, 0.0, 1.0, 1.0],
            },
            [1.0; 4],
            UNTEXTURED,
        )
    }
}

/// Narrows the mask clears among `quads` (a long note's material passes)
/// to the box the others cover: each then clears that note's pixels, not
/// the whole target, every pass.
fn bound_clears(quads: &mut [TexQuad]) {
    let mut bounds = [
        f32::INFINITY,
        f32::INFINITY,
        f32::NEG_INFINITY,
        f32::NEG_INFINITY,
    ];
    let mut add = |[x, y]: [f32; 2]| {
        bounds = [
            bounds[0].min(x),
            bounds[1].min(y),
            bounds[2].max(x),
            bounds[3].max(y),
        ];
    };
    for q in quads.iter().filter(|q| q.mask != Mask::Clear) {
        match q.shape {
            Shape::Quad { transform: m, .. } => {
                for (cx, cy) in [(-0.5, -0.5), (0.5, -0.5), (-0.5, 0.5), (0.5, 0.5)] {
                    add([m[0] * cx + m[2] * cy + m[4], m[1] * cx + m[3] * cy + m[5]]);
                }
            }
            Shape::Projected { corners, .. } => corners.into_iter().for_each(&mut add),
            Shape::Triangle { points, .. } => points.into_iter().for_each(&mut add),
        }
    }
    // Nothing else drawn (or not a number): the clears stay as they are.
    if !(bounds[0] <= bounds[2] && bounds[1] <= bounds[3]) {
        return;
    }
    let [x0, y0] = [bounds[0].floor() - 1.0, bounds[1].floor() - 1.0];
    let [x1, y1] = [bounds[2].ceil() + 1.0, bounds[3].ceil() + 1.0];
    for q in quads.iter_mut().filter(|q| q.mask == Mask::Clear) {
        q.shape = Shape::Quad {
            transform: [x1 - x0, 0.0, 0.0, y1 - y0, (x0 + x1) * 0.5, (y0 + y1) * 0.5],
            uv: [0.0, 0.0, 1.0, 1.0],
        };
    }
}

/// A sprite element's current frame: texture, rectangle (image fractions)
/// and shown size in StepMania pixels.
fn sprite_frame(element: &Element) -> Option<(u32, [f32; 4], [f32; 2])> {
    let Element::Actor(a) = element else {
        return None;
    };
    match &a.kind {
        Kind::Sprite(s) => Some((s.texture, s.states[s.state()].rect, s.size)),
        _ => None,
    }
}

impl NoteLayer for NoteskinPlay {
    fn materials(&self) -> &[crate::material::MaterialParams] {
        &self.materials
    }

    fn skin_materials(&self) -> &[crate::material::Compiled] {
        &self.skin_materials
    }

    fn atlas_textures(&self) -> &[u32] {
        self.atlas.get_or_init(|| self.packable())
    }

    fn draws(&self, lane: usize, part: LayerPart) -> bool {
        let Some(l) = self.lanes.get(lane) else {
            return false;
        };
        match part {
            LayerPart::Receptor => l.receptor.is_skin(),
            LayerPart::Tap => l.tap.is_skin(),
            LayerPart::Lift => l.lift.is_skin(),
            LayerPart::Fake => l.fake.is_skin(),
            LayerPart::Mine => l.mine.is_skin(),
            LayerPart::Hold => l.hold.drawn(),
            LayerPart::Roll => l.roll.drawn(),
            LayerPart::HoldHead => l.hold.heads(),
            LayerPart::RollHead => l.roll.heads(),
        }
    }

    fn receptors(
        &mut self,
        _frame: &Frame,
        layout: &Layout,
        geo: &FieldGeometry,
        out: &mut Vec<TexQuad>,
    ) {
        self.quads.set(0);
        self.triangles.set(0);
        self.camera = Camera::of(geo);
        self.column_actors(layout, geo, out, |l| &mut l.receptor);
    }

    fn explosions(
        &mut self,
        _frame: &Frame,
        layout: &Layout,
        geo: &FieldGeometry,
        out: &mut Vec<TexQuad>,
    ) {
        self.camera = Camera::of(geo);
        self.column_actors(layout, geo, out, |l| &mut l.explosion);
        self.explosion_instances(layout, geo, out);
    }

    /// `NoteDisplay::DrawHold`: top cap, body, bottom cap, then the tail and
    /// the head, holds in row order.
    fn holds(
        &mut self,
        frame: &Frame,
        layout: &Layout,
        geo: &FieldGeometry,
        out: &mut Vec<TexQuad>,
    ) {
        self.camera = Camera::of(geo);
        let k = geo.arrow / 64.0;
        let mut lanes = std::mem::take(&mut self.lanes);
        let mut order: Vec<&NoteSprite> = frame
            .notes
            .iter()
            .filter(|n| {
                matches!(
                    n.kind,
                    SpriteKind::HoldHead { .. } | SpriteKind::RollHead { .. }
                )
            })
            .collect();
        order.sort_by(|a, b| a.beat.total_cmp(&b.beat));
        for note in order {
            let (tail_y, active, dropped, roll) = match note.kind {
                SpriteKind::HoldHead {
                    tail_y,
                    active,
                    dropped,
                } => (tail_y, active, dropped, false),
                SpriteKind::RollHead {
                    tail_y,
                    active,
                    dropped,
                } => (tail_y, active, dropped, true),
                _ => continue,
            };
            let i = usize::from(note.lane);
            let (Some(l), Some(lane)) = (layout.lanes.get(i), lanes.get_mut(i)) else {
                continue;
            };
            // The hold in its head's color (the copies swapped back below).
            let note_type = note_type(note.beat);
            lane.swap_hold_colors(note_type, roll);
            self.last_note_positions.insert(note.note_index, note.y);
            let m = &lane.metrics;
            // The Active sprites while the button is held (always inside a
            // roll; `Player.cpp:1381-1386`, `1414-1424`).
            let pressed = frame.receptors.get(i).is_some_and(|r| r.pressed);
            // An explicit three-state skin follows the note's lifecycle:
            // keep active art during release grace, until the note drops.
            let held = active && (roll || pressed || m.hold_use_dropped_state);
            let x = geo.lane_x(l.column);
            let head_y = geo.lane_y(l.scroll_sign, note.y);
            let tail_px = geo.lane_y(l.scroll_sign, tail_y);
            // The column's reverse; StepMania's "head" is the end higher on
            // screen.
            let reverse = geo.lane_direction(l.scroll_sign) < 0.0;
            let dropped_state = dropped && m.hold_use_dropped_state;
            let state = if dropped_state { 2 } else { usize::from(held) };
            let color_scale = if dropped_state {
                1.0
            } else {
                lerp(
                    m.hold_let_go_gray_percent,
                    1.0,
                    note.hold_life.clamp(0.0, 1.0),
                )
            };
            // A `Def.LongNote` draws the whole long note.
            let parts = if roll { &lane.roll } else { &lane.hold };
            if let Element::LongNote(ln) = &parts.body[state] {
                let clocks = self.note_clocks(i, note.beat, frame);
                let seed = note_seed(note.note_index);
                let fill = match ln
                    .fill
                    .as_ref()
                    .and_then(|f| Some((f, f.material.as_deref()?)))
                {
                    Some((f, mat)) => match (mat.compiled, self.material_slot(mat.params)) {
                        (Some(index), Some(slot)) => {
                            let image_id = |n: usize| {
                                mat.images
                                    .get(n)
                                    .and_then(|&t| self.texture_id(t))
                                    .unwrap_or(UNTEXTURED)
                            };
                            hold_material::Fill::Skin {
                                quad: crate::material::MaterialQuad {
                                    material: crate::material::LOCAL | index,
                                    slot,
                                    axes: [0.0; 4],
                                    origin: [0.0; 2],
                                    phase: f
                                        .value(ddi_noteskin::component::Property::Phase, &clocks),
                                    seed,
                                    value: [0.0; 4],
                                    images: [image_id(0), image_id(1)],
                                },
                                anchor: ln.anchor,
                                origin: mat.origin,
                            }
                        }
                        _ => hold_material::Fill::Nothing,
                    },
                    None => hold_material::Fill::Nothing,
                };
                let layers = hold_material::HoldLayers::of_long_note(ln, &clocks, fill);
                // The head's parts: its band into the mask, its shadow
                // outside the envelope, the rest over everything.
                let mut head_draws = Vec::new();
                if let Some(h) = &ln.head {
                    self.draw_component(
                        h,
                        [x, head_y],
                        k,
                        (note.alpha, color_scale),
                        seed,
                        &clocks,
                        &|p| match p.role.as_deref() {
                            None => Some(Mask::None),
                            Some("band") => Some(Mask::Write),
                            Some("shadow") => Some(Mask::Outside),
                            Some(_) => None,
                        },
                        &mut head_draws,
                    );
                }
                self.material_hold(
                    &layers,
                    note,
                    frame,
                    geo,
                    [x, head_y, tail_px, k, color_scale],
                    &head_draws,
                    out,
                );
                lane.swap_hold_colors(note_type, roll);
                continue;
            }
            let parts = if roll { &mut lane.roll } else { &mut lane.hold };
            if parts.drawn() {
                let layers: &[(usize, QuadBlend)] = if dropped_state {
                    &[(2, QuadBlend::Normal)]
                } else if held && m.hold_active_is_add_layer {
                    &[(0, QuadBlend::Normal), (1, QuadBlend::Add)]
                } else if held {
                    &[(1, QuadBlend::Normal)]
                } else {
                    &[(0, QuadBlend::Normal)]
                };
                // At most two spans.
                let mut spans = [None; 2];
                for (span, s) in spans
                    .iter_mut()
                    .zip(frame.appearance.visible_spans(note.y, tail_y))
                {
                    *span = Some(s);
                }
                for &(a, blend) in layers {
                    let spec = HoldDraw {
                        x,
                        head_y,
                        tail_px,
                        reverse,
                        k,
                        color_scale,
                        blend,
                        spans,
                        lane_sign: l.scroll_sign,
                    };
                    self.hold_body(parts, a, m, note, frame, geo, &spec, out);
                }
            }
            let a = state;
            // The tail, then the head on top (at the hold's ends, swapped
            // under reverse unless `FlipHeadAndTailWhenReverse`).
            let flip_ends = reverse && m.flip_head_and_tail_when_reverse;
            let (start_y, end_y) = if reverse {
                (tail_px, head_y)
            } else {
                (head_y, tail_px)
            };
            let (head_at, tail_at) = if flip_ends {
                (end_y, start_y)
            } else {
                (start_y, end_y)
            };
            let tail_vis = frame.appearance.visibility(tail_y);
            if m.hold_tail_is_above_wavy_parts {
                let mut tail = std::mem::replace(&mut parts.tail[a], Element::Blank);
                self.note_actor(
                    &mut tail,
                    m,
                    Part::HoldTail,
                    note.beat,
                    tail_vis,
                    frame,
                    [x, tail_at],
                    k,
                    color_scale,
                    out,
                );
                let parts = if roll { &mut lane.roll } else { &mut lane.hold };
                parts.tail[a] = tail;
            }
            let parts = if roll { &mut lane.roll } else { &mut lane.hold };
            if m.hold_head_is_above_wavy_parts && parts.heads() {
                let mut head = std::mem::replace(&mut parts.head[a], Element::Blank);
                self.note_actor(
                    &mut head,
                    m,
                    Part::HoldHead,
                    note.beat,
                    (note.alpha, note.glow),
                    frame,
                    [x, head_at],
                    k,
                    color_scale,
                    out,
                );
                let parts = if roll { &mut lane.roll } else { &mut lane.hold };
                parts.head[a] = head;
            }
            lane.swap_hold_colors(note_type, roll);
        }
        self.lanes = lanes;
    }

    /// Taps, lifts, fakes and mines in row order, the later on top
    /// (`NoteDisplay.cpp:628-637`, `RenderEarlierNotesOnTop` off).
    fn notes(
        &mut self,
        frame: &Frame,
        layout: &Layout,
        geo: &FieldGeometry,
        out: &mut Vec<TexQuad>,
    ) {
        self.camera = Camera::of(geo);
        let k = geo.arrow / 64.0;
        let mut lanes = std::mem::take(&mut self.lanes);
        let mut electricity = Vec::new();
        let joined = self.mine_links(frame, layout, geo, &mut lanes, out, &mut electricity);
        let mut order: Vec<&NoteSprite> = frame
            .notes
            .iter()
            .filter(|n| n.alpha > 0.0 || n.glow > 0.0)
            .collect();
        order.sort_by(|a, b| a.beat.total_cmp(&b.beat));
        for note in order {
            let i = usize::from(note.lane);
            let (Some(l), Some(lane)) = (layout.lanes.get(i), lanes.get_mut(i)) else {
                continue;
            };
            let (slot, part) = match note.kind {
                SpriteKind::Tap | SpriteKind::Dummy => (Slot::Tap, Part::TapNote),
                SpriteKind::Lift => (Slot::Lift, Part::TapLift),
                SpriteKind::Fake => (Slot::Fake, Part::TapFake),
                SpriteKind::Mine | SpriteKind::Shock => (Slot::Mine, Part::TapMine),
                _ => continue,
            };
            let note_type = note_type(note.beat);
            let at = [geo.lane_x(l.column), geo.lane_y(l.scroll_sign, note.y)];
            let mut e = std::mem::replace(lane.note_slot(slot, note_type), Element::Blank);
            if let Element::Lift(lift) = &e {
                let dir = geo.lane_direction(l.scroll_sign);
                self.component_lift(lift, i, note, frame, at, k, dir, out);
                *lane.note_slot(slot, note_type) = e;
                self.last_note_positions.insert(note.note_index, note.y);
                continue;
            }
            if let Element::Component(c) = &e {
                let clocks = self.note_clocks(i, note.beat, frame);
                self.draw_component(
                    c,
                    at,
                    k,
                    (note.alpha, 1.0),
                    note_seed(note.note_index),
                    &clocks,
                    &|_| Some(Mask::None),
                    out,
                );
                *lane.note_slot(slot, note_type) = e;
                self.last_note_positions.insert(note.note_index, note.y);
                continue;
            }
            let split =
                matches!(slot, Slot::Mine) && lane.metrics.mine_connect_rows && split_mine_rims(&e);
            let visibility = if matches!(slot, Slot::Mine) && lane.metrics.mine_connect_rows {
                mine_layers(
                    &mut e,
                    if split {
                        MineLayers::Base
                    } else {
                        MineLayers::Complete
                    },
                    joined.get(&note.note_index).copied().unwrap_or([false; 2]),
                )
            } else {
                None
            };
            self.note_actor(
                &mut e,
                &lane.metrics,
                part,
                note.beat,
                (note.alpha, note.glow),
                frame,
                at,
                k,
                1.0,
                out,
            );
            restore_mine_layers(&mut e, visibility);
            if split {
                let visibility = mine_layers(
                    &mut e,
                    MineLayers::Electricity,
                    joined.get(&note.note_index).copied().unwrap_or([false; 2]),
                );
                self.note_actor(
                    &mut e,
                    &lane.metrics,
                    part,
                    note.beat,
                    (note.alpha, note.glow),
                    frame,
                    at,
                    k,
                    1.0,
                    &mut electricity,
                );
                restore_mine_layers(&mut e, visibility);
            }
            *lane.note_slot(slot, note_type) = e;
            self.last_note_positions.insert(note.note_index, note.y);
        }
        out.extend(electricity);
        self.lanes = lanes;
    }
}

/// One visible mine's actual field position, after the layout and reverse.
#[derive(Clone, Copy)]
struct MineEndpoint<'a> {
    note: &'a NoteSprite,
    at: [f32; 2],
}

/// Adjacent nodes joined in each row, independently of lane indices: a
/// row's nodes are of one `key` and within an eighth of a lane (`width`)
/// of each other vertically, joined left to right where at most a lane
/// apart. A missing node leaves a break rather than a bridge across the
/// empty lane. Mine links and judgment links both join this way.
fn adjacent_pairs<T: Copy>(
    mut nodes: Vec<T>,
    at: impl Fn(&T) -> [f32; 2],
    key: impl Fn(&T, &T) -> std::cmp::Ordering,
    width: f32,
) -> Vec<[T; 2]> {
    let tolerance = width * 0.125;
    nodes.sort_by(|a, b| {
        let ([ax, ay], [bx, by]) = (at(a), at(b));
        key(a, b)
            .then_with(|| ay.total_cmp(&by))
            .then_with(|| ax.total_cmp(&bx))
    });
    let mut pairs = Vec::new();
    let mut start = 0;
    while start < nodes.len() {
        let first = nodes[start];
        let mut end = start + 1;
        while end < nodes.len()
            && key(&nodes[end], &first).is_eq()
            && (at(&nodes[end])[1] - at(&first)[1]).abs() <= tolerance
        {
            end += 1;
        }
        nodes[start..end].sort_by(|a, b| at(a)[0].total_cmp(&at(b)[0]));
        pairs.extend(nodes[start..end].windows(2).filter_map(|pair| {
            let distance = at(&pair[1])[0] - at(&pair[0])[0];
            (distance > 0.001 && distance <= width + 0.001).then_some([pair[0], pair[1]])
        }));
        start = end;
    }
    pairs
}

/// Visible mines joined in each chart row (`width` a lane).
fn aligned_mines(nodes: Vec<MineEndpoint<'_>>, width: f32) -> Vec<[MineEndpoint<'_>; 2]> {
    adjacent_pairs(
        nodes,
        |n| n.at,
        |a, b| a.note.beat.total_cmp(&b.note.beat),
        width,
    )
}

/// Where a bridge authored over 64 units (x from -32 to 32) is drawn
/// from `left` to `right`: only its length stretches, its thickness stays
/// at the note's drawing scale (`arrow` pixels a lane).
fn bridge(left: [f32; 2], right: [f32; 2], arrow: f32) -> [f32; 6] {
    let [dx, dy] = [right[0] - left[0], right[1] - left[1]];
    let length = dx.hypot(dy);
    let k = arrow / 64.0;
    [
        dx / 64.0,
        dy / 64.0,
        -dy / length * k,
        dx / length * k,
        (left[0] + right[0]) * 0.5,
        (left[1] + right[1]) * 0.5,
    ]
}

fn has_mine_link(element: &Element) -> bool {
    matches!(element, Element::Actor(a) if a.children().iter().any(|c| c.name == "MineLink"))
}

fn split_mine_rims(element: &Element) -> bool {
    matches!(element, Element::Actor(a) if ["MineRimLeft","MineRimRight"].iter().all(|name| a.children().iter().any(|c| c.name == *name)))
}
#[derive(Clone, Copy)]
enum MineLayers {
    Complete,
    Base,
    Electricity,
    Link,
}

/// Keep original visibility intact across the base, local and bridge passes.
fn mine_layers(element: &mut Element, mode: MineLayers, joined: [bool; 2]) -> Option<Vec<bool>> {
    let Element::Actor(a) = element else {
        return None;
    };
    let index = a.children().iter().position(|c| c.name == "MineLink")?;
    let visibility = a.children().iter().map(|c| c.visible).collect();
    for (i, c) in a.children_mut().iter_mut().enumerate() {
        let rim = c.name == "MineRimLeft" || c.name == "MineRimRight";
        let keep = match mode {
            MineLayers::Link => i == index,
            MineLayers::Base => i != index && !rim,
            MineLayers::Electricity => rim,
            MineLayers::Complete => i != index,
        };
        if !keep
            || (c.name == "MineRimLeft" && joined[0])
            || (c.name == "MineRimRight" && joined[1])
        {
            c.visible = false;
        }
    }
    Some(visibility)
}

fn restore_mine_layers(element: &mut Element, visibility: Option<Vec<bool>>) {
    if let (Element::Actor(a), Some(visibility)) = (element, visibility) {
        for (child, visible) in a.children_mut().iter_mut().zip(visibility) {
            child.visible = visible;
        }
    }
}

impl NoteskinPlay {
    fn mine_links(
        &self,
        frame: &Frame,
        layout: &Layout,
        geo: &FieldGeometry,
        lanes: &mut [Lane],
        out: &mut Vec<TexQuad>,
        electricity: &mut Vec<TexQuad>,
    ) -> std::collections::HashMap<usize, [bool; 2]> {
        let mut joined = std::collections::HashMap::<usize, [bool; 2]>::new();
        let nodes = frame
            .notes
            .iter()
            .filter_map(|note| {
                if !matches!(note.kind, SpriteKind::Mine | SpriteKind::Shock)
                    || (note.alpha <= 0.0 && note.glow <= 0.0)
                {
                    return None;
                }
                let lane = lanes.get_mut(usize::from(note.lane))?;
                let l = layout.lanes.get(usize::from(note.lane))?;
                if !lane.metrics.mine_connect_rows
                    || !has_mine_link(lane.note_slot(Slot::Mine, note_type(note.beat)))
                {
                    return None;
                }
                let at = [geo.lane_x(l.column), geo.lane_y(l.scroll_sign, note.y)];
                at.iter()
                    .all(|v| v.is_finite())
                    .then_some(MineEndpoint { note, at })
            })
            .collect();
        for [left, right] in aligned_mines(nodes, geo.arrow) {
            joined.entry(left.note.note_index).or_default()[1] = true;
            joined.entry(right.note.note_index).or_default()[0] = true;
            let lane = &mut lanes[usize::from(left.note.lane)];
            let e = lane.note_slot(Slot::Mine, note_type(left.note.beat));
            let split = split_mine_rims(e);
            let visibility = mine_layers(e, MineLayers::Link, [false; 2]);
            if let Element::Actor(a) = e {
                a.reset_placement();
                a.set_note_colors(
                    [1.0, 1.0, 1.0, left.note.alpha.min(right.note.alpha)],
                    [1.0, 1.0, 1.0, left.note.glow.min(right.note.glow)],
                );
                self.emit(
                    a,
                    bridge(left.at, right.at, geo.arrow),
                    [0.0; 2],
                    if split { electricity } else { out },
                );
            }
            restore_mine_layers(e, visibility);
        }
        joined
    }
}

/// Roll cue frames may follow the judging window instead of the musical beat.
fn long_note_animation_percent(
    metrics: &NoteDisplay,
    part: &PartMetrics,
    note: &NoteSprite,
    frame: &Frame,
) -> f32 {
    if matches!(note.kind, SpriteKind::RollHead { .. }) {
        let period =
            frame.roll_window_seconds * f64::from(metrics.roll_animation_timing_window_ratio);
        if period.is_finite() && period > 0.0 {
            return (frame.song_time / period).rem_euclid(1.0) as f32;
        }
    }
    animation_percent(
        part,
        metrics.animation_is_beat_based,
        frame.beat as f32,
        frame.song_time as f32,
        note.beat,
    )
}

/// Where and how a hold's body is drawn.
struct HoldDraw {
    x: f32,
    head_y: f32,
    tail_px: f32,
    reverse: bool,
    k: f32,
    color_scale: f32,
    blend: QuadBlend,
    /// The parts Hidden/Sudden leave visible, in arrow heights, in order
    /// ([`ddi_engine::appearance::Appearance::visible_spans`]).
    spans: [Option<(f32, f32)>; 2],
    lane_sign: i8,
}

impl NoteskinPlay {
    /// One actor per column at its receptor (receptors, explosions).
    fn column_actors(
        &mut self,
        layout: &Layout,
        geo: &FieldGeometry,
        out: &mut Vec<TexQuad>,
        pick: impl Fn(&mut Lane) -> &mut Element,
    ) {
        let k = geo.arrow / 64.0;
        let mut lanes = std::mem::take(&mut self.lanes);
        for (i, lane) in lanes.iter_mut().enumerate() {
            let Some(l) = layout.lanes.get(i) else {
                continue;
            };
            let at = [geo.lane_x(l.column), geo.lane_receptor_y(l.scroll_sign)];
            let explosion = std::ptr::eq(pick(lane), &lane.explosion);
            let step = lane.step.filter(|h| h.at <= self.frame_time.0);
            match pick(lane) {
                Element::Actor(a) => self.emit(a, [k, 0.0, 0.0, k, at[0], at[1]], [0.0; 2], out),
                // An explosion's component draws an instance a judgment
                // ([`NoteskinPlay::explosion_instances`]).
                Element::Component(_) if explosion => {}
                Element::Component(c) => {
                    // A receptor: its `hit` clock and `When` parts from
                    // what the lane's last press did.
                    let clocks = ddi_noteskin::component::Clocks {
                        hit: step.map(|h| (self.frame_time.0 - h.at) as f32),
                        ..self.clocks(i)
                    };
                    let shows = move |p: &ddi_noteskin::component::Part| {
                        let shown = p.when.is_empty()
                            || step.is_some_and(|h| p.shows_for(h.name, h.bright));
                        shown.then_some(Mask::None)
                    };
                    self.draw_component(
                        c,
                        at,
                        k,
                        (1.0, 1.0),
                        i as f32 * 0.618_034,
                        &clocks,
                        &shows,
                        out,
                    );
                }
                _ => {}
            }
        }
        self.lanes = lanes;
    }

    /// Each lane's explosion component, once for each of its judgments
    /// still showing (its parts as `When` and `Bright` choose, its `hit`
    /// clock the time since), and while a long note is held (`Holding`,
    /// `Rolling`).
    fn explosion_instances(
        &mut self,
        layout: &Layout,
        geo: &FieldGeometry,
        out: &mut Vec<TexQuad>,
    ) {
        let k = geo.arrow / 64.0;
        let now = self.frame_time.0;
        let lanes = std::mem::take(&mut self.lanes);
        let links = self.judgment_links(&lanes, layout, geo);
        for (i, lane) in lanes.iter().enumerate() {
            let (Some(l), Element::Component(c)) = (layout.lanes.get(i), &lane.explosion) else {
                continue;
            };
            let at = [geo.lane_x(l.column), geo.lane_receptor_y(l.scroll_sign)];
            let held = held_name(lane.holding)
                .zip(lane.holding_since)
                .map(|(name, t)| (name, t, false, None));
            let tail = lane
                .ended
                .filter(|(_, _, end)| now - end <= f64::from(c.duration))
                .map(|(name, t, end)| (name, t, false, Some(end)));
            let instances = lane
                .hits
                .iter()
                .map(|h| (h.name, h.at, h.bright, None))
                .chain(tail)
                .chain(held);
            for (name, at_time, bright, ended) in instances {
                let age = now - at_time;
                let holding = matches!(name, "Holding" | "Rolling");
                if age < 0.0 || (!holding && age > f64::from(c.duration)) {
                    continue;
                }
                let clocks = ddi_noteskin::component::Clocks {
                    hit: Some(age as f32),
                    ended: ended.map(|end: f64| (end - at_time) as f32),
                    ..self.clocks(i)
                };
                self.draw_component(
                    c,
                    at,
                    k,
                    (1.0, 1.0),
                    hit_seed(i, at_time),
                    &clocks,
                    &|p| {
                        (p.shows_for(name, bright)
                            && links.shows(i, name, at_time, p.role.as_deref()))
                        .then_some(Mask::None)
                    },
                    out,
                );
            }
        }
        self.draw_judgment_links(&lanes, &links, geo, out);
        self.lanes = lanes;
    }

    /// What lane `lane`'s components read now, with no note drawn
    /// (`ddi_noteskin::component::Clocks`).
    fn clocks(&self, lane: usize) -> ddi_noteskin::component::Clocks {
        let p = self.presses.get(lane).copied().unwrap_or_default();
        let since = |t: Option<f64>| t.map(|t| (self.frame_time.0 - t) as f32);
        ddi_noteskin::component::Clocks {
            beat: self.frame_time.1 as f32,
            seconds: self.frame_time.0 as f32,
            note: None,
            roll_window: self.frame_time.2 as f32,
            press: since(p.pressed),
            release: since(p.released),
            hit: None,
            seed: 0.0,
            ended: None,
        }
    }

    /// What lane `lane`'s components read for the note at `note` (its
    /// beat). Note animation follows the frame's beat even when an editor
    /// holds receptors and the skin's other clocks at rest.
    fn note_clocks(
        &self,
        lane: usize,
        note: f32,
        frame: &Frame,
    ) -> ddi_noteskin::component::Clocks {
        ddi_noteskin::component::Clocks {
            beat: frame.beat as f32,
            note: Some(note),
            ..self.clocks(lane)
        }
    }

    /// A component's parts as drawn at `clocks`, its origin at `at` (screen
    /// pixels), `k` screen pixels a unit, faded to `alpha`.
    #[allow(clippy::too_many_arguments)]
    fn draw_component(
        &mut self,
        c: &ddi_noteskin::component::Component,
        at: [f32; 2],
        k: f32,
        (alpha, tint): (f32, f32),
        seed: f32,
        clocks: &ddi_noteskin::component::Clocks,
        mask_of: &dyn Fn(&ddi_noteskin::component::Part) -> Option<Mask>,
        out: &mut Vec<TexQuad>,
    ) {
        // The instance's seed for its random bindings.
        let clocks = &ddi_noteskin::component::Clocks { seed, ..*clocks };
        // Its own animation, turned and zoomed about its pivot, inside its
        // turn for the lane.
        let place = c.placement(clocks);
        let [px, py] = c.pivot;
        let own = affine_mul(
            [1.0, 0.0, 0.0, 1.0, place.x + px, place.y + py],
            affine_mul(
                rotation(place.rotation),
                [
                    place.zoom,
                    0.0,
                    0.0,
                    place.zoom,
                    -px * place.zoom,
                    -py * place.zoom,
                ],
            ),
        );
        let turn = affine_mul(
            affine_mul(rotation(c.rotation), [c.zoom, 0.0, 0.0, c.zoom, 0.0, 0.0]),
            own,
        );
        let outer = affine_mul([k, 0.0, 0.0, k, at[0], at[1]], turn);
        self.draw_parts(c, turn, outer, (alpha, tint), clocks, mask_of, out);
    }

    /// A component's parts `mask_of` keeps as drawn at `clocks` (seeded),
    /// placed on the screen by `outer`; `lane` is the lane's turn (in its
    /// units), a material's space ([`NoteskinPlay::draw_component`]; a
    /// judgment link's `outer` is its bridge alone).
    #[allow(clippy::too_many_arguments)]
    fn draw_parts(
        &mut self,
        c: &ddi_noteskin::component::Component,
        lane: [f32; 6],
        outer: [f32; 6],
        (alpha, tint): (f32, f32),
        clocks: &ddi_noteskin::component::Clocks,
        mask_of: &dyn Fn(&ddi_noteskin::component::Part) -> Option<Mask>,
        out: &mut Vec<TexQuad>,
    ) {
        let [vx, vy, vw, vh] = c.view_box;
        // Unit square -> the part's image over the view box.
        let image = [vw, 0.0, 0.0, vh, vx + vw / 2.0, vy + vh / 2.0];
        let seed = clocks.seed;
        for pose in c.pose(clocks) {
            let Some(mask) = mask_of(pose.part) else {
                continue;
            };
            let Some(texture) = self
                .texture_ids
                .get(pose.texture as usize)
                .copied()
                .flatten()
            else {
                continue;
            };
            let part = affine_mul(
                [1.0, 0.0, 0.0, 1.0, pose.x, pose.y],
                affine_mul(
                    rotation(pose.rotation),
                    [pose.zoom, 0.0, 0.0, pose.zoom, 0.0, 0.0],
                ),
            );
            // A material's coordinates: the component's (or the lane's),
            // so moving a part moves its mask through the paint, not the
            // paint.
            let material = pose.part.material.as_deref().and_then(|m| {
                let index = m.compiled?;
                let slot = self.material_slot(m.params)?;
                let space = match m.space {
                    ddi_noteskin::component::Space::Note => affine_mul(part, image),
                    ddi_noteskin::component::Space::Lane => {
                        affine_mul(lane, affine_mul(part, image))
                    }
                };
                let image_id = |i: usize| {
                    m.images
                        .get(i)
                        .and_then(|&t| self.texture_id(t))
                        .unwrap_or(UNTEXTURED)
                };
                Some(crate::material::MaterialQuad {
                    material: crate::material::LOCAL | index,
                    slot,
                    axes: [space[0], space[1], space[2], space[3]],
                    origin: [space[4] - m.origin[0], space[5] - m.origin[1]],
                    phase: pose.phase,
                    seed,
                    value: [0.0; 4],
                    images: [image_id(0), image_id(1)],
                })
            });
            out.push(TexQuad {
                material,
                mask,
                blend: match (mask, pose.blend) {
                    // Into the mask only.
                    (Mask::Write | Mask::Erase, _) => QuadBlend::NoEffect,
                    (_, ddi_noteskin::component::PartBlend::Normal) => QuadBlend::Normal,
                    (_, ddi_noteskin::component::PartBlend::Add) => QuadBlend::Add,
                },
                ..TexQuad::new(
                    Shape::Quad {
                        transform: affine_mul(outer, affine_mul(part, image)),
                        uv: [0.0, 0.0, 1.0, 1.0],
                    },
                    to_target(
                        [
                            pose.tint[0] * tint,
                            pose.tint[1] * tint,
                            pose.tint[2] * tint,
                            pose.tint[3] * pose.opacity * alpha,
                        ],
                        self.srgb,
                    ),
                    texture,
                )
            });
        }
    }

    /// `DrawHoldBody` for one layer (inactive or active parts).
    #[allow(clippy::too_many_arguments)]
    fn hold_body(
        &self,
        parts: &mut HoldParts,
        a: usize,
        m: &NoteDisplay,
        note: &NoteSprite,
        frame: &Frame,
        geo: &FieldGeometry,
        d: &HoldDraw,
        out: &mut Vec<TexQuad>,
    ) {
        let k = d.k;
        let (mut top, mut bottom) = if d.reverse {
            (d.tail_px, d.head_y)
        } else {
            (d.head_y, d.tail_px)
        };
        let flip = d.reverse && m.flip_hold_body_when_reverse;
        let start = m.start_drawing_hold_body_offset_from_head as f32 * k;
        let stop = m.stop_drawing_hold_body_offset_from_tail as f32 * k;
        if flip {
            top -= stop;
            bottom -= start;
        } else {
            top += start;
            bottom += stop;
        }
        // Each part's frame follows the head's beat (`GetHoldSprite`).
        for (e, part) in [
            (&mut parts.topcap[a], Part::HoldTopCap),
            (&mut parts.body[a], Part::HoldBody),
            (&mut parts.bottomcap[a], Part::HoldBottomCap),
        ] {
            if let Element::Actor(actor) = e
                && let Kind::Sprite(s) = &mut actor.kind
            {
                let pm = m.part(part);
                let percent = long_note_animation_percent(m, pm, note, frame);
                let length: f32 = s.states.iter().map(|st| st.delay).sum();
                s.set_seconds_into_animation(percent * length);
            }
        }
        let (mut cap_top, body, mut cap_bottom) = (
            sprite_frame(&parts.topcap[a]),
            sprite_frame(&parts.body[a]),
            sprite_frame(&parts.bottomcap[a]),
        );
        if flip {
            std::mem::swap(&mut cap_top, &mut cap_bottom);
        }
        let flip_rect = |r: [f32; 4]| if flip { [r[0], r[3], r[2], r[1]] } else { r };
        // The visible stretches of the hold on screen (Hidden/Sudden),
        // `ArrowGetAlphaOrGlow` per vertex in StepMania.
        let visible = d.spans.map(|span| {
            span.map(|(lo, hi)| {
                let (a0, a1) = (geo.lane_y(d.lane_sign, lo), geo.lane_y(d.lane_sign, hi));
                (a0.min(a1), a0.max(a1))
            })
        });
        // A cap shows when the end of the body it is attached to does.
        let shown_at = |y: f32| {
            visible
                .iter()
                .flatten()
                .any(|&(v0, v1)| v0 <= y + 0.5 && v1 >= y - 0.5)
        };
        let c = d.color_scale;
        let color = to_target([c, c, c, 1.0], self.srgb);
        let quad = |texture: u32, y0: f32, y1: f32, width: f32, uv: [f32; 4], wrap: bool| TexQuad {
            blend: d.blend,
            wrap,
            ..TexQuad::new(
                Shape::Quad {
                    transform: [width, 0.0, 0.0, y1 - y0, d.x, (y0 + y1) / 2.0],
                    uv,
                },
                color,
                texture,
            )
        };
        let id = |t: u32| self.texture_id(t);
        // The top cap: its whole frame above the head end.
        if let Some((t, r, size)) = cap_top
            && let Some(id) = id(t)
        {
            let h = size[1] * k;
            if shown_at(top) {
                out.push(quad(id, top - h, top, size[0] * k, flip_rect(r), false));
            }
        }
        // The body: tiled every frame height, the texture coordinate whole
        // at the bottom end (at the top with `TopHoldAnchorWhenReverse`),
        // cut to the visible stretches.
        if let Some((t, r, size)) = body
            && let Some(id) = id(t)
            && bottom > top
        {
            let r = flip_rect(r);
            let h = (size[1] * k).max(1e-3);
            let span = (r[3] - r[1]) * (bottom - top) / h;
            let anchor_top = d.reverse && m.top_hold_anchor_when_reverse;
            let (v0, v1) = if anchor_top {
                (r[1], r[1] + span)
            } else {
                let end = r[1] + span;
                let add = (end - 0.0001).ceil() - end;
                let shift = (r[1] + add).floor();
                (r[1] + add - shift, end + add - shift)
            };
            let v_at = |y: f32| v0 + (v1 - v0) * (y - top) / (bottom - top);
            for &(s0, s1) in visible.iter().flatten() {
                let (y0, y1) = (s0.max(top), s1.min(bottom));
                if y1 > y0 {
                    // Overshoot by a pixel at the bottom, as StepMania does.
                    let y1d = if y1 >= bottom { y1 + 1.0 } else { y1 };
                    out.push(quad(
                        id,
                        y0,
                        y1d,
                        size[0] * k,
                        [r[0], v_at(y0), r[2], v_at(y1)],
                        true,
                    ));
                }
            }
        }
        // The bottom cap below the tail end, cropped from its top when the
        // hold is shorter than it.
        if let Some((t, r, size)) = cap_bottom
            && let Some(id) = id(t)
        {
            let h = size[1] * k;
            let y0 = (bottom + 1.0).max(top);
            let y1 = bottom + 1.0 + h;
            let r = flip_rect(r);
            let shown_part = ((y1 - y0) / h).clamp(0.0, 1.0);
            let v0 = r[3] - (r[3] - r[1]) * shown_part;
            if y1 > y0 && shown_at(bottom) {
                out.push(quad(id, y0, y1, size[0] * k, [r[0], v0, r[2], r[3]], false));
            }
        }
    }
}

fn lerp(a: f32, b: f32, t: f32) -> f32 {
    a + (b - a) * t
}

/// One shared phase for an opt-in layered note; elapsed sprite durations
/// may differ, so each layer maps that phase through its own state sequence.
fn set_note_animation(actor: &mut Actor, percent: f32, children: bool) {
    match &mut actor.kind {
        Kind::Sprite(s) => {
            let length: f32 = s.states.iter().map(|state| state.delay).sum();
            s.set_seconds_into_animation(percent * length);
        }
        Kind::Model(m) => m.seconds = percent * m.animation_length(),
        _ => {}
    }
    if children {
        for child in actor.children_mut() {
            set_note_animation(child, percent, true);
        }
    }
}

/// What the color-scale and pixel maths of a hold look like, for tests.
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scores_have_stepmanias_names() {
        assert_eq!(score_name(Judgement::W1), "W1");
        assert_eq!(score_name(Judgement::Miss), "Miss");
        assert_eq!(lerp(0.25, 1.0, 0.0), 0.25);
    }

    #[test]
    fn layered_notes_follow_beats_and_vivid_displacement_only_when_opted_in() {
        use ddi_actor::{Sprite, State};
        let play = lua_play("return Def.ActorFrame {}");
        let mut metrics = play.lanes[0].metrics.clone();
        metrics.animation_is_beat_based = true;
        let sprite = |count| {
            Actor::new(Kind::Sprite(Sprite::new(
                0,
                (0..count)
                    .map(|i| State {
                        rect: [
                            i as f32 / count as f32,
                            0.0,
                            (i + 1) as f32 / count as f32,
                            1.0,
                        ],
                        delay: 1.0 / count as f32,
                    })
                    .collect(),
                [64.0; 2],
            )))
        };
        // Two frames also reproduce an alignment wrapper around a skin's
        // shadow, color sheets and charge sheet (which have different lengths).
        let mut element = Element::Actor(Box::new(Actor::new(Kind::Frame(vec![Actor::new(
            Kind::Frame(vec![sprite(128), sprite(16), sprite(1)]),
        )]))));
        fn states(actor: &Actor) -> Vec<usize> {
            let mut result = match &actor.kind {
                Kind::Sprite(s) => vec![s.state()],
                _ => Vec::new(),
            };
            for child in actor.children() {
                result.extend(states(child));
            }
            result
        }
        for part in [Part::TapNote, Part::TapLift, Part::TapFake, Part::HoldHead] {
            let p = &mut metrics.parts[part as usize];
            p.animation_length = 4.0;
            p.animation_is_vivid = true;
            p.animation_on_children = true;
            p.color_spacing = [0.0; 2];
            for (song_beat, note_beat, expected) in [
                (1.0, 0.0, vec![32, 4, 0]),
                (1.0, 0.25, vec![64, 8, 0]),
                (1.0, 0.5, vec![96, 12, 0]),
                (1.0, 0.75, vec![0, 0, 0]),
                (5.0, 0.0, vec![32, 4, 0]),
                (-0.25, 0.0, vec![120, 15, 0]),
            ] {
                for song_time in [0.0, 0.125, 30.0] {
                    let mut f = frame(song_time, None);
                    f.beat = song_beat;
                    play.note_actor(
                        &mut element,
                        &metrics,
                        part,
                        note_beat,
                        (1.0, 0.0),
                        &f,
                        [0.0; 2],
                        1.0,
                        1.0,
                        &mut Vec::new(),
                    );
                    assert_eq!(
                        states(actor(&element)),
                        expected,
                        "{part:?}, beat {song_beat}, note {note_beat}"
                    );
                }
            }
        }
        let before = states(actor(&element));
        // Mine children keep their independent electricity clock. Existing
        // ActorFrame skins likewise retain their original behavior by default.
        for part in [Part::TapMine, Part::HoldBody] {
            assert!(!metrics.part(part).animation_on_children);
            play.note_actor(
                &mut element,
                &metrics,
                part,
                0.0,
                (1.0, 0.0),
                &frame(99.0, None),
                [0.0; 2],
                1.0,
                1.0,
                &mut Vec::new(),
            );
            assert_eq!(states(actor(&element)), before);
        }
        let mut root = Element::Actor(Box::new(sprite(128)));
        metrics.parts[Part::TapNote as usize].animation_on_children = false;
        play.note_actor(
            &mut root,
            &metrics,
            Part::TapNote,
            0.0,
            (1.0, 0.0),
            &frame(0.5, None),
            [0.0; 2],
            1.0,
            1.0,
            &mut Vec::new(),
        );
        assert_eq!(
            states(actor(&root)),
            vec![32],
            "ordinary root sprites still follow metrics"
        );
    }

    use ddi_engine::appearance::Appearance;
    use ddi_engine::frame::ReceptorState;

    struct NoImages;

    impl Images for NoImages {
        fn size(&self, _: ddi_noteskin::Dir, _: &str) -> Option<(u32, u32)> {
            Some((64, 64))
        }

        fn max_size(&self) -> u32 {
            4096
        }
    }

    fn frame(song_time: f64, holding: Option<HoldKind>) -> Frame {
        Frame {
            song_time,
            beat: song_time * 2.0,
            note_motion: Default::default(),
            roll_window_seconds: 0.35,
            lanes: 4,
            receptors: vec![
                ReceptorState {
                    pressed: false,
                    flash: None,
                    holding,
                };
                4
            ],
            notes: Vec::new(),
            judgement: None,
            holds: Default::default(),
            combo: 0,
            score: Default::default(),
            life: 0.5,
            danger: false,
            failed: false,
            finished: false,
            visible_range: 12.0,
            appearance: Appearance::Visible,
            cover: Default::default(),
        }
    }

    /// A skin's Lua through a play: `tap` is its `Down Tap Note.lua`.
    fn lua_play(tap: &str) -> NoteskinPlay {
        lua_play_with(tap, &[])
    }

    #[test]
    fn judgment_contours_link_only_adjacent_visible_matching_effects() {
        use ddi_noteskin::component::{Binding, Clock, Component, Part as ComponentPart, Property};
        let mut play = lua_play("return Def.Sprite {Texture='x.png'}");
        let parts = [None, Some("rim-left"), Some("rim-right"), Some("link")]
            .into_iter()
            .enumerate()
            .map(|(i, role)| {
                let mut part = ComponentPart::new(None, role.map(str::to_string), vec![i as u32]);
                part.when = vec!["HitMine".into(), "W1".into()];
                part.bindings.push(Binding {
                    property: Property::Opacity,
                    clock: Clock::Hit,
                    function: ddi_noteskin::component::Function::Curve {
                        keys: vec![(0.0, 1.0), (0.45, 0.0)],
                        ease: ddi_noteskin::component::Ease::Linear,
                        length: None,
                    },
                });
                part
            })
            .collect();
        let component = Component {
            file: "test.svg".into(),
            id: "shock".into(),
            view_box: [-48.0, -48.0, 96.0, 96.0],
            rotation: 0.0,
            zoom: 1.0,
            parts,
            duration: 0.5,
            bindings: Vec::new(),
            pivot: [0.0; 2],
        };
        for lane in &mut play.lanes {
            lane.explosion = Element::Component(Box::new(component.clone()));
        }
        play.texture_ids = vec![Some(70), Some(71), Some(72), Some(73)];
        play.frame_time = (0.2, 0.4, 0.35);
        let hit = |at, name| Hit {
            at,
            name,
            bright: false,
        };
        play.lanes[0].hits.push_back(hit(0.0, "HitMine"));
        // A slightly later neighbor still joins; the bridge uses the weaker fade.
        play.lanes[1].hits.push_back(hit(0.1, "HitMine"));
        let mut layout = Layout::dance_single();
        let mut geo = FieldGeometry {
            arrow: 64.0,
            left: 0.0,
            receptor_y: 64.0,
            direction: 1.0,
            width: 256.0,
            height: 512.0,
        };
        let draw = |play: &mut NoteskinPlay, layout: &Layout, geo: &FieldGeometry| {
            let mut out = Vec::new();
            play.explosion_instances(layout, geo, &mut out);
            out
        };
        let drawn = draw(&mut play, &layout, &geo);
        assert_eq!(drawn.iter().filter(|q| q.texture == 73).count(), 1);
        assert_eq!(drawn.iter().filter(|q| q.texture == 71).count(), 1);
        assert_eq!(drawn.iter().filter(|q| q.texture == 72).count(), 1);
        let bridge = drawn.iter().find(|q| q.texture == 73).unwrap();
        let left = drawn.iter().find(|q| q.texture == 71).unwrap();
        assert!((bridge.color[3] - left.color[3]).abs() < 1e-6);
        // Repeated hits do not emit duplicate bridges; only the linked
        // instance gives its rim up, the older one keeps both.
        play.lanes[0].hits.push_back(hit(0.15, "HitMine"));
        let repeated = draw(&mut play, &layout, &geo);
        assert_eq!(repeated.iter().filter(|q| q.texture == 73).count(), 1);
        assert_eq!(repeated.iter().filter(|q| q.texture == 71).count(), 2);
        assert_eq!(repeated.iter().filter(|q| q.texture == 72).count(), 2);
        // Same geometry in reverse; no hardcoded receptor Y or button order.
        geo.direction = -1.0;
        geo.receptor_y = 448.0;
        layout.lanes.swap(0, 1);
        let reversed = draw(&mut play, &layout, &geo);
        assert_eq!(reversed.iter().filter(|q| q.texture == 73).count(), 1);
        let Shape::Quad { transform, .. } =
            reversed.iter().find(|q| q.texture == 73).unwrap().shape
        else {
            panic!("bridge quad")
        };
        assert_eq!(transform[5], 448.0);
        layout.lanes.swap(0, 1);
        // Opposite scroll rows, an empty column, or another judgment never joins.
        layout.lanes[1].scroll_sign = -1;
        assert!(
            !draw(&mut play, &layout, &geo)
                .iter()
                .any(|q| q.texture == 73)
        );
        layout.lanes[1].scroll_sign = 1;
        play.lanes[2].hits = std::mem::take(&mut play.lanes[1].hits);
        assert!(
            !draw(&mut play, &layout, &geo)
                .iter()
                .any(|q| q.texture == 73)
        );
        play.lanes[1].hits = std::mem::take(&mut play.lanes[2].hits);
        play.lanes[1].hits.back_mut().unwrap().name = "W1";
        assert!(
            !draw(&mut play, &layout, &geo)
                .iter()
                .any(|q| q.texture == 73)
        );
        play.lanes[1].hits.back_mut().unwrap().name = "HitMine";
        // The neighbor is transparent before its component lifetime expires.
        play.frame_time.0 = 0.55;
        let expired = draw(&mut play, &layout, &geo);
        assert!(!expired.iter().any(|q| q.texture == 73));
        assert!(expired.iter().any(|q| q.texture == 71));
        assert!(expired.iter().any(|q| q.texture == 72));
        // Two scroll rows can share columns without interrupting each other's links.
        play.frame_time.0 = 0.2;
        for lane in &mut play.lanes {
            lane.hits.clear();
            lane.hits.push_back(hit(0.0, "HitMine"));
        }
        let chain = draw(&mut play, &layout, &geo);
        assert_eq!(chain.iter().filter(|q| q.texture == 73).count(), 3);
        assert_eq!(chain.iter().filter(|q| q.texture == 71).count(), 1);
        assert_eq!(chain.iter().filter(|q| q.texture == 72).count(), 1);
        layout.lanes[2].column = 0.0;
        layout.lanes[3].column = 1.0;
        layout.lanes[2].scroll_sign = -1;
        layout.lanes[3].scroll_sign = -1;
        let rows = draw(&mut play, &layout, &geo);
        assert_eq!(rows.iter().filter(|q| q.texture == 73).count(), 2);
        // A component turned for its lane still bridges straight across.
        for lane in &mut play.lanes {
            if let Element::Component(c) = &mut lane.explosion {
                c.rotation = 90.0;
            }
        }
        let turned = draw(&mut play, &layout, &geo);
        let bridges: Vec<_> = turned.iter().filter(|q| q.texture == 73).collect();
        assert_eq!(bridges.len(), 2);
        for q in bridges {
            let Shape::Quad { transform, .. } = q.shape else {
                panic!("bridge quad")
            };
            assert!(
                transform[0] > 0.0 && transform[1].abs() < 1e-3,
                "{transform:?}"
            );
        }
    }

    #[test]
    fn explosion_variation_stays_fixed_when_the_judgment_queue_rolls_over() {
        use ddi_noteskin::component::{Component, Part as ComponentPart, PartMaterial, Space};
        let mut play = lua_play("return Def.Sprite {Texture='x.png'}");
        let mut part = ComponentPart::new(Some("burst".into()), None, vec![0]);
        part.when = vec!["W1".into(), "Holding".into()];
        part.material = Some(Box::new(PartMaterial {
            file: "test.wgsl".into(),
            source: String::new(),
            space: Space::Note,
            origin: [0.0; 2],
            params: [[0.0; 4]; 8],
            images: vec![],
            compiled: Some(0),
        }));
        play.lanes[0].explosion = Element::Component(Box::new(Component {
            file: "test.svg".into(),
            id: "explosion".into(),
            view_box: [-48.0, -48.0, 96.0, 96.0],
            rotation: 0.0,
            zoom: 1.0,
            parts: vec![part],
            duration: 1.0,
            bindings: Vec::new(),
            pivot: [0.0; 2],
        }));
        play.texture_ids[0] = Some(7);
        play.frame_time = (0.1, 0.2, 0.35);
        play.lanes[0].holding = Some(HoldKind::Hold);
        play.lanes[0].holding_since = Some(0.005);
        for n in 1..=MAX_HITS {
            play.lanes[0].hits.push_back(Hit {
                at: n as f64 * 0.01,
                name: "W1",
                bright: false,
            });
        }
        let geo = FieldGeometry {
            arrow: 64.0,
            left: 0.0,
            receptor_y: 64.0,
            direction: 1.0,
            width: 256.0,
            height: 512.0,
        };
        let draw = |play: &mut NoteskinPlay| {
            let mut out = Vec::new();
            play.explosion_instances(&Layout::dance_single(), &geo, &mut out);
            out.into_iter()
                .map(|q| q.material.unwrap().seed)
                .collect::<Vec<_>>()
        };
        let before = draw(&mut play);
        assert_eq!(before.len(), MAX_HITS + 1);
        play.lanes[0].hits.pop_front();
        play.lanes[0].hits.push_back(Hit {
            at: 0.09,
            name: "W1",
            bright: false,
        });
        let after = draw(&mut play);
        assert_eq!(
            &before[1..MAX_HITS],
            &after[..MAX_HITS - 1],
            "surviving hits do not rotate"
        );
        assert_eq!(
            before[MAX_HITS], after[MAX_HITS],
            "the holding effect keeps its seed too"
        );
        assert!(
            !before.contains(&after[MAX_HITS - 1]),
            "the new hit gets its own variation"
        );
    }

    #[test]
    fn a_held_effect_finishes_its_cycle_after_the_hold_ends() {
        use ddi_noteskin::component::{
            Binding, Clock, Component, Ease, Function, Part as ComponentPart, PartMaterial,
            Property, Space,
        };
        let mut play = lua_play("return Def.Sprite {Texture='x.png'}");
        // Shown for the first half of each 0.2 s cycle.
        let mut part = ComponentPart::new(Some("echo".into()), None, vec![0]);
        part.when = vec!["Holding".into()];
        part.bindings.push(Binding {
            property: Property::Opacity,
            clock: Clock::Hit,
            function: Function::Curve {
                keys: vec![(0.0, 1.0), (0.1, 1.0), (0.1001, 0.0)],
                ease: Ease::Linear,
                length: Some(0.2),
            },
        });
        part.material = Some(Box::new(PartMaterial {
            file: "test.wgsl".into(),
            source: String::new(),
            space: Space::Note,
            origin: [0.0; 2],
            params: [[0.0; 4]; 8],
            images: vec![],
            compiled: Some(0),
        }));
        play.lanes[0].explosion = Element::Component(Box::new(Component {
            file: "test.svg".into(),
            id: "explosion".into(),
            view_box: [-48.0, -48.0, 96.0, 96.0],
            rotation: 0.0,
            zoom: 1.0,
            parts: vec![part],
            duration: 0.5,
            bindings: Vec::new(),
            pivot: [0.0; 2],
        }));
        play.texture_ids[0] = Some(7);
        let geo = FieldGeometry {
            arrow: 64.0,
            left: 0.0,
            receptor_y: 64.0,
            direction: 1.0,
            width: 256.0,
            height: 512.0,
        };
        let drawn = |play: &mut NoteskinPlay, at: f64, holding| {
            play.update(&frame(at, holding), [].iter(), false);
            let mut out = Vec::new();
            play.explosion_instances(&Layout::dance_single(), &geo, &mut out);
            out.into_iter()
                .filter(|q| q.color[3] > 0.0)
                .map(|q| q.material.unwrap().seed)
                .collect::<Vec<_>>()
        };
        let hold = Some(HoldKind::Hold);
        assert!(drawn(&mut play, 0.0, None).is_empty());
        let held = drawn(&mut play, 0.2, hold);
        assert_eq!(held.len(), 1);
        assert!(drawn(&mut play, 0.35, hold).is_empty());
        assert_eq!(drawn(&mut play, 0.45, hold), held, "its second cycle");
        // Ended at 0.48, mid-pulse: the pulse plays out with the hold's seed.
        assert_eq!(drawn(&mut play, 0.48, None), held, "the pulse under way");
        assert!(drawn(&mut play, 0.55, None).is_empty());
        // Its third would show from 0.6; the hold is over, so it does not.
        assert!(drawn(&mut play, 0.65, None).is_empty(), "no new one");
        assert!(play.lanes[0].ended.is_some());
        // A seek back forgets it.
        drawn(&mut play, 0.1, None);
        assert!(play.lanes[0].ended.is_none());
    }

    #[test]
    fn a_seek_in_the_preview_shows_the_judgments_under_way() {
        use crate::field_preview::step_to;
        use ddi_engine::demo::Demo;
        use ddi_engine::frame::SkinEventKind;
        use ddi_noteskin::component::{Component, Part as ComponentPart, PartMaterial, Space};
        let skin = || {
            let mut play = lua_play("return Def.Sprite {Texture='x.png'}");
            let mut part = ComponentPart::new(Some("flash".into()), None, vec![0]);
            part.when = ["W1", "W2", "W3", "W4", "W5"].map(String::from).to_vec();
            part.material = Some(Box::new(PartMaterial {
                file: "test.wgsl".into(),
                source: String::new(),
                space: Space::Note,
                origin: [0.0; 2],
                params: [[0.0; 4]; 8],
                images: vec![],
                compiled: Some(0),
            }));
            let explosion = Element::Component(Box::new(Component {
                file: "test.svg".into(),
                id: "explosion".into(),
                view_box: [-48.0, -48.0, 96.0, 96.0],
                rotation: 0.0,
                zoom: 1.0,
                parts: vec![part],
                duration: 0.5,
                bindings: Vec::new(),
                pivot: [0.0; 2],
            }));
            for lane in &mut play.lanes {
                lane.explosion = explosion.clone();
            }
            play.texture_ids[0] = Some(7);
            play
        };
        let demo = || Demo::new("dance-single", 150.0, 1.0).unwrap();
        let geo = FieldGeometry {
            arrow: 64.0,
            left: 0.0,
            receptor_y: 64.0,
            direction: 1.0,
            width: 256.0,
            height: 512.0,
        };
        let drawn = |play: &mut NoteskinPlay| {
            let mut out = Vec::new();
            play.explosion_instances(&Layout::dance_single(), &geo, &mut out);
            let mut seeds: Vec<_> = out
                .into_iter()
                .map(|q| {
                    (
                        q.material.unwrap().seed.to_bits(),
                        q.color.map(f32::to_bits),
                    )
                })
                .collect();
            seeds.sort_unstable();
            seeds
        };
        // A little after the third tap judged past the first second.
        let mut probe = demo();
        let taps: Vec<f64> = (0..600)
            .flat_map(|i| probe.step(f64::from(i) / 60.0).events)
            .filter(|e| matches!(e.kind, SkinEventKind::Tap { .. }) && e.song_time > 1.5)
            .map(|e| e.song_time)
            .collect();
        let at = taps[2] + 0.1;
        // Played to it a frame at a time.
        let (mut played, mut d) = (skin(), demo());
        let mut last = None;
        for t in (0..=(at * 60.0) as u32)
            .map(|i| f64::from(i) / 60.0)
            .chain([at])
        {
            step_to(&mut d, Some(&mut played), last, t, false);
            last = Some(t);
        }
        let want = drawn(&mut played);
        assert!(want.len() >= 2, "{want:?}");
        // Sought: the same instances, each at the same point of its fade.
        let (mut sought, mut d) = (skin(), demo());
        step_to(&mut d, Some(&mut sought), None, at, false);
        assert_eq!(drawn(&mut sought), want);
        // The demo's own jump plays nothing.
        let (mut jumped, mut d) = (skin(), demo());
        let step = d.step(at);
        jumped.update(&step.frame, step.events.iter(), false);
        assert!(drawn(&mut jumped).is_empty());
    }

    #[test]
    fn textures_of_the_same_pixels_share_an_id() {
        let mut play = lua_play("return Def.Sprite {Texture='x.png'}");
        play.texture_ids = vec![None; 4];
        play.shared = vec![None; 4];
        // 0 and 2 the same pixels, 1 another's; 3 not to share; a hash
        // shared by images that are not the same is not enough.
        let hashes = [Some(7), Some(7), Some(7), None];
        let pixels = ["a", "b", "a", "a"];
        let pairs = play.share_textures(|i| hashes[i], |a, b| pixels[a] == pixels[b]);
        assert_eq!(pairs, vec![(2, 0)]);
        play.set_texture(0, 10);
        play.set_texture(1, 11);
        assert_eq!(play.texture_ids, vec![Some(10), Some(11), Some(10), None]);
    }

    #[test]
    fn a_part_sharing_its_image_with_a_hold_body_stays_out_of_the_atlas() {
        use ddi_noteskin::build::TextureKey;
        use ddi_noteskin::component::{Component, Part as ComponentPart};
        let mut play = lua_play("return Def.Sprite {Texture='x.png'}");
        let part = play.textures.id(TextureKey::Skin {
            dir: ddi_noteskin::Dir::Folder(0),
            path: "arrow.svg#tap/0".into(),
        });
        play.lanes[0].explosion = Element::Component(Box::new(Component {
            file: "arrow.svg".into(),
            id: "tap".into(),
            view_box: [-48.0, -48.0, 96.0, 96.0],
            rotation: 0.0,
            zoom: 1.0,
            parts: vec![ComponentPart::new(Some("fill".into()), None, vec![part])],
            duration: 1.0,
            bindings: Vec::new(),
            pivot: [0.0; 2],
        }));
        let count = play.textures.keys().len();
        play.texture_ids = (0..count as u32).map(|i| Some(10 + i)).collect();
        assert_eq!(play.packable(), [10 + part]);
        // A sprite's image (texture 0, `x.png`) the same pixels as the
        // part's: one id, its quads moved into the page with the part's.
        play.texture_ids = vec![Some(10); count];
        assert_eq!(play.packable(), [10]);
        // A hold's body drawing it repeats it (wrapped): not packed.
        play.lanes[0].hold.body[0] = Element::Actor(Box::new(Actor::new(Kind::Sprite(
            ddi_actor::Sprite::new(0, Vec::new(), [64.0; 2]),
        ))));
        assert!(play.packable().is_empty());
    }

    #[test]
    fn textures_of_one_document_take_its_image_once() {
        use ddi_noteskin::build::{TextureKey, Textures};
        let mut play = lua_play("return Def.Sprite {Texture='x.png'}");
        let key = |p: &str| TextureKey::Skin {
            dir: ddi_noteskin::Dir::Folder(0),
            path: p.into(),
        };
        let mut textures = Textures::default();
        // `b` drawn from `a`'s document, `c` its own; `d` the same as
        // `c`'s pixels; `m` a model's texture of `a`'s file `x`, as a
        // sprite's `x` is.
        for p in ["b", "a", "c", "d", "x"] {
            textures.id(key(p));
        }
        textures.model_id(key("m"));
        play.textures = textures;
        play.texture_ids = vec![None; 6];
        play.shared = vec![None; 6];
        let mut images: std::collections::HashMap<(usize, String), &str> = [
            ((0, "a".to_string()), "A"),
            ((0, "c".to_string()), "C"),
            ((0, "d".to_string()), "C"),
            ((0, "x".to_string()), "X"),
            ((0, "unused".to_string()), "U"),
        ]
        .into();
        let same = [
            ((0, "b".to_string()), "a".to_string()),
            ((0, "m".to_string()), "x".to_string()),
        ]
        .into();
        let (out, pairs) = play.take_images(&mut images, &same);
        // The first texture of `a`'s document takes its image; the model's
        // gets a copy of its own.
        assert_eq!(
            out,
            [Some("A"), None, Some("C"), Some("C"), Some("X"), Some("X")]
        );
        assert_eq!(pairs, [(1, 0)]);
        assert_eq!(
            images.into_keys().collect::<Vec<_>>(),
            [(0, "unused".to_string())]
        );
        // Pixels shared after: those sharing a texture that shares go on
        // to the one kept.
        play.shared[4] = Some(3);
        let hashes = [None, None, Some(1), Some(1), None, None];
        let pairs = play.share_textures(|i| hashes[i], |_, _| true);
        assert_eq!(pairs, [(3, 2)]);
        assert_eq!(play.shared, [None, Some(0), None, Some(2), Some(2), None]);
        play.set_texture(0, 10);
        play.set_texture(2, 12);
        assert_eq!(
            play.texture_ids,
            [Some(10), Some(10), Some(12), Some(12), Some(12), None]
        );
    }

    /// [`lua_play`] with more text files.
    fn lua_play_with(tap: &str, more: &[(&str, &str)]) -> NoteskinPlay {
        lua_play_on(&Layout::dance_single(), tap, more)
    }

    fn lua_play_on(layout: &Layout, tap: &str, more: &[(&str, &str)]) -> NoteskinPlay {
        let file = |name: &str, text: &str| (name.to_string(), text.to_string());
        let mut texts = vec![
            file("metrics.ini", "[Global]\nFallbackNoteSkin=default\n"),
            file(
                "Down Explosion.lua",
                "return Def.ActorFrame {\n\
                   ColumnJudgmentMessageCommand = function(self, p)\n\
                     self:x(p.Column * 10 + (p.Bright and 1 or 0))\n\
                     self.last = p.TapNoteScore or p.HoldNoteScore\n\
                   end;\n\
                   W2Command = function(self) self:y(self.last == 'TapNoteScore_W2' and 2 or -1) end;\n\
                   HoldingOnCommand = function(self) self:zoom(3) end;\n\
                   Def.Sprite { Texture = 'x.png' };\n\
                 }\n",
            ),
            file(
                "Down Receptor.lua",
                "return Def.ActorFrame {\n\
                   InitCommand = function(self)\n\
                     self:SetUpdateFunction(function(s) s:y(GAMESTATE:GetSongBeat()) end)\n\
                   end;\n\
                   PressCommand = function(self) self:x(7) end;\n\
                   Def.Sprite { Texture = 'x.png' };\n\
                 }\n",
            ),
            file("Down Tap Note.lua", tap),
        ];
        texts.extend(more.iter().map(|(n, t)| file(n, t)));
        let mut files: Vec<String> = texts.iter().map(|(n, _)| n.clone()).collect();
        files.push("x.png".into());
        let folders = vec![Folder {
            game: "dance".into(),
            name: "live".into(),
            files,
            texts,
        }];
        let skin = Skin::load(&folders, "dance", "live").unwrap().into_owned();
        let mut t = 0.0;
        let lua = LuaSkin::new(
            skin,
            Env::default(),
            ddi_lua::Settings::default(),
            LoadBudget::default(),
            Box::new(move || {
                t += 1e-6;
                t
            }),
        )
        .unwrap();
        let mut f = std::pin::pin!(NoteskinPlay::with_lua(lua, layout, &NoImages, false, None));
        match f
            .as_mut()
            .poll(&mut std::task::Context::from_waker(std::task::Waker::noop()))
        {
            std::task::Poll::Ready(r) => r.unwrap().unwrap(),
            std::task::Poll::Pending => panic!("pending"),
        }
    }

    /// The shells' load ([`NoteskinPlay::load_lua`]): with no pause it
    /// never waits (desktop polls it to its end at once); with one it
    /// gives the host turns (the web's) and builds the same skin; a
    /// missing skin is an error.
    #[test]
    fn load_lua_runs_through_or_in_slices() {
        let file = |name: &str, text: &str| (name.to_string(), text.to_string());
        let texts = vec![
            file("metrics.ini", "[Global]\nFallbackNoteSkin=default\n"),
            file(
                "Down Tap Note.lua",
                "local n = 0\nfor i = 1, 100000 do n = n + i end\n\
                 return Def.ActorFrame { Def.Sprite { Texture = 'x.png' } }\n",
            ),
        ];
        let mut files: Vec<String> = texts.iter().map(|(n, _)| n.clone()).collect();
        files.push("x.png".into());
        let folders = vec![Folder {
            game: "dance".into(),
            name: "slow".into(),
            files,
            texts,
        }];
        let clock = || -> Box<dyn FnMut() -> f64> {
            let mut t = 0.0;
            Box::new(move || {
                t += 1e-6;
                t
            })
        };
        let cx = &mut std::task::Context::from_waker(std::task::Waker::noop());
        let layout = Layout::dance_single();
        let mut load = |name: &str, pause: Option<&mut Pause>| {
            let mut f = std::pin::pin!(NoteskinPlay::load_lua(
                &folders,
                "dance",
                name,
                Env::default(),
                7,
                &layout,
                &NoImages,
                clock(),
                pause,
            ));
            match f.as_mut().poll(cx) {
                std::task::Poll::Ready(r) => r,
                std::task::Poll::Pending => panic!("waited without a pause"),
            }
        };
        let straight = load("slow", None).unwrap().unwrap();
        // The host's turns: each resolves at once, counted.
        let turns = std::rc::Rc::new(std::cell::Cell::new(0));
        let counted = turns.clone();
        let mut pause = move || -> std::pin::Pin<Box<dyn Future<Output = ()>>> {
            counted.set(counted.get() + 1);
            Box::pin(std::future::ready(()))
        };
        let sliced = load("slow", Some(&mut pause)).unwrap().unwrap();
        assert!(turns.get() > 0);
        assert_eq!(straight.own_elements(1), sliced.own_elements(1));
        assert_eq!(straight.notes, sliced.notes);
        assert!(load("none", None).is_err());
    }

    /// The shipped DanOni scripts, through exactly the same loader as play.
    fn danoni_play(layout: &Layout) -> NoteskinPlay {
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../assets/noteskins/dance/DanOni");
        let mut files: Vec<String> = std::fs::read_dir(&root)
            .unwrap()
            .flatten()
            .filter(|e| e.path().is_file())
            .map(|e| e.file_name().to_str().unwrap().to_owned())
            .collect();
        files.sort();
        let texts = files
            .iter()
            .filter(|f| ddi_noteskin::import::kept_text(f))
            .map(|f| (f.clone(), std::fs::read_to_string(root.join(f)).unwrap()))
            .collect();
        let folders = [Folder {
            game: "dance".into(),
            name: "DanOni".into(),
            files,
            texts,
        }];
        let skin = Skin::load(&folders, "dance", "DanOni")
            .unwrap()
            .into_owned();
        let lua = LuaSkin::new(
            skin,
            Env {
                columns: layout.lane_count() as u32,
                ..Env::default()
            },
            ddi_lua::Settings::default(),
            LoadBudget::default(),
            Box::new(|| 0.0),
        )
        .unwrap();
        let mut future =
            std::pin::pin!(NoteskinPlay::with_lua(lua, layout, &NoImages, false, None));
        let std::task::Poll::Ready(result) = future
            .as_mut()
            .poll(&mut std::task::Context::from_waker(std::task::Waker::noop()))
        else {
            panic!("pending")
        };
        let mut play = result.unwrap().unwrap();
        assert!(play.notes.is_empty(), "{:?}", play.notes);
        for i in 0..play.texture_keys().len() {
            play.set_texture(i, i as u32);
        }
        play
    }

    fn danoni_note(index: usize, lane: u8, y: f32, kind: SpriteKind) -> NoteSprite {
        NoteSprite {
            note_index: index,
            lane,
            y,
            kind,
            quantization: ddi_chart::Quantization::N4,
            beat_frac: 0.0,
            color: None,
            alpha: 1.0,
            glow: 0.0,
            beat: index as f32,
            hold_life: 0.0,
        }
    }

    #[test]
    fn mine_adjacency_is_independent_of_lane_order_in_each_layout() {
        for id in [
            "dance-single",
            "dance-solo",
            "dance-double",
            "pump-single",
            "danoni-9B",
        ] {
            let layout = Layout::builtin(id).unwrap();
            let notes: Vec<_> = layout
                .lanes
                .iter()
                .enumerate()
                .map(|(i, _)| {
                    let mut note = danoni_note(i, i as u8, 2.0, SpriteKind::Mine);
                    note.beat = 8.0;
                    note
                })
                .collect();
            for reverse in [false, true] {
                let geo =
                    FieldGeometry::new(1200.0, 800.0, crate::scene::field_span(&layout), reverse);
                let nodes: Vec<_> = notes
                    .iter()
                    .zip(&layout.lanes)
                    .map(|(note, l)| MineEndpoint {
                        note,
                        at: [geo.lane_x(l.column), geo.lane_y(l.scroll_sign, note.y)],
                    })
                    .collect();
                let mut row_counts = std::collections::HashMap::new();
                for node in &nodes {
                    *row_counts.entry(node.at[1].to_bits()).or_insert(0usize) += 1;
                }
                let expected: usize = row_counts.values().map(|n| n.saturating_sub(1)).sum();
                let links = aligned_mines(nodes.clone(), geo.arrow);
                assert_eq!(links.len(), expected, "{id} reverse={reverse}");
                let mut reordered = nodes.clone();
                reordered.reverse();
                let shuffled = aligned_mines(reordered, geo.arrow);
                let ids = |links: Vec<[MineEndpoint<'_>; 2]>| {
                    links
                        .iter()
                        .map(|[a, b]| (a.note.lane, b.note.lane))
                        .collect::<Vec<_>>()
                };
                assert_eq!(ids(links.clone()), ids(shuffled));
                for [a, b] in links {
                    assert!(a.at[0] < b.at[0]);
                    assert_eq!(a.at[1], b.at[1]);
                    assert!(
                        !nodes
                            .iter()
                            .any(|n| n.at[1] == a.at[1] && n.at[0] > a.at[0] && n.at[0] < b.at[0]),
                        "only physical neighbours connect"
                    );
                }
            }
        }
    }

    #[test]
    fn mine_links_stop_at_empty_lanes() {
        let mut notes: Vec<_> = (0..4)
            .map(|lane| danoni_note(lane, lane as u8, 2.0, SpriteKind::Mine))
            .collect();
        for note in &mut notes {
            note.beat = 4.0;
        }
        let nodes: Vec<_> = notes
            .iter()
            .enumerate()
            .map(|(i, note)| MineEndpoint {
                note,
                at: [i as f32 * 64.0, 100.0],
            })
            .collect();
        assert_eq!(aligned_mines(nodes.clone(), 64.0).len(), 3);
        let separated = vec![nodes[0], nodes[2], nodes[3]];
        let links = aligned_mines(separated, 64.0);
        assert_eq!(links.len(), 1);
        assert_eq!((links[0][0].note.lane, links[0][1].note.lane), (2, 3));
        assert!(aligned_mines(vec![nodes[0], nodes[3]], 64.0).is_empty());
    }

    #[test]
    fn layered_mines_connect_drawn_neighbours_and_keep_complete_isolated_effects() {
        let mut layout = Layout::dance_single();
        for (lane, column) in layout.lanes.iter_mut().zip([3.0, 0.0, 2.0, 1.0]) {
            lane.column = column;
        }
        let mine = r#"return Def.ActorFrame {
            Def.Sprite { Name='Base', Texture='x.png' },
            Def.Sprite { Name='LocalElectricity', Texture='x.png' },
            Def.Sprite { Name='MineLink', Texture='x.png' },
        }"#;
        let mut play = lua_play_on(
            &layout,
            "return Def.Sprite {Texture='x.png'}",
            &[
                ("Left Tap Mine.lua", mine),
                ("Down Tap Mine.lua", mine),
                ("Up Tap Mine.lua", mine),
                ("Right Tap Mine.lua", mine),
            ],
        );
        for i in 0..play.texture_keys().len() {
            play.set_texture(i, i as u32);
        }
        for lane in &mut play.lanes {
            lane.metrics.mine_connect_rows = true;
        }
        for reverse in [false, true] {
            let geo = FieldGeometry::new(800.0, 800.0, 4.0, reverse);
            let mut f = frame(0.1, None);
            f.notes = (0..3)
                .map(|lane| danoni_note(lane, lane as u8, 2.0, SpriteKind::Mine))
                .collect();
            for note in &mut f.notes {
                note.beat = 4.0;
            }
            let mut out = Vec::new();
            play.update(&f, [].iter(), reverse);
            play.receptors(&f, &layout, &geo, &mut out);
            out.clear();
            play.notes(&f, &layout, &geo, &mut out);
            assert_eq!(
                out.len(),
                7,
                "three bases + local effects, one adjacent bridge: {:?}; {:?}",
                play.lanes
                    .iter()
                    .map(|l| (l.mine.is_skin(), has_mine_link(&l.mine)))
                    .collect::<Vec<_>>(),
                play.problems()
            );
            for (quad, (a, b)) in out[..1].iter().zip([(2.0, 3.0)]) {
                let Shape::Quad { transform, .. } = quad.shape else {
                    panic!("not a quad")
                };
                assert!((transform[4] - (geo.lane_x(a) + geo.lane_x(b)) * 0.5).abs() < 1e-4);
                assert!((transform[0] - (geo.lane_x(b) - geo.lane_x(a))).abs() < 1e-4);
                assert!(
                    (transform[3].abs() - geo.arrow).abs() < 1e-4,
                    "stretch length, preserve bolt thickness"
                );
            }
            f.notes[2].alpha = 0.0;
            out.clear();
            play.notes(&f, &layout, &geo, &mut out);
            assert_eq!(
                out.len(),
                4,
                "hidden mines add neither local art nor bridges across the empty lanes"
            );
            f.notes[2].alpha = 1.0;
            f.notes[1].kind = SpriteKind::Shock;
            out.clear();
            play.notes(&f, &layout, &geo, &mut out);
            assert_eq!(
                out.len(),
                7,
                "shock presentation shares the layered mine artwork"
            );
            // A different physical row must not connect even at the same beat.
            f.notes[2].y = 3.0;
            out.clear();
            play.notes(&f, &layout, &geo, &mut out);
            assert_eq!(out.len(), 6);
            // Equal screen positions on different chart rows must not connect.
            f.notes[2].y = 2.0;
            f.notes[0].beat = 5.0;
            out.clear();
            play.notes(&f, &layout, &geo, &mut out);
            assert_eq!(out.len(), 6);
            f.notes.truncate(1);
            out.clear();
            play.notes(&f, &layout, &geo, &mut out);
            assert_eq!(
                out.len(),
                2,
                "isolated mine keeps its base and full local electricity"
            );
            let Element::Actor(a) = &play.lanes[0].mine else {
                panic!("missing mine")
            };
            assert!(
                a.children().iter().all(|c| c.visible),
                "temporary draw visibility must be restored"
            );
            play.lanes[0].metrics.mine_connect_rows = false;
            out.clear();
            play.notes(&f, &layout, &geo, &mut out);
            assert_eq!(out.len(), 3, "no extension behaviour without opt-in");
            play.lanes[0].metrics.mine_connect_rows = true;
        }
    }

    #[test]
    fn mine_rims_merge_only_toward_visible_physical_neighbours() {
        let mut layout = Layout::dance_single();
        for (lane, column) in layout.lanes.iter_mut().zip([3.0, 0.0, 2.0, 1.0]) {
            lane.column = column;
        }
        let mine = r#"return Def.ActorFrame {
            Def.Sprite {Name='Base',Texture='x.png',InitCommand=function(self) self:diffuse(1,0,0,1) end},
            Def.Sprite {Name='MineRimLeft',Texture='x.png',InitCommand=function(self) self:diffuse(0,1,0,1) end},
            Def.Sprite {Name='MineRimRight',Texture='x.png',InitCommand=function(self) self:diffuse(0,1,0,1) end},
            Def.Sprite {Name='MineLink',Texture='x.png',InitCommand=function(self) self:diffuse(0,0,1,1) end},
        }"#;
        let mut play = lua_play_on(
            &layout,
            "return Def.Sprite {Texture='x.png'}",
            &[
                ("Left Tap Mine.lua", mine),
                ("Down Tap Mine.lua", mine),
                ("Up Tap Mine.lua", mine),
                ("Right Tap Mine.lua", mine),
            ],
        );
        for i in 0..play.texture_keys().len() {
            play.set_texture(i, i as u32);
        }
        for lane in &mut play.lanes {
            lane.metrics.mine_connect_rows = true;
        }
        for reverse in [false, true] {
            let geo = FieldGeometry::new(800.0, 800.0, 4.0, reverse);
            let mut f = frame(0.1, None);
            f.notes = (0..4)
                .map(|lane| {
                    let mut n = danoni_note(lane, lane as u8, 2.0, SpriteKind::Mine);
                    n.beat = 4.0;
                    n
                })
                .collect();
            let draw = |play: &mut NoteskinPlay, f: &Frame| {
                let mut out = Vec::new();
                play.notes(f, &layout, &geo, &mut out);
                out.len()
            };
            play.update(&f, [].iter(), reverse);
            assert_eq!(
                draw(&mut play, &f),
                9,
                "four bases, two outside arcs, three links"
            );
            let mut ordered = Vec::new();
            play.notes(&f, &layout, &geo, &mut ordered);
            assert!(
                ordered[..4]
                    .iter()
                    .all(|q| q.color[0] > 0.9 && q.color[1] < 0.1 && q.color[2] < 0.1),
                "all mine bodies render before electricity"
            );
            assert!(
                ordered[4..]
                    .iter()
                    .all(|q| q.color[0] < 0.1 && (q.color[1] > 0.9 || q.color[2] > 0.9)),
                "both rims and connecting arcs render on top"
            );
            f.notes[3].alpha = 0.0; // Physical column 1; leave an actual gap.
            assert_eq!(
                draw(&mut play, &f),
                8,
                "isolated mine plus adjacent pair keep only their outside arcs"
            );
            f.notes[3].alpha = 1.0;
            assert_eq!(
                draw(&mut play, &f),
                9,
                "temporary arc hiding must not persist"
            );
            for lane in &play.lanes {
                let Element::Actor(a) = &lane.mine else {
                    panic!("actor")
                };
                assert!(a.children().iter().all(|c| c.visible));
            }
            for (i, n) in f.notes.iter_mut().enumerate() {
                n.beat = i as f32;
            }
            assert_eq!(
                draw(&mut play, &f),
                12,
                "different chart rows retain complete local rings"
            );
            f.notes.truncate(1);
            assert_eq!(
                draw(&mut play, &f),
                3,
                "a single mine keeps both halves and no bridge"
            );
        }
    }

    #[test]
    fn material_roll_shadows_and_charge_follow_the_moving_body() {
        use ddi_actor::{Sprite, State};
        let layout = Layout::builtin("dance-single").unwrap();
        let play = danoni_play(&layout);
        let sprite = |texture, count| {
            Element::Actor(Box::new(Actor::new(Kind::Sprite(Sprite::new(
                texture,
                (0..count)
                    .map(|i| State {
                        rect: [
                            i as f32 / count as f32,
                            0.0,
                            (i + 1) as f32 / count as f32,
                            1.0,
                        ],
                        delay: 0.1,
                    })
                    .collect(),
                [96.0, 96.0],
            )))))
        };
        let mut parts = HoldParts {
            head: vec![Element::Blank],
            tail: vec![Element::Blank],
            body: vec![sprite(0, 5)],
            bottomcap: vec![sprite(1, 5)],
            topcap: vec![sprite(2, 4)],
        };
        // A material's fill, its quad a stand-in.
        let fill = hold_material::Fill::Skin {
            quad: crate::material::MaterialQuad {
                material: 0,
                slot: 0,
                axes: [0.0; 4],
                origin: [0.0; 2],
                phase: 0.0,
                seed: 0.0,
                value: [0.0; 4],
                images: [UNTEXTURED; 2],
            },
            anchor: ddi_noteskin::component::Anchor::Head,
            origin: [0.0; 2],
        };
        let geo = FieldGeometry::new(800.0, 800.0, 4.0, false);
        for crop in [-12.75_f32, 0.0, 15.75] {
            let Element::Actor(topcap) = &mut parts.topcap[0] else {
                panic!("sprite")
            };
            topcap
                .call(
                    &ddi_actor::commands::Call {
                        method: "y".into(),
                        args: vec![ddi_actor::commands::Arg::Num(crop)],
                    },
                    &mut Problems::default(),
                    0,
                )
                .unwrap();
            for length in [32.0, 64.0, 96.0, 173.0] {
                for dir in [-1.0, 1.0] {
                    let note = danoni_note(
                        0,
                        0,
                        0.0,
                        SpriteKind::RollHead {
                            tail_y: length / 64.0,
                            active: false,
                            dropped: false,
                        },
                    );
                    let mut out = Vec::new();
                    let layers =
                        hold_material::HoldLayers::of_sprites(&parts, 0, 25.0, fill, Some(0.85))
                            .expect("every layer");
                    play.material_hold(
                        &layers,
                        &note,
                        &frame(0.0, None),
                        &geo,
                        [200.0, 400.0, 400.0 + length * dir, 1.0, 1.0],
                        &[],
                        &mut out,
                    );
                    // The wave is one painted quad over the visible note,
                    // and the mask is cleared within the note's box.
                    let painted: Vec<_> = out.iter().filter(|q| q.material.is_some()).collect();
                    assert_eq!(painted.len(), 1);
                    for q in out.iter().filter(|q| q.mask == Mask::Clear) {
                        let Shape::Quad { transform, .. } = q.shape else {
                            panic!("quad")
                        };
                        assert!(transform[0] < 120.0 && transform[3] < length + 120.0);
                    }
                    let overlays: Vec<_> = out
                        .iter()
                        .filter(|q| q.texture == 1 && q.mask != Mask::Write)
                        .collect();
                    assert_eq!(
                        overlays.len(),
                        3,
                        "cap shadow, overlap shading, charge: one pass each"
                    );
                    for q in overlays {
                        let Shape::Quad { transform, uv } = q.shape else {
                            panic!("quad")
                        };
                        let top = (transform[5] - 400.0) * dir - transform[3].abs() / 2.0;
                        assert!(
                            (top - (length - 48.0).max(crop)).abs() < 1e-4,
                            "all cap overlays stay attached to the cap"
                        );
                        assert!(
                            (uv[1] - (crop + 48.0 - length).max(0.0) / 96.0).abs() < 1e-5,
                            "crop only at the head plane"
                        );
                    }
                    let body: Vec<_> = out.iter().filter(|q| q.texture == 0).collect();
                    assert_eq!(
                        body.len(),
                        if length - 48.0 > crop { 5 } else { 0 },
                        "band, envelope, exterior, overlap, pulse"
                    );
                    if !body.is_empty() {
                        let Shape::Quad {
                            transform: position,
                            uv: coords,
                        } = body[0].shape
                        else {
                            panic!("quad")
                        };
                        assert!(
                            (coords[1] - (crop + 48.0 - length) / 25.0).abs() < 1e-5,
                            "phase follows the shrinking tail distance"
                        );
                        for q in body {
                            let Shape::Quad { transform, uv } = q.shape else {
                                panic!("quad")
                            };
                            assert_eq!(transform, position, "repeat layers move together");
                            assert_eq!(
                                [uv[1], uv[3]],
                                [coords[1], coords[3]],
                                "repeat layers sample the same tail-anchored phase"
                            );
                        }
                    }
                    assert!(
                        out.iter()
                            .filter(|q| q.texture == 2)
                            .all(|q| matches!(q.shape,Shape::Quad{uv,..} if uv[0]<0.5)),
                        "never draw frozen head crops of body shading/charge"
                    );
                }
            }
        }
    }

    #[test]
    fn roll_cues_follow_scaled_windows_in_seconds_without_changing_holds() {
        use ddi_engine::rules::presets;
        let skin = Skin::load(&[], "dance", "default").unwrap();
        let mut metrics = NoteDisplay::read(&skin, "Down");
        metrics.roll_animation_timing_window_ratio = 0.75;
        metrics.animation_is_beat_based = true;
        metrics.parts[Part::HoldBody as usize].animation_length = 4.0;
        metrics.parts[Part::HoldBody as usize].animation_is_vivid = false;
        let note = danoni_note(
            0,
            0,
            0.0,
            SpriteKind::RollHead {
                tail_y: 2.0,
                active: true,
                dropped: false,
            },
        );
        for (mut rules, expected_period) in [
            (presets::itg(), 0.2625),
            (presets::sm5(), 0.375),
            (presets::ddr_a(), 0.1875),
        ] {
            // Include non-default timing scale/add, which judging applies to rolls.
            rules.judge.scale = 0.8;
            rules.judge.add = 0.01;
            let period = expected_period * 0.8 + 0.0075;
            for cycles in [0.125, 0.375, 0.625, 0.875, 3.375] {
                let mut f = frame(cycles * period, None);
                f.roll_window_seconds = rules.judge.roll_window_seconds();
                // Including a frozen beat (a stop): only elapsed seconds count.
                for beat in [0.0, 1.0, 15.25, 700.0] {
                    f.beat = beat;
                    let phase = long_note_animation_percent(
                        &metrics,
                        metrics.part(Part::HoldBody),
                        &note,
                        &f,
                    );
                    assert!((f64::from(phase) - cycles.fract()).abs() < 1e-5);
                }
            }
        }
        let mut f = frame(0.1, None);
        f.beat = 1.0;
        let mut hold = note;
        hold.kind = SpriteKind::HoldHead {
            tail_y: 2.0,
            active: true,
            dropped: false,
        };
        assert_eq!(
            long_note_animation_percent(&metrics, metrics.part(Part::HoldBody), &hold, &f),
            0.25
        );
        metrics.roll_animation_timing_window_ratio = 0.0;
        assert_eq!(
            long_note_animation_percent(&metrics, metrics.part(Part::HoldBody), &note, &f),
            0.25
        );
        metrics.roll_animation_timing_window_ratio = 0.75;
        f.roll_window_seconds = 0.0;
        assert_eq!(
            long_note_animation_percent(&metrics, metrics.part(Part::HoldBody), &note, &f),
            0.25
        );
    }

    #[test]
    fn danoni_flash_uses_the_hit_notes_last_drawn_position_in_every_orientation() {
        use ddi_engine::frame::SkinNote;
        for reverse in [false, true] {
            for lower in [false, true] {
                let mut layout = Layout::builtin("danoni-7").unwrap();
                if lower {
                    for l in &mut layout.lanes {
                        l.scroll_sign = -1;
                    }
                }
                for auto in [false, true] {
                    let mut play = danoni_play(&layout);
                    let geo = FieldGeometry::new(700.0, 800.0, 7.0, reverse);
                    let mut before = frame(0.0, None);
                    before.notes = (0..7)
                        .map(|i| danoni_note(i, i as u8, 0.35 + i as f32 * 0.1, SpriteKind::Tap))
                        .collect();
                    play.update(&before, [].iter(), reverse);
                    play.notes(&before, &layout, &geo, &mut Vec::new());
                    let events: Vec<_> = (0..7u8)
                        .flat_map(|lane| {
                            [
                                SkinEvent {
                                    seq: u64::from(lane) * 2,
                                    lane,
                                    kind: SkinEventKind::Press,
                                    song_time: 0.01,
                                    autoplay: auto,
                                    danoni_judgments: false,
                                    note: None,
                                },
                                SkinEvent {
                                    seq: u64::from(lane) * 2 + 1,
                                    lane,
                                    kind: SkinEventKind::NoteTap {
                                        judgement: Judgement::W3,
                                        bright: true,
                                    },
                                    song_time: 0.01,
                                    autoplay: auto,
                                    danoni_judgments: false,
                                    note: Some(SkinNote {
                                        index: usize::from(lane),
                                        y: -1.0,
                                    }),
                                },
                            ]
                        })
                        .collect();
                    let hit = frame(0.01, None);
                    play.update(&hit, events.iter(), reverse);
                    for lane in 0..7 {
                        let ex = actor(&play.lanes[lane].explosion);
                        let flash = &ex.children()[1];
                        assert_eq!(
                            flash.current().diffuse(),
                            if auto {
                                [0.6, 0.6, 0.6, 1.0]
                            } else {
                                [0.6, 1.0, 0.6, 0.75]
                            }
                        );
                        assert_eq!(
                            actor(&play.lanes[lane].receptor).children()[1].visible,
                            !auto
                        );
                        // Rotate the local displacement back into field coordinates.
                        let (sin, cos) = ex.base_rotation[2].to_radians().sin_cos();
                        let [x, y, _] = flash.current().pos;
                        let expected = if auto {
                            0.0
                        } else {
                            (0.35 + lane as f32 * 0.1)
                                * 64.0
                                * geo.lane_direction(layout.lanes[lane].scroll_sign)
                        };
                        assert!((x * cos - y * sin).abs() < 1e-4);
                        assert!(
                            (x * sin + y * cos - expected).abs() < 1e-4,
                            "lane {lane}, reverse {reverse}, lower {lower}"
                        );
                    }
                    // The actual quads must have those centers too (not just Lua state).
                    let mut out = Vec::new();
                    play.explosions(&hit, &layout, &geo, &mut out);
                    assert_eq!(out.len(), 7);
                    for (lane, q) in out.iter().enumerate() {
                        let Shape::Quad { transform, .. } = q.shape else {
                            panic!("not a quad")
                        };
                        assert!(
                            (transform[4] - geo.lane_x(layout.lanes[lane].column)).abs() < 1e-3
                        );
                        let offset = if auto { 0.0 } else { 0.35 + lane as f32 * 0.1 };
                        assert!(
                            (transform[5] - geo.lane_y(layout.lanes[lane].scroll_sign, offset))
                                .abs()
                                < 1e-3
                        );
                    }
                    assert!(play.lua_failure().is_none(), "{:?}", play.problems());
                }
            }
        }
    }

    #[test]
    fn danoni_dropped_freezes_are_per_note_and_keep_exact_colors() {
        let layout = Layout::builtin("danoni-7").unwrap();
        let geo = FieldGeometry::new(700.0, 800.0, 7.0, false);
        let mut play = danoni_play(&layout);
        assert_eq!(play.own_elements(0), 36);
        let mut f = frame(0.0, None);
        f.receptors.resize(7, f.receptors[0]);
        for (lane, roll) in [(0, false), (3, true)] {
            // A release still inside the grace period must keep active art.
            f.receptors[usize::from(lane)].pressed = false;
            for (i, (active, dropped, y)) in
                [(false, false, 2.0), (true, false, 0.0), (false, true, -0.4)]
                    .into_iter()
                    .enumerate()
            {
                let kind = if roll {
                    SpriteKind::RollHead {
                        tail_y: y + 1.5,
                        active,
                        dropped,
                    }
                } else {
                    SpriteKind::HoldHead {
                        tail_y: y + 1.5,
                        active,
                        dropped,
                    }
                };
                f.notes
                    .push(danoni_note(usize::from(lane) * 3 + i, lane, y, kind));
            }
        }
        play.update(&f, [].iter(), false);
        let mut out = Vec::new();
        play.holds(&f, &layout, &geo, &mut out);
        for name in ["_bar inactive.svg", "_bar active.svg", "_bar dropped.svg"] {
            let id = play
                .texture_keys()
                .iter()
                .position(|key| matches!(key,TextureKey::Skin {path,..} if path == name))
                .unwrap() as u32;
            let drawn: Vec<_> = out.iter().filter(|q| q.texture == id).collect();
            assert!(!drawn.is_empty(), "{name}");
            assert!(
                drawn.iter().all(|q| q.color == [1.0; 4]),
                "dropped artwork must not be dimmed by hold life"
            );
        }
        for lane in [0, 3] {
            let parts = if lane == 0 {
                &play.lanes[lane].hold
            } else {
                &play.lanes[lane].roll
            };
            for part in [&parts.head, &parts.tail] {
                let dropped = actor(&part[2]);
                assert_eq!(
                    dropped.children()[0].current().diffuse(),
                    [0.2, 0.2, 0.2, 1.0]
                );
                assert_eq!(
                    dropped.children()[1].current().diffuse(),
                    [0.8, 0.8, 0.8, 1.0]
                );
                assert!(dropped.children()[1].visible);
                assert_eq!(
                    actor(&part[0]).children()[1].current().diffuse(),
                    [0.4, 1.0, 1.0, 1.0]
                );
            }
        }
        assert!(out.iter().any(|q| q.color == [0.8, 0.8, 0.8, 1.0]));
        assert!(out.iter().any(|q| q.color == [0.2, 0.2, 0.2, 1.0]));
        assert!(play.lua_failure().is_none(), "{:?}", play.problems());
        // A normal StepMania skin still loads exactly the original two states.
        let standard = lua_play("return Def.ActorFrame { Def.Sprite {Texture='x.png'} }");
        assert_eq!(standard.lanes[0].hold.head.len(), 2);
        assert_eq!(standard.lanes[0].elements().count(), 26);
    }

    fn actor(e: &Element) -> &Actor {
        match e {
            Element::Actor(a) => a,
            other => panic!("{other:?}"),
        }
    }

    use std::future::Future;

    #[test]
    fn an_onigiri_the_skins_lua_cannot_draw_as_its_centre_is_its_down() {
        // Centre art, but a redirect table without `Center`: `GetPath`
        // gets no button and the element fails.
        let skin_lua = "local ret = ... or {}\n\
            ret.RedirTable = { Left = 'Down', Down = 'Down', Up = 'Down', Right = 'Down' }\n\
            local OldRedir = ret.Redir\n\
            ret.Redir = function(b, e) b, e = OldRedir(b, e) return ret.RedirTable[b], e end\n\
            return ret\n";
        let layout = Layout::builtin("danoni-5").unwrap();
        let play = lua_play_on(
            &layout,
            "return Def.ActorFrame { Def.Sprite { Texture = 'x.png' } }\n",
            &[
                ("NoteSkin.lua", skin_lua),
                (
                    "Center Tap Note.lua",
                    "return Def.Sprite { Texture = 'x.png' }\n",
                ),
            ],
        );
        // The onigiri (lane 4) is drawn with Down's tap note, the skin's.
        let onigiri = &play.lanes[4];
        assert!(matches!(onigiri.tap, Element::Actor(_)), "{:?}", play.notes);
        assert!(matches!(onigiri.receptor, Element::Actor(_)));
    }

    #[test]
    fn a_dancing_onigiri_layout_takes_the_skins_centre_and_turns_its_down() {
        let files = [
            "metrics.ini",
            "Down Tap Note.png",
            "Down Receptor.png",
            "Center Tap Note.png",
            "Center Receptor.png",
        ];
        let folders = vec![Folder {
            game: "dance".into(),
            name: "s".into(),
            files: files.iter().map(|f| f.to_string()).collect(),
            texts: vec![(
                "metrics.ini".into(),
                "[Global]\nFallbackNoteSkin=common\n".into(),
            )],
        }];
        let skin = Skin::load(&folders, "dance", "s").unwrap();
        let layout = Layout::builtin("danoni-7").unwrap();
        let play = NoteskinPlay::new(&skin, &layout, &NoImages, false).unwrap();
        let texture = |e: &Element| match &actor(e).kind {
            ddi_actor::Kind::Sprite(s) => play.textures.keys()[s.texture as usize].clone(),
            other => panic!("{other:?}"),
        };
        // Lane 1, down-left: Down's tap note and receptor turned 45°.
        let lane = &play.lanes[1];
        assert_eq!(actor(&lane.tap).base_rotation[2], 45.0);
        assert_eq!(actor(&lane.receptor).base_rotation[2], 45.0);
        // Lane 3, the onigiri: the skin's centre, unturned.
        let onigiri = &play.lanes[3];
        assert_eq!(
            texture(&onigiri.tap),
            TextureKey::Skin {
                dir: ddi_noteskin::Dir::Folder(0),
                path: "Center Tap Note.png".into()
            }
        );
        assert_eq!(actor(&onigiri.tap).base_rotation[2], 0.0);
        // A layout with a lane no skin draws: none.
        let seven_i = ddi_chart::layout::danoni::builtin("7i")
            .unwrap()
            .layout("danoni-7i".into());
        assert!(NoteskinPlay::new(&skin, &seven_i, &NoImages, false).is_none());
    }

    #[test]
    fn a_pump_layout_takes_the_skins_own_panels_and_turns_its_down() {
        let files = [
            "metrics.ini",
            "Down Tap Note.png",
            "Down Receptor.png",
            "DownLeft Tap Note.png",
            "DownLeft Receptor.png",
        ];
        let folders = vec![Folder {
            game: "dance".into(),
            name: "s".into(),
            files: files.iter().map(|f| f.to_string()).collect(),
            texts: vec![(
                "metrics.ini".into(),
                "[Global]\nFallbackNoteSkin=common\n".into(),
            )],
        }];
        let skin = Skin::load(&folders, "dance", "s").unwrap();
        for id in ["pump-single", "pump-halfdouble", "pump-double"] {
            let layout = Layout::builtin(id).unwrap();
            let play = NoteskinPlay::new(&skin, &layout, &NoImages, false).unwrap();
            let texture = |e: &Element| match &actor(e).kind {
                ddi_actor::Kind::Sprite(s) => match &play.textures.keys()[s.texture as usize] {
                    TextureKey::Skin { path, .. } => path.clone(),
                    other => panic!("{other:?}"),
                },
                other => panic!("{other:?}"),
            };
            for (lane, info) in play.lanes.iter().zip(&layout.lanes) {
                let (tap, turn) = match info.name.as_str() {
                    // The skin's own down-left, as it is.
                    "downleft" => ("DownLeft Tap Note.png", 0.0),
                    // Down turned for the down-right, the stand-in's
                    // diagonals for the upper ones, Down for the centre.
                    "downright" => ("Down Tap Note.png", 315.0),
                    "center" => ("Down Tap Note.png", 0.0),
                    _ => continue,
                };
                assert_eq!(texture(&lane.tap), tap, "{id} {}", info.name);
                assert_eq!(
                    actor(&lane.tap).base_rotation[2],
                    turn,
                    "{id} {}",
                    info.name
                );
                assert_eq!(
                    actor(&lane.receptor).base_rotation[2],
                    turn,
                    "{id} {}",
                    info.name
                );
            }
        }
    }

    /// A pump skin with the left side's art only (as StepMania's
    /// `pump/simple`), on StepMania's `pump/default` (the stand-in, its
    /// Lua): every panel draws its own, the right side the left's,
    /// mirrored (`BaseRotationY` 180), and nothing falls back on a dance
    /// skin.
    #[test]
    fn a_pump_skin_draws_every_panel_through_pump_defaults_lua() {
        let mut files: Vec<String> = [
            "UpLeft Tap Note.png",
            "DownLeft Tap Note.png",
            "Center Tap Note.png",
            "UpLeft Ready Receptor.png",
            "DownLeft Ready Receptor.png",
            "Center Ready Receptor.png",
            "UpLeft Hold Body Active.png",
            "DownLeft Hold Body Active.png",
            "Center Hold Body Active.png",
        ]
        .iter()
        .map(|f| f.to_string())
        .collect();
        files.push("metrics.ini".into());
        let folders = vec![
            // A dance skin of the same name: never used.
            Folder {
                game: "dance".into(),
                name: "simple".into(),
                files: vec!["Down Tap Note.png".into()],
                texts: Vec::new(),
            },
            Folder {
                game: "pump".into(),
                name: "simple".into(),
                files,
                texts: vec![("metrics.ini".into(), String::new())],
            },
        ];
        let skin = Skin::load(&folders, "pump", "simple").unwrap().into_owned();
        let layout = Layout::pump_single();
        let mut t = 0.0;
        let lua = LuaSkin::new(
            skin,
            Env {
                game: "pump".into(),
                style: "single".into(),
                columns: 5,
                ..Env::default()
            },
            ddi_lua::Settings::default(),
            LoadBudget::default(),
            Box::new(move || {
                t += 1e-6;
                t
            }),
        )
        .unwrap();
        let mut f = std::pin::pin!(NoteskinPlay::with_lua(lua, &layout, &NoImages, false, None));
        let play = match f
            .as_mut()
            .poll(&mut std::task::Context::from_waker(std::task::Waker::noop()))
        {
            std::task::Poll::Ready(r) => r.unwrap().unwrap(),
            std::task::Poll::Pending => panic!("pending"),
        };
        // The textures the tap notes draw, and whether they are mirrored.
        let tap = |lane: &Lane| -> (String, f32) {
            // Its texture, and the turn about y down to it.
            fn first_sprite(a: &Actor) -> Option<(u32, f32)> {
                if let ddi_actor::Kind::Sprite(s) = &a.kind {
                    return Some((s.texture, a.base_rotation[1]));
                }
                let (t, r) = a.children().iter().find_map(first_sprite)?;
                Some((t, r + a.base_rotation[1]))
            }
            let a = actor(&lane.tap);
            let (texture, turned) = first_sprite(a).expect("a sprite");
            let path = match &play.textures.keys()[texture as usize] {
                TextureKey::Skin { path, .. } => path.clone(),
                other => panic!("{other:?}"),
            };
            (path, turned)
        };
        let got: Vec<(String, f32)> = play.lanes.iter().map(tap).collect();
        assert_eq!(
            got,
            [
                ("DownLeft Tap Note.png".into(), 0.0),
                ("UpLeft Tap Note.png".into(), 0.0),
                ("Center Tap Note.png".into(), 0.0),
                ("UpLeft Tap Note.png".into(), 180.0),
                ("DownLeft Tap Note.png".into(), 180.0),
            ]
        );
    }

    #[test]
    fn a_skins_lua_plays_the_engines_events() {
        let mut play = lua_play("return Def.ActorFrame { Def.Sprite { Texture = 'x.png' } }\n");
        assert!(play.runs_lua());
        let down = 1;
        let events = [
            SkinEvent {
                autoplay: false,
                danoni_judgments: false,
                note: None,
                seq: 0,
                lane: down,
                kind: SkinEventKind::Press,
                song_time: 0.0,
            },
            SkinEvent {
                autoplay: false,
                danoni_judgments: false,
                note: None,
                seq: 1,
                lane: down,
                kind: SkinEventKind::Tap {
                    judgement: Judgement::W2,
                    bright: true,
                },
                song_time: 0.0,
            },
        ];
        play.update(&frame(0.0, None), events.iter(), false);
        play.update(&frame(0.5, Some(HoldKind::Hold)), [].iter(), false);
        let lane = &play.lanes[usize::from(down)];
        assert_eq!(actor(&lane.receptor).current().pos[..2], [7.0, 1.0]);
        let ex = actor(&lane.explosion).current();
        assert_eq!(ex.pos[..2], [11.0, 2.0]);
        assert_eq!(ex.scale[0], 3.0);
        assert!(play.problems().is_empty(), "{:?}", play.problems());
        assert!(play.lua_fuel().1 > 0);
        // A retry starts from the trees as built; the Lua goes on.
        play.restart();
        play.update(&frame(0.0, None), [].iter(), false);
        assert_eq!(
            actor(&play.lanes[1].receptor).current().pos[..2],
            [0.0, 0.0]
        );
    }

    #[test]
    fn skins_get_the_games_judgement_colors() {
        // `JudgmentLineToColor` (crates/noteskin/src/lua/prelude.lua) keeps its own
        // table: it must match the receptor flashes'.
        let play = lua_play(
            "local t = Def.ActorFrame {}\n\
             for _, j in ipairs({ 'W1', 'W2', 'W3', 'W4', 'W5', 'Miss' }) do\n\
               t[#t + 1] = Def.Sprite {\n\
                 Texture = 'x.png';\n\
                 InitCommand = function(self) self:diffuse(JudgmentLineToColor('JudgmentLine_' .. j)) end;\n\
               }\n\
             end\n\
             return t\n",
        );
        let Element::Actor(tap) = &play.lanes[0].tap else {
            panic!("{:?}", play.notes)
        };
        let judgements = [
            Judgement::W1,
            Judgement::W2,
            Judgement::W3,
            Judgement::W4,
            Judgement::W5,
            Judgement::Miss,
        ];
        for (child, j) in tap.children().iter().zip(judgements) {
            assert_eq!(
                child.current().diffuse(),
                crate::scene::judgement_color(j),
                "{j:?}"
            );
        }
    }

    #[test]
    fn a_tap_whose_lua_reads_its_color_is_drawn_per_note_type() {
        let mut play = lua_play(
            "local x = ({ ['4th'] = 1, ['8th'] = 2 })[Var 'Color'] or 9\n\
             return Def.ActorFrame {\n\
               InitCommand = function(self) self:x(x) end;\n\
               Def.Sprite { Texture = 'x.png' };\n\
             }\n",
        );
        assert!(play.notes.is_empty(), "{:?}", play.notes);
        let lane = &mut play.lanes[0];
        assert_eq!(lane.colors.len(), 8);
        let x = |e: &mut Element| match e {
            Element::Actor(a) => a.current().pos[0],
            _ => panic!("{e:?}"),
        };
        assert_eq!(x(lane.note_slot(Slot::Tap, 0)), 1.0);
        assert_eq!(x(lane.note_slot(Slot::Tap, 1)), 2.0);
        assert_eq!(x(lane.note_slot(Slot::Tap, 8)), 9.0);
        // Heads are taps in dance/default: colored too, swapped in for a
        // hold of their type.
        lane.swap_hold_colors(1, false);
        assert_eq!(x(&mut lane.hold.head[1]), 2.0);
        lane.swap_hold_colors(1, false);
        assert_eq!(x(&mut lane.hold.head[1]), 1.0);
        // Every copy is lent to the Lua and updated.
        assert!(lane.elements_mut().count() > 26);
        // A skin that does not read it loads its notes once.
        let plain = lua_play("return Def.ActorFrame { Def.Sprite { Texture = 'x.png' } }\n");
        assert!(plain.lanes[0].colors.is_empty());
        // Sprites' textures have no mipmaps.
        assert!((0..plain.texture_keys().len()).all(|i| !plain.texture_mipmaps(i)));
    }

    /// A 64-pixel square facing the viewer (MilkShape's y up, counter-
    /// clockwise), textured with `x.png`.
    const SQUARE_MODEL: &str = "// MilkShape 3D ASCII\n\
        Frames: 30\nFrame: 1\n\
        Meshes: 1\n\"Square\" 0 0\n\
        4\n\
        0 -32 -32 0 0 1 -1\n\
        0 32 -32 0 1 1 -1\n\
        0 32 32 0 1 0 -1\n\
        0 -32 32 0 0 0 -1\n\
        1\n0 0 1\n\
        2\n0 0 1 2 0 0 0 1\n0 0 2 3 0 0 0 1\n\
        Materials: 1\n\"Mat\"\n\
        0 0 0 1\n1 1 1 1\n0 0 0 1\n0 0 0 1\n0\n1\n\
        \"x.png\"\n\"\"\n\
        Bones: 0\n";

    #[test]
    fn a_tap_note_model_is_drawn_as_textured_triangles_facing_the_viewer() {
        let tap = |turn: &str| {
            format!(
                // Not a tail call: its paths are relative to this file.
                "local m = Def.Model {{ Meshes = 'square.txt', Materials = 'square.txt', \
                 Bones = 'square.txt', InitCommand = function(self) self:{turn} end }}\n\
                 return m\n"
            )
        };
        let geo = FieldGeometry::new(1280.0, 720.0, 4.0, false);
        let layout = Layout::dance_single();
        let mut f = frame(0.0, None);
        f.notes.push(NoteSprite {
            note_index: 0,
            lane: 1,
            y: 2.0,
            kind: SpriteKind::Tap,
            quantization: ddi_chart::Quantization::N4,
            beat_frac: 0.0,
            color: None,
            alpha: 1.0,
            glow: 0.0,
            beat: 0.0,
            hold_life: 1.0,
        });
        let draw = |turn: &str| {
            let mut play = lua_play_with(&tap(turn), &[("square.txt", SQUARE_MODEL)]);
            assert!(play.notes.is_empty(), "{:?}", play.notes);
            // x.png is the model's (mipmapped, as StepMania's model textures
            // are) and the receptor's and explosion's sprites'.
            assert!((0..play.texture_keys().len()).any(|i| play.texture_mipmaps(i)));
            for i in 0..play.texture_keys().len() {
                play.set_texture(i, 7);
            }
            let mut out = Vec::new();
            play.notes(&f, &layout, &geo, &mut out);
            out
        };

        // Down's lane at x 604, the note two arrows (72 pixels) under its
        // receptor at y 108; 64 of the skin's pixels are 72.
        let out = draw("zoom(1)");
        assert_eq!(out.len(), 2);
        let mut seen = Vec::new();
        for q in &out {
            assert_eq!((q.texture, q.glow, q.wrap), (7, false, true));
            let Shape::Triangle { points, w, .. } = q.shape else {
                panic!("{q:?}");
            };
            assert_eq!(w, [1.0; 3]);
            seen.extend(points);
        }
        let range = |i: usize| {
            let v = seen.iter().map(|p| p[i]);
            (
                v.clone().fold(f32::MAX, f32::min),
                v.fold(f32::MIN, f32::max),
            )
        };
        assert_eq!(range(0), (568.0, 640.0));
        assert_eq!(range(1), (216.0, 288.0));

        // Turned away (the field resets a note's own rotation, not its
        // base one), its back is culled.
        assert!(draw("baserotationy(180)").is_empty());
        // Tilted, its far edge is narrower than its near one.
        let tilted = draw("baserotationx(60)");
        assert_eq!(tilted.len(), 2);
        let corners: Vec<[f32; 2]> = tilted
            .iter()
            .flat_map(|q| match q.shape {
                Shape::Triangle { points, .. } => points,
                _ => panic!(),
            })
            .collect();
        let width_at = |top: bool| {
            let y = corners.iter().map(|p| p[1]);
            let edge = if top {
                y.fold(f32::MAX, f32::min)
            } else {
                y.fold(f32::MIN, f32::max)
            };
            let xs: Vec<f32> = corners
                .iter()
                .filter(|p| (p[1] - edge).abs() < 0.01)
                .map(|p| p[0])
                .collect();
            xs.iter().fold(f32::MIN, |a, &b| a.max(b)) - xs.iter().fold(f32::MAX, |a, &b| a.min(b))
        };
        assert!(
            (width_at(true) - width_at(false)).abs() > 1.0,
            "{corners:?}"
        );
    }

    #[test]
    fn a_color_that_fails_to_load_is_drawn_as_the_4ths() {
        let mut play = lua_play(
            "if Var 'Color' == '12th' then error('no 12ths') end\n\
             local x = ({ ['4th'] = 1, ['8th'] = 2 })[Var 'Color'] or 9\n\
             return Def.ActorFrame {\n\
               InitCommand = function(self) self:x(x) end;\n\
               Def.Sprite { Texture = 'x.png' };\n\
             }\n",
        );
        let lane = &mut play.lanes[0];
        assert!(lane.colors[1].tap.is_none());
        let Element::Actor(a) = lane.note_slot(Slot::Tap, 2) else {
            panic!()
        };
        assert_eq!(a.current().pos[0], 1.0);
    }

    #[test]
    fn a_skins_lua_that_stops_leaves_the_built_in_skin() {
        let mut play = lua_play(
            "return Def.ActorFrame {\n\
               InitCommand = function(self) self:SetUpdateFunction(function() while true do end end) end;\n\
               Def.Sprite { Texture = 'x.png' };\n\
             }\n",
        );
        assert!(play.draws(1, LayerPart::Receptor));
        play.update(&frame(0.0, None), [].iter(), false);
        assert!(!play.runs_lua());
        assert!(
            play.lua_failure().is_some_and(|f| f.contains("fuel")),
            "{:?}",
            play.lua_failure()
        );
        for part in [LayerPart::Receptor, LayerPart::Tap] {
            assert!(!play.draws(1, part));
        }
        play.restart();
        assert!(!play.draws(1, LayerPart::Receptor));
        play.update(&frame(0.1, None), [].iter(), false);
    }
}
