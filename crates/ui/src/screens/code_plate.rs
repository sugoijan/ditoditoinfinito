//! Desktop's sign-in code on a plate of its own (round SF; one component
//! since round DC): the attempt's code large in the theme's face, grouped
//! as the browser's page asks for it, on a plate the width of its place;
//! at its right end, small, Copy, or Copied for [`COPIED_FOR`] seconds
//! once it was pressed (a click, a tap, Enter, a pad's confirm: the screen
//! that shows it asks its shell to copy the code, `UiRequest::CopyText`).
//! The sign-in dialog, the Twitch access trip's dialog
//! ([`super::online_home::SignInDialog`]) and an invite's page
//! ([`super::online`], through the page's [`super::page::Block::Code`])
//! draw it alike.

use ddi_actor::Actor;
use ddi_l10n::UiText;
use ddi_l10n::msg::room as m;

use super::Env;
use crate::look::{self, Spec, call};

/// How long the plate says the code was copied, in seconds.
pub const COPIED_FOR: f64 = 2.0;
/// The plate's height.
pub const CODE_H: f32 = 64.0;

/// The plate's actors: the plate (`name`, what is pressed), the code
/// (`Code.Text`) and its small words (`Code.Hint`), both decorations of
/// it; centered at `[cx, cy]`, `w` wide. `code` as shown (grouped).
pub fn plate(env: &Env, name: &str, code: &str, copied: bool, [cx, cy]: [f32; 2], w: f32) -> Vec<Actor> {
    let plate = env.component("Button", "Button", &Spec::new([w, CODE_H], ""), name, [cx, cy]);
    let shown: String = UiText::data(code).into();
    let mut text = look::tracked(env.text("Code.Text", &shown, 34.0, 700, "Text"), 160);
    call(&mut text, "xy", &[cx, cy]);
    text.decoration = true;
    let hint: String = words(copied).into();
    let mut hint = super::right(
        env.text("Code.Hint", &hint, 12.0, 600, "Muted"),
        cx + w / 2.0 - 14.0,
        cy,
    );
    hint.decoration = true;
    vec![plate, text, hint]
}

/// The plate's small words: Copy, or Copied a moment after a press.
pub fn words(copied: bool) -> UiText {
    if copied {
        m::sign_in_code_copied()
    } else {
        m::sign_in_code_copy()
    }
}

/// When the code was last copied (a screen's clock), so that its plate
/// says Copied for [`COPIED_FOR`] seconds.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Copied(Option<f64>);

impl Copied {
    /// Whether the plate says Copied now.
    pub fn shown(&self) -> bool {
        self.0.is_some()
    }

    /// Pressed at `now`: its moment begun (again); whether the words
    /// changed (they had said Copy).
    pub fn press(&mut self, now: f64) -> bool {
        self.0.replace(now).is_none()
    }

    /// Its moment over at `now`: Copy again; whether the words changed.
    pub fn expire(&mut self, now: f64) -> bool {
        if self.0.is_some_and(|t| now - t >= COPIED_FOR) {
            self.0 = None;
            return true;
        }
        false
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn copied_lasts_its_moment_and_a_press_begins_it_again() {
        let mut c = Copied::default();
        assert!(!c.shown());
        assert!(c.press(1.0));
        assert!(!c.press(2.5), "already said");
        assert!(!c.expire(4.4));
        assert!(c.shown());
        assert!(c.expire(4.5));
        assert!(!c.shown() && !c.expire(9.0));
    }
}
