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
