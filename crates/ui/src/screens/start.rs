//! The title menu (`ScreenTitleMenu` in the theme compatibility model).
//! Choice IDs and capabilities belong to the host; actors belong to the look.
use std::rc::Rc;

use super::{Env, ground};
use crate::look::{Spec, call};
use crate::nav::{Focusable, MenuAction};
use crate::screen::{Cx, Handled, Input, Screen};
use ddi_actor::{Actor, Kind};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Choice {
    Singleplayer,
    Options,
    Credits,
    Quit,
}

impl Choice {
    pub fn id(self) -> &'static str {
        match self {
            Self::Singleplayer => "Singleplayer",
            Self::Options => "Options",
            Self::Credits => "Credits",
            Self::Quit => "Quit",
        }
    }
}

pub type Action = Rc<dyn Fn(Choice, &mut Cx)>;

pub struct StartScreen {
    env: Env,
    desktop: bool,
    action: Action,
    size: [f32; 2],
    zoom: f32,
    initial_focus: Option<String>,
}

impl StartScreen {
    pub fn new(env: Env, desktop: bool, action: Action) -> Self {
        Self {
            env,
            desktop,
            action,
            size: [0.0; 2],
            zoom: 1.0,
            initial_focus: None,
        }
    }

    pub fn with_focus(mut self, focus: Option<String>) -> Self {
        self.initial_focus = focus;
        self
    }

    fn choices(&self) -> Vec<Choice> {
        let mut choices = vec![Choice::Singleplayer, Choice::Options, Choice::Credits];
        if self.desktop {
            choices.push(Choice::Quit);
        }
        choices
    }

    fn tree(&mut self, cx: &Cx) -> Actor {
        let vp = cx.viewport().screen;
        let (size, zoom) = super::narrow_fit(&self.env, vp);
        self.size = size;
        self.zoom = zoom;
        let [w, h] = size;
        let env = &self.env;
        let width = env.metric("TitleMenu.Width", 380.0).min(w - 32.0);
        let css_scale = cx.viewport().scale / cx.viewport().ratio * zoom;
        let row = env
            .metric("TitleMenu.RowHeight", 52.0)
            .max(44.0 / css_scale);
        let gap = env.metric("TitleMenu.Gap", 12.0);
        let count = if self.desktop { 5.0 } else { 4.0 };
        let total = count * (row + gap) + 70.0;
        let top = ((h - total) / 2.0).max(8.0);
        let mut children = vec![ground(env, size)];
        let mut title = env.text(
            "Title",
            "Dito Dito Infinito",
            env.metric("TitleMenu.TitleSize", 30.0),
            700,
            "Text",
        );
        call(&mut title, "xy", &[w / 2.0, top + 24.0]);
        children.push(title);
        let mut entries = vec![
            ("Singleplayer", "Singleplayer", "TitleMenu.Primary"),
            (
                "Multiplayer",
                "Multiplayer · Coming soon",
                "TitleMenu.Disabled",
            ),
            ("Options", "Options", "TitleMenu.Choice"),
            ("Credits", "Credits", "TitleMenu.Choice"),
        ];
        if self.desktop {
            entries.push(("Quit", "Quit", "TitleMenu.Choice"));
        }
        for (i, (id, label, component)) in entries.iter().enumerate() {
            children.push(env.component(
                component,
                "Button",
                &Spec::new([width, row], *label),
                id,
                [w / 2.0, top + 70.0 + row / 2.0 + i as f32 * (row + gap)],
            ));
        }
        let mut root = Actor::new(Kind::Frame(children));
        call(&mut root, "zoom", &[zoom]);
        super::entrance(&mut root, env);
        self.publish();
        root
    }

    fn publish(&self) {
        let mut st = self.env.state.borrow_mut();
        st.screen = "start".into();
        st.compact = self.size[0] < super::DESIGN[0] || self.size[1] < 480.0;
        st.backdrop = None;
        st.pulsing = false;
        st.pulse = 0.0;
        st.hints = vec!["↑↓ Move".into(), "ENTER Choose".into()];
        st.page_items = self.focusables().into_iter().map(|f| f.name).collect();
    }
}

impl Screen for StartScreen {
    fn name(&self) -> &str {
        "start"
    }
    fn build(&mut self, cx: &mut Cx) -> Actor {
        let root = self.tree(cx);
        if let Some(f) = self
            .initial_focus
            .take()
            .filter(|f| self.focusables().iter().any(|c| &c.name == f))
        {
            cx.focus(f);
        }
        root
    }
    fn focusables(&self) -> Vec<Focusable> {
        let mut focus: Vec<_> = self
            .choices()
            .iter()
            .map(|c| Focusable::new(c.id()))
            .collect();
        // Coming soon is selectable, but deliberately absent from the actions.
        focus.insert(1, Focusable::new("Multiplayer"));
        focus
    }
    fn update(&mut self, _dt: f32, root: &mut Actor, cx: &mut Cx) {
        if !cx.is_current_screen() {
            return;
        }
        let (size, zoom) = super::narrow_fit(&self.env, cx.viewport().screen);
        if self.size != size || self.zoom != zoom {
            *root = self.tree(cx);
            if let Some(actor) = cx
                .focused()
                .and_then(|name| crate::hit::find_mut(root, name))
            {
                crate::widgets::play(actor, "GainFocus");
            }
            root.finish_tweening();
        }
        self.publish();
    }
    fn input(&mut self, input: &Input, _root: &mut Actor, cx: &mut Cx) -> Handled {
        match input {
            Input::Menu {
                action: MenuAction::Back,
                ..
            } => Handled::Yes,
            Input::Activate { name } => {
                if let Some(choice) = self.choices().into_iter().find(|c| c.id() == name) {
                    (self.action)(choice, cx);
                }
                Handled::Yes
            }
            _ => Handled::No,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::look::Plain;
    use crate::testing::Harness;
    use crate::{PointerKind, TextMode, UiRequest};
    use std::cell::RefCell;

    fn env() -> Env {
        Env::new(
            Rc::new(Plain),
            Rc::new(|s| ddi_library::search::Fold::nfkd(&ddi_library::search::Nfkd, s)),
        )
    }

    #[test]
    fn title_choices_focus_but_do_not_activate_multiplayer_and_put_quit_last() {
        for desktop in [false, true] {
            let calls = Rc::new(RefCell::new(Vec::new()));
            let sink = calls.clone();
            let mut h = Harness::new(super::super::DESIGN, [1280.0, 720.0], 1.0, TextMode::Own);
            h.push(StartScreen::new(
                env(),
                desktop,
                Rc::new(move |choice, _| sink.borrow_mut().push(choice)),
            ));
            h.run(0.3);
            assert_eq!(h.ui.focused(), Some("Singleplayer"));
            for key in ["Escape", "Backspace"] {
                h.key(key, key);
            }
            assert_eq!(h.ui.layers().len(), 1);
            h.key("ArrowDown", "ArrowDown");
            assert_eq!(h.ui.focused(), Some("Multiplayer"));
            h.key("Enter", "Enter");
            h.pad(MenuAction::Start, true);
            h.pad(MenuAction::Start, false);
            h.tap(PointerKind::Mouse, "Multiplayer");
            h.tap(PointerKind::Touch, "Multiplayer");
            assert!(calls.borrow().is_empty());
            h.key("ArrowDown", "ArrowDown");
            assert_eq!(h.ui.focused(), Some("Options"));
            h.key("Enter", "Enter");
            assert_eq!(*calls.borrow(), [Choice::Options]);
            assert_eq!(h.texts().iter().any(|s| s == "Quit"), desktop);
            h.pad(MenuAction::Down, true);
            h.pad(MenuAction::Down, false);
            assert_eq!(h.ui.focused(), Some("Credits"));
            if desktop {
                h.pad(MenuAction::Down, true);
                h.pad(MenuAction::Down, false);
                assert_eq!(h.ui.focused(), Some("Quit"));
                assert!(h.centre("Quit")[1] > h.centre("Credits")[1]);
            }
        }
    }

    #[test]
    fn title_choices_fit_and_keep_touch_targets_after_resizing() {
        for desktop in [false, true] {
            let mut h = Harness::new(super::super::DESIGN, [1280.0, 720.0], 1.0, TextMode::Own);
            h.push(StartScreen::new(env(), desktop, Rc::new(|_, _| {})));
            h.run(0.3);
            h.pad(MenuAction::Down, true);
            h.pad(MenuAction::Down, false);
            assert_eq!(h.ui.focused(), Some("Multiplayer"));
            for size in [
                [1280.0, 720.0],
                [390.0, 844.0],
                [844.0, 390.0],
                [320.0, 568.0],
            ] {
                h.send(crate::UiEvent::Resize { size, ratio: 1.0 });
                h.run(0.3);
                assert_eq!(h.ui.focused(), Some("Multiplayer"));
                for name in ["Singleplayer", "Multiplayer", "Options", "Credits", "Quit"] {
                    if name == "Quit" && !desktop {
                        continue;
                    }
                    let [x0, y0, x1, y1] = h.ui.bounds(name).unwrap();
                    let [x0, y0] = h.css([x0, y0]);
                    let [x1, y1] = h.css([x1, y1]);
                    assert!(
                        x0 >= 0.0 && y0 >= 0.0 && x1 <= size[0] && y1 <= size[1],
                        "{name} {size:?}"
                    );
                    assert!(y1 - y0 >= 43.99, "{name} height {}", y1 - y0);
                }
            }
        }
    }

    #[test]
    fn singleplayer_does_not_pass_start_through_and_back_returns_once() {
        let env = env();
        let mut h = Harness::new(super::super::DESIGN, [1280.0, 720.0], 1.0, TextMode::Own);
        h.push(super::super::sample::start_screen(&env, true));
        h.run(0.3);
        h.key_down("Enter", "Enter");
        h.run(0.5);
        assert_eq!(h.ui.layers().last().unwrap().name, "select");
        h.key_up("Enter", "Enter");
        let song = env.state.borrow().song.clone();
        h.key_down("Backspace", "Backspace");
        h.run(0.5);
        assert_eq!(h.ui.layers().last().unwrap().name, "start");
        assert_eq!(env.state.borrow().song, song);
        assert!(!h.requests.contains(&UiRequest::Quit));
        h.key_up("Backspace", "Backspace");
        assert_eq!(h.ui.focused(), Some("Singleplayer"));
    }

    #[test]
    fn options_returns_to_title_and_restores_focus() {
        let mut h = Harness::new(super::super::DESIGN, [1280.0, 720.0], 1.0, TextMode::Own);
        h.push(super::super::sample::start_screen(&env(), true));
        h.run(0.3);
        h.key("ArrowDown", "ArrowDown");
        h.key("ArrowDown", "ArrowDown");
        h.key("Enter", "Enter");
        h.run(0.3);
        assert_eq!(h.ui.layers().last().unwrap().name, "options");
        h.key("Escape", "Escape");
        h.run(0.3);
        assert_eq!(h.ui.layers().last().unwrap().name, "start");
        assert_eq!(h.ui.focused(), Some("Options"));
    }
}
