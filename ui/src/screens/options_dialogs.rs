//! The options screen's dialogs: a confirmation (resets, removing a
//! noteskin), the guided binding ([`BindFlow`]: each lane in turn on the
//! device being bound, then a live test) and taking one key for a lane.
//! The two that listen take keys and controllers' controls as they are
//! ([`Screen::raw_input`]); pointers still press their buttons.

use std::rc::Rc;

use ddi_actor::{Actor, Kind};
use ddi_platform::DeviceId;

use super::select_dialogs::{button, dialog_root, shell, wrap, wrap_lines};
use super::{Env, left};
use crate::nav::{Focusable, MenuAction};
use crate::screen::{Cx, Handled, Input, Screen};
use crate::state::bind::{BindFlow, Bound, Edge, Holds, Phase, pad_back};

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

/// Told what was bound.
pub type BoundFn = Rc<dyn Fn(Bound, &mut Cx)>;

/// The guided binding.
pub struct BindDialog {
    env: Env,
    flow: BindFlow,
    /// Controllers' buttons held (a second cancels).
    holds: Holds,
    /// The layout's name ("Single").
    layout: String,
    on_save: BoundFn,
    screen: [f32; 2],
}

impl BindDialog {
    pub fn new(env: Env, flow: BindFlow, layout: &str, on_save: BoundFn) -> BindDialog {
        BindDialog {
            env,
            flow,
            holds: Holds::default(),
            layout: layout.into(),
            on_save,
            screen: super::DESIGN,
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
        let mut lines: Vec<(String, &str)> = vec![(f.prompt(), "Text")];
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
                (true, true) => "Start saves; Back, Escape or holding a button cancels.",
                (true, false) => "Enter (or Space) saves; Escape cancels.",
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
        for (i, (t, colour)) in wrapped.iter().enumerate() {
            let mut a = env.text(
                &format!("Line{i}"),
                t,
                if i == 0 { 16.0 } else { 14.0 },
                if i == 0 { 600 } else { 400 },
                colour,
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
            let colour = if current || lit { "Accent" } else { "Text" };
            let name = if i >= f.lanes {
                format!("{} (optional)", f.steps[i])
            } else {
                f.steps[i].clone()
            };
            let mut n = env.text(&format!("Step{i}"), &name, 13.0, 600, colour);
            wrap(&mut n, cw * 0.5);
            c.push(left(n, cx0, cy));
            let mut v = env.text(
                &format!("Step{i}.Value"),
                &f.step_value(i),
                13.0,
                400,
                colour,
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
        dialog_root(c)
    }

    fn rebuild(&self, root: &mut Actor) {
        let fresh = self.tree();
        if let (Kind::Frame(old), Kind::Frame(new)) = (&mut root.kind, fresh.kind) {
            *old = new;
        }
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
        self.screen = [vp.width, vp.height];
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
                match self.flow.edge(e) {
                    Edge::Cancel => cx.close(true),
                    Edge::Save => self.save(cx),
                    Edge::Changed => self.rebuild(root),
                    Edge::Ignored => {}
                }
                Handled::Yes
            }
            Input::Activate { name } => {
                match name.as_str() {
                    "Save" => self.save(cx),
                    "Redo" => {
                        self.flow.redo();
                        self.rebuild(root);
                    }
                    "Skip" => {
                        self.flow.skip();
                        self.rebuild(root);
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

/// Told the key taken.
pub type KeyFn = Rc<dyn Fn(String, &mut Cx)>;

/// "Press a key for Left": the next key pressed, Escape to cancel.
pub struct KeyDialog {
    env: Env,
    lane: String,
    on_key: KeyFn,
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

    fn tree(&self) -> Actor {
        let env = self.env.clone();
        let w = 460.0_f32.min(self.screen[0] - 32.0);
        let h = 170.0;
        let (mut c, [x, y]) = shell(
            &env,
            self.screen,
            [w, h],
            &format!("A key for {}", self.lane),
        );
        let mut p = env.text(
            "Prompt",
            "Press a key (Escape, a controller's Back or holding its button cancels).",
            14.0,
            600,
            "Text",
        );
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
                    "Backspace" => {
                        self.message =
                            Some("Backspace leaves a song; press a different key.".into());
                        let fresh = self.tree();
                        if let (Kind::Frame(old), Kind::Frame(new)) = (&mut root.kind, fresh.kind) {
                            *old = new;
                        }
                    }
                    code => {
                        (self.on_key)(code.to_string(), cx);
                        cx.close(false);
                    }
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
