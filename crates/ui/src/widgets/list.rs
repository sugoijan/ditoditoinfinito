//! A virtual list: as many items as a library has (ten thousand songs
//! under their packs' headers), but actors only for those that show and a
//! margin around them, reused as the list moves, as StepMania's music
//! wheel keeps a fixed set of wheel items and refills them
//! (`WheelBase::RebuildWheelItems`). So the actors, the text runs and the
//! images asked for stay bounded whatever the list's length.
//!
//! - **Items** come from a [`ListModel`]: rows and section headers, each
//!   a fixed height of its own ([`ListConfig`]); the model fills a
//!   component's parts (`Title`, `Subtitle`, `Banner`) when an actor is
//!   given an item, and may give items stable keys so the selection
//!   follows its item when the model changes ([`List::reload`]).
//! - **Scrolling**: a drag moves it, a flick carries on with inertia that
//!   slows by [`FRICTION`], a wheel's notches and the keys glide to a
//!   target (a notch is a row in the wheel style); once still, it snaps
//!   to the nearest item's top (plain) or puts the nearest item under the
//!   focus line and selects it (wheel). A trackpad's stream
//!   ([`crate::gesture::WheelKind::Precise`]) moves it one for one, as a
//!   drag, and once it ends (no event for [`STREAM_END`]) the list snaps
//!   as after a drag. Dragged or scrolled past an end, it gives less and
//!   less ([`RUBBER`]) and springs back when let go. A press catches a
//!   moving list (the tap that does is no selection). A scrollbar's
//!   thumb can be dragged. Positions are kept in `f64`, so a glide ends
//!   exactly however long the list.
//! - **Selection**: Up and Down (keys, pads, repeated when held) move it,
//!   the list following; a tap selects, a tap on the selected item (or
//!   Start) activates it.
//! - **Images** (banners) are hidden while the list moves faster than
//!   [`FAST`], so a flick through ten thousand rows asks for no image it
//!   passes; they show (and load) once it slows.
//! - **Wheel style**: the selected item sits at a fixed line and the
//!   others are placed around it by the look ([`crate::look::wheel`]).
//! - **Motion blur**: moving faster than the look's
//!   `List.MotionBlurFrom` ([`BLUR_FROM`]), each item is smeared along
//!   the list (`motionblur`, our actor extension) by `List.MotionBlur`
//!   ([`BLUR`]) of a 60th of a second's travel past that speed, at most a
//!   row; none at rest, none when the list does not glide (reduced
//!   motion).
//!
//! Actors: a frame named [`ListConfig::name`], clipped to the list's
//! rectangle, holding `<name>.Items` (the reused `<component>.Row` and
//! `<component>.Header` components, falling back to `List.Row` and
//! `List.Header`, named `<name>#<slot>`) and `<name>.Bar`. Pointers are
//! taken through the list frame's transform, so it works wherever it is
//! placed.

use ddi_actor::{Actor, Kind};

use crate::gesture::{Gesture, WheelKind};
use crate::hit;
use crate::look::{self, Look, Spec, call, named};
use crate::nav::MenuAction;
use crate::screen::{Cx, Handled, Input};

/// How fast a flick slows: velocity × e^(−FRICTION·t).
pub const FRICTION: f64 = 3.5;
/// Below this (virtual pixels a second) a flick stops and the list snaps.
pub const STOP_SPEED: f64 = 40.0;
/// How fast the list glides to a target (wheel, keys, snap): the gap
/// shrinks by e^(−GLIDE·t).
pub const GLIDE: f64 = 14.0;
/// Faster than this (virtual pixels a second: a row of 48 on screen for
/// a third of a second under a list 500 high), rows show no images.
pub const FAST: f64 = 1500.0;
/// A trackpad's stream has ended once no event came for this long
/// (seconds): the fingers rest or lifted and the platform's momentum ran
/// out ([`crate::gesture::STREAM_GAP`]).
pub const STREAM_END: f64 = 0.12;
/// A stream of at most this many events, moving less than half a row,
/// was a wheel's notch reported as a few pixels (no trackpad gesture is
/// so short): it moves a notch when it ends.
pub const SHORT_STREAM: u32 = 2;
/// A notch's worth of a wheel, virtual pixels (Firefox's 3 lines,
/// Chrome's 100 to 120 pixels): a row each in the wheel style.
pub const NOTCH: f64 = 120.0;
/// Past an end, the list shows `d·c·x / (c·x + d)` of `x` pulled, `d`
/// half its height (iOS's rubber band, `c` 0.55): about half at first,
/// never more than `d`.
pub const RUBBER: f64 = 0.55;
/// The default share of a 60th of a second's travel a moving item is
/// smeared over (`List.MotionBlur`).
pub const BLUR: f32 = 0.5;
/// The default speed motion blur starts at, virtual pixels a second
/// (`List.MotionBlurFrom`): a slow drag or a glide's end stays sharp.
pub const BLUR_FROM: f32 = 600.0;
/// The measured speed's smoothing (seconds), over frames whose moves
/// come in uneven steps (a pointer's or a trackpad's events).
const MOTION_SMOOTH: f64 = 0.04;
/// Below this measured speed the list counts as still (virtual pixels a
/// second).
const MOTION_FLOOR: f64 = 20.0;

/// What an item is.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ItemKind {
    Header,
    Row,
}

/// The items.
pub trait ListModel {
    fn len(&self) -> usize;
    fn is_empty(&self) -> bool {
        self.len() == 0
    }
    fn kind(&self, i: usize) -> ItemKind;
    /// Fills item `i`'s component (`crate::widgets::set_text`,
    /// `set_image` with an image from [`Cx::texture`]).
    fn fill(&self, i: usize, item: &mut Actor, cx: &mut Cx);
    /// A key of item `i` that stays when the items change (a song's id),
    /// so the selection follows it; `None` keeps the selection's index.
    fn key(&self, _i: usize) -> Option<u64> {
        None
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum ListStyle {
    /// Items one under the other.
    Plain,
    /// The selected item at `focus` (a fraction of the height), the
    /// others placed by the look.
    Wheel { focus: f32 },
}

#[derive(Clone, Debug, PartialEq)]
pub struct ListConfig {
    pub name: String,
    /// The look's components: `<component>.Row`, `.Header`, `.Bar`
    /// (falling back to `List.…`).
    pub component: String,
    /// `[x, y, width, height]` in the parent's space.
    pub rect: [f32; 4],
    pub row: f32,
    pub header: f32,
    pub style: ListStyle,
    /// Built beyond the visible part, above and below.
    pub margin: f32,
    pub bar: f32,
    /// Glides to its targets (wheel, keys, snaps); off (reduced motion),
    /// it jumps there, and its items never blur.
    pub glide: bool,
    /// Let the look blend row geometry around the visual wheel focus.
    pub continuous_selection: bool,
}

impl ListConfig {
    pub fn new(name: &str, rect: [f32; 4]) -> ListConfig {
        ListConfig {
            name: name.into(),
            component: "List".into(),
            rect,
            row: 48.0,
            header: 30.0,
            style: ListStyle::Plain,
            margin: 96.0,
            bar: 8.0,
            glide: true,
            continuous_selection: false,
        }
    }
}

/// What happened to the list.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ListEvent {
    Selected(usize),
    Activated(usize),
}

#[derive(Clone, Debug)]
struct Slot {
    kind: ItemKind,
    item: Option<usize>,
    selected: Option<bool>,
    /// The model gave the row an image (its `Banner` sprite has one).
    image: bool,
}

/// A trackpad's stream of wheel events, while it lasts.
#[derive(Clone, Copy, Debug)]
struct Stream {
    /// Seconds since its last event.
    idle: f64,
    events: u32,
    moved: f64,
    /// Where the list was going when it began.
    from: f64,
}

/// The list's state.
pub struct List {
    pub config: ListConfig,
    offsets: Vec<f64>,
    kinds: Vec<ItemKind>,
    pos: f64,
    target: Option<f64>,
    velocity: f64,
    dragging: bool,
    stream: Option<Stream>,
    /// Where the last update left it, and how fast it has moved since
    /// (smoothed; drags and streams have no velocity of their own).
    last_pos: f64,
    motion: f64,
    /// The blur last given to the items (virtual pixels).
    blur: f32,
    /// The press caught a moving list: its tap only stops it.
    caught: bool,
    /// Dragging the scrollbar's thumb: where it was grabbed (from its
    /// top).
    thumb: Option<f64>,
    selected: usize,
    selected_key: Option<u64>,
    slots: Vec<Slot>,
    /// Whether images were hidden for speed at the last update.
    fast: bool,
    /// Items filled since the start, for tests.
    pub fills: usize,
}

impl List {
    pub fn new(config: ListConfig, model: &dyn ListModel) -> List {
        let mut list = List {
            config,
            offsets: Vec::new(),
            kinds: Vec::new(),
            pos: 0.0,
            target: None,
            velocity: 0.0,
            dragging: false,
            stream: None,
            last_pos: 0.0,
            motion: 0.0,
            blur: 0.0,
            caught: false,
            thumb: None,
            selected: 0,
            selected_key: None,
            slots: Vec::new(),
            fast: false,
            fills: 0,
        };
        list.reload(model);
        list.selected_key = (!model.is_empty()).then(|| model.key(0)).flatten();
        list
    }

    /// The model changed (a pack opened or closed, a search): the items
    /// are laid out again and every actor refilled. The selection stays
    /// on its item when the model gives keys (else on its index, within
    /// the new end); the scroll and any glide stay within the new ends;
    /// a wheel puts the selection back on its line. `Selected` when the
    /// selected index changed.
    pub fn reload(&mut self, model: &dyn ListModel) -> Option<ListEvent> {
        let n = model.len();
        self.kinds = (0..n).map(|i| model.kind(i)).collect();
        self.offsets = Vec::with_capacity(n + 1);
        let mut y = 0.0;
        for k in &self.kinds {
            self.offsets.push(y);
            y += self.height(*k);
        }
        self.offsets.push(y);
        for s in &mut self.slots {
            s.item = None;
            s.selected = None;
        }
        let before = self.selected;
        let by_key = self
            .selected_key
            .and_then(|k| (0..n).find(|&i| model.key(i) == Some(k)));
        self.selected = by_key.unwrap_or(self.selected).min(n.saturating_sub(1));
        self.selected_key = (n > 0).then(|| model.key(self.selected)).flatten();
        self.pos = self.clamp(self.pos);
        self.target = self.target.map(|t| self.clamp(t));
        if let Some(f) = self.focus_line()
            && n > 0
        {
            self.pos = self.clamp(self.centre(self.selected) - f);
            self.target = None;
            self.velocity = 0.0;
            self.stream = None;
        }
        // Laid out again is no movement.
        self.last_pos = self.pos;
        let moved = n > 0 && self.selected != before;
        if moved && self.focus_line().is_none() {
            // Keep it in view where it went.
            let key = self.selected_key;
            self.select(self.selected);
            self.selected_key = key;
        }
        moved.then_some(ListEvent::Selected(self.selected))
    }

    fn height(&self, k: ItemKind) -> f64 {
        f64::from(match k {
            ItemKind::Header => self.config.header,
            ItemKind::Row => self.config.row,
        })
    }

    pub fn len(&self) -> usize {
        self.kinds.len()
    }

    pub fn is_empty(&self) -> bool {
        self.kinds.is_empty()
    }

    /// The selected item; `None` in an empty list.
    pub fn selected(&self) -> Option<usize> {
        (!self.kinds.is_empty()).then_some(self.selected)
    }

    /// How far from the top the list is scrolled (virtual pixels).
    pub fn scroll(&self) -> f64 {
        self.pos
    }

    /// Item actors built so far (the pool): bounded by what shows.
    pub fn pool(&self) -> usize {
        self.slots.len()
    }

    /// Items with an actor now.
    pub fn shown(&self) -> Vec<usize> {
        let mut v: Vec<usize> = self.slots.iter().filter_map(|s| s.item).collect();
        v.sort_unstable();
        v
    }

    /// Whether it moves by itself (inertia, a glide), a trackpad's stream
    /// is open, or it has just moved (its blur still to clear): frames
    /// needed.
    pub fn animating(&self) -> bool {
        self.velocity != 0.0 || self.target.is_some() || self.stream.is_some() || self.motion > 0.0
    }

    /// How fast it moves now, virtual pixels a second: its inertia or
    /// glide, else as measured over the last frames (a drag, a stream).
    pub fn speed(&self) -> f64 {
        if self.velocity != 0.0 {
            self.velocity.abs()
        } else if let Some(t) = self.target {
            (t - self.pos).abs() * GLIDE
        } else {
            self.motion
        }
    }

    /// The blur given to its items now (virtual pixels along the list).
    pub fn blur(&self) -> f32 {
        self.blur
    }

    fn content(&self) -> f64 {
        *self.offsets.last().unwrap_or(&0.0)
    }

    fn height_shown(&self) -> f64 {
        f64::from(self.config.rect[3])
    }

    /// The wheel's focus line, from the top.
    fn focus_line(&self) -> Option<f64> {
        match self.config.style {
            ListStyle::Plain => None,
            ListStyle::Wheel { focus } => Some(self.height_shown() * f64::from(focus)),
        }
    }

    fn min_scroll(&self) -> f64 {
        match self.focus_line() {
            // The first item's centre can reach the line.
            Some(f) => self.kinds.first().map_or(0.0, |k| self.height(*k) / 2.0) - f,
            None => 0.0,
        }
    }

    fn max_scroll(&self) -> f64 {
        match self.focus_line() {
            Some(f) => {
                let n = self.kinds.len();
                if n == 0 {
                    return self.min_scroll();
                }
                self.centre(n - 1) - f
            }
            None => (self.content() - self.height_shown()).max(0.0),
        }
    }

    fn clamp(&self, pos: f64) -> f64 {
        let min = self.min_scroll();
        pos.clamp(min, self.max_scroll().max(min))
    }

    /// `pos` moved by `delta` as a finger moves it: one for one within the
    /// ends, past them less and less ([`RUBBER`]).
    fn rubber(&self, pos: f64, delta: f64) -> f64 {
        let (min, max) = (self.min_scroll(), self.max_scroll().max(self.min_scroll()));
        let d = (self.height_shown() / 2.0).max(1.0);
        // Shown past the end for `x` pulled, and back.
        let shown = |x: f64| d * RUBBER * x / (RUBBER * x + d);
        let pulled = |o: f64| o * d / (RUBBER * (d - o.min(d * 0.999)));
        let finger = if pos < min {
            min - pulled(min - pos)
        } else if pos > max {
            max + pulled(pos - max)
        } else {
            pos
        } + delta;
        if finger < min {
            min - shown(min - finger)
        } else if finger > max {
            max + shown(finger - max)
        } else {
            finger
        }
    }

    fn centre(&self, i: usize) -> f64 {
        (self.offsets[i] + self.offsets[i + 1]) / 2.0
    }

    /// The item at `y` from the content's top.
    fn item_at(&self, y: f64) -> usize {
        let n = self.kinds.len();
        if n == 0 {
            return 0;
        }
        self.offsets.partition_point(|&o| o <= y).clamp(1, n) - 1
    }

    fn set_selected(&mut self, i: usize, model_key: Option<u64>) {
        self.selected = i;
        self.selected_key = model_key;
    }

    /// Selects item `i` and brings it into view.
    pub fn select(&mut self, i: usize) {
        if self.kinds.is_empty() {
            return;
        }
        self.selected = i.min(self.kinds.len() - 1);
        self.selected_key = None;
        self.velocity = 0.0;
        // Keys and taps end a trackpad's stream: its end would snap back.
        self.stream = None;
        let (top, bottom) = (self.offsets[self.selected], self.offsets[self.selected + 1]);
        let to = match self.focus_line() {
            Some(f) => self.centre(self.selected) - f,
            None => {
                let pad = f64::from(self.config.row) * 0.5;
                // Judged where the list is going, within its ends; landing
                // on an item's top, as a snap does.
                let at = self.clamp(self.target.unwrap_or(self.pos));
                if top < at + pad {
                    self.offsets[self.item_at((top - pad).max(0.0))]
                } else if bottom > at + self.height_shown() - pad {
                    let i = self.item_at(bottom - self.height_shown() + pad);
                    self.offsets[(i + 1).min(self.kinds.len())]
                } else {
                    self.target = (self.target.is_some()).then_some(at);
                    return;
                }
            }
        };
        self.target = Some(self.clamp(to));
    }

    /// The name of the actor showing item `i`, if one does.
    pub fn item_actor(&self, i: usize) -> Option<String> {
        let k = self.slots.iter().position(|s| s.item == Some(i))?;
        Some(format!("{}#{k}", self.config.name))
    }

    /// The list's actors were built again ([`List::build`] into a new
    /// tree): rows are made afresh at the next update.
    pub fn reset_actors(&mut self) {
        self.slots.clear();
    }

    /// Ends any glide at once, where it was going (a list laid out
    /// again, a restored place).
    pub fn finish(&mut self) {
        if let Some(t) = self.target.take() {
            self.pos = t;
        }
        self.velocity = 0.0;
        self.last_pos = self.pos;
    }

    /// Target scroll for selecting an item, without changing selection.
    pub fn selection_scroll(&self, i: usize) -> f64 {
        if self.is_empty() {
            return self.pos;
        }
        let i = i.min(self.len() - 1);
        if let Some(f) = self.focus_line() {
            return self.clamp(self.centre(i) - f);
        }
        let pad = f64::from(self.config.row) * 0.5;
        let at = self.clamp(self.target.unwrap_or(self.pos));
        if self.offsets[i] < at + pad {
            self.clamp(self.offsets[self.item_at((self.offsets[i] - pad).max(0.0))])
        } else if self.offsets[i + 1] > at + self.height_shown() - pad {
            let j = self.item_at(self.offsets[i + 1] - self.height_shown() + pad);
            self.clamp(self.offsets[(j + 1).min(self.len())])
        } else {
            at
        }
    }

    /// Commits selection while a screen owns the scrolling trajectory.
    pub fn select_without_motion(&mut self, i: usize, model: &dyn ListModel) {
        self.select(i);
        self.selected_key = model.key(self.selected);
        self.target = None;
    }

    /// Applies a screen-owned visual position; never settles selection.
    pub fn visual_scroll(&mut self, pos: f64) {
        self.pos = self.clamp(pos);
        self.target = None;
        self.velocity = 0.0;
        self.stream = None;
    }

    /// Scrolls to `pos` at once (tests, a restored place).
    pub fn jump(&mut self, pos: f64) {
        self.pos = self.clamp(pos);
        self.target = None;
        self.velocity = 0.0;
        self.stream = None;
        self.last_pos = self.pos;
    }

    /// Where item `i`'s top shows now, from the list's top (virtual
    /// pixels; in a wheel, before the look places it).
    pub fn item_top(&self, i: usize) -> Option<f64> {
        (i < self.len()).then(|| self.offsets[i] - self.pos)
    }

    /// Scrolls at once so that item `i`'s top shows at `top` (from
    /// [`List::item_top`] before the items changed): what the list showed
    /// stays in its place when the items around it change (a pack opened
    /// above or below it), and a glide to the selection
    /// ([`List::select`]) goes on from there.
    pub fn keep_item_at(&mut self, i: usize, top: f64) {
        if i >= self.len() {
            return;
        }
        self.pos = self.clamp(self.offsets[i] - top);
        self.target = None;
        self.velocity = 0.0;
        self.stream = None;
        self.last_pos = self.pos;
    }

    /// The list's actors, to put in the screen's tree.
    pub fn build(&self, look: &dyn Look) -> Actor {
        let [x, y, w, h] = self.config.rect;
        let name = &self.config.name;
        let items = named(Kind::Frame(Vec::new()), &format!("{name}.Items"));
        let mut bar = look::component_or(
            look,
            &format!("{}.Bar", self.config.component),
            "List.Bar",
            &Spec::new([self.config.bar, h], ""),
        );
        bar.name = format!("{name}.Bar");
        call(&mut bar, "x", &[w - self.config.bar / 2.0]);
        let mut frame = named(Kind::Frame(vec![items, bar]), name);
        call(&mut frame, "xy", &[x, y]);
        frame.clip = Some([0.0, 0.0, w, h]);
        frame
    }

    /// One frame: inertia and glides, then the actors brought up to date
    /// (built, refilled, placed). A list that came to rest after a flick
    /// snaps; a wheel list at rest selects the item at its line.
    pub fn update(
        &mut self,
        dt: f32,
        root: &mut Actor,
        model: &dyn ListModel,
        look: &dyn Look,
        cx: &mut Cx,
    ) -> Option<ListEvent> {
        let dt = f64::from(dt);
        let mut event = None;
        // A trackpad's stream ends when its events stop: it snaps as a
        // drag let go does, unless it was a wheel's notch in disguise.
        if let Some(s) = &mut self.stream {
            s.idle += dt;
            if s.idle >= STREAM_END {
                let s = *s;
                self.stream = None;
                let row = f64::from(self.config.row);
                event = if s.events <= SHORT_STREAM && s.moved != 0.0 && s.moved.abs() < row / 2.0 {
                    self.target = Some(s.from);
                    self.notch(s.moved.signum() * NOTCH);
                    None
                } else if let Some(t) = self.target {
                    // Still gliding (a notch's, a key's): it snaps where
                    // the glide goes.
                    let pos = std::mem::replace(&mut self.pos, t);
                    let event = self.settle(model);
                    self.target = Some(self.target.unwrap_or(t));
                    self.pos = pos;
                    event
                } else {
                    self.settle(model)
                };
            }
        }
        // A glide goes on under a stream (no inertia: a wheel stopped it).
        if !self.dragging && self.thumb.is_none() {
            if self.velocity != 0.0 {
                let inside = self.clamp(self.pos) == self.pos;
                self.pos += self.velocity * dt;
                self.velocity *= (-FRICTION * dt).exp();
                let clamped = self.clamp(self.pos);
                if clamped != self.pos || self.velocity.abs() < STOP_SPEED {
                    // Running into an end stops it there; flicked from past
                    // one, it springs back.
                    if inside {
                        self.pos = clamped;
                    }
                    self.velocity = 0.0;
                    event = self.settle(model);
                }
            } else if let Some(t) = self.target.filter(|_| !self.config.glide) {
                self.pos = t;
                self.target = None;
                event = self.at_rest(model);
            } else if let Some(t) = self.target {
                let next = self.pos + (t - self.pos) * (1.0 - (-GLIDE * dt).exp());
                // Close enough, or no closer in the numbers' precision.
                if (t - next).abs() < 0.5 || next == self.pos {
                    self.pos = t;
                    self.target = None;
                    event = self.at_rest(model);
                } else {
                    self.pos = next;
                }
            }
        }
        if dt > 0.0 {
            let v = (self.pos - self.last_pos).abs() / dt;
            self.motion += (v - self.motion) * (1.0 - (-dt / MOTION_SMOOTH).exp());
            if self.motion < MOTION_FLOOR {
                self.motion = 0.0;
            }
        }
        self.last_pos = self.pos;
        self.sync(root, model, look, cx);
        event
    }

    /// A wheel's notch (or a few, by `dy`'s size, [`NOTCH`] each): a row
    /// each in the wheel style, from the row it is going to; [`NOTCH`]
    /// pixels each plain, whatever the platform calls a notch.
    fn notch(&mut self, dy: f64) {
        let at = self.target.unwrap_or(self.pos);
        let rows = (dy.abs() / NOTCH).round().max(1.0);
        let to = match self.focus_line() {
            Some(f) if !self.kinds.is_empty() => {
                let rows = rows as usize;
                let i = self.item_at(self.clamp(at) + f);
                let j = if dy > 0.0 {
                    (i + rows).min(self.kinds.len() - 1)
                } else {
                    i.saturating_sub(rows)
                };
                self.centre(j) - f
            }
            _ => at + dy.signum() * rows * NOTCH,
        };
        self.target = Some(self.clamp(to));
    }

    /// A wheel list at rest selects what is on its line.
    fn at_rest(&mut self, model: &dyn ListModel) -> Option<ListEvent> {
        let f = self.focus_line()?;
        if self.kinds.is_empty() {
            return None;
        }
        let i = self.item_at(self.pos + f);
        (i != self.selected).then(|| {
            self.set_selected(i, model.key(i));
            ListEvent::Selected(i)
        })
    }

    /// At rest after a flick or a drag: snap (a wheel to the item on its
    /// line, selected once it is there).
    fn settle(&mut self, model: &dyn ListModel) -> Option<ListEvent> {
        if self.kinds.is_empty() {
            return None;
        }
        let i = match self.focus_line() {
            Some(f) => {
                let i = self.item_at(self.pos + f);
                self.target = Some(self.clamp(self.centre(i) - f));
                i
            }
            None => {
                let i = self.item_at(self.pos);
                let (a, b) = (self.offsets[i], self.offsets[i + 1]);
                let to = if self.pos - a < b - self.pos { a } else { b };
                self.target = Some(self.clamp(to));
                return None;
            }
        };
        if self.target == Some(self.pos) {
            self.target = None;
        }
        (i != self.selected).then(|| {
            self.set_selected(i, model.key(i));
            ListEvent::Selected(i)
        })
    }

    fn sync(&mut self, root: &mut Actor, model: &dyn ListModel, look: &dyn Look, cx: &mut Cx) {
        let name = self.config.name.clone();
        let Some(path) = hit::find(root, &format!("{name}.Items")) else {
            return;
        };
        let n = self.kinds.len();
        let h = self.height_shown();
        let w = self.config.rect[2] - self.config.bar - 2.0;
        let margin = f64::from(self.config.margin);
        let (top, bottom) = (self.pos - margin, self.pos + h + margin);
        let first = if n == 0 {
            0
        } else {
            self.item_at(top.max(0.0))
        };
        let mut last = first;
        while last < n && self.offsets[last] < bottom {
            last += 1;
        }
        // Free what left the range (or changed kind).
        for s in &mut self.slots {
            if let Some(i) = s.item
                && (i < first || i >= last || self.kinds[i] != s.kind)
            {
                s.item = None;
            }
        }
        let Some(items) = hit::actor_at_mut(root, &path) else {
            return;
        };
        for i in first..last {
            if self.slots.iter().any(|s| s.item == Some(i)) {
                continue;
            }
            let kind = self.kinds[i];
            let k = match self
                .slots
                .iter()
                .position(|s| s.item.is_none() && s.kind == kind)
            {
                Some(k) => k,
                None => {
                    let (part, height) = match kind {
                        ItemKind::Header => ("Header", self.config.header),
                        ItemKind::Row => ("Row", self.config.row),
                    };
                    let mut a = look::component_or(
                        look,
                        &format!("{}.{part}", self.config.component),
                        &format!("List.{part}"),
                        &Spec::new([w, height - 2.0], ""),
                    );
                    a.name = format!("{name}#{}", self.slots.len());
                    if let Kind::Frame(children) = &mut items.kind {
                        children.push(a);
                    }
                    self.slots.push(Slot {
                        kind,
                        item: None,
                        selected: None,
                        image: false,
                    });
                    self.slots.len() - 1
                }
            };
            self.slots[k].item = Some(i);
            self.slots[k].selected = None;
            if let Some(a) = items.children_mut().get_mut(k) {
                model.fill(i, a, cx);
                self.fills += 1;
                self.slots[k].image = hit::find_mut(a, "Banner").is_some_and(|b| b.visible);
            }
        }
        // Images only while slow enough to look at them.
        let speed = self.speed();
        let fast = speed > FAST;
        let focus = self.focus_line();
        let row = f64::from(self.config.row);
        let blur = if self.config.glide {
            let past = speed as f32 - look::metric(look, "List.MotionBlurFrom", BLUR_FROM);
            (look::metric(look, "List.MotionBlur", BLUR) * past.max(0.0) / 60.0)
                .min(self.config.row)
        } else {
            0.0
        };
        self.blur = blur;
        let children = items.children_mut();
        for (k, s) in self.slots.iter_mut().enumerate() {
            let Some(a) = children.get_mut(k) else {
                continue;
            };
            let Some(i) = s.item else {
                a.visible = false;
                continue;
            };
            let y = self.offsets[i] - self.pos;
            // Built and filled in the margin, drawn only where it shows.
            a.visible = y + (self.offsets[i + 1] - self.offsets[i]) > 0.0 && y < h;

            if s.image
                && let Some(b) = hit::find_mut(a, "Banner")
            {
                b.visible = !fast;
            }
            match focus {
                None => {
                    call(a, "xy", &[0.0, y as f32]);
                    a.motion_blur = blur;
                }
                Some(f) => {
                    let mid = (self.offsets[i] + self.offsets[i + 1]) / 2.0 - self.pos;
                    let place = look::wheel(look, &name, ((mid - f) / row) as f32);
                    call(a, "xy", &[place.x, y as f32 + place.y]);
                    call(a, "zoom", &[place.zoom]);
                    // In the row's own units.
                    a.motion_blur = if place.zoom > 0.0 {
                        blur / place.zoom
                    } else {
                        0.0
                    };
                    call(a, "diffusealpha", &[place.alpha]);
                    // Moved right, the row keeps its right edge: its
                    // `Right` part moves back, its plate and what is drawn
                    // around it (`Plate`, `Line`, `Shadow`) narrow.
                    if let Some(r) = hit::find_mut(a, "Right") {
                        call(r, "x", &[-place.x]);
                    }
                    let narrow = ((w - place.x) / w.max(1.0)).clamp(0.0, 1.0);
                    for part in ["Plate", "Line", "Shadow"] {
                        if let Some(p) = hit::find_mut(a, part) {
                            call(p, "zoomx", &[narrow]);
                        }
                    }
                }
            }
            let selected = i == self.selected;
            let continuous = self.config.continuous_selection
                && focus.is_some_and(|f| {
                    let mid = (self.offsets[i] + self.offsets[i + 1]) / 2.0 - self.pos;
                    let d = ((mid - f) / row).abs().min(1.0) as f32;
                    let weight = 1.0 - d * d * (3.0 - 2.0 * d);
                    look.wheel_focus(&name, a, w, weight)
                });
            if !continuous && s.selected != Some(selected) {
                s.selected = Some(selected);
                a.play(
                    if selected { "Select" } else { "Deselect" },
                    &mut ddi_actor::Problems::default(),
                );
            }
        }
        self.fast = fast;
        // The scrollbar.
        if let Some(bar) = hit::find_mut(root, &format!("{name}.Bar")) {
            let range = self.max_scroll() - self.min_scroll();
            bar.visible = range > 0.0 && focus.is_none();
            if bar.visible {
                let thumb = self.thumb_size();
                let y =
                    ((self.pos - self.min_scroll()) / range * (h - thumb)).clamp(0.0, h - thumb);
                if let Some(t) = hit::find_mut(bar, "Thumb") {
                    if let Kind::Sprite(s) = &mut t.kind {
                        s.size = [self.config.bar, thumb as f32];
                    }
                    call(t, "y", &[y as f32]);
                }
            }
        }
    }

    /// The thumb's length: the share shown, at least 24 (or the whole bar
    /// when shorter).
    fn thumb_size(&self) -> f64 {
        let h = self.height_shown();
        (h * h / self.content().max(1.0)).clamp(24.0_f64.min(h), h)
    }

    /// The item an actor shows, from a target's chain.
    fn item_of(&self, target: &crate::hit::Target) -> Option<usize> {
        let prefix = format!("{}#", self.config.name);
        target.chain.iter().find_map(|n| {
            let k: usize = n.strip_prefix(&prefix)?.parse().ok()?;
            self.slots.get(k)?.item
        })
    }

    /// An input: pointer gestures on the list, the wheel over it, and menu
    /// actions when the screen gives them (the list focused). `root` is
    /// the screen's tree, through which pointers are taken into the list's
    /// own space; `model` gives keys for the selection.
    pub fn input(
        &mut self,
        input: &Input,
        root: &Actor,
        model: &dyn ListModel,
    ) -> (Handled, Option<ListEvent>) {
        let name = self.config.name.clone();
        match input {
            Input::Pointer {
                target: Some(t),
                gesture,
                pos,
                ..
            } if t.within(&name) => {
                let on_bar = t.within(&format!("{name}.Bar"));
                let local_y = hit::local_point(root, &hit::IDENTITY, &name, *pos)
                    .map_or(f64::from(pos[1] - self.config.rect[1]), |p| f64::from(p[1]));
                if on_bar {
                    return (self.bar(*gesture, local_y), None);
                }
                match gesture {
                    Gesture::Press => {
                        // Catch a moving list.
                        self.caught = self.velocity != 0.0 || self.target.is_some();
                        self.velocity = 0.0;
                        self.target = None;
                        self.stream = None;
                    }
                    Gesture::DragStart => self.dragging = true,
                    Gesture::Drag { delta } => {
                        self.pos = self.rubber(self.pos, -f64::from(delta[1]));
                    }
                    Gesture::Flick { velocity } => {
                        self.dragging = false;
                        self.velocity = -f64::from(velocity[1]);
                    }
                    Gesture::DragEnd { .. } | Gesture::Cancel | Gesture::Release => {
                        self.dragging = false;
                        return (Handled::Yes, self.settle(model));
                    }
                    Gesture::Tap => {
                        // The tap that caught a moving list only stops it.
                        if std::mem::take(&mut self.caught) {
                            return (Handled::Yes, self.settle(model));
                        }
                        let Some(i) = self.item_of(t) else {
                            return (Handled::Yes, None);
                        };
                        if i == self.selected && self.target.is_none() {
                            return (Handled::Yes, Some(ListEvent::Activated(i)));
                        }
                        self.select(i);
                        self.selected_key = model.key(i);
                        return (Handled::Yes, Some(ListEvent::Selected(i)));
                    }
                    Gesture::LongPress => {}
                }
                (Handled::Yes, None)
            }
            Input::Wheel {
                target: Some(t),
                delta,
                kind,
                ..
            } if t.within(&name) => {
                let dy = f64::from(delta[1]);
                // Sideways (Shift with a wheel, a tilt, a trackpad's
                // horizontal swipe): not this list's.
                if dy == 0.0 {
                    return (Handled::No, None);
                }
                self.velocity = 0.0;
                match kind {
                    WheelKind::Notch => {
                        self.stream = None;
                        self.notch(dy);
                    }
                    WheelKind::Precise => {
                        let from = self.target.unwrap_or(self.pos);
                        let s = self.stream.get_or_insert(Stream {
                            idle: 0.0,
                            events: 0,
                            moved: 0.0,
                            from,
                        });
                        s.idle = 0.0;
                        s.events += 1;
                        s.moved += dy;
                        match self.target {
                            // A glide under way carries the fingers' moves
                            // on with it.
                            Some(t) => {
                                self.pos += dy;
                                self.target = Some(self.clamp(t + dy));
                            }
                            None => self.pos = self.rubber(self.pos, dy),
                        }
                    }
                }
                (Handled::Yes, None)
            }
            Input::Menu { action, .. } => match action {
                MenuAction::Up | MenuAction::Down => {
                    if self.kinds.is_empty() {
                        return (Handled::Yes, None);
                    }
                    let i = match action {
                        MenuAction::Up => self.selected.saturating_sub(1),
                        _ => (self.selected + 1).min(self.kinds.len() - 1),
                    };
                    let moved = i != self.selected;
                    self.select(i);
                    self.selected_key = model.key(i);
                    (Handled::Yes, moved.then_some(ListEvent::Selected(i)))
                }
                MenuAction::Start if !self.kinds.is_empty() => {
                    (Handled::Yes, Some(ListEvent::Activated(self.selected)))
                }
                _ => (Handled::No, None),
            },
            _ => (Handled::No, None),
        }
    }

    /// The scrollbar: a press on the track jumps there, the thumb drags.
    fn bar(&mut self, gesture: Gesture, y: f64) -> Handled {
        let h = self.height_shown();
        let range = self.max_scroll() - self.min_scroll();
        if range <= 0.0 {
            return Handled::Yes;
        }
        let thumb = self.thumb_size();
        let min = self.min_scroll();
        let thumb_y = (self.pos - min) / range * (h - thumb);
        let to = |grab: f64| {
            let top = (y - grab).clamp(0.0, (h - thumb).max(0.0));
            min + top / (h - thumb).max(1.0) * range
        };
        match gesture {
            Gesture::Press => {
                self.velocity = 0.0;
                self.target = None;
                self.stream = None;
                let grab = if (thumb_y..thumb_y + thumb).contains(&y) {
                    y - thumb_y
                } else {
                    thumb / 2.0
                };
                self.thumb = Some(grab);
                self.pos = self.clamp(to(grab));
            }
            Gesture::Drag { .. } | Gesture::DragStart => {
                if let Some(grab) = self.thumb {
                    self.pos = self.clamp(to(grab));
                }
            }
            _ => self.thumb = None,
        }
        Handled::Yes
    }
}
