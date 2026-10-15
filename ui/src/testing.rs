//! A headless harness: drives a [`Ui`] with scripted events and frames,
//! natively, and reads back its actors, its requests and what it drew
//! (text measured by [`FixedMeasure`]: every character half the size
//! wide). Positions given to it are virtual pixels, turned into the CSS
//! pixels a shell would send; host time advances with the frames.

use ddi_actor::text::FixedMeasure;
use ddi_actor::{Actor, Drawn};

use crate::event::{
    DeltaMode, KeyEvent, MenuEvent, Modifiers, PointerEvent, PointerKind, PointerPhase, TextEvent,
    TextSync, UiEvent, WheelEvent, buttons, wheel_pixels,
};
use crate::nav::MenuAction;
use crate::request::UiRequest;
use crate::screen::Screen;
use crate::text_field::TextMode;
use crate::ui::Ui;

/// Frames a second [`Harness::run`] steps at.
pub const FPS: f32 = 60.0;

pub struct Harness {
    pub ui: Ui,
    pub measure: FixedMeasure,
    /// Every request so far, in order.
    pub requests: Vec<UiRequest>,
    /// What the last frame drew.
    pub drawn: Vec<Drawn>,
    /// Host time, milliseconds.
    pub now: f64,
    /// Frames drawn.
    pub frames: usize,
    /// Whether the last event sent was consumed ([`crate::Outcome`]).
    pub consumed: bool,
}

impl Harness {
    /// A UI designed at `design` on a surface `size` CSS pixels big at
    /// device pixel ratio `ratio`, requiring gestures as the web does.
    pub fn new(design: [f32; 2], size: [f32; 2], ratio: f32, mode: TextMode) -> Harness {
        Harness::with_gestures(design, size, ratio, mode, true)
    }

    /// [`Harness::new`], saying whether gestures are required (desktop:
    /// no).
    pub fn with_gestures(
        design: [f32; 2],
        size: [f32; 2],
        ratio: f32,
        mode: TextMode,
        gestures: bool,
    ) -> Harness {
        let mut h = Harness {
            ui: Ui::new(design, mode, gestures),
            measure: FixedMeasure,
            requests: Vec::new(),
            drawn: Vec::new(),
            now: 1000.0,
            frames: 0,
            consumed: false,
        };
        h.send(UiEvent::Resize { size, ratio });
        h
    }

    pub fn push(&mut self, screen: impl Screen + 'static) {
        self.ui.push(screen);
        self.frame(0.0);
    }

    /// Sends an event; the requests [`Ui::handle`] returned (also kept in
    /// [`Harness::requests`]; whether it was consumed in
    /// [`Harness::consumed`]).
    pub fn send(&mut self, event: UiEvent) -> Vec<UiRequest> {
        let o = self.ui.handle(event);
        self.consumed = o.consumed;
        self.requests.extend(o.requests.iter().cloned());
        o.requests
    }

    /// One frame of `dt` seconds: update, draw, the frame's requests.
    pub fn frame(&mut self, dt: f32) -> Vec<UiRequest> {
        self.now += f64::from(dt) * 1000.0;
        self.ui.update(dt);
        self.drawn = self.ui.draw(&mut self.measure);
        self.frames += 1;
        let r = self.ui.take_requests();
        self.requests.extend(r.iter().cloned());
        r
    }

    /// Frames at [`FPS`] for `seconds`.
    pub fn run(&mut self, seconds: f32) {
        let n = (seconds * FPS).round() as usize;
        for _ in 0..n {
            self.frame(1.0 / FPS);
        }
    }

    /// Frames at [`FPS`] only while the UI says it needs them, at most
    /// `seconds`; how many were drawn.
    pub fn run_on_demand(&mut self, seconds: f32) -> usize {
        let n = (seconds * FPS).round() as usize;
        let mut drawn = 0;
        for _ in 0..n {
            if self.ui.needs_redraw() {
                self.frame(1.0 / FPS);
                drawn += 1;
            } else {
                self.now += 1000.0 / f64::from(FPS);
            }
        }
        drawn
    }

    /// A virtual point in CSS pixels.
    pub fn css(&self, p: [f32; 2]) -> [f32; 2] {
        let v = self.ui.viewport();
        let ph = v.to_physical(p);
        [ph[0] / v.ratio, ph[1] / v.ratio]
    }

    /// The centre of the actor named `name` (virtual pixels).
    pub fn centre(&self, name: &str) -> [f32; 2] {
        let b = self
            .ui
            .bounds(name)
            .unwrap_or_else(|| panic!("no actor {name:?} with a box"));
        [(b[0] + b[2]) / 2.0, (b[1] + b[3]) / 2.0]
    }

    pub fn actor(&self, name: &str) -> &Actor {
        self.ui
            .actor(name)
            .unwrap_or_else(|| panic!("no actor {name:?}"))
    }

    // Keys and pads.

    pub fn key_down(&mut self, code: &str, key: &str) -> Vec<UiRequest> {
        let mut e = KeyEvent::down(code, key);
        e.time = self.now;
        self.send(UiEvent::Key(e))
    }

    pub fn key_up(&mut self, code: &str, key: &str) -> Vec<UiRequest> {
        let mut e = KeyEvent::up(code, key);
        e.time = self.now;
        self.send(UiEvent::Key(e))
    }

    /// A key pressed with `modifiers` and released.
    pub fn key_with(&mut self, code: &str, key: &str, modifiers: Modifiers) -> Vec<UiRequest> {
        let mut e = KeyEvent::down(code, key);
        e.modifiers = modifiers;
        e.time = self.now;
        let mut r = self.send(UiEvent::Key(e.clone()));
        e.down = false;
        r.extend(self.send(UiEvent::Key(e)));
        r
    }

    /// A key pressed and released.
    pub fn key(&mut self, code: &str, key: &str) -> Vec<UiRequest> {
        self.key_with(code, key, Modifiers::default())
    }

    /// A controller's menu button, pressed (`down`) or released, counted
    /// as a gesture.
    pub fn pad(&mut self, action: MenuAction, down: bool) -> Vec<UiRequest> {
        let e = MenuEvent {
            action,
            down,
            source: 0,
            gesture: true,
            time: self.now,
        };
        self.send(UiEvent::Menu(e))
    }

    // Pointers.

    /// A pointer event at a virtual point.
    pub fn pointer(
        &mut self,
        id: u64,
        kind: PointerKind,
        phase: PointerPhase,
        at: [f32; 2],
    ) -> Vec<UiRequest> {
        let held = matches!(phase, PointerPhase::Down | PointerPhase::Move);
        let e = PointerEvent {
            id,
            kind,
            phase,
            pos: self.css(at),
            buttons: if held { buttons::PRIMARY } else { 0 },
            modifiers: Modifiers::default(),
            time: self.now,
        };
        self.send(UiEvent::Pointer(e))
    }

    /// A mouse moved over a virtual point, no button held.
    pub fn hover(&mut self, at: [f32; 2]) -> Vec<UiRequest> {
        let e = PointerEvent {
            id: 1,
            kind: PointerKind::Mouse,
            phase: PointerPhase::Move,
            pos: self.css(at),
            buttons: 0,
            modifiers: Modifiers::default(),
            time: self.now,
        };
        self.send(UiEvent::Pointer(e))
    }

    /// Pressed and released at the centre of `name` 80 ms apart; what the
    /// release returned.
    pub fn tap(&mut self, kind: PointerKind, name: &str) -> Vec<UiRequest> {
        let at = self.centre(name);
        self.tap_at(kind, at)
    }

    pub fn tap_at(&mut self, kind: PointerKind, at: [f32; 2]) -> Vec<UiRequest> {
        self.pointer(1, kind, PointerPhase::Down, at);
        self.now += 80.0;
        self.pointer(1, kind, PointerPhase::Up, at)
    }

    /// A drag from `from` to `to` (virtual) in `steps` moves over
    /// `seconds`, a frame between moves, released at the end.
    pub fn drag(
        &mut self,
        kind: PointerKind,
        from: [f32; 2],
        to: [f32; 2],
        seconds: f32,
        steps: usize,
    ) {
        self.pointer(1, kind, PointerPhase::Down, from);
        for i in 1..=steps {
            let t = i as f32 / steps as f32;
            self.frame(seconds / steps as f32);
            let at = [
                from[0] + (to[0] - from[0]) * t,
                from[1] + (to[1] - from[1]) * t,
            ];
            self.pointer(1, kind, PointerPhase::Move, at);
        }
        self.pointer(1, kind, PointerPhase::Up, to);
    }

    /// A wheel at a virtual point, `lines` of it (Firefox's unit).
    pub fn wheel_lines(&mut self, at: [f32; 2], lines: [f32; 2]) -> Vec<UiRequest> {
        let e = WheelEvent {
            pos: self.css(at),
            delta: wheel_pixels(lines, DeltaMode::Line, [0.0; 2]),
            modifiers: Modifiers::default(),
            time: self.now,
        };
        self.send(UiEvent::Wheel(e))
    }

    // Text.

    /// The web's textarea state: `text`, the selection and composition in
    /// UTF-16 units.
    pub fn sync(&mut self, text: &str, selection: [usize; 2], composition: Option<[usize; 2]>) {
        self.send(UiEvent::Text(TextEvent::Sync(TextSync {
            text: text.into(),
            selection,
            composition,
        })));
    }

    // Reading back.

    /// The text runs the last frame drew, in order.
    pub fn texts(&self) -> Vec<String> {
        self.drawn
            .iter()
            .filter_map(|d| match d {
                Drawn::Text(t) => Some(t.text.to_string()),
                _ => None,
            })
            .collect()
    }

    /// The quads the last frame drew in `color` (the focus ring's, the
    /// caret's).
    pub fn quads_of(&self, color: [f32; 4]) -> Vec<[f32; 6]> {
        self.drawn
            .iter()
            .filter_map(|d| match d {
                Drawn::Quad(q) if q.diffuse[0] == color => Some(q.transform),
                _ => None,
            })
            .collect()
    }

    /// The requests so far that match.
    pub fn requested(&self, f: impl Fn(&UiRequest) -> bool) -> usize {
        self.requests.iter().filter(|r| f(r)).count()
    }
}
