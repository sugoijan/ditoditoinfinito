//! The browser history entries a shell keeps for the UI's layers, so the
//! browser's Back closes the top one (the web's host,
//! `crates/web/src/components/ui_dev.rs`): one entry for each screen or modal
//! dialog over the first, pushed as they open; those of layers closed
//! otherwise are gone back over quietly, in one `history.go(-n)`, which
//! fires one `popstate` whatever `n` is: one pop to ignore.
//!
//! One thing at a time ([`Entries`]): a `go` is carried out later, and
//! until its `popstate` has come nothing else may touch the history. An
//! entry pushed meanwhile (a dialog closing as the next one opens: the
//! room's More menu's Stop match and the standings it brings up) makes
//! Firefox drop the `go` with no `popstate` at all, and Chrome drop the
//! entry; either way the count of pops to ignore and the entries no longer
//! agree with the history, and a navigation that waits for the pop never
//! runs (the screen's parts leave and nothing follows: the mostly black
//! room of 2026-11-12).

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

/// How long a `go` of ours may take, seconds: past it its `popstate` is
/// taken for lost ([`Entries::overdue`]), so that nothing waits on it
/// forever. Browsers carry a `go` out within a few milliseconds.
pub const POP_WAIT: f64 = 1.0;

/// What a `popstate` on the shell's route was ([`Entries::popped`]).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Pop {
    /// The one a `go` of ours brought: nothing for the screens.
    Ours,
    /// The browser's Back out of a layer's entry: the menu's Back.
    Back,
    /// The browser's Forward into the entry of a layer closed since: go
    /// back over this many entries again, quietly ([`Entries::went`]).
    Forward(usize),
    /// Neither (the same depth): nothing to do.
    Nothing,
}

/// The entries a shell has in the history for its layers, and the pops of
/// its own that are on their way. The shell asks [`Entries::sync`] what to
/// do each time the layers may have changed, tells it of each `popstate`
/// ([`Entries::popped`]), and navigates only while not [`Entries::busy`].
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Entries {
    /// Entries pushed for the layers over the first, as the history has
    /// them once the pops on their way have come.
    depth: usize,
    /// `popstate`s of ours that will come.
    pending: u32,
    /// When the oldest of them was asked for (the shell's clock, seconds).
    since: f64,
}

impl Entries {
    pub fn depth(&self) -> usize {
        self.depth
    }

    /// A `go` of ours is on its way: the history is not to be pushed to,
    /// gone through or navigated until its `popstate` has come.
    pub fn busy(&self) -> bool {
        self.pending > 0
    }

    /// What to do to the history at `now` so that it has `wanted` entries
    /// for the layers: nothing while a `go` of ours is on its way (asked
    /// again once its pop has come).
    pub fn sync(&mut self, wanted: usize, now: f64) -> Step {
        if self.busy() {
            return Step::default();
        }
        let step = step(self.depth, wanted);
        if step.ignore_pops > 0 {
            self.pending += step.ignore_pops;
            self.since = now;
        }
        self.depth = wanted;
        step
    }

    /// A `go` of the shell's own beside [`Entries::sync`]'s (stepping off
    /// a closed layer's entry): one more pop to ignore.
    pub fn went(&mut self, now: f64) {
        if self.pending == 0 {
            self.since = now;
        }
        self.pending += 1;
    }

    /// A `popstate` on the shell's route, reaching an entry marked for
    /// `reached` layers (0: the route's own).
    pub fn popped(&mut self, reached: usize) -> Pop {
        if self.pending > 0 {
            self.pending -= 1;
            return Pop::Ours;
        }
        match reached.cmp(&self.depth) {
            std::cmp::Ordering::Greater => Pop::Forward(reached - self.depth),
            std::cmp::Ordering::Equal => Pop::Nothing,
            std::cmp::Ordering::Less => {
                self.depth = reached;
                Pop::Back
            }
        }
    }

    /// A pop of ours has not come for [`POP_WAIT`]: it never will.
    pub fn overdue(&self, now: f64) -> bool {
        self.busy() && now - self.since > POP_WAIT
    }

    /// The pops on their way are lost: the history is where it is, on an
    /// entry marked for `reached` layers.
    pub fn lost(&mut self, reached: usize) {
        self.pending = 0;
        self.depth = reached;
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
    /// A browser's session history as the shells meet it: entries by
    /// their layer mark (0: a route's own), a `go` carried out only when
    /// its task runs ([`Browser::deliver`]), and what each browser does
    /// to an entry pushed before then (measured in headless Firefox and
    /// Chrome, 2026-11-12).
    #[derive(Clone, Copy, PartialEq, Debug)]
    enum Engine {
        /// The push drops the `go`: no `popstate` ever.
        Firefox,
        /// The `go` wins: the pushed entry is gone.
        Chrome,
    }

    struct Browser {
        kind: Engine,
        entries: Vec<usize>,
        current: usize,
        going: Option<usize>,
    }

    impl Browser {
        fn new(kind: Engine) -> Browser {
            Browser {
                kind,
                entries: vec![0],
                current: 0,
                going: None,
            }
        }

        fn push(&mut self, mark: usize) {
            if self.going.is_some() {
                match self.kind {
                    Engine::Firefox => self.going = None,
                    Engine::Chrome => return,
                }
            }
            self.entries.truncate(self.current + 1);
            self.entries.push(mark);
            self.current += 1;
        }

        fn go_back(&mut self, n: usize) {
            self.going = Some(self.current.saturating_sub(n));
        }

        /// The `go`'s task runs: the mark of the entry its `popstate`
        /// reaches.
        fn deliver(&mut self) -> Option<usize> {
            let to = self.going.take()?;
            self.current = to;
            Some(self.entries[to])
        }

        fn apply(&mut self, step: &Step) {
            for mark in &step.push {
                self.push(*mark);
            }
            if let Some(n) = step.back {
                self.go_back(n);
            }
        }
    }

    /// The room's More menu's Stop match (2026-11-12): the menu closes
    /// and the standings open before the menu's `go` is carried out. The
    /// entries stay the history's, every pop is accounted for and a
    /// navigation after it is never held back; then the route's own entry
    /// is the current one again.
    #[test]
    fn a_layer_opening_as_another_closes_waits_for_the_pop() {
        for kind in [Engine::Firefox, Engine::Chrome] {
            let mut b = Browser::new(kind);
            let mut e = Entries::default();
            // More opens.
            b.apply(&e.sync(1, 0.0));
            assert_eq!((b.current, b.entries.clone()), (1, vec![0, 1]), "{kind:?}");
            // It closes: a `go` on its way.
            b.apply(&e.sync(0, 1.0));
            assert!(e.busy());
            // The standings open before the pop: nothing is pushed yet.
            let early = e.sync(1, 1.002);
            assert_eq!(early, Step::default(), "{kind:?}");
            b.apply(&early);
            // The pop comes: ours; then the standings' entry.
            assert_eq!(e.popped(b.deliver().expect("the go is carried out")), Pop::Ours);
            assert!(!e.busy());
            b.apply(&e.sync(1, 1.004));
            assert_eq!((b.current, b.entries[b.current]), (1, 1), "{kind:?}");
            // The standings close; their pop comes.
            b.apply(&e.sync(0, 2.0));
            assert_eq!(e.popped(b.deliver().expect("carried out")), Pop::Ours);
            // Nothing holds a navigation back, on the route's own entry.
            assert!(!e.busy(), "{kind:?}");
            assert_eq!((b.current, b.entries[b.current], e.depth()), (0, 0, 0), "{kind:?}");
            // The browser's own Back and Forward are still told apart.
            b.apply(&e.sync(1, 3.0));
            b.go_back(1);
            assert_eq!(e.popped(b.deliver().unwrap()), Pop::Back);
            assert_eq!(e.depth(), 0);
            assert_eq!(e.popped(1), Pop::Forward(1));
            e.went(3.1);
            assert_eq!(e.popped(0), Pop::Ours);
            assert_eq!(e.popped(0), Pop::Nothing);
        }
    }

    /// What went wrong before: the entry pushed while the `go` was on its
    /// way. In Firefox the pop never comes; in Chrome the entry is gone
    /// and the next close leaves the route.
    #[test]
    fn pushing_before_the_pop_is_what_breaks_the_history() {
        let mut f = Browser::new(Engine::Firefox);
        f.apply(&step(0, 1));
        f.apply(&step(1, 0));
        f.apply(&step(0, 1));
        assert_eq!(f.deliver(), None, "the go was dropped: its pop never comes");
        let mut c = Browser::new(Engine::Chrome);
        c.apply(&step(0, 1));
        c.apply(&step(1, 0));
        c.apply(&step(0, 1));
        assert_eq!(c.deliver(), Some(0));
        assert_eq!(c.current, 0, "on the route's entry with a layer thought pushed");
    }

    /// A pop that never comes (whatever dropped its `go`) holds nothing
    /// back for longer than [`POP_WAIT`]: the entries are read again from
    /// where the history is.
    #[test]
    fn a_lost_pop_is_given_up_after_a_while() {
        let mut e = Entries::default();
        assert_eq!(e.sync(1, 0.0).push, vec![1]);
        assert_eq!(e.sync(0, 5.0).back, Some(1));
        assert!(e.busy() && !e.overdue(5.5));
        assert_eq!(e.sync(2, 5.5), Step::default());
        assert!(e.overdue(5.0 + POP_WAIT + 0.01));
        // Still on the layer's entry: gone back over again.
        e.lost(1);
        assert!(!e.busy());
        assert_eq!(e.sync(0, 6.1).back, Some(1));
        assert_eq!(e.popped(0), Pop::Ours);
        assert_eq!(e.depth(), 0);
    }
}
