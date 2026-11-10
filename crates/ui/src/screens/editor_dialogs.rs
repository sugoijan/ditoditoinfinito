//! The chart editor's dialogs: a list of actions to pick from (the menu,
//! the timing at the cursor, the song's and the chart's fields), in
//! columns when they do not fit one, and a value to type (a BPM, a
//! title), checked before it is taken.

use std::rc::Rc;

use ddi_actor::Actor;

use super::dialog::{button_rows, fitted_dialog, shell, wrap_lines};
use super::{Env, left};
use crate::nav::{Focusable, MenuAction};
use crate::request::{EnterKey, InputHints, InputMode};
use crate::screen::{Cx, Handled, Input, Screen};
use crate::widgets::field::{FieldEvent, SearchField};
use crate::widgets::set_text;
use ddi_l10n::msg::editor as em;

/// What a typed value does: `Err` says why it was not taken.
pub type SubmitFn = Rc<dyn Fn(&str, &mut Cx) -> Result<(), String>>;

/// A value to type: Enter or OK takes it (when it can be), Escape or
/// Cancel leaves it as it was.
pub struct ValueDialog {
    env: Env,
    title: String,
    note: String,
    field: SearchField,
    on_submit: SubmitFn,
    error: String,
    /// Typing has started (the field was activated once).
    started: bool,
}

impl ValueDialog {
    pub fn new(env: Env, title: &str, note: &str, value: &str, on_submit: SubmitFn) -> Self {
        let mut field = SearchField::new("Value", "", [0.0, 0.0], [300.0, 40.0]);
        field.text = value.into();
        field.max_items = 0;
        field.hints = InputHints {
            mode: InputMode::Text,
            enter: EnterKey::Done,
            ..InputHints::default()
        };
        ValueDialog {
            env,
            title: title.into(),
            note: note.into(),
            field,
            on_submit,
            error: String::new(),
            started: false,
        }
    }

    /// A whole number to type: the keyboard of digits where there is one.
    pub fn numeric(mut self) -> Self {
        self.field.hints.mode = InputMode::Numeric;
        self
    }

    /// The same validated value, edited through numeric controls.
    pub fn numbers(self, numbers: super::numbers::Numbers) -> super::numbers::NumberDialog {
        super::numbers::NumberDialog::new(self.env, &self.title, numbers, self.on_submit)
    }

    fn submit(&mut self, text: &str, root: &mut Actor, cx: &mut Cx) {
        match (self.on_submit)(text, cx) {
            Ok(()) => {
                cx.end_text();
                cx.close(false);
            }
            Err(e) => {
                self.error = e;
                set_text(root, "Error", &self.error);
            }
        }
    }
}

impl Screen for ValueDialog {
    fn presentation(&self) -> Option<crate::look::Presentation<'_>> {
        Some(self.env.presentation())
    }

    fn cursor(&self, target: &crate::hit::Target) -> Option<crate::request::Cursor> {
        self.field.cursor(target)
    }

    fn name(&self) -> &str {
        "editor-value"
    }

    fn build(&mut self, cx: &mut Cx) -> Actor {
        let vp = cx.viewport().screen;
        let (screen, scale) = super::narrow_fit(&self.env, vp);
        let env = self.env.clone();
        let w = 480.0_f32.min(screen[0] - 32.0);
        let note = wrap_lines(&self.note, w - 48.0, 13.0);
        let h = 196.0 + note.len() as f32 * 17.0;
        let (mut c, [x, y]) = shell(&env, screen, [w, h], &self.title);
        let mut at = y + 58.0;
        for (i, line) in note.iter().enumerate() {
            c.push(left(
                env.text(&format!("Note{i}"), line, 13.0, 400, "Muted"),
                x + 24.0,
                at,
            ));
            at += 17.0;
        }
        self.field.centre = [x + w / 2.0, at + 24.0];
        self.field.size = [w - 48.0, 40.0];
        self.field.look = env.look.clone();
        c.push(self.field.build(&*env.look));
        let mut error = env.text("Error", &self.error, 13.0, 400, "Accent");
        crate::look::ellipsized(&mut error, w - 48.0);
        c.push(left(error, x + 24.0, at + 58.0));
        let buttons = [
            ("Cancel", ddi_l10n::msg::common::cancel().into(), "Button"),
            ("Ok", em::ok().into(), "Button"),
        ];
        // Eight lower than the other dialogs' buttons.
        c.extend(button_rows(&env, &buttons, 2, [x, y + 8.0, w, h], 160.0));
        let mut root = fitted_dialog(c, screen, h, scale);
        if !self.started {
            // Typing at once: the dialog is opened by a press.
            self.started = true;
            let activate = Input::Activate {
                name: self.field.name.clone(),
            };
            self.field.input(&activate, &mut root, cx, &|_| Vec::new());
            cx.focus(self.field.name.clone());
        }
        root
    }

    fn focusables(&self) -> Vec<Focusable> {
        vec![
            Focusable::new(self.field.name.clone()),
            Focusable::new("Ok"),
            Focusable::new("Cancel"),
        ]
    }

    fn input(&mut self, input: &Input, root: &mut Actor, cx: &mut Cx) -> Handled {
        if let Input::Menu { action, .. } = input
            && cx.menu_from_pad()
        {
            if *action == MenuAction::Start && cx.focused() == Some(self.field.name.as_str()) {
                let text = self.field.text.clone();
                self.submit(&text, root, cx);
                return Handled::Yes;
            }
            if action.is_direction() {
                cx.end_text();
            }
        }
        let (handled, event) = self.field.input(input, root, cx, &|_| Vec::new());
        if let Some(FieldEvent::Submitted(text)) = event {
            self.submit(&text, root, cx);
            return Handled::Yes;
        }
        if let Input::Text {
            change: crate::screen::TextChange::Cancel,
            ..
        } = input
        {
            cx.end_text();
            cx.close(true);
            return Handled::Yes;
        }
        if handled == Handled::Yes {
            return handled;
        }
        match input {
            Input::Activate { name } if name == "Ok" => {
                let text = self.field.text.clone();
                self.submit(&text, root, cx);
                Handled::Yes
            }
            Input::Activate { name } if name == "Cancel" => {
                cx.end_text();
                cx.close(true);
                Handled::Yes
            }
            Input::Menu {
                action: MenuAction::Back,
                ..
            }
            | Input::Outside => {
                cx.end_text();
                cx.close(true);
                Handled::Yes
            }
            _ => Handled::No,
        }
    }
}
