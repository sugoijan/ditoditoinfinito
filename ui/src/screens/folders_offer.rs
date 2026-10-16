//! The first start's offer on desktop: song folders of other dance games
//! found on this computer (StepMania 5, ITGmania, OutFox), read where they
//! are if the player says so (`docs/plans/desktop.md`, D7). "Add them" or
//! "No thanks"; either way the answer is kept by the shell and the dialog
//! never comes back.

use std::rc::Rc;

use ddi_actor::Actor;

use super::select_dialogs::{button, dialog_root, shell, wrap, wrap_lines};
use super::{Env, left};
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
        let mut lines: Vec<(String, &'static str)> = wrap_lines(
            "Song folders of other dance games are on this computer. The game can read them \
             where they are: nothing is copied or changed.",
            w,
            14.0,
        )
        .into_iter()
        .map(|l| (l, "Text"))
        .collect();
        lines.push((String::new(), "Muted"));
        for f in self.folders.iter().take(SHOWN) {
            lines.push((f.clone(), "Muted"));
        }
        if self.folders.len() > SHOWN {
            lines.push((format!("and {} more", self.folders.len() - SHOWN), "Muted"));
        }
        lines.push((String::new(), "Muted"));
        lines.extend(
            wrap_lines(
                "Options lists the song folders: add or remove them there at any time.",
                w,
                14.0,
            )
            .into_iter()
            .map(|l| (l, "Muted")),
        );
        lines
    }
}

impl Screen for FoldersOfferDialog {
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
        let (mut c, [x, y]) = shell(&env, screen, [w, h], "Add the songs you have?");
        for (i, (line, colour)) in lines.iter().enumerate() {
            let mut b = env.text(&format!("Body{i}"), line, 14.0, 400, colour);
            wrap(&mut b, w - 48.0);
            c.push(left(b, x + 24.0, y + 66.0 + i as f32 * 18.0));
        }
        let bw = ((w - 58.0) / 2.0).min(180.0);
        let by = y + h - 44.0;
        c.push(button(
            &env,
            "Decline",
            "No thanks",
            [x + w - 24.0 - bw * 1.5 - 10.0, by],
            bw,
            "Button",
        ));
        c.push(button(
            &env,
            "Add",
            if self.folders.len() == 1 {
                "Add it"
            } else {
                "Add them"
            },
            [x + w - 24.0 - bw / 2.0, by],
            bw,
            "PlayButton",
        ));
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
