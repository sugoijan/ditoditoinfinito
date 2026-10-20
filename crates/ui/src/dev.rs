//! The developer screen (`docs/plans/ui-core.md`, step 3): every primitive
//! of the core on one screen, run by the web shell at `#/ui-dev` and by
//! the desktop window alike, so both run the same code. Not a design:
//! plain quads and runs.
//!
//! - A grid of buttons (focus by explicit neighbours at the row ends, by
//!   place elsewhere): **Beep** asks for a sound (a gesture-bound request;
//!   a long press on it asks from a timer and is refused), **Link** opens
//!   a page, **Files** a file picker, **Dialog** a modal dialog (Back or a
//!   press outside cancels it), **Popup** a popup the screen owns,
//!   **Count** counts.
//! - A text field: Start or a tap types into it; Enter submits, Escape
//!   cancels.
//! - A strip of rows (the song library's titles when the shell gives
//!   them, [`DevState::songs`], then numbered ones) to drag, flick (with
//!   inertia), wheel and long press.
//! - A status line: the last thing that happened.
//!
//! What it did is kept in a [`DevState`] the shell shares, for tests and
//! the debug view.

use std::cell::RefCell;
use std::rc::Rc;

use ddi_actor::text::TextStyle;
use ddi_actor::{Actor, Kind, Sprite, Text};

use crate::gesture::Gesture;
use crate::hit::find_mut;
use crate::look::{command_of, run};
use crate::message::Message;
use crate::nav::Focusable;
use crate::request::{EnterKey, InputHints, InputMode, UiRequest};
use crate::screen::{Cx, Handled, Input, Screen, TextChange};
use crate::widgets::set_text;

/// The screen's design size (16:9; the width follows the window).
pub const DESIGN: [f32; 2] = [960.0, 540.0];

/// A font stack the shells resolve (the web's browser, desktop's
/// cosmic-text).
pub const FONT: &str = "system-ui, sans-serif";

/// The page **Link** opens.
pub const LINK: &str = "https://github.com/sugoijan/ditoditoinfinito";

const BUTTONS: [[&str; 3]; 2] = [["Beep", "Link", "Files"], ["Dialog", "Popup", "Count"]];
const ROW: f32 = 44.0;
const STRIP_HEIGHT: f32 = 400.0;
/// How fast a flick slows, per second.
const FRICTION: f32 = 4.0;

/// What the screen did, shared with the shell.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct DevState {
    /// The last thing that happened, as the status line shows it.
    pub status: String,
    pub count: u32,
    /// Sounds asked for, and requests refused for want of a gesture.
    pub beeps: u32,
    pub refused: u32,
    /// The field's text, as last edited or submitted.
    pub field: String,
    pub submitted: Vec<String>,
    /// The strip's scroll (virtual pixels from its top) and its inertia.
    pub strip: f32,
    pub velocity: f32,
    /// Rows of the strip: song titles from the shell, else numbers.
    pub songs: Vec<String>,
    pub dialogs: u32,
    pub popups: u32,
}

/// Shared [`DevState`].
pub type Shared = Rc<RefCell<DevState>>;

/// The message a shell sends once it has put the library's titles in
/// [`DevState::songs`].
pub const SONGS_MESSAGE: &str = "DevSongs";

fn actor(kind: Kind, name: &str, init: &str) -> Actor {
    let mut a = Actor::new(kind);
    a.name = name.into();
    run(&mut a, init);
    a
}

fn quad(name: &str, size: [f32; 2], init: &str) -> Actor {
    actor(Kind::Sprite(Sprite::quad(size)), name, init)
}

fn text(name: &str, s: &str, size: f32, init: &str) -> Actor {
    actor(
        Kind::Text(Text::new(s, TextStyle::new(FONT, size))),
        name,
        init,
    )
}

fn frame(name: &str, children: Vec<Actor>, init: &str) -> Actor {
    actor(Kind::Frame(children), name, init)
}

/// A button: a frame named `name` holding its plate and its label
/// (`<name>Label`), lit while focused.
fn button(name: &str, label: &str, x: f32, y: f32, w: f32) -> Actor {
    let mut plate = quad("", [w, 48.0], "diffuse,0.18,0.19,0.24,1");
    command_of(&mut plate, "GainFocus", "diffuse,0.25,0.36,0.75,1");
    command_of(&mut plate, "LoseFocus", "diffuse,0.18,0.19,0.24,1");
    frame(
        name,
        vec![plate, text(&format!("{name}Label"), label, 20.0, "")],
        &format!("xy,{x},{y}"),
    )
}

/// How far the strip scrolls.
fn max_scroll(s: &DevState) -> f32 {
    (row_count(&s.songs) as f32 * ROW - STRIP_HEIGHT).max(0.0)
}

/// The strip's rows: the titles, then numbered ones up to 40 (enough to
/// scroll).
fn row_count(titles: &[String]) -> usize {
    titles.len().max(40)
}

fn rows(titles: &[String]) -> Vec<Actor> {
    let count = row_count(titles);
    (0..count)
        .map(|i| {
            let label = titles
                .get(i)
                .cloned()
                .unwrap_or_else(|| format!("Row {}", i + 1));
            let shade = if i % 2 == 0 { 0.13 } else { 0.16 };
            frame(
                &format!("Row{i}"),
                vec![
                    quad(
                        "",
                        [300.0, ROW - 4.0],
                        &format!("diffuse,{shade},{shade},{},1", shade + 0.04),
                    ),
                    text("", &label, 17.0, "x,-140;halign,0;maxwidth,280"),
                ],
                &format!("y,{}", i as f32 * ROW + ROW / 2.0),
            )
        })
        .collect()
}

/// The developer screen.
pub struct DevScreen {
    pub state: Shared,
    /// The widget screen's state and the shell's NFKD, for its button.
    widgets: Option<(
        crate::dev_widgets::SharedWidgets,
        crate::dev_widgets::FoldFn,
    )>,
}

impl DevScreen {
    pub fn new(state: Shared) -> DevScreen {
        DevScreen {
            state,
            widgets: None,
        }
    }

    /// With a **Widgets** button opening the widget screen
    /// (`crate::dev_widgets`).
    pub fn with_widgets(
        state: Shared,
        widgets: crate::dev_widgets::SharedWidgets,
        fold: crate::dev_widgets::FoldFn,
    ) -> DevScreen {
        DevScreen {
            state,
            widgets: Some((widgets, fold)),
        }
    }

    fn status(&self, root: &mut Actor, s: String) {
        set_text(root, "Status", &s);
        self.state.borrow_mut().status = s;
    }

    fn max_scroll(&self) -> f32 {
        max_scroll(&self.state.borrow())
    }

    /// Moves the strip to `to`, within its ends.
    fn scroll(&self, root: &mut Actor, to: f32) {
        let to = to.clamp(0.0, self.max_scroll());
        {
            let mut s = self.state.borrow_mut();
            if to == 0.0 || to == max_scroll(&s) {
                s.velocity = 0.0;
            }
            s.strip = to;
        }
        if let Some(r) = find_mut(root, "Rows") {
            run(r, &format!("y,{}", -to - STRIP_HEIGHT / 2.0));
        }
    }

    fn field_hint(&self, root: &mut Actor, typing: bool) {
        let empty = self.state.borrow().field.is_empty();
        if let Some(h) = find_mut(root, "FieldHint") {
            h.visible = empty && !typing;
        }
    }
}

impl Screen for DevScreen {
    fn name(&self) -> &str {
        "dev"
    }

    fn build(&mut self, _cx: &mut Cx) -> Actor {
        let mut children = vec![text(
            "Title",
            "Dito Dito Infinito: the UI core",
            26.0,
            "xy,40,44;halign,0",
        )];
        for (r, row) in BUTTONS.iter().enumerate() {
            for (c, name) in row.iter().enumerate() {
                let label = if *name == "Count" {
                    format!("Count {}", self.state.borrow().count)
                } else {
                    name.to_string()
                };
                children.push(button(
                    name,
                    &label,
                    120.0 + c as f32 * 180.0,
                    120.0 + r as f32 * 68.0,
                    160.0,
                ));
            }
        }
        if self.widgets.is_some() {
            children.push(button("Widgets", "Widgets", 640.0, 44.0, 140.0));
        }
        children.push(frame(
            "Field",
            vec![
                quad("", [520.0, 40.0], "diffuse,0.1,0.1,0.13,1"),
                text(
                    "FieldText",
                    "",
                    20.0,
                    "x,-250;halign,0;diffuse,0.95,0.95,0.9,1",
                ),
                text(
                    "FieldHint",
                    "Type here (Start or a tap)",
                    20.0,
                    "x,-250;halign,0;diffuse,0.5,0.5,0.55,1",
                ),
            ],
            "xy,300,300",
        ));
        children.push(text(
            "Status",
            "Ready",
            18.0,
            "xy,40,360;halign,0;diffuse,0.8,0.85,0.6,1",
        ));
        let titles = self.state.borrow().songs.clone();
        let mut strip = frame(
            "Strip",
            vec![frame(
                "Rows",
                rows(&titles),
                &format!("y,{}", -STRIP_HEIGHT / 2.0),
            )],
            "xy,760,290",
        );
        strip.clip = Some([-160.0, -STRIP_HEIGHT / 2.0, 160.0, STRIP_HEIGHT / 2.0]);
        children.push(strip);
        let mut root = frame("", children, "");
        let scroll = self.state.borrow().strip;
        self.scroll(&mut root, scroll);
        root
    }

    fn focusables(&self) -> Vec<Focusable> {
        let mut out: Vec<Focusable> = BUTTONS
            .iter()
            .flatten()
            .map(|n| Focusable::new(*n))
            .collect();
        // The rows' ends wrap round.
        out[0].left = Some("Files".into());
        out[2].right = Some("Beep".into());
        out[3].left = Some("Count".into());
        out[5].right = Some("Dialog".into());
        out.push(Focusable::new("Field"));
        if self.widgets.is_some() {
            out.push(Focusable::new("Widgets"));
        }
        out
    }

    fn animating(&self) -> bool {
        self.state.borrow().velocity != 0.0
    }

    fn update(&mut self, dt: f32, root: &mut Actor, _cx: &mut Cx) {
        // What overlays said.
        let status = self.state.borrow().status.clone();
        if let Some(a) = find_mut(root, "Status")
            && let Kind::Text(t) = &mut a.kind
            && t.text() != status
            && !status.is_empty()
        {
            t.set_text(&status);
        }
        let (v, at) = {
            let s = self.state.borrow();
            (s.velocity, s.strip)
        };
        if v != 0.0 {
            let v2 = v * (-FRICTION * dt).exp();
            self.state.borrow_mut().velocity = if v2.abs() < 20.0 { 0.0 } else { v2 };
            self.scroll(root, at + v * dt);
        }
    }

    fn input(&mut self, input: &Input, root: &mut Actor, cx: &mut Cx) -> Handled {
        match input {
            Input::Activate { name } => {
                match name.as_str() {
                    "Beep" => {
                        self.state.borrow_mut().beeps += 1;
                        cx.request(UiRequest::StartAudio);
                        self.status(root, "beep".into());
                    }
                    "Link" => {
                        cx.request(UiRequest::OpenUrl(LINK.into()));
                        self.status(root, "link".into());
                    }
                    "Files" => {
                        cx.request(UiRequest::PickFiles { folder: false });
                        self.status(root, "files".into());
                    }
                    "Dialog" => {
                        self.state.borrow_mut().dialogs += 1;
                        cx.overlay(
                            DevDialog {
                                state: self.state.clone(),
                            },
                            true,
                        );
                    }
                    "Popup" => {
                        self.state.borrow_mut().popups += 1;
                        cx.overlay(
                            DevPopup {
                                state: self.state.clone(),
                            },
                            false,
                        );
                    }
                    "Widgets" => {
                        if let Some((w, fold)) = &self.widgets {
                            cx.push(crate::dev_widgets::WidgetScreen::new(
                                w.clone(),
                                fold.clone(),
                            ));
                        }
                    }
                    "Count" => {
                        let n = {
                            let mut s = self.state.borrow_mut();
                            s.count += 1;
                            s.count
                        };
                        set_text(root, "CountLabel", &format!("Count {n}"));
                        self.status(root, format!("count {n}"));
                    }
                    "Field" => {
                        let text = self.state.borrow().field.clone();
                        cx.begin_text(
                            "FieldText",
                            &text,
                            InputHints {
                                mode: InputMode::Search,
                                enter: EnterKey::Done,
                                max_chars: Some(60),
                            },
                        );
                        self.field_hint(root, true);
                    }
                    _ => return Handled::No,
                }
                Handled::Yes
            }
            Input::Pointer {
                target, gesture, ..
            } => {
                let target = target.as_ref();
                let in_strip = target.is_some_and(|t| t.within("Strip"));
                let button =
                    target.and_then(|t| BUTTONS.iter().flatten().find(|b| t.within(b)).copied());
                if let Some(b) = button
                    && let Some(a) = find_mut(root, b)
                {
                    match gesture {
                        Gesture::Press => run(a, "stoptweening;linear,0.05;zoom,0.94"),
                        Gesture::Tap
                        | Gesture::Cancel
                        | Gesture::Release
                        | Gesture::DragStart
                        | Gesture::LongPress => run(a, "stoptweening;linear,0.08;zoom,1"),
                        _ => {}
                    }
                }
                match gesture {
                    Gesture::LongPress if button == Some("Beep") => {
                        // From a timer: refused where gestures count.
                        cx.request(UiRequest::StartAudio);
                        self.status(root, "long press Beep".into());
                        Handled::Yes
                    }
                    Gesture::LongPress => {
                        let name = target.map_or("-".to_string(), |t| t.name.clone());
                        self.status(root, format!("long press {name}"));
                        Handled::Yes
                    }
                    Gesture::Press if in_strip => {
                        self.state.borrow_mut().velocity = 0.0;
                        Handled::Yes
                    }
                    Gesture::Drag { delta } if in_strip => {
                        let at = self.state.borrow().strip;
                        self.scroll(root, at - delta[1]);
                        Handled::Yes
                    }
                    Gesture::Flick { velocity } if in_strip => {
                        self.state.borrow_mut().velocity = -velocity[1];
                        self.status(root, format!("flick {:.0}", velocity[1]));
                        Handled::Yes
                    }
                    Gesture::DragEnd { .. } if in_strip => {
                        self.status(root, "drag".into());
                        Handled::Yes
                    }
                    Gesture::Tap if in_strip => {
                        let row = target.map_or("-".to_string(), |t| t.name.clone());
                        self.status(root, format!("tap {row}"));
                        Handled::Yes
                    }
                    _ => Handled::No,
                }
            }
            Input::Wheel {
                target: Some(t),
                delta,
                ..
            } if t.within("Strip") => {
                let at = self.state.borrow().strip;
                self.state.borrow_mut().velocity = 0.0;
                self.scroll(root, at + delta[1]);
                self.status(root, format!("wheel {:.0}", delta[1]));
                Handled::Yes
            }
            Input::Text { change, field, .. } => {
                self.state.borrow_mut().field = field.text().to_string();
                match change {
                    TextChange::Submit => {
                        self.state
                            .borrow_mut()
                            .submitted
                            .push(field.text().to_string());
                        self.status(root, format!("submit {}", field.text()));
                        set_text(root, "FieldText", field.text());
                        cx.end_text();
                        self.field_hint(root, false);
                    }
                    TextChange::Cancel | TextChange::Blurred => {
                        set_text(root, "FieldText", field.text());
                        if *change == TextChange::Cancel {
                            cx.end_text();
                        }
                        self.status(root, format!("{change:?}").to_lowercase());
                        self.field_hint(root, false);
                    }
                    TextChange::Edited | TextChange::Moved => {}
                }
                Handled::Yes
            }
            Input::NeedsGesture(r) => {
                self.state.borrow_mut().refused += 1;
                let what = match r {
                    UiRequest::StartAudio => "sound",
                    UiRequest::StartTextInput { .. } => "typing",
                    _ => "that",
                };
                self.status(root, format!("needs a tap or a key for {what}"));
                Handled::Yes
            }
            _ => Handled::No,
        }
    }

    fn message(&mut self, m: &Message, root: &mut Actor, _cx: &mut Cx) {
        if m.name == SONGS_MESSAGE
            && let Some(r) = find_mut(root, "Rows")
        {
            let titles = self.state.borrow().songs.clone();
            r.kind = Kind::Frame(rows(&titles));
            let at = self.state.borrow().strip;
            self.scroll(root, at);
        }
    }
}

/// A modal yes/no.
struct DevDialog {
    state: Shared,
}

impl Screen for DevDialog {
    fn name(&self) -> &str {
        "dialog"
    }

    fn build(&mut self, _cx: &mut Cx) -> Actor {
        let mut root = frame(
            "",
            vec![
                // The dimmed screen around (decoration: a press there is
                // outside the dialog).
                quad("", [4000.0, 4000.0], "diffuse,0,0,0,0.55"),
                quad(
                    "Panel",
                    [440.0, 200.0],
                    "xy,480,270;diffuse,0.14,0.14,0.18,1",
                ),
                text(
                    "Question",
                    "A dialog: Back or a press outside cancels",
                    18.0,
                    "xy,480,220",
                ),
                button("Yes", "Yes", 400.0, 310.0, 140.0),
                button("No", "No", 560.0, 310.0, 140.0),
            ],
            "",
        );
        command_of(&mut root, "On", "diffusealpha,0;linear,0.12;diffusealpha,1");
        command_of(&mut root, "Off", "linear,0.1;diffusealpha,0");
        command_of(&mut root, "Cancel", "linear,0.1;diffusealpha,0");
        root
    }

    fn focusables(&self) -> Vec<Focusable> {
        vec![Focusable::new("Yes"), Focusable::new("No")]
    }

    fn input(&mut self, input: &Input, root: &mut Actor, cx: &mut Cx) -> Handled {
        let _ = root;
        // The screen under shows it at its next update.
        let say = |s: &str| self.state.borrow_mut().status = s.into();
        match input {
            Input::Activate { name } if name == "Yes" => {
                say("dialog yes");
                cx.close(false);
                Handled::Yes
            }
            Input::Activate { name } if name == "No" => {
                say("dialog no");
                cx.close(true);
                Handled::Yes
            }
            Input::Outside => {
                say("dialog outside");
                cx.close(true);
                Handled::Yes
            }
            Input::Menu {
                action: crate::nav::MenuAction::Back,
                ..
            } => {
                say("dialog back");
                Handled::No
            }
            _ => Handled::No,
        }
    }
}

/// A popup of three items, under the Popup button.
struct DevPopup {
    state: Shared,
}

impl Screen for DevPopup {
    fn name(&self) -> &str {
        "popup"
    }

    fn build(&mut self, _cx: &mut Cx) -> Actor {
        let items = ["A", "B", "C"]
            .iter()
            .enumerate()
            .map(|(i, n)| {
                frame(
                    &format!("Pop{n}"),
                    vec![
                        quad("", [150.0, 34.0], "diffuse,0.22,0.3,0.24,1"),
                        text("", &format!("Item {n}"), 17.0, ""),
                    ],
                    &format!("xy,480,{}", 240.0 + i as f32 * 38.0),
                )
            })
            .collect();
        frame("", items, "")
    }

    fn input(&mut self, input: &Input, _root: &mut Actor, cx: &mut Cx) -> Handled {
        if let Input::Pointer {
            target: Some(t),
            gesture: Gesture::Tap,
            ..
        } = input
        {
            self.state.borrow_mut().status = format!("popup {}", t.name);
            cx.close(false);
            return Handled::Yes;
        }
        Handled::No
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::event::PointerKind;
    use crate::testing::Harness;
    use crate::text_field::TextMode;

    fn dev() -> (Harness, Shared) {
        let state = Shared::default();
        let mut h = Harness::new(DESIGN, [1280.0, 720.0], 2.0, TextMode::Mirror);
        h.push(DevScreen::new(state.clone()));
        (h, state)
    }

    #[test]
    fn every_primitive_answers() {
        let (mut h, state) = dev();
        assert_eq!(h.ui.focused(), Some("Beep"));
        // Explicit wrap, then by place.
        h.key("ArrowLeft", "ArrowLeft");
        assert_eq!(h.ui.focused(), Some("Files"));
        h.key("ArrowDown", "ArrowDown");
        assert_eq!(h.ui.focused(), Some("Count"));
        h.key("Enter", "Enter");
        assert_eq!(state.borrow().count, 1);
        h.frame(0.0);
        assert!(h.texts().contains(&"Count 1".to_string()));
        // A tap on Beep asks for sound inside the gesture.
        let r = h.tap(PointerKind::Touch, "Beep");
        assert_eq!(r, vec![UiRequest::StartAudio]);
        // A long press on it is refused.
        let at = h.centre("Beep");
        h.pointer(1, PointerKind::Touch, crate::event::PointerPhase::Down, at);
        h.run(0.6);
        assert_eq!(state.borrow().refused, 1);
        h.pointer(1, PointerKind::Touch, crate::event::PointerPhase::Up, at);
        // The dialog: outside cancels.
        h.tap(PointerKind::Mouse, "Dialog");
        assert_eq!(h.ui.layers().len(), 2);
        // It takes input once it has come in.
        h.run(0.2);
        h.tap_at(PointerKind::Mouse, [20.0, 20.0]);
        h.run(0.2);
        assert_eq!(h.ui.layers().len(), 1);
        assert_eq!(state.borrow().status, "dialog outside");
        // The popup: an item closes it.
        h.tap(PointerKind::Mouse, "Popup");
        h.tap(PointerKind::Mouse, "PopB");
        assert_eq!(state.borrow().status, "popup PopB");
        assert_eq!(h.ui.layers().len(), 1);
        // The strip: a flick with inertia, then the wheel.
        let from = h.centre("Strip");
        h.drag(PointerKind::Touch, from, [from[0], from[1] - 120.0], 0.1, 5);
        assert!(state.borrow().velocity > 0.0);
        assert!(h.ui.needs_redraw());
        h.run(3.0);
        assert_eq!(state.borrow().velocity, 0.0);
        assert!(state.borrow().strip > 120.0);
        let before = state.borrow().strip;
        h.wheel_lines(from, [0.0, -1.0]);
        assert!(state.borrow().strip < before);
        // Typing.
        let r = h.tap(PointerKind::Touch, "Field");
        assert!(
            matches!(r.first(), Some(UiRequest::StartTextInput { .. })),
            "{r:?}"
        );
        h.sync("dito", [4, 4], None);
        h.key("Enter", "Enter");
        assert_eq!(state.borrow().submitted, vec!["dito".to_string()]);
    }

    #[test]
    fn the_library_fills_the_strip() {
        let (mut h, state) = dev();
        state.borrow_mut().songs = vec!["Sakura".into(), "東京".into()];
        h.ui.broadcast(Message::new(SONGS_MESSAGE));
        h.frame(0.0);
        assert!(h.texts().contains(&"東京".to_string()));
        // Numbered rows follow, so a short library still scrolls.
        assert!(!h.texts().contains(&"Row 1".to_string()));
        assert!(h.texts().contains(&"Row 3".to_string()));
        let at = h.centre("Strip");
        h.wheel_lines(at, [0.0, 2.0]);
        assert!(state.borrow().strip > 0.0);
    }
}
