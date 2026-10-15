//! Options (`docs/plans/theme.md`, drawn in layout A):
//!
//! - **Left**: the sections (`Options.Section`).
//! - **Middle**: option rows as StepMania lays them out (`Options.Row`):
//!   the label on the left, the choices on the right with the chosen one
//!   underlined (`OptionRow.Choice`, `OptionRow.Cursor`); numbers step
//!   with ◀ value ▶.
//! - **Right**: a preview of the field (`Options.Preview`; a placeholder
//!   until step 6 draws the field there).
//! - **Below**: what the focused option does, in a sentence
//!   (`Options.Description`).
//! - **Keys**: Up and Down move through the options, Left and Right
//!   change the focused one, Tab (the menu's Select) moves to the next
//!   section, Back leaves. Changes are saved as they are made (the
//!   shell's [`ChangeFn`]).

use std::rc::Rc;

use ddi_actor::{Actor, Kind};

use super::data::{OptionDef, OptionKind, OptionsData};
use super::{Env, fit, fit_root, footer, frame, ground, header, left, right};
use crate::hit;
use crate::look::{self, Spec, call};
use crate::nav::{Focusable, MenuAction};
use crate::screen::{Cx, Handled, Input, Screen};
use crate::widgets::set_text;

/// Told each change: the section's name and the option as it is now.
pub type ChangeFn = Rc<dyn Fn(&str, &OptionDef)>;

const ROW: f32 = 42.0;
const ROW_GAP: f32 = 4.0;
const HEADING: f32 = 26.0;
const LABEL_W: f32 = 220.0;

pub struct OptionsScreen {
    env: Env,
    data: OptionsData,
    section: usize,
    focus: usize,
    screen: [f32; 2],
    changed: Option<ChangeFn>,
    lit: Option<String>,
}

impl OptionsScreen {
    pub fn new(env: Env, data: OptionsData, changed: Option<ChangeFn>) -> OptionsScreen {
        OptionsScreen {
            env,
            data,
            section: 0,
            focus: 0,
            screen: super::DESIGN,
            changed,
            lit: None,
        }
    }

    fn options(&self) -> &[OptionDef] {
        self.data
            .sections
            .get(self.section)
            .map_or(&[], |s| &s.options)
    }

    fn places(&self) -> ([f32; 3], [f32; 3], [f32; 4], [f32; 4]) {
        let env = &self.env;
        let [w, h] = self.screen;
        let design_w = env.metric("Design.Width", 1280.0);
        let m = env.metric("Margin", 32.0);
        let rows_x = env.metric("Options.Rows.X", 272.0);
        let preview_x = w - (design_w - env.metric("Options.Preview.X", 904.0));
        let desc_y = h - 116.0;
        (
            [m, 88.0, env.metric("Options.Rail.Width", 208.0)],
            [rows_x, 88.0, env.metric("Options.Rows.Width", 600.0)],
            [preview_x, 88.0, 344.0, (desc_y - 16.0 - 88.0).min(500.0)],
            [rows_x, desc_y, w - rows_x - m, 60.0],
        )
    }

    fn build_tree(&mut self) -> Actor {
        let env = self.env.clone();
        let screen = self.screen;
        let (rail, rows, preview, desc) = self.places();
        let mut children = vec![ground(&env, screen)];
        // The sections.
        let mut items = Vec::new();
        for (i, s) in self.data.sections.iter().enumerate() {
            let mut a = env.component(
                "Options.Section",
                "List.Header",
                &Spec::new([rail[2], 44.0], &s.name),
                &format!("Section{i}"),
                [0.0, i as f32 * 48.0],
            );
            set_text(&mut a, "Title", &s.name);
            crate::widgets::play(
                &mut a,
                if i == self.section {
                    "Select"
                } else {
                    "Deselect"
                },
            );
            a.finish_tweening();
            items.push(a);
        }
        let mut r = frame("Sections", items);
        call(&mut r, "xy", &[rail[0], rail[1]]);
        children.push(r);
        children.push(self.rows_actor(rows));
        // The preview: a placeholder for the field until step 6.
        let [px, py, pw, ph] = preview;
        let mut p = env.component(
            "Options.Preview",
            "Panel",
            &Spec::new([pw, ph], &self.data.preview),
            "Preview",
            [px, py],
        );
        set_text(&mut p, "Title", &self.data.preview);
        children.push(p);
        let [dx, dy, dw, dh] = desc;
        children.push(env.component(
            "Options.Description",
            "Panel",
            &Spec::new([dw, dh], ""),
            "Description",
            [dx, dy],
        ));
        children.push(header(&env, screen[0], "OPTIONS"));
        let margin = env.metric("Margin", 32.0);
        children.push(right(
            env.text("Saved", "Saved as you change them", 13.0, 400, "Muted"),
            screen[0] - margin,
            32.0,
        ));
        let (f, texts) = footer(
            &env,
            screen,
            &[
                (&["↑↓"], "Option"),
                (&["←→"], "Change"),
                (&["TAB"], "Section"),
                (&["ESC"], "Back"),
            ],
            "",
            &[("Link.Back", "Back to songs")],
        );
        env.state.borrow_mut().hints = texts;
        children.push(f);
        let mut root = Actor::new(Kind::Frame(children));
        super::entrance(&mut root, &env);
        self.lit = None;
        root
    }

    /// The rows of the section, with their headings.
    fn rows_actor(&self, [x, y, w]: [f32; 3]) -> Actor {
        let env = &self.env;
        let mut children = Vec::new();
        let mut at = 0.0;
        for (i, o) in self.options().iter().enumerate() {
            if let Some(h) = &o.heading {
                let t = look::tracked(env.text(&format!("Heading{i}"), h, 11.0, 600, "Faint"), 120);
                children.push(left(t, 16.0, at + HEADING / 2.0 + 2.0));
                at += HEADING;
            }
            let mut row = env.component(
                "Options.Row",
                "List.Row",
                &Spec::new([w, ROW], &o.label),
                &format!("Option{i}"),
                [0.0, at],
            );
            set_text(&mut row, "Label", &o.label);
            set_text(&mut row, "Title", &o.label);
            if let Kind::Frame(c) = &mut row.kind {
                c.push(self.values(i, o));
            }
            // `Select`, not focus: the rows' frame is what takes focus.
            crate::widgets::play(
                &mut row,
                if i == self.focus {
                    "Select"
                } else {
                    "Deselect"
                },
            );
            row.finish_tweening();
            children.push(row);
            at += ROW + ROW_GAP;
        }
        let mut f = frame("Rows", children);
        call(&mut f, "xy", &[x, y]);
        f
    }

    /// A row's values: choices with the chosen one underlined, or a
    /// stepper.
    fn values(&self, i: usize, o: &OptionDef) -> Actor {
        let env = &self.env;
        let x0 = 16.0 + LABEL_W;
        let mid = ROW / 2.0;
        let mut c = Vec::new();
        match &o.kind {
            OptionKind::Choices(choices) => {
                let mut x = x0;
                for (j, ch) in choices.iter().enumerate() {
                    let chosen = j == o.index;
                    let w = look::mono_width(ch, 15.0);
                    let mut t = env.text(
                        &format!("Option{i}.Choice{j}"),
                        ch,
                        15.0,
                        if chosen { 600 } else { 400 },
                        if chosen { "Text" } else { "Faint" },
                    );
                    t = left(t, x, mid);
                    c.push(t);
                    if chosen {
                        let mut cur = look::component(
                            &*env.look,
                            "OptionRow.Cursor",
                            &Spec::new([w, 2.0], ""),
                        );
                        cur.name = format!("Option{i}.Cursor");
                        call(&mut cur, "xy", &[x + w / 2.0, mid + 12.0]);
                        c.push(cur);
                    }
                    x += w + 16.0;
                }
            }
            OptionKind::Stepper(values) => {
                let v = values.get(o.index).map_or("", String::as_str);
                c.push(left(
                    env.text(&format!("Option{i}.Down"), "◀", 14.0, 400, "Accent"),
                    x0,
                    mid,
                ));
                let mut val = env.numbers(&format!("Option{i}.Value"), v, 22.0, "Text");
                call(&mut val, "xy", &[x0 + 22.0 + 40.0, mid]);
                c.push(val);
                c.push(left(
                    env.text(&format!("Option{i}.Up"), "▶", 14.0, 400, "Accent"),
                    x0 + 22.0 + 80.0 + 8.0,
                    mid,
                ));
                if !o.note.is_empty() {
                    c.push(left(
                        env.text(&format!("Option{i}.Note"), &o.note, 13.0, 400, "Muted"),
                        x0 + 140.0,
                        mid,
                    ));
                }
            }
        }
        frame(&format!("Option{i}.Values"), c)
    }

    fn refresh(&mut self, root: &mut Actor) {
        let (_, rows, _, _) = self.places();
        let fresh = self.rows_actor(rows);
        if let Some(r) = hit::find_mut(root, "Rows") {
            *r = fresh;
        }
        for i in 0..self.data.sections.len() {
            super::play(
                root,
                &format!("Section{i}"),
                if i == self.section {
                    "Select"
                } else {
                    "Deselect"
                },
            );
        }
        let (label, description) = self
            .options()
            .get(self.focus)
            .map(|o| (o.label.clone(), o.description.clone()))
            .unwrap_or_default();
        super::set(root, "Description", "Title", &label);
        super::set(root, "Description", "Note", &description);
        self.lit = None;
        self.publish();
    }

    fn change(&mut self, by: i32, root: &mut Actor) {
        let section = self.section;
        let Some(o) = self
            .data
            .sections
            .get_mut(section)
            .and_then(|s| s.options.get_mut(self.focus))
        else {
            return;
        };
        let n = o.values().len() as i32;
        let to = (o.index as i32 + by).clamp(0, (n - 1).max(0)) as usize;
        if to == o.index {
            return;
        }
        o.index = to;
        let o = o.clone();
        let name = self.data.sections[section].name.clone();
        self.refresh(root);
        if let Some(f) = &self.changed {
            f(&name, &o);
        }
        self.env.state.borrow_mut().status = format!("{} {}", o.label, o.value());
    }

    fn set_section(&mut self, i: usize, root: &mut Actor) {
        if i >= self.data.sections.len() {
            return;
        }
        self.section = i;
        self.focus = 0;
        self.refresh(root);
    }

    fn publish(&self) {
        let mut st = self.env.state.borrow_mut();
        st.screen = "options".into();
        st.section = self
            .data
            .sections
            .get(self.section)
            .map(|s| s.name.clone())
            .unwrap_or_default();
        let o = self.options().get(self.focus);
        st.option = o.map(|o| o.label.clone()).unwrap_or_default();
        st.value = o.map(|o| o.value().to_string()).unwrap_or_default();
    }
}

impl Screen for OptionsScreen {
    fn name(&self) -> &str {
        "options"
    }

    fn build(&mut self, cx: &mut Cx) -> Actor {
        self.env.state.borrow_mut().over += 1;
        let vp = cx.viewport().screen;
        self.screen = fit(&*self.env.look, vp);
        let mut root = self.build_tree();
        fit_root(&*self.env.look, &mut root, vp);
        self.refresh(&mut root);
        root
    }

    fn focusables(&self) -> Vec<Focusable> {
        let mut v = vec![Focusable::new("Rows"), Focusable::new("Link.Back")];
        v.extend((0..self.data.sections.len()).map(|i| Focusable::new(format!("Section{i}"))));
        v
    }

    fn update(&mut self, _dt: f32, root: &mut Actor, cx: &mut Cx) {
        let vp = cx.viewport().screen;
        if fit(&*self.env.look, vp) != self.screen {
            self.screen = fit(&*self.env.look, vp);
            *root = self.build_tree();
            fit_root(&*self.env.look, root, vp);
            self.refresh(root);
            root.finish_tweening();
        }
        if self.env.state.borrow().screen != "options" {
            self.publish();
        }
    }

    fn input(&mut self, input: &Input, root: &mut Actor, cx: &mut Cx) -> Handled {
        let n = self.options().len();
        match input {
            Input::Menu { action, .. } => match action {
                MenuAction::Up => {
                    self.focus = self.focus.saturating_sub(1);
                    self.refresh(root);
                    Handled::Yes
                }
                MenuAction::Down => {
                    self.focus = (self.focus + 1).min(n.saturating_sub(1));
                    self.refresh(root);
                    Handled::Yes
                }
                MenuAction::Left => {
                    self.change(-1, root);
                    Handled::Yes
                }
                MenuAction::Right => {
                    self.change(1, root);
                    Handled::Yes
                }
                MenuAction::Select => {
                    let next = (self.section + 1) % self.data.sections.len().max(1);
                    self.set_section(next, root);
                    Handled::Yes
                }
                MenuAction::Back => {
                    super::leave(&self.env, cx, true);
                    Handled::Yes
                }
                MenuAction::Start if cx.focused().is_none_or(|f| f == "Rows") => {
                    self.change(1, root);
                    Handled::Yes
                }
                _ => Handled::No,
            },
            Input::Pointer {
                target: Some(t),
                gesture: crate::Gesture::Tap,
                ..
            } => {
                // A choice, a stepper's arrow, or a row.
                let mut handled = Handled::No;
                for name in &t.chain {
                    if let Some(rest) = name.strip_prefix("Option") {
                        let (i, what) = rest.split_once('.').unwrap_or((rest, ""));
                        let Ok(i) = i.parse::<usize>() else { continue };
                        if i != self.focus {
                            self.focus = i.min(n.saturating_sub(1));
                            self.refresh(root);
                        }
                        if let Some(j) = what
                            .strip_prefix("Choice")
                            .and_then(|j| j.parse::<i32>().ok())
                        {
                            let at = self.options().get(i).map_or(0, |o| o.index as i32);
                            self.change(j - at, root);
                        } else if what == "Down" {
                            self.change(-1, root);
                        } else if what == "Up" {
                            self.change(1, root);
                        }
                        handled = Handled::Yes;
                        break;
                    }
                }
                handled
            }
            Input::Activate { name } => {
                if let Some(i) = name.strip_prefix("Section").and_then(|n| n.parse().ok()) {
                    self.set_section(i, root);
                } else if name == "Link.Back" {
                    super::leave(&self.env, cx, true);
                }
                Handled::Yes
            }
            _ => Handled::No,
        }
    }
}
