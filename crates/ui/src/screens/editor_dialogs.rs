//! The chart editor's dialogs: a list of actions to pick from (the menu,
//! the timing at the cursor, the song's and the chart's fields), in
//! columns when they do not fit one, and a value to type (a BPM, a
//! title), checked before it is taken.

use std::rc::Rc;

use ddi_actor::Actor;

use super::select_dialogs::{button, fitted_dialog, shell, wrap, wrap_lines};
use super::{Env, left};
use crate::nav::{Focusable, MenuAction};
use crate::request::{EnterKey, InputHints, InputMode};
use crate::screen::{Cx, Handled, Input, Screen};
use crate::widgets::field::{FieldEvent, SearchField};
use crate::widgets::set_text;

/// What picking an action does (its id).
pub type PickFn = Rc<dyn Fn(&str, &mut Cx)>;

/// What a typed value does: `Err` says why it was not taken.
pub type SubmitFn = Rc<dyn Fn(&str, &mut Cx) -> Result<(), String>>;

/// Height of an action's button and the gap after it.
const ROW: f32 = 40.0;
const GAP: f32 = 8.0;

/// A list of actions: picking one closes the list, then does it.
pub struct ActionsDialog {
    env: Env,
    title: String,
    note: String,
    /// `(id, label)`.
    items: Vec<(String, String)>,
    on_pick: PickFn,
    page: usize,
    per_page: usize,
}

impl ActionsDialog {
    pub fn new(env: Env, title: &str, items: Vec<(String, String)>, on_pick: PickFn) -> Self {
        ActionsDialog {
            env,
            title: title.into(),
            note: String::new(),
            items,
            on_pick,
            page: 0,
            per_page: usize::MAX,
        }
    }

    /// With a line of text under the title.
    pub fn with_note(mut self, note: &str) -> Self {
        self.note = note.into();
        self
    }

    fn name(id: &str) -> String {
        format!("Item.{id}")
    }
}

impl Screen for ActionsDialog {
    fn presentation(&self) -> Option<crate::look::Presentation<'_>> {
        Some(self.env.presentation())
    }

    fn name(&self) -> &str {
        "editor-actions"
    }

    fn build(&mut self, cx: &mut Cx) -> Actor {
        let vp = cx.viewport().screen;
        let (screen, scale) = super::narrow_fit(&self.env, vp);
        let env = self.env.clone();
        let top = if self.note.is_empty() { 64.0 } else { 86.0 };
        let fit = (((screen[1] - 32.0 - top - 20.0) / (ROW + GAP)).floor() as usize).max(3);
        let columns = if screen[0] >= 720.0 { 2 } else { 1 };
        self.per_page = (fit * columns).saturating_sub(3).max(1);
        self.page = self
            .page
            .min(self.items.len().saturating_sub(1) / self.per_page);
        let start = self.page * self.per_page;
        let end = (start + self.per_page).min(self.items.len());
        let n = end - start + 1 + usize::from(self.page > 0) + usize::from(end < self.items.len());
        let columns = columns.min(n);
        let rows = n.div_ceil(columns);
        let col_w =
            ((screen[0] - 32.0 - 48.0 - (columns - 1) as f32 * 10.0) / columns as f32).min(300.0);
        let w = (columns as f32 * col_w + (columns - 1) as f32 * 10.0 + 48.0).max(320.0);
        let w = w.min(screen[0] - 16.0);
        let h = top + rows as f32 * (ROW + GAP) + 20.0;
        let (mut c, [x, y]) = shell(&env, screen, [w, h], &self.title);
        if !self.note.is_empty() {
            let mut t = env.text("Note", &self.note, 13.0, 400, "Muted");
            wrap(&mut t, w - 48.0);
            c.push(left(t, x + 24.0, y + 58.0));
        }
        let ids = self
            .items
            .iter()
            .skip(start)
            .take(self.per_page)
            .map(|(id, label)| (Self::name(id), label.clone()))
            .chain((self.page > 0).then(|| ("PreviousPage".into(), "Previous page".into())))
            .chain((end < self.items.len()).then(|| ("NextPage".into(), "Next page".into())))
            .chain(std::iter::once(("Close".to_string(), "Close".to_string())));
        for (i, (name, label)) in ids.enumerate() {
            let (col, row) = (i / rows, i % rows);
            let cx_ = x + 24.0 + col as f32 * (col_w + 10.0) + col_w / 2.0;
            let cy = y + top + row as f32 * (ROW + GAP) + ROW / 2.0;
            c.push(button(&env, &name, &label, [cx_, cy], col_w, "Button"));
        }
        // Where the shells' debug views find the buttons.
        env.state.borrow_mut().page_items = self.focusables().into_iter().map(|f| f.name).collect();
        fitted_dialog(c, screen, h, scale)
    }

    fn focusables(&self) -> Vec<Focusable> {
        let start = self.page.saturating_mul(self.per_page);
        let mut names: Vec<_> = self
            .items
            .iter()
            .skip(start)
            .take(self.per_page)
            .map(|(id, _)| Self::name(id))
            .collect();
        if self.page > 0 {
            names.push("PreviousPage".into());
        }
        if start.saturating_add(self.per_page) < self.items.len() {
            names.push("NextPage".into());
        }
        names.push("Close".into());
        names
            .iter()
            .enumerate()
            .map(|(i, n)| {
                let mut f = Focusable::new(n);
                f.up = i.checked_sub(1).map(|j| names[j].clone());
                f.down = names.get(i + 1).cloned();
                f
            })
            .collect()
    }

    fn input(&mut self, input: &Input, root: &mut Actor, cx: &mut Cx) -> Handled {
        match input {
            Input::Activate { name } if name == "PreviousPage" || name == "NextPage" => {
                if name == "PreviousPage" {
                    self.page = self.page.saturating_sub(1);
                } else {
                    self.page += 1;
                }
                *root = self.build(cx);
                if let Some(f) = self.focusables().first() {
                    cx.focus(f.name.clone());
                }
                Handled::Yes
            }
            Input::Activate { name } if name == "Close" => {
                cx.close(true);
                Handled::Yes
            }
            Input::Activate { name } => {
                if let Some(id) = name.strip_prefix("Item.") {
                    cx.close(false);
                    (self.on_pick)(id, cx);
                }
                Handled::Yes
            }
            Input::Outside => {
                cx.close(true);
                Handled::Yes
            }
            _ => Handled::No,
        }
    }
}

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
        wrap(&mut error, w - 48.0);
        c.push(left(error, x + 24.0, at + 58.0));
        let bw = ((w - 58.0) / 2.0).min(160.0);
        let by = y + h - 36.0;
        c.push(button(
            &env,
            "Cancel",
            "Cancel",
            [x + w - 24.0 - bw * 1.5 - 10.0, by],
            bw,
            "Button",
        ));
        c.push(button(
            &env,
            "Ok",
            "OK",
            [x + w - 24.0 - bw / 2.0, by],
            bw,
            "Button",
        ));
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
