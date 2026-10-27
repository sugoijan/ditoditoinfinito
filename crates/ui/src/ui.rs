//! The driver: events in, requests out, frames stepped and drawn.
//!
//! **Per event** the shell calls [`Ui::handle`] in its own handler and
//! carries out the [`Outcome`]'s requests before the handler returns, so
//! those that need a user gesture still have it (P2); `consumed` says
//! whether to `preventDefault`. What counts as a gesture is the HTML
//! standard's activation-triggering input: a key press other than Escape,
//! a mouse button's press, a touch's or a pen's release; a mouse's
//! release counts within [`ACTIVATION_WINDOW`] of its press (browsers keep
//! the press's activation for a while: Chrome and Firefox 5 s, Safari
//! about 1 s), so a click, which fires on release, may start sound. A long
//! press fires from a frame and is never one, nor is the lift after it.
//! Where gestures are required (the web), a request that needs one made
//! outside one is refused: the screen that made it gets
//! [`Input::NeedsGesture`] and asks again from the next activation. Nothing
//! is queued for later, so nothing stale fires on an unrelated tap.
//!
//! **Per frame** [`Ui::update`] steps key repeats, long presses, the
//! actors (tweens, effects), the screens and their transitions;
//! [`Ui::draw`] measures text and draws every layer, bottom first, into a
//! list of [`Drawn`] in virtual pixels; [`Ui::take_requests`] collects
//! what frames asked for. [`Ui::needs_redraw`] says whether a frame is
//! needed at all (P13).
//!
//! **Layers**, bottom first: screens, each over the last, every one
//! drawn; overlays belong to the layer that opened them, are drawn and
//! take input directly above it, and leave with it. Popups pass on input
//! they leave; dialogs are modal (input and focus stay inside, a press
//! outside is [`Input::Outside`]). Input goes down from the top through
//! popups to the first dialog or screen. A screen pushed from a dialog
//! goes above it. A layer coming in or leaving takes no input, as
//! `ScreenWithMenuElements` ignores Start and Back while it transitions;
//! a layer that drops out of the input's way (covered, leaving) has its
//! pointers cancelled.
//!
//! **Transitions** follow `ScreenWithMenuElements` (`TweenOnScreen`,
//! `StartTransitioningScreen`, `Cancel`) and `Transition`
//! (`src/Transition.cpp`): coming in, the root plays `On` and a child
//! named `In` plays `StartTransitioning`; leaving, `Off` and `Out`;
//! backing out, `Cancel` and the `Cancel` child (else `Out`). The
//! transition lasts as long as the tweens these queued anywhere in the
//! tree, measured once when it starts (StepMania measures the screen's
//! tween time left and hibernates its `Out` to end with it), so a looping
//! command cannot hold a screen forever. Screens follow each other, never
//! cross: a screen pushed over another ([`Cx::push`]) waits for the one
//! under it to play `Off` and `Out` (covered, it stays as those left it),
//! and that one plays `On` and `In` again once the screen over it has
//! left. A [`UiRequest::Navigate`] or [`UiRequest::Back`] from a screen
//! (the web's way between its pages) leaves the same way: the current
//! screen plays `Off` (or `Cancel`, backing out) and the request goes out
//! once it has, the layer kept as it ended, taking nothing, until the
//! shell acts on it. Both shells see the same sequence; the shell's own
//! [`Ui::push`] is immediate.

use ddi_actor::screen::Viewport;
use ddi_actor::text::TextMeasure;
use ddi_actor::{
    Actor, Blend, Clocks, Depth, Drawn, Kind, Problems, Quad, UNTEXTURED, Value, View,
};

use crate::event::{
    KeyEvent, MenuEvent, PointerEvent, PointerKind, PointerPhase, TextEvent, UiEvent, WheelEvent,
    buttons,
};
use crate::gesture::{Gesture, Tracker};
use crate::hit::{self, Affine, IDENTITY, Target};
use crate::message::Message;
use crate::nav::{self, MenuAction, Repeater, Source};
use crate::request::{CssRect, Cursor, InputHints, UiRequest};
use crate::screen::{Cx, Handled, Input, Op, Screen, TextChange};
use crate::text_field::{KeyResult, TextField, TextMode};
use crate::textures::TextureTable;
use std::cell::RefCell;
use std::rc::Rc;

/// The message screens hear when the design size changed
/// ([`Ui::set_design`]).
pub const DESIGN_MESSAGE: &str = "DesignChanged";

/// How long after a mouse press its release still counts as a gesture,
/// host milliseconds (Safari's transient activation, the shortest).
pub const ACTIVATION_WINDOW: f64 = 1000.0;

/// The focus ring drawn by default (a theme may draw its own and turn
/// this off, [`Ui::set_focus_ring`]): this far outside the actor, this
/// thick, this color (virtual pixels, encoded RGBA).
pub const RING_GAP: f32 = 3.0;
pub const RING_WIDTH: f32 = 2.0;
pub const RING_COLOR: [f32; 4] = [1.0, 0.78, 0.2, 1.0];
/// The caret and the composition's underline, as a fraction of the text's
/// size (at least a virtual pixel), and the selection's color.
pub const CARET_WIDTH: f32 = 1.0 / 14.0;
pub const SELECTION_COLOR: [f32; 4] = [0.35, 0.55, 1.0, 0.4];

/// What an event came to.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Outcome {
    /// A screen, the field being typed into or menu navigation took it:
    /// the shell calls `preventDefault` (arrows, Space, Tab, Page Up and
    /// Down a screen used, ctrl+wheel it scrolled). Keys the textarea
    /// edits with are not consumed.
    pub consumed: bool,
    /// For the shell to carry out now.
    pub requests: Vec<UiRequest>,
}

#[derive(Debug)]
enum Phase {
    Entering(f32),
    Active,
    Leaving(f32),
    /// A screen playing its `Off` before the one pushed over it
    /// ([`Layer::then`]) comes in.
    Covering(f32),
    /// A screen under another, as its `Off` left it: it plays `On` again
    /// once the one over it has left.
    Covered,
    /// Left with a request ([`Layer::leave_with`]) the shell has yet to
    /// act on: drawn as it ended, taking nothing.
    Left,
}

impl Phase {
    /// Moving: a frame is needed.
    fn in_transition(&self) -> bool {
        matches!(
            self,
            Phase::Entering(_) | Phase::Leaving(_) | Phase::Covering(_)
        )
    }

    /// On its way out, or out: not the current screen.
    fn going(&self) -> bool {
        matches!(self, Phase::Leaving(_) | Phase::Covering(_) | Phase::Left)
    }
}

struct Layer {
    id: u64,
    screen: Box<dyn Screen>,
    root: Actor,
    phase: Phase,
    /// What comes in once this one has left ([`Cx::replace`]) or played
    /// its `Off` under it ([`Cx::push`]).
    then: Option<Box<dyn Screen>>,
    /// The request this screen leaves with ([`UiRequest::Navigate`],
    /// [`UiRequest::Back`]): sent once it has left.
    leave_with: Option<UiRequest>,
    focused: Option<String>,
    overlay: bool,
    modal: bool,
    /// An overlay's: the layer that opened it.
    owner: Option<u64>,
    /// The app's chrome ([`Ui::push_chrome`]): always on top, focused
    /// and given menu actions only while it has the menu's focus
    /// ([`Ui::chrome_focused`]), pointers on its actors.
    chrome: bool,
    /// The screen size (virtual pixels) its root was built for: an
    /// overlay is built again when it changes (a window resized, the
    /// chrome's banner come or gone), since dialogs lay out only there.
    built: [f32; 2],
}

/// A pointer held down: where its gesture goes.
struct Pointer {
    id: u64,
    layer: u64,
    target: Option<Target>,
    tracker: Tracker,
    kind: PointerKind,
    /// Host time of the press.
    down: f64,
    /// Where it is, virtual pixels.
    pos: [f32; 2],
}

struct ActiveText {
    layer: u64,
    name: String,
    field: TextField,
}

/// A layer's state, for tests and the debug view.
#[derive(Clone, Debug, PartialEq)]
pub struct LayerInfo {
    pub name: String,
    pub overlay: bool,
    pub modal: bool,
    /// `"in"`, `"active"`, `"out"` (leaving, or left with a request the
    /// shell has yet to act on), `"covering"` (playing its `Off` before
    /// the screen pushed over it comes in) or `"covered"` (under another
    /// screen).
    pub phase: &'static str,
    pub focused: Option<String>,
}

/// The UI: screens, input routing, focus, typing, transitions.
pub struct Ui {
    design: [f32; 2],
    viewport: Viewport,
    mode: TextMode,
    gestures: bool,
    /// Bottom first.
    layers: Vec<Layer>,
    next_id: u64,
    out: Vec<UiRequest>,
    consumed: bool,
    /// A pad's press is being handled ([`Ui::refused`]).
    pad_press: bool,
    textures: Rc<RefCell<TextureTable>>,
    pointers: Vec<Pointer>,
    hover: Option<(u64, Option<Target>)>,
    /// Where the mouse is (virtual pixels), while it is over the surface.
    mouse: Option<[f32; 2]>,
    /// The pointer's look told to the shell, and the hover it was worked
    /// out for ([`Ui::refresh_cursor`]).
    cursor: Cursor,
    cursor_for: Option<(u64, Option<Target>)>,
    repeater: Repeater,
    ring_shown: bool,
    draw_ring: bool,
    /// The chrome's height across the top (virtual pixels): the other
    /// layers are laid out below it ([`Screen::reserve`]).
    inset: f32,
    /// The chrome layer that has the menu's focus (reached by Up past a
    /// screen's top, or F2), if one has.
    chrome_focus: Option<u64>,
    /// Where the focus was when the chrome took it: the layer and its
    /// focused actor, given back when the chrome lets go.
    chrome_return: Option<(u64, Option<String>)>,
    text: Option<ActiveText>,
    ime_rect: Option<CssRect>,
    last_mouse_down: Option<f64>,
    time: f64,
    dirty: bool,
    problems: Problems,
    /// A keyboard key's menu action is being handled ([`Cx::menu_from_keyboard`]).
    menu_key: bool,
    menu_source: Option<Source>,
    /// A controller's menu action is being handled ([`Cx::menu_from_pad`]).
    menu_pad: bool,
    /// Wheel notches told from trackpad streams.
    wheel_stream: crate::gesture::WheelStream,
    /// The chrome is being built (its viewport is the whole surface).
    building_chrome: bool,
}

impl Ui {
    /// A UI for a theme designed at `design` (virtual pixels; the width
    /// follows the window's aspect), typing in `mode` (the web mirrors
    /// its textarea, desktop edits itself), and whether the platform
    /// requires user gestures for some requests (the web: yes; desktop:
    /// no, and every request goes out at once). Until the first
    /// [`UiEvent::Resize`] the surface is the design at a ratio of 1.
    pub fn new(design: [f32; 2], mode: TextMode, gestures: bool) -> Ui {
        Ui {
            design,
            viewport: Viewport::new(design, design, 1.0),
            mode,
            gestures,
            layers: Vec::new(),
            next_id: 1,
            out: Vec::new(),
            consumed: false,
            pad_press: false,
            textures: Rc::default(),
            pointers: Vec::new(),
            hover: None,
            mouse: None,
            cursor: Cursor::Default,
            cursor_for: None,
            repeater: Repeater::default(),
            ring_shown: false,
            draw_ring: true,
            inset: 0.0,
            chrome_focus: None,
            chrome_return: None,
            building_chrome: false,
            text: None,
            ime_rect: None,
            last_mouse_down: None,
            time: 0.0,
            dirty: true,
            problems: Problems::default(),
            menu_key: false,
            menu_source: None,
            menu_pad: false,
            wheel_stream: Default::default(),
        }
    }

    pub fn viewport(&self) -> &Viewport {
        &self.viewport
    }

    /// The design size (the virtual screen's, before the window's aspect).
    pub fn design(&self) -> [f32; 2] {
        self.design
    }

    /// A new design size (a compact layout's, for a small window): the
    /// virtual screen follows it, and every screen hears
    /// [`DESIGN_MESSAGE`] to lay itself out again.
    pub fn set_design(&mut self, design: [f32; 2]) {
        if design == self.design {
            return;
        }
        self.design = design;
        let v = self.viewport;
        let css = [v.physical[0] / v.ratio, v.physical[1] / v.ratio];
        self.viewport = Viewport::new(design, css, v.ratio);
        self.ime_rect = None;
        self.broadcast(Message::new(DESIGN_MESSAGE));
    }

    /// Pushes a screen (the first one, at boot), at once: a screen under
    /// it is covered as it is, and comes in again when uncovered (a
    /// screen's own [`Cx::push`] lets the one under it leave first).
    pub fn push(&mut self, screen: impl Screen + 'static) {
        if let Some(l) = self.current_screen_mut()
            && matches!(l.phase, Phase::Active | Phase::Entering(_))
        {
            l.phase = Phase::Covered;
        }
        self.push_layer(Box::new(screen), false, false, None, false);
        self.dirty = true;
    }

    /// The current screen's layer: the topmost that is neither an
    /// overlay nor the chrome.
    fn current_screen_mut(&mut self) -> Option<&mut Layer> {
        self.layers
            .iter_mut()
            .rev()
            .find(|l| !l.chrome && !l.overlay)
    }

    /// A screen pushed from a call: the current screen plays `Off` and
    /// `Out` first (its typing ended, its pointers cancelled), and
    /// `screen` comes in once it has.
    fn push_screen(&mut self, screen: Box<dyn Screen>, gesture: bool) {
        let mut problems = Problems::default();
        let mut screen = Some(screen);
        let covering = match self.current_screen_mut() {
            Some(top) if matches!(top.phase, Phase::Active | Phase::Entering(_)) => {
                let left = start_transition(&mut top.root, "Off", &["Out"], &mut problems);
                if left > 0.0 {
                    top.phase = Phase::Covering(left);
                    top.then = screen.take();
                    Some(top.id)
                } else {
                    top.phase = Phase::Covered;
                    None
                }
            }
            _ => None,
        };
        self.problems.messages.extend(problems.messages);
        if let Some(id) = covering {
            self.release_layer(id);
            self.cancel_directions();
            self.prune_inputs();
            self.dirty = true;
        } else if let Some(screen) = screen {
            self.push_layer(screen, false, false, None, gesture);
        }
    }

    /// The current screen comes in again once nothing covers it.
    fn uncover(&mut self) {
        let mut problems = Problems::default();
        let Some(top) = self.current_screen_mut() else {
            return;
        };
        if !matches!(top.phase, Phase::Covered) {
            return;
        }
        let left = start_transition(&mut top.root, "On", &["In"], &mut problems);
        top.phase = if left > 0.0 {
            Phase::Entering(left)
        } else {
            Phase::Active
        };
        self.problems.messages.extend(problems.messages);
        self.dirty = true;
    }

    /// A request the current screen leaves with: it plays `Off` (or
    /// `Cancel` for [`UiRequest::Back`]) and the request goes out once it
    /// has left. Sent at once where no screen is shown.
    fn leave_with(&mut self, request: UiRequest) {
        let cancel = matches!(request, UiRequest::Back);
        let Some(top) = self.current_screen_mut() else {
            self.out.push(request);
            return;
        };
        if top.phase.going() {
            return;
        }
        top.leave_with = Some(request);
        let id = top.id;
        self.close(id, cancel);
    }

    /// Puts `screen` over every screen and dialog for as long as the UI
    /// runs: the app's notices. It takes the menu's focus only when sent
    /// there (F2, or [`Cx::enter_chrome`] and a fresh Up past a screen's
    /// top where nothing is above), and gives it back on Down, Back or F2
    /// to where it was; keys otherwise go to the screens. A pointer on
    /// one of its actors goes to it (unless [`Screen::lets_through`]),
    /// anywhere else to what is under it. The other layers are laid out
    /// below the room it reserves ([`Screen::reserve`]); screens pushed
    /// later still go under it.
    pub fn push_chrome(&mut self, screen: impl Screen + 'static) {
        self.building_chrome = true;
        self.push_layer(Box::new(screen), true, false, None, false);
        self.building_chrome = false;
        if let Some(l) = self.layers.last_mut() {
            l.chrome = true;
            l.focused = None;
        }
        self.follow_inset();
        self.dirty = true;
    }

    /// The other layers' viewport: the part of the surface below the
    /// chrome, a design's screen fitted into it as into a window that
    /// size (everything a little smaller, nothing cut).
    fn inner_viewport(&self) -> Viewport {
        let v = self.viewport;
        if self.inset <= 0.0 {
            return v;
        }
        let css = [v.physical[0] / v.ratio, v.physical[1] / v.ratio];
        let top = self.inset * v.scale / v.ratio;
        let mut inner = Viewport::new(self.design, [css[0], (css[1] - top).max(1.0)], v.ratio);
        inner.offset[1] += top * v.ratio;
        inner
    }

    /// Layer `id`'s place on the whole screen: its virtual pixels times
    /// `k`, plus `t` (identity for the chrome and without it).
    fn map_of(&self, id: u64) -> (f32, [f32; 2]) {
        self.map_for(self.layer(id).is_some_and(|l| l.chrome))
    }

    fn map_for(&self, chrome: bool) -> (f32, [f32; 2]) {
        if chrome || self.inset <= 0.0 {
            return (1.0, [0.0, 0.0]);
        }
        let (o, i) = (self.viewport, self.inner_viewport());
        (
            i.scale / o.scale,
            [
                (i.offset[0] - o.offset[0]) / o.scale,
                (i.offset[1] - o.offset[1]) / o.scale,
            ],
        )
    }

    /// A point of the whole screen in layer `id`'s pixels.
    fn local(&self, id: u64, p: [f32; 2]) -> [f32; 2] {
        let (k, t) = self.map_of(id);
        [(p[0] - t[0]) / k, (p[1] - t[1]) / k]
    }

    /// Layer `id`'s viewport: the whole surface for the chrome, the part
    /// below it for the rest.
    fn viewport_of(&self, id: u64) -> Viewport {
        if self.layer(id).is_some_and(|l| l.chrome) {
            self.viewport
        } else {
            self.inner_viewport()
        }
    }

    /// The chrome's height now; the screens lay themselves out again at
    /// their next frame when it changed. Its focus goes when it has
    /// nothing to focus.
    fn follow_inset(&mut self) {
        let inset: f32 = self
            .layers
            .iter()
            .filter(|l| l.chrome && !matches!(l.phase, Phase::Leaving(_)))
            .map(|l| l.screen.reserve())
            .sum();
        if inset != self.inset {
            self.inset = inset.max(0.0);
            self.dirty = true;
        }
        if let Some(c) = self.chrome_focus
            && self
                .layer(c)
                .is_none_or(|l| l.screen.focusables().is_empty())
        {
            self.leave_chrome(false);
        }
    }

    /// The chrome takes the menu's focus (its first action).
    fn enter_chrome(&mut self, gesture: bool) -> bool {
        self.cancel_directions();
        let Some((id, first)) = self.layers.iter().rev().find_map(|l| {
            let f = l.screen.focusables().into_iter().next()?;
            (l.chrome && matches!(l.phase, Phase::Active)).then_some((l.id, f.name))
        }) else {
            return false;
        };
        if self.chrome_focus.is_none() {
            self.chrome_return = self
                .focus_layer()
                .and_then(|l| Some((l, self.layer(l)?.focused.clone())));
        }
        self.chrome_focus = Some(id);
        self.ring_shown = true;
        self.set_focus(id, Some(first), gesture);
        true
    }

    /// The menu's focus goes back from the chrome to the screen, on what
    /// it had when the chrome took it (if that is still there).
    fn leave_chrome(&mut self, gesture: bool) {
        if let Some(id) = self.chrome_focus.take() {
            self.set_focus(id, None, gesture);
        }
        if let Some((l, name)) = self.chrome_return.take()
            && let Some(layer) = self.layer(l)
            && layer.focused != name
            && name
                .as_ref()
                .is_none_or(|n| layer.screen.focusables().iter().any(|f| &f.name == n))
        {
            self.set_focus(l, name, gesture);
        }
    }

    /// Whether the chrome has the menu's focus (tests, the shells' debug
    /// state).
    pub fn chrome_focused(&self) -> Option<&str> {
        let id = self.chrome_focus?;
        self.layer(id)?.focused.as_deref()
    }

    /// Broadcasts a message to every layer (the app's own:
    /// `CurrentSongChanged`, a device notice). Not a gesture.
    pub fn broadcast(&mut self, message: Message) {
        self.broadcast_inner(&message, false);
        self.dirty = true;
    }

    /// Whether the plain focus ring is drawn (a theme draws its own).
    pub fn set_focus_ring(&mut self, draw: bool) {
        self.draw_ring = draw;
        self.dirty = true;
    }

    /// Handles one event.
    pub fn handle(&mut self, event: UiEvent) -> Outcome {
        self.dirty = true;
        self.consumed = false;
        match event {
            UiEvent::Pointer(e) => self.pointer(e),
            UiEvent::Wheel(e) => self.wheel(e),
            UiEvent::Key(e) => self.key(e),
            UiEvent::Text(e) => self.text_event(e),
            UiEvent::Menu(e) => self.menu_event(e),
            UiEvent::Paste(s) => {
                if self.mode == TextMode::Own
                    && let Some(t) = &mut self.text
                {
                    self.consumed = true;
                    if t.field.insert(&s) {
                        self.text_changed(TextChange::Edited, false);
                    }
                }
            }
            UiEvent::Raw(e) => self.raw(e),
            UiEvent::Focus(true) => {}
            UiEvent::Focus(false) => self.blur(),
            UiEvent::Resize { size, ratio } => {
                self.viewport = Viewport::new(self.design, size, ratio);
                self.ime_rect = None;
            }
        }
        self.follow_inset();
        Outcome {
            consumed: self.consumed,
            requests: std::mem::take(&mut self.out),
        }
    }

    /// Steps `dt` seconds: key repeats, long presses, actors, screens,
    /// transitions.
    pub fn update(&mut self, dt: f32) {
        let dt = if dt.is_finite() { dt.max(0.0) } else { 0.0 };
        self.time += f64::from(dt);
        self.follow_inset();
        for action in self.repeater.update(dt) {
            self.menu(action, true, false);
        }
        let mut fired = Vec::new();
        for p in &mut self.pointers {
            let mut g = Vec::new();
            p.tracker.tick(dt, &mut g);
            for g in g {
                fired.push((p.layer, p.target.clone(), g, p.pos, p.kind));
            }
        }
        for (layer, target, g, pos, kind) in fired {
            self.deliver_gesture(layer, target, g, pos, kind, false);
        }
        let clocks = Clocks {
            timer: self.time as f32,
            ..Clocks::default()
        };
        for id in self.layer_ids() {
            let mut cx = self.cx(id, false);
            let Some(layer) = self.layer_mut(id) else {
                continue;
            };
            // An overlay laid out for another size: built again, as it
            // is (no transition).
            let size = [cx.viewport().screen.width, cx.viewport().screen.height];
            if layer.overlay
                && !layer.chrome
                && size != layer.built
                && !matches!(layer.phase, Phase::Leaving(_))
            {
                layer.built = size;
                layer.root = layer.screen.build(&mut cx);
                // The focused actor lit again.
                if let Some(a) = layer
                    .focused
                    .as_deref()
                    .and_then(|n| hit::find_mut(&mut layer.root, n))
                {
                    a.play("GainFocus", &mut Problems::default());
                }
                layer.root.finish_tweening();
            }
            let mut problems = Problems::default();
            layer.root.update(dt, &clocks, &mut problems);
            layer.screen.update(dt, &mut layer.root, &mut cx);
            let mut entered = false;
            let mut covered = None;
            let done = match &mut layer.phase {
                Phase::Entering(left) => {
                    *left -= dt;
                    entered = *left <= 0.0;
                    false
                }
                Phase::Leaving(left) => {
                    *left -= dt;
                    *left <= 0.0
                }
                Phase::Covering(left) => {
                    *left -= dt;
                    if *left <= 0.0 {
                        covered = layer.then.take();
                        layer.phase = Phase::Covered;
                    }
                    false
                }
                Phase::Active | Phase::Covered | Phase::Left => false,
            };
            if entered {
                layer.phase = Phase::Active;
            }
            self.problems.messages.extend(problems.messages);
            self.apply(id, cx);
            if done {
                self.left(id);
            }
            if let Some(next) = covered {
                self.push_layer(next, false, false, None, false);
            }
        }
        self.problems.messages.truncate(64);
        self.follow_inset();
        // What is under a still mouse changes too (a list scrolled under
        // it, a screen come in): the hover follows, with the cursor.
        if let Some(pos) = self.mouse
            && !self.pointers.iter().any(|p| p.kind == PointerKind::Mouse)
        {
            let now = self.pick(pos).map(|(l, t, _)| (l, t));
            self.set_hover(now);
        }
        self.refresh_cursor();
    }

    /// Tells the shell the pointer's look when what the mouse is over
    /// calls for another: its screen's [`Screen::cursor`], else a pointer
    /// over a focusable actor or a list's item (not a list's empty space,
    /// which a tap does not act on), else the default.
    fn refresh_cursor(&mut self) {
        if self.cursor_for == self.hover {
            return;
        }
        self.cursor_for = self.hover.clone();
        let cursor = match &self.hover {
            Some((l, Some(t))) => self
                .layer(*l)
                .map_or(Cursor::Default, |layer| cursor_over(&*layer.screen, t)),
            _ => Cursor::Default,
        };
        if cursor != self.cursor {
            self.cursor = cursor;
            self.out.push(UiRequest::SetCursor(cursor));
        }
    }

    /// Draws every layer, bottom first, in virtual pixels.
    pub fn draw(&mut self, measure: &mut dyn TextMeasure) -> Vec<Drawn> {
        self.draw_in(measure, IDENTITY)
    }

    /// [`Ui::draw`] under `transform` (virtual pixels to the target's
    /// units: a renderer drawing in virtual pixels scaled by
    /// `Viewport::scale` passes the viewport's offset over that scale).
    pub fn draw_in(&mut self, measure: &mut dyn TextMeasure, transform: Affine) -> Vec<Drawn> {
        let mut out = Vec::new();
        let view = View {
            transform,
            size: [self.viewport.screen.width, self.viewport.screen.height],
        };
        let focus_layer = self.focus_layer();
        let mut ime = None;
        let ring = self.draw_ring && self.ring_shown;
        let (k, sh) = self.map_for(false);
        let text = &mut self.text;
        let base = transform;
        for layer in self.layers.iter_mut() {
            // Below the chrome, the rest.
            let transform = if layer.chrome || k == 1.0 && sh == [0.0, 0.0] {
                base
            } else {
                let [a, b, c, d, tx, ty] = base;
                [
                    a * k,
                    b * k,
                    c * k,
                    d * k,
                    a * sh[0] + c * sh[1] + tx,
                    b * sh[0] + d * sh[1] + ty,
                ]
            };
            // The field being typed shows the field's text.
            let typing = text.as_ref().filter(|t| t.layer == layer.id).and_then(|t| {
                let path = hit::find(&layer.root, &t.name)?;
                Some((path, t.field.display().text))
            });
            if let Some((path, shown)) = &typing
                && let Some(a) = hit::actor_at_mut(&mut layer.root, path)
                && let Kind::Text(run) = &mut a.kind
            {
                run.set_text(shown);
            }
            layer.root.measure(measure);
            layer
                .root
                .draw_in(&view, transform, [1.0; 4], [1.0, 1.0, 1.0, 0.0], &mut out);
            if let (Some((path, _)), Some(t)) = (&typing, text.as_ref()) {
                // The caret in the whole screen's pixels (a layer below
                // the chrome is shifted and scaled).
                let (k, sh) = if layer.chrome {
                    (1.0, [0.0, 0.0])
                } else {
                    (k, sh)
                };
                ime = field_marks(&layer.root, path, &t.field, measure, &transform, &mut out)
                    .map(|r| shift(r, k, sh));
            }
            if ring
                && Some(layer.id) == focus_layer
                && let Some(name) = &layer.focused
                && let Some(b) = visible_bounds(&layer.root, name)
            {
                ring_quads(&transform, b, &mut out);
            }
        }
        if let Some(caret) = ime {
            let css = self.css_rect(caret);
            if self.ime_rect != Some(css) {
                self.ime_rect = Some(css);
                self.out.push(UiRequest::SetImeCursorArea(css));
            }
        }
        self.dirty = false;
        out
    }

    /// Requests made outside [`Ui::handle`] (frames, the IME's caret).
    /// None of them needs a gesture.
    pub fn take_requests(&mut self) -> Vec<UiRequest> {
        std::mem::take(&mut self.out)
    }

    /// Whether a frame is needed: input arrived or a screen asked for
    /// something since the last draw, a layer is coming in or leaving, an
    /// actor moves on the top screen or above it (covered screens wait),
    /// a held direction may repeat, a press may become a long press or a
    /// screen's deadline came. Otherwise the shell may stop its frames
    /// until the next event or [`Ui::wake_in`] (P13).
    pub fn needs_redraw(&self) -> bool {
        self.dirty
            || self.repeater.holding()
            || self.pointers.iter().any(|p| p.tracker.waiting())
            || self.layers.iter().any(|l| l.phase.in_transition())
            || self
                .awake()
                .iter()
                .any(|l| l.screen.animating() || l.root.in_motion())
            || self.wake_in() == Some(0.0)
    }

    /// The top screen and the overlays above it: covered screens wait.
    fn awake(&self) -> &[Layer] {
        let top = self.layers.iter().rposition(|l| !l.overlay).unwrap_or(0);
        &self.layers[top.min(self.layers.len())..]
    }

    /// Seconds until a screen has something to do though nothing moves
    /// ([`Screen::wake_at`]), none past: an idle shell asks for a frame
    /// once that long has passed and it told [`Ui::idle_for`] so.
    pub fn wake_in(&self) -> Option<f64> {
        self.awake()
            .iter()
            .filter_map(|l| l.screen.wake_at())
            .filter(|at| at.is_finite())
            .min_by(f64::total_cmp)
            .map(|at| (at - self.time).max(0.0))
    }

    /// The shell drew no frames for `seconds` (idle, nothing moving): the
    /// clock moves on by as much, so the screens' deadlines are met
    /// ([`Ui::wake_in`]). Nothing else is stepped.
    pub fn idle_for(&mut self, seconds: f64) {
        if seconds.is_finite() && seconds > 0.0 {
            self.time += seconds;
        }
    }

    /// An idle shell's wait for [`Ui::wake_in`], in seconds on the shell's
    /// own clock: frames stopped at `since` (`now` if not yet set; a frame
    /// clears it). When to wake; at or past it, the clock moves on by the
    /// time idle ([`Ui::idle_for`]), `since` is cleared and the UI needs
    /// a frame.
    pub fn idle_wake(&mut self, since: &mut Option<f64>, now: f64) -> Option<f64> {
        let from = *since.get_or_insert(now);
        if self.needs_redraw() {
            return None;
        }
        let at = from + self.wake_in()?;
        if now < at {
            return Some(at);
        }
        self.idle_for(now - from);
        *since = None;
        None
    }

    /// The layers, bottom first (not the chrome).
    pub fn layers(&self) -> Vec<LayerInfo> {
        self.layers
            .iter()
            .filter(|l| !l.chrome)
            .map(|l| LayerInfo {
                name: l.screen.name().to_string(),
                overlay: l.overlay,
                modal: l.modal,
                phase: match l.phase {
                    Phase::Entering(_) => "in",
                    Phase::Active => "active",
                    Phase::Leaving(_) | Phase::Left => "out",
                    Phase::Covering(_) => "covering",
                    Phase::Covered => "covered",
                },
                focused: l.focused.clone(),
            })
            .collect()
    }

    /// The actor named `name` in the topmost layer that has one.
    pub fn actor(&self, name: &str) -> Option<&Actor> {
        self.layers.iter().rev().find_map(|l| {
            let path = hit::find(&l.root, name)?;
            hit::actor_at(&l.root, &path)
        })
    }

    /// Where the actor named `name` is (its box, or its children's),
    /// virtual pixels `[left, top, right, bottom]`.
    pub fn bounds(&self, name: &str) -> Option<[f32; 4]> {
        self.layers.iter().rev().find_map(|l| {
            let path = hit::find(&l.root, name)?;
            let b = hit::bounds_deep(&l.root, &IDENTITY, &path)?;
            Some(self.shifted(l, b))
        })
    }

    /// What of the actor named `name` is on view ([`hit::visible_rect`]:
    /// shown, cut by the clips around it), virtual pixels `[left, top,
    /// right, bottom]`; `None` when hidden or clipped away.
    pub fn visible_bounds(&self, name: &str) -> Option<[f32; 4]> {
        self.layers.iter().rev().find_map(|l| {
            let path = hit::find(&l.root, name)?;
            let b = hit::visible_rect(&l.root, &IDENTITY, &path)?;
            Some(self.shifted(l, b))
        })
    }

    /// A layer's box on the whole screen (below the chrome).
    fn shifted(&self, l: &Layer, b: [f32; 4]) -> [f32; 4] {
        let (k, t) = self.map_for(l.chrome);
        shift(b, k, t)
    }

    /// Where the part `part` of the actor named `owner` is (the first of
    /// that name inside it), virtual pixels `[left, top, right, bottom]`.
    pub fn bounds_of(&self, owner: &str, part: &str) -> Option<[f32; 4]> {
        self.layers.iter().rev().find_map(|l| {
            let mut path = hit::find(&l.root, owner)?;
            let inside = hit::find(hit::actor_at(&l.root, &path)?, part)?;
            path.extend(inside);
            let b = hit::bounds_deep(&l.root, &IDENTITY, &path)?;
            Some(self.shifted(l, b))
        })
    }

    /// The focused actor of the layer that takes focus (the top dialog,
    /// else the top screen).
    pub fn focused(&self) -> Option<&str> {
        let id = self.focus_layer()?;
        self.layer(id)?.focused.as_deref()
    }

    /// Whether the focus ring shows (after a key or pad press, until the
    /// pointer is used).
    pub fn focus_ring_shown(&self) -> bool {
        self.ring_shown
    }

    /// The field being typed into, and the text actor showing it.
    pub fn text_field(&self) -> Option<(&str, &TextField)> {
        self.text.as_ref().map(|t| (t.name.as_str(), &t.field))
    }

    /// The source, longest side and filtering of an actor texture id handed
    /// out by [`Cx::texture`] or [`Cx::blurred_texture`], for the shell's cache.
    pub fn texture_source(&self, id: u32) -> Option<crate::textures::TextureSource> {
        self.textures.borrow().source(id)
    }

    /// How many image sources were asked for so far.
    pub fn texture_count(&self) -> usize {
        self.textures.borrow().len()
    }

    /// What the actors' commands reported (most recent last, at most 64).
    pub fn problems(&self) -> &[String] {
        &self.problems.messages
    }

    // Layers.

    fn layer_ids(&self) -> Vec<u64> {
        self.layers.iter().map(|l| l.id).collect()
    }

    fn index(&self, id: u64) -> Option<usize> {
        self.layers.iter().position(|l| l.id == id)
    }

    fn layer(&self, id: u64) -> Option<&Layer> {
        self.layers.iter().find(|l| l.id == id)
    }

    fn layer_mut(&mut self, id: u64) -> Option<&mut Layer> {
        self.layers.iter_mut().find(|l| l.id == id)
    }

    /// Whether layer `id` is an overlay of `owner`'s, or of one of its
    /// overlays'.
    fn owned_by(&self, id: u64, owner: u64) -> bool {
        let mut at = self.layer(id).and_then(|l| l.owner);
        for _ in 0..self.layers.len() {
            match at {
                Some(o) if o == owner => return true,
                Some(o) => at = self.layer(o).and_then(|l| l.owner),
                None => return false,
            }
        }
        false
    }

    fn screens(&self) -> usize {
        self.layers.iter().filter(|l| !l.overlay).count()
    }

    /// The layers input goes through, topmost first: popups down to the
    /// first dialog or screen, where focus is (the chrome is not).
    fn input_chain(&self) -> Vec<u64> {
        let mut chain = Vec::new();
        for l in self.layers.iter().rev() {
            if l.chrome {
                continue;
            }
            chain.push(l.id);
            if l.modal || !l.overlay {
                break;
            }
        }
        chain
    }

    fn focus_layer(&self) -> Option<u64> {
        self.input_chain().last().copied()
    }

    fn active(&self, id: u64) -> bool {
        self.layer(id)
            .is_some_and(|l| matches!(l.phase, Phase::Active))
    }

    /// Whether layer `id` takes input now.
    fn takes_input(&self, id: u64) -> bool {
        self.active(id)
            && (self.input_chain().contains(&id) || self.layer(id).is_some_and(|l| l.chrome))
    }

    fn cx(&self, id: u64, gesture: bool) -> Cx {
        let mut cx = Cx::new(
            gesture,
            self.viewport_of(id),
            self.time,
            self.layer(id).and_then(|l| l.focused.clone()),
            self.textures.clone(),
        );
        cx.menu_key = self.menu_key;
        cx.menu_source = self.menu_source.clone();
        cx.menu_pad = self.menu_pad;
        cx.by_keys = self.ring_shown;
        cx.current_screen = self
            .layers
            .iter()
            .rev()
            .find(|l| !l.chrome && !l.overlay)
            .is_some_and(|l| l.id == id && !l.phase.going());
        cx
    }

    fn push_layer(
        &mut self,
        mut screen: Box<dyn Screen>,
        overlay: bool,
        modal: bool,
        owner: Option<u64>,
        gesture: bool,
    ) {
        let id = self.next_id;
        self.next_id += 1;
        // A screen or a dialog over the field being typed into ends the
        // typing (a popup, such as its autocomplete, does not).
        if (!overlay || modal) && self.text.take().is_some() {
            self.ime_rect = None;
            self.out.push(UiRequest::StopTextInput);
        }
        let viewport = if self.building_chrome {
            self.viewport
        } else {
            self.inner_viewport()
        };
        let built = [viewport.screen.width, viewport.screen.height];
        let mut cx = Cx::new(gesture, viewport, self.time, None, self.textures.clone());
        let mut root = screen.build(&mut cx);
        let left = start_transition(&mut root, "On", &["In"], &mut self.problems);
        let owner = if overlay { owner } else { None };
        let layer = Layer {
            id,
            screen,
            root,
            phase: if left > 0.0 {
                Phase::Entering(left)
            } else {
                Phase::Active
            },
            then: None,
            leave_with: None,
            focused: None,
            overlay,
            modal,
            owner,
            chrome: false,
            built,
        };
        // An overlay goes directly above its owner and the owner's other
        // overlays; a screen on top of everything but the chrome.
        let at = match owner.and_then(|o| Some((o, self.index(o)?))) {
            Some((o, i)) => {
                let mut j = i + 1;
                while j < self.layers.len() && self.owned_by(self.layers[j].id, o) {
                    j += 1;
                }
                j
            }
            None => self
                .layers
                .iter()
                .position(|l| l.chrome)
                .unwrap_or(self.layers.len()),
        };
        self.cancel_directions();
        self.layers.insert(at, layer);
        self.repeater.clear();
        self.apply(id, cx);
        // The first focusable takes focus, unless the screen chose.
        if self.layer(id).is_some_and(|l| l.focused.is_none())
            && let Some(first) = self
                .layer(id)
                .and_then(|l| l.screen.focusables().into_iter().next())
        {
            self.set_focus(id, Some(first.name), gesture);
        }
        self.prune_inputs();
    }

    /// Starts layer `id`'s leaving, and its overlays'; it goes once its
    /// transition ends.
    fn close(&mut self, id: u64, cancel: bool) {
        let mut problems = Problems::default();
        let Some(layer) = self.layer_mut(id) else {
            return;
        };
        if matches!(layer.phase, Phase::Leaving(_) | Phase::Left) {
            return;
        }
        let left = if cancel {
            let child = if hit::find(&layer.root, "Cancel").is_some() {
                "Cancel"
            } else {
                "Out"
            };
            start_transition(&mut layer.root, "Cancel", &[child], &mut problems)
        } else {
            start_transition(&mut layer.root, "Off", &["Out"], &mut problems)
        };
        layer.phase = Phase::Leaving(left);
        self.problems.messages.extend(problems.messages);
        let owned: Vec<u64> = self
            .layers
            .iter()
            .filter(|l| l.owner == Some(id))
            .map(|l| l.id)
            .collect();
        for o in owned {
            self.close(o, cancel);
        }
        self.release_layer(id);
        if left <= 0.0 {
            self.left(id);
        }
        self.prune_inputs();
    }

    /// Layer `id`'s leaving transition ended: it goes, unless it leaves
    /// with a request ([`Layer::leave_with`]), which goes out now while
    /// the layer stays as it ended for the shell to act on it.
    fn left(&mut self, id: u64) {
        let Some(request) = self.layer_mut(id).and_then(|l| l.leave_with.take()) else {
            self.remove(id);
            return;
        };
        if let Some(l) = self.layer_mut(id) {
            l.phase = Phase::Left;
        }
        self.out.push(request);
        self.dirty = true;
    }

    /// What a leaving layer held: typing, pointers, hover, held keys.
    fn release_layer(&mut self, id: u64) {
        if self.text.as_ref().is_some_and(|t| t.layer == id) {
            self.text = None;
            self.ime_rect = None;
            self.out.push(UiRequest::StopTextInput);
        }
        let held: Vec<u64> = self
            .pointers
            .iter()
            .filter(|p| p.layer == id)
            .map(|p| p.id)
            .collect();
        for p in held {
            self.cancel_pointer(p);
        }
        if self.hover.as_ref().is_some_and(|(l, _)| *l == id) {
            self.hover = None;
        }
        self.repeater.clear();
    }

    /// Takes layer `id` and its overlays away; what replaces it comes in.
    fn remove(&mut self, id: u64) {
        let owned: Vec<u64> = self
            .layers
            .iter()
            .filter(|l| l.owner == Some(id))
            .map(|l| l.id)
            .collect();
        for o in owned {
            self.remove(o);
        }
        self.release_layer(id);
        let removed = self.index(id).map(|i| self.layers.remove(i));
        let Some(removed) = removed else {
            return;
        };
        if let Some(next) = removed.then {
            self.push_layer(next, false, false, None, false);
        } else if !removed.overlay {
            self.uncover();
        }
        self.prune_inputs();
        self.dirty = true;
    }

    /// Cancels the pointers held on layers that no longer take input (a
    /// dialog came over them, they are leaving), and the hover there.
    fn prune_inputs(&mut self) {
        let chain = self.input_chain();
        let stale: Vec<u64> = self
            .pointers
            .iter()
            .filter(|p| !chain.contains(&p.layer) || !self.active(p.layer))
            .map(|p| p.id)
            .collect();
        for p in stale {
            self.cancel_pointer(p);
        }
        if self.hover.as_ref().is_some_and(|(l, _)| !chain.contains(l)) {
            self.hover = None;
        }
    }

    /// Carries out what layer `id`'s screen asked for in a call.
    fn apply(&mut self, id: u64, cx: Cx) {
        let gesture = cx.in_gesture();
        for op in cx.ops {
            self.dirty = true;
            match op {
                Op::Request(r @ (UiRequest::Navigate(_) | UiRequest::Back)) => self.leave_with(r),
                Op::Request(r) => self.emit(id, r, gesture),
                Op::Push(s) => self.push_screen(s, gesture),
                Op::Overlay { screen, modal } => {
                    self.push_layer(screen, true, modal, Some(id), gesture)
                }
                Op::Replace(s) => {
                    if let Some(l) = self.layer_mut(id) {
                        l.then = Some(s);
                    }
                    self.close(id, false);
                }
                Op::Close { cancel } => self.close(id, cancel),
                Op::Broadcast(m) => self.broadcast_inner(&m, gesture),
                Op::Focus(name) => self.set_focus(id, Some(name), gesture),
                Op::EnterChrome => {
                    if self.focus_layer() == Some(id) {
                        self.enter_chrome(gesture);
                    }
                }
                Op::BeginText { name, text, hints } => {
                    self.begin_text(id, name, &text, hints, gesture)
                }
                Op::SetText(s) => {
                    if let Some(t) = self.text.as_mut().filter(|t| t.layer == id) {
                        t.field.set_text(&s);
                        if self.mode == TextMode::Mirror {
                            let text = t.field.text().to_string();
                            let selection = t.field.selection_utf16();
                            self.out.push(UiRequest::SetText { text, selection });
                        }
                    }
                }
                Op::EndText => {
                    if self.text.as_ref().is_some_and(|t| t.layer == id) {
                        self.text = None;
                        self.ime_rect = None;
                        self.out.push(UiRequest::StopTextInput);
                    }
                }
            }
        }
    }

    /// A request from layer `id`: out now, or refused outside a gesture
    /// where one is needed.
    fn emit(&mut self, id: u64, r: UiRequest, gesture: bool) {
        if self.refused(&r, gesture) {
            self.dispatch(id, Input::NeedsGesture(r), false);
        } else {
            self.out.push(r);
        }
    }

    /// Whether `r` needs a gesture this call does not give: none, or a
    /// pad's press for what pads cannot start.
    fn refused(&self, r: &UiRequest, gesture: bool) -> bool {
        self.gestures && r.needs_gesture() && (!gesture || (self.pad_press && !r.pad_may_start()))
    }

    /// Hands `input` to layer `id`'s screen.
    fn dispatch(&mut self, id: u64, input: Input, gesture: bool) -> Handled {
        let mut cx = self.cx(id, gesture);
        let Some(layer) = self.layer_mut(id) else {
            return Handled::No;
        };
        let handled = layer.screen.input(&input, &mut layer.root, &mut cx);
        if handled == Handled::Yes {
            self.consumed = true;
        }
        self.apply(id, cx);
        handled
    }

    fn broadcast_inner(&mut self, message: &Message, gesture: bool) {
        for id in self.layer_ids() {
            let mut cx = self.cx(id, gesture);
            let Some(layer) = self.layer_mut(id) else {
                continue;
            };
            let mut problems = Problems::default();
            message.deliver(&mut layer.root, &mut problems);
            layer.screen.message(message, &mut layer.root, &mut cx);
            self.problems.messages.extend(problems.messages);
            self.apply(id, cx);
        }
    }

    // Focus.

    fn set_focus(&mut self, id: u64, to: Option<String>, gesture: bool) {
        let mut problems = Problems::default();
        let Some(layer) = self.layer_mut(id) else {
            return;
        };
        if layer.focused == to {
            return;
        }
        let from = std::mem::replace(&mut layer.focused, to.clone());
        // StepMania's `LoseFocus` and `GainFocus` commands
        // (`ScreenSelectMaster`, option rows).
        if let Some(a) = from
            .as_deref()
            .and_then(|n| hit::find_mut(&mut layer.root, n))
        {
            a.play("LoseFocus", &mut problems);
        }
        if let Some(a) = to
            .as_deref()
            .and_then(|n| hit::find_mut(&mut layer.root, n))
        {
            a.play("GainFocus", &mut problems);
        }
        self.problems.messages.extend(problems.messages);
        self.dirty = true;
        self.dispatch(id, Input::Focus { from, to }, gesture);
    }

    /// The default for a menu action a screen left.
    /// What a menu action does where the screen left it; whether the
    /// focus moved (or something happened).
    fn menu_default(&mut self, id: u64, action: MenuAction, gesture: bool) -> bool {
        let Some(layer) = self.layer(id) else {
            return false;
        };
        let focusables = layer.screen.focusables();
        let rect = |name: &str| visible_bounds(&layer.root, name);
        // Focus on nothing, or on something hidden or gone: a direction or
        // Start goes to the first focusable shown.
        if (action.is_direction() || action == MenuAction::Start)
            && layer.focused.as_deref().is_none_or(|f| rect(f).is_none())
        {
            if let Some(f) = focusables.iter().find(|f| rect(&f.name).is_some()) {
                self.set_focus(id, Some(f.name.clone()), gesture);
                return true;
            }
            return false;
        }
        let current = layer.focused.clone().unwrap_or_default();
        match action {
            a if a.is_direction() => {
                let next = match focusables.iter().find(|f| f.name == current) {
                    Some(f) if f.neighbour(a).is_some() => f.neighbour(a).map(str::to_string),
                    _ => {
                        let others: Vec<(String, [f32; 4])> = focusables
                            .iter()
                            .filter(|f| f.name != current)
                            .filter_map(|f| Some((f.name.clone(), rect(&f.name)?)))
                            .collect();
                        let rects: Vec<[f32; 4]> = others.iter().map(|(_, r)| *r).collect();
                        rect(&current)
                            .and_then(|from| nav::spatial(from, a, &rects))
                            .map(|i| others[i].0.clone())
                    }
                };
                match next {
                    Some(next) => {
                        self.set_focus(id, Some(next), gesture);
                        true
                    }
                    None => false,
                }
            }
            MenuAction::Start => {
                self.dispatch(id, Input::Activate { name: current }, gesture);
                true
            }
            MenuAction::Back if layer.overlay || self.screens() > 1 => {
                self.close(id, true);
                true
            }
            _ => false,
        }
    }

    // Input.

    fn menu(&mut self, action: MenuAction, repeat: bool, gesture: bool) {
        self.ring_shown = true;
        self.dirty = true;
        // The chrome's actions, while it has the focus: Left and Right
        // between them, Start presses one, Down or Back go back.
        if let Some(c) = self.chrome_focus {
            self.consumed = true;
            if self.dispatch(c, Input::Menu { action, repeat }, gesture) == Handled::Yes {
                return;
            }
            match action {
                MenuAction::Down | MenuAction::Back => self.leave_chrome(gesture),
                MenuAction::Left | MenuAction::Right | MenuAction::Start => {
                    self.menu_default(c, action, gesture);
                }
                _ => {}
            }
            return;
        }
        let chain = self.input_chain();
        // Menu buttons are ours wherever they go (no page scrolling).
        self.consumed |= !chain.is_empty();
        for id in chain {
            if !self.active(id) {
                if self.blocks(id) {
                    return;
                }
                continue;
            }
            if self.dispatch(id, Input::Menu { action, repeat }, gesture) == Handled::Yes {
                return;
            }
            if self.focus_layer() == Some(id) {
                let moved = self.menu_default(id, action, gesture);
                // Up past a screen's top (a fresh press, not a held one):
                // into the chrome, when it has something to do.
                if !moved
                    && action == MenuAction::Up
                    && !repeat
                    && self.layer(id).is_some_and(|l| !l.overlay)
                {
                    self.enter_chrome(gesture);
                }
                return;
            }
        }
    }

    /// Whether input stops at layer `id` (a dialog, a screen).
    fn blocks(&self, id: u64) -> bool {
        self.layer(id).is_some_and(|l| l.modal || !l.overlay)
    }

    fn release_owned_repeat(&mut self, source: &Source) {
        if self
            .focus_layer()
            .and_then(|id| self.layer(id))
            .is_some_and(|l| l.screen.owns_direction_repeat())
        {
            self.repeater.release(source);
        }
    }

    fn menu_release(&mut self, source: Source) {
        for id in self.input_chain() {
            if self.active(id) && self.layer(id).is_some_and(|l| l.screen.direction_edges()) {
                self.dispatch(
                    id,
                    Input::MenuRelease {
                        source: source.clone(),
                    },
                    false,
                );
            }
            if self.blocks(id) {
                break;
            }
        }
    }

    fn cancel_directions(&mut self) {
        for id in self.layer_ids() {
            if self.layer(id).is_some_and(|l| l.screen.direction_edges()) {
                self.dispatch(id, Input::CancelDirections, false);
            }
        }
    }

    fn menu_event(&mut self, e: MenuEvent) {
        let source = Source::Pad(e.source, e.action);
        if e.down {
            self.repeater.press(source.clone(), e.action);
            self.menu_source = Some(source.clone());
            // A pad press starts audio where the platform says so, never
            // what needs transient activation ([`UiRequest::pad_may_start`]).
            self.pad_press = e.gesture;
            self.menu_pad = true;
            self.menu(e.action, false, e.gesture);
            self.release_owned_repeat(&source);
            self.menu_source = None;
            self.menu_pad = false;
            self.pad_press = false;
        } else {
            self.repeater.release(&source);
            if e.action.is_direction() {
                self.menu_release(source);
            }
        }
    }

    /// Whether the screen on top takes keys and controls as they are
    /// ([`Screen::raw_input`]): the shell then sends a controller's edges
    /// as [`UiEvent::Raw`].
    pub fn wants_raw(&self) -> bool {
        self.raw_layer().is_some()
    }

    fn raw_layer(&self) -> Option<u64> {
        let id = self.focus_layer()?;
        let l = self.layer(id)?;
        (matches!(l.phase, Phase::Active) && l.screen.raw_input()).then_some(id)
    }

    fn raw(&mut self, e: ddi_platform::RawInput) {
        // Gone raw between the shell's look and now: dropped.
        if let Some(id) = self.raw_layer() {
            self.consumed = true;
            self.dispatch(id, Input::Raw(e), false);
        }
    }

    fn key(&mut self, k: KeyEvent) {
        // An activation-triggering key press: any but Escape.
        let gesture = k.down && !k.repeat && k.code != "Escape";
        if !k.down {
            self.repeater.release(&Source::Key(k.code.clone()));
            if nav::key_action(&k.code).is_some_and(|a| a.is_direction()) {
                self.menu_release(Source::Key(k.code.clone()));
            }
        }
        // Binding: keys as they are, but never the browser's or the
        // system's shortcuts.
        if let Some(id) = self.raw_layer()
            && !k.modifiers.any_but_shift()
            && !is_function_key(&k.code)
        {
            self.consumed = true;
            if !k.repeat {
                self.repeater.clear();
                self.dispatch(
                    id,
                    Input::Raw(ddi_platform::RawInput {
                        device: ddi_platform::DeviceId::Keyboard,
                        control: k.code.clone(),
                        pressed: k.down,
                        host_time: ddi_platform::HostTime(k.time / 1000.0),
                        slot: 0,
                    }),
                    gesture,
                );
            }
            return;
        }
        if self.typing_layer().is_some() {
            self.text_key(k, gesture);
            return;
        }
        // F2: the chrome's actions (the app's notices), and back; not
        // while a dialog binds keys (it would only take the focus away).
        if k.code == "F2" && k.down && !k.repeat && self.raw_layer().is_none() {
            self.consumed = true;
            if self.chrome_focus.is_some() {
                self.leave_chrome(gesture);
            } else {
                self.enter_chrome(gesture);
            }
            return;
        }
        let action = (!k.modifiers.any_but_shift())
            .then(|| nav::key_action(&k.code))
            .flatten();
        // A screen holding keys (the editor's lanes) hears them let go.
        if !k.down
            && let Some(id) = self.input_chain().into_iter().find(|&id| {
                self.active(id) && self.layer(id).is_some_and(|l| l.screen.key_releases())
            })
        {
            self.dispatch(id, Input::Key(k.clone()), false);
        }
        if !k.down || k.repeat {
            // The OS's repeats of a menu key are ours too (no scrolling).
            self.consumed |= k.repeat && action.is_some() && !self.input_chain().is_empty();
            return;
        }
        if let Some(action) = action {
            let source = Source::Key(k.code.clone());
            self.repeater.press(source.clone(), action);
            self.menu_source = Some(source.clone());
            self.menu_key = true;
            self.menu(action, false, gesture);
            self.release_owned_repeat(&source);
            self.menu_source = None;
            self.menu_key = false;
            return;
        }
        for id in self.input_chain() {
            if !self.active(id) {
                if self.blocks(id) {
                    return;
                }
                continue;
            }
            if self.dispatch(id, Input::Key(k.clone()), gesture) == Handled::Yes || self.blocks(id)
            {
                return;
            }
        }
    }

    /// The layer being typed into, when it takes input.
    fn typing_layer(&self) -> Option<u64> {
        let t = self.text.as_ref()?;
        self.takes_input(t.layer).then_some(t.layer)
    }

    fn text_key(&mut self, k: KeyEvent, gesture: bool) {
        let Some(t) = self.text.as_mut() else { return };
        let layer = t.layer;
        // The keys a field leaves to its screen (an autocomplete list).
        let passes = matches!(
            k.key.as_str(),
            "ArrowUp" | "ArrowDown" | "Tab" | "PageUp" | "PageDown"
        );
        match self.mode {
            TextMode::Mirror => {
                // The IME's (Safari's committing Enter comes with key code
                // 229, which the shell reports as composing).
                if !k.down || k.composing || k.key == "Process" {
                    return;
                }
                match k.key.as_str() {
                    // Never the browser's: it would move focus out of the
                    // hidden textarea. The screen may use it (a completion).
                    "Tab" => {
                        self.consumed = true;
                        self.dispatch(layer, Input::Key(k), gesture);
                    }
                    "Enter" => {
                        self.consumed = true;
                        self.text_changed(TextChange::Submit, gesture);
                    }
                    "Escape" => {
                        self.consumed = true;
                        self.text_changed(TextChange::Cancel, gesture);
                    }
                    _ if passes => {
                        self.dispatch(layer, Input::Key(k), gesture);
                    }
                    // The textarea's.
                    _ => {}
                }
            }
            TextMode::Own => {
                let result = t.field.key(&k);
                self.consumed |= result != KeyResult::Ignored;
                match result {
                    KeyResult::Edited => self.text_changed(TextChange::Edited, gesture),
                    KeyResult::Moved => self.text_changed(TextChange::Moved, gesture),
                    KeyResult::Submit => self.text_changed(TextChange::Submit, gesture),
                    KeyResult::Cancel => self.text_changed(TextChange::Cancel, gesture),
                    KeyResult::Copy { text, cut } => {
                        self.emit(layer, UiRequest::CopyText(text), gesture);
                        if cut {
                            self.text_changed(TextChange::Edited, gesture);
                        }
                    }
                    KeyResult::Paste => self.emit(layer, UiRequest::Paste, gesture),
                    KeyResult::Consumed => {}
                    KeyResult::Ignored => {
                        // Tab stays with the field, as on the web.
                        self.consumed |= k.key == "Tab";
                        if k.down && passes {
                            self.dispatch(layer, Input::Key(k), gesture);
                        }
                    }
                }
            }
        }
    }

    fn text_event(&mut self, e: TextEvent) {
        let mode = self.mode;
        if e == TextEvent::Blurred {
            self.end_typing(false);
            return;
        }
        let Some(t) = self.text.as_mut() else { return };
        self.consumed = true;
        let change = match (e, mode) {
            (TextEvent::Sync(s), TextMode::Mirror) => {
                let before = t.field.text().to_string();
                let was_composing = t.field.is_composing();
                if !t.field.sync(&s) {
                    return;
                }
                // `maxlength` counts UTF-16 units: we count characters.
                // A paste may bring line breaks and tabs: one line, as
                // typing on desktop keeps it; then our length.
                if t.field.strip_controls() | t.field.enforce_max() {
                    let text = t.field.text().to_string();
                    let selection = t.field.selection_utf16();
                    self.out.push(UiRequest::SetText { text, selection });
                }
                // A composition begun or ended is an edit too.
                if t.field.text() == before && t.field.is_composing() == was_composing {
                    TextChange::Moved
                } else {
                    TextChange::Edited
                }
            }
            (TextEvent::Preedit { text, cursor }, TextMode::Own) => {
                if !t.field.set_preedit(&text, cursor) {
                    return;
                }
                TextChange::Edited
            }
            (TextEvent::Commit(text), TextMode::Own) => {
                if !t.field.commit(&text) {
                    return;
                }
                TextChange::Edited
            }
            _ => return,
        };
        self.text_changed(change, false);
    }

    fn text_changed(&mut self, change: TextChange, gesture: bool) {
        let Some(t) = &self.text else { return };
        let (layer, name, field) = (t.layer, t.name.clone(), t.field.clone());
        self.dirty = true;
        self.dispatch(
            layer,
            Input::Text {
                name,
                change,
                field,
            },
            gesture,
        );
    }

    /// Typing ends from outside the screen (the platform's field lost
    /// focus; a press elsewhere, which also asks the shell to stop): the
    /// screen hears [`TextChange::Blurred`] with the text as it was.
    fn end_typing(&mut self, stop: bool) {
        let Some(t) = self.text.take() else { return };
        self.ime_rect = None;
        self.dirty = true;
        if stop {
            self.out.push(UiRequest::StopTextInput);
        }
        self.dispatch(
            t.layer,
            Input::Text {
                name: t.name,
                change: TextChange::Blurred,
                field: t.field,
            },
            false,
        );
    }

    /// Whether a press on `target` of layer `layer` is inside the field
    /// being typed into: on its text actor or an actor named around it,
    /// or on an overlay of its screen (its autocomplete).
    fn inside_field(&self, layer: u64, target: Option<&Target>) -> bool {
        let Some(t) = &self.text else {
            return true;
        };
        if layer != t.layer {
            return self.owned_by(layer, t.layer);
        }
        let Some(target) = target else {
            return false;
        };
        let Some(l) = self.layer(layer) else {
            return false;
        };
        let Some(path) = hit::find(&l.root, &t.name) else {
            return false;
        };
        (1..=path.len())
            .filter_map(|d| hit::actor_at(&l.root, &path[..d]))
            .filter(|a| !a.name.is_empty())
            .any(|a| target.within(&a.name))
    }

    fn begin_text(&mut self, id: u64, name: String, text: &str, hints: InputHints, gesture: bool) {
        let mut field = TextField::new(self.mode, text);
        field.max_chars = hints.max_chars;
        // Where the field was last drawn; the caret's own place follows
        // with the next frame.
        let caret = self
            .layer(id)
            .and_then(|l| {
                // The right edge of the text actor's box (a line high,
                // even while empty).
                let path = hit::find(&l.root, &name)?;
                let a = hit::actor_at(&l.root, &path)?;
                let m = hit::transform_of(&l.root, &IDENTITY, &path)?;
                let [w, h] = a.size();
                let top = hit::apply(&m, [w / 2.0, -h / 2.0]);
                let bottom = hit::apply(&m, [w / 2.0, h / 2.0]);
                Some([
                    top[0].min(bottom[0]),
                    top[1].min(bottom[1]),
                    top[0].max(bottom[0]) + 1.0,
                    top[1].max(bottom[1]),
                ])
            })
            .map_or([0.0, 0.0, 1.0, 16.0], |r| {
                let (k, sh) = self.map_of(id);
                self.css_rect(shift(r, k, sh))
            });
        let request = UiRequest::StartTextInput {
            caret,
            text: field.text().to_string(),
            selection: field.selection_utf16(),
            hints,
        };
        // Only inside a gesture (a phone's keyboard opens only from one);
        // refused, the field does not start.
        if self.refused(&request, gesture) {
            self.dispatch(id, Input::NeedsGesture(request), false);
            return;
        }
        self.out.push(request);
        self.text = Some(ActiveText {
            layer: id,
            name,
            field,
        });
        self.ime_rect = None;
    }

    /// Virtual `[left, top, right, bottom]` as a CSS `[x, y, w, h]`.
    fn css_rect(&self, r: [f32; 4]) -> CssRect {
        let v = &self.viewport;
        let a = v.to_physical([r[0], r[1]]);
        let b = v.to_physical([r[2], r[3]]);
        [
            a[0] / v.ratio,
            a[1] / v.ratio,
            (b[0] - a[0]) / v.ratio,
            (b[1] - a[1]) / v.ratio,
        ]
    }

    fn virtual_per_css(&self) -> f32 {
        self.viewport.ratio / self.viewport.scale
    }

    /// The topmost target under `pos` along the input chain: the layer,
    /// and the target (`None`: a dialog's outside, or a screen's nothing).
    fn pick(&self, pos: [f32; 2]) -> Option<(u64, Option<Target>, bool)> {
        // The chrome's actors first; around them, what is under.
        for l in self.layers.iter().rev().filter(|l| l.chrome) {
            if matches!(l.phase, Phase::Active)
                && let Some(t) = hit::target_at(&l.root, &IDENTITY, pos)
                && !l.screen.lets_through(&t)
            {
                return Some((l.id, Some(t), false));
            }
        }
        // The rest lie below the chrome.
        let (k, t) = self.map_for(false);
        let pos = [(pos[0] - t[0]) / k, (pos[1] - t[1]) / k];
        for id in self.input_chain() {
            let layer = self.layer(id)?;
            if !matches!(layer.phase, Phase::Active) {
                if self.blocks(id) {
                    return None;
                }
                continue;
            }
            if let Some(t) = hit::target_at(&layer.root, &IDENTITY, pos) {
                return Some((id, Some(t), false));
            }
            if layer.modal {
                return Some((id, None, true));
            }
            if !layer.overlay {
                return Some((id, None, false));
            }
        }
        None
    }

    fn pointer(&mut self, e: PointerEvent) {
        let pos = self.viewport.to_virtual(e.pos);
        match e.phase {
            PointerPhase::Down => {
                if e.kind == PointerKind::Mouse && e.buttons & buttons::PRIMARY == 0 {
                    return;
                }
                self.ring_shown = false;
                let gesture = e.kind == PointerKind::Mouse;
                if gesture {
                    self.last_mouse_down = Some(e.time);
                }
                // A pointer id pressed again (a lost release): the old
                // gesture is over.
                self.cancel_pointer(e.id);
                let Some((layer, target, outside)) = self.pick(pos) else {
                    return;
                };
                self.consumed = true;
                if self.chrome_focus.is_some_and(|c| c != layer) {
                    self.leave_chrome(gesture);
                }
                if !self.inside_field(layer, target.as_ref()) {
                    self.end_typing(true);
                }
                if outside {
                    self.dispatch(layer, Input::Outside, gesture);
                    return;
                }
                self.pointers.push(Pointer {
                    id: e.id,
                    layer,
                    target: target.clone(),
                    tracker: Tracker::press(e.kind, e.pos, e.time),
                    kind: e.kind,
                    down: e.time,
                    pos,
                });
                self.deliver_gesture(layer, target, Gesture::Press, pos, e.kind, gesture);
            }
            PointerPhase::Move => {
                if let Some(i) = self.pointers.iter().position(|p| p.id == e.id) {
                    self.consumed = true;
                    let mut g = Vec::new();
                    self.pointers[i].tracker.moved(e.pos, e.time, &mut g);
                    self.pointers[i].pos = pos;
                    let (layer, target) = (self.pointers[i].layer, self.pointers[i].target.clone());
                    for g in g {
                        self.deliver_gesture(layer, target.clone(), g, pos, e.kind, false);
                    }
                } else if e.kind == PointerKind::Mouse {
                    self.ring_shown = false;
                    self.mouse = Some(pos);
                    let now = self.pick(pos).map(|(l, t, _)| (l, t));
                    self.set_hover(now);
                }
            }
            PointerPhase::Up => {
                let Some(i) = self.pointers.iter().position(|p| p.id == e.id) else {
                    return;
                };
                self.consumed = true;
                let mut p = self.pointers.remove(i);
                // The lift after a long press is never a gesture (P2).
                let gesture = !p.tracker.long_pressed()
                    && match e.kind {
                        PointerKind::Mouse => self
                            .last_mouse_down
                            .is_some_and(|t| t == p.down && e.time - t <= ACTIVATION_WINDOW),
                        PointerKind::Touch | PointerKind::Pen => true,
                    };
                let mut g = Vec::new();
                p.tracker.released(e.pos, e.time, &mut g);
                for g in g {
                    self.deliver_gesture(p.layer, p.target.clone(), g, pos, e.kind, gesture);
                }
            }
            PointerPhase::Cancel => self.cancel_pointer(e.id),
            PointerPhase::Leave => {
                self.mouse = None;
                self.set_hover(None);
            }
        }
        if e.kind == PointerKind::Mouse && e.phase != PointerPhase::Leave {
            self.mouse = Some(pos);
        }
        self.refresh_cursor();
    }

    fn cancel_pointer(&mut self, id: u64) {
        if let Some(i) = self.pointers.iter().position(|p| p.id == id) {
            let mut p = self.pointers.remove(i);
            let mut g = Vec::new();
            p.tracker.cancel(&mut g);
            for g in g {
                self.deliver_gesture(p.layer, p.target.clone(), g, p.pos, p.kind, false);
            }
        }
    }

    fn set_hover(&mut self, now: Option<(u64, Option<Target>)>) {
        if self.hover == now {
            return;
        }
        let old = std::mem::replace(&mut self.hover, now.clone());
        if let Some((l, Some(_))) = &old
            && now.as_ref().is_none_or(|(n, _)| n != l)
        {
            self.dispatch(*l, Input::Hover { target: None }, false);
        }
        if let Some((l, target)) = now {
            self.dispatch(l, Input::Hover { target }, false);
        }
    }

    /// A gesture for layer `layer`: only while it takes input, but a
    /// cancel always (to undo a press's look).
    fn deliver_gesture(
        &mut self,
        layer: u64,
        target: Option<Target>,
        gesture: Gesture,
        pos: [f32; 2],
        kind: PointerKind,
        activation: bool,
    ) {
        if gesture != Gesture::Cancel && !self.takes_input(layer) {
            return;
        }
        let k = self.virtual_per_css() / self.map_of(layer).0;
        let pos = self.local(layer, pos);
        let input = Input::Pointer {
            target: target.clone(),
            gesture: gesture.scaled(k),
            pos,
            kind,
        };
        let handled = self.dispatch(layer, input, activation);
        // A tap on a focusable actor (or inside one) focuses and
        // activates it; one on a list's empty space only focuses it.
        if handled == Handled::No
            && gesture == Gesture::Tap
            && let Some(t) = target
            && let Some(f) = self.layer(layer).and_then(|l| {
                let focusables = l.screen.focusables();
                t.chain
                    .iter()
                    .find_map(|n| focusables.iter().find(|f| &f.name == n).cloned())
            })
        {
            self.set_focus(layer, Some(f.name.clone()), activation);
            if !f.list {
                self.dispatch(layer, Input::Activate { name: f.name }, activation);
            }
        }
    }

    fn wheel(&mut self, e: WheelEvent) {
        // With Ctrl or ⌘ a wheel zooms (a trackpad's pinch is one): the
        // page's zoom on the web, nothing of ours.
        if e.modifiers.command() {
            return;
        }
        let kind = self.wheel_stream.classify(&e);
        let pos = self.viewport.to_virtual(e.pos);
        let k = self.virtual_per_css();
        if let Some((layer, target, false)) = self.pick(pos) {
            let k = k / self.map_of(layer).0;
            let delta = [e.delta[0] * k, e.delta[1] * k];
            let pos = self.local(layer, pos);
            self.dispatch(
                layer,
                Input::Wheel {
                    target,
                    delta,
                    pos,
                    kind,
                },
                false,
            );
        }
    }

    /// Focus lost: held input released, captures cancelled.
    fn blur(&mut self) {
        self.cancel_directions();
        self.repeater.clear();
        let ids: Vec<u64> = self.pointers.iter().map(|p| p.id).collect();
        for id in ids {
            self.cancel_pointer(id);
        }
        self.set_hover(None);
    }
}

/// The rectangle of the actor named `name`, if it is shown.
fn visible_bounds(root: &Actor, name: &str) -> Option<[f32; 4]> {
    let path = hit::find(root, name)?;
    if !hit::shown(root, &path) {
        return None;
    }
    hit::bounds_deep(root, &IDENTITY, &path)
}

/// Plays `command` on the root and `StartTransitioning` on the first of
/// `layers` found; how long the tweens this queued last.
fn start_transition(
    root: &mut Actor,
    command: &str,
    layers: &[&str],
    problems: &mut Problems,
) -> f32 {
    root.play(command, problems);
    if let Some(path) = layers.iter().find_map(|n| hit::find(root, n))
        && let Some(layer) = hit::actor_at_mut(root, &path)
    {
        layer.play("StartTransitioning", problems);
    }
    tween_time_left(root, 0)
}

/// `ActorFrame::GetTweenTimeLeft`: the longest of the actor's and its
/// children's.
fn tween_time_left(a: &Actor, depth: usize) -> f32 {
    let own = match a.getter("GetTweenTimeLeft") {
        Some(Value::Num(t)) => t,
        _ => 0.0,
    };
    if depth > 40 {
        return own;
    }
    a.children()
        .iter()
        .map(|c| tween_time_left(c, depth + 1))
        .fold(own, f32::max)
}

/// A solid quad over `[left, top, right, bottom]` under `m`.
fn quad(m: &Affine, r: [f32; 4], color: [f32; 4]) -> Drawn {
    let unit = [
        r[2] - r[0],
        0.0,
        0.0,
        r[3] - r[1],
        (r[0] + r[2]) / 2.0,
        (r[1] + r[3]) / 2.0,
    ];
    Drawn::Quad(Quad {
        transform: hit::mul(m, &unit),
        texture: UNTEXTURED,
        fit: ddi_actor::Fit::Stretch,
        uv: [0.0, 0.0, 1.0, 1.0],
        texcoord_offset: [0.0; 2],
        diffuse: [color; 4],
        glow: [1.0, 1.0, 1.0, 0.0],
        blend: Blend::Normal,
        depth: Depth::default(),
        projected: None,
        shape: ddi_actor::Shape::PLAIN,
        size: [1.0; 2],
        blur: [0.0; 2],
    })
}

fn ring_quads(m: &Affine, b: [f32; 4], out: &mut Vec<Drawn>) {
    let (g, w) = (RING_GAP, RING_WIDTH);
    let o = [b[0] - g - w, b[1] - g - w, b[2] + g + w, b[3] + g + w];
    let i = [b[0] - g, b[1] - g, b[2] + g, b[3] + g];
    for r in [
        [o[0], o[1], o[2], i[1]],
        [o[0], i[3], o[2], o[3]],
        [o[0], i[1], i[0], i[3]],
        [i[2], i[1], o[2], i[3]],
    ] {
        out.push(quad(m, r, RING_COLOR));
    }
}

/// A layer's `[left, top, right, bottom]` on the whole screen: times `k`,
/// plus `t` ([`Ui::map_of`]).
fn shift(r: [f32; 4], k: f32, t: [f32; 2]) -> [f32; 4] {
    [
        r[0] * k + t[0],
        r[1] * k + t[1],
        r[2] * k + t[0],
        r[3] * k + t[1],
    ]
}

/// The field's selection, composition underline and caret over its text
/// actor (at `path`); the caret's rectangle, virtual pixels.
fn field_marks(
    root: &Actor,
    path: &[usize],
    field: &TextField,
    measure: &mut dyn TextMeasure,
    transform: &Affine,
    out: &mut Vec<Drawn>,
) -> Option<[f32; 4]> {
    let a = hit::actor_at(root, path)?;
    let Kind::Text(run) = &a.kind else {
        return None;
    };
    if !hit::shown(root, path) {
        return None;
    }
    let m = hit::transform_of(root, &IDENTITY, path)?;
    let style = run.style().clone();
    let metrics = run.metrics().unwrap_or_default();
    let [w, h] = a.size();
    // The run's space: the pen on the baseline at the box's left
    // (`Actor::draw`).
    let on_target = hit::mul(transform, &m);
    let pen = hit::mul(
        &on_target,
        &[1.0, 0.0, 0.0, 1.0, -w / 2.0, -h / 2.0 + metrics.baseline],
    );
    let l = field.layout(measure, &style);
    let (top, bottom) = (-metrics.baseline, style.line_height() - metrics.baseline);
    let thin = (style.size() * CARET_WIDTH).max(1.0);
    let color = a.current().diffuse[0];
    if let Some([x0, x1]) = l.selection {
        out.push(quad(&pen, [x0, top, x1, bottom], SELECTION_COLOR));
    }
    if let Some([x0, x1]) = l.preedit {
        out.push(quad(&pen, [x0, thin, x1, thin * 2.0], color));
    }
    let caret = [l.caret - thin / 2.0, top, l.caret + thin / 2.0, bottom];
    out.push(quad(&pen, caret, color));
    // The caret's rectangle in virtual pixels, for the IME.
    let in_virtual = hit::mul(
        &m,
        &[1.0, 0.0, 0.0, 1.0, -w / 2.0, -h / 2.0 + metrics.baseline],
    );
    let corners = [[caret[0], caret[1]], [caret[2], caret[3]]].map(|p| hit::apply(&in_virtual, p));
    Some([
        corners[0][0].min(corners[1][0]),
        corners[0][1].min(corners[1][1]),
        corners[0][0].max(corners[1][0]),
        corners[0][1].max(corners[1][1]),
    ])
}

/// `F1`…`F24`: never bound (the browser's and the system's).
fn is_function_key(code: &str) -> bool {
    code.strip_prefix('F')
        .is_some_and(|n| !n.is_empty() && n.bytes().all(|b| b.is_ascii_digit()))
}

/// The pointer's look over `target` on `screen` ([`Ui::refresh_cursor`]).
fn cursor_over(screen: &dyn Screen, target: &Target) -> Cursor {
    if let Some(c) = screen.cursor(target) {
        return c;
    }
    let focusables = screen.focusables();
    let lists: Vec<&str> = focusables
        .iter()
        .filter(|f| f.list)
        .map(|f| f.name.as_str())
        .collect();
    for name in &target.chain {
        // A list's item (`Wheel#3`).
        if name
            .rsplit_once('#')
            .is_some_and(|(list, _)| lists.contains(&list))
        {
            return Cursor::Pointer;
        }
        if let Some(f) = focusables.iter().find(|f| &f.name == name) {
            return if f.list {
                Cursor::Default
            } else {
                Cursor::Pointer
            };
        }
    }
    Cursor::Default
}
