//! Widgets (`docs/plans/ui-core.md`, step 4): plain Rust state over named
//! actors that a [`crate::look::Look`] builds. A screen keeps its widgets,
//! puts what they build into its tree, hands them the inputs, frames and
//! the tree (`input`, `update`), and reads back what happened. Widgets
//! never draw: they show, move and fill their component's parts and play
//! its commands, so a theme changes how they look and not how they work.
//!
//! - [`list`]: a virtual list (rows built only for what shows, reused),
//!   with section headers, inertia, snapping, a scrollbar and a wheel
//!   style.
//! - [`controls`]: buttons, toggles, option rows, sliders.
//! - [`field`]: a text field with an autocomplete popup.

pub mod controls;
pub mod field;
pub mod list;

use ddi_actor::{Actor, Kind};

use crate::hit;

/// The part named `part` inside `widget` (not elsewhere in the screen).
pub fn part<'a>(widget: &'a mut Actor, part: &str) -> Option<&'a mut Actor> {
    hit::find_mut(widget, part)
}

/// Sets the text of part `part` of `widget`.
pub fn set_text(widget: &mut Actor, part_name: &str, s: &str) {
    if let Some(a) = part(widget, part_name)
        && let Kind::Text(t) = &mut a.kind
    {
        t.set_text(s);
    }
}

/// Puts actor texture `texture` (from [`crate::Cx::texture`]) on sprite
/// part `part`, shown; `None` takes it off and hides the part (the
/// placeholder under it shows).
pub fn set_image(widget: &mut Actor, part_name: &str, texture: Option<u32>) {
    if let Some(a) = part(widget, part_name) {
        let shown = texture.is_some();
        if let Kind::Sprite(s) = &mut a.kind {
            s.texture = texture.unwrap_or(ddi_actor::UNTEXTURED);
        }
        a.visible = shown;
    }
}

/// Plays `command` on `widget` (and everything in it).
pub fn play(widget: &mut Actor, command: &str) {
    widget.play(command, &mut ddi_actor::Problems::default());
}

/// Whether `input` is a pointer's gesture on `name` (or inside it).
pub(crate) fn on<'a>(input: &'a crate::Input, name: &str) -> Option<&'a crate::Gesture> {
    match input {
        crate::Input::Pointer {
            target: Some(t),
            gesture,
            ..
        } if t.within(name) => Some(gesture),
        _ => None,
    }
}
