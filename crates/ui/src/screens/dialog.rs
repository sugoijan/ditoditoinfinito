//! What the dialogs share: the dimmed screen and panel, their buttons and
//! body lines, wrapping text, and the two dialogs other screens open: a
//! confirmation and a list of actions to pick from.

use std::rc::Rc;

use ddi_actor::{Actor, Kind};
use ddi_l10n::msg::{common, editor as em};

use super::{Env, left};
use crate::look::{self, Spec, call, quad_at};
use crate::nav::{Focusable, vertical_chain};
use crate::screen::{Cx, Handled, Input, Screen};

/// The dimmed screen around a dialog, its panel and its title.
pub(crate) fn shell(
    env: &Env,
    screen: [f32; 2],
    size: [f32; 2],
    title: &str,
) -> (Vec<Actor>, [f32; 2]) {
    let [w, h] = size;
    let x = (screen[0] - w) / 2.0;
    let y = (screen[1] - h) / 2.0;
    let dim = quad_at(
        "Dim",
        [screen[0] * 4.0, screen[1] * 4.0],
        [0.0, 0.0, 0.0, 0.6],
        [screen[0] / 2.0, screen[1] / 2.0],
    );
    let panel = env.component("Dialog", "Panel", &Spec::new(size, ""), "Panel", [x, y]);
    let mut t = env.text("Title", title, 18.0, 700, "Text");
    t = left(t, x + 24.0, y + 30.0);
    (vec![dim, panel, t], [x, y])
}

/// A dialog's `buttons` (name, label, component), left to right, at its
/// bottom right in rows of `per_row`, inside the dialog at `[x, y, w, h]`:
/// as wide as a row has room for, `max` at most while they fit in one.
pub(crate) fn button_rows(
    env: &Env,
    buttons: &[(&'static str, String, &'static str)],
    per_row: usize,
    [x, y, w, h]: [f32; 4],
    max: f32,
) -> Vec<Actor> {
    let room = w - 48.0;
    let rows = buttons.len().div_ceil(per_row);
    let mut out = Vec::new();
    for (r, chunk) in buttons.chunks(per_row).enumerate() {
        let k = chunk.len() as f32;
        let bw = ((room - (k - 1.0) * 10.0) / k).min(if rows > 1 { room } else { max });
        let mut bx = x + w - 24.0 - k * bw - (k - 1.0) * 10.0;
        let by = y + h - 44.0 - (rows - 1 - r) as f32 * 50.0;
        for (name, label, kind) in chunk {
            out.push(button(env, name, label, [bx + bw / 2.0, by], bw, kind));
            bx += bw + 10.0;
        }
    }
    out
}

pub(crate) fn button(
    env: &Env,
    name: &str,
    label: &str,
    centre: [f32; 2],
    w: f32,
    kind: &str,
) -> Actor {
    env.component(kind, "Button", &Spec::new([w, 40.0], label), name, centre)
}

/// `text` in lines no wider than `width` at `size` (monospace widths),
/// broken where Unicode allows (`ddi_actor::text::wrap`: between words,
/// between Japanese characters but never before 。、」 and their kind);
/// a piece too long for a line (a link, a hash) is cut. In a
/// pseudo-language a message («…») or data (‹…›) keeps its marks on each
/// of its lines, so that a line from its middle still reads as passed
/// through a message (`l10n_tests.rs`).
pub fn wrap_lines(text: &str, width: f32, size: f32) -> Vec<String> {
    let wrap = |text: &str, width: f32| {
        ddi_actor::text::wrap(text, width, true, &mut |s| look::mono_width(s, size))
    };
    if ddi_l10n::current().status == ddi_l10n::Status::Pseudo {
        for (open, close) in [('«', '»'), ('‹', '›')] {
            let Some(inner) = text.strip_prefix(open).and_then(|t| t.strip_suffix(close)) else {
                continue;
            };
            let marks = look::mono_width(&format!("{open}{close}"), size);
            let lines = wrap(inner, width - marks);
            if lines.len() > 1 {
                return lines
                    .into_iter()
                    .map(|l| format!("{open}{l}{close}"))
                    .collect();
            }
        }
    }
    wrap(text, width)
}

/// A dialog's body under its title, for the panel at `[x, y]`, `w` wide:
/// each line a text of its own, cut to the panel, in the color `color`
/// gives its index.
pub(crate) fn dialog_body<S: AsRef<str>>(
    env: &Env,
    children: &mut Vec<Actor>,
    lines: &[S],
    [x, y]: [f32; 2],
    w: f32,
    color: impl Fn(usize) -> &'static str,
) {
    for (i, line) in lines.iter().enumerate() {
        let mut b = env.text(&format!("Body{i}"), line.as_ref(), 14.0, 400, color(i));
        look::ellipsized(&mut b, w - 48.0);
        children.push(left(b, x + 24.0, y + 66.0 + i as f32 * 18.0));
    }
}

pub(crate) fn dialog_root(children: Vec<Actor>) -> Actor {
    let mut root = Actor::new(Kind::Frame(children));
    look::command_of(&mut root, "On", "diffusealpha,0;linear,0.12;diffusealpha,1");
    look::command_of(&mut root, "Off", "linear,0.1;diffusealpha,0");
    look::command_of(&mut root, "Cancel", "linear,0.1;diffusealpha,0");
    root
}

/// Keep a form's actions on screen when chrome or a short window leaves less
/// room than its natural height. Positions remain in the form's own space.
pub(crate) fn fitted_dialog(
    children: Vec<Actor>,
    screen: [f32; 2],
    height: f32,
    scale: f32,
) -> Actor {
    let mut root = dialog_root(children);
    let zoom = ((screen[1] - 16.0).max(1.0) / height).min(1.0);
    call(&mut root, "zoom", &[zoom * scale]);
    call(
        &mut root,
        "xy",
        &[
            screen[0] * (1.0 - zoom) * scale / 2.0,
            screen[1] * (1.0 - zoom) * scale / 2.0,
        ],
    );
    root
}

/// Told when the dialog's question is answered yes.
pub type ConfirmFn = Rc<dyn Fn(&mut Cx)>;

/// "Reset settings?": the consequence, then a button that does it and
/// one that does not.
pub struct ConfirmDialog {
    env: Env,
    title: String,
    body: String,
    yes: String,
    on_yes: ConfirmFn,
    /// A button between the two that does something first and leaves the
    /// dialog open ("Export first").
    extra: Option<(String, ConfirmFn)>,
    /// The first button's words where they are not Cancel ("Stay").
    no: Option<String>,
    /// The screen's name where a shell or a test tells it from the other
    /// questions.
    name: &'static str,
}

impl ConfirmDialog {
    pub fn new(env: Env, title: &str, body: &str, yes: &str, on_yes: ConfirmFn) -> ConfirmDialog {
        ConfirmDialog {
            env,
            title: title.into(),
            body: body.into(),
            yes: yes.into(),
            on_yes,
            extra: None,
            no: None,
            name: "confirm",
        }
    }

    /// With `label` on the button that changes nothing, in Cancel's
    /// place (still the one chosen first).
    pub fn with_no(mut self, label: &str) -> ConfirmDialog {
        self.no = Some(label.into());
        self
    }

    /// Named `name` among the layers.
    pub fn named(mut self, name: &'static str) -> ConfirmDialog {
        self.name = name;
        self
    }

    /// With a button `label` between Cancel and the confirmation that
    /// runs `on` and keeps the dialog open.
    pub fn with_extra(mut self, label: &str, on: ConfirmFn) -> ConfirmDialog {
        self.extra = Some((label.into(), on));
        self
    }
}

impl Screen for ConfirmDialog {
    fn presentation(&self) -> Option<crate::look::Presentation<'_>> {
        Some(self.env.presentation())
    }

    fn name(&self) -> &str {
        self.name
    }

    fn build(&mut self, cx: &mut Cx) -> Actor {
        let vp = cx.viewport().screen;
        let screen = [vp.width, vp.height];
        let env = self.env.clone();
        let w = 520.0_f32.min(screen[0] - 32.0);
        let body = wrap_lines(&self.body, w - 48.0, 14.0);
        let h = 160.0 + body.len() as f32 * 18.0;
        let (mut c, [x, y]) = shell(&env, screen, [w, h], &self.title);
        dialog_body(&env, &mut c, &body, [x, y], w, |_| "Muted");
        let no: String = match &self.no {
            Some(label) => label.clone(),
            None => common::cancel().into(),
        };
        let mut buttons = vec![("Cancel", no, "Button")];
        if let Some((label, _)) = &self.extra {
            buttons.push(("Extra", label.clone(), "Button"));
        }
        buttons.push(("Confirm", self.yes.clone(), "DangerButton"));
        let per_row = buttons.len();
        c.extend(button_rows(&env, &buttons, per_row, [x, y, w, h], 180.0));
        dialog_root(c)
    }

    fn focusables(&self) -> Vec<Focusable> {
        // Cancel first: Start alone never confirms.
        let mut v = vec![Focusable::new("Cancel")];
        if self.extra.is_some() {
            v.push(Focusable::new("Extra"));
        }
        v.push(Focusable::new("Confirm"));
        v
    }

    fn input(&mut self, input: &Input, _root: &mut Actor, cx: &mut Cx) -> Handled {
        match input {
            Input::Activate { name } if name == "Confirm" => {
                (self.on_yes)(cx);
                cx.close(false);
                Handled::Yes
            }
            Input::Activate { name } if name == "Cancel" => {
                cx.close(true);
                Handled::Yes
            }
            Input::Activate { name } if name == "Extra" => {
                if let Some((_, on)) = &self.extra {
                    on(cx);
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

/// What picking an action does (its id).
pub type PickFn = Rc<dyn Fn(&str, &mut Cx)>;

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
            crate::look::ellipsized(&mut t, w - 48.0);
            c.push(left(t, x + 24.0, y + 58.0));
        }
        let ids = self
            .items
            .iter()
            .skip(start)
            .take(self.per_page)
            .map(|(id, label)| (Self::name(id), label.clone()))
            .chain((self.page > 0).then(|| ("PreviousPage".into(), em::previous_page().into())))
            .chain((end < self.items.len()).then(|| ("NextPage".into(), em::next_page().into())))
            .chain(std::iter::once((
                "Close".to_string(),
                ddi_l10n::msg::common::close().into(),
            )));
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
        vertical_chain(&names)
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
