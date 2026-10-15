//! What widgets look like, behind a small seam (`docs/plans/ui-core.md`,
//! P6): a widget asks the [`Look`] for its component by name (`Button`,
//! `List.Row`, `Slider`, …) and gets actors back; a look that has no such
//! component falls back to [`Plain`], the built-in dark look, as noteskins
//! fall back to `default`. Widgets never draw themselves: they move,
//! show and fill the component's named parts and play commands on it
//! (`GainFocus`, `LoseFocus`, `Press`, `Release`, `Select`, `Deselect`,
//! `On`, `Off`), as StepMania's screens drive a theme's actors. A theme
//! (step 5) is a `Look` whose components carry its art and commands.
//!
//! Parts by component (the names widgets look for inside it; a look may
//! add others):
//!
//! - `Button`: `Plate`, `Label`.
//! - `Toggle`: `Plate`, `Label`, `Box`, `Mark`.
//! - `OptionRow`: `Plate`, `Label`; `OptionRow.Choice`: a text;
//!   `OptionRow.Cursor`: under the chosen one.
//! - `Slider`: `Plate`, `Label`, `Track`, `Fill`, `Knob`, `Value`.
//! - `List.Row`: `Plate`, `Banner` (a sprite the row's image is put on, a
//!   placeholder until it loads), `Title`, `Subtitle`; `List.Header`:
//!   `Plate`, `Title`; `List.Bar`: `Track`, `Thumb`.
//! - `TextField`: `Box`, `Hint` (the text actor is the widget's);
//!   `Popup.Item`: `Plate`, `Text`, `Note`.
//! - `Panel`: `Plate`, `Title`.
//!
//! Sizes are virtual pixels; components are centred on their origin,
//! except list rows and headers, whose origin is their top left.

use std::sync::Arc;

use ddi_actor::commands::{Arg, Call, parse};
use ddi_actor::text::TextStyle;
use ddi_actor::{Actor, Kind, Problems, Rgba, Sprite, Text};

/// What a component is built for.
#[derive(Clone, Debug, PartialEq)]
pub struct Spec {
    pub size: [f32; 2],
    /// Its label or first text.
    pub text: String,
}

impl Spec {
    pub fn new(size: [f32; 2], text: impl Into<String>) -> Spec {
        Spec {
            size,
            text: text.into(),
        }
    }
}

/// Where a wheel puts a row `distance` rows from the selected one.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct WheelPlace {
    /// Moved right, virtual pixels.
    pub x: f32,
    pub zoom: f32,
    pub alpha: f32,
}

/// A look: components by name.
pub trait Look {
    /// Component `name` built for `spec`; `None` to use the plain one.
    fn component(&self, _name: &str, _spec: &Spec) -> Option<Actor> {
        None
    }

    /// A named colour (`Graph.Line`, `Graph.Bar`, `Text`, `Accent`, …).
    fn color(&self, _name: &str) -> Option<Rgba> {
        None
    }

    /// The font stack.
    fn font(&self) -> Option<Arc<str>> {
        None
    }

    /// Where wheel-style list `list` puts a row `distance` rows from the
    /// selected one.
    fn wheel(&self, _list: &str, _distance: f32) -> Option<WheelPlace> {
        None
    }
}

/// Component `name` from `look`, else [`Plain`]'s (in the look's font),
/// else an empty frame.
pub fn component(look: &dyn Look, name: &str, spec: &Spec) -> Actor {
    look.component(name, spec)
        .or_else(|| plain(name, spec, &font(look)))
        .unwrap_or_else(|| Actor::new(Kind::Frame(Vec::new())))
}

/// Component `name` (`SongList.Row`) from `look`, else its `fallback`
/// (`List.Row`) from it, else [`Plain`]'s fallback.
pub fn component_or(look: &dyn Look, name: &str, fallback: &str, spec: &Spec) -> Actor {
    look.component(name, spec)
        .unwrap_or_else(|| component(look, fallback, spec))
}

pub fn color(look: &dyn Look, name: &str) -> Rgba {
    look.color(name)
        .or_else(|| Plain.color(name))
        .unwrap_or([1.0; 4])
}

pub fn font(look: &dyn Look) -> Arc<str> {
    look.font().unwrap_or_else(|| PLAIN_FONT.into())
}

pub fn wheel(look: &dyn Look, list: &str, distance: f32) -> WheelPlace {
    look.wheel(list, distance)
        .or_else(|| Plain.wheel(list, distance))
        .unwrap_or(WheelPlace {
            x: 0.0,
            zoom: 1.0,
            alpha: 1.0,
        })
}

/// The built-in look: plates on black, white text, one blue accent;
/// close to today's pages.
#[derive(Clone, Copy, Debug, Default)]
pub struct Plain;

const PLAIN_FONT: &str = "system-ui, sans-serif";

const PLATE: Rgba = [0.11, 0.12, 0.15, 1.0];
const LIT: Rgba = [0.22, 0.33, 0.72, 1.0];
const TEXT: Rgba = [0.95, 0.95, 0.95, 1.0];
const MUTED: Rgba = [0.6, 0.62, 0.68, 1.0];

/// Runs a command string on an actor (the plain look's own; ignores what
/// fails).
pub(crate) fn run(a: &mut Actor, commands: &str) {
    let mut p = Problems::default();
    if let Ok(calls) = parse(commands) {
        for c in calls {
            let _ = a.call(&c, &mut p, 0);
        }
    }
}

/// [`call`] for tests and shells: a method with numbers.
#[doc(hidden)]
pub fn call_for_tests(a: &mut Actor, method: &str, args: &[f32]) {
    call(a, method, args);
}

/// One method call with numbers, no parsing (per frame).
pub(crate) fn call(a: &mut Actor, method: &str, args: &[f32]) {
    let c = Call {
        method: method.into(),
        args: args.iter().map(|&n| Arg::Num(n)).collect(),
    };
    let _ = a.call(&c, &mut Problems::default(), 0);
}

pub(crate) fn named(kind: Kind, name: &str) -> Actor {
    let mut a = Actor::new(kind);
    a.name = name.into();
    a
}

pub(crate) fn quad(name: &str, size: [f32; 2], color: Rgba) -> Actor {
    let mut a = named(Kind::Sprite(Sprite::quad(size)), name);
    call(&mut a, "diffuse", &color);
    a
}

pub(crate) fn text(name: &str, s: &str, size: f32, color: Rgba) -> Actor {
    text_in(PLAIN_FONT, name, s, size, color)
}

/// A run of text in `font`.
pub(crate) fn text_in(font: &str, name: &str, s: &str, size: f32, color: Rgba) -> Actor {
    let mut a = named(Kind::Text(Text::new(s, TextStyle::new(font, size))), name);
    call(&mut a, "diffuse", &color);
    a
}

fn command(a: &mut Actor, name: &str, s: &str) {
    a.set_command(name, parse(s).unwrap_or_default());
}

fn frame(children: Vec<Actor>) -> Actor {
    Actor::new(Kind::Frame(children))
}

fn lit_plate(size: [f32; 2]) -> Actor {
    let mut plate = quad("Plate", size, PLATE);
    command(
        &mut plate,
        "GainFocus",
        "stoptweening;linear,0.08;diffuse,0.22,0.33,0.72,1",
    );
    command(
        &mut plate,
        "LoseFocus",
        "stoptweening;linear,0.08;diffuse,0.11,0.12,0.15,1",
    );
    plate
}

/// The plain components, their texts in `font`.
fn plain(name: &str, spec: &Spec, font: &str) -> Option<Actor> {
    {
        let text = |n: &str, t: &str, size: f32, c: Rgba| text_in(font, n, t, size, c);
        let [w, h] = spec.size;
        let size = (h * 0.45).clamp(12.0, 22.0);
        let a = match name {
            "Button" => {
                let mut f = frame(vec![
                    lit_plate(spec.size),
                    text("Label", &spec.text, size, TEXT),
                ]);
                command(&mut f, "Press", "stoptweening;linear,0.05;zoom,0.95");
                command(&mut f, "Release", "stoptweening;linear,0.08;zoom,1");
                f
            }
            "Toggle" => {
                let mut label = text("Label", &spec.text, size, TEXT);
                call(&mut label, "x", &[-w / 2.0 + 12.0]);
                call(&mut label, "halign", &[0.0]);
                let mut bx = quad("Box", [h * 0.55, h * 0.55], [0.3, 0.31, 0.36, 1.0]);
                call(&mut bx, "x", &[w / 2.0 - h * 0.5]);
                let mut mark = quad("Mark", [h * 0.35, h * 0.35], LIT);
                call(&mut mark, "x", &[w / 2.0 - h * 0.5]);
                command(
                    &mut mark,
                    "On",
                    "stoptweening;linear,0.08;zoom,1;diffusealpha,1",
                );
                command(
                    &mut mark,
                    "Off",
                    "stoptweening;linear,0.08;zoom,0.3;diffusealpha,0",
                );
                frame(vec![lit_plate(spec.size), label, bx, mark])
            }
            "OptionRow" | "Slider" => {
                let mut label = text("Label", &spec.text, size, MUTED);
                call(&mut label, "x", &[-w / 2.0 + 12.0]);
                call(&mut label, "halign", &[0.0]);
                let mut parts = vec![lit_plate(spec.size), label];
                if name == "OptionRow" {
                    // Where the choices go: the right two thirds.
                    let span = w * 2.0 / 3.0 - 16.0;
                    let mut choices = quad("Choices", [span, h], [0.0; 4]);
                    call(&mut choices, "x", &[-w / 2.0 + w / 3.0 + span / 2.0]);
                    choices.visible = false;
                    parts.push(choices);
                }
                if name == "Slider" {
                    let track_w = w * 0.5;
                    let left = w / 2.0 - 70.0 - track_w;
                    let mut track = quad("Track", [track_w, 4.0], [0.3, 0.31, 0.36, 1.0]);
                    call(&mut track, "x", &[left + track_w / 2.0]);
                    let mut fill = quad("Fill", [track_w, 4.0], LIT);
                    call(&mut fill, "x", &[left]);
                    call(&mut fill, "halign", &[0.0]);
                    let mut knob = quad("Knob", [10.0, h * 0.6], TEXT);
                    call(&mut knob, "x", &[left]);
                    let mut value = text("Value", "", size, TEXT);
                    call(&mut value, "x", &[w / 2.0 - 12.0]);
                    call(&mut value, "halign", &[1.0]);
                    parts.extend([track, fill, knob, value]);
                }
                frame(parts)
            }
            "OptionRow.Choice" => text("", &spec.text, size, TEXT),
            "OptionRow.Cursor" => quad("", [w, 2.0], LIT),
            "List.Row" => {
                let mut plate = quad("Plate", spec.size, PLATE);
                call(&mut plate, "halign", &[0.0]);
                call(&mut plate, "valign", &[0.0]);
                let ban = [h * 2.4, h - 8.0];
                let mut placeholder = quad("", ban, [0.18, 0.19, 0.23, 1.0]);
                let mut banner = named(Kind::Sprite(Sprite::quad(ban)), "Banner");
                for b in [&mut placeholder, &mut banner] {
                    call(b, "xy", &[4.0 + ban[0] / 2.0, h / 2.0]);
                }
                banner.visible = false;
                let mut title = text("Title", "", 16.0, TEXT);
                call(&mut title, "xy", &[ban[0] + 14.0, h * 0.36]);
                call(&mut title, "halign", &[0.0]);
                call(&mut title, "maxwidth", &[w - ban[0] - 24.0]);
                let mut sub = text("Subtitle", "", 12.0, MUTED);
                call(&mut sub, "xy", &[ban[0] + 14.0, h * 0.72]);
                call(&mut sub, "halign", &[0.0]);
                call(&mut sub, "maxwidth", &[w - ban[0] - 24.0]);
                command(
                    &mut plate,
                    "Select",
                    "stoptweening;linear,0.06;diffuse,0.22,0.33,0.72,1",
                );
                command(
                    &mut plate,
                    "Deselect",
                    "stoptweening;linear,0.06;diffuse,0.11,0.12,0.15,1",
                );
                frame(vec![plate, placeholder, banner, title, sub])
            }
            "List.Header" => {
                let mut plate = quad("Plate", spec.size, [0.05, 0.05, 0.07, 1.0]);
                call(&mut plate, "halign", &[0.0]);
                call(&mut plate, "valign", &[0.0]);
                command(
                    &mut plate,
                    "Select",
                    "stoptweening;linear,0.06;diffuse,0.16,0.22,0.45,1",
                );
                command(
                    &mut plate,
                    "Deselect",
                    "stoptweening;linear,0.06;diffuse,0.05,0.05,0.07,1",
                );
                let mut title = text("Title", "", 15.0, [0.75, 0.8, 1.0, 1.0]);
                call(&mut title, "xy", &[10.0, h / 2.0]);
                call(&mut title, "halign", &[0.0]);
                call(&mut title, "maxwidth", &[w - 20.0]);
                frame(vec![plate, title])
            }
            "List.Bar" => {
                let mut track = quad("Track", spec.size, [0.08, 0.08, 0.1, 1.0]);
                call(&mut track, "valign", &[0.0]);
                let mut thumb = quad("Thumb", [spec.size[0], 40.0], [0.35, 0.37, 0.45, 1.0]);
                call(&mut thumb, "valign", &[0.0]);
                frame(vec![track, thumb])
            }
            "TextField" => {
                let mut bx = lit_plate(spec.size);
                bx.name = "Box".into();
                let mut hint = text("Hint", &spec.text, size, MUTED);
                call(&mut hint, "x", &[-w / 2.0 + 10.0]);
                call(&mut hint, "halign", &[0.0]);
                frame(vec![bx, hint])
            }
            "Popup.Item" => {
                let mut plate = quad("Plate", spec.size, [0.15, 0.16, 0.2, 1.0]);
                command(
                    &mut plate,
                    "Select",
                    "stoptweening;diffuse,0.22,0.33,0.72,1",
                );
                command(
                    &mut plate,
                    "Deselect",
                    "stoptweening;diffuse,0.15,0.16,0.2,1",
                );
                let mut t = text("Text", &spec.text, size, TEXT);
                call(&mut t, "x", &[-w / 2.0 + 10.0]);
                call(&mut t, "halign", &[0.0]);
                call(&mut t, "maxwidth", &[w - 90.0]);
                let mut note = text("Note", "", size * 0.75, MUTED);
                call(&mut note, "x", &[w / 2.0 - 10.0]);
                call(&mut note, "halign", &[1.0]);
                frame(vec![plate, t, note])
            }
            "Panel" => {
                let mut plate = quad("Plate", spec.size, [0.07, 0.075, 0.09, 1.0]);
                call(&mut plate, "halign", &[0.0]);
                call(&mut plate, "valign", &[0.0]);
                let mut title = text("Title", &spec.text, 13.0, MUTED);
                call(&mut title, "xy", &[10.0, 14.0]);
                call(&mut title, "halign", &[0.0]);
                frame(vec![plate, title])
            }
            _ => return None,
        };
        Some(a)
    }
}

impl Look for Plain {
    fn component(&self, name: &str, spec: &Spec) -> Option<Actor> {
        plain(name, spec, PLAIN_FONT)
    }

    fn color(&self, name: &str) -> Option<Rgba> {
        Some(match name {
            "Text" => TEXT,
            "Muted" => MUTED,
            "Accent" | "Graph.Line" => [0.45, 0.62, 1.0, 1.0],
            "Graph.Bar" => [0.35, 0.5, 0.9, 1.0],
            "Graph.Point" => [1.0, 0.8, 0.35, 1.0],
            "Graph.Axis" => [0.3, 0.31, 0.36, 1.0],
            "Plate" => PLATE,
            // A screen's own ground (screens under it stay drawn).
            "Background" => [12.0 / 255.0, 12.0 / 255.0, 18.0 / 255.0, 1.0],
            _ => return None,
        })
    }

    fn wheel(&self, _list: &str, distance: f32) -> Option<WheelPlace> {
        // A gentle arc: rows away from the selection move right, shrink
        // and fade.
        let d = distance.abs();
        Some(WheelPlace {
            x: d * d * 6.0,
            zoom: (1.0 - d * 0.04).max(0.7),
            alpha: (1.0 - d * 0.12).max(0.25),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct Red;

    impl Look for Red {
        fn component(&self, name: &str, spec: &Spec) -> Option<Actor> {
            (name == "Button").then(|| quad("Plate", spec.size, [1.0, 0.0, 0.0, 1.0]))
        }

        fn color(&self, name: &str) -> Option<Rgba> {
            (name == "Accent").then_some([1.0, 0.0, 0.0, 1.0])
        }
    }

    #[test]
    fn looks_fall_back_to_the_plain_one() {
        let b = component(&Red, "Button", &Spec::new([10.0, 10.0], "x"));
        assert_eq!(b.name, "Plate");
        let t = component(&Red, "Toggle", &Spec::new([100.0, 30.0], "On"));
        assert!(t.children().iter().any(|c| c.name == "Mark"));
        assert_eq!(color(&Red, "Accent"), [1.0, 0.0, 0.0, 1.0]);
        assert_eq!(color(&Red, "Text"), TEXT);
        let empty = component(&Red, "Nothing", &Spec::new([1.0, 1.0], ""));
        assert!(empty.children().is_empty());
    }

    struct Mono;

    impl Look for Mono {
        fn font(&self) -> Option<Arc<str>> {
            Some("monospace".into())
        }

        fn component(&self, name: &str, spec: &Spec) -> Option<Actor> {
            (name == "SongList.Row").then(|| quad("Special", spec.size, [1.0; 4]))
        }
    }

    #[test]
    fn the_looks_font_and_prefixed_components() {
        // The plain components take the look's font.
        let b = component(&Mono, "Button", &Spec::new([100.0, 40.0], "Go"));
        let label = b.children().iter().find(|c| c.name == "Label").unwrap();
        let Kind::Text(t) = &label.kind else { panic!() };
        assert_eq!(&*t.style().family, "monospace");
        // A list's own component, else the generic one.
        assert_eq!(
            component_or(
                &Mono,
                "SongList.Row",
                "List.Row",
                &Spec::new([10.0, 10.0], "")
            )
            .name,
            "Special"
        );
        let other = component_or(
            &Mono,
            "Other.Row",
            "List.Row",
            &Spec::new([100.0, 48.0], ""),
        );
        assert!(other.children().iter().any(|c| c.name == "Banner"));
    }
}
