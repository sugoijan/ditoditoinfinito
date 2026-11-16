//! A room's stage's prompt once the player is ready (feedback round O:
//! "too easy to cancel a ready"): becoming ready is one press; taking it
//! back, by Ready's button or by Back, is the game's deliberate gesture,
//! held [`CANCEL_HOLD`] or pressed twice within [`CANCEL_DOUBLE_TAP`] (a
//! play's quit's timings), on every input, on the harness's fake clock.

use std::cell::RefCell;
use std::rc::Rc;

use crate::screens::Env;
use crate::screens::data::{ChartEntry, Difficulty, SongEntry};
use crate::screens::prompt::{PromptChoice, PromptScreen, PromptState};
use crate::screens::room::{RoomCall, RoomShell};
use crate::screens::test_kit::{env, harness};
use crate::event::UiEvent;
use crate::gesture::{CANCEL_DOUBLE_TAP, CANCEL_HINT, CANCEL_HOLD};
use crate::state::room::{RoomClient, SharedRoom};
use crate::testing::Harness;
use crate::{MenuAction, PointerKind, PointerPhase};

/// What the prompt told its shell, and whether inside a gesture.
type Seen = Rc<RefCell<Vec<(PromptChoice, bool)>>>;

struct Rig {
    h: Harness,
    room: SharedRoom,
    seen: Seen,
}

fn song() -> SongEntry {
    SongEntry {
        id: "single".into(),
        title: "Single Song".into(),
        artist: "Artist".into(),
        bpm_text: "150".into(),
        length: 120.0,
        charts: vec![ChartEntry {
            style: "dance-single".into(),
            difficulty: Difficulty::Hard,
            meter: 9,
            notes: 300,
            index: 0,
            digest: Some(ddi_chart::ChartDigest(0xabc0)),
            ..ChartEntry::default()
        }],
        ..SongEntry::default()
    }
}

/// A practice room's first stage's prompt. `loads`: the song is prepared
/// at once, so that Ready alone with the CPUs counts the start down;
/// otherwise it never finishes loading, and the room waits with the
/// player ready (as for people still loading or getting ready).
fn rig_with(env: Env, loads: bool, attacks: bool) -> Rig {
    let room = RoomClient::shared(0, 1);
    if attacks {
        let mut rules = room.borrow().rules().clone();
        rules.set_attacks(true);
        room.borrow_mut().save_rules(rules).unwrap();
    }
    let s = song();
    room.borrow_mut().pick(&s, &s.charts[0]).unwrap();
    let order = room.borrow_mut().take_stage_order().unwrap();
    let seen: Seen = Rc::default();
    let sink = seen.clone();
    let shared = room.clone();
    let shell: RoomShell = Rc::new(move |call, _cx| {
        if let RoomCall::Prepare(job) = call
            && loads
        {
            shared.borrow_mut().finish_load(&job, Ok(()));
        }
    });
    let state = Rc::new(RefCell::new(PromptState::default()));
    state.borrow_mut().info.title = "Single Song".into();
    let mut h = harness();
    h.push(
        PromptScreen::new(
            env,
            state,
            Rc::new(move |c, cx| sink.borrow_mut().push((c, cx.in_gesture()))),
        )
        .with_room(room.clone(), order.stage, shell),
    );
    h.run(0.5);
    Rig { h, room, seen }
}

/// Ready, by Enter, the room waiting for others.
fn ready() -> Rig {
    let mut r = rig_with(env(), false, false);
    enter(&mut r.h);
    assert!(r.room.borrow().ready());
    assert_eq!(phase(&r), ddi_room::PhaseKind::Preparing);
    r.h.run(1.0);
    r
}

fn phase(r: &Rig) -> ddi_room::PhaseKind {
    r.room.borrow().state().phase.kind()
}

fn enter(h: &mut Harness) {
    h.key("Enter", "Enter");
}

fn escape(h: &mut Harness) {
    h.key("Escape", "Escape");
}

fn has(h: &Harness, text: &str) -> bool {
    h.texts().iter().any(|t| t == text)
}

fn shown(h: &Harness, name: &str) -> bool {
    h.ui.visible_bounds(name).is_some()
}

/// Button `button`'s fill (`CountdownFill`), when shown: its share.
fn fill(h: &Harness, button: &str) -> Option<f32> {
    let b = h.actor(button);
    let path = crate::hit::find(b, "CountdownFill")?;
    let f = crate::hit::actor_at(b, &path)?;
    let [x0, _, x1, _] = f.clip?;
    let w = f.children().first()?.size()[0];
    f.visible.then_some((x1 - x0) / w)
}

fn backs(r: &Rig) -> usize {
    r.seen
        .borrow()
        .iter()
        .filter(|(c, _)| *c == PromptChoice::Back)
        .count()
}

const UNREADY_HINT: &str = "Hold or press twice to cancel";
const LEAVE_HINT: &str = "Hold or press twice to leave";

/// Ready is one press, in the gesture the web makes its audio context
/// in; then the button says Cancel ready, the hint under it says how,
/// and the footer's keys say both.
#[test]
fn becoming_ready_stays_one_press_and_the_hint_says_how_to_cancel() {
    let mut r = rig_with(env(), false, false);
    assert!(!shown(&r.h, "Room.UnreadyHint"), "nothing to cancel yet");
    assert!(has(&r.h, "Back"), "{:?}", r.h.texts());
    enter(&mut r.h);
    r.h.run(0.1);
    assert!(r.room.borrow().ready());
    assert_eq!(*r.seen.borrow(), [(PromptChoice::Ready, true)]);
    assert!(has(&r.h, "Cancel ready"), "{:?}", r.h.texts());
    assert!(shown(&r.h, "Room.UnreadyHint"));
    assert!(has(&r.h, UNREADY_HINT));
    assert!(!shown(&r.h, "Room.LeaveHint"));
    let footer = |text: &str| r.h.texts().iter().any(|t| t.ends_with(text));
    assert!(footer(" Cancel ready: hold or press twice"), "{:?}", r.h.texts());
    assert!(footer(" Leave: hold or press twice"), "{:?}", r.h.texts());
}

/// One press while ready only shows the hint (brightened); a second
/// within the double press's time takes Ready back; one later is a first
/// press again.
#[test]
fn a_second_press_in_time_takes_ready_back_and_a_late_one_does_not() {
    let mut r = ready();
    let muted = r.h.actor("Room.UnreadyHint").current().diffuse();
    enter(&mut r.h);
    r.h.run(0.1);
    assert!(r.room.borrow().ready(), "one press: still ready");
    assert_ne!(r.h.actor("Room.UnreadyHint").current().diffuse(), muted, "the hint brightened");
    // Too late for a second press.
    r.h.run((CANCEL_DOUBLE_TAP + 0.1) as f32);
    enter(&mut r.h);
    r.h.run(0.1);
    assert!(r.room.borrow().ready(), "too late: a first press again");
    r.h.run((CANCEL_DOUBLE_TAP - 0.25) as f32);
    enter(&mut r.h);
    r.h.run(0.1);
    assert!(!r.room.borrow().ready(), "twice in time: not ready");
    assert!(has(&r.h, "Ready"), "{:?}", r.h.texts());
    assert!(!shown(&r.h, "Room.UnreadyHint"));
    // Nothing more told the shell: taking Ready back needs no audio.
    assert_eq!(r.seen.borrow().len(), 1);
    // The hint fades back over its time.
    let mut r = ready();
    enter(&mut r.h);
    r.h.run((CANCEL_HINT + 0.2) as f32);
    assert_eq!(r.h.actor("Room.UnreadyHint").current().diffuse(), muted);
}

/// Held: Ready taken back at the hold's time and not before, the button
/// filling meanwhile; let go early, nothing, the fill running back.
#[test]
fn a_hold_takes_ready_back_at_its_time_and_a_release_early_does_nothing() {
    let mut r = ready();
    assert_eq!(fill(&r.h, "Start"), None);
    r.h.key_down("Enter", "Enter");
    r.h.run((CANCEL_HOLD * 0.5) as f32);
    let half = fill(&r.h, "Start").expect("the fill follows the hold");
    assert!((0.35..0.65).contains(&half), "{half}");
    r.h.run((CANCEL_HOLD * 0.5 - 0.1) as f32);
    assert!(r.room.borrow().ready(), "not before the hold's time");
    r.h.run(0.15);
    assert!(!r.room.borrow().ready(), "held long enough");
    r.h.key_up("Enter", "Enter");
    r.h.run(0.3);
    assert!(!r.room.borrow().ready(), "the release does nothing");
    assert_eq!(fill(&r.h, "Start"), None);

    let mut r = ready();
    r.h.key_down("Enter", "Enter");
    r.h.run((CANCEL_HOLD * 0.6) as f32);
    r.h.key_up("Enter", "Enter");
    r.h.frame(1.0 / 60.0);
    let back = fill(&r.h, "Start").expect("running back");
    assert!(back > 0.0 && back < 0.6, "{back}");
    r.h.run(1.5);
    assert!(r.room.borrow().ready(), "let go early: still ready");
    assert_eq!(fill(&r.h, "Start"), None, "run back");
}

/// Space and Numpad Enter as Enter; a controller's confirm (A) pressed
/// twice or held; another key's release ends no hold.
#[test]
fn every_confirming_key_and_a_controllers_confirm_take_the_gesture() {
    for code in ["Space", "NumpadEnter"] {
        let mut r = ready();
        r.h.key(code, code);
        r.h.run(0.1);
        assert!(r.room.borrow().ready(), "{code} once");
        r.h.key(code, code);
        r.h.run(0.1);
        assert!(!r.room.borrow().ready(), "{code} twice");
    }
    let pad = |r: &mut Rig, down: bool| {
        r.h.controller(&Default::default(), "Pad", true, "button:0", down);
    };
    let mut r = ready();
    pad(&mut r, true);
    pad(&mut r, false);
    r.h.run(0.1);
    assert!(r.room.borrow().ready(), "A once");
    pad(&mut r, true);
    pad(&mut r, false);
    r.h.run(0.1);
    assert!(!r.room.borrow().ready(), "A twice");
    let mut r = ready();
    pad(&mut r, true);
    r.h.run((CANCEL_HOLD - 0.1) as f32);
    assert!(r.room.borrow().ready());
    r.h.run(0.2);
    assert!(!r.room.borrow().ready(), "A held");
    let mut r = ready();
    pad(&mut r, true);
    r.h.run(0.3);
    pad(&mut r, false);
    r.h.run(1.0);
    assert!(r.room.borrow().ready(), "A let go early");
    // The menu's own pad events (as the web's do): held, then let go.
    let mut r = ready();
    r.h.pad(MenuAction::Start, true);
    r.h.run(0.3);
    r.h.key("KeyX", "x");
    r.h.run(0.2);
    r.h.pad(MenuAction::Start, false);
    r.h.run(1.0);
    assert!(r.room.borrow().ready(), "a key's release is not the pad's");
}

/// A click or a tap on Cancel ready: once, the hint; a double click or a
/// double tap takes Ready back, its last tap pressing nothing (Ready
/// again); held, the fill follows the finger, and a finger that slides off
/// the button lets go.
#[test]
fn clicks_and_taps_take_the_gesture_and_sliding_off_cancels_it() {
    for kind in [PointerKind::Mouse, PointerKind::Touch] {
        let mut r = ready();
        r.h.tap(kind, "Start");
        r.h.run(0.1);
        assert!(r.room.borrow().ready(), "{kind:?} once");
        r.h.tap(kind, "Start");
        r.h.run(0.3);
        assert!(!r.room.borrow().ready(), "{kind:?} twice");
        assert_eq!(r.seen.borrow().len(), 1, "the last tap readied nothing");
        let mut r = ready();
        let at = r.h.centre("Start");
        r.h.pointer(1, kind, PointerPhase::Down, at);
        r.h.run((CANCEL_HOLD * 0.5) as f32);
        assert!(fill(&r.h, "Start").is_some_and(|f| f > 0.3), "{kind:?}");
        r.h.run((CANCEL_HOLD * 0.5 + 0.1) as f32);
        assert!(!r.room.borrow().ready(), "{kind:?} held");
        r.h.pointer(1, kind, PointerPhase::Up, at);
        r.h.run(0.3);
        assert!(!r.room.borrow().ready(), "{kind:?}: its release readies nothing");
        // Slid off the button: no hold.
        let mut r = ready();
        let at = r.h.centre("Start");
        r.h.pointer(1, kind, PointerPhase::Down, at);
        r.h.run(0.2);
        for dy in [10.0, 30.0, 60.0, 120.0] {
            r.h.pointer(1, kind, PointerPhase::Move, [at[0], at[1] - dy]);
            r.h.frame(1.0 / 60.0);
        }
        r.h.run(1.0);
        assert!(r.room.borrow().ready(), "{kind:?} slid off");
        r.h.pointer(1, kind, PointerPhase::Up, [at[0], at[1] - 120.0]);
        r.h.run(0.3);
        assert!(r.room.borrow().ready());
        assert_eq!(fill(&r.h, "Start"), None);
    }
    // A tap on the card's background readies (the prompt was one big
    // button); while ready it does nothing.
    let mut r = rig_with(env(), false, false);
    let at = [40.0, 300.0];
    r.h.tap_at(PointerKind::Touch, at);
    r.h.run(0.2);
    assert!(r.room.borrow().ready());
    for _ in 0..3 {
        r.h.tap_at(PointerKind::Touch, at);
        r.h.run(0.1);
    }
    assert!(r.room.borrow().ready(), "background taps while ready");
}

/// Back while ready leaves the stage for good: it too takes the gesture
/// (Esc, Backspace, the Back button, a controller's Back, the browser's
/// Back, which is the menu's), its hint saying so; not ready, one press
/// leaves as ever.
#[test]
fn back_while_ready_takes_the_gesture_and_not_ready_leaves_at_once() {
    let mut r = rig_with(env(), false, false);
    escape(&mut r.h);
    assert_eq!(backs(&r), 1, "not ready: one press");

    let mut r = ready();
    escape(&mut r.h);
    r.h.run(0.1);
    assert_eq!(backs(&r), 0);
    assert!(shown(&r.h, "Room.LeaveHint") && !shown(&r.h, "Room.UnreadyHint"));
    assert!(has(&r.h, LEAVE_HINT));
    escape(&mut r.h);
    assert_eq!(backs(&r), 1, "twice in time");

    let mut r = ready();
    escape(&mut r.h);
    r.h.run((CANCEL_HINT + 0.1) as f32);
    assert!(shown(&r.h, "Room.UnreadyHint"), "back to Ready's hint");
    r.h.key("Backspace", "Backspace");
    r.h.run(0.2);
    r.h.key("Backspace", "Backspace");
    assert_eq!(backs(&r), 1, "Backspace twice");

    let mut r = ready();
    r.h.key_down("Escape", "Escape");
    r.h.run((CANCEL_HOLD * 0.5) as f32);
    assert!(fill(&r.h, "Back").is_some_and(|f| f > 0.3), "Back's own fill");
    assert_eq!(fill(&r.h, "Start"), None);
    r.h.run((CANCEL_HOLD * 0.5 - 0.1) as f32);
    assert_eq!(backs(&r), 0);
    r.h.run(0.2);
    assert_eq!(backs(&r), 1, "Esc held");
    r.h.key_up("Escape", "Escape");

    let mut r = ready();
    r.h.tap(PointerKind::Mouse, "Back");
    r.h.run(0.1);
    assert_eq!(backs(&r), 0, "the Back button once");
    r.h.tap(PointerKind::Mouse, "Back");
    assert_eq!(backs(&r), 1, "the Back button twice");

    let mut r = ready();
    r.h.controller(&Default::default(), "Pad", true, "button:1", true);
    r.h.run((CANCEL_HOLD + 0.1) as f32);
    assert_eq!(backs(&r), 1, "a controller's Back held");

    // The browser's Back is the menu's, pressed and let go at once.
    let mut r = ready();
    for _ in 0..2 {
        for down in [true, false] {
            r.h.send(UiEvent::Menu(crate::event::MenuEvent {
                action: MenuAction::Back,
                down,
                source: u32::MAX,
                gesture: false,
                time: 0.0,
            }));
        }
        assert!(backs(&r) <= 1);
        r.h.frame(0.2);
    }
    assert_eq!(backs(&r), 1, "the browser's Back twice");

    // Enter on the focused Back is Back's gesture.
    let mut r = ready();
    r.h.key("ArrowLeft", "ArrowLeft");
    assert_eq!(r.h.ui.focused(), Some("Back"));
    enter(&mut r.h);
    r.h.run(0.1);
    assert_eq!(backs(&r), 0);
    assert!(r.room.borrow().ready());
    enter(&mut r.h);
    assert_eq!(backs(&r), 1);
}

/// The window losing the focus lets every hold go (its release may never
/// come); changing the attack policy never takes Ready back.
#[test]
fn focus_loss_ends_a_hold_and_the_policy_row_leaves_ready_alone() {
    let mut r = ready();
    r.h.key_down("Enter", "Enter");
    r.h.run(0.3);
    r.h.send(UiEvent::Focus(false));
    r.h.run(1.0);
    assert!(r.room.borrow().ready(), "the hold ended with the focus");

    let mut r = rig_with(env(), false, true);
    enter(&mut r.h);
    assert!(r.room.borrow().ready());
    let before = r.room.borrow().my_targeting();
    // Up to the policy's row, then Left and Right, held and pressed.
    while r.h.ui.focused() != Some(super::TARGETING) {
        r.h.key("ArrowUp", "ArrowUp");
    }
    for key in ["ArrowRight", "ArrowRight", "ArrowLeft", "ArrowRight"] {
        r.h.key(key, key);
        r.h.run(0.1);
    }
    r.h.key_down("ArrowRight", "ArrowRight");
    r.h.run(1.0);
    r.h.key_up("ArrowRight", "ArrowRight");
    r.h.tap(PointerKind::Touch, &format!("{}.Up", super::TARGETING));
    r.h.run(0.2);
    assert_ne!(r.room.borrow().my_targeting(), before);
    assert!(r.room.borrow().ready(), "the policy changed, Ready kept");
}

/// Under reduced motion: no pulse and no running back; the fill in whole
/// halves; the hold done after the same time; the hint always shown while
/// ready.
#[test]
fn under_reduced_motion_the_hold_takes_as_long_and_the_fill_steps() {
    let e = env();
    e.reduced_motion.set(true);
    let mut r = rig_with(e, false, false);
    enter(&mut r.h);
    r.h.run(0.5);
    assert!(shown(&r.h, "Room.UnreadyHint"));
    r.h.key_down("Enter", "Enter");
    r.h.run(0.1);
    assert_eq!(fill(&r.h, "Start"), None, "under half: nothing yet");
    r.h.run((CANCEL_HOLD * 0.5) as f32);
    assert_eq!(fill(&r.h, "Start"), Some(0.5), "a whole half");
    assert_eq!(r.h.actor("Room.UnreadyHint").current().scale[0], 1.0, "no pulse");
    r.h.run((CANCEL_HOLD * 0.5 - 0.2) as f32);
    assert!(r.room.borrow().ready(), "not before the hold's time");
    r.h.run(0.2);
    assert!(!r.room.borrow().ready());
    r.h.key_up("Enter", "Enter");
    // Let go early: the fill goes at once.
    enter(&mut r.h);
    r.h.run(0.2);
    r.h.key_down("Enter", "Enter");
    r.h.run((CANCEL_HOLD * 0.6) as f32);
    assert_eq!(fill(&r.h, "Start"), Some(0.5));
    r.h.key_up("Enter", "Enter");
    r.h.frame(1.0 / 60.0);
    assert_eq!(fill(&r.h, "Start"), None);
    assert!(r.room.borrow().ready());
}

/// Everyone ready (alone with the CPUs, the song loaded): the room counts
/// the start down. Round P1b (the maintainer's of 2026-11-16): in CPU
/// practice Cancel ready, done by the gesture, cancels the countdown and
/// the player is not ready again in a window opened again (it was refused
/// before, so that it could never work there); readying again counts down
/// again. Back, done by the gesture, leaves as it always did; one press
/// of either does nothing.
#[test]
fn during_practices_countdown_cancel_ready_cancels_it_and_back_leaves() {
    let mut r = rig_with(env(), true, false);
    enter(&mut r.h);
    r.h.run(0.2);
    assert_eq!(phase(&r), ddi_room::PhaseKind::Countdown);
    assert!(has(&r.h, "Cancel ready"), "{:?}", r.h.texts());
    assert!(shown(&r.h, "Room.UnreadyHint"));
    enter(&mut r.h);
    r.h.run(0.1);
    assert_eq!(phase(&r), ddi_room::PhaseKind::Countdown, "one press: nothing");
    enter(&mut r.h);
    r.h.run(0.1);
    assert!(r.room.borrow().refusal().is_none(), "taken");
    assert_eq!(phase(&r), ddi_room::PhaseKind::Preparing);
    assert!(!r.room.borrow().ready());
    assert!(has(&r.h, "Ready"), "{:?}", r.h.texts());
    assert!(!shown(&r.h, "Room.UnreadyHint"));
    // Ready again: the countdown again, and the play starts at its end.
    r.h.run((CANCEL_DOUBLE_TAP + 0.1) as f32);
    enter(&mut r.h);
    r.h.run(0.2);
    assert_eq!(phase(&r), ddi_room::PhaseKind::Countdown);
    escape(&mut r.h);
    r.h.run(0.1);
    assert_eq!(backs(&r), 0, "one Back: still here");
    escape(&mut r.h);
    assert_eq!(backs(&r), 1, "Back twice leaves");
}

/// A practice room's first stage started by the room (Ready alone with
/// the CPUs, the song loaded, the countdown run), its prompt then on the
/// shell's loading screen, as the play not ready yet puts it.
fn loading(e: Env) -> Rig {
    let room = RoomClient::shared(0, 1);
    let s = song();
    room.borrow_mut().pick(&s, &s.charts[0]).unwrap();
    let order = room.borrow_mut().take_stage_order().unwrap();
    let seen: Seen = Rc::default();
    let sink = seen.clone();
    let shared = room.clone();
    let shell: RoomShell = Rc::new(move |call, _cx| {
        if let RoomCall::Prepare(job) = call {
            shared.borrow_mut().finish_load(&job, Ok(()));
        }
    });
    let state = Rc::new(RefCell::new(PromptState::default()));
    state.borrow_mut().info.title = "Single Song".into();
    let step = state.clone();
    let mut h = harness();
    h.push(
        PromptScreen::new(
            e,
            state,
            Rc::new(move |c, cx| {
                // The shell's Start: the loading screen, the play not ready.
                if c == PromptChoice::Start {
                    step.borrow_mut().set_step(crate::screens::prompt::PromptStep::Loading);
                }
                sink.borrow_mut().push((c, cx.in_gesture()));
            }),
        )
        .with_room(room.clone(), order.stage, shell),
    );
    h.run(0.5);
    enter(&mut h);
    h.run(3.5);
    assert!(room.borrow().stage_started(order.stage));
    assert_eq!(seen.borrow().last().map(|s| &s.0), Some(&PromptChoice::Start));
    h.run(0.5);
    assert!(has(&h, "Loading"), "{:?}", h.texts());
    // The card gone once its fade's time is over (it stayed before round
    // P2: nothing built the loading screen again), its hints with it.
    assert!(!shown(&h, "Start") && !shown(&h, "Room.UnreadyHint"), "{:?}", h.texts());
    Rig { h, room, seen }
}

/// The loading screen after the room started the stage (feedback round
/// P2): Back there leaves a stage that has begun, so it too takes the
/// deliberate gesture, held or pressed twice, by every input, its hint
/// saying so after one press; a late second press is a first again.
#[test]
fn back_on_the_loading_screen_of_a_started_stage_takes_the_gesture() {
    let mut r = loading(env());
    assert!(!shown(&r.h, "Room.LeaveHint"), "no hint before a press");
    escape(&mut r.h);
    r.h.run(0.1);
    assert_eq!(backs(&r), 0, "one Esc: still loading");
    assert!(shown(&r.h, "Room.LeaveHint") && has(&r.h, LEAVE_HINT), "{:?}", r.h.texts());
    r.h.run((CANCEL_HINT + 0.1) as f32);
    assert!(!shown(&r.h, "Room.LeaveHint"), "the hint goes after its time");
    escape(&mut r.h);
    r.h.run((CANCEL_DOUBLE_TAP + 0.1) as f32);
    escape(&mut r.h);
    r.h.run(0.1);
    assert_eq!(backs(&r), 0, "too late: a first press again");
    r.h.key("Backspace", "Backspace");
    assert_eq!(backs(&r), 1, "twice in time");

    let mut r = loading(env());
    r.h.key_down("Escape", "Escape");
    r.h.run((CANCEL_HOLD - 0.1) as f32);
    assert_eq!(backs(&r), 0, "not before the hold's time");
    assert!(shown(&r.h, "Room.LeaveHint"), "the hint while held");
    r.h.run(0.2);
    assert_eq!(backs(&r), 1, "Esc held");
    r.h.key_up("Escape", "Escape");
    r.h.run(0.5);
    assert_eq!(backs(&r), 1, "once");

    let mut r = loading(env());
    r.h.key_down("Escape", "Escape");
    r.h.run(0.3);
    r.h.key_up("Escape", "Escape");
    r.h.run(1.0);
    assert_eq!(backs(&r), 0, "let go early");

    let mut r = loading(env());
    r.h.controller(&Default::default(), "Pad", true, "button:1", true);
    r.h.run((CANCEL_HOLD + 0.1) as f32);
    assert_eq!(backs(&r), 1, "a controller's Back held");

    // The browser's Back, the menu's pressed and let go at once, twice.
    let mut r = loading(env());
    for _ in 0..2 {
        for down in [true, false] {
            r.h.send(UiEvent::Menu(crate::event::MenuEvent {
                action: MenuAction::Back,
                down,
                source: u32::MAX,
                gesture: false,
                time: 0.0,
            }));
        }
        r.h.frame(0.2);
    }
    assert_eq!(backs(&r), 1, "the browser's Back twice");

    // Taps and Enter leave nothing.
    let mut r = loading(env());
    for _ in 0..2 {
        r.h.tap_at(PointerKind::Touch, [40.0, 300.0]);
        enter(&mut r.h);
        r.h.run(0.1);
    }
    assert_eq!(backs(&r), 0);
}
