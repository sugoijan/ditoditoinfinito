//! The browser history entries a shell keeps for the UI's layers, so the
//! browser's Back closes the top one (the web's host,
//! `crates/web/src/components/ui_dev.rs`): one entry for each screen or modal
//! dialog over the first, pushed as they open; those of layers closed
//! otherwise are gone back over quietly, in one `history.go(-n)`, which
//! fires one `popstate` whatever `n` is: one pop to ignore.

use crate::ui::LayerInfo;

/// The entries the layers want: one for each screen or modal dialog
/// shown over the first (a popup has none; one leaving has had its
/// Back).
pub fn depth(layers: &[LayerInfo]) -> usize {
    layers
        .iter()
        .filter(|l| (l.modal || !l.overlay) && l.phase != "out")
        .count()
        .saturating_sub(1)
}

/// What to do to the history to go from `from` entries to `to`.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Step {
    /// Entries to push, by the depth each marks.
    pub push: Vec<usize>,
    /// Entries to go back over at once (`history.go(-n)`).
    pub back: Option<usize>,
    /// `popstate` events of ours that will come and are not the
    /// player's Back: one for a `go`, however far.
    pub ignore_pops: u32,
}

pub fn step(from: usize, to: usize) -> Step {
    if to > from {
        Step {
            push: (from + 1..=to).collect(),
            ..Step::default()
        }
    } else if to < from {
        Step {
            back: Some(from - to),
            ignore_pops: 1,
            ..Step::default()
        }
    } else {
        Step::default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::TextMode;
    use crate::message::Message;
    use crate::screen::{Cx, Handled, Input, Screen};
    use crate::testing::Harness;
    use ddi_actor::{Actor, Kind};

    /// `under` opens a modal dialog over itself when built, and leaves
    /// when told; the dialog, on `x`, leaves and tells it: two layers
    /// closed in one call.
    struct Layer {
        name: &'static str,
        opened: bool,
    }

    impl Screen for Layer {
        fn name(&self) -> &str {
            self.name
        }
        fn build(&mut self, cx: &mut Cx) -> Actor {
            if self.name == "under" && !std::mem::replace(&mut self.opened, true) {
                cx.overlay(
                    Layer {
                        name: "dialog",
                        opened: false,
                    },
                    true,
                );
            }
            Actor::new(Kind::Frame(Vec::new()))
        }
        fn message(&mut self, m: &Message, _root: &mut Actor, cx: &mut Cx) {
            if m.name == "CloseUnder" && self.name == "under" {
                cx.close(false);
            }
        }
        fn input(&mut self, input: &Input, _root: &mut Actor, cx: &mut Cx) -> Handled {
            match input {
                Input::Key(k) if k.down && self.name == "dialog" && k.key == "x" => {
                    cx.close(false);
                    cx.broadcast(Message::new("CloseUnder"));
                    Handled::Yes
                }
                _ => Handled::No,
            }
        }
    }

    #[test]
    fn two_layers_closed_at_once_are_one_go_and_one_pop() {
        let mut h = Harness::new(
            crate::screens::DESIGN,
            [1280.0, 720.0],
            1.0,
            TextMode::Mirror,
        );
        h.push(Layer {
            name: "first",
            opened: false,
        });
        h.run(0.3);
        assert_eq!(depth(&h.ui.layers()), 0);
        h.push(Layer {
            name: "under",
            opened: false,
        });
        h.run(0.5);
        // The screen and its dialog: two entries, pushed one by one.
        let two = depth(&h.ui.layers());
        assert_eq!(two, 2, "{:?}", h.ui.layers());
        assert_eq!(step(0, two).push, vec![1, 2]);
        // One key: the dialog and the screen under it both leave.
        h.key("KeyX", "x");
        let gone = depth(&h.ui.layers());
        assert_eq!(gone, 0, "{:?}", h.ui.layers());
        let s = step(two, gone);
        assert_eq!(s.back, Some(2));
        assert_eq!(s.ignore_pops, 1, "one go(-2), one popstate");
        assert_eq!(step(3, 3), Step::default());
    }
}
