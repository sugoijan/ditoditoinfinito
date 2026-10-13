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

use ddi_chart::Layout;
use ddi_engine::frame::{Frame, HoldKind, NoteSprite, SkinEvent, SkinEventKind, SpriteKind};
use ddi_engine::rules::Judgement;
use ddi_lua::Failure;
use ddi_noteskin::actor::{Actor, Blend, Clocks, Drawn, Kind, ModelDraw, Problems, Quad};
use ddi_noteskin::build::{Built, Images, TextureKey, Textures, build};
use ddi_noteskin::lua::{FrameBudget, LuaSkin, NOTE_COLORS, Param, Pause, Position};
use ddi_noteskin::metrics::{NoteDisplay, Part};
use ddi_noteskin::notes::{animation_percent, color_offset, note_type};
use ddi_noteskin::{Button, Skin};

use crate::color::to_target;
use crate::model::{Camera, triangles};
use crate::scene::FieldGeometry;
use crate::textured::{QuadBlend, Shape, TexQuad, UNTEXTURED};

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
}

/// One element of a lane.
#[derive(Clone, Debug)]
enum Element {
    Actor(Box<Actor>),
    BuiltIn,
    Blank,
}

impl Element {
    fn is_skin(&self) -> bool {
        !matches!(self, Element::BuiltIn)
    }
}

/// A hold's or roll's parts, inactive then active.
#[derive(Clone, Debug)]
struct HoldParts {
    head: [Element; 2],
    tail: [Element; 2],
    topcap: [Element; 2],
    body: [Element; 2],
    bottomcap: [Element; 2],
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

/// A lane's note elements in one note type's colour, for a skin whose
/// Lua reads `Var "Color"` (`None`: as the 4ths'). ITGmania loads taps,
/// lifts, fakes, mines and every hold and roll part per colour; a hold is
/// drawn in its head's.
#[derive(Clone, Debug, Default)]
struct ColorSet {
    tap: Option<Element>,
    lift: Option<Element>,
    fake: Option<Element>,
    mine: Option<Element>,
    /// Head, tail, top cap, body, bottom cap; inactive then active (as
    /// [`HoldParts::all`]).
    hold: [[Option<Element>; 2]; 5],
    roll: [[Option<Element>; 2]; 5],
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
    /// colour.
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
        let part = ["Head", "Tail", "Topcap", "Body", "Bottomcap"]
            .iter()
            .position(|p| Some(*p) == words.next())?;
        let active = match words.next()? {
            "Inactive" => 0,
            "Active" => 1,
            _ => return None,
        };
        Some(&mut parts[part][active])
    }

    fn get_mut(&mut self, slot: Slot) -> &mut Option<Element> {
        match slot {
            Slot::Tap => &mut self.tap,
            Slot::Lift => &mut self.lift,
            Slot::Fake => &mut self.fake,
            Slot::Mine => &mut self.mine,
        }
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
    /// The layout's scroll sign for the lane (-1: a lower-row lane).
    scroll_sign: i8,
    /// The note elements of 8ths to 192nds, when the skin's Lua colours
    /// notes by `Var "Color"` (empty otherwise: every note is as the 4ths).
    colors: Vec<ColorSet>,
}

/// A noteskin playing.
pub struct NoteskinPlay {
    pub name: String,
    lanes: Vec<Lane>,
    /// The lanes as built, for a retry (StepMania loads the actors again).
    initial: Vec<Lane>,
    /// Quads drawn in the last frame (debug).
    quads: std::cell::Cell<usize>,
    /// Models' triangles drawn this frame (at most [`MAX_FRAME_TRIANGLES`]).
    triangles: std::cell::Cell<usize>,
    /// The field's perspective this frame, for models.
    camera: Camera,
    textures: Textures,
    /// The renderer's id for each of [`NoteskinPlay::textures`].
    texture_ids: Vec<Option<u32>>,
    next_seq: u64,
    started: bool,
    last_song_time: Option<f64>,
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
    /// Fuel the skin's Lua spent in the last frame, and at most in one.
    lua_fuel: (u64, u64),
}

/// The most triangles a frame's models draw (a model has at most
/// [`ddi_noteskin::model::MAX_TRIANGLES`]; the corpus's, under 2,000).
const MAX_FRAME_TRIANGLES: usize = 1 << 18;

/// The elements of a lane, in the order they are built.
const LANE_ELEMENTS: [&str; 26] = [
    "Hold Head Inactive",
    "Hold Head Active",
    "Hold Tail Inactive",
    "Hold Tail Active",
    "Hold Topcap Inactive",
    "Hold Topcap Active",
    "Hold Body Inactive",
    "Hold Body Active",
    "Hold Bottomcap Inactive",
    "Hold Bottomcap Active",
    "Roll Head Inactive",
    "Roll Head Active",
    "Roll Tail Inactive",
    "Roll Tail Active",
    "Roll Topcap Inactive",
    "Roll Topcap Active",
    "Roll Body Inactive",
    "Roll Body Active",
    "Roll Bottomcap Inactive",
    "Roll Bottomcap Active",
    "Receptor",
    "Explosion",
    "Tap Note",
    "Tap Lift",
    "Tap Fake",
    "Tap Mine",
];

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
    }
}

impl Lane {
    /// A lane from its elements, built in [`LANE_ELEMENTS`]' order.
    fn assemble(elements: Vec<Element>, metrics: NoteDisplay, scroll_sign: i8) -> Lane {
        let mut e = elements.into_iter();
        let mut next = || e.next().unwrap_or(Element::BuiltIn);
        let mut pair = || [next(), next()];
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
    /// copies: before drawing a hold in its head's colour, and after.
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
        for (base, copy) in parts.all_mut().zip(copies.iter_mut().flatten()) {
            if let Some(copy) = copy {
                std::mem::swap(base, copy);
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
            let button = Button::from_lane(&lane.name)?;
            let elements = LANE_ELEMENTS
                .iter()
                .map(|name| {
                    let built = build(skin, button, name, &mut textures, images);
                    to_element(built, button, name, &textures, &mut notes)
                })
                .collect();
            let metrics = NoteDisplay::read(skin, button.name());
            lanes.push(Lane::assemble(elements, metrics, lane.scroll_sign));
        }
        Some(NoteskinPlay::from_lanes(
            skin.name.clone(),
            lanes,
            textures,
            notes,
            srgb,
            None,
        ))
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
        let Some(buttons) = layout
            .lanes
            .iter()
            .map(|l| Button::from_lane(&l.name))
            .collect::<Option<Vec<_>>>()
        else {
            return Ok(None);
        };
        let mut textures = Textures::default();
        let mut notes = Vec::new();
        let mut lanes = Vec::new();
        for (lane, button) in layout.lanes.iter().zip(buttons) {
            let mut elements = Vec::new();
            // The note elements whose Lua read their colour.
            let mut colored = Vec::new();
            for name in LANE_ELEMENTS {
                let (built, read) = lua
                    .element_colored_sliced(
                        button,
                        name,
                        NOTE_COLORS[0],
                        &mut textures,
                        images,
                        pause.as_deref_mut(),
                    )
                    .await?;
                if read && ColorSet::default().place(name).is_some() {
                    colored.push(name);
                }
                elements.push(to_element(built, button, name, &textures, &mut notes));
            }
            let metrics = NoteDisplay::read(&lua.skin(), button.name());
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
                                &mut textures,
                                images,
                                pause.as_deref_mut(),
                            )
                            .await?;
                        if let Some(place) = set.place(name) {
                            *place = Some(to_element(built, button, name, &textures, &mut notes));
                        }
                    }
                    assembled.colors.push(set);
                }
            }
            lanes.push(assembled);
        }
        // Every element is loaded: the play takes the trees it draws.
        for lane in &mut lanes {
            for e in lane.elements_mut() {
                if let Element::Actor(a) = e
                    && !lua.reclaim(a)
                {
                    *e = Element::BuiltIn;
                }
            }
            lane.settle_colors();
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

    fn from_lanes(
        name: String,
        lanes: Vec<Lane>,
        textures: Textures,
        notes: Vec<String>,
        srgb: bool,
        lua: Option<LuaSkin>,
    ) -> NoteskinPlay {
        let count = textures.keys().len();
        let mut play = NoteskinPlay {
            name,
            quads: std::cell::Cell::new(0),
            triangles: std::cell::Cell::new(0),
            camera: Camera {
                vanish: [0.0; 2],
                distance: 1.0,
            },
            initial: lanes.clone(),
            lanes,
            textures,
            texture_ids: vec![None; count],
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
            lua_fuel: (0, 0),
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

    /// Why the skin's Lua stopped during the play, if it did.
    pub fn lua_failure(&self) -> Option<&str> {
        self.lua_failure.as_ref().map(|(m, _)| m.as_str())
    }

    /// Whether the skin's Lua stopped on the clock alone (not its fuel,
    /// memory or a host limit): a slow or stalled machine, not the skin.
    pub fn lua_stopped_on_clock(&self) -> bool {
        self.lua_failure.as_ref().is_some_and(|(_, clock)| *clock)
    }

    /// Fuel the skin's Lua spent in the last frame, and the most in one.
    pub fn lua_fuel(&self) -> (u64, u64) {
        self.lua_fuel
    }

    /// The textures the actors draw, by index.
    pub fn texture_keys(&self) -> &[TextureKey] {
        self.textures.keys()
    }

    /// Whether texture `index` gets mipmaps (a model's).
    pub fn texture_mipmaps(&self, index: usize) -> bool {
        self.textures.mipmaps(index)
    }

    /// Tells which renderer texture stands for texture `index`.
    pub fn set_texture(&mut self, index: usize, id: u32) {
        if let Some(slot) = self.texture_ids.get_mut(index) {
            *slot = Some(id);
        }
    }

    /// Whether the target is sRGB (colours are converted for it).
    pub fn set_srgb(&mut self, srgb: bool) {
        self.srgb = srgb;
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
            ..
        } = self;
        let mut cmd = Commands {
            lua: lua.as_mut(),
            failed: None,
        };
        if !*started {
            *started = true;
            for lane in lanes.iter_mut() {
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
                               names: &[&str],
                               cmd: &mut Commands,
                               p: &mut Problems| {
                let mut params = vec![
                    (score.0, Param::Str(score.1)),
                    ("Column", Param::Int(column)),
                ];
                if b {
                    params.push(("Bright", Param::Bool(true)));
                }
                cmd.play(&mut lane.explosion, "ColumnJudgmentMessage", &params, p);
                for n in names {
                    cmd.play(&mut lane.explosion, n, &[], p);
                }
            };
            let tap = |s: &str| ("TapNoteScore", format!("TapNoteScore_{s}"));
            match e.kind {
                SkinEventKind::Press => cmd.play(&mut lane.receptor, "Press", &[], p),
                SkinEventKind::Lift => cmd.play(&mut lane.receptor, "Lift", &[], p),
                SkinEventKind::Step(Some(j)) => cmd.play(&mut lane.receptor, score_name(j), &[], p),
                SkinEventKind::Step(None) => cmd.play(&mut lane.receptor, "None", &[], p),
                SkinEventKind::StepMine => cmd.play(&mut lane.receptor, "HitMine", &[], p),
                SkinEventKind::Tap {
                    judgement,
                    bright: b,
                } => explode(
                    tap(score_name(judgement)),
                    b,
                    &["Judgment", bright(b), score_name(judgement)],
                    &mut cmd,
                    p,
                ),
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
                lane.holding = now;
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

    /// Appends `actor`'s quads at `transform`, coloured, with `offset` added
    /// to its texture coordinates (a note's colour).
    fn emit(
        &self,
        actor: &mut Actor,
        transform: [f32; 6],
        offset: [f32; 2],
        out: &mut Vec<TexQuad>,
    ) {
        let mut drawn = Vec::new();
        actor.draw(transform, [1.0; 4], [1.0, 1.0, 1.0, 0.0], &mut drawn);
        for d in drawn {
            match d {
                Drawn::Quad(q) => self.push(q, offset, out),
                Drawn::Model(m) => self.push_model(&m, offset, out),
            }
        }
    }

    /// A model's triangles (`Model::DrawPrimitives`): every mesh's diffuse
    /// pass, then every mesh's glow pass.
    fn push_model(&self, m: &ModelDraw, offset: [f32; 2], out: &mut Vec<TexQuad>) {
        self.quads.set(self.quads.get() + 1);
        if m.diffuse[3] < 0.001 && m.glow[3] < 0.001 {
            return;
        }
        let mut seen = Vec::new();
        triangles(m, &self.camera, offset, &mut seen);
        // A frame's budget: models past it are not drawn.
        let passes = 1 + usize::from(m.glow[3] > 0.0001);
        let total = self.triangles.get() + seen.len() * passes;
        if total > MAX_FRAME_TRIANGLES {
            return;
        }
        self.triangles.set(total);
        // `DrawPrimitives` sets the blend back to normal after each mesh:
        // the actor's blends the first mesh, and the glow pass only when
        // no diffuse pass came before it.
        let blend = quad_blend(m.blend);
        let diffuse = m.diffuse[3] > 0.0;
        let mesh_blend = |mesh: usize| {
            if mesh == 0 { blend } else { QuadBlend::Normal }
        };
        let glow_blend = if diffuse { QuadBlend::Normal } else { blend };
        // A mesh's texture id (`UNTEXTURED`: its colour alone; `None`: not
        // loaded) and colour; a mesh without a material is grey
        // (`DrawPrimitives`' fallback material, unlit).
        let meshes: Vec<(Option<u32>, [f32; 4])> = m
            .geometry
            .meshes
            .iter()
            .map(
                |mesh| match mesh.material.and_then(|i| m.materials.get(i)) {
                    Some(&(Some(t), color, _)) => {
                        (self.texture_ids.get(t as usize).copied().flatten(), color)
                    }
                    Some(&(None, color, _)) => (Some(UNTEXTURED), color),
                    None => (Some(UNTEXTURED), [0.9, 0.9, 0.9, 1.0]),
                },
            )
            .collect();
        let triangle =
            |s: &crate::model::Seen, texture: u32, color: [f32; 4], glow: bool, blend| TexQuad {
                shape: Shape::Triangle {
                    points: s.points,
                    w: s.w,
                    uv: s.uv,
                },
                color: to_target(color, self.srgb),
                glow,
                texture,
                blend,
                wrap: true,
            };
        if diffuse {
            for s in &seen {
                if let (Some(texture), color) = meshes[s.mesh] {
                    out.push(triangle(s, texture, color, false, mesh_blend(s.mesh)));
                }
            }
        }
        if m.glow[3] > 0.0001 {
            for s in &seen {
                if let (Some(texture), _) = meshes[s.mesh] {
                    out.push(triangle(s, texture, m.glow, true, glow_blend));
                }
            }
        }
    }

    fn push(&self, q: Quad, offset: [f32; 2], out: &mut Vec<TexQuad>) {
        self.quads.set(self.quads.get() + 1);
        let Some(Some(texture)) = self.texture_ids.get(q.texture as usize) else {
            return;
        };
        let o = [
            offset[0] + q.texcoord_offset[0],
            offset[1] + q.texcoord_offset[1],
        ];
        let uv = [
            q.uv[0] + o[0],
            q.uv[1] + o[1],
            q.uv[2] + o[0],
            q.uv[3] + o[1],
        ];
        let base = TexQuad {
            shape: Shape::Quad {
                transform: q.transform,
                uv,
            },
            color: to_target(q.diffuse, self.srgb),
            glow: false,
            texture: *texture,
            blend: quad_blend(q.blend),
            wrap: false,
        };
        if q.diffuse[3] > 0.0 {
            out.push(base);
        }
        if q.glow[3] > 0.0 {
            out.push(TexQuad {
                color: to_target(q.glow, self.srgb),
                glow: true,
                ..base
            });
        }
    }

    /// A note part drawn as StepMania's `DrawActor` does: placed by the
    /// field, its frame from the beat, its colour by texture translation,
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
        let percent = || {
            animation_percent(
                m,
                metrics.animation_is_beat_based,
                frame.beat as f32,
                frame.song_time as f32,
                beat,
            )
        };
        match &mut actor.kind {
            Kind::Sprite(s) => {
                let length: f32 = s.states.iter().map(|st| st.delay).sum();
                s.set_seconds_into_animation(percent() * length);
                padded = s.padded_to_image;
            }
            // Its textures' scroll and frames; a model's texture is
            // stretched to its image (no padding to take out).
            Kind::Model(model) => model.seconds = percent() * model.animation_length(),
            _ => {}
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

    /// Every element, the colours' too.
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
            let (tail_y, active, roll) = match note.kind {
                SpriteKind::HoldHead { tail_y, active, .. } => (tail_y, active, false),
                SpriteKind::RollHead { tail_y, active, .. } => (tail_y, active, true),
                _ => continue,
            };
            let i = usize::from(note.lane);
            let (Some(l), Some(lane)) = (layout.lanes.get(i), lanes.get_mut(i)) else {
                continue;
            };
            // The hold in its head's colour (the copies swapped back below).
            let note_type = note_type(note.beat);
            lane.swap_hold_colors(note_type, roll);
            let m = lane.metrics.clone();
            // The Active sprites while the button is held (always inside a
            // roll; `Player.cpp:1381-1386`, `1414-1424`).
            let pressed = frame.receptors.get(i).is_some_and(|r| r.pressed);
            let held = active && (roll || pressed);
            let x = geo.lane_x(l.column);
            let head_y = geo.lane_y(l.scroll_sign, note.y);
            let tail_px = geo.lane_y(l.scroll_sign, tail_y);
            // The column's reverse; StepMania's "head" is the end higher on
            // screen.
            let reverse = geo.lane_direction(l.scroll_sign) < 0.0;
            let color_scale = lerp(
                m.hold_let_go_gray_percent,
                1.0,
                note.hold_life.clamp(0.0, 1.0),
            );
            let parts = if roll { &mut lane.roll } else { &mut lane.hold };
            if parts.drawn() {
                let layers: &[(usize, QuadBlend)] = if held && m.hold_active_is_add_layer {
                    &[(0, QuadBlend::Normal), (1, QuadBlend::Add)]
                } else if held {
                    &[(1, QuadBlend::Normal)]
                } else {
                    &[(0, QuadBlend::Normal)]
                };
                for &(a, blend) in layers {
                    let spec = HoldDraw {
                        x,
                        head_y,
                        tail_px,
                        reverse,
                        k,
                        color_scale,
                        blend,
                        spans: frame.appearance.visible_spans(note.y, tail_y).collect(),
                        lane_sign: l.scroll_sign,
                    };
                    self.hold_body(parts, a, &m, note, frame, geo, &spec, out);
                }
            }
            let a = usize::from(held);
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
                    &m,
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
                    &m,
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
            let m = lane.metrics.clone();
            self.note_actor(
                &mut e,
                &m,
                part,
                note.beat,
                (note.alpha, note.glow),
                frame,
                at,
                k,
                1.0,
                out,
            );
            *lane.note_slot(slot, note_type) = e;
        }
        self.lanes = lanes;
    }
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
    /// The parts Hidden/Sudden leave visible, in arrow heights.
    spans: Vec<(f32, f32)>,
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
            let (Some(l), Element::Actor(a)) = (layout.lanes.get(i), pick(lane)) else {
                continue;
            };
            let at = [geo.lane_x(l.column), geo.lane_receptor_y(l.scroll_sign)];
            self.emit(a, [k, 0.0, 0.0, k, at[0], at[1]], [0.0; 2], out);
        }
        self.lanes = lanes;
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
                let percent = animation_percent(
                    pm,
                    m.animation_is_beat_based,
                    frame.beat as f32,
                    frame.song_time as f32,
                    note.beat,
                );
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
        let visible: Vec<(f32, f32)> = d
            .spans
            .iter()
            .map(|&(lo, hi)| {
                let (a0, a1) = (geo.lane_y(d.lane_sign, lo), geo.lane_y(d.lane_sign, hi));
                (a0.min(a1), a0.max(a1))
            })
            .collect();
        // A cap shows when the end of the body it is attached to does.
        let shown_at = |y: f32| {
            visible
                .iter()
                .any(|&(v0, v1)| v0 <= y + 0.5 && v1 >= y - 0.5)
        };
        let c = d.color_scale;
        let color = to_target([c, c, c, 1.0], self.srgb);
        let quad = |texture: u32, y0: f32, y1: f32, width: f32, uv: [f32; 4], wrap: bool| TexQuad {
            shape: Shape::Quad {
                transform: [width, 0.0, 0.0, y1 - y0, d.x, (y0 + y1) / 2.0],
                uv,
            },
            color,
            glow: false,
            texture,
            blend: d.blend,
            wrap,
        };
        let id = |t: u32| self.texture_ids.get(t as usize).copied().flatten();
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
            for &(s0, s1) in &visible {
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

fn quad_blend(b: Blend) -> QuadBlend {
    match b {
        Blend::Add => QuadBlend::Add,
        Blend::Subtract => QuadBlend::Subtract,
        Blend::Modulate => QuadBlend::Modulate,
        Blend::Normal | Blend::Other => QuadBlend::Normal,
    }
}

fn lerp(a: f32, b: f32, t: f32) -> f32 {
    a + (b - a) * t
}

/// What the colour-scale and pixel maths of a hold look like, for tests.
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scores_have_stepmanias_names() {
        assert_eq!(score_name(Judgement::W1), "W1");
        assert_eq!(score_name(Judgement::Miss), "Miss");
        assert_eq!(lerp(0.25, 1.0, 0.0), 0.25);
    }

    use ddi_engine::appearance::Appearance;
    use ddi_engine::frame::ReceptorState;
    use ddi_noteskin::Folder;
    use ddi_noteskin::lua::{Env, LoadBudget};

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
            combo: 0,
            score: Default::default(),
            life: 0.5,
            danger: false,
            failed: false,
            finished: false,
            visible_range: 12.0,
            appearance: Appearance::Visible,
        }
    }

    /// A skin's Lua through a play: `tap` is its `Down Tap Note.lua`.
    fn lua_play(tap: &str) -> NoteskinPlay {
        lua_play_with(tap, &[])
    }

    /// [`lua_play`] with more text files.
    fn lua_play_with(tap: &str, more: &[(&str, &str)]) -> NoteskinPlay {
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
        let layout = Layout::dance_single();
        let mut f = std::pin::pin!(NoteskinPlay::with_lua(lua, &layout, &NoImages, false, None));
        match f
            .as_mut()
            .poll(&mut std::task::Context::from_waker(std::task::Waker::noop()))
        {
            std::task::Poll::Ready(r) => r.unwrap().unwrap(),
            std::task::Poll::Pending => panic!("pending"),
        }
    }

    fn actor(e: &Element) -> &Actor {
        match e {
            Element::Actor(a) => a,
            other => panic!("{other:?}"),
        }
    }

    use std::future::Future;

    #[test]
    fn a_skins_lua_plays_the_engines_events() {
        let mut play = lua_play("return Def.ActorFrame { Def.Sprite { Texture = 'x.png' } }\n");
        assert!(play.runs_lua());
        let down = 1;
        let events = [
            SkinEvent {
                seq: 0,
                lane: down,
                kind: SkinEventKind::Press,
                song_time: 0.0,
            },
            SkinEvent {
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
    fn skins_get_the_games_judgement_colours() {
        // `JudgmentLineToColor` (noteskin/src/lua/prelude.lua) keeps its own
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
                child.current().diffuse,
                crate::scene::judgement_color(j),
                "{j:?}"
            );
        }
    }

    #[test]
    fn a_tap_whose_lua_reads_its_colour_is_drawn_per_note_type() {
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
        // Heads are taps in dance/default: coloured too, swapped in for a
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
            assert!(play.texture_mipmaps(0) || play.texture_mipmaps(1));
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
    fn a_colour_that_fails_to_load_is_drawn_as_the_4ths() {
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
