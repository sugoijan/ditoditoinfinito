//! Controllers in the UI core (`docs/plans/controller-input-core.md`): a
//! pad's edges resolved by the UI's bindings, what each held control
//! produced released by its release or its pad's going, and the hints'
//! source following the input the player last used.

use std::cell::RefCell;
use std::rc::Rc;

use ddi_actor::{Actor, Kind};
use ddi_engine::{ConnectedPad, ControlBindings};
use ddi_platform::{DeviceId, HostTime, RawInput};
use ddi_ui::controller_hints::{CHANGED, Source};
use ddi_ui::event::MenuEvent;
use ddi_ui::testing::Harness;
use ddi_ui::{Cx, Handled, Input, MenuAction, Message, Screen, TextMode, UiEvent};

type Log = Rc<RefCell<Vec<String>>>;

/// Logs the menu actions, direction releases, raw edges and hint changes
/// it hears; binds controls (`raw`) or not; closes on Back, and opens a
/// dialog on the message `Dialog` (`dialog`).
struct Recorder {
    name: &'static str,
    log: Log,
    raw: bool,
    dialog: bool,
}

impl Screen for Recorder {
    fn name(&self) -> &str {
        self.name
    }

    fn build(&mut self, _cx: &mut Cx) -> Actor {
        Actor::new(Kind::Frame(vec![]))
    }

    fn direction_edges(&self) -> bool {
        true
    }

    fn raw_input(&self) -> bool {
        self.raw
    }

    fn input(&mut self, input: &Input, _r: &mut Actor, cx: &mut Cx) -> Handled {
        if let Input::Menu {
            action: MenuAction::Back,
            repeat: false,
        } = input
        {
            cx.close(true);
            return Handled::Yes;
        }
        let line = match input {
            Input::Menu { action, repeat } => format!("{} menu {action:?} {repeat}", self.name),
            Input::MenuRelease { source } => format!("{} release {source:?}", self.name),
            Input::Raw(e) => format!("{} raw {} {}", self.name, e.control, e.pressed),
            _ => return Handled::No,
        };
        self.log.borrow_mut().push(line);
        Handled::Yes
    }

    fn message(&mut self, m: &Message, _r: &mut Actor, cx: &mut Cx) {
        if m.name == CHANGED {
            self.log.borrow_mut().push(format!("{} changed", self.name));
        }
        if m.name == "Dialog" && self.dialog {
            cx.overlay(recorder("dialog", &self.log, false), true);
        }
    }
}

fn harness() -> Harness {
    Harness::new([1280.0, 720.0], [1280.0, 720.0], 1.0, TextMode::Mirror)
}

fn recorder(name: &'static str, log: &Log, raw: bool) -> Recorder {
    Recorder {
        name,
        log: log.clone(),
        raw,
        dialog: false,
    }
}

/// Back, as a synthetic action (the source unchanged): the top closes.
fn back(h: &mut Harness) {
    for down in [true, false] {
        h.send(UiEvent::Menu(MenuEvent {
            action: MenuAction::Back,
            down,
            source: 9,
            gesture: false,
            time: 0.0,
        }));
    }
    h.run(0.5);
}

/// Controls whose Back for `Pad` is `control`.
fn back_on(control: &str) -> ControlBindings {
    let mut controls = ControlBindings::default();
    controls.set_pad(
        ddi_engine::PadBindings {
            id: "Pad".into(),
            back: Some(control.into()),
            ..Default::default()
        },
        0.0,
    );
    controls
}

fn pad(id: &str, slot: u32) -> ConnectedPad {
    ConnectedPad {
        id: id.into(),
        index: slot,
        standard: true,
        vendor_product: None,
    }
}

fn edge(id: &str, slot: u32, control: &str, pressed: bool) -> RawInput {
    RawInput {
        device: DeviceId::Gamepad(id.into()),
        control: control.into(),
        pressed,
        host_time: HostTime(0.0),
        slot,
    }
}

fn count(log: &Log, line: &str) -> usize {
    log.borrow().iter().filter(|l| *l == line).count()
}

fn take(log: &Log) -> Vec<String> {
    std::mem::take(&mut *log.borrow_mut())
}

#[test]
fn a_held_pad_that_goes_releases_what_it_pressed() {
    let log = Log::default();
    let mut h = harness();
    h.push(recorder("list", &log, false));
    h.pads(vec![pad("Pad", 0)]);
    h.edge(edge("Pad", 0, "button:13", true));
    assert_eq!(count(&log, "list menu Down false"), 1);
    h.run(0.6);
    assert!(
        count(&log, "list menu Down true") > 0,
        "it repeats while held"
    );
    take(&log);
    h.pads(vec![]);
    assert_eq!(
        log.borrow().as_slice(),
        ["list release Pad(0, Down)", "list changed"],
        "released once, from what it pressed"
    );
    take(&log);
    h.run(1.5);
    // A late release resolves nothing (the pad's gone, its bindings too).
    h.edge(edge("Pad", 0, "button:13", false));
    assert!(log.borrow().is_empty(), "{:?}", log.borrow());
    assert!(!h.ui.hints().active());
}

#[test]
fn a_binding_screen_hears_a_gone_pads_held_controls_let_go() {
    let log = Log::default();
    let mut h = harness();
    h.push(recorder("bind", &log, true));
    h.pads(vec![pad("Pad", 0)]);
    h.edge(edge("Pad", 0, "button:3", true));
    assert_eq!(take(&log), ["bind changed", "bind raw button:3 true"]);
    h.pads(vec![]);
    assert_eq!(take(&log), ["bind raw button:3 false", "bind changed"]);
}

#[test]
fn a_release_goes_where_its_press_went() {
    let log = Log::default();
    let mut h = harness();
    h.push(recorder("list", &log, false));
    h.pads(vec![pad("Pad", 0)]);
    // Pressed as a menu action, released under a binding screen: the
    // action released (no more repeats), the binding screen hears it.
    h.edge(edge("Pad", 0, "button:13", true));
    h.push(recorder("bind", &log, true));
    h.run(0.6);
    take(&log);
    h.edge(edge("Pad", 0, "button:13", false));
    assert_eq!(
        take(&log),
        ["bind release Pad(0, Down)", "bind raw button:13 false"]
    );
    h.run(1.0);
    assert!(log.borrow().iter().all(|l| !l.contains("menu Down")));
    // Pressed under the binding screen, released once it went: no menu
    // release.
    h.edge(edge("Pad", 0, "button:13", true));
    assert_eq!(take(&log), ["bind raw button:13 true"]);
    back(&mut h);
    take(&log);
    h.edge(edge("Pad", 0, "button:13", false));
    h.run(0.5);
    assert!(log.borrow().is_empty(), "{:?}", log.borrow());
}

#[test]
fn hints_follow_the_input_last_used_across_screens() {
    let log = Log::default();
    let mut h = harness();
    h.push(recorder("one", &log, false));
    h.pads(vec![pad("Pad", 0)]);
    assert!(!h.ui.hints().active());
    h.edge(edge("Pad", 0, "button:0", true));
    assert!(h.ui.hints().active());
    // Through a push, a dialog and a pop.
    h.push(Recorder {
        dialog: true,
        ..recorder("two", &log, false)
    });
    h.ui.broadcast(Message::new("Dialog"));
    h.run(0.5);
    assert_eq!(
        h.ui.layers().last().map(|l| l.name.as_str()),
        Some("dialog")
    );
    assert!(h.ui.hints().active());
    back(&mut h);
    back(&mut h);
    assert_eq!(h.ui.layers().last().map(|l| l.name.as_str()), Some("one"));
    assert!(h.ui.hints().active());
    // A key's first press, not its repeat; a held pad does not reclaim.
    h.key("ArrowUp", "ArrowUp");
    assert!(!h.ui.hints().active());
    h.edge(edge("Pad", 0, "button:0", true));
    assert!(!h.ui.hints().active(), "a held control's press again");
    h.edge(edge("Pad", 0, "button:0", false));
    h.edge(edge("Pad", 0, "button:0", true));
    assert!(h.ui.hints().active());
    h.send(UiEvent::Key(ddi_ui::KeyEvent {
        repeat: true,
        ..ddi_ui::KeyEvent::down("ArrowUp", "ArrowUp")
    }));
    assert!(h.ui.hints().active(), "a repeat is not a press");
    // The window lost its focus: what was held is let go, and a press
    // again is fresh.
    h.key("ArrowUp", "ArrowUp");
    h.send(UiEvent::Focus(false));
    h.edge(edge("Pad", 0, "button:0", true));
    assert!(h.ui.hints().active());
    // A synthetic action changes nothing.
    h.ui.set_source(Source::Keyboard);
    h.send(UiEvent::Menu(MenuEvent {
        action: MenuAction::Start,
        down: true,
        source: 0,
        gesture: false,
        time: 0.0,
    }));
    assert!(!h.ui.hints().active());
}

#[test]
fn a_new_ui_takes_the_source_over_while_its_pad_is_there() {
    let source = Source::Pad {
        slot: 0,
        id: "Pad".into(),
    };
    let mut h = harness();
    h.pads(vec![pad("Pad", 0)]);
    h.ui.set_source(source.clone());
    assert!(h.ui.hints().active());
    // Another pad in the slot: the keyboard, and the old one's stale edge
    // claims nothing.
    let mut other = harness();
    other.pads(vec![pad("Other", 0)]);
    other.ui.set_source(source.clone());
    assert_eq!(other.ui.hints().source(), &Source::Keyboard);
    other.edge(edge("Pad", 0, "button:0", true));
    assert_eq!(other.ui.hints().source(), &Source::Keyboard);
    // None connected: the keyboard.
    let mut none = harness();
    none.ui.set_source(source);
    assert_eq!(none.ui.hints().source(), &Source::Keyboard);
}

#[test]
fn hints_change_only_when_something_shown_does() {
    let log = Log::default();
    let mut h = harness();
    h.push(recorder("one", &log, false));
    h.run(0.5);
    assert!(!h.ui.needs_redraw());
    for _ in 0..3 {
        h.ui.update(0.0);
        assert!(!h.ui.needs_redraw());
    }
    // The keyboard is the source: new bindings show nothing.
    let controls = back_on("button:3");
    h.ui.set_controls(controls.clone(), Default::default());
    assert!(!h.ui.needs_redraw() && log.borrow().is_empty());
    h.pads(vec![pad("Pad", 0)]);
    assert!(log.borrow().is_empty(), "the keyboard is the source");
    h.edge(edge("Pad", 0, "button:0", true));
    h.edge(edge("Pad", 0, "button:0", false));
    h.run(0.5);
    take(&log);
    // The same again: nothing.
    h.ui.set_controls(controls.clone(), Default::default());
    assert!(!h.ui.needs_redraw() && log.borrow().is_empty());
    // Other bindings with a pad the source: once, and a frame.
    h.ui.set_controls(back_on("button:2"), Default::default());
    assert!(h.ui.needs_redraw());
    assert_eq!(take(&log), ["one changed"]);
    h.run(0.5);
    // Pads: the same list nothing, another once.
    h.pads(vec![pad("Pad", 0)]);
    assert!(log.borrow().is_empty());
    h.ui.set_pads(vec![pad("Pad", 0), pad("Two", 1)]);
    assert!(h.ui.needs_redraw());
    assert_eq!(take(&log), ["one changed"]);
}

#[test]
fn the_bindings_resolve_a_pads_edges() {
    let log = Log::default();
    let mut h = harness();
    h.push(recorder("one", &log, false));
    h.pads(vec![pad("Pad", 0)]);
    h.edge(edge("Pad", 0, "button:3", true));
    h.edge(edge("Pad", 0, "button:3", false));
    assert_eq!(count(&log, "one menu Select false"), 1);
    h.ui.set_controls(back_on("button:3"), Default::default());
    h.edge(edge("Pad", 0, "button:3", true));
    // Back closed it.
    assert!(h.ui.layers().is_empty() || h.ui.layers()[0].phase != "active");
}

#[test]
fn the_settings_leave_the_hints_alone() {
    ddi_platform::storage::install(Rc::new(ddi_platform::storage::MemoryStore::default()));
    let mut h = harness();
    h.pads(vec![pad("Pad", 0)]);
    h.ui.set_controls(back_on("button:3"), Default::default());
    h.ui.set_source(Source::Pad {
        slot: 0,
        id: "Pad".into(),
    });
    let before = h.ui.hints().clone();
    let settings = ddi_ui::state::settings::Settings::load();
    settings.save();
    h.run(0.1);
    assert_eq!(h.ui.hints(), &before);
}
