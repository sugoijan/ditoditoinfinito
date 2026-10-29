//! The options screen's dialogs: a confirmation (resets, removing a
//! noteskin), the guided binding ([`BindFlow`]: each lane in turn on the
//! device being bound, then a live test) and taking one key (or one
//! controller's control) for a lane.
//! The two that listen take keys and controllers' controls as they are
//! ([`Screen::raw_input`]); pointers still press their buttons.

use std::rc::Rc;

use ddi_actor::{Actor, Kind};
use ddi_platform::DeviceId;

use super::data::SharesState;
use super::options_shell::{SHARES_CHANGED, SharesFeed};
use super::select_dialogs::{button, dialog_root, fitted_dialog, shell, wrap, wrap_lines};
use super::{Env, left};
use crate::look::{self, Spec, call, quad};
use crate::message::Message;
use crate::nav::{Focusable, MenuAction};
use crate::request::{EnterKey, InputHints, InputMode, UiRequest};
use crate::screen::{Cx, Handled, Input, Screen};
use crate::state::bind::{BindFlow, Bound, Edge, Holds, Phase, pad_back};
use crate::widgets::field::{FieldEvent, SearchField};

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
        }
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
        "confirm"
    }

    fn build(&mut self, cx: &mut Cx) -> Actor {
        let vp = cx.viewport().screen;
        let screen = [vp.width, vp.height];
        let env = self.env.clone();
        let w = 520.0_f32.min(screen[0] - 32.0);
        let body = wrap_lines(&self.body, w - 48.0, 14.0);
        let h = 160.0 + body.len() as f32 * 18.0;
        let (mut c, [x, y]) = shell(&env, screen, [w, h], &self.title);
        for (i, line) in body.iter().enumerate() {
            let mut b = env.text(&format!("Body{i}"), line, 14.0, 400, "Muted");
            wrap(&mut b, w - 48.0);
            c.push(left(b, x + 24.0, y + 66.0 + i as f32 * 18.0));
        }
        // Cancel, the extra one, the confirmation: from the right.
        let n = if self.extra.is_some() { 3.0 } else { 2.0 };
        let bw = ((w - 48.0 - (n - 1.0) * 10.0) / n).min(180.0);
        let by = y + h - 44.0;
        let at = |k: f32| [x + w - 24.0 - bw / 2.0 - k * (bw + 10.0), by];
        c.push(button(&env, "Cancel", "Cancel", at(n - 1.0), bw, "Button"));
        if let Some((label, _)) = &self.extra {
            c.push(button(&env, "Extra", label, at(1.0), bw, "Button"));
        }
        c.push(button(
            &env,
            "Confirm",
            &self.yes,
            at(0.0),
            bw,
            "DangerButton",
        ));
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

/// Told the token typed.
pub type TokenFn = Rc<dyn Fn(&str, &mut Cx)>;

/// The upload token (`docs/plans/shared-songs.md`): one field to paste
/// it into, Save and Cancel. It shows while typed; the row never shows
/// it after.
pub struct TokenDialog {
    env: Env,
    field: SearchField,
    on_save: TokenFn,
}

impl TokenDialog {
    pub fn new(env: Env, token: &str, on_save: TokenFn) -> TokenDialog {
        let mut field = SearchField::new("TokenField", "paste the token", [0.0; 2], [1.0; 2]);
        field.hints = InputHints {
            mode: InputMode::Text,
            enter: EnterKey::Done,
            max_chars: Some(512),
        };
        field.text = token.to_string();
        TokenDialog {
            env,
            field,
            on_save,
        }
    }

    fn save(&mut self, cx: &mut Cx) {
        cx.end_text();
        (self.on_save)(&self.field.text, cx);
        cx.close(false);
    }
}

impl Screen for TokenDialog {
    fn presentation(&self) -> Option<crate::look::Presentation<'_>> {
        Some(self.env.presentation())
    }

    fn name(&self) -> &str {
        "token"
    }

    fn cursor(&self, target: &crate::hit::Target) -> Option<crate::request::Cursor> {
        self.field.cursor(target)
    }

    fn build(&mut self, cx: &mut Cx) -> Actor {
        let vp = cx.viewport().screen;
        let screen = [vp.width, vp.height];
        let env = self.env.clone();
        let w = 520.0_f32.min(screen[0] - 32.0);
        let body = wrap_lines(
            "The token whoever runs the sharing server gave you. Uploads carry it; the game \
             never shows it again.",
            w - 48.0,
            14.0,
        );
        let h = 230.0 + body.len() as f32 * 18.0;
        let (mut c, [x, y]) = shell(&env, screen, [w, h], "Upload token");
        for (i, line) in body.iter().enumerate() {
            let mut b = env.text(&format!("Body{i}"), line, 14.0, 400, "Muted");
            wrap(&mut b, w - 48.0);
            c.push(left(b, x + 24.0, y + 66.0 + i as f32 * 18.0));
        }
        let fy = y + 66.0 + body.len() as f32 * 18.0 + 34.0;
        self.field.centre = [x + w / 2.0, fy];
        self.field.size = [w - 48.0, 40.0];
        c.push(self.field.build(&*self.env.look));
        let bw = ((w - 48.0 - 10.0) / 2.0).min(180.0);
        let by = y + h - 44.0;
        c.push(button(
            &env,
            "Cancel",
            "Cancel",
            [x + w - 24.0 - bw / 2.0 - (bw + 10.0), by],
            bw,
            "Button",
        ));
        c.push(button(
            &env,
            "Save",
            "Save",
            [x + w - 24.0 - bw / 2.0, by],
            bw,
            "Button",
        ));
        env.state.borrow_mut().page_items =
            vec!["TokenField".into(), "Cancel".into(), "Save".into()];
        dialog_root(c)
    }

    fn focusables(&self) -> Vec<Focusable> {
        vec![
            Focusable::new("TokenField"),
            Focusable::new("Cancel"),
            Focusable::new("Save"),
        ]
    }

    fn input(&mut self, input: &Input, root: &mut Actor, cx: &mut Cx) -> Handled {
        if let Input::Menu { action, .. } = input
            && cx.menu_from_pad()
            && cx.focused() == Some("TokenField")
        {
            match action {
                MenuAction::Start => {
                    self.save(cx);
                    return Handled::Yes;
                }
                MenuAction::Back => {
                    cx.end_text();
                    cx.focus("Cancel");
                    return Handled::Yes;
                }
                action if action.is_direction() => cx.end_text(),
                _ => {}
            }
        }
        let (handled, event) = self.field.input(input, root, cx, &|_| Vec::new());
        if let Some(FieldEvent::Submitted(_)) = event {
            self.save(cx);
            return Handled::Yes;
        }
        if handled == Handled::Yes {
            return Handled::Yes;
        }
        match input {
            Input::Activate { name } if name == "Save" => {
                self.save(cx);
                Handled::Yes
            }
            Input::Activate { name } if name == "Cancel" => {
                cx.end_text();
                cx.close(true);
                Handled::Yes
            }
            Input::Outside => {
                cx.end_text();
                cx.close(true);
                Handled::Yes
            }
            _ => Handled::No,
        }
    }
}

/// Told the share (by id) to delete.
pub type DeleteFn = Rc<dyn Fn(&str)>;

/// Options' Shares dialog (`docs/plans/shared-songs.md`): the shares the
/// token may see, as the shell reads them into the feed
/// ([`SharesFeed`], [`SHARES_CHANGED`]): each with its pack, songs,
/// size and time left; the chosen one's link copied or opened, or the
/// share deleted (Delete pressed twice, as the plays list's Remove).
pub struct SharesDialog {
    env: Env,
    feed: SharesFeed,
    delete: DeleteFn,
    chosen: usize,
    first: usize,
    arming: bool,
    screen: [f32; 2],
    focused: bool,
    /// The wall clock at the last build (the times left).
    now: f64,
}

/// A row's height in the shares list.
const SHARE_ROW: f32 = 46.0;

impl SharesDialog {
    pub fn new(env: Env, feed: SharesFeed, delete: DeleteFn) -> SharesDialog {
        SharesDialog {
            env,
            feed,
            delete,
            chosen: 0,
            first: 0,
            arming: false,
            screen: super::DESIGN,
            focused: false,
            now: 0.0,
        }
    }

    fn per_page(&self) -> usize {
        let room = self.screen[1] - 16.0 - 70.0 - 2.0 * 50.0 - 40.0;
        ((room / (SHARE_ROW + 6.0)).floor() as usize).clamp(3, 8)
    }

    fn rows(&self) -> Vec<super::data::ShareRow> {
        self.feed.borrow().shares.clone()
    }

    /// The buttons now, `(name, label, kind)`.
    fn buttons(&self) -> Vec<(&'static str, String, &'static str)> {
        let view = self.feed.borrow();
        let n = view.shares.len();
        let mut v = Vec::new();
        if view.state == SharesState::Ready && n > 0 {
            v.push(("Copy", "Copy link".to_string(), "Button"));
            v.push(("Open", "Open".to_string(), "Button"));
            v.push((
                "Delete",
                if self.arming {
                    "Sure? Delete"
                } else {
                    "Delete"
                }
                .to_string(),
                "DangerButton",
            ));
            if self.first > 0 {
                v.push(("Newer", "Newer".to_string(), "Button"));
            }
            if self.first + self.per_page() < n {
                v.push(("Older", "Older".to_string(), "Button"));
            }
        }
        v.push(("Close", "Close".to_string(), "Button"));
        v
    }

    /// One row: the pack, then what it holds and when it goes.
    fn row(&self, i: usize, r: &super::data::ShareRow, at: [f32; 3]) -> Vec<Actor> {
        let env = &self.env;
        let [x, y, w] = at;
        let s = &r.summary;
        let now = self.now;
        let when = if !s.finished {
            "never finished".to_string()
        } else {
            ddi_library::share::expires_in(s.expires_at.map(|t| t as f64), now)
                .unwrap_or_else(|| "never expires".into())
        };
        let detail = format!(
            "{} {} · {} · {}{}",
            s.songs,
            if s.songs == 1 { "song" } else { "songs" },
            ddi_library::share::size_text(s.bytes),
            when,
            if s.mine { "" } else { " · another's" }
        );
        let chosen = i == self.chosen;
        let mut plate = look::component_or(
            &*env.look,
            "Button",
            "Button",
            &Spec::new([w, SHARE_ROW], ""),
        );
        plate.name = format!("Row{i}");
        call(&mut plate, "xy", &[x + w / 2.0, y + SHARE_ROW / 2.0]);
        // The chosen one ringed in the accent.
        let mut out = vec![plate];
        if chosen {
            let c = env.color("Accent");
            for (size, at) in [
                ([w + 6.0, 2.0], [x + w / 2.0, y - 2.0]),
                ([w + 6.0, 2.0], [x + w / 2.0, y + SHARE_ROW + 2.0]),
                ([2.0, SHARE_ROW + 6.0], [x - 2.0, y + SHARE_ROW / 2.0]),
                ([2.0, SHARE_ROW + 6.0], [x + w + 2.0, y + SHARE_ROW / 2.0]),
            ] {
                let mut q = quad("", size, c);
                call(&mut q, "xy", &at);
                out.push(q);
            }
        }
        let pack = if s.pack.is_empty() {
            s.id.clone()
        } else {
            s.pack.clone()
        };
        let mut title = env.text(&format!("Row{i}.Pack"), &pack, 15.0, 600, "Text");
        title.decoration = true;
        let mut line = env.text(&format!("Row{i}.Detail"), &detail, 12.0, 400, "Muted");
        line.decoration = true;
        out.push(left(title, x + 12.0, y + 14.0));
        out.push(left(line, x + 12.0, y + 32.0));
        out
    }

    fn rebuild(&mut self, root: &mut Actor, cx: &mut Cx, focus: &str) {
        *root = self.build(cx);
        root.finish_tweening();
        cx.focus(focus);
    }
}

impl Screen for SharesDialog {
    fn presentation(&self) -> Option<crate::look::Presentation<'_>> {
        Some(self.env.presentation())
    }

    fn name(&self) -> &str {
        "shares"
    }

    fn build(&mut self, cx: &mut Cx) -> Actor {
        let vp = cx.viewport().screen;
        self.screen = [vp.width, vp.height];
        self.now = cx.clock();
        let env = self.env.clone();
        let w = 720.0_f32.min(self.screen[0] - 32.0);
        let rows = self.rows();
        self.chosen = self.chosen.min(rows.len().saturating_sub(1));
        if self.chosen < self.first || self.chosen >= self.first + self.per_page() {
            self.first = self.chosen - self.chosen % self.per_page();
        }
        let page: Vec<(usize, &super::data::ShareRow)> = rows
            .iter()
            .enumerate()
            .skip(self.first)
            .take(self.per_page())
            .collect();
        let buttons = self.buttons();
        let room = w - 48.0;
        let widths: Vec<f32> = buttons
            .iter()
            .map(|(_, l, _)| look::mono_width(l, 15.0) + 32.0)
            .collect();
        let (state, error, admin) = {
            let v = self.feed.borrow();
            (v.state, v.error.clone(), v.admin)
        };
        let title = if admin {
            "SHARES · ALL"
        } else {
            "SHARES · YOURS"
        };
        let empty = match state {
            SharesState::Loading => Some("Reading the shares…".to_string()),
            SharesState::Failed => Some(format!("The shares could not be read: {error}")),
            SharesState::Ready if rows.is_empty() => Some("No share yet.".to_string()),
            SharesState::Ready => None,
        };
        let body = if empty.is_some() {
            30.0
        } else {
            page.len() as f32 * (SHARE_ROW + 6.0)
        };
        let h = (70.0 + body + 20.0 + 50.0 + 6.0).min(self.screen[1] - 16.0);
        let (mut children, [x, y]) = shell(&env, self.screen, [w, h], title);
        if let Some(text) = empty {
            for (i, line) in wrap_lines(&text, room, 14.0).iter().enumerate() {
                children.push(left(
                    env.text(&format!("Empty{i}"), line, 14.0, 400, "Muted"),
                    x + 24.0,
                    y + 76.0 + i as f32 * 18.0,
                ));
            }
        } else {
            for (k, (i, r)) in page.iter().enumerate() {
                children.extend(self.row(
                    *i,
                    r,
                    [x + 24.0, y + 60.0 + k as f32 * (SHARE_ROW + 6.0), room],
                ));
            }
            if rows.len() > page.len() {
                let t = env.text(
                    "Page",
                    &format!(
                        "{}–{} of {}",
                        self.first + 1,
                        self.first + page.len(),
                        rows.len()
                    ),
                    12.0,
                    400,
                    "Faint",
                );
                children.push(super::right(t, x + w - 24.0, y + 30.0));
            }
        }
        let by = y + h - 30.0;
        let mut bx = x + 24.0;
        for (i, (name, label, kind)) in buttons.iter().enumerate() {
            children.push(button(
                &env,
                name,
                label,
                [bx + widths[i] / 2.0, by],
                widths[i],
                kind,
            ));
            bx += widths[i] + 10.0;
        }
        // The rows and the buttons by name, for the debug state's rects.
        self.env.state.borrow_mut().page_items = page
            .iter()
            .map(|(i, _)| format!("Row{i}"))
            .chain(buttons.iter().map(|(n, _, _)| n.to_string()))
            .collect();
        if !std::mem::replace(&mut self.focused, true) {
            cx.focus("Close");
        }
        dialog_root(children)
    }

    fn focusables(&self) -> Vec<Focusable> {
        let n = self.rows().len();
        (self.first..(self.first + self.per_page()).min(n))
            .map(|i| Focusable::new(format!("Row{i}")))
            .chain(
                self.buttons()
                    .into_iter()
                    .map(|(n, _, _)| Focusable::new(n)),
            )
            .collect()
    }

    fn message(&mut self, m: &Message, root: &mut Actor, cx: &mut Cx) {
        if m.name == SHARES_CHANGED {
            self.arming = false;
            let focus = if self.rows().is_empty() {
                "Close"
            } else {
                "Copy"
            };
            self.rebuild(root, cx, focus);
        }
    }

    fn input(&mut self, input: &Input, root: &mut Actor, cx: &mut Cx) -> Handled {
        if self.arming
            && let Input::Menu { action, .. } = input
            && matches!(
                action,
                MenuAction::Up | MenuAction::Down | MenuAction::Left | MenuAction::Right
            )
        {
            self.arming = false;
            *root = self.build(cx);
            root.finish_tweening();
            return Handled::No;
        }
        match input {
            Input::Activate { name } => {
                if let Some(i) = name
                    .strip_prefix("Row")
                    .and_then(|n| n.parse::<usize>().ok())
                {
                    if i < self.rows().len() {
                        self.chosen = i;
                        self.arming = false;
                        self.rebuild(root, cx, "Copy");
                    }
                    return Handled::Yes;
                }
                let chosen = self.rows().get(self.chosen).cloned();
                match (name.as_str(), chosen) {
                    ("Copy", Some(r)) => cx.request(UiRequest::CopyText(r.link)),
                    ("Open", Some(r)) => cx.request(UiRequest::OpenUrl(r.link)),
                    ("Delete", Some(r)) => {
                        if self.arming {
                            self.arming = false;
                            self.feed.borrow_mut().state = SharesState::Loading;
                            (self.delete)(&r.summary.id);
                            self.rebuild(root, cx, "Close");
                        } else {
                            self.arming = true;
                            self.rebuild(root, cx, "Delete");
                        }
                    }
                    ("Newer", _) => {
                        let n = self.per_page();
                        self.first = self.first.saturating_sub(n);
                        self.chosen = self.first;
                        self.arming = false;
                        self.rebuild(root, cx, if self.first > 0 { "Newer" } else { "Older" });
                    }
                    ("Older", _) => {
                        let n = self.per_page();
                        self.first += n;
                        self.chosen = self.first;
                        self.arming = false;
                        let more = self.first + n < self.rows().len();
                        self.rebuild(root, cx, if more { "Older" } else { "Newer" });
                    }
                    ("Close", _) => cx.close(true),
                    _ => return Handled::No,
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

/// Told what was bound.
pub type BoundFn = Rc<dyn Fn(Bound, &mut Cx)>;

/// The guided binding.
pub struct BindDialog {
    env: Env,
    flow: BindFlow,
    capture: bool,
    candidate_release: Option<String>,
    /// Controllers' buttons held (a second cancels).
    holds: Holds,
    /// The layout's name ("Single").
    layout: String,
    on_save: BoundFn,
    screen: [f32; 2],
    scale: f32,
}

impl BindDialog {
    pub fn new(env: Env, flow: BindFlow, layout: &str, on_save: BoundFn) -> BindDialog {
        BindDialog {
            env,
            capture: !flow.standard_device(),
            candidate_release: None,
            flow,
            holds: Holds::default(),
            layout: layout.into(),
            on_save,
            screen: super::DESIGN,
            scale: 1.0,
        }
    }

    fn title(&self) -> String {
        match &self.flow.device {
            Some(DeviceId::Keyboard) => format!("Bind the keyboard for {}", self.layout),
            Some(DeviceId::Gamepad(id)) => format!("Bind “{id}” for {}", self.layout),
            _ => "Bind a device".into(),
        }
    }

    fn buttons(&self) -> Vec<(&'static str, &'static str, &'static str)> {
        let mut v = Vec::new();
        if self.flow.standard_device() && !self.capture && matches!(self.flow.phase, Phase::Step(_))
        {
            v.push(("Capture", "Capture", "PlayButton"));
        }
        if self.flow.phase == Phase::Review {
            v.push(("Save", "Save", "PlayButton"));
            v.push(("Redo", "Redo", "Button"));
        }
        if self.flow.can_skip() {
            v.push(("Skip", "Skip", "Button"));
        }
        v.push(("Cancel", "Cancel", "Button"));
        v
    }

    fn tree(&self) -> Actor {
        let env = self.env.clone();
        let screen = self.screen;
        let w = 600.0_f32.min(screen[0] - 32.0);
        let f = &self.flow;
        let steps = if f.device.is_some() {
            f.step_count()
        } else {
            0
        };
        let cols = if w > 480.0 { 3 } else { 2 };
        let step_rows = steps.div_ceil(cols);
        let prompt = if f.phase == Phase::Review && f.is_pad() {
            "Bindings ready. Choose Save, Redo or Cancel.".to_string()
        } else {
            f.prompt()
        };
        let mut lines: Vec<(String, &str)> = vec![(prompt, "Text")];
        if f.standard_device() {
            lines.push((
                if self.capture {
                    "Recording a control; any button may be bound. Hold to cancel."
                } else {
                    "Choose Capture to record this step. Directions navigate actions."
                }
                .into(),
                "Muted",
            ));
        }
        if f.phase == Phase::Choose {
            lines.push((
                "Controllers appear once one of their buttons is pressed.".into(),
                "Muted",
            ));
        }
        if let Phase::Step(s) = f.phase
            && s >= f.lanes
        {
            lines.push((
                "Skip keeps the current one; Start and Back are shared by every style.".into(),
                "Muted",
            ));
        }
        if let Some(wt) = &f.waiting
            && f.phase != Phase::Review
        {
            lines.push((
                format!("Waiting for {} to be released.", f.label(wt)),
                "Muted",
            ));
        }
        lines.push((
            match (f.phase == Phase::Review, f.is_pad()) {
                (true, true) => {
                    "Directions choose an action; confirm activates it. Hold a button to cancel."
                }
                (true, false) => "Enter (or Space) saves; Escape cancels.",
                (false, true) if self.capture => "Escape or holding a controller button cancels.",
                (false, _) => "Escape, a controller's Back or holding its button cancels.",
            }
            .into(),
            "Muted",
        ));
        if let Some(m) = &f.message {
            lines.push((m.clone(), "Danger"));
        }
        for warning in f.warnings() {
            lines.push((warning, "Danger"));
        }
        let mut wrapped: Vec<(String, &str)> = Vec::new();
        for (t, c) in lines {
            for l in wrap_lines(&t, w - 48.0, 14.0) {
                wrapped.push((l, c));
            }
        }
        let h = 140.0 + wrapped.len() as f32 * 20.0 + step_rows as f32 * 34.0;
        let (mut c, [x, y]) = shell(&env, screen, [w, h], &self.title());
        let mut ly = y + 64.0;
        for (i, (t, color)) in wrapped.iter().enumerate() {
            let mut a = env.text(
                &format!("Line{i}"),
                t,
                if i == 0 { 16.0 } else { 14.0 },
                if i == 0 { 600 } else { 400 },
                color,
            );
            wrap(&mut a, w - 48.0);
            c.push(left(a, x + 24.0, ly));
            ly += 20.0;
        }
        ly += 8.0;
        let cw = (w - 48.0 - (cols - 1) as f32 * 8.0) / cols as f32;
        for i in 0..steps {
            let (r, k) = (i / cols, i % cols);
            let cx0 = x + 24.0 + k as f32 * (cw + 8.0);
            let cy = ly + r as f32 * 34.0 + 14.0;
            let current = f.phase == Phase::Step(i);
            let lit = f.phase == Phase::Review
                && f.recorded[i]
                    .as_ref()
                    .is_some_and(|ctl| f.held.contains(ctl));
            let color = if current || lit { "Accent" } else { "Text" };
            let name = if i >= f.lanes {
                format!("{} (optional)", f.steps[i])
            } else {
                f.steps[i].clone()
            };
            let mut n = env.text(&format!("Step{i}"), &name, 13.0, 600, color);
            wrap(&mut n, cw * 0.5);
            c.push(left(n, cx0, cy));
            let mut v = env.text(
                &format!("Step{i}.Value"),
                &f.step_value(i),
                13.0,
                400,
                color,
            );
            wrap(&mut v, cw * 0.5 - 6.0);
            c.push(left(v, cx0 + cw * 0.5 + 6.0, cy));
        }
        let buttons = self.buttons();
        let n = buttons.len() as f32;
        let bw = ((w - 48.0 - (n - 1.0) * 10.0) / n).min(150.0);
        let mut bx = x + w - 24.0 - n * bw - (n - 1.0) * 10.0;
        for (name, label, kind) in buttons {
            c.push(button(
                &env,
                name,
                label,
                [bx + bw / 2.0, y + h - 44.0],
                bw,
                kind,
            ));
            bx += bw + 10.0;
        }
        fitted_dialog(c, screen, h, self.scale)
    }

    fn rebuild(&self, root: &mut Actor, cx: &Cx) {
        *root = self.tree();
        super::restore_focus(root, cx.focused());
        self.publish();
    }

    fn publish(&self) {
        let f = &self.flow;
        let mut st = self.env.state.borrow_mut();
        st.status = format!(
            "bind {} {}",
            match f.phase {
                Phase::Choose => "choose".to_string(),
                Phase::Step(s) => format!("step {s}"),
                Phase::Review => "review".into(),
            },
            f.message.clone().unwrap_or_default()
        )
        .trim_end()
        .to_string();
    }

    fn save(&self, cx: &mut Cx) {
        if let Some(b) = self.flow.bound() {
            (self.on_save)(b, cx);
        }
        cx.close(false);
    }
}

impl Screen for BindDialog {
    fn presentation(&self) -> Option<crate::look::Presentation<'_>> {
        Some(self.env.presentation())
    }

    fn name(&self) -> &str {
        "bind"
    }

    fn animating(&self) -> bool {
        self.holds.any()
    }

    fn update(&mut self, dt: f32, _root: &mut Actor, cx: &mut Cx) {
        // A lane held to test it in review does not count.
        let review = self.flow.phase == Phase::Review;
        let device = self.flow.device.clone();
        let flow = &self.flow;
        if self.holds.tick(dt, |d, c| {
            review && device.as_ref() == Some(d) && flow.is_lane(c)
        }) {
            self.holds = Holds::default();
            cx.close(true);
        }
    }

    fn build(&mut self, cx: &mut Cx) -> Actor {
        let vp = cx.viewport().screen;
        (self.screen, self.scale) = super::narrow_fit(&self.env, vp);
        self.publish();
        self.tree()
    }

    fn raw_input(&self) -> bool {
        true
    }

    fn focusables(&self) -> Vec<Focusable> {
        self.buttons()
            .into_iter()
            .map(|(n, _, _)| Focusable::new(n))
            .collect()
    }

    fn input(&mut self, input: &Input, root: &mut Actor, cx: &mut Cx) -> Handled {
        match input {
            Input::Raw(e) => {
                self.holds.edge(e);
                let chosen = self.flow.device.as_ref() == Some(&e.device);
                // Navigation is separate from capture, so face buttons have no
                // reserved meaning while a candidate is being recorded.
                let navigate = chosen
                    && (!self.capture || self.flow.phase == Phase::Review)
                    && self.flow.waiting.is_none();
                if navigate && let Some(action) = self.flow.menu_control(&e.control) {
                    if e.pressed {
                        let buttons = self.buttons();
                        let at = buttons
                            .iter()
                            .position(|(n, _, _)| Some(*n) == cx.focused())
                            .unwrap_or(0);
                        match MenuAction::from(action) {
                            MenuAction::Left
                            | MenuAction::Up
                            | MenuAction::Right
                            | MenuAction::Down => {
                                let next = if matches!(
                                    action,
                                    ddi_engine::MenuInput::Left | ddi_engine::MenuInput::Up
                                ) {
                                    at.saturating_sub(1)
                                } else {
                                    (at + 1).min(buttons.len() - 1)
                                };
                                cx.focus(buttons[next].0);
                            }
                            MenuAction::Start | MenuAction::ControllerStart => {
                                let name = buttons[at].0;
                                if name == "Capture" {
                                    self.capture = true;
                                    self.flow.waiting = Some(e.control.clone());
                                    self.flow.held.insert(e.control.clone());
                                    self.rebuild(root, cx);
                                } else {
                                    self.input(&Input::Activate { name: name.into() }, root, cx);
                                }
                            }
                            MenuAction::Back => cx.close(true),
                            _ => {}
                        }
                    }
                    return Handled::Yes;
                }
                if !chosen
                    || self.capture
                    || self.flow.phase == Phase::Choose
                    || self.flow.waiting.is_some()
                {
                    let before = self.flow.phase;
                    match self.flow.edge(e) {
                        Edge::Cancel => cx.close(true),
                        Edge::Save => self.save(cx),
                        Edge::Changed => {
                            if self.flow.standard_device() {
                                if before == Phase::Choose {
                                    self.capture = false;
                                } else if e.pressed && before != self.flow.phase {
                                    self.candidate_release = Some(e.control.clone());
                                }
                                if !e.pressed && self.candidate_release.as_ref() == Some(&e.control)
                                {
                                    self.candidate_release = None;
                                    self.capture = false;
                                    self.holds = Holds::default();
                                    cx.focus(if self.flow.phase == Phase::Review {
                                        "Save"
                                    } else {
                                        "Capture"
                                    });
                                }
                            }
                            self.rebuild(root, cx);
                        }
                        Edge::Ignored => {}
                    }
                } else if chosen && self.flow.phase == Phase::Review {
                    // Non-menu lanes still light in review without saving.
                    if e.pressed {
                        self.flow.held.insert(e.control.clone());
                    } else {
                        self.flow.held.remove(&e.control);
                    }
                    self.rebuild(root, cx);
                }
                Handled::Yes
            }
            Input::Activate { name } => {
                match name.as_str() {
                    "Capture" => {
                        self.capture = true;
                        self.rebuild(root, cx);
                    }
                    "Save" => self.save(cx),
                    "Redo" => {
                        self.flow.redo();
                        self.capture = !self.flow.standard_device();
                        cx.focus(if self.capture { "Cancel" } else { "Capture" });
                        self.rebuild(root, cx);
                    }
                    "Skip" => {
                        self.flow.skip();
                        cx.focus(if self.flow.phase == Phase::Review {
                            "Save"
                        } else {
                            "Capture"
                        });
                        self.rebuild(root, cx);
                    }
                    _ => cx.close(true),
                }
                Handled::Yes
            }
            Input::Menu {
                action: MenuAction::Back,
                ..
            } => {
                cx.close(true);
                Handled::Yes
            }
            _ => Handled::No,
        }
    }
}

/// Told the key (or the controller's control) taken.
pub type KeyFn = Rc<dyn Fn(String, &mut Cx)>;

/// "Press a key for Left": the next key pressed, Escape to cancel. Or,
/// for a controller ([`KeyDialog::for_pad`]), the next button, stick or
/// hat direction it presses.
pub struct KeyDialog {
    env: Env,
    lane: String,
    on_key: KeyFn,
    /// The controller whose control is taken (none: the keyboard's key).
    pad: Option<String>,
    /// The saved bindings and the standard controllers (their Back
    /// cancels), and the buttons held.
    pads: (ddi_engine::ControlBindings, Vec<String>),
    holds: Holds,
    message: Option<String>,
    screen: [f32; 2],
}

impl KeyDialog {
    pub fn new(env: Env, lane: &str, on_key: KeyFn) -> KeyDialog {
        KeyDialog {
            env,
            lane: lane.into(),
            on_key,
            pad: None,
            pads: Default::default(),
            holds: Holds::default(),
            message: None,
            screen: super::DESIGN,
        }
    }

    /// Controllers' Back cancels as these bindings read it.
    pub fn with_pads(
        mut self,
        controls: ddi_engine::ControlBindings,
        standard: Vec<String>,
    ) -> KeyDialog {
        self.pads = (controls, standard);
        self
    }

    /// The next control controller `id` presses, for a lane: not its
    /// Start (refused: Start and Back act before any lane), its Back
    /// cancels, as another controller's Back, Escape or holding a button.
    pub fn for_pad(
        env: Env,
        lane: &str,
        id: &str,
        controls: ddi_engine::ControlBindings,
        standard: Vec<String>,
        on_control: KeyFn,
    ) -> KeyDialog {
        let mut d = KeyDialog::new(env, lane, on_control).with_pads(controls, standard);
        d.pad = Some(id.to_string());
        d
    }

    /// The controller's own Start and Back (saved, else the standard
    /// ones).
    fn pad_start_back(&self, id: &str) -> (Option<String>, Option<String>) {
        let (controls, standard) = &self.pads;
        controls
            .pad(id, standard.iter().any(|s| s == id))
            .map_or((None, None), |p| (p.start, p.back))
    }

    fn say(&mut self, message: String, root: &mut Actor) {
        self.message = Some(message);
        let fresh = self.tree();
        if let (Kind::Frame(old), Kind::Frame(new)) = (&mut root.kind, fresh.kind) {
            *old = new;
        }
    }

    fn tree(&self) -> Actor {
        let env = self.env.clone();
        let w = 460.0_f32.min(self.screen[0] - 32.0);
        let h = 170.0;
        let (title, prompt) = if self.pad.is_some() {
            (
                format!("A button for {}", self.lane),
                "Press a button, a stick or a hat direction on the controller (Escape, its Back \
                 or holding a button cancels).",
            )
        } else {
            (
                format!("A key for {}", self.lane),
                "Press a key (Escape, a controller's Back or holding its button cancels).",
            )
        };
        let (mut c, [x, y]) = shell(&env, self.screen, [w, h], &title);
        let mut p = env.text("Prompt", prompt, 14.0, 600, "Text");
        wrap(&mut p, w - 48.0);
        c.push(left(p, x + 24.0, y + 66.0));
        if let Some(m) = &self.message {
            let mut a = env.text("Message", m, 13.0, 400, "Danger");
            wrap(&mut a, w - 48.0);
            c.push(left(a, x + 24.0, y + 90.0));
        }
        c.push(button(
            &env,
            "Cancel",
            "Cancel",
            [x + w - 24.0 - 70.0, y + h - 36.0],
            140.0,
            "Button",
        ));
        dialog_root(c)
    }
}

impl Screen for KeyDialog {
    fn presentation(&self) -> Option<crate::look::Presentation<'_>> {
        Some(self.env.presentation())
    }

    fn name(&self) -> &str {
        "key"
    }

    fn animating(&self) -> bool {
        self.holds.any()
    }

    fn update(&mut self, dt: f32, _root: &mut Actor, cx: &mut Cx) {
        if self.holds.tick(dt, |_, _| false) {
            self.holds = Holds::default();
            cx.close(true);
        }
    }

    fn build(&mut self, cx: &mut Cx) -> Actor {
        let vp = cx.viewport().screen;
        self.screen = [vp.width, vp.height];
        self.tree()
    }

    fn raw_input(&self) -> bool {
        true
    }

    fn focusables(&self) -> Vec<Focusable> {
        vec![Focusable::new("Cancel")]
    }

    fn input(&mut self, input: &Input, root: &mut Actor, cx: &mut Cx) -> Handled {
        match input {
            Input::Raw(e) if e.device == DeviceId::Keyboard && e.pressed => {
                match e.control.as_str() {
                    "Escape" => cx.close(true),
                    _ if self.pad.is_some() => {
                        self.say("Press it on the controller.".into(), root);
                    }
                    "Backspace" => {
                        self.say(
                            "Backspace leaves a song; press a different key.".into(),
                            root,
                        );
                    }
                    code => {
                        (self.on_key)(code.to_string(), cx);
                        cx.close(false);
                    }
                }
                Handled::Yes
            }
            // The controller's own: taken, unless it is its Start or Back.
            Input::Raw(e)
                if self
                    .pad
                    .as_ref()
                    .is_some_and(|id| e.device == DeviceId::Gamepad(id.clone())) =>
            {
                self.holds.edge(e);
                if !e.pressed {
                    return Handled::Yes;
                }
                let id = self.pad.clone().unwrap_or_default();
                let (start, back) = self.pad_start_back(&id);
                let label = || {
                    e.control
                        .parse::<ddi_platform::gamepad::Control>()
                        .map(|c| c.label())
                        .unwrap_or_else(|_| e.control.clone())
                };
                if back.as_deref() == Some(e.control.as_str()) {
                    cx.close(true);
                } else if start.as_deref() == Some(e.control.as_str()) {
                    self.say(
                        format!(
                            "{} is this controller's Start. Press a different one.",
                            label()
                        ),
                        root,
                    );
                } else {
                    (self.on_key)(e.control.clone(), cx);
                    cx.close(false);
                }
                Handled::Yes
            }
            Input::Raw(e) => {
                self.holds.edge(e);
                if e.pressed && pad_back(&self.pads.0, &self.pads.1, e) {
                    cx.close(true);
                }
                Handled::Yes
            }
            Input::Activate { .. } | Input::Outside => {
                cx.close(true);
                Handled::Yes
            }
            _ => Handled::No,
        }
    }
}
