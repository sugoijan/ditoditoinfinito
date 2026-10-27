//! Controller-editable numeric forms. The caller's validator remains authoritative.
use std::rc::Rc;

use ddi_actor::Actor;

use super::editor_dialogs::{SubmitFn, ValueDialog};
use super::select_dialogs::{button, fitted_dialog, shell, wrap};
use super::{Env, left};
use crate::nav::{Focusable, MenuAction};
use crate::screen::{Cx, Handled, Input, Screen};

#[derive(Clone)]
pub struct Number {
    pub label: &'static str,
    pub value: f64,
    pub step: f64,
    pub min: f64,
    pub max: f64,
}

impl Number {
    pub fn new(label: &'static str, value: f64, step: f64, min: f64, max: f64) -> Self {
        Self {
            label,
            value,
            step,
            min,
            max,
        }
    }
}

#[derive(Clone, Copy)]
pub enum Format {
    Scalar,
    Pair(&'static str),
    Speed,
    DisplayBpm,
}

pub struct Numbers {
    pub fields: Vec<Number>,
    pub format: Format,
    pub choice: usize,
    pub remove: bool,
}

impl Numbers {
    pub fn scalar(value: f64, step: f64, min: f64, max: f64) -> Self {
        Self {
            fields: vec![Number::new("Value", value, step, min, max)],
            format: Format::Scalar,
            choice: 0,
            remove: false,
        }
    }

    fn choices(&self) -> &'static [&'static str] {
        match self.format {
            Format::Speed => &["Beats", "Seconds"],
            Format::DisplayBpm => &["Automatic", "Random", "Single BPM", "BPM range"],
            _ => &[],
        }
    }

    fn text(&self) -> String {
        let v: Vec<_> = self.fields.iter().map(|n| n.value.to_string()).collect();
        match self.format {
            Format::Scalar => v[0].clone(),
            Format::Pair(sep) => v.join(sep),
            Format::Speed => format!(
                "{} {} {}",
                v[0],
                v[1],
                if self.choice == 0 { "beats" } else { "s" }
            ),
            Format::DisplayBpm => match self.choice {
                0 => String::new(),
                1 => "*".into(),
                2 => v[0].clone(),
                _ => v.join("-"),
            },
        }
    }
}

pub struct NumberDialog {
    env: Env,
    title: String,
    numbers: Numbers,
    initial: Vec<f64>,
    initial_choice: usize,
    coarse: bool,
    error: String,
    submit: SubmitFn,
}

impl NumberDialog {
    pub fn new(env: Env, title: &str, numbers: Numbers, submit: SubmitFn) -> Self {
        let initial = numbers.fields.iter().map(|n| n.value).collect();
        let initial_choice = numbers.choice;
        Self {
            env,
            title: title.into(),
            numbers,
            initial,
            initial_choice,
            coarse: false,
            error: String::new(),
            submit,
        }
    }

    fn apply(&mut self, value: &str, root: &mut Actor, cx: &mut Cx) {
        match (self.submit)(value, cx) {
            Ok(()) => cx.close(false),
            Err(error) => {
                self.error = error;
                *root = self.build(cx);
            }
        }
    }

    fn change(&mut self, name: &str, by: f64, root: &mut Actor, cx: &mut Cx) {
        if let Some(i) = name
            .strip_prefix("Number")
            .and_then(|n| n.parse::<usize>().ok())
        {
            if let Some(n) = self.numbers.fields.get_mut(i) {
                let value = (n.value + by * n.step * if self.coarse { 10.0 } else { 1.0 })
                    .clamp(n.min, n.max);
                // Avoid accumulating binary rounding noise in decimal increments.
                n.value = (value * 1e9).round() / 1e9;
            }
        } else if name == "Units" {
            let count = self.numbers.choices().len();
            self.numbers.choice =
                (self.numbers.choice + if by < 0.0 { count - 1 } else { 1 }) % count;
        } else {
            return;
        }
        self.error.clear();
        *root = self.build(cx);
    }
}

impl Screen for NumberDialog {
    fn name(&self) -> &str {
        "number-value"
    }

    fn build(&mut self, cx: &mut Cx) -> Actor {
        let vp = cx.viewport().screen;
        let (screen, scale) = super::narrow_fit(&self.env, vp);
        let w = 480.0_f32.min(screen[0] - 24.0);
        let n = self.numbers.fields.len() + usize::from(!self.numbers.choices().is_empty());
        let h = 224.0 + n as f32 * 40.0;
        let (mut c, [x, y]) = shell(&self.env, screen, [w, h], &self.title);
        for (i, number) in self.numbers.fields.iter().enumerate() {
            let label = format!("‹  {}: {}  ›", number.label, number.value);
            c.push(button(
                &self.env,
                &format!("Number{i}"),
                &label,
                [x + w / 2.0, y + 70.0 + i as f32 * 40.0],
                w - 48.0,
                "Button",
            ));
        }
        if !self.numbers.choices().is_empty() {
            c.push(button(
                &self.env,
                "Units",
                self.numbers.choices()[self.numbers.choice],
                [x + w / 2.0, y + 70.0 + (n - 1) as f32 * 40.0],
                w - 48.0,
                "Button",
            ));
        }
        let actions = self.actions();
        let bw = (w - 58.0) / 2.0;
        for (i, (id, label)) in actions.iter().enumerate() {
            c.push(button(
                &self.env,
                id,
                label,
                [
                    x + 24.0 + bw / 2.0 + (i % 2) as f32 * (bw + 10.0),
                    y + 76.0 + n as f32 * 40.0 + (i / 2) as f32 * 40.0,
                ],
                bw,
                "Button",
            ));
        }
        let mut error = self.env.text("Error", &self.error, 12.0, 400, "Accent");
        wrap(&mut error, w - 48.0);
        c.push(left(error, x + 24.0, y + h - 24.0));
        let mut root = fitted_dialog(c, screen, h, scale);
        super::restore_focus(&mut root, cx.focused());
        root
    }

    fn focusables(&self) -> Vec<Focusable> {
        (0..self.numbers.fields.len())
            .map(|i| Focusable::new(format!("Number{i}")))
            .chain((!self.numbers.choices().is_empty()).then(|| Focusable::new("Units")))
            .chain(self.actions().into_iter().map(|(id, _)| Focusable::new(id)))
            .collect()
    }

    fn message(&mut self, m: &crate::Message, _: &mut Actor, cx: &mut Cx) {
        if m.name == "Numbers.Accepted" {
            cx.close(false);
        }
    }

    fn input(&mut self, input: &Input, root: &mut Actor, cx: &mut Cx) -> Handled {
        match input {
            Input::Menu {
                action: action @ (MenuAction::Left | MenuAction::Right),
                ..
            } if cx
                .focused()
                .is_some_and(|f| f.starts_with("Number") || f == "Units") =>
            {
                let name = cx.focused().unwrap().to_string();
                self.change(
                    &name,
                    if *action == MenuAction::Left {
                        -1.0
                    } else {
                        1.0
                    },
                    root,
                    cx,
                );
            }
            Input::Activate { name } => match name.as_str() {
                "Ok" => self.apply(&self.numbers.text(), root, cx),
                "Cancel" => cx.close(true),
                "Remove" => self.apply("", root, cx),
                "Reset" => {
                    for (n, value) in self.numbers.fields.iter_mut().zip(&self.initial) {
                        n.value = *value;
                    }
                    self.numbers.choice = self.initial_choice;
                    self.error.clear();
                    *root = self.build(cx);
                }
                "Step" => {
                    self.coarse = !self.coarse;
                    *root = self.build(cx);
                }
                "Type" => {
                    let submit = self.submit.clone();
                    cx.overlay(
                        ValueDialog::new(
                            self.env.clone(),
                            &self.title,
                            "",
                            &self.numbers.text(),
                            Rc::new(move |s, cx| {
                                submit(s, cx)?;
                                cx.broadcast(crate::Message::new("Numbers.Accepted"));
                                Ok(())
                            }),
                        ),
                        true,
                    );
                }
                name => self.change(name, 1.0, root, cx),
            },
            _ => return Handled::No,
        }
        Handled::Yes
    }
}

impl NumberDialog {
    fn actions(&self) -> Vec<(&'static str, &'static str)> {
        let mut actions = vec![
            (
                "Step",
                if self.coarse {
                    "Step: coarse ×10"
                } else {
                    "Step: fine"
                },
            ),
            ("Type", "Type value…"),
            ("Reset", "Reset changes"),
        ];
        if self.numbers.remove {
            actions.push(("Remove", "Remove"));
        }
        actions.extend([("Ok", "Apply"), ("Cancel", "Back")]);
        actions
    }
}
