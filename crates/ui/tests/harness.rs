//! The UI core driven headlessly through `ddi_ui::testing::Harness`, with
//! three small screens: a menu (buttons, a search field, a scrolling
//! list), a screen that comes in and backs out with transitions, and a
//! modal dialog.

use std::cell::RefCell;
use std::rc::Rc;

use ddi_actor::commands::parse;
use ddi_actor::text::TextStyle;
use ddi_actor::{Actor, Kind, Problems, Sprite, Text};
use ddi_ui::event::{KeyEvent, Modifiers, PointerKind, PointerPhase, TextEvent};
use ddi_ui::hit::find_mut;
use ddi_ui::request::{EnterKey, InputHints, InputMode};
use ddi_ui::testing::Harness;
use ddi_ui::{
    Cx, Focusable, Gesture, Handled, Input, MenuAction, Message, Param, RING_COLOR, Screen,
    TextChange, TextMode, UiEvent, UiRequest,
};

type Log = Rc<RefCell<Vec<String>>>;

/// 640×360 virtual on a 1280×720 CSS surface at 1.5×: 3 physical pixels
/// and 2 CSS pixels a virtual one.
fn harness(mode: TextMode) -> (Harness, Log) {
    let log: Log = Rc::default();
    let mut h = Harness::new([640.0, 360.0], [1280.0, 720.0], 1.5, mode);
    h.push(Menu { log: log.clone() });
    (h, log)
}

fn commands(a: &mut Actor, s: &str) {
    let mut p = Problems::default();
    for c in parse(s).unwrap() {
        a.call(&c, &mut p, 0).unwrap();
    }
}

fn actor(kind: Kind, name: &str, init: &str) -> Actor {
    let mut a = Actor::new(kind);
    a.name = name.into();
    if !init.is_empty() {
        commands(&mut a, init);
    }
    a
}

fn quad(name: &str, size: [f32; 2], init: &str) -> Actor {
    actor(Kind::Sprite(Sprite::quad(size)), name, init)
}

fn text(name: &str, s: &str, size: f32, init: &str) -> Actor {
    actor(
        Kind::Text(Text::new(s, TextStyle::new("sans-serif", size))),
        name,
        init,
    )
}

fn frame(name: &str, children: Vec<Actor>, init: &str) -> Actor {
    actor(Kind::Frame(children), name, init)
}

fn on(a: &mut Actor, command: &str, s: &str) {
    a.set_command(command, parse(s).unwrap());
}

fn button(name: &str, y: f32) -> Actor {
    let hide = format!("Hide{name}Message");
    let mut b = frame(
        name,
        vec![
            quad("", [160.0, 40.0], ""),
            text(&format!("{name}Label"), name, 16.0, ""),
        ],
        &format!("xy,320,{y}"),
    );
    on(&mut b, "GainFocus", "zoom,1.1");
    on(&mut b, "LoseFocus", "zoom,1");
    on(&mut b, &hide, "visible,0");
    b
}

fn describe(input: &Input) -> String {
    let name = |t: &Option<ddi_ui::Target>| t.as_ref().map_or("-".to_string(), |t| t.name.clone());
    match input {
        Input::Menu { action, repeat } => {
            format!("menu {action:?}{}", if *repeat { " repeat" } else { "" })
        }
        Input::Key(k) => format!("key {}", k.key),
        Input::Pointer {
            target, gesture, ..
        } => match gesture {
            Gesture::Press => format!("press {}", name(target)),
            Gesture::Tap => format!("tap {}", name(target)),
            Gesture::LongPress => format!("long {}", name(target)),
            Gesture::DragStart => format!("dragstart {}", name(target)),
            Gesture::Drag { delta } => format!("drag {} {}", delta[0], delta[1]),
            Gesture::DragEnd { velocity } => format!("dragend {:.0}", velocity[1]),
            Gesture::Flick { velocity } => format!("flick {:.0}", velocity[1]),
            Gesture::Release => format!("release {}", name(target)),
            Gesture::Cancel => format!("cancel {}", name(target)),
        },
        Input::Hover { target } => format!("hover {}", name(target)),
        Input::Wheel { target, delta, .. } => format!("wheel {} {}", name(target), delta[1]),
        Input::Activate { name } => format!("activate {name}"),
        Input::Focus { to, .. } => format!("focus {}", to.as_deref().unwrap_or("-")),
        Input::Text { change, field, .. } => format!("text {change:?} {}", field.text()),
        Input::Outside => "outside".into(),
        Input::NeedsGesture(r) => format!("needs {r:?}"),
        Input::MenuRelease { source } => format!("menu release {source:?}"),
        Input::CancelDirections => "cancel directions".into(),
        Input::Raw(e) => format!("raw {} {}", e.control, e.pressed),
    }
}

/// Buttons, a search field, a clipped list of rows, a turned badge.
struct Menu {
    log: Log,
}

impl Screen for Menu {
    fn name(&self) -> &str {
        "menu"
    }

    fn build(&mut self, _cx: &mut Cx) -> Actor {
        let mut title = text("Title", "Dito", 20.0, "xy,320,40");
        on(
            &mut title,
            "SongChangedMessage",
            "linear,0.5;diffuse,1,0,0,1",
        );
        let rows = (0..10)
            .map(|i| {
                quad(
                    &format!("Row{i}"),
                    [120.0, 30.0],
                    &format!("y,{}", -85.0 + i as f32 * 40.0),
                )
            })
            .collect();
        let mut list = frame("List", vec![frame("Rows", rows, "")], "xy,540,200");
        list.clip = Some([-60.0, -100.0, 60.0, 100.0]);
        frame(
            "",
            vec![
                title,
                button("Play", 120.0),
                button("Options", 180.0),
                button("Quit", 240.0),
                // The field: its box and its text in one named frame, so a
                // press on either is inside it.
                frame(
                    "SearchBox",
                    vec![
                        quad("", [200.0, 24.0], "diffusealpha,0.2"),
                        text("Search", "", 16.0, "x,-90;halign,0;diffuse,0.9,0.9,0.2,1"),
                    ],
                    "xy,200,320",
                ),
                list,
                quad("Badge", [20.0, 20.0], "xy,560,40;zoom,2;rotationz,45"),
            ],
            "",
        )
    }

    fn focusables(&self) -> Vec<Focusable> {
        vec![
            Focusable::new("Play"),
            Focusable::new("Options"),
            Focusable {
                down: Some("Play".into()),
                ..Focusable::new("Quit")
            },
            Focusable::new("SearchBox"),
        ]
    }

    fn input(&mut self, input: &Input, root: &mut Actor, cx: &mut Cx) -> Handled {
        self.log
            .borrow_mut()
            .push(format!("menu: {}", describe(input)));
        match input {
            Input::Activate { name } => match name.as_str() {
                "Play" => {
                    cx.request(UiRequest::StartAudio);
                    cx.push(Second {
                        log: self.log.clone(),
                    });
                }
                "Options" => cx.overlay(
                    Dialog {
                        log: self.log.clone(),
                    },
                    true,
                ),
                "Quit" => cx.request(UiRequest::OpenUrl("https://example.org".into())),
                "SearchBox" => cx.begin_text(
                    "Search",
                    "",
                    InputHints {
                        mode: InputMode::Search,
                        enter: EnterKey::Search,
                        max_chars: Some(40),
                    },
                ),
                _ => {}
            },
            Input::Pointer {
                target: Some(t),
                gesture: Gesture::LongPress,
                ..
            } if t.within("Play") => {
                cx.request(UiRequest::PlayPreview {
                    song: "dito".into(),
                });
            }
            Input::Pointer {
                target: Some(t),
                gesture: Gesture::Drag { delta },
                ..
            } if t.within("List") => {
                let rows = find_mut(root, "Rows").unwrap();
                let y = rows.current().pos[1] + delta[1];
                commands(rows, &format!("y,{y}"));
                return Handled::Yes;
            }
            Input::Text {
                change: TextChange::Submit | TextChange::Cancel,
                ..
            } => cx.end_text(),
            Input::Key(k) if k.key == "/" => return Handled::Yes,
            // An autocomplete would move through its list.
            Input::Key(k) if k.key == "ArrowDown" => return Handled::Yes,
            Input::Wheel {
                target: Some(t), ..
            } if t.within("List") => return Handled::Yes,
            _ => {}
        }
        Handled::No
    }

    fn message(&mut self, m: &Message, _root: &mut Actor, _cx: &mut Cx) {
        self.log
            .borrow_mut()
            .push(format!("menu: message {} {:?}", m.name, m.get("Song")));
    }
}

/// Comes in over 0.5 s (its `In` sleeps), backs out over 0.3 s (its
/// `Cancel`), leaves over 0.2 s (`Off`); Select replaces it.
struct Second {
    log: Log,
}

impl Screen for Second {
    fn name(&self) -> &str {
        "second"
    }

    fn build(&mut self, _cx: &mut Cx) -> Actor {
        let mut in_layer = quad("In", [640.0, 360.0], "visible,0");
        on(&mut in_layer, "StartTransitioning", "sleep,0.5");
        let mut cancel = quad("Cancel", [640.0, 360.0], "visible,0");
        on(&mut cancel, "StartTransitioning", "linear,0.3");
        let mut root = frame(
            "",
            vec![quad("Ok", [100.0, 40.0], "xy,320,180"), in_layer, cancel],
            "",
        );
        on(&mut root, "On", "diffusealpha,0;linear,0.25;diffusealpha,1");
        on(&mut root, "Off", "linear,0.2;diffusealpha,0");
        root
    }

    fn focusables(&self) -> Vec<Focusable> {
        vec![Focusable::new("Ok")]
    }

    fn input(&mut self, input: &Input, _root: &mut Actor, cx: &mut Cx) -> Handled {
        self.log
            .borrow_mut()
            .push(format!("second: {}", describe(input)));
        if let Input::Menu {
            action: MenuAction::Select,
            ..
        } = input
        {
            cx.replace(Third);
            return Handled::Yes;
        }
        Handled::No
    }

    fn message(&mut self, m: &Message, _root: &mut Actor, _cx: &mut Cx) {
        self.log
            .borrow_mut()
            .push(format!("second: message {}", m.name));
    }
}

struct Third;

impl Screen for Third {
    fn name(&self) -> &str {
        "third"
    }

    fn build(&mut self, _cx: &mut Cx) -> Actor {
        frame("", vec![quad("Done", [10.0, 10.0], "")], "")
    }
}

/// A modal yes/no.
struct Dialog {
    log: Log,
}

impl Screen for Dialog {
    fn name(&self) -> &str {
        "dialog"
    }

    fn build(&mut self, _cx: &mut Cx) -> Actor {
        frame(
            "",
            vec![
                quad(
                    "Panel",
                    [300.0, 120.0],
                    "xy,320,180;diffuse,0.1,0.1,0.1,0.9",
                ),
                quad("Yes", [100.0, 40.0], "xy,260,200"),
                quad("No", [100.0, 40.0], "xy,380,200"),
            ],
            "",
        )
    }

    fn focusables(&self) -> Vec<Focusable> {
        vec![Focusable::new("Yes"), Focusable::new("No")]
    }

    fn input(&mut self, input: &Input, _root: &mut Actor, cx: &mut Cx) -> Handled {
        self.log
            .borrow_mut()
            .push(format!("dialog: {}", describe(input)));
        match input {
            Input::Outside => cx.close(true),
            Input::Activate { name } if name == "Yes" => {
                cx.broadcast(Message::new("Confirmed"));
                cx.close(false);
            }
            _ => {}
        }
        Handled::No
    }
}

fn logged(log: &Log, prefix: &str) -> Vec<String> {
    log.borrow()
        .iter()
        .filter(|l| l.starts_with(prefix))
        .cloned()
        .collect()
}

fn clear(log: &Log) {
    log.borrow_mut().clear();
}

fn layers(h: &Harness) -> Vec<(String, &'static str)> {
    h.ui.layers()
        .into_iter()
        .map(|l| (l.name, l.phase))
        .collect()
}

#[test]
fn focus_moves_by_neighbours_and_place_and_the_ring_follows_keys() {
    let (mut h, log) = harness(TextMode::Mirror);
    // The first focusable takes focus and plays `GainFocus`; no ring yet.
    assert_eq!(h.ui.focused(), Some("Play"));
    assert!((h.actor("Play").current().scale[0] - 1.1).abs() < 1e-6);
    assert!(h.quads_of(RING_COLOR).is_empty());
    h.key("ArrowDown", "ArrowDown");
    h.frame(0.0);
    assert_eq!(h.ui.focused(), Some("Options"));
    assert_eq!(h.actor("Play").current().scale[0], 1.0);
    // The ring: four quads around the zoomed button.
    assert_eq!(h.quads_of(RING_COLOR).len(), 4);
    h.key("ArrowDown", "ArrowDown");
    // Quit's explicit neighbour below is Play.
    h.key("ArrowDown", "ArrowDown");
    assert_eq!(h.ui.focused(), Some("Play"));
    // Nothing focusable above Play.
    h.key("ArrowUp", "ArrowUp");
    assert_eq!(h.ui.focused(), Some("Play"));
    h.key("ArrowDown", "ArrowDown");
    h.key("ArrowDown", "ArrowDown");
    assert_eq!(h.ui.focused(), Some("Quit"));
    // Left of Quit, by place: the search box.
    h.key("ArrowLeft", "ArrowLeft");
    assert_eq!(h.ui.focused(), Some("SearchBox"));
    // Nothing further left.
    h.key("ArrowLeft", "ArrowLeft");
    assert_eq!(h.ui.focused(), Some("SearchBox"));
    // A mouse moving hides the ring.
    h.hover([10.0, 10.0]);
    h.frame(0.0);
    assert!(!h.ui.focus_ring_shown());
    assert!(h.quads_of(RING_COLOR).is_empty());
    // Shortcuts by logical key go to the screen; unknown keys too.
    clear(&log);
    h.key("Slash", "/");
    assert_eq!(logged(&log, "menu: key"), vec!["menu: key /"]);
    // Navigation reads physical codes: an AZERTY "a" on KeyQ is no
    // direction, and nothing moves.
    h.key("KeyQ", "a");
    assert_eq!(h.ui.focused(), Some("SearchBox"));
}

#[test]
fn held_directions_repeat_by_us_and_stop_on_blur() {
    let (mut h, log) = harness(TextMode::Mirror);
    clear(&log);
    h.key_down("ArrowDown", "ArrowDown");
    // OS repeats are ignored.
    for _ in 0..10 {
        let mut e = KeyEvent::down("ArrowDown", "ArrowDown");
        e.repeat = true;
        h.send(UiEvent::Key(e));
    }
    h.run(0.95);
    // The press, then repeats at 0.375, 0.5, 0.625, 0.75 and 0.875 s.
    let menus = logged(&log, "menu: menu");
    assert_eq!(menus.len(), 6, "{menus:?}");
    assert_eq!(logged(&log, "menu: menu Down repeat").len(), 5);
    assert_eq!(logged(&log, "menu: focus").len(), 6);
    // Losing focus lets go: no key up will come.
    h.send(UiEvent::Focus(false));
    h.run(1.0);
    assert_eq!(logged(&log, "menu: menu").len(), 6);
    // A pad repeats alike.
    clear(&log);
    h.pad(MenuAction::Up, true);
    h.run(0.45);
    h.pad(MenuAction::Up, false);
    h.run(0.5);
    assert_eq!(logged(&log, "menu: menu Up").len(), 2, "{:?}", log.borrow());
}

#[test]
fn gesture_bound_requests_come_back_inside_the_gesture() {
    // A mouse click: the release is within the press's activation. The
    // mouse comes over the button first, as a real one does (a press
    // where it has not been sets its hover, and its cursor, first).
    let (mut h, log) = harness(TextMode::Mirror);
    let at = h.centre("Quit");
    h.hover(at);
    let down = h.pointer(1, PointerKind::Mouse, PointerPhase::Down, at);
    assert!(down.is_empty(), "{down:?}");
    h.now += 100.0;
    let up = h.pointer(1, PointerKind::Mouse, PointerPhase::Up, at);
    assert_eq!(up, vec![UiRequest::OpenUrl("https://example.org".into())]);
    // The tap focused Quit (through its label) and activated it.
    assert!(logged(&log, "menu: tap").contains(&"menu: tap QuitLabel".to_string()));
    assert_eq!(h.ui.focused(), Some("Quit"));

    // A touch: nothing on the press, the request on the release.
    let (mut h, _) = harness(TextMode::Mirror);
    let at = h.centre("Quit");
    assert!(
        h.pointer(1, PointerKind::Touch, PointerPhase::Down, at)
            .is_empty()
    );
    h.now += 100.0;
    assert_eq!(
        h.pointer(1, PointerKind::Touch, PointerPhase::Up, at).len(),
        1
    );

    // A mouse held 1.5 s: its release is no gesture any more. The request
    // is refused, the screen told; nothing comes later.
    let (mut h, log) = harness(TextMode::Mirror);
    h.pointer(1, PointerKind::Mouse, PointerPhase::Down, at);
    h.run(1.5);
    let up = h.pointer(1, PointerKind::Mouse, PointerPhase::Up, at);
    assert!(up.is_empty());
    assert_eq!(
        logged(&log, "menu: needs"),
        vec![r#"menu: needs OpenUrl("https://example.org")"#]
    );
    assert!(h.key_down("KeyA", "a").is_empty());
    assert_eq!(h.requested(|r| matches!(r, UiRequest::OpenUrl(_))), 0);

    // Where gestures are not required (desktop), it goes out at once.
    let log: Log = Rc::default();
    let mut h = Harness::with_gestures([640.0, 360.0], [1280.0, 720.0], 1.5, TextMode::Own, false);
    h.push(Menu { log: log.clone() });
    h.pointer(1, PointerKind::Mouse, PointerPhase::Down, at);
    h.run(1.5);
    let up = h.pointer(1, PointerKind::Mouse, PointerPhase::Up, at);
    assert_eq!(up, vec![UiRequest::OpenUrl("https://example.org".into())]);
    assert!(logged(&log, "menu: needs").is_empty());
}

#[test]
fn long_presses_fire_from_frames_and_wait_for_a_gesture() {
    let (mut h, log) = harness(TextMode::Mirror);
    let at = h.centre("Play");
    h.pointer(1, PointerKind::Touch, PointerPhase::Down, at);
    assert!(h.ui.needs_redraw(), "a long press needs frames");
    h.run(0.6);
    assert!(logged(&log, "menu: long").contains(&"menu: long PlayLabel".to_string()));
    // The preview needs a gesture: refused, the screen told.
    assert_eq!(
        logged(&log, "menu: needs"),
        vec![r#"menu: needs PlayPreview { song: "dito" }"#]
    );
    // The lift after a long press is no tap (Play is not activated) and no
    // gesture either (P2).
    h.now += 100.0;
    let up = h.pointer(1, PointerKind::Touch, PointerPhase::Up, at);
    assert!(up.is_empty());
    assert!(logged(&log, "menu: release").contains(&"menu: release PlayLabel".to_string()));
    assert!(logged(&log, "menu: activate").is_empty());
    assert_eq!(
        h.requested(|r| matches!(r, UiRequest::PlayPreview { .. })),
        0
    );

    // The first frame after the shell idled spans the gap: no long press.
    let (mut h, log) = harness(TextMode::Mirror);
    h.pointer(1, PointerKind::Touch, PointerPhase::Down, at);
    h.frame(0.8);
    assert!(logged(&log, "menu: long").is_empty());
    // Desktop: the preview goes out from the frame.
    let log: Log = Rc::default();
    let mut h = Harness::with_gestures([640.0, 360.0], [1280.0, 720.0], 1.5, TextMode::Own, false);
    h.push(Menu { log: log.clone() });
    h.pointer(1, PointerKind::Touch, PointerPhase::Down, at);
    h.run(0.6);
    assert_eq!(
        h.requested(|r| matches!(r, UiRequest::PlayPreview { .. })),
        1
    );
}

#[test]
fn hidden_focus_falls_back_to_the_first_shown() {
    let (mut h, _) = harness(TextMode::Mirror);
    assert_eq!(h.ui.focused(), Some("Play"));
    h.ui.broadcast(Message::new("HidePlay"));
    // Play is gone: a direction goes to the first focusable shown, and
    // the ring is not drawn round nothing.
    h.frame(0.0);
    h.key("ArrowUp", "ArrowUp");
    assert_eq!(h.ui.focused(), Some("Options"));
    h.ui.broadcast(Message::new("HideOptions"));
    // Start too, without activating anything.
    let r = h.key("Enter", "Enter");
    assert!(r.is_empty());
    assert_eq!(h.ui.focused(), Some("Quit"));
}

#[test]
fn consumed_events_are_reported() {
    let (mut h, _) = harness(TextMode::Mirror);
    // Menu keys are ours (no page scroll), OS repeats too.
    h.key_down("ArrowDown", "ArrowDown");
    assert!(h.consumed);
    let mut e = KeyEvent::down("ArrowDown", "ArrowDown");
    e.repeat = true;
    h.send(UiEvent::Key(e));
    assert!(h.consumed);
    h.key_up("ArrowDown", "ArrowDown");
    // Select (Tab) does nothing by default, and is still ours.
    h.key_down("Tab", "Tab");
    assert!(h.consumed);
    h.key_up("Tab", "Tab");
    // A key nobody wants is not.
    h.key_down("KeyQ", "q");
    assert!(!h.consumed);
    // A wheel the list scrolls is (ctrl+wheel too); elsewhere not.
    h.wheel_lines([540.0, 215.0], [0.0, 1.0]);
    assert!(h.consumed);
    h.wheel_lines([20.0, 20.0], [0.0, 1.0]);
    assert!(!h.consumed);
    // Typing: the textarea's keys are not consumed; Enter and keys the
    // screen takes (an autocomplete's ArrowDown) are.
    h.tap(PointerKind::Mouse, "SearchBox");
    for (code, key) in [
        ("Space", " "),
        ("Backspace", "Backspace"),
        ("ArrowLeft", "ArrowLeft"),
    ] {
        h.key_down(code, key);
        assert!(!h.consumed, "{key}");
    }
    // Tab stays in the field (the screen hears it).
    h.key_down("Tab", "Tab");
    assert!(h.consumed);
    h.key_down("ArrowDown", "ArrowDown");
    assert!(h.consumed);
    h.key_down("Enter", "Enter");
    assert!(h.consumed);
}

#[test]
fn hits_follow_the_actors_transforms() {
    let (mut h, log) = harness(TextMode::Mirror);
    // The badge is a 40×40 square turned 45°: a diamond reaching 28.3 px
    // from its centre along the axes.
    h.tap_at(PointerKind::Mouse, [585.0, 40.0]);
    assert!(logged(&log, "menu: tap").contains(&"menu: tap Badge".to_string()));
    clear(&log);
    // Inside its unturned square, outside the diamond.
    h.tap_at(PointerKind::Mouse, [578.0, 58.0]);
    assert_eq!(logged(&log, "menu: tap"), vec!["menu: tap -"]);
    // A row under the list's clip is not hit, and a row inside is.
    clear(&log);
    h.tap_at(PointerKind::Mouse, [540.0, 315.0]);
    assert_eq!(logged(&log, "menu: tap"), vec!["menu: tap -"]);
    clear(&log);
    h.tap_at(PointerKind::Mouse, [540.0, 115.0]);
    assert_eq!(logged(&log, "menu: tap"), vec!["menu: tap Row0"]);
}

#[test]
fn drags_and_flicks_in_virtual_pixels() {
    let (mut h, log) = harness(TextMode::Mirror);
    let from = h.centre("Row3");
    // 100 virtual pixels up in 0.1 s: a flick of about 1,000 a second.
    h.drag(PointerKind::Touch, from, [from[0], from[1] - 100.0], 0.1, 5);
    let l = log.borrow().clone();
    assert!(l.contains(&"menu: dragstart Row3".to_string()), "{l:?}");
    let moved: f32 = l
        .iter()
        .filter_map(|s| {
            s.strip_prefix("menu: drag ")?
                .split(' ')
                .nth(1)?
                .parse::<f32>()
                .ok()
        })
        .sum();
    assert!((moved + 100.0).abs() < 1e-3, "{moved}");
    let flick = l
        .iter()
        .find_map(|s| s.strip_prefix("menu: flick "))
        .unwrap();
    let v: f32 = flick.parse().unwrap();
    assert!((v + 1000.0).abs() < 60.0, "{v}");
    // The screen scrolled its rows with the drag.
    assert!((h.actor("Rows").current().pos[1] + 100.0).abs() < 1e-3);
    // A slow drag just ends.
    clear(&log);
    let from = h.centre("Row5");
    h.drag(PointerKind::Mouse, from, [from[0], from[1] + 30.0], 1.0, 10);
    assert_eq!(logged(&log, "menu: dragend").len(), 1, "{:?}", log.borrow());
    assert!(logged(&log, "menu: tap").is_empty());
}

#[test]
fn wheel_lines_become_virtual_pixels_on_the_target() {
    let (mut h, log) = harness(TextMode::Mirror);
    // Over a row, then between rows (the list's own clip).
    h.wheel_lines([540.0, 200.0], [0.0, 3.0]);
    h.wheel_lines([540.0, 215.0], [0.0, -1.0]);
    // 3 lines are 120 CSS pixels, 60 virtual.
    assert_eq!(
        logged(&log, "menu: wheel"),
        vec!["menu: wheel Row2 60", "menu: wheel List -20"]
    );
    // With Ctrl or ⌘ (a trackpad's pinch) it zooms the page: not ours.
    for modifiers in [
        ddi_ui::Modifiers {
            ctrl: true,
            ..Default::default()
        },
        ddi_ui::Modifiers {
            meta: true,
            ..Default::default()
        },
    ] {
        let pos = h.css([540.0, 200.0]);
        h.send(UiEvent::Wheel(ddi_ui::event::WheelEvent {
            pos,
            delta: [0.0, 3.0],
            source: ddi_ui::event::WheelSource::Pixels,
            modifiers,
            time: h.now,
        }));
    }
    assert_eq!(logged(&log, "menu: wheel").len(), 2);
}

#[test]
fn screens_come_in_and_back_out_with_their_transitions() {
    let (mut h, log) = harness(TextMode::Mirror);
    // Start on Play: the audio request comes back inside the key press.
    let r = h.key("Enter", "Enter");
    assert_eq!(r, vec![UiRequest::StartAudio]);
    assert_eq!(
        layers(&h),
        vec![("menu".into(), "covered"), ("second".into(), "in")]
    );
    // Coming in for 0.5 s (its `In` sleeps); input meanwhile goes nowhere.
    h.run(0.25);
    clear(&log);
    h.key("Enter", "Enter");
    h.key("ArrowDown", "ArrowDown");
    assert!(log.borrow().is_empty(), "{:?}", log.borrow());
    assert!(h.ui.needs_redraw());
    h.run(0.3);
    assert_eq!(layers(&h)[1], ("second".into(), "active"));
    assert_eq!(h.ui.focused(), Some("Ok"));
    // Back: its `Cancel` layer runs 0.3 s, then it is gone and the menu
    // has its focus and its input back.
    h.key("Escape", "Escape");
    assert_eq!(layers(&h)[1], ("second".into(), "out"));
    h.run(0.2);
    assert_eq!(layers(&h).len(), 2);
    h.run(0.15);
    assert_eq!(layers(&h), vec![("menu".into(), "active")]);
    assert_eq!(h.ui.focused(), Some("Play"));
    clear(&log);
    h.key("ArrowDown", "ArrowDown");
    assert_eq!(h.ui.focused(), Some("Options"));
    // Back on the only screen does nothing.
    h.key("Escape", "Escape");
    assert_eq!(layers(&h).len(), 1);

    // Replace: the old one plays `Off` (0.2 s), then the new one comes.
    h.key("ArrowUp", "ArrowUp");
    h.key("Enter", "Enter");
    h.run(0.6);
    h.key("Tab", "Tab");
    assert_eq!(layers(&h)[1], ("second".into(), "out"));
    h.run(0.25);
    assert_eq!(
        layers(&h),
        vec![("menu".into(), "covered"), ("third".into(), "active")]
    );
}

#[test]
fn dialogs_trap_focus_and_input() {
    let (mut h, log) = harness(TextMode::Mirror);
    h.key("ArrowDown", "ArrowDown");
    h.key("Enter", "Enter");
    assert_eq!(layers(&h)[1], ("dialog".into(), "active"));
    assert_eq!(h.ui.focused(), Some("Yes"));
    h.key("ArrowRight", "ArrowRight");
    assert_eq!(h.ui.focused(), Some("No"));
    h.key("ArrowRight", "ArrowRight");
    h.key("ArrowDown", "ArrowDown");
    assert_eq!(h.ui.focused(), Some("No"));
    // The menu under it kept its focus and heard nothing.
    assert_eq!(h.ui.layers()[0].focused.as_deref(), Some("Options"));
    assert!(logged(&log, "menu: menu Right").is_empty());
    // A press outside (over the menu's title) is the dialog's; it closes.
    // The mouse comes there first, as a real one does.
    let title = h.centre("Title");
    h.hover(title);
    clear(&log);
    h.tap_at(PointerKind::Mouse, title);
    assert_eq!(logged(&log, ""), vec!["dialog: outside"]);
    assert_eq!(layers(&h), vec![("menu".into(), "active")]);
    // Again, and Yes: a message every screen hears, and it closes.
    h.key("Enter", "Enter");
    h.key("Enter", "Enter");
    assert_eq!(
        logged(&log, "menu: message"),
        vec!["menu: message Confirmed None"]
    );
    assert_eq!(layers(&h).len(), 1);
    // Back closes a dialog too.
    h.key("Enter", "Enter");
    h.key("Escape", "Escape");
    assert_eq!(layers(&h).len(), 1);
}

#[test]
fn messages_reach_actors_and_screens() {
    let (mut h, log) = harness(TextMode::Mirror);
    h.key("Enter", "Enter");
    h.ui.broadcast(Message::new("SongChanged").with("Song", Param::Str("u-1".into())));
    assert_eq!(
        logged(&log, "menu: message"),
        vec![r#"menu: message SongChanged Some(Str("u-1"))"#]
    );
    assert_eq!(
        logged(&log, "second: message"),
        vec!["second: message SongChanged"]
    );
    // The title's `SongChangedMessageCommand` tweens it red.
    h.run(0.6);
    assert_eq!(h.actor("Title").current().diffuse[0], [1.0, 0.0, 0.0, 1.0]);
}

#[test]
fn frames_only_while_something_moves() {
    let (mut h, _) = harness(TextMode::Mirror);
    h.run(0.1);
    assert!(!h.ui.needs_redraw());
    assert_eq!(h.run_on_demand(1.0), 0);
    // A message's half-second tween: about 30 frames, then none.
    h.ui.broadcast(Message::new("SongChanged"));
    let n = h.run_on_demand(2.0);
    assert!((30..=33).contains(&n), "{n}");
    // An input: one frame.
    h.hover([5.0, 5.0]);
    assert_eq!(h.run_on_demand(1.0), 1);
}

#[test]
fn losing_focus_cancels_captures() {
    let (mut h, log) = harness(TextMode::Mirror);
    let from = h.centre("Row2");
    h.pointer(7, PointerKind::Touch, PointerPhase::Down, from);
    h.pointer(
        7,
        PointerKind::Touch,
        PointerPhase::Move,
        [from[0], from[1] - 40.0],
    );
    h.send(UiEvent::Focus(false));
    assert!(logged(&log, "menu: cancel").contains(&"menu: cancel Row2".to_string()));
    clear(&log);
    h.pointer(7, PointerKind::Touch, PointerPhase::Up, from);
    assert!(log.borrow().is_empty());
}

#[test]
fn typing_mirrors_the_textarea() {
    let (mut h, log) = harness(TextMode::Mirror);
    // A tap on the box starts typing, inside the tap's release.
    let r = h.tap(PointerKind::Touch, "SearchBox");
    let Some(UiRequest::StartTextInput { caret, hints, .. }) = r.first() else {
        panic!("{r:?}");
    };
    assert_eq!(hints.mode, InputMode::Search);
    // At the field (CSS pixels: twice the virtual ones).
    assert!((caret[0] - 220.0).abs() < 1.0, "{caret:?}");
    // Composing に: shown, underlined, the caret after it.
    h.sync("に", [1, 1], Some([0, 1]));
    assert_eq!(logged(&log, "menu: text"), vec!["menu: text Edited に"]);
    h.frame(0.0);
    assert!(h.texts().contains(&"に".to_string()));
    let ink = [0.9, 0.9, 0.2, 1.0];
    assert_eq!(h.quads_of(ink).len(), 2, "underline and caret");
    // The IME's place follows the caret: 8 virtual pixels after the
    // field's left (110), so x = 236 CSS pixels.
    let ime: Vec<&UiRequest> = h
        .requests
        .iter()
        .filter(|r| matches!(r, UiRequest::SetImeCursorArea(_)))
        .collect();
    let Some(UiRequest::SetImeCursorArea(rect)) = ime.last() else {
        panic!("no caret area");
    };
    assert!((rect[0] - 236.0).abs() < 2.0, "{rect:?}");
    // Enter while composing is the IME's; so is Safari's committing
    // Enter (after `compositionend`, `isComposing` false but key code 229,
    // which the shell reports as composing).
    let mut enter = KeyEvent::down("Enter", "Enter");
    enter.composing = true;
    h.send(UiEvent::Key(enter.clone()));
    h.sync("に", [1, 1], None);
    h.send(UiEvent::Key(enter));
    assert!(logged(&log, "menu: text Submit").is_empty());
    assert!(!h.consumed);
    h.sync("日本", [2, 2], None);
    h.frame(0.0);
    assert_eq!(h.quads_of(ink).len(), 1, "the caret alone");
    assert_eq!(h.ui.text_field().unwrap().1.text(), "日本");
    // Arrows down go to the screen (an autocomplete), not to focus.
    clear(&log);
    h.key("ArrowDown", "ArrowDown");
    assert_eq!(logged(&log, "menu:"), vec!["menu: key ArrowDown"]);
    // Letters are the textarea's: no shortcut, no navigation.
    h.key("Slash", "/");
    assert_eq!(logged(&log, "menu:").len(), 1);
    // Enter submits; the screen ends typing.
    h.key("Enter", "Enter");
    assert_eq!(logged(&log, "menu: text"), vec!["menu: text Submit 日本"]);
    assert!(h.requests.contains(&UiRequest::StopTextInput));
    assert!(h.ui.text_field().is_none());
}

#[test]
fn typing_by_ourselves_on_desktop() {
    let (mut h, log) = harness(TextMode::Own);
    // Start from the keyboard: down to Quit, left to the box, Enter.
    h.key("ArrowDown", "ArrowDown");
    h.key("ArrowDown", "ArrowDown");
    h.key("ArrowLeft", "ArrowLeft");
    let r = h.key("Enter", "Enter");
    assert!(
        matches!(r.first(), Some(UiRequest::StartTextInput { .. })),
        "{r:?}"
    );
    let ime = |t: TextEvent| UiEvent::Text(t);
    h.send(ime(TextEvent::Preedit {
        text: "にほ".into(),
        cursor: Some((6, 6)),
    }));
    h.frame(0.0);
    assert!(h.texts().contains(&"にほ".to_string()));
    h.send(ime(TextEvent::Commit("日本".into())));
    h.send(ime(TextEvent::Commit("語".into())));
    assert_eq!(h.ui.text_field().unwrap().1.text(), "日本語");
    h.key("Backspace", "Backspace");
    assert_eq!(h.ui.text_field().unwrap().1.text(), "日本");
    let ctrl = Modifiers {
        ctrl: true,
        ..Modifiers::default()
    };
    h.key_with("KeyA", "a", ctrl);
    let r = h.key_with("KeyC", "c", ctrl);
    assert_eq!(r, vec![UiRequest::CopyText("日本".into())]);
    let r = h.key_with("KeyV", "v", ctrl);
    assert_eq!(r, vec![UiRequest::Paste]);
    h.send(UiEvent::Paste("dito\ndito".into()));
    assert_eq!(h.ui.text_field().unwrap().1.text(), "dito dito");
    h.frame(0.0);
    assert!(h.texts().contains(&"dito dito".to_string()));
    // Escape cancels; the screen ends typing.
    h.key("Escape", "Escape");
    assert!(logged(&log, "menu: text Cancel").len() == 1);
    assert!(h.ui.text_field().is_none());
}

#[test]
fn typing_ends_on_blur_and_on_a_press_elsewhere() {
    let (mut h, log) = harness(TextMode::Mirror);
    h.tap(PointerKind::Touch, "SearchBox");
    h.sync("dito", [4, 4], None);
    // The textarea lost focus: typing ends, the text kept, nothing to stop.
    clear(&log);
    let r = h.send(UiEvent::Text(TextEvent::Blurred));
    assert!(r.is_empty());
    assert_eq!(logged(&log, "menu: text"), vec!["menu: text Blurred dito"]);
    assert!(h.ui.text_field().is_none());
    // Again, then a press on the field itself keeps typing, and one
    // elsewhere ends it and asks the shell to stop.
    h.tap(PointerKind::Touch, "SearchBox");
    clear(&log);
    let at = h.centre("SearchBox");
    h.pointer(1, PointerKind::Touch, PointerPhase::Down, at);
    assert!(h.ui.text_field().is_some());
    h.pointer(1, PointerKind::Touch, PointerPhase::Cancel, at);
    let title = h.centre("Title");
    let r = h.pointer(2, PointerKind::Mouse, PointerPhase::Down, title);
    assert_eq!(r, vec![UiRequest::StopTextInput]);
    assert_eq!(logged(&log, "menu: text"), vec!["menu: text Blurred "]);
    assert!(h.ui.text_field().is_none());
}

#[test]
fn mirrored_text_keeps_to_the_limit() {
    let (mut h, _) = harness(TextMode::Mirror);
    h.tap(PointerKind::Touch, "SearchBox");
    // 41 characters, 82 UTF-16 units: cut to 40, the textarea told.
    let long: String = "𝄞".repeat(41);
    let r = h.send(UiEvent::Text(TextEvent::Sync(ddi_ui::event::TextSync {
        text: long,
        selection: [82, 82],
        composition: None,
    })));
    assert_eq!(
        r,
        vec![UiRequest::SetText {
            text: "𝄞".repeat(40),
            selection: [80, 80]
        }]
    );
    assert_eq!(h.ui.text_field().unwrap().1.text().chars().count(), 40);
}

#[test]
fn pads_start_audio_but_not_windows() {
    let (mut h, log) = harness(TextMode::Mirror);
    // Start on Quit (a link) from a pad the browser counts as a gesture:
    // refused, as window.open needs a key, a click or a touch.
    h.key("ArrowDown", "ArrowDown");
    h.key("ArrowDown", "ArrowDown");
    clear(&log);
    let r = h.pad(MenuAction::Start, true);
    h.pad(MenuAction::Start, false);
    assert!(r.is_empty(), "{r:?}");
    assert_eq!(
        logged(&log, "menu: needs"),
        vec![r#"menu: needs OpenUrl("https://example.org")"#]
    );
    // Start on Play: audio is the pad's to start.
    h.key("ArrowDown", "ArrowDown");
    let r = h.pad(MenuAction::Start, true);
    assert_eq!(r, vec![UiRequest::StartAudio]);
}

#[test]
fn pasted_line_breaks_are_flattened() {
    let (mut h, _) = harness(TextMode::Mirror);
    h.tap(PointerKind::Touch, "SearchBox");
    let r = h.send(UiEvent::Text(TextEvent::Sync(ddi_ui::event::TextSync {
        text: "a\nb".into(),
        selection: [3, 3],
        composition: None,
    })));
    assert_eq!(
        r,
        vec![UiRequest::SetText {
            text: "a b".into(),
            selection: [3, 3]
        }]
    );
}
