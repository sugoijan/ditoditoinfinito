//! The first start's offer on desktop: song folders of other dance games
//! found on this computer (StepMania 5, ITGmania, OutFox), read where they
//! are if the player says so (`docs/plans/desktop.md`, D7). "Add them" or
//! "No thanks"; either way the answer is kept by the shell and the dialog
//! never comes back.

use std::rc::Rc;

use ddi_actor::Actor;
use ddi_l10n::UiText;
use ddi_l10n::msg::pages as pm;

use super::Env;
use super::dialog::{button_rows, dialog_body, dialog_root, shell, wrap_lines};
use crate::nav::{Focusable, MenuAction};
use crate::screen::{Cx, Handled, Input, Screen};

/// Told the answer: `true` to add the folders.
pub type AnswerFn = Rc<dyn Fn(bool, &mut Cx)>;

/// The folders found, the question, the two answers.
pub struct FoldersOfferDialog {
    env: Env,
    folders: Vec<String>,
    on_answer: AnswerFn,
}

/// The folders listed at most (the rest summed up).
const SHOWN: usize = 5;

impl FoldersOfferDialog {
    pub fn new(env: Env, folders: Vec<String>, on_answer: AnswerFn) -> FoldersOfferDialog {
        FoldersOfferDialog {
            env,
            folders,
            on_answer,
        }
    }

    fn lines(&self, w: f32) -> Vec<(String, &'static str)> {
        let mut lines: Vec<(String, &'static str)> = wrap_lines(&pm::offer_found(), w, 14.0)
            .into_iter()
            .map(|l| (l, "Text"))
            .collect();
        lines.push((String::new(), "Muted"));
        for f in self.folders.iter().take(SHOWN) {
            lines.push((UiText::data(f).into(), "Muted"));
        }
        if self.folders.len() > SHOWN {
            lines.push((pm::offer_more(self.folders.len() - SHOWN).into(), "Muted"));
        }
        lines.push((String::new(), "Muted"));
        lines.extend(
            wrap_lines(&pm::offer_where(), w, 14.0)
                .into_iter()
                .map(|l| (l, "Muted")),
        );
        lines
    }
}

impl Screen for FoldersOfferDialog {
    fn presentation(&self) -> Option<crate::look::Presentation<'_>> {
        Some(self.env.presentation())
    }

    fn name(&self) -> &str {
        "folders-offer"
    }

    fn build(&mut self, cx: &mut Cx) -> Actor {
        let vp = cx.viewport().screen;
        let screen = [vp.width, vp.height];
        let env = self.env.clone();
        let w = 600.0_f32.min(screen[0] - 32.0);
        let lines = self.lines(w - 48.0);
        let h = (140.0 + lines.len() as f32 * 18.0).min(screen[1] - 32.0);
        let (mut c, [x, y]) = shell(&env, screen, [w, h], &pm::offer_title());
        let texts: Vec<&str> = lines.iter().map(|(line, _)| line.as_str()).collect();
        dialog_body(&env, &mut c, &texts, [x, y], w, |i| lines[i].1);
        let buttons = [
            ("Decline", pm::offer_decline().into(), "Button"),
            (
                "Add",
                pm::offer_add(self.folders.len()).into(),
                "PlayButton",
            ),
        ];
        c.extend(button_rows(&env, &buttons, 2, [x, y, w, h], 180.0));
        dialog_root(c)
    }

    fn focusables(&self) -> Vec<Focusable> {
        vec![Focusable::new("Add"), Focusable::new("Decline")]
    }

    fn input(&mut self, input: &Input, _root: &mut Actor, cx: &mut Cx) -> Handled {
        match input {
            Input::Activate { name } if name == "Add" => {
                (self.on_answer)(true, cx);
                cx.close(false);
                Handled::Yes
            }
            Input::Activate { name } if name == "Decline" => {
                (self.on_answer)(false, cx);
                cx.close(true);
                Handled::Yes
            }
            // Backing out is an answer too: the offer is not made again.
            Input::Outside
            | Input::Menu {
                action: MenuAction::Back,
                ..
            } => {
                (self.on_answer)(false, cx);
                cx.close(true);
                Handled::Yes
            }
            _ => Handled::No,
        }
    }
}
