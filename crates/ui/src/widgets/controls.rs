//! The toggle, by focus (keys, pads), mouse and touch, and the press look
//! of what a pointer presses.
//!
//! A toggle is a named component the look builds; focus moves to it by
//! name (it is a [`crate::Focusable`]), Start or a tap activates it (the
//! Ui's [`crate::Input::Activate`]).

use ddi_actor::Actor;

use crate::gesture::Gesture;
use crate::look::{self, Look, Spec, call};
use crate::screen::Input;
use crate::widgets::{on, part, play};

fn place(mut a: Actor, name: &str, centre: [f32; 2]) -> Actor {
    a.name = name.into();
    call(&mut a, "xy", &[centre[0], centre[1]]);
    a
}

/// Press and release looks for a pointer on `name`.
pub(crate) fn press_look(input: &Input, name: &str, root: &mut Actor) {
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

/// A toggle.
pub struct Toggle {
    pub name: String,
    pub label: String,
    /// Centre and size, virtual pixels.
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
