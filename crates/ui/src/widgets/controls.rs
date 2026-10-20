//! Buttons, toggles, option rows and sliders, by focus (keys, pads),
//! mouse and touch.
//!
//! Each is a named component the look builds; focus moves to it by name
//! (it is a [`crate::Focusable`]), Start or a tap activates it (the Ui's
//! [`crate::Input::Activate`]). Option rows are StepMania's: a row of
//! choices, Left and Right change the chosen one while the row has focus
//! (the screen hands it the menu actions), a tap picks one. A slider moves
//! by a step with Left and Right and follows a press or a drag along its
//! track.

use ddi_actor::{Actor, Kind};

use crate::gesture::Gesture;
use crate::look::{self, Look, Spec, call};
use crate::nav::MenuAction;
use crate::screen::{Handled, Input};
use crate::widgets::{on, part, play, set_text};

fn place(mut a: Actor, name: &str, centre: [f32; 2]) -> Actor {
    a.name = name.into();
    call(&mut a, "xy", &[centre[0], centre[1]]);
    a
}

/// Press and release looks for a pointer on `name`.
fn press_look(input: &Input, name: &str, root: &mut Actor) {
    if let Some(g) = on(input, name)
        && let Some(a) = crate::hit::find_mut(root, name)
    {
        match g {
            Gesture::Press => play(a, "Press"),
            Gesture::Tap
            | Gesture::Cancel
            | Gesture::Release
            | Gesture::DragStart
            | Gesture::LongPress => play(a, "Release"),
            _ => {}
        }
    }
}

/// A button.
pub struct Button {
    pub name: String,
    pub label: String,
    /// Centre and size, virtual pixels.
    pub centre: [f32; 2],
    pub size: [f32; 2],
}

impl Button {
    pub fn new(name: &str, label: &str, centre: [f32; 2], size: [f32; 2]) -> Button {
        Button {
            name: name.into(),
            label: label.into(),
            centre,
            size,
        }
    }

    pub fn build(&self, look: &dyn Look) -> Actor {
        let a = look::component(look, "Button", &Spec::new(self.size, &self.label));
        place(a, &self.name, self.centre)
    }

    /// Whether it was activated.
    pub fn input(&mut self, input: &Input, root: &mut Actor) -> bool {
        press_look(input, &self.name, root);
        matches!(input, Input::Activate { name } if *name == self.name)
    }
}

/// A toggle.
pub struct Toggle {
    pub name: String,
    pub label: String,
    pub centre: [f32; 2],
    pub size: [f32; 2],
    pub on: bool,
}

impl Toggle {
    pub fn new(name: &str, label: &str, centre: [f32; 2], size: [f32; 2], on: bool) -> Toggle {
        Toggle {
            name: name.into(),
            label: label.into(),
            centre,
            size,
            on,
        }
    }

    pub fn build(&self, look: &dyn Look) -> Actor {
        let mut a = look::component(look, "Toggle", &Spec::new(self.size, &self.label));
        if let Some(m) = part(&mut a, "Mark") {
            play(m, if self.on { "On" } else { "Off" });
            m.finish_tweening();
        }
        place(a, &self.name, self.centre)
    }

    /// The new state when it was flipped.
    pub fn input(&mut self, input: &Input, root: &mut Actor) -> Option<bool> {
        press_look(input, &self.name, root);
        if !matches!(input, Input::Activate { name } if *name == self.name) {
            return None;
        }
        self.on = !self.on;
        if let Some(a) = crate::hit::find_mut(root, &self.name)
            && let Some(m) = part(a, "Mark")
        {
            play(m, if self.on { "On" } else { "Off" });
        }
        Some(self.on)
    }
}

/// A row of choices.
pub struct OptionRow {
    pub name: String,
    pub label: String,
    pub centre: [f32; 2],
    pub size: [f32; 2],
    pub choices: Vec<String>,
    pub index: usize,
    /// Where the choices go, from the row's centre: `[left, span]`, from
    /// the component's `Choices` part when it has one.
    area: std::cell::Cell<[f32; 2]>,
}

impl OptionRow {
    pub fn new(
        name: &str,
        label: &str,
        centre: [f32; 2],
        size: [f32; 2],
        choices: &[&str],
        index: usize,
    ) -> OptionRow {
        OptionRow {
            name: name.into(),
            label: label.into(),
            centre,
            size,
            choices: choices.iter().map(|c| c.to_string()).collect(),
            index: index.min(choices.len().saturating_sub(1)),
            area: std::cell::Cell::new([-size[0] / 6.0, size[0] * 2.0 / 3.0 - 16.0]),
        }
    }

    /// Where choice `i` is, from the row's centre: spread over the area.
    fn choice_x(&self, i: usize) -> f32 {
        let [left, span] = self.area.get();
        let n = self.choices.len().max(1) as f32;
        left + span * (i as f32 + 0.5) / n
    }

    pub fn build(&self, look: &dyn Look) -> Actor {
        let mut a = look::component(look, "OptionRow", &Spec::new(self.size, &self.label));
        // The look's place for the choices: its `Choices` part.
        if let Some(c) = part(&mut a, "Choices") {
            let span = c.size()[0] * c.current().scale[0];
            self.area.set([c.current().pos[0] - span / 2.0, span]);
        }
        let slot = self.area.get()[1] / self.choices.len().max(1) as f32;
        let mut extra = Vec::new();
        for (i, c) in self.choices.iter().enumerate() {
            let mut t = look::component(
                look,
                "OptionRow.Choice",
                &Spec::new([slot, self.size[1]], c),
            );
            t.name = format!("{}.Choice{i}", self.name);
            if let Kind::Text(text) = &mut t.kind {
                text.max_width = slot - 6.0;
            }
            call(&mut t, "x", &[self.choice_x(i)]);
            extra.push(t);
        }
        let mut cursor =
            look::component(look, "OptionRow.Cursor", &Spec::new([slot - 8.0, 2.0], ""));
        cursor.name = "Cursor".into();
        call(
            &mut cursor,
            "xy",
            &[self.choice_x(self.index), self.size[1] / 2.0 - 5.0],
        );
        extra.push(cursor);
        if let Kind::Frame(children) = &mut a.kind {
            children.extend(extra);
        }
        place(a, &self.name, self.centre)
    }

    fn show(&self, root: &mut Actor) {
        let x = self.choice_x(self.index);
        if let Some(a) = crate::hit::find_mut(root, &self.name)
            && let Some(c) = part(a, "Cursor")
        {
            look::run(c, &format!("stoptweening;decelerate,0.1;x,{x}"));
        }
    }

    /// The new choice when it changed. Left and Right only reach it while
    /// it has focus (the screen's call).
    pub fn input(
        &mut self,
        input: &Input,
        root: &mut Actor,
        focused: bool,
    ) -> (Handled, Option<usize>) {
        let before = self.index;
        let handled = match input {
            Input::Menu {
                action: MenuAction::Left,
                ..
            } if focused => {
                self.index = self.index.saturating_sub(1);
                Handled::Yes
            }
            Input::Menu {
                action: MenuAction::Right,
                ..
            } if focused => {
                self.index = (self.index + 1).min(self.choices.len().saturating_sub(1));
                Handled::Yes
            }
            Input::Pointer {
                target: Some(t),
                gesture: Gesture::Tap,
                ..
            } if t.within(&self.name) => {
                let prefix = format!("{}.Choice", self.name);
                if let Some(i) = t
                    .chain
                    .iter()
                    .find_map(|n| n.strip_prefix(&prefix)?.parse::<usize>().ok())
                {
                    self.index = i;
                }
                // The tap also focuses the row (the Ui's default).
                Handled::No
            }
            _ => Handled::No,
        };
        if self.index != before {
            self.show(root);
            return (handled, Some(self.index));
        }
        (handled, None)
    }
}

/// A slider over `0..=1`.
pub struct Slider {
    pub name: String,
    pub label: String,
    pub centre: [f32; 2],
    pub size: [f32; 2],
    pub value: f32,
    pub step: f32,
    dragging: bool,
    /// The track's left end and length from the centre: the component's
    /// `Track` part.
    track: std::cell::Cell<(f32, f32)>,
}

impl Slider {
    pub fn new(name: &str, label: &str, centre: [f32; 2], size: [f32; 2], value: f32) -> Slider {
        Slider {
            name: name.into(),
            label: label.into(),
            centre,
            size,
            value: value.clamp(0.0, 1.0),
            step: 0.05,
            dragging: false,
            track: std::cell::Cell::new((-size[0] / 4.0, size[0] / 2.0)),
        }
    }

    fn track(&self) -> (f32, f32) {
        self.track.get()
    }

    pub fn build(&self, look: &dyn Look) -> Actor {
        let mut a = look::component(look, "Slider", &Spec::new(self.size, &self.label));
        // The track as the look placed it (centred on its x).
        if let Some(t) = part(&mut a, "Track") {
            let len = t.size()[0] * t.current().scale[0];
            self.track.set((t.current().pos[0] - len / 2.0, len));
        }
        let mut a = place(a, &self.name, self.centre);
        self.show_in(&mut a);
        a
    }

    fn show_in(&self, a: &mut Actor) {
        let (left, len) = self.track();
        if let Some(f) = part(a, "Fill") {
            call(f, "zoomx", &[self.value]);
        }
        if let Some(k) = part(a, "Knob") {
            call(k, "x", &[left + len * self.value]);
        }
        set_text(a, "Value", &format!("{:.0} %", self.value * 100.0));
    }

    fn set(&mut self, v: f32, root: &mut Actor) -> Option<f32> {
        let v = (v.clamp(0.0, 1.0) * 1000.0).round() / 1000.0;
        if v == self.value {
            return None;
        }
        self.value = v;
        if let Some(a) = crate::hit::find_mut(root, &self.name) {
            self.show_in(a);
        }
        Some(v)
    }

    /// The new value when it changed.
    pub fn input(
        &mut self,
        input: &Input,
        root: &mut Actor,
        focused: bool,
    ) -> (Handled, Option<f32>) {
        match input {
            Input::Menu {
                action: MenuAction::Left,
                ..
            } if focused => (Handled::Yes, self.set(self.value - self.step, root)),
            Input::Menu {
                action: MenuAction::Right,
                ..
            } if focused => (Handled::Yes, self.set(self.value + self.step, root)),
            Input::Pointer {
                target: Some(t),
                gesture,
                pos,
                ..
            } if t.within(&self.name) => {
                let (left, len) = self.track();
                // Along the slider, wherever it is placed.
                let x = crate::hit::local_point(root, &crate::hit::IDENTITY, &self.name, *pos)
                    .map_or(pos[0] - self.centre[0], |p| p[0]);
                let v = (x - left) / len.max(1.0);
                match gesture {
                    Gesture::Press => {
                        // On the track (or near it): follow the pointer.
                        if x >= left - 12.0 && x <= left + len + 12.0 {
                            self.dragging = true;
                            return (Handled::Yes, self.set(v, root));
                        }
                        (Handled::No, None)
                    }
                    Gesture::Drag { .. } | Gesture::DragStart if self.dragging => {
                        (Handled::Yes, self.set(v, root))
                    }
                    Gesture::Tap | Gesture::DragEnd { .. } | Gesture::Flick { .. }
                        if self.dragging =>
                    {
                        self.dragging = false;
                        (Handled::Yes, None)
                    }
                    Gesture::Cancel | Gesture::Release => {
                        self.dragging = false;
                        (Handled::No, None)
                    }
                    _ => (Handled::No, None),
                }
            }
            _ => (Handled::No, None),
        }
    }
}
