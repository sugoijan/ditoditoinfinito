//! A text field and the popup list under it (autocomplete).
//!
//! The field is one named frame holding its box and its text actor
//! (`<name>.Text`), so a press on either is inside it (the Ui ends typing
//! on a press elsewhere). Start or a tap starts typing
//! ([`crate::Cx::begin_text`]).
//!
//! The popup lists suggestions for what is typed, from the screen's
//! function (a library's, `ddi_library::suggest`). It is an overlay layer
//! the field's screen owns ([`crate::Cx::overlay`]), so it lies over
//! everything of the screen and leaves with it; its items are
//! `<name>.Item<i>`. It opens under the field, or over it when it would
//! pass the visible bottom (the shell's size is the visible area: a
//! phone's keyboard shrinks it). It shows only while something is typed
//! and nothing is being composed (an IME's candidates would cover it and
//! its keys are the IME's): Up and Down move through it, Enter takes the
//! highlighted one (else submits what is typed), Escape closes it (a
//! second Escape stops typing), a tap takes one. A screen with a field
//! hands it its messages ([`SearchField::message`]): that is how a tap on
//! the popup comes back.

use std::cell::RefCell;
use std::rc::Rc;

use ddi_actor::{Actor, Kind};

use crate::gesture::Gesture;
use crate::hit;
use crate::look::{self, Look, Plain, Spec, call};
use crate::message::{Message, Param};
use crate::request::InputHints;
use crate::screen::{Cx, Handled, Input, Screen, TextChange};
use crate::widgets::{part, play, set_text};

/// The message the popup sends its screen when an item is tapped
/// (`Field`: the field's name, `Item`: which).
pub const PICK_MESSAGE: &str = "FieldPick";

/// What happened to the field.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum FieldEvent {
    /// The text was typed into.
    Changed(String),
    /// Enter, or a suggestion taken (by Enter or a tap).
    Submitted(String),
}

/// What the popup shows, shared by the field and its layer.
#[derive(Default)]
struct PopupState {
    items: Vec<(String, String)>,
    highlight: Option<usize>,
    open: bool,
    /// Its layer is up.
    layer: bool,
    /// The field's rectangle, `[left, top, right, bottom]` on the screen.
    field: [f32; 4],
    dirty: bool,
}

pub struct SearchField {
    pub name: String,
    pub hint: String,
    pub centre: [f32; 2],
    pub size: [f32; 2],
    pub hints: InputHints,
    pub text: String,
    /// Popup rows at most.
    pub max_items: usize,
    pub suggestions: Vec<(String, String)>,
    /// The popup's look (the field's own is the one `build` is given).
    pub look: Rc<dyn Look>,
    popup: Rc<RefCell<PopupState>>,
    typing: bool,
}

impl SearchField {
    pub fn new(name: &str, hint: &str, centre: [f32; 2], size: [f32; 2]) -> SearchField {
        SearchField {
            name: name.into(),
            hint: hint.into(),
            centre,
            size,
            hints: InputHints::default(),
            text: String::new(),
            max_items: 6,
            suggestions: Vec::new(),
            look: Rc::new(Plain),
            popup: Rc::default(),
            typing: false,
        }
    }

    pub fn text_actor(&self) -> String {
        format!("{}.Text", self.name)
    }

    pub fn popup_open(&self) -> bool {
        self.popup.borrow().open
    }

    pub fn highlight(&self) -> Option<usize> {
        self.popup.borrow().highlight
    }

    pub fn build(&self, look: &dyn Look) -> Actor {
        let [w, h] = self.size;
        let mut a = look::component(look, "TextField", &Spec::new(self.size, &self.hint));
        a.name = self.name.clone();
        call(&mut a, "xy", &[self.centre[0], self.centre[1]]);
        let mut text = look::text_in(
            &look::font(look),
            &self.text_actor(),
            &self.text,
            (h * 0.45).clamp(12.0, 22.0),
            look::color(look, "Text"),
        );
        call(&mut text, "x", &[-w / 2.0 + 10.0]);
        call(&mut text, "halign", &[0.0]);
        if let Kind::Frame(children) = &mut a.kind {
            children.push(text);
        }
        self.show_hint(&mut a);
        a
    }

    fn show_hint(&self, root: &mut Actor) {
        if let Some(a) = hit::find_mut(root, &self.name)
            && let Some(hint) = part(a, "Hint")
        {
            hint.visible = self.text.is_empty() && !self.typing;
        }
    }

    /// Opens (or updates) the popup with `self.suggestions`, under the
    /// field as it is on the screen.
    fn open(&mut self, root: &Actor, cx: &mut Cx) {
        let rect = hit::find(root, &self.name)
            .and_then(|p| hit::bounds_deep(root, &hit::IDENTITY, &p))
            .unwrap_or([0.0; 4]);
        let mut s = self.popup.borrow_mut();
        s.items = self.suggestions.clone();
        s.highlight = None;
        s.open = !s.items.is_empty();
        s.field = rect;
        s.dirty = true;
        if s.open && !s.layer {
            s.layer = true;
            drop(s);
            cx.overlay(
                Popup {
                    field: self.name.clone(),
                    state: self.popup.clone(),
                    look: self.look.clone(),
                    max_items: self.max_items,
                    item_h: self.size[1] * 0.85,
                    width: self.size[0],
                },
                false,
            );
        }
    }

    /// Closes the popup (a controller's Back, as Escape's first press);
    /// whether it was open.
    pub fn close_popup(&mut self) -> bool {
        let open = self.popup_open();
        self.close();
        open
    }

    /// Typing stopped by the screen (a controller's button rather than
    /// a key): the text kept, the popup closed.
    pub fn stop(&mut self, root: &mut Actor, cx: &mut Cx) {
        self.close();
        if self.typing {
            self.typing = false;
            cx.end_text();
        }
        set_text(root, &self.text_actor(), &self.text.clone());
        self.show_hint(root);
    }

    /// Typing could not start (no gesture for the platform's field).
    pub fn refused(&mut self, root: &mut Actor) {
        self.typing = false;
        self.show_hint(root);
    }

    fn close(&mut self) {
        let mut s = self.popup.borrow_mut();
        s.open = false;
        s.highlight = None;
        s.dirty = true;
    }

    /// Takes suggestion `i`: it becomes the text, typed into the
    /// platform's field too.
    fn take(&mut self, i: usize, root: &mut Actor, cx: &mut Cx) -> Option<FieldEvent> {
        let (s, _) = self.popup.borrow().items.get(i)?.clone();
        self.text = s.clone();
        cx.set_text(&s);
        set_text(root, &self.text_actor(), &s);
        self.close();
        Some(FieldEvent::Submitted(s))
    }

    /// A message for the screen: a tap on the popup.
    pub fn message(&mut self, m: &Message, root: &mut Actor, cx: &mut Cx) -> Option<FieldEvent> {
        if m.name != PICK_MESSAGE || m.get("Field") != Some(&Param::Str(self.name.clone())) {
            return None;
        }
        let Some(Param::Int(i)) = m.get("Item") else {
            return None;
        };
        self.take(*i as usize, root, cx)
    }

    /// An input. `suggest` gives the suggestions (text, a note such as
    /// "artist") for what is typed.
    pub fn input(
        &mut self,
        input: &Input,
        root: &mut Actor,
        cx: &mut Cx,
        suggest: &dyn Fn(&str) -> Vec<(String, String)>,
    ) -> (Handled, Option<FieldEvent>) {
        match input {
            Input::Activate { name } if *name == self.name => {
                self.typing = true;
                cx.begin_text(self.text_actor(), &self.text, self.hints);
                self.show_hint(root);
                (Handled::Yes, None)
            }
            Input::Text {
                name,
                change,
                field,
            } if *name == self.text_actor() => {
                self.text = field.text().to_string();
                match change {
                    TextChange::Edited => {
                        if field.is_composing() || self.text.trim().is_empty() {
                            self.close();
                        } else {
                            self.suggestions = suggest(&self.text);
                            self.suggestions.truncate(self.max_items);
                            if self.suggestions.is_empty() {
                                self.close();
                            } else {
                                self.open(root, cx);
                            }
                        }
                        (Handled::Yes, Some(FieldEvent::Changed(self.text.clone())))
                    }
                    TextChange::Moved => (Handled::Yes, None),
                    TextChange::Submit => {
                        let pick = {
                            let s = self.popup.borrow();
                            s.highlight.filter(|_| s.open)
                        };
                        if let Some(i) = pick {
                            let e = self.take(i, root, cx);
                            return (Handled::Yes, e);
                        }
                        self.close();
                        (Handled::Yes, Some(FieldEvent::Submitted(self.text.clone())))
                    }
                    TextChange::Cancel => {
                        if self.popup_open() {
                            self.close();
                        } else {
                            self.typing = false;
                            cx.end_text();
                            set_text(root, &self.text_actor(), &self.text.clone());
                            self.show_hint(root);
                        }
                        (Handled::Yes, None)
                    }
                    TextChange::Blurred => {
                        self.typing = false;
                        set_text(root, &self.text_actor(), &self.text.clone());
                        self.close();
                        self.show_hint(root);
                        (Handled::Yes, None)
                    }
                }
            }
            Input::Key(k) if self.popup_open() && k.down && !k.composing => {
                let mut s = self.popup.borrow_mut();
                let n = s.items.len().min(self.max_items);
                if n == 0 {
                    return (Handled::No, None);
                }
                let h = match (k.key.as_str(), s.highlight) {
                    ("ArrowDown", None) => 0,
                    ("ArrowDown", Some(i)) => (i + 1) % n,
                    ("ArrowUp", None | Some(0)) => n - 1,
                    ("ArrowUp", Some(i)) => i - 1,
                    _ => return (Handled::No, None),
                };
                s.highlight = Some(h);
                s.dirty = true;
                (Handled::Yes, None)
            }
            _ => (Handled::No, None),
        }
    }
}

/// The popup's layer.
struct Popup {
    field: String,
    state: Rc<RefCell<PopupState>>,
    look: Rc<dyn Look>,
    max_items: usize,
    item_h: f32,
    width: f32,
}

impl Popup {
    fn item(&self, i: usize) -> String {
        format!("{}.Item{i}", self.field)
    }
}

impl Screen for Popup {
    fn name(&self) -> &str {
        "popup"
    }

    fn build(&mut self, _cx: &mut Cx) -> Actor {
        let look: &dyn Look = &*self.look;
        let items = (0..self.max_items)
            .map(|i| {
                let mut it = look::component(
                    look,
                    "Popup.Item",
                    &Spec::new([self.width, self.item_h - 2.0], ""),
                );
                it.name = self.item(i);
                it.visible = false;
                it
            })
            .collect();
        Actor::new(Kind::Frame(items))
    }

    fn update(&mut self, _dt: f32, root: &mut Actor, cx: &mut Cx) {
        let mut s = self.state.borrow_mut();
        if !s.open {
            if s.layer {
                s.layer = false;
                cx.close(false);
            }
            return;
        }
        if !std::mem::take(&mut s.dirty) {
            return;
        }
        let n = s.items.len().min(self.max_items);
        let [l, top, r, bottom] = s.field;
        let x = (l + r) / 2.0;
        // Under the field, or over it when it would pass the visible
        // bottom.
        let below = bottom + self.item_h * n as f32 + 2.0 <= cx.viewport().screen.height;
        for i in 0..self.max_items {
            let Some(it) = hit::find_mut(root, &self.item(i)) else {
                continue;
            };
            match s.items.get(i).filter(|_| i < n) {
                Some((text, note)) => {
                    it.visible = true;
                    let y = if below {
                        bottom + 2.0 + self.item_h * (i as f32 + 0.5)
                    } else {
                        top - 2.0 - self.item_h * ((n - i) as f32 - 0.5)
                    };
                    call(it, "xy", &[x, y]);
                    set_text(it, "Text", text);
                    set_text(it, "Note", note);
                    play(
                        it,
                        if s.highlight == Some(i) {
                            "Select"
                        } else {
                            "Deselect"
                        },
                    );
                }
                None => it.visible = false,
            }
        }
    }

    fn input(&mut self, input: &Input, _root: &mut Actor, cx: &mut Cx) -> Handled {
        if let Input::Pointer {
            target: Some(t),
            gesture: Gesture::Tap,
            ..
        } = input
        {
            let prefix = format!("{}.Item", self.field);
            if let Some(i) = t
                .chain
                .iter()
                .find_map(|n| n.strip_prefix(&prefix)?.parse::<i64>().ok())
            {
                cx.broadcast(
                    Message::new(PICK_MESSAGE)
                        .with("Field", Param::Str(self.field.clone()))
                        .with("Item", Param::Int(i)),
                );
            }
            return Handled::Yes;
        }
        Handled::No
    }
}
