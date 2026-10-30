//! Layers and gestures across screens: overlays belong to their screen,
//! covered layers lose their pointers, requests made outside a gesture are
//! refused rather than kept, pads repeat per button.

use std::cell::RefCell;
use std::rc::Rc;

use ddi_actor::commands::parse;
use ddi_actor::{Actor, Kind, Problems, Sprite};
use ddi_ui::event::{MenuEvent, PointerKind, PointerPhase};
use ddi_ui::testing::Harness;
use ddi_ui::{
    Cx, Focusable, Handled, Input, MenuAction, Message, Screen, TextMode, UiEvent, UiRequest,
};

type Log = Rc<RefCell<Vec<String>>>;

fn quad(name: &str, size: [f32; 2], init: &str) -> Actor {
    let mut a = Actor::new(Kind::Sprite(Sprite::quad(size)));
    a.name = name.into();
    let mut p = Problems::default();
    for c in parse(init).unwrap() {
        a.call(&c, &mut p, 0).unwrap();
    }
    a
}

struct Base(&'static str);

impl Screen for Base {
    fn name(&self) -> &str {
        self.0
    }

    fn build(&mut self, _cx: &mut Cx) -> Actor {
        Actor::new(Kind::Frame(vec![]))
    }
}

/// A song wheel: settles on a song two frames in and asks for its preview
/// (from a frame: no gesture); `Btn` stops it and leaves, `Pop` opens a
/// popup; the message `Dialog` opens a dialog, `Popup` a popup.
struct Wheel {
    log: Log,
    frames: u32,
}

impl Screen for Wheel {
    fn name(&self) -> &str {
        "wheel"
    }

    fn build(&mut self, _cx: &mut Cx) -> Actor {
        Actor::new(Kind::Frame(vec![
            quad("Btn", [100.0, 40.0], "xy,320,180"),
            quad("Pop", [50.0, 20.0], "xy,100,100"),
        ]))
    }

    fn focusables(&self) -> Vec<Focusable> {
        vec![Focusable::new("Btn"), Focusable::new("Pop")]
    }

    fn update(&mut self, _dt: f32, _r: &mut Actor, cx: &mut Cx) {
        self.frames += 1;
        if self.frames == 2 {
            cx.request(UiRequest::PlayPreview { song: "old".into() });
        }
    }

    fn input(&mut self, input: &Input, _r: &mut Actor, cx: &mut Cx) -> Handled {
        let line = match input {
            Input::Menu { action, repeat } => format!("menu {action:?} {repeat}"),
            Input::NeedsGesture(r) => format!("needs {r:?}"),
            Input::Activate { name } => format!("activate {name}"),
            Input::Pointer { gesture, .. } => format!("pointer {gesture:?}"),
            _ => String::new(),
        };
        self.log.borrow_mut().push(line);
        match input {
            Input::Activate { name } if name == "Pop" => {
                cx.overlay(Popup, false);
                Handled::Yes
            }
            Input::Activate { name } if name == "Btn" => {
                cx.request(UiRequest::StopPreview);
                cx.replace(Base("base2"));
                Handled::Yes
            }
            _ => Handled::No,
        }
    }

    fn message(&mut self, m: &Message, _r: &mut Actor, cx: &mut Cx) {
        match m.name.as_str() {
            "Dialog" => cx.overlay(Dialog, true),
            "Popup" => cx.overlay(Popup, false),
            _ => {}
        }
    }
}

struct Popup;

impl Screen for Popup {
    fn name(&self) -> &str {
        "popup"
    }

    fn build(&mut self, _cx: &mut Cx) -> Actor {
        Actor::new(Kind::Frame(vec![quad("Item", [50.0, 20.0], "xy,500,300")]))
    }
}

/// A dialog whose `Go` pushes a screen.
struct Dialog;

impl Screen for Dialog {
    fn name(&self) -> &str {
        "dialog"
    }

    fn build(&mut self, _cx: &mut Cx) -> Actor {
        Actor::new(Kind::Frame(vec![quad("Go", [50.0, 20.0], "xy,320,300")]))
    }

    fn focusables(&self) -> Vec<Focusable> {
        vec![Focusable::new("Go")]
    }

    fn input(&mut self, input: &Input, _r: &mut Actor, cx: &mut Cx) -> Handled {
        if let Input::Activate { .. } = input {
            cx.push(Base("pushed"));
            return Handled::Yes;
        }
        Handled::No
    }
}

fn setup() -> (Harness, Log) {
    let log: Log = Rc::default();
    let mut h = Harness::new([640.0, 360.0], [640.0, 360.0], 1.0, TextMode::Mirror);
    h.push(Base("base"));
    h.push(Wheel {
        log: log.clone(),
        frames: 0,
    });
    h.run(0.1);
    (h, log)
}

fn names(h: &Harness) -> Vec<String> {
    h.ui.layers().into_iter().map(|l| l.name).collect()
}

fn pad(action: MenuAction, down: bool, gesture: bool) -> UiEvent {
    UiEvent::Menu(MenuEvent {
        action,
        down,
        source: 0,
        gesture,
        time: 0.0,
    })
}

#[test]
fn requests_outside_a_gesture_are_refused_not_kept() {
    let (mut h, log) = setup();
    // The preview asked for from a frame was refused, the screen told.
    assert!(
        log.borrow()
            .contains(&r#"needs PlayPreview { song: "old" }"#.to_string())
    );
    // A pad press Safari does not count activates Btn: the stop goes out
    // (it needs no gesture), the screen leaves.
    let r = h.send(pad(MenuAction::Start, true, false));
    assert_eq!(r, vec![UiRequest::StopPreview]);
    h.run(0.5);
    assert_eq!(names(&h), vec!["base", "base2"]);
    // A later, unrelated tap starts nothing old.
    let r = h.tap_at(PointerKind::Touch, [10.0, 10.0]);
    assert!(r.is_empty(), "{r:?}");
    assert_eq!(
        h.requested(|r| matches!(r, UiRequest::PlayPreview { .. })),
        0
    );
}

#[test]
fn overlays_leave_with_their_screen() {
    let (mut h, _) = setup();
    h.key("ArrowUp", "ArrowUp");
    assert_eq!(h.ui.focused(), Some("Pop"));
    h.key("Enter", "Enter");
    assert_eq!(names(&h), vec!["base", "wheel", "popup"]);
    // Back: the popup leaves it to the wheel, which backs out, taking
    // its popup along.
    h.key("Escape", "Escape");
    h.run(0.5);
    assert_eq!(names(&h), vec!["base"]);
}

#[test]
fn overlays_sit_on_their_owner_and_screens_on_top() {
    let (mut h, _) = setup();
    h.ui.broadcast(Message::new("Dialog"));
    assert_eq!(names(&h), vec!["base", "wheel", "dialog"]);
    // From the dialog, a screen: above the dialog.
    h.key("Enter", "Enter");
    assert_eq!(names(&h), vec!["base", "wheel", "dialog", "pushed"]);
    // The covered wheel opens a popup: directly above the wheel and its
    // dialog, under the pushed screen, which keeps the input.
    h.ui.broadcast(Message::new("Popup"));
    assert_eq!(
        names(&h),
        vec!["base", "wheel", "dialog", "popup", "pushed"]
    );
    let item = h.centre("Item");
    let r = h.tap_at(PointerKind::Mouse, item);
    assert!(r.is_empty());
    assert_eq!(h.ui.focused(), None);
}

#[test]
fn pads_repeat_per_button() {
    let (mut h, log) = setup();
    // Right held, Up tapped and let go: Right goes on repeating.
    h.send(pad(MenuAction::Right, true, true));
    h.send(pad(MenuAction::Up, true, true));
    h.send(pad(MenuAction::Up, false, true));
    log.borrow_mut().clear();
    h.run(1.0);
    let n = log
        .borrow()
        .iter()
        .filter(|l| *l == "menu Right true")
        .count();
    assert!(n >= 5, "{:?}", log.borrow());
    h.send(pad(MenuAction::Right, false, true));
    log.borrow_mut().clear();
    h.run(1.0);
    assert!(log.borrow().iter().all(|l| !l.starts_with("menu")));
}

#[test]
fn a_dialog_over_a_held_press_cancels_it() {
    let (mut h, log) = setup();
    let at = h.centre("Btn");
    h.pointer(1, PointerKind::Mouse, PointerPhase::Down, at);
    // A modal dialog opens meanwhile: the wheel's press is cancelled.
    h.ui.broadcast(Message::new("Dialog"));
    assert!(log.borrow().contains(&"pointer Cancel".to_string()));
    h.frame(0.016);
    log.borrow_mut().clear();
    let r = h.pointer(1, PointerKind::Mouse, PointerPhase::Up, at);
    assert!(r.is_empty());
    assert!(log.borrow().is_empty(), "{:?}", log.borrow());
}

/// The app's chrome (notices): on top of every screen and dialog, pressed
/// by pointers on its actors only, never given keys, never focused.
struct Chrome {
    log: Log,
}

impl Screen for Chrome {
    fn name(&self) -> &str {
        "chrome"
    }

    fn build(&mut self, _cx: &mut Cx) -> Actor {
        Actor::new(Kind::Frame(vec![quad("Later", [80.0, 30.0], "xy,600,20")]))
    }

    fn input(&mut self, input: &Input, _r: &mut Actor, _cx: &mut Cx) -> Handled {
        if let Input::Pointer {
            target: Some(t),
            gesture: ddi_ui::Gesture::Tap,
            ..
        } = input
        {
            self.log.borrow_mut().push(format!("chrome tap {}", t.name));
            return Handled::Yes;
        }
        self.log.borrow_mut().push("chrome other".into());
        Handled::No
    }
}

#[test]
fn the_chrome_stays_on_top_and_takes_only_its_pointers() {
    let log: Log = Rc::default();
    let mut h = Harness::new([640.0, 360.0], [640.0, 360.0], 1.0, TextMode::Own);
    h.push(Wheel {
        log: log.clone(),
        frames: 0,
    });
    h.ui.push_chrome(Chrome { log: log.clone() });
    h.run(0.2);
    // Keys and menu actions go to the screen.
    h.key("ArrowDown", "ArrowDown");
    assert!(
        log.borrow().iter().any(|l| l == "menu Down false"),
        "{log:?}"
    );
    assert!(
        !log.borrow().iter().any(|l| l.starts_with("chrome")),
        "{log:?}"
    );
    // A tap on the chrome's actor is the chrome's; elsewhere, the screen's.
    h.tap(PointerKind::Mouse, "Later");
    assert!(
        log.borrow().iter().any(|l| l == "chrome tap Later"),
        "{log:?}"
    );
    h.tap(PointerKind::Mouse, "Btn");
    assert!(log.borrow().iter().any(|l| l == "activate Btn"), "{log:?}");
    // A screen pushed later goes under it; the layers list leaves it out.
    h.ui.push(Base("next"));
    h.run(0.3);
    let names: Vec<String> = h.ui.layers().into_iter().map(|l| l.name).collect();
    assert_eq!(names.last().map(String::as_str), Some("next"));
    assert!(!names.iter().any(|n| n == "chrome"), "{names:?}");
    log.borrow_mut().clear();
    h.tap(PointerKind::Mouse, "Later");
    assert!(
        log.borrow().iter().any(|l| l == "chrome tap Later"),
        "{log:?}"
    );
}

/// A screen with something to do `.0` seconds after it is built,
/// nothing moving meanwhile.
struct Due(f64, Log);

impl Screen for Due {
    fn name(&self) -> &str {
        "due"
    }

    fn build(&mut self, cx: &mut Cx) -> Actor {
        self.0 += cx.time();
        Actor::new(Kind::Frame(vec![]))
    }

    fn update(&mut self, _dt: f32, _root: &mut Actor, cx: &mut Cx) {
        if cx.time() >= self.0 {
            self.0 = f64::INFINITY;
            self.1.borrow_mut().push("due".into());
        }
    }

    fn wake_at(&self) -> Option<f64> {
        Some(self.0)
    }
}

#[test]
fn an_idle_shell_wakes_for_a_deadline_of_the_top_screen() {
    let log = Log::default();
    let mut h = Harness::new([1280.0, 720.0], [1280.0, 720.0], 1.0, TextMode::Own);
    h.push(Due(2.0, log.clone()));
    h.run_on_demand(0.5);
    // Idle until it: the shell's wait is the time left.
    assert!(!h.ui.needs_redraw());
    let left = h.ui.wake_in().unwrap();
    assert!(left > 1.0 && left <= 2.0, "{left}");
    let mut since = None;
    assert_eq!(h.ui.idle_wake(&mut since, 10.0), Some(10.0 + left));
    // Woken early (another event): the same time.
    assert_eq!(h.ui.idle_wake(&mut since, 10.5), Some(10.0 + left));
    assert!(log.borrow().is_empty());
    // Then a frame meets it, and nothing is left to wait for.
    assert_eq!(h.ui.idle_wake(&mut since, 10.0 + left), None);
    assert_eq!(since, None);
    assert!(h.ui.needs_redraw());
    h.frame(1.0 / 60.0);
    assert_eq!(log.borrow().len(), 1);
    assert_eq!(h.ui.wake_in(), None);
    assert!(!h.ui.needs_redraw());
    // The harness, as the shells: no frames until it is due.
    let log = Log::default();
    let mut h = Harness::new([1280.0, 720.0], [1280.0, 720.0], 1.0, TextMode::Own);
    h.push(Due(3.0, log.clone()));
    let drawn = h.run_on_demand(2.9);
    assert!(log.borrow().is_empty());
    let drawn = drawn + h.run_on_demand(0.5);
    assert_eq!(log.borrow().len(), 1);
    assert!(drawn <= 2, "{drawn} frames");
    // A covered screen waits.
    h.push(Due(1.0, log.clone()));
    h.push(Base("over"));
    h.run_on_demand(3.0);
    assert_eq!(h.ui.wake_in(), None, "covered");
    assert_eq!(log.borrow().len(), 1);
}

/// A screen whose button asks the shell to navigate (the web's way
/// between pages); its root slides out over 0.2 s when it leaves.
struct Leaver(&'static str);

impl Screen for Leaver {
    fn name(&self) -> &str {
        "leaver"
    }

    fn build(&mut self, _cx: &mut Cx) -> Actor {
        let mut root = Actor::new(Kind::Frame(vec![quad("Btn", [100.0, 40.0], "xy,320,180")]));
        root.set_command("Off", parse("linear,0.2;addx,50").unwrap());
        root.set_command("Cancel", parse("linear,0.2;addx,-50").unwrap());
        root
    }

    fn focusables(&self) -> Vec<Focusable> {
        vec![Focusable::new("Btn")]
    }

    fn input(&mut self, input: &Input, _r: &mut Actor, cx: &mut Cx) -> Handled {
        if let Input::Activate { .. } = input {
            cx.request(if self.0 == "back" {
                UiRequest::Back
            } else {
                UiRequest::Navigate(self.0.into())
            });
            return Handled::Yes;
        }
        Handled::No
    }
}

#[test]
fn a_navigation_leaves_the_screen_first() {
    for (what, dx) in [("#/select", 50.0), ("back", -50.0)] {
        let mut h = Harness::new([640.0, 360.0], [640.0, 360.0], 1.0, TextMode::Mirror);
        h.push(Leaver(what));
        h.run(0.1);
        let rest = h.ui.bounds("Btn").unwrap()[0];
        let at_once = h.key("Enter", "Enter");
        let navigates = |r: &[UiRequest]| {
            r.iter()
                .any(|r| matches!(r, UiRequest::Navigate(_) | UiRequest::Back))
        };
        assert!(
            !navigates(&at_once),
            "{what}: the screen leaves first: {at_once:?}"
        );
        assert_eq!(h.ui.layers()[0].phase, "out");
        assert_eq!(
            h.ui.focused(),
            Some("Btn"),
            "{what}: still known while leaving"
        );
        // Out, slid its way (`Off` going on, `Cancel` back).
        h.run(0.1);
        let mid = h.ui.bounds("Btn").unwrap()[0];
        assert!((mid - rest) * dx > 0.0, "{what}: moving: {mid} from {rest}");
        assert!(!navigates(&h.requests), "{what}: not before it has left");
        h.run(0.15);
        assert!(
            navigates(&h.requests),
            "{what}: once it has left: {:?}",
            h.requests
        );
        // Kept as it ended, taking nothing, until the shell acts.
        assert_eq!(h.ui.layers().len(), 1);
        assert_eq!(h.ui.layers()[0].phase, "out");
        assert_eq!(h.ui.focused(), Some("Btn"));
        let n = h.requests.len();
        h.key("Enter", "Enter");
        h.run(0.1);
        assert_eq!(h.requests.len(), n, "{what}: nothing more");
        assert!(!h.ui.needs_redraw(), "{what}: at rest");
    }
}

/// A screen whose root comes in from the left and leaves to the right
/// (back where it was, unseen, as the theme's parts do); its button
/// pushes `Over`, whose button closes it.
struct Under;

impl Screen for Under {
    fn name(&self) -> &str {
        "under"
    }

    fn build(&mut self, _cx: &mut Cx) -> Actor {
        let mut root = Actor::new(Kind::Frame(vec![quad("Btn", [100.0, 40.0], "xy,320,180")]));
        root.set_command("On", parse("addx,-50;linear,0.2;addx,50").unwrap());
        root.set_command("Off", parse("linear,0.2;addx,50;sleep,0;addx,-50").unwrap());
        root
    }

    fn focusables(&self) -> Vec<Focusable> {
        vec![Focusable::new("Btn")]
    }

    fn input(&mut self, input: &Input, _r: &mut Actor, cx: &mut Cx) -> Handled {
        if let Input::Activate { .. } = input {
            cx.push(Over);
            return Handled::Yes;
        }
        Handled::No
    }
}

struct Over;

impl Screen for Over {
    fn name(&self) -> &str {
        "over"
    }

    fn build(&mut self, _cx: &mut Cx) -> Actor {
        let mut root = Actor::new(Kind::Frame(vec![quad("Go", [100.0, 40.0], "xy,320,180")]));
        root.set_command(
            "On",
            parse("diffusealpha,0;linear,0.2;diffusealpha,1").unwrap(),
        );
        root.set_command("Cancel", parse("linear,0.2;diffusealpha,0").unwrap());
        root
    }

    fn focusables(&self) -> Vec<Focusable> {
        vec![Focusable::new("Go")]
    }

    fn input(&mut self, input: &Input, _r: &mut Actor, cx: &mut Cx) -> Handled {
        if let Input::Activate { .. } = input {
            cx.close(true);
            return Handled::Yes;
        }
        Handled::No
    }
}

#[test]
fn a_pushed_screen_waits_for_the_one_under_it_which_comes_back() {
    let mut h = Harness::new([640.0, 360.0], [640.0, 360.0], 1.0, TextMode::Mirror);
    h.push(Under);
    h.run(0.3);
    let rest = h.ui.bounds("Btn").unwrap()[0];
    let phases = |h: &Harness| -> Vec<(String, &'static str)> {
        h.ui.layers()
            .into_iter()
            .map(|l| (l.name, l.phase))
            .collect()
    };
    h.key("Enter", "Enter");
    assert_eq!(phases(&h), vec![("under".to_string(), "covering")]);
    h.run(0.1);
    let mid = h.ui.bounds("Btn").unwrap()[0];
    assert!(mid > rest, "leaving to the right: {mid} from {rest}");
    assert_eq!(phases(&h), vec![("under".to_string(), "covering")]);
    assert_eq!(h.ui.focused(), Some("Btn"), "kept while covered");
    // Gone: back where it was, the pushed screen coming in over it.
    h.run(0.15);
    assert_eq!(
        phases(&h),
        vec![("under".to_string(), "covered"), ("over".to_string(), "in")]
    );
    assert_eq!(h.ui.bounds("Btn").unwrap()[0], rest);
    h.run(0.3);
    assert_eq!(
        phases(&h),
        vec![
            ("under".to_string(), "covered"),
            ("over".to_string(), "active")
        ]
    );
    assert!(!h.ui.needs_redraw(), "covered, at rest");
    assert_eq!(h.ui.focused(), Some("Go"));
    // Closed, the one under it comes in again, from the left.
    h.key("Enter", "Enter");
    assert_eq!(
        phases(&h),
        vec![
            ("under".to_string(), "covered"),
            ("over".to_string(), "out")
        ]
    );
    h.run(0.25);
    assert_eq!(phases(&h), vec![("under".to_string(), "in")]);
    let back = h.ui.bounds("Btn").unwrap()[0];
    assert!(back < rest, "coming in from the left: {back} to {rest}");
    assert_eq!(h.ui.focused(), Some("Btn"));
    h.run(0.3);
    assert_eq!(phases(&h), vec![("under".to_string(), "active")]);
    assert_eq!(h.ui.bounds("Btn").unwrap()[0], rest);
    // The shell's own push is immediate: the screen under is covered as
    // it is, and comes in again when uncovered.
    h.push(Over);
    assert_eq!(
        phases(&h),
        vec![("under".to_string(), "covered"), ("over".to_string(), "in")]
    );
    h.run(0.3);
    h.key("Enter", "Enter");
    h.run(0.25);
    assert_eq!(phases(&h), vec![("under".to_string(), "in")]);
}

/// A screen takes input as soon as it comes in; a press while the screen
/// under it leaves (covering) or while it leaves itself is dropped.
#[test]
fn a_screen_takes_input_while_it_comes_in_but_not_while_one_leaves() {
    let mut h = Harness::new([640.0, 360.0], [640.0, 360.0], 1.0, TextMode::Mirror);
    h.push(Under);
    h.run(0.3);
    let phases = |h: &Harness| -> Vec<(String, &'static str)> {
        h.ui.layers()
            .into_iter()
            .map(|l| (l.name, l.phase))
            .collect()
    };
    // Covering: a second press goes nowhere (no second `Over`).
    h.key("Enter", "Enter");
    h.run(0.05);
    h.key("Enter", "Enter");
    assert_eq!(phases(&h), vec![("under".to_string(), "covering")]);
    h.run(0.2);
    assert_eq!(
        phases(&h),
        vec![("under".to_string(), "covered"), ("over".to_string(), "in")]
    );
    // Coming in: Start acts at once (it closes `Over`).
    h.key("Enter", "Enter");
    assert_eq!(
        phases(&h),
        vec![
            ("under".to_string(), "covered"),
            ("over".to_string(), "out")
        ]
    );
    // Leaving: dropped; then the screen under comes back and takes input
    // while it comes in.
    h.key("Enter", "Enter");
    for _ in 0..30 {
        if h.ui.layers().len() == 1 {
            break;
        }
        h.run(0.05);
    }
    assert_eq!(phases(&h), vec![("under".to_string(), "in")]);
    h.key("Enter", "Enter");
    assert_eq!(phases(&h), vec![("under".to_string(), "covering")]);
    // Back during an entrance leaves.
    for _ in 0..30 {
        if h.ui.layers().len() == 2 {
            break;
        }
        h.run(0.05);
    }
    assert_eq!(h.ui.layers()[1].phase, "in");
    h.key("Escape", "Escape");
    assert_eq!(h.ui.layers()[1].phase, "out");
}

/// A screen recording what it is told of being shown; Enter pushes
/// `Over`, D opens a dialog, P a popup, B leaves with Back.
struct Shown {
    log: Log,
}

impl Screen for Shown {
    fn name(&self) -> &str {
        "shown"
    }

    fn build(&mut self, _cx: &mut Cx) -> Actor {
        let mut root = Actor::new(Kind::Frame(vec![quad("Btn", [100.0, 40.0], "xy,320,180")]));
        root.set_command(
            "On",
            parse("diffusealpha,0;linear,0.1;diffusealpha,1").unwrap(),
        );
        root.set_command("Off", parse("linear,0.1;diffusealpha,0").unwrap());
        root
    }

    fn focusables(&self) -> Vec<Focusable> {
        vec![Focusable::new("Btn")]
    }

    fn shown(&mut self, shown: bool, _r: &mut Actor, _cx: &mut Cx) {
        self.log.borrow_mut().push(format!("shown {shown}"));
    }

    fn input(&mut self, input: &Input, _r: &mut Actor, cx: &mut Cx) -> Handled {
        match input {
            Input::Activate { .. } => cx.push(Over),
            Input::Key(k) if k.down && k.key == "d" => cx.overlay(Dialog, true),
            Input::Key(k) if k.down && k.key == "p" => cx.overlay(Popup, false),
            Input::Key(k) if k.down && k.key == "b" => cx.request(UiRequest::Back),
            _ => return Handled::No,
        }
        Handled::Yes
    }
}

#[test]
fn screens_are_told_when_covered_and_uncovered_but_not_for_dialogs() {
    let log: Log = Rc::default();
    let mut h = Harness::new([640.0, 360.0], [640.0, 360.0], 1.0, TextMode::Mirror);
    h.push(Shown { log: log.clone() });
    h.run(0.3);
    assert!(log.borrow().is_empty(), "nothing at its push");
    // A dialog and a popup cover nothing.
    h.key("KeyD", "d");
    h.run(0.1);
    h.key("Escape", "Escape");
    h.run(0.3);
    h.key("KeyP", "p");
    h.run(0.1);
    assert!(log.borrow().is_empty(), "{:?}", log.borrow());
    h.key("Escape", "Escape");
    h.run(0.3);
    // Covered by a screen's push: once; uncovered as it comes in: once.
    h.key("Enter", "Enter");
    h.run(0.5);
    assert_eq!(*log.borrow(), ["shown false"]);
    h.key("Enter", "Enter");
    h.run(0.35);
    assert_eq!(*log.borrow(), ["shown false", "shown true"]);
    h.run(0.3);
    // The shell's own push: once.
    h.push(Over);
    assert_eq!(log.borrow().len(), 3);
    assert_eq!(log.borrow()[2], "shown false");
    h.run(0.3);
    h.key("Enter", "Enter");
    h.run(0.5);
    assert_eq!(log.borrow()[3], "shown true");
    // Leaving with Back: once, as it starts leaving.
    h.key("KeyB", "b");
    assert_eq!(log.borrow().len(), 5);
    assert_eq!(log.borrow()[4], "shown false");
    h.run(0.5);
    assert_eq!(log.borrow().len(), 5);
}
