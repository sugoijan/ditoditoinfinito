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
//! command cannot hold a screen forever.

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
use crate::request::{CssRect, InputHints, UiRequest};
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
/// thick, this colour (virtual pixels, encoded RGBA).
pub const RING_GAP: f32 = 3.0;
pub const RING_WIDTH: f32 = 2.0;
pub const RING_COLOR: [f32; 4] = [1.0, 0.78, 0.2, 1.0];
/// The caret and the composition's underline, as a fraction of the text's
/// size (at least a virtual pixel), and the selection's colour.
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
}

struct Layer {
    id: u64,
    screen: Box<dyn Screen>,
    root: Actor,
    phase: Phase,
    /// What comes in once this one has left ([`Cx::replace`]).
    then: Option<Box<dyn Screen>>,
    focused: Option<String>,
    overlay: bool,
    modal: bool,
    /// An overlay's: the layer that opened it.
    owner: Option<u64>,
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
    /// `"in"`, `"active"` or `"out"`.
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
    repeater: Repeater,
    ring_shown: bool,
    draw_ring: bool,
    text: Option<ActiveText>,
    ime_rect: Option<CssRect>,
    last_mouse_down: Option<f64>,
    time: f64,
    dirty: bool,
    problems: Problems,
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
            repeater: Repeater::default(),
            ring_shown: false,
            draw_ring: true,
            text: None,
            ime_rect: None,
            last_mouse_down: None,
            time: 0.0,
            dirty: true,
            problems: Problems::default(),
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

    /// Pushes a screen (the first one, at boot).
    pub fn push(&mut self, screen: impl Screen + 'static) {
        self.push_layer(Box::new(screen), false, false, None, false);
        self.dirty = true;
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
            UiEvent::Focus(true) => {}
            UiEvent::Focus(false) => self.blur(),
            UiEvent::Resize { size, ratio } => {
                self.viewport = Viewport::new(self.design, size, ratio);
                self.ime_rect = None;
            }
        }
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
            let mut problems = Problems::default();
            layer.root.update(dt, &clocks, &mut problems);
            layer.screen.update(dt, &mut layer.root, &mut cx);
            let mut entered = false;
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
                Phase::Active => false,
            };
            if entered {
                layer.phase = Phase::Active;
            }
            self.problems.messages.extend(problems.messages);
            self.apply(id, cx);
            if done {
                self.remove(id);
            }
        }
        self.problems.messages.truncate(64);
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
        let text = &mut self.text;
        for layer in self.layers.iter_mut() {
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
                ime = field_marks(&layer.root, path, &t.field, measure, &transform, &mut out);
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
    /// a held direction may repeat or a press may become a long press.
    /// Otherwise the shell may stop its frames until the next event (P13).
    pub fn needs_redraw(&self) -> bool {
        let top = self.layers.iter().rposition(|l| !l.overlay).unwrap_or(0);
        self.dirty
            || self.repeater.holding()
            || self.pointers.iter().any(|p| p.tracker.waiting())
            || self
                .layers
                .iter()
                .any(|l| !matches!(l.phase, Phase::Active))
            || self.layers[top.min(self.layers.len())..]
                .iter()
                .any(|l| l.screen.animating() || l.root.in_motion())
    }

    /// The layers, bottom first.
    pub fn layers(&self) -> Vec<LayerInfo> {
        self.layers
            .iter()
            .map(|l| LayerInfo {
                name: l.screen.name().to_string(),
                overlay: l.overlay,
                modal: l.modal,
                phase: match l.phase {
                    Phase::Entering(_) => "in",
                    Phase::Active => "active",
                    Phase::Leaving(_) => "out",
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
            hit::bounds_deep(&l.root, &IDENTITY, &path)
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

    /// The source and the longest side (virtual pixels) of an actor
    /// texture id handed out by [`Cx::texture`], for the shell's cache.
    pub fn texture_source(&self, id: u32) -> Option<(std::sync::Arc<str>, f32)> {
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
    /// first dialog or screen, where focus is.
    fn input_chain(&self) -> Vec<u64> {
        let mut chain = Vec::new();
        for l in self.layers.iter().rev() {
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
        self.active(id) && self.input_chain().contains(&id)
    }

    fn cx(&self, id: u64, gesture: bool) -> Cx {
        Cx::new(
            gesture,
            self.viewport,
            self.time,
            self.layer(id).and_then(|l| l.focused.clone()),
            self.textures.clone(),
        )
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
        let mut cx = Cx::new(
            gesture,
            self.viewport,
            self.time,
            None,
            self.textures.clone(),
        );
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
            focused: None,
            overlay,
            modal,
            owner,
        };
        // An overlay goes directly above its owner and the owner's other
        // overlays; a screen on top of everything.
        let at = match owner.and_then(|o| Some((o, self.index(o)?))) {
            Some((o, i)) => {
                let mut j = i + 1;
                while j < self.layers.len() && self.owned_by(self.layers[j].id, o) {
                    j += 1;
                }
                j
            }
            None => self.layers.len(),
        };
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
        if matches!(layer.phase, Phase::Leaving(_)) {
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
            self.remove(id);
        }
        self.prune_inputs();
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
        let then = self.index(id).and_then(|i| self.layers.remove(i).then);
        if let Some(next) = then {
            self.push_layer(next, false, false, None, false);
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
                Op::Request(r) => self.emit(id, r, gesture),
                Op::Push(s) => self.push_layer(s, false, false, None, gesture),
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
    fn menu_default(&mut self, id: u64, action: MenuAction, gesture: bool) {
        let Some(layer) = self.layer(id) else {
            return;
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
            }
            return;
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
                if let Some(next) = next {
                    self.set_focus(id, Some(next), gesture);
                }
            }
            MenuAction::Start => {
                self.dispatch(id, Input::Activate { name: current }, gesture);
            }
            MenuAction::Back if layer.overlay || self.screens() > 1 => self.close(id, true),
            _ => {}
        }
    }

    // Input.

    fn menu(&mut self, action: MenuAction, repeat: bool, gesture: bool) {
        self.ring_shown = true;
        self.dirty = true;
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
                self.menu_default(id, action, gesture);
                return;
            }
        }
    }

    /// Whether input stops at layer `id` (a dialog, a screen).
    fn blocks(&self, id: u64) -> bool {
        self.layer(id).is_some_and(|l| l.modal || !l.overlay)
    }

    fn menu_event(&mut self, e: MenuEvent) {
        let source = Source::Pad(e.source, e.action);
        if e.down {
            self.repeater.press(source, e.action);
            // A pad press starts audio where the platform says so, never
            // what needs transient activation ([`UiRequest::pad_may_start`]).
            self.pad_press = e.gesture;
            self.menu(e.action, false, e.gesture);
            self.pad_press = false;
        } else {
            self.repeater.release(&source);
        }
    }

    fn key(&mut self, k: KeyEvent) {
        // An activation-triggering key press: any but Escape.
        let gesture = k.down && !k.repeat && k.code != "Escape";
        if !k.down {
            self.repeater.release(&Source::Key(k.code.clone()));
        }
        if self.typing_layer().is_some() {
            self.text_key(k, gesture);
            return;
        }
        let action = (!k.modifiers.any_but_shift())
            .then(|| nav::key_action(&k.code))
            .flatten();
        if !k.down || k.repeat {
            // The OS's repeats of a menu key are ours too (no scrolling).
            self.consumed |= k.repeat && action.is_some() && !self.input_chain().is_empty();
            return;
        }
        if let Some(action) = action {
            self.repeater.press(Source::Key(k.code.clone()), action);
            self.menu(action, false, gesture);
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
            .map_or([0.0, 0.0, 1.0, 16.0], |r| self.css_rect(r));
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
            PointerPhase::Leave => self.set_hover(None),
        }
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
        let k = self.virtual_per_css();
        let input = Input::Pointer {
            target: target.clone(),
            gesture: gesture.scaled(k),
            pos,
            kind,
        };
        let handled = self.dispatch(layer, input, activation);
        // A tap on a focusable actor (or inside one) focuses and
        // activates it.
        if handled == Handled::No
            && gesture == Gesture::Tap
            && let Some(t) = target
            && let Some(name) = self.layer(layer).and_then(|l| {
                let names: Vec<String> =
                    l.screen.focusables().into_iter().map(|f| f.name).collect();
                t.chain.iter().find(|n| names.contains(n)).cloned()
            })
        {
            self.set_focus(layer, Some(name.clone()), activation);
            self.dispatch(layer, Input::Activate { name }, activation);
        }
    }

    fn wheel(&mut self, e: WheelEvent) {
        let pos = self.viewport.to_virtual(e.pos);
        let k = self.virtual_per_css();
        if let Some((layer, target, false)) = self.pick(pos) {
            let delta = [e.delta[0] * k, e.delta[1] * k];
            self.dispatch(layer, Input::Wheel { target, delta, pos }, false);
        }
    }

    /// Focus lost: held input released, captures cancelled.
    fn blur(&mut self) {
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
        uv: [0.0, 0.0, 1.0, 1.0],
        texcoord_offset: [0.0; 2],
        diffuse: [color; 4],
        glow: [1.0, 1.0, 1.0, 0.0],
        blend: Blend::Normal,
        depth: Depth::default(),
        projected: None,
        shape: ddi_actor::Shape::PLAIN,
        size: [1.0; 2],
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
