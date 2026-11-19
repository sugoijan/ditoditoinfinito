//! The multiplayer prototype's screens in the headless harness, bound to
//! the practice authority (`state::room`), as the maintainer's match flow
//! has them (M7b): the gated start menu, the Rules pop-up first (Reset and
//! OK), song select as the room (the host's pick starting the match, a
//! chart the CPUs cannot play refused), the CPU roster (Add random, Add
//! custom by level and by name, the cap), the stage's prompt with Ready
//! and its players, the standings after a stage and the match's results,
//! and an online room's host and follower.

use std::cell::RefCell;
use std::rc::Rc;

use super::data::{ChartEntry, Difficulty, SelectData, SongEntry};
use super::prompt::{PromptChoice, PromptScreen, PromptState};
use super::room::{RoomCall, RoomShell};
use super::start::{Choice, StartScreen};
use super::test_kit::{Orders, Uploads, env, harness, shell, shell_with, there, top};
use super::{DESIGN, Env, sample};
use crate::state::room::{RoomClient, SharedRoom, StageOrder};
use crate::testing::Harness;
use crate::{PointerKind, TextMode};
use ddi_room::Phase;

fn chart(style: &str, index: usize, meter: u32) -> ChartEntry {
    ChartEntry {
        style: style.into(),
        difficulty: Difficulty::Hard,
        meter,
        notes: 300,
        index,
        digest: Some(ddi_chart::ChartDigest(0xabc0 + index as u128)),
        ..ChartEntry::default()
    }
}

/// A library of two songs: one with a Single chart, one with a Double
/// chart alone; song select shows `style` first.
fn library(style: &str) -> SelectData {
    let mut data = SelectData {
        styles: sample::styles(),
        songs: vec![
            SongEntry {
                id: "single".into(),
                title: "Single Song".into(),
                artist: "Artist".into(),
                bpm_text: "150".into(),
                length: 120.0,
                charts: vec![chart("dance-single", 0, 9)],
                ..SongEntry::default()
            },
            SongEntry {
                id: "double".into(),
                title: "Double Song".into(),
                artist: "Artist".into(),
                bpm_text: "170".into(),
                length: 100.0,
                charts: vec![chart("dance-double", 0, 11)],
                ..SongEntry::default()
            },
        ],
        style: Some(style.into()),
        ..SelectData::default()
    };
    if style == "dance-double" {
        data.songs.reverse();
    }
    data
}

/// The room's song select over [`library`] in `style`.
fn room_select(
    env: &Env,
    room: SharedRoom,
    style: &str,
    orders: &Orders,
) -> super::select::SelectScreen {
    sample::room_select(env, room.clone(), library(style))
        .with_room(room.clone(), shell(&room, orders))
}


fn view(env: &Env) -> super::room::RoomView {
    env.state
        .borrow()
        .room
        .clone()
        .expect("the room said what it shows")
}

/// Taps `name`, as a mouse would.
fn press(h: &mut Harness, name: &str) {
    if top(h) == "room-rules" {
        reveal(h, name);
    }
    h.tap(PointerKind::Mouse, name);
    h.run(0.5);
}

/// The Players pop-up, by L (a wide screen has no button for it since
/// follow-up round F: the panel beside the song, L and More open it).
fn open_players(h: &mut Harness) {
    h.key("KeyL", "l");
    h.run(0.5);
}

/// Scrolls the Rules pop-up's rows by the wheel until `name` is shown
/// whole (a row of the pop-up scrolled away).
///
/// Fails where there is no such row or it never comes into view whole
/// (review tests-8: it returned as if done, and the test went on to
/// press whatever was there).
fn reveal(h: &mut Harness, name: &str) {
    assert!(
        h.ui.bounds(name).is_some(),
        "no {name} to reveal (renamed, or not in this pop-up): {:?}",
        h.ui.layers()
    );
    let whole = |h: &Harness| match (h.ui.bounds(name), h.ui.visible_bounds(name)) {
        (Some(b), Some(v)) => b.iter().zip(v).all(|(a, b)| (a - b).abs() < 0.5),
        _ => false,
    };
    for down in [true, false] {
        for _ in 0..200 {
            if whole(h) {
                return;
            }
            let at = h.centre("Title");
            let at = [at[0], at[1] + 80.0];
            h.wheel(at, [0.0, if down { 40.0 } else { -40.0 }], crate::event::WheelSource::Pixels);
            h.run(0.05);
        }
    }
    panic!("{name} never came into view whole");
}

fn has(h: &Harness, text: &str) -> bool {
    h.texts().iter().any(|t| t == text)
}

/// Multiplayer is no longer behind a switch (2026-11-14): the start menu
/// opens its home by default; turned off (only a test does) it says
/// Coming soon as before.
#[test]
fn the_start_menu_opens_multiplayer_by_default() {
    for on in [false, true] {
        let calls = Rc::new(RefCell::new(Vec::new()));
        let sink = calls.clone();
        let mut h = harness();
        let start =
            StartScreen::new(env(), false, Rc::new(move |c, _| sink.borrow_mut().push(c)));
        h.push(if on { start } else { start.with_multiplayer(false) });
        h.run(0.3);
        h.key("a", "a");
        h.key("ArrowDown", "ArrowDown");
        assert_eq!(h.ui.focused(), Some("Multiplayer"));
        h.key("Enter", "Enter");
        h.tap(PointerKind::Touch, "Multiplayer");
        let texts = h.texts();
        if on {
            assert_eq!(*calls.borrow(), [Choice::Multiplayer, Choice::Multiplayer]);
            assert!(texts.iter().any(|t| t == "Multiplayer"), "{texts:?}");
            assert!(!texts.iter().any(|t| t.contains("Coming soon")));
        } else {
            assert!(calls.borrow().is_empty());
            assert!(texts.iter().any(|t| t.contains("Coming soon")), "{texts:?}");
        }
    }
}

/// The multiplayer home's order (the maintainer, 2026-11-14: "CPU
/// practice could be the last option"): Find match and Browse rooms
/// (Coming soon), Private room, Join by link, CPU practice; the first key
/// focuses Private room, the first that opens something.
#[test]
fn the_home_lists_cpu_practice_last_and_the_first_key_focuses_private_room() {
    let env = env();
    let mut h = harness();
    h.push(sample::start_screen(&env, false));
    h.run(0.3);
    h.key("a", "a");
    h.key("ArrowDown", "ArrowDown");
    h.key("Enter", "Enter");
    h.run(0.5);
    assert_eq!(top(&h), "multiplayer");
    let ys: Vec<f32> = ["find", "browse", "private", "join", "practice"]
        .iter()
        .map(|id| h.centre(&format!("Choice.{id}"))[1])
        .collect();
    assert!(ys.windows(2).all(|w| w[0] < w[1]), "{ys:?}");
    assert_eq!(h.ui.focused(), None, "no focus until a key");
    h.key("ArrowDown", "ArrowDown");
    assert_eq!(h.ui.focused(), Some("Choice.private"));
    h.key("ArrowDown", "ArrowDown");
    h.key("ArrowDown", "ArrowDown");
    assert_eq!(h.ui.focused(), Some("Choice.practice"));
}

/// CPU practice opens straight on the room's song select, with the
/// default rules and roster and no pop-up (the maintainer, 2026-11-12);
/// the Rules are behind their button, in their one form there (Cancel and
/// Apply), and stay closed when the player leaves the room and comes back.
#[test]
fn practice_opens_straight_on_song_select_with_the_rules_behind_their_button() {
    let env = env();
    let mut h = harness();
    h.push(sample::start_screen(&env, false).with_multiplayer(true));
    h.run(0.3);
    h.key("a", "a");
    h.key("ArrowDown", "ArrowDown");
    h.key("Enter", "Enter");
    h.run(0.5);
    assert_eq!(top(&h), "multiplayer");
    press(&mut h, "Choice.practice");
    assert_eq!(top(&h), "select");
    let v = view(&env);
    assert!(!v.rules_open, "{v:?}");
    assert_eq!((v.view.as_str(), v.phase.as_str()), ("select", "lobby"));
    assert!(v.host);
    // The defaults: four stages, three CPUs, on the players panel.
    assert_eq!((v.stage_count, v.cpus), (4, 3));
    assert_eq!(said(&h, "Players.Count"), "4 / 73");
    assert_eq!(v.status, "Pick a song to start the match.");
    assert!(
        has(&h, "Pick a song to start the match."),
        "{:?}",
        h.texts()
    );
    assert!(has(&h, "CPU PRACTICE") || h.texts().iter().any(|t| t.contains("PRACTICE")));
    // Nobody is "Not ready" before a pick gathers readiness.
    assert!(!has(&h, "Not ready"), "{:?}", h.texts());
    // The Rules from their button: Cancel and Apply; Back closes them.
    press(&mut h, "Room.Rules");
    assert_eq!(top(&h), "room-rules");
    let v = view(&env);
    assert!(v.rules_open && !v.rules_first, "{v:?}");
    assert!(has(&h, "Cancel") && has(&h, "Apply"), "{:?}", h.texts());
    assert!(!has(&h, "Reset"));
    h.key("Escape", "Escape");
    h.run(0.5);
    assert_eq!(top(&h), "select");
    // Back to the home; practice again: song select, no pop-up.
    h.key("Escape", "Escape");
    h.run(0.5);
    assert_eq!(top(&h), "multiplayer");
    press(&mut h, "Choice.practice");
    assert_eq!(top(&h), "select");
    h.run(0.5);
    assert_eq!(top(&h), "select");
}

/// The hidden option (`Settings::multiplayer_rules_first`, off by
/// default): entering opens the Rules pop-up first, as M7b had it (Reset
/// and OK); OK leaves it closed until the player leaves the room and comes
/// back.
#[test]
fn the_hidden_option_opens_the_rules_first_on_entering() {
    let mut settings = crate::state::settings::Settings::load();
    assert!(!settings.multiplayer_rules_first, "off by default");
    settings.multiplayer_rules_first = true;
    settings.save();
    let env = env();
    let mut h = harness();
    h.push(sample::start_screen(&env, false).with_multiplayer(true));
    h.run(0.3);
    h.key("a", "a");
    h.key("ArrowDown", "ArrowDown");
    h.key("Enter", "Enter");
    h.run(0.5);
    assert_eq!(top(&h), "multiplayer");
    press(&mut h, "Choice.practice");
    assert_eq!(top(&h), "room-rules");
    let v = view(&env);
    assert!(v.rules_open && v.rules_first, "{v:?}");
    assert!(has(&h, "Reset") && has(&h, "OK"), "{:?}", h.texts());
    assert!(!has(&h, "Apply"));
    press(&mut h, "Ok");
    assert_eq!(top(&h), "select");
    assert!(!view(&env).rules_open);
    // Not again while the player stays.
    h.run(0.5);
    assert_eq!(top(&h), "select");
    // Back to the home; practice again: the Rules first again.
    h.key("Escape", "Escape");
    h.run(0.5);
    assert_eq!(top(&h), "multiplayer");
    press(&mut h, "Choice.practice");
    assert_eq!(top(&h), "room-rules");
    // Back on the first pop-up applies nothing and closes it.
    h.key("Escape", "Escape");
    h.run(0.5);
    assert_eq!(top(&h), "select");
}

/// The host's Start on a song is the pick. The room's song select offers
/// only the styles its CPUs can play (feedback round B): a library kept
/// on Double opens on Single, a song without a Single chart is grayed and
/// Start on it picks nothing; a chart the CPUs cannot play is refused in a
/// pop-up wherever it is picked all the same; a playable one starts the
/// match and the stage's prompt opens.
#[test]
fn the_hosts_start_picks_the_song_and_starts_the_match() {
    let env = env();
    let room = RoomClient::shared(0, 1);
    room.borrow_mut().set_rules_seen(true);
    let orders = Orders::default();
    let mut h = harness();
    h.push(room_select(&env, room.clone(), "dance-double", &orders));
    h.run(0.5);
    assert_eq!(top(&h), "select");
    let texts = h.texts();
    assert!(
        texts.iter().any(|t| t == "No Single charts for this song."),
        "{texts:?}"
    );
    assert!(!texts.iter().any(|t| t.contains("Double ·")), "{texts:?}");
    h.key("Enter", "Enter");
    h.run(0.3);
    assert_eq!(top(&h), "select", "nothing to pick on a grayed song");
    assert_eq!(view(&env).phase, "lobby", "nothing started");
    // Picked all the same (an older build's song select, a direct call).
    let double = library("dance-double").songs[0].clone();
    let why = room
        .borrow_mut()
        .pick(&double, &double.charts[0])
        .expect_err("the CPUs cannot play Double");
    h.push(super::room_dialogs::NoticeDialog::refusal(
        env.clone(),
        &why,
    ));
    room.borrow_mut().clear_refusal();
    h.run(0.3);
    assert_eq!(top(&h), "room-notice");
    assert!(
        h.texts()
            .iter()
            .any(|t| t.contains("one-player charts with four lanes")),
        "{:?}",
        h.texts()
    );
    h.key("Enter", "Enter");
    h.run(0.3);
    assert_eq!(view(&env).phase, "lobby", "nothing started");
    let mut h = harness();
    h.push(room_select(&env, room.clone(), "dance-single", &orders));
    h.run(0.5);
    h.key("Enter", "Enter");
    h.run(0.5);
    let v = view(&env);
    assert_eq!(v.phase, "preparing", "{v:?}");
    assert!(v.locked);
    assert_eq!(v.stages[0].song.as_deref(), Some("single"));
    assert!(!v.ready, "picking never readies");
    assert_eq!(orders.borrow().len(), 1, "the stage's prompt opened");
    assert_eq!(orders.borrow()[0].song.id, "single");
    assert_eq!(orders.borrow()[0].cpus.len(), 3);
    assert!(
        v.picks[0].starts_with("Stage 1: Single Song"),
        "{:?}",
        v.picks
    );
}

/// A practice room's roster is remembered (the maintainer, 2026-11-13):
/// the CPUs a Rules pop-up applied, by name, level, personality and
/// policy in order, are the next practice room's; Reset still gives the
/// default; the stage count is not kept; a roster this build cannot read
/// is the default.
#[test]
fn a_practice_rooms_roster_is_what_the_next_one_starts_with() {
    let env = env();
    let names = |room: &SharedRoom| -> Vec<(Option<String>, u8, Option<String>)> {
        room.borrow()
            .state()
            .next
            .rules
            .cpu
            .roster
            .iter()
            .map(|s| (s.name.clone(), s.level, s.personality.clone()))
            .collect()
    };
    let room = RoomClient::shared(0, 1);
    let default = names(&room);
    assert_eq!(default.len(), 3);
    room.borrow_mut().set_rules_seen(true);
    let orders = Orders::default();
    let mut h = harness();
    h.push(room_select(&env, room.clone(), "dance-single", &orders));
    h.run(0.5);
    press(&mut h, "Room.Rules");
    press(&mut h, "AddRandom");
    press(&mut h, "AddRandom");
    press(&mut h, "Remove.0");
    press(&mut h, "Dec.stages");
    press(&mut h, "Apply");
    assert_eq!(top(&h), "select");
    let kept = names(&room);
    assert_eq!(kept.len(), 4);
    assert_ne!(kept, default);
    assert_eq!(room.borrow().state().next.rules.stages, 3);
    // The next practice room: the same CPUs in the same order, the
    // stages as ever, each CPU's seed its own room's.
    let next = RoomClient::shared(0, 2);
    assert_eq!(names(&next), kept);
    assert_eq!(next.borrow().state().next.rules.stages, 4);
    assert_ne!(
        next.borrow().state().next.rules.cpu.roster[0].seed,
        room.borrow().state().next.rules.cpu.roster[0].seed
    );
    // Reset gives the default roster, and that is remembered in turn.
    let mut h = harness();
    next.borrow_mut().set_rules_seen(true);
    h.push(room_select(&env, next.clone(), "dance-single", &orders));
    h.run(0.5);
    let mut settings = crate::state::settings::Settings::load();
    settings.multiplayer_rules_first = true;
    settings.save();
    next.borrow_mut().set_rules_seen(false);
    h.run(0.5);
    assert_eq!(top(&h), "room-rules");
    press(&mut h, "Reset");
    press(&mut h, "Ok");
    let reset = names(&next);
    assert_eq!(reset.len(), 3);
    assert_eq!(names(&RoomClient::shared(0, 3)), reset);
    // A roster this build cannot read (a later shape, a level it lacks):
    // the default, the rest of the settings untouched.
    for odd in [
        serde_json::json!({ "v": 2, "cpus": [] }),
        serde_json::json!({ "v": 1, "cpus": [{ "name": "X", "level": 40 }] }),
        serde_json::json!("not a roster"),
    ] {
        let mut settings = crate::state::settings::Settings::load();
        settings.practice_roster = Some(odd);
        settings.save();
        let fresh = RoomClient::shared(0, 1);
        assert_eq!(names(&fresh), default);
        assert!(crate::state::settings::Settings::load().multiplayer_rules_first);
    }
    // Not online rooms, nor a room made without the shells' constructor.
    assert!(!RoomClient::new(0, 1).remembers_roster());
}

/// The Rules pop-up: Reset restores the defaults and stays; the roster
/// takes CPUs at random, by level (several at once) and by name, each
/// removable, up to the cap; Apply saves them; a running match's rules
/// are locked and shown.
#[test]
fn the_rules_pop_up_edits_the_roster_within_the_cap() {
    let env = env();
    let room = RoomClient::shared(0, 1);
    room.borrow_mut().set_rules_seen(true);
    let orders = Orders::default();
    let mut h = harness();
    h.push(room_select(&env, room.clone(), "dance-single", &orders));
    h.run(0.5);
    press(&mut h, "Room.Rules");
    assert_eq!(top(&h), "room-rules");
    assert!(has(&h, "Cancel") && has(&h, "Apply"));
    assert_eq!(view(&env).draft, vec![4, 3]);
    let first: String = room.borrow().rules().cpu.roster[0].name.clone().unwrap();
    assert!(has(&h, &first), "{first}: {:?}", h.texts());
    // The roster's levels are stars, never a number: each row's, the
    // level its CPU has.
    use crate::stars::{fills, shown};
    let seated = room.borrow().rules().cpu.roster.clone();
    for (k, spec) in seated.iter().enumerate() {
        assert_eq!(
            shown(h.actor(&format!("Cpu{k}.Stars"))),
            fills(spec.level + 1),
            "row {k}"
        );
    }
    press(&mut h, "AddRandom");
    assert_eq!(view(&env).draft, vec![4, 4]);
    press(&mut h, "Remove.0");
    assert_eq!(view(&env).draft, vec![4, 3]);
    // Gone from the draft's roster; the players panel under the pop-up
    // shows the room's saved rules until Apply.
    let named = h.texts().iter().filter(|t| **t == first).count();
    assert_eq!(named, 1, "{:?}", h.texts());
    let no_numbers = |h: &Harness| {
        h.texts()
            .iter()
            // ("2 of 10"; the Charts rule's sentence has words "of" too)
            .all(|t| {
                !(t.contains(" of ") && t.chars().any(|c| c.is_ascii_digit()))
                    && !t.contains("Lv")
                    && !t.contains("Level ")
            })
    };
    assert!(no_numbers(&h), "{:?}", h.texts());
    // Add custom: two at level 10, then one by name. The level starts at
    // the engine's default (level 8: three gold, two silver) and steps to
    // the last (five gold); its row says "Level" and shows the stars.
    press(&mut h, "AddCustom");
    assert_eq!(top(&h), "room-add-cpus");
    assert!(has(&h, "Level"));
    assert_eq!(shown(h.actor("Add.level.Stars")), fills(8));
    for _ in 0..3 {
        press(&mut h, "Inc.level");
    }
    assert_eq!(shown(h.actor("Add.level.Stars")), fills(10));
    for _ in 0..12 {
        press(&mut h, "Dec.level");
    }
    assert_eq!(shown(h.actor("Add.level.Stars")), fills(1), "the weakest");
    for _ in 0..9 {
        press(&mut h, "Inc.level");
    }
    press(&mut h, "Inc.count");
    assert!(has(&h, "Add 2 CPUs"), "{:?}", h.texts());
    assert!(no_numbers(&h), "{:?}", h.texts());
    // The CPUs listed by name show theirs.
    let named = crate::state::cpu_roster::unseated(&room.borrow().rules().cpu.roster);
    assert_eq!(shown(h.actor("Name.0.Stars")), fills(named[0].level));
    press(&mut h, "Add.ByLevel");
    assert_eq!(view(&env).draft, vec![4, 5]);
    press(&mut h, "Name.0");
    assert_eq!(view(&env).draft, vec![4, 6]);
    press(&mut h, "Close");
    assert_eq!(top(&h), "room-rules");
    // Up to the cap: 72 CPUs beside the player, then nothing more.
    for _ in 0..66 {
        press(&mut h, "AddRandom");
    }
    assert_eq!(view(&env).draft, vec![4, 72]);
    press(&mut h, "AddRandom");
    assert_eq!(view(&env).draft, vec![4, 72], "the cap holds");
    // Back to eight for the rest.
    for k in (8..72).rev() {
        press(&mut h, &format!("Remove.{k}"));
    }
    assert_eq!(view(&env).draft, vec![4, 8]);
    press(&mut h, "Dec.stages");
    press(&mut h, "Dec.stages");
    // The match's rules (D0): DDI, with no row of its own; its
    // difficulty (Normal first) and Boos (Off first), and Attacks, On
    // first in practice (the maintainer, 2026-11-12).
    assert!(!there(&h, "Dec.ruleset") && !there(&h, "Rule.ruleset"));
    assert!(
        has(&h, "Everyone plays the match under DDI with these rules, whatever their own options say."),
        "{:?}",
        h.texts()
    );
    let draft = |env: &Env| view(env).draft_rules.expect("the draft");
    assert_eq!(draft(&env), ("normal".to_string(), false, true));
    assert_eq!(said(&h, "Rule.difficulty.Value"), "Normal");
    press(&mut h, "Inc.difficulty");
    assert_eq!(said(&h, "Rule.difficulty.Value"), "Hard");
    press(&mut h, "Inc.difficulty");
    assert_eq!(draft(&env).0, "hard", "stops at the last");
    for _ in 0..3 {
        press(&mut h, "Dec.difficulty");
    }
    assert_eq!(draft(&env).0, "easy", "stops at the first");
    assert_eq!(said(&h, "Rule.difficulty.Value"), "Easy");
    press(&mut h, "Inc.difficulty");
    press(&mut h, "Inc.difficulty");
    assert_eq!(said(&h, "Rule.boos.Value"), "Off");
    press(&mut h, "Inc.boos");
    assert_eq!(said(&h, "Rule.boos.Value"), "On");
    press(&mut h, "Inc.boos");
    assert!(draft(&env).1, "stays on");
    press(&mut h, "Dec.boos");
    assert!(!draft(&env).1);
    press(&mut h, "Inc.boos");
    assert_eq!(said(&h, "Rule.attacks.Value"), "On");
    press(&mut h, "Dec.attacks");
    assert_eq!(said(&h, "Rule.attacks.Value"), "Off");
    assert_eq!(draft(&env), ("hard".to_string(), true, false));
    press(&mut h, "Inc.attacks");
    assert_eq!(said(&h, "Rule.attacks.Value"), "On");
    assert_eq!(draft(&env), ("hard".to_string(), true, true));
    press(&mut h, "Apply");
    assert_eq!(top(&h), "select");
    let v = view(&env);
    assert_eq!((v.stage_count, v.cpus), (2, 8));
    assert_eq!(v.ruleset, "ddi");
    assert_eq!((v.difficulty.as_str(), v.boos, v.attacks), ("hard", true, true));
    assert!(v.attacks_offered);
    {
        let r = room.borrow();
        let rules = r.rules();
        assert_eq!(rules.difficulty, ddi_room::MatchDifficulty::Hard);
        assert!(rules.boos && rules.attacks());
        assert_eq!(rules.battle, "gauge-attacks-v0");
    }
    let at_ten = room
        .borrow()
        .rules()
        .cpu
        .roster
        .iter()
        .filter(|c| c.level == 9)
        .count();
    assert!(at_ten >= 2, "{at_ten}");
    // Reset (the first form, which the hidden option opens on entering):
    // the defaults, the pop-up still open.
    press(&mut h, "Room.Rules");
    press(&mut h, "Cancel");
    assert_eq!(top(&h), "select");
    let mut settings = crate::state::settings::Settings::load();
    settings.multiplayer_rules_first = true;
    settings.save();
    room.borrow_mut().set_rules_seen(false);
    h.run(0.5);
    assert_eq!(top(&h), "room-rules");
    press(&mut h, "Reset");
    assert_eq!(top(&h), "room-rules");
    assert_eq!(view(&env).draft, vec![4, 3]);
    press(&mut h, "Ok");
    assert_eq!(view(&env).stage_count, 4);
    // Reset put the match's rules back too: Normal, no Boos, attacks On.
    let v = view(&env);
    assert_eq!((v.difficulty.as_str(), v.boos, v.attacks), ("normal", false, true));
    // A running match: its rules locked, shown in a line that names what
    // differs from the usual.
    {
        let mut rules = room.borrow().rules().clone();
        rules.difficulty = ddi_room::MatchDifficulty::Easy;
        rules.boos = true;
        rules.set_attacks(false);
        room.borrow_mut().save_rules(rules).unwrap();
    }
    let s = &library("dance-single").songs[0];
    room.borrow_mut().pick(s, &s.charts[0]).unwrap();
    h.run(0.5);
    assert_eq!(view(&env).phase, "preparing");
    press(&mut h, "Room.Rules");
    assert!(has(&h, "This match is on stage 1 of 4, under these rules:"), "{:?}", h.texts());
    assert!(
        h.texts()
            .iter()
            .any(|t| t == "4 stages · 3 CPUs · DDI · Easy timing · Boos · No attacks"),
        "{:?}",
        h.texts()
    );
}

/// The Points row (2026-11-13): how strongly a field of nine and more
/// favors its top places, Strong first, each choice with its sentence;
/// applied, it is the match's and the summary names it; Reset gives
/// Strong.
#[test]
fn the_points_row_chooses_the_bias_where_the_room_offers_it() {
    let env = env();
    let room = RoomClient::shared(0, 1);
    room.borrow_mut().set_rules_seen(true);
    let orders = Orders::default();
    let mut h = harness();
    h.push(room_select(&env, room.clone(), "dance-single", &orders));
    h.run(0.5);
    press(&mut h, "Room.Rules");
    assert_eq!(said(&h, "Rule.points.Value"), "Strong");
    assert_eq!(view(&env).draft_points.as_deref(), Some("strong"));
    assert!(has(&h, "The top places earn the most; lower places in a large field earn smaller awards."));
    assert!(
        h.texts().iter().any(|t| t.starts_with("The Points choice changes nothing")),
        "{:?}",
        h.texts()
    );
    press(&mut h, "Inc.points");
    assert_eq!(said(&h, "Rule.points.Value"), "Medium");
    press(&mut h, "Inc.points");
    press(&mut h, "Inc.points");
    assert_eq!(said(&h, "Rule.points.Value"), "Mild", "stops at the last");
    assert!(has(&h, "Points reach furthest down a large field; the top places lead by less."));
    press(&mut h, "Dec.points");
    press(&mut h, "Apply");
    assert_eq!(room.borrow().rules().points, ddi_room::PointsBias::Medium);
    let words = super::room::rules_words(
        &crate::state::room::StageRules::of(room.borrow().rules()),
        None,
    );
    assert_eq!(String::from(words), "DDI · Medium points");
    // Reset's defaults are Strong.
    let mut settings = crate::state::settings::Settings::load();
    settings.multiplayer_rules_first = true;
    settings.save();
    room.borrow_mut().set_rules_seen(false);
    h.run(0.5);
    press(&mut h, "Reset");
    assert_eq!(view(&env).draft_points.as_deref(), Some("strong"));
    press(&mut h, "Ok");
    assert_eq!(room.borrow().rules().points, ddi_room::PointsBias::Strong);
    settings.multiplayer_rules_first = false;
    settings.save();
}

/// The Charts row: Adaptive by default since 2026-11-14 (a new practice
/// room's, Reset's), each choice with its sentence; As picked applied is
/// the match's and the summary names it (Adaptive, the usual, is never
/// named); a room whose authority cannot honor Adaptive shows the row
/// fixed at As picked and its summary says nothing of it.
#[test]
fn the_charts_row_chooses_adaptive_where_the_room_offers_it() {
    let env = env();
    let room = RoomClient::shared(0, 1);
    assert_eq!(room.borrow().rules().charts, ddi_room::Charts::Adaptive);
    room.borrow_mut().set_rules_seen(true);
    let orders = Orders::default();
    let mut h = harness();
    h.push(room_select(&env, room.clone(), "dance-single", &orders));
    h.run(0.5);
    press(&mut h, "Room.Rules");
    assert_eq!(said(&h, "Rule.charts.Value"), "Adaptive");
    assert_eq!(view(&env).draft_charts.as_deref(), Some("adaptive"));
    assert!(
        h.texts().iter().any(|t| t.starts_with("Each player's chart changes with their skill.")),
        "{:?}",
        h.texts()
    );
    press(&mut h, "Dec.charts");
    assert_eq!(said(&h, "Rule.charts.Value"), "As picked");
    assert_eq!(view(&env).draft_charts.as_deref(), Some("as_picked"));
    assert!(
        h.texts().iter().any(|t| t.starts_with("Everyone plays the chart picked")),
        "{:?}",
        h.texts()
    );
    press(&mut h, "Apply");
    assert_eq!(room.borrow().rules().charts, ddi_room::Charts::AsPicked);
    let words = |r: &ddi_room::MatchRules| {
        String::from(super::room::rules_words(
            &crate::state::room::StageRules::of(r),
            None,
        ))
    };
    assert_eq!(words(room.borrow().rules()), "DDI · Charts as picked");
    let mut adaptive = room.borrow().rules().clone();
    adaptive.charts = ddi_room::Charts::Adaptive;
    assert_eq!(words(&adaptive), "DDI");
    // Where Adaptive is not offered, As picked is no choice: unsaid.
    assert_eq!(
        String::from(super::room::rules_words_in(
            &crate::state::room::StageRules::of(room.borrow().rules()),
            None,
            false,
        )),
        "DDI"
    );
    // Reset's default is Adaptive.
    let mut settings = crate::state::settings::Settings::load();
    settings.multiplayer_rules_first = true;
    settings.save();
    room.borrow_mut().set_rules_seen(false);
    h.run(0.5);
    press(&mut h, "Reset");
    assert_eq!(view(&env).draft_charts.as_deref(), Some("adaptive"));
    press(&mut h, "Ok");
    assert_eq!(room.borrow().rules().charts, ddi_room::Charts::Adaptive);
    settings.multiplayer_rules_first = false;
    settings.save();
}

/// Attacks at once (`attacks-v3`): under Attacks while they are On,
/// three by default with its sentence; 1, 2, 3, No limit; applied, the
/// summary names it where it is not three; Reset gives three; gone with
/// Attacks Off.
#[test]
fn the_attacks_at_once_row_shows_under_attacks_and_three_is_the_default() {
    let env = env();
    let room = RoomClient::shared(0, 1);
    room.borrow_mut().set_rules_seen(true);
    let orders = Orders::default();
    let mut h = harness();
    h.push(room_select(&env, room.clone(), "dance-single", &orders));
    h.run(0.5);
    press(&mut h, "Room.Rules");
    assert_eq!(said(&h, "Rule.at_once"), "Attacks at once");
    assert_eq!(said(&h, "Rule.at_once.Value"), "3");
    assert_eq!(view(&env).draft_at_once.as_deref(), Some("three"));
    assert!(
        h.texts().iter().any(|t| t.starts_with("How many attacks can be on their way")),
        "{:?}",
        h.texts()
    );
    press(&mut h, "Inc.at_once");
    assert_eq!(said(&h, "Rule.at_once.Value"), "No limit");
    press(&mut h, "Dec.at_once");
    press(&mut h, "Dec.at_once");
    press(&mut h, "Dec.at_once");
    assert_eq!(said(&h, "Rule.at_once.Value"), "1");
    assert_eq!(view(&env).draft_at_once.as_deref(), Some("one"));
    // Attacks Off: the row goes.
    press(&mut h, "Dec.attacks");
    assert!(!there(&h, "Inc.at_once"));
    assert_eq!(view(&env).draft_at_once, None);
    press(&mut h, "Inc.attacks");
    assert_eq!(said(&h, "Rule.at_once.Value"), "1");
    press(&mut h, "Apply");
    assert_eq!(room.borrow().rules().at_once, ddi_room::AttacksAtOnce::One);
    let words = |r: &ddi_room::MatchRules, attacks| {
        String::from(super::room::rules_words(
            &crate::state::room::StageRules::of(r),
            attacks,
        ))
    };
    assert_eq!(words(room.borrow().rules(), Some(true)), "DDI · One attack at once");
    assert_eq!(words(room.borrow().rules(), Some(false)), "DDI · No attacks");
    assert_eq!(words(&ddi_room::MatchRules::hosted_for(&ddi_room::RoomPolicy::default().with_live()), Some(true)), "DDI");
    // Its stage rules' plan names the limit.
    assert_eq!(
        crate::state::room::StageRules::of(room.borrow().rules()).attack_rules().at_once(),
        Some(1)
    );
    // Reset's default is three.
    let mut settings = crate::state::settings::Settings::load();
    settings.multiplayer_rules_first = true;
    settings.save();
    room.borrow_mut().set_rules_seen(false);
    h.run(0.5);
    press(&mut h, "Reset");
    assert_eq!(view(&env).draft_at_once.as_deref(), Some("three"));
    press(&mut h, "Ok");
    assert_eq!(room.borrow().rules().at_once, ddi_room::AttacksAtOnce::Three);
    settings.multiplayer_rules_first = false;
    settings.save();
}

/// The rules' summary says what differs from the usual: attacks are On by
/// default where they are offered, so it says "No attacks" where the
/// match has them turned off, and nothing of them where it has them or
/// where the room offers none (an online room).
#[test]
fn the_rules_summary_says_no_attacks_only_where_they_are_turned_off() {
    use super::room::rules_words;
    // The usual rules: Adaptive charts, a new room's.
    let rules = crate::state::room::StageRules {
        charts: ddi_room::Charts::Adaptive,
        easiest: ddi_room::AtEasiest::Out,
        ..Default::default()
    };
    let words = |r: &crate::state::room::StageRules, attacks| String::from(rules_words(r, attacks));
    assert_eq!(words(&rules, Some(true)), "DDI");
    assert_eq!(words(&rules, Some(false)), "DDI · No attacks");
    assert_eq!(words(&rules, None), "DDI");
    let hard = crate::state::room::StageRules {
        difficulty: ddi_room::MatchDifficulty::Hard,
        boos: true,
        ..rules.clone()
    };
    assert_eq!(
        words(&hard, Some(false)),
        "DDI · Hard timing · Boos · No attacks"
    );
    // A stage's prompt: the same words after the stage's line.
    use super::prompt::{PromptInfo, StageLine};
    let line = |attacks| StageLine {
        number: 1,
        stages: 4,
        cpus: 3,
        others: 0,
        rules: &rules,
        attacks,
        adaptive_offered: true,
    };
    let mut info = PromptInfo::default();
    info.set_stage(line(Some(false)));
    assert!(info.mods.ends_with("Rules: DDI · No attacks"), "{}", info.mods);
    let mut info = PromptInfo::default();
    info.set_stage(line(Some(true)));
    assert!(info.mods.ends_with("Rules: DDI"), "{}", info.mods);
    // A new practice room has attacks, and Reset's defaults too.
    let room = RoomClient::shared(0, 1);
    assert!(room.borrow().rules().attacks() && room.borrow().attacks_on());
}

/// The DDI ruleset's round 3: the match's Difficulty, Boos and Attacks are
/// rows of the Rules pop-up a keyboard and a controller reach and change
/// like Stages (the focus walks the rows' two buttons, Confirm presses the
/// one focused), and what was applied is the room's.
#[test]
fn the_match_rules_rows_work_by_keyboard_and_controller() {
    use crate::nav::MenuAction;
    let env = env();
    let room = RoomClient::shared(0, 1);
    room.borrow_mut().set_rules_seen(true);
    let orders = Orders::default();
    let mut h = harness();
    h.push(room_select(&env, room.clone(), "dance-single", &orders));
    h.run(0.5);
    press(&mut h, "Room.Rules");
    assert_eq!(top(&h), "room-rules");
    // Every row's buttons are in the focus walk, in the rows' order.
    let names: Vec<String> = env.state.borrow().page_items.clone();
    let at = |n: &str| names.iter().position(|x| x == n).unwrap_or_else(|| panic!("{n}"));
    assert!(at("Inc.stages") < at("Dec.difficulty"));
    assert!(at("Inc.difficulty") < at("Dec.boos"));
    assert!(at("Inc.boos") < at("AddRandom"));
    assert!(at("AddCustom") < at("Dec.attacks"));
    assert!(at("Inc.attacks") < at("Apply"));
    // A keyboard: walk to a row's button (up or down the rows, then
    // along the row), Enter presses it.
    let walk_to = |h: &mut Harness, name: &str, down: bool, pad: bool| {
        let row = name.split_once('.').map_or(name, |(_, row)| row);
        for _ in 0..80 {
            let at = h.ui.focused().map(str::to_string);
            if at.as_deref() == Some(name) {
                return;
            }
            let along = at
                .as_deref()
                .and_then(|a| a.split_once('.'))
                .is_some_and(|(_, r)| r == row);
            let (action, code) = match (along, down) {
                (true, _) => (MenuAction::Right, "ArrowRight"),
                (false, true) => (MenuAction::Down, "ArrowDown"),
                (false, false) => (MenuAction::Up, "ArrowUp"),
            };
            if pad {
                h.pad(action, true);
                h.pad(action, false);
            } else {
                h.key(code, code);
            }
            h.run(0.05);
        }
        panic!("never reached {name}: {:?}", h.ui.focused());
    };
    assert_eq!(h.ui.focused(), Some("Dec.stages"), "it opens on its first row");
    walk_to(&mut h, "Inc.difficulty", true, false);
    h.key("Enter", "Enter");
    h.run(0.3);
    assert_eq!(said(&h, "Rule.difficulty.Value"), "Hard");
    assert_eq!(h.ui.focused(), Some("Inc.difficulty"), "the focus stays");
    walk_to(&mut h, "Inc.boos", true, false);
    h.key("Space", " ");
    h.run(0.3);
    assert_eq!(said(&h, "Rule.boos.Value"), "On");
    // A controller: the same walk, Start presses.
    walk_to(&mut h, "Inc.attacks", true, true);
    h.pad(MenuAction::Start, true);
    h.pad(MenuAction::Start, false);
    h.run(0.3);
    assert_eq!(said(&h, "Rule.attacks.Value"), "On");
    walk_to(&mut h, "Apply", true, true);
    h.pad(MenuAction::Start, true);
    h.pad(MenuAction::Start, false);
    h.run(0.5);
    assert_eq!(top(&h), "select");
    let r = room.borrow();
    assert_eq!(r.rules().difficulty, ddi_room::MatchDifficulty::Hard);
    assert!(r.rules().boos && r.rules().attacks());
}

/// The stage's prompt in a room: Start is Ready (toggled, the shell told
/// inside the press), the players listed with their readiness, and the
/// stage started by the room (Start chosen for the shell) once everyone
/// is ready.
#[test]
fn the_prompt_is_ready_lists_the_players_and_starts_with_the_room() {
    let env = env();
    let room = RoomClient::shared(0, 1);
    let orders = Orders::default();
    let s = &library("dance-single").songs[0];
    room.borrow_mut().pick(s, &s.charts[0]).unwrap();
    let order = room.borrow_mut().take_stage_order().unwrap();
    let seen: Rc<RefCell<Vec<PromptChoice>>> = Rc::default();
    let sink = seen.clone();
    let state = Rc::new(RefCell::new(PromptState::default()));
    state.borrow_mut().info.title = "Single Song".into();
    let mut h = harness();
    h.push(
        PromptScreen::new(
            env.clone(),
            state.clone(),
            Rc::new(move |c, _| sink.borrow_mut().push(c)),
        )
        .with_room(room.clone(), order.stage, shell(&room, &orders)),
    );
    h.run(0.5);
    assert!(has(&h, "Ready"), "{:?}", h.texts());
    assert!(has(&h, "You"), "{:?}", h.texts());
    let cpu = order.cpus[0].name.clone().unwrap();
    assert!(has(&h, &cpu), "{cpu}: {:?}", h.texts());
    assert!(
        h.texts()
            .iter()
            .any(|t| t.starts_with("Stage 1 starts once everyone is ready")),
        "{:?}",
        h.texts()
    );
    // Ready and back: toggled, the shell told each time.
    press(&mut h, "Start");
    assert!(room.borrow().ready());
    // Alone with CPUs, ready and loaded: the countdown.
    assert!(matches!(
        room.borrow().state().phase,
        Phase::Countdown { .. }
    ));
    assert_eq!(*seen.borrow(), [PromptChoice::Ready]);
    assert!(has(&h, "Cancel ready"), "{:?}", h.texts());
    h.run(3.5);
    assert_eq!(
        seen.borrow().last(),
        Some(&PromptChoice::Start),
        "the room started it"
    );
    assert!(room.borrow().stage_started(order.stage));
}

/// A practice room with the Attacks rule on, its first song picked: the
/// stage's order and its prompt on the harness.
fn attacks_prompt(env: &Env, room: &SharedRoom, h: &mut Harness) -> StageOrder {
    let mut rules = room.borrow().rules().clone();
    rules.set_attacks(true);
    room.borrow_mut().save_rules(rules).unwrap();
    let s = &library("dance-single").songs[0];
    room.borrow_mut().pick(s, &s.charts[0]).unwrap();
    let order = room.borrow_mut().take_stage_order().unwrap();
    let state = Rc::new(RefCell::new(PromptState::default()));
    state.borrow_mut().info.title = "Single Song".into();
    h.push(
        PromptScreen::new(env.clone(), state, Rc::new(|_, _| {})).with_room(
            room.clone(),
            order.stage,
            shell(room, &Orders::default()),
        ),
    );
    h.run(0.5);
    order
}

/// The policy row says how to change it during play (the maintainer,
/// 2026-11-13): Shift's cap and "during play" from the keyboard; the
/// controller's Back once a controller is the hints' source; nothing on a
/// touch screen, where the label is tapped.
#[test]
fn the_prompt_s_policy_row_says_how_to_change_it_during_play() {
    let row = "Opt.room.targeting";
    let key = format!("{row}.Key");
    let room = RoomClient::shared(0, 1);
    let mut h = harness();
    let e = env();
    attacks_prompt(&e, &room, &mut h);
    assert!(there(&h, &key), "{:?}", h.texts());
    assert!(h.texts().iter().any(|t| t.contains("during play")), "{:?}", h.texts());
    assert!(h.texts().iter().any(|t| t.contains("shift")), "{:?}", h.texts());
    // A controller's edge: its Back's cap.
    let controls = ddi_engine::ControlBindings::default();
    h.controller(&controls, "Pad", true, "button:12", true);
    h.controller(&controls, "Pad", true, "button:12", false);
    h.run(0.2);
    assert!(h.ui.hints().active());
    assert!(there(&h, &key));
    assert!(!h.texts().iter().any(|t| t.contains("shift")), "{:?}", h.texts());
    assert!(h.texts().iter().any(|t| t.contains("during play")));
    // A touch screen: none (the row is there, its hint not).
    let room = RoomClient::shared(0, 1);
    let mut rules = room.borrow().rules().clone();
    rules.set_attacks(true);
    room.borrow_mut().save_rules(rules).unwrap();
    let s = &library("dance-single").songs[0];
    room.borrow_mut().pick(s, &s.charts[0]).unwrap();
    let order = room.borrow_mut().take_stage_order().unwrap();
    let state = Rc::new(RefCell::new(PromptState::default()));
    state.borrow_mut().touch = true;
    let mut h = harness();
    h.push(
        PromptScreen::new(env(), state, Rc::new(|_, _| {})).with_room(
            room.clone(),
            order.stage,
            shell(&room, &Orders::default()),
        ),
    );
    h.run(0.5);
    assert!(there(&h, row));
    assert!(!there(&h, &key));
}

/// M8 round 3 (D8): where the match has attacks, the stage's prompt has
/// the player's attack policy as one quiet row with what it does: Left
/// and Right, a tap on its arrows or on the row, and Enter change it
/// (Spread, Retaliate, Leader, in a ring), the room told each time,
/// readiness untouched. Without attacks the row is not there.
#[test]
fn the_prompt_has_the_attack_policy_where_the_match_has_attacks() {
    use crate::MenuAction;
    use ddi_room::attacks::Targeting;
    let row = "Opt.room.targeting";
    // Without attacks (the rule turned off): no row.
    let plain = RoomClient::shared(0, 1);
    {
        let mut rules = plain.borrow().rules().clone();
        rules.set_attacks(false);
        plain.borrow_mut().save_rules(rules).unwrap();
    }
    let mut h = harness();
    let s = &library("dance-single").songs[0];
    plain.borrow_mut().pick(s, &s.charts[0]).unwrap();
    let order = plain.borrow_mut().take_stage_order().unwrap();
    h.push(
        PromptScreen::new(
            env(),
            Rc::new(RefCell::new(PromptState::default())),
            Rc::new(|_, _| {}),
        )
        .with_room(plain.clone(), order.stage, shell(&plain, &Orders::default())),
    );
    h.run(0.5);
    assert!(!there(&h, row));
    assert!(!has(&h, "Targeting"));
    // With attacks.
    let room = RoomClient::shared(0, 1);
    let mut h = harness();
    let e = env();
    let order = attacks_prompt(&e, &room, &mut h);
    assert_eq!(order.targeting, Targeting::Spread);
    assert!(there(&h, row));
    assert_eq!(said(&h, &format!("{row}.Label")), "Targeting");
    assert_eq!(said(&h, &format!("{row}.Value")), "Spread");
    assert!(
        has(&h, "Your attacks go to whoever has been attacked least."),
        "{:?}",
        h.texts()
    );
    // Up from Ready reaches the row; Right and Left step it.
    assert_eq!(h.ui.focused(), Some("Start"));
    h.pad(MenuAction::Up, true);
    h.pad(MenuAction::Up, false);
    h.run(0.2);
    assert_eq!(h.ui.focused(), Some(row));
    h.pad(MenuAction::Right, true);
    h.pad(MenuAction::Right, false);
    h.run(0.2);
    assert_eq!(room.borrow().my_targeting(), Targeting::Retaliate);
    assert_eq!(said(&h, &format!("{row}.Value")), "Retaliate");
    assert!(has(&h, "Your attacks go to whoever attacked you last."));
    h.pad(MenuAction::Left, true);
    h.pad(MenuAction::Left, false);
    h.pad(MenuAction::Left, true);
    h.pad(MenuAction::Left, false);
    h.run(0.2);
    assert_eq!(room.borrow().my_targeting(), Targeting::Leader, "a ring");
    assert!(has(&h, "Your attacks go to whoever has the most life."));
    // A tap on the row cycles; on its arrows steps.
    press(&mut h, &format!("{row}.Label"));
    assert_eq!(room.borrow().my_targeting(), Targeting::Spread);
    press(&mut h, &format!("{row}.Up"));
    assert_eq!(room.borrow().my_targeting(), Targeting::Retaliate);
    press(&mut h, &format!("{row}.Down"));
    assert_eq!(room.borrow().my_targeting(), Targeting::Spread);
    // Ready, then changed: still ready (the countdown goes on), and the
    // room has the policy for the play.
    press(&mut h, "Start");
    assert!(room.borrow().ready());
    assert!(matches!(
        room.borrow().state().phase,
        Phase::Countdown { .. }
    ));
    press(&mut h, &format!("{row}.Up"));
    assert_eq!(room.borrow().my_targeting(), Targeting::Retaliate);
    assert!(room.borrow().ready());
    assert!(matches!(
        room.borrow().state().phase,
        Phase::Countdown { .. }
    ));
    // The debug state says it.
    assert_eq!(
        e.state.borrow().room.as_ref().unwrap().targeting,
        "retaliate"
    );
    // The stage in play: the room refuses a change (the play's own), and
    // takes what the play left the policy at afterwards.
    h.run(3.5);
    assert!(room.borrow().stage_started(order.stage));
    assert!(room.borrow_mut().set_targeting(Targeting::Leader).is_err());
    room.borrow_mut().clear_refusal();
    assert_eq!(room.borrow().my_targeting(), Targeting::Retaliate);
}

/// The players' rows say each one's attack policy, quietly, where the
/// match has attacks (the panel beside the song, the Players pop-up),
/// and nothing of it where it has none; a roster CPU's is its own.
#[test]
fn the_players_rows_say_each_policy_where_the_match_has_attacks() {
    use super::room::player_rows;
    use ddi_room::attacks::Targeting;
    let room = RoomClient::shared(0, 1);
    let mut rules = room.borrow().rules().clone();
    rules.set_attacks(false);
    room.borrow_mut().save_rules(rules.clone()).unwrap();
    assert!(player_rows(&room.borrow()).iter().all(|p| p.policy.is_empty()));
    rules.set_attacks(true);
    // A roster CPU with a policy of its own.
    rules.cpu.roster[0].targeting = Some(Targeting::Leader);
    rules.cpu.roster[1].targeting = None;
    room.borrow_mut().save_rules(rules).unwrap();
    room.borrow_mut().set_targeting(Targeting::Retaliate).unwrap();
    let rows = player_rows(&room.borrow());
    assert_eq!(rows[0].policy, "Retaliate", "You");
    assert_eq!(rows[1].policy, "Leader");
    assert_eq!(rows[2].policy, "Spread");
    // Drawn on the panel and in the pop-up.
    let e = env();
    let panel = super::room::players_panel(&e, &room.borrow(), [0.0, 0.0, 520.0, 200.0]);
    fn collect(a: &ddi_actor::Actor, out: &mut Vec<String>) {
        if let ddi_actor::Kind::Text(t) = &a.kind {
            out.push(t.text().to_string());
        }
        for c in a.children() {
            collect(c, out);
        }
    }
    let texts = |a: &ddi_actor::Actor| {
        let mut out = Vec::new();
        collect(a, &mut out);
        out
    };
    let t = texts(&panel);
    assert!(t.contains(&"Retaliate".to_string()) && t.contains(&"Leader".to_string()), "{t:?}");
    let rows = {
        let r = room.borrow();
        super::room::panel_rows(&r, super::room::player_rows(&r))
    };
    let (popup, _) = super::room::panel(&e, &rows, 0, [0.0, 0.0, 420.0, 600.0]);
    let t = texts(&popup);
    assert!(t.contains(&"Retaliate".to_string()) && t.contains(&"Leader".to_string()), "{t:?}");
}

/// Scrolled into the middle of a long list, the panel's rows end above
/// its "N more below" mark: both marks have their room.
#[test]
fn the_panels_rows_stay_clear_of_both_marks() {
    use super::room::PanelRow;
    let e = env();
    let rows: Vec<PanelRow> = (0..20)
        .map(|k| PanelRow::Heading(format!("Row {k}"), String::new()))
        .collect();
    let height = 204.0;
    let (panel, first) = super::room::panel(&e, &rows, 5, [0.0, 0.0, 300.0, height]);
    assert_eq!(first, 5);
    let drawn: Vec<usize> = (0..rows.len())
        .filter(|k| crate::hit::find(&panel, &format!("Panel{k}.Heading")).is_some())
        .collect();
    assert!(crate::hit::find(&panel, "Panel.Up").is_some());
    assert!(crate::hit::find(&panel, "Panel.Down").is_some());
    // Rows of 30 from under the top mark (24), the bottom mark's 24 kept.
    let end = 24.0 + drawn.len() as f32 * 30.0;
    assert!(end <= height - 24.0, "{drawn:?} end at {end}");
    assert_eq!(drawn.first(), Some(&5));
}

/// One not ready at the deadline sits the stage out: the prompt says so
/// and goes back to the room.
#[test]
fn not_ready_in_time_sits_out_and_the_prompt_goes_back() {
    use crate::state::online::tests::room::{client, person, prepare, song, tick};
    use ddi_room::fake::FakeServer;
    use ddi_room::{Identity, Intent, ManualClock, RoomPolicy};
    let clock = ManualClock::new(1_000);
    let server = FakeServer::hosted(
        "Room000001",
        Identity::new("dev:alice"),
        "alice",
        RoomPolicy::default(),
        clock.clone(),
    )
    .shared();
    for who in ["bob", "carol"] {
        server
            .borrow_mut()
            .join(Identity::new(format!("dev:{who}")), who, Intent::Play)
            .unwrap();
    }
    let alice = Rc::new(RefCell::new(client(
        &server,
        person(&server, "alice"),
        "alice",
        1_000,
    )));
    let bob = Rc::new(RefCell::new(client(
        &server,
        person(&server, "bob"),
        "bob",
        1_000,
    )));
    let carol = Rc::new(RefCell::new(client(
        &server,
        person(&server, "carol"),
        "carol",
        1_000,
    )));
    let step = |ms: u64| {
        tick(
            &clock,
            &mut [
                &mut alice.borrow_mut(),
                &mut bob.borrow_mut(),
                &mut carol.borrow_mut(),
            ],
            ms,
        )
    };
    step(400);
    let s = song();
    alice.borrow_mut().pick(&s, &s.charts[0]).unwrap();
    step(400);
    crate::state::online::tests::room::upload(&server, &mut alice.borrow_mut(), "abc12345");
    step(400);
    for r in [&alice, &bob, &carol] {
        prepare(&mut r.borrow_mut());
    }
    step(400);
    let order = bob.borrow_mut().take_stage_order().unwrap();
    let seen: Rc<RefCell<Vec<PromptChoice>>> = Rc::default();
    let sink = seen.clone();
    let orders = Orders::default();
    let eb = env();
    let mut hb = harness();
    hb.push(
        PromptScreen::new(
            eb.clone(),
            Rc::new(RefCell::new(PromptState::default())),
            Rc::new(move |c, _| sink.borrow_mut().push(c)),
        )
        .with_room(bob.clone(), order.stage, shell(&bob, &orders)),
    );
    hb.run(0.3);
    assert!(has(&hb, "alice") && has(&hb, "carol"), "{:?}", hb.texts());
    alice.borrow_mut().set_ready(true).unwrap();
    carol.borrow_mut().set_ready(true).unwrap();
    step(400);
    hb.run(0.3);
    assert_eq!(
        bob.borrow().state().phase.kind(),
        ddi_room::PhaseKind::Preparing
    );
    // Bob never readies: at the deadline he sits out.
    step(30_000);
    step(4_000);
    hb.run(0.3);
    assert!(
        has(&hb, "You weren't ready when stage 1 started. You play the next one."),
        "{:?}",
        hb.texts()
    );
    hb.run(3.0);
    assert_eq!(seen.borrow().last(), Some(&PromptChoice::Back));
    assert!(!seen.borrow().contains(&PromptChoice::Start));
}

/// Round P1b (the maintainer's of 2026-11-16): a follower's failed
/// download of the stage's song is tried again by itself twice, after 2 s
/// and 5 s (`LOAD_RETRY_V0_MS`), the room hearing "loading" all along;
/// once the tries are spent the failure is reported (everyone's lists say
/// "Download failed") and the follower's song select has Retry beside its
/// status line (and More's item) while the window is open, which loads
/// again and, this time, readies the song.
#[test]
fn a_followers_failed_download_is_tried_again_then_offers_retry() {
    use crate::state::online::tests::room::{client, person, prepare, song, tick};
    use super::room;
    use crate::state::room::{LOAD_RETRY_V0_MS, LoadProblem};
    use ddi_room::fake::FakeServer;
    use ddi_room::{Identity, Intent, Loading, ManualClock, ParticipantId, RoomPolicy};
    let clock = ManualClock::new(1_000);
    let server = FakeServer::hosted(
        "Room000001",
        Identity::new("dev:alice"),
        "alice",
        RoomPolicy::default(),
        clock.clone(),
    )
    .shared();
    server
        .borrow_mut()
        .join(Identity::new("dev:bob"), "bob", Intent::Play)
        .unwrap();
    let alice = Rc::new(RefCell::new(client(
        &server,
        person(&server, "alice"),
        "alice",
        1_000,
    )));
    let bob = Rc::new(RefCell::new(client(
        &server,
        person(&server, "bob"),
        "bob",
        1_000,
    )));
    let step = |ms: u64| {
        tick(
            &clock,
            &mut [&mut alice.borrow_mut(), &mut bob.borrow_mut()],
            ms,
        )
    };
    step(400);
    let s = song();
    alice.borrow_mut().pick(&s, &s.charts[0]).unwrap();
    step(400);
    crate::state::online::tests::room::upload(&server, &mut alice.borrow_mut(), "abc12345");
    step(400);
    prepare(&mut alice.borrow_mut());
    step(400);
    let stage = bob.borrow().state().upcoming_stage().unwrap();
    let bobs = || {
        server
            .borrow()
            .state()
            .stage(stage)
            .unwrap()
            .prep(ParticipantId(2))
            .loading
            .clone()
    };
    // Every download fails: each failure but the last is tried again.
    let fail = |bob: &Rc<RefCell<crate::state::room::RoomClient>>| {
        let jobs = bob.borrow_mut().take_load_jobs();
        assert_eq!(jobs.len(), 1, "one job");
        bob.borrow_mut()
            .finish_shared_load(&jobs[0], Err(LoadProblem::Download));
    };
    fail(&bob);
    step(400);
    assert!(matches!(bobs(), Loading::Loading { .. }), "{:?}", bobs());
    assert!(bob.borrow_mut().take_load_jobs().is_empty(), "not before its wait");
    step(LOAD_RETRY_V0_MS[0]);
    fail(&bob);
    step(LOAD_RETRY_V0_MS[1]);
    assert!(matches!(bobs(), Loading::Loading { .. }), "{:?}", bobs());
    assert!(!bob.borrow().may_retry_load(stage));
    fail(&bob);
    step(400);
    assert_eq!(
        bobs(),
        Loading::Failed {
            code: LoadProblem::Download.code().into()
        }
    );
    assert!(bob.borrow().may_retry_load(stage));
    // Bob's song select: the failure and Retry.
    let orders = Orders::default();
    let eb = env();
    let mut hb = harness();
    hb.push(
        sample::room_screen(&eb, bob.clone())
            .with_room(bob.clone(), shell(&bob, &orders)),
    );
    hb.run(0.5);
    assert!(has(&hb, "The room's song could not be downloaded."), "{:?}", hb.texts());
    assert!(has(&hb, "Retry"), "{:?}", hb.texts());
    let items: Vec<String> = room::more_items(&bob.borrow(), false)
        .into_iter()
        .map(|(id, _)| id)
        .collect();
    assert!(items.contains(&room::RETRY_LOAD.to_string()), "{items:?}");
    // (This shell prepares what it is handed at once, from the share.)
    press(&mut hb, room::RETRY_LOAD);
    assert!(!bob.borrow().may_retry_load(stage));
    step(400);
    assert_eq!(bobs(), Loading::Loaded);
    hb.run(0.5);
    assert!(!has(&hb, "Retry"), "{:?}", hb.texts());
}

/// Round P1b (the maintainer's of 2026-11-16): a member who missed the
/// Ready window keeps the stage's Ready screen for the late join after
/// its start (it used to turn into "sitting out"): its Ready enters them
/// (`EnteredLate`) and the play starts, late; one who lets the late join
/// pass watches as before.
#[test]
fn a_member_who_missed_the_window_may_still_ready_within_the_late_join() {
    use crate::state::online::tests::room::{client, person, prepare, song, tick};
    use ddi_room::fake::FakeServer;
    use ddi_room::{Identity, Intent, ManualClock, ParticipantId, RoomPolicy};
    for press in [true, false] {
        let clock = ManualClock::new(1_000);
        let server = FakeServer::hosted(
            "Room000001",
            Identity::new("dev:alice"),
            "alice",
            RoomPolicy::default().with_live(),
            clock.clone(),
        )
        .shared();
        for who in ["bob", "carol"] {
            server
                .borrow_mut()
                .join(Identity::new(format!("dev:{who}")), who, Intent::Play)
                .unwrap();
        }
        let alice = Rc::new(RefCell::new(client(
            &server,
            person(&server, "alice"),
            "alice",
            1_000,
        )));
        let bob = Rc::new(RefCell::new(client(
            &server,
            person(&server, "bob"),
            "bob",
            1_000,
        )));
        let carol = Rc::new(RefCell::new(client(
            &server,
            person(&server, "carol"),
            "carol",
            1_000,
        )));
        let step = |ms: u64| {
            for _ in 0..ms / 100 {
                tick(
                    &clock,
                    &mut [
                        &mut alice.borrow_mut(),
                        &mut bob.borrow_mut(),
                        &mut carol.borrow_mut(),
                    ],
                    100,
                );
            }
        };
        step(2_000);
        let s = song();
        alice.borrow_mut().pick(&s, &s.charts[0]).unwrap();
        step(400);
        crate::state::online::tests::room::upload(&server, &mut alice.borrow_mut(), "abc12345");
        step(400);
        for r in [&alice, &bob, &carol] {
            prepare(&mut r.borrow_mut());
        }
        step(400);
        let order = bob.borrow_mut().take_stage_order().unwrap();
        let seen: Rc<RefCell<Vec<PromptChoice>>> = Rc::default();
        let sink = seen.clone();
        let orders = Orders::default();
        let mut hb = harness();
        hb.push(
            PromptScreen::new(
                env(),
                Rc::new(RefCell::new(PromptState::default())),
                Rc::new(move |c, _| sink.borrow_mut().push(c)),
            )
            .with_room(bob.clone(), order.stage, shell(&bob, &orders)),
        );
        hb.run(0.3);
        alice.borrow_mut().set_ready(true).unwrap();
        carol.borrow_mut().set_ready(true).unwrap();
        // Bob lets the window run out: the stage commits without him.
        for _ in 0..80 {
            step(500);
            hb.run(0.05);
            if matches!(
                server.borrow().state().phase,
                ddi_room::Phase::Countdown { .. }
            ) {
                break;
            }
        }
        let stage = order.stage;
        let play = || server.borrow().state().stage(stage).unwrap().play.clone().unwrap();
        assert!(!play().lineup.contains(&ParticipantId(2)));
        // Past the countdown and two seconds into the song: still his
        // Ready screen, its Ready on offer.
        step(6_000);
        hb.run(0.3);
        assert_eq!(
            server.borrow().state().phase.kind(),
            ddi_room::PhaseKind::Stage
        );
        assert!(bob.borrow().may_enter_late(stage), "{press}");
        assert!(has(&hb, "Ready"), "{press}: {:?}", hb.texts());
        // Round Y3: the card says Ready still enters, never that he is
        // watching (it said "You weren't ready when stage 1 started.
        // You're watching this stage…" beside the Ready it offered).
        let line = |hb: &Harness| hb.texts().join(" | ");
        assert!(
            line(&hb).contains("Stage 1 has started. Press Ready now to play it from here."),
            "{press}: {}",
            line(&hb)
        );
        assert!(!line(&hb).contains("watching this stage"), "{}", line(&hb));
        assert!(!seen.borrow().contains(&PromptChoice::Start));
        if press {
            hb.key("Enter", "Enter");
            step(400);
            hb.run(0.3);
            assert!(play().lineup.contains(&ParticipantId(2)), "entered");
            assert!(seen.borrow().contains(&PromptChoice::Start), "the play starts");
        } else {
            // The late join passes: watched as before.
            step(4_000);
            hb.run(0.3);
            assert!(!bob.borrow().may_enter_late(stage));
            assert!(!play().lineup.contains(&ParticipantId(2)));
        }
    }
}

/// Round P1b: an online room's committed start is past taking Ready
/// back, and its prompt says so: during the countdown the button says
/// "Starting…" (never "Cancel ready"), no hint says how to cancel, and
/// Enter twice asks the room nothing (no refusal on the status line).
#[test]
fn an_online_rooms_countdown_says_starting_and_offers_no_cancel() {
    use crate::state::online::tests::room::{client, person, prepare, song, tick};
    use ddi_room::fake::FakeServer;
    use ddi_room::{Identity, Intent, ManualClock, RoomPolicy};
    let clock = ManualClock::new(1_000);
    let server = FakeServer::hosted(
        "Room000001",
        Identity::new("dev:alice"),
        "alice",
        RoomPolicy::default(),
        clock.clone(),
    )
    .shared();
    server
        .borrow_mut()
        .join(Identity::new("dev:bob"), "bob", Intent::Play)
        .unwrap();
    let alice = Rc::new(RefCell::new(client(
        &server,
        person(&server, "alice"),
        "alice",
        1_000,
    )));
    let bob = Rc::new(RefCell::new(client(
        &server,
        person(&server, "bob"),
        "bob",
        1_000,
    )));
    let step = |ms: u64| {
        tick(
            &clock,
            &mut [&mut alice.borrow_mut(), &mut bob.borrow_mut()],
            ms,
        )
    };
    step(400);
    let s = song();
    alice.borrow_mut().pick(&s, &s.charts[0]).unwrap();
    step(400);
    crate::state::online::tests::room::upload(&server, &mut alice.borrow_mut(), "abc12345");
    step(400);
    for r in [&alice, &bob] {
        prepare(&mut r.borrow_mut());
    }
    step(400);
    let order = bob.borrow_mut().take_stage_order().unwrap();
    let orders = Orders::default();
    let mut hb = harness();
    hb.push(
        PromptScreen::new(
            env(),
            Rc::new(RefCell::new(PromptState::default())),
            Rc::new(|_, _| {}),
        )
        .with_room(bob.clone(), order.stage, shell(&bob, &orders)),
    );
    hb.run(0.3);
    hb.key("Enter", "Enter");
    hb.run(0.2);
    assert!(bob.borrow().ready());
    assert!(has(&hb, "Cancel ready"), "{:?}", hb.texts());
    assert!(hb.ui.visible_bounds("Room.UnreadyHint").is_some());
    alice.borrow_mut().set_ready(true).unwrap();
    step(200);
    hb.run(0.3);
    assert_eq!(
        bob.borrow().state().phase.kind(),
        ddi_room::PhaseKind::Countdown
    );
    assert!(!bob.borrow().may_take_ready_back());
    assert!(has(&hb, "Starting…"), "{:?}", hb.texts());
    assert!(!has(&hb, "Cancel ready"), "{:?}", hb.texts());
    assert!(hb.ui.visible_bounds("Room.UnreadyHint").is_none());
    for _ in 0..2 {
        hb.key("Enter", "Enter");
        hb.run(0.1);
    }
    assert!(bob.borrow().refusal().is_none(), "nothing asked of the room");
    assert!(bob.borrow().ready());
    assert_eq!(
        bob.borrow().state().phase.kind(),
        ddi_room::PhaseKind::Countdown
    );
}

/// Stages reported as a stage runner does: back on song select the
/// stage's standings pop up once, Continue, the next pick; after the
/// last, the match's results, and a pick opens the next match.
#[test]
fn standings_follow_each_stage_and_the_matchs_results_then_the_next_match() {
    use ddi_room::{Finish, Finisher};
    let env = env();
    let room = RoomClient::shared(0, 1);
    room.borrow_mut().set_rules_seen(true);
    let rules = ddi_room::MatchRules {
        stages: 2,
        ..room.borrow().rules().clone()
    };
    room.borrow_mut().save_rules(rules).unwrap();
    let orders = Orders::default();
    let mut h = harness();
    h.push(room_select(&env, room.clone(), "dance-single", &orders));
    h.run(0.5);
    let mut now = 0;
    let mut play = |h: &mut Harness, place: usize| {
        h.key("Enter", "Enter");
        h.run(0.3);
        let order = orders.borrow_mut().pop().expect("a prompt");
        room.borrow_mut().set_ready(true).unwrap();
        now += 4_000;
        room.borrow_mut().tick(now);
        let mut ids: Vec<_> = order.cpus.iter().map(|c| c.participant).collect();
        ids.insert(place - 1, order.me);
        let ranking = ids
            .into_iter()
            .map(|participant| {
                vec![Finisher {
                    participant,
                    finish: Finish::Survived,
                }]
            })
            .collect();
        room.borrow_mut()
            .report_stage(order.stage, ranking, false)
            .unwrap();
        h.run(0.5);
    };
    play(&mut h, 1);
    assert_eq!(top(&h), "room-standings");
    assert!(view(&env).standings_open);
    assert!(has(&h, "Stage 1 standings"), "{:?}", h.texts());
    assert!(
        h.texts()
            .iter()
            .any(|t| t.starts_with("Stage 1: place 1 of 4")),
        "{:?}",
        h.texts()
    );
    press(&mut h, "Continue");
    assert_eq!(top(&h), "select", "{:?}", h.texts());
    h.run(0.5);
    assert_eq!(top(&h), "select", "once");
    assert_eq!(view(&env).status, "Pick stage 2's song.");
    play(&mut h, 4);
    assert_eq!(top(&h), "room-standings");
    assert!(has(&h, "Match results"), "{:?}", h.texts());
    press(&mut h, "Continue");
    assert_eq!(
        view(&env).status,
        "Match over. Pick a song to start the next match."
    );
    h.key("Enter", "Enter");
    h.run(0.3);
    let v = view(&env);
    assert_eq!((v.phase.as_str(), v.history), ("preparing", 1));
}

/// A room of 73 (the player and 72 CPUs): the player's line first in the
/// panel, the rest counted; a stage settled by the top-biased table; the
/// standings keep the player's line in view however far down they are.
#[test]
fn a_room_of_seventy_three_keeps_the_player_in_view_in_its_lists() {
    use ddi_room::{Finish, Finisher};
    let env = env();
    let room = RoomClient::shared(0, 1);
    room.borrow_mut().set_rules_seen(true);
    {
        let roster = crate::state::cpu_roster::at_level(&[], 4, 72, 3);
        let mut rules = crate::state::cpu_roster::with_roster(room.borrow().rules().clone(), roster, 1);
        rules.stages = 2;
        room.borrow_mut().save_rules(rules).unwrap();
    }
    let orders = Orders::default();
    let mut h = harness();
    h.push(room_select(&env, room.clone(), "dance-single", &orders));
    h.run(0.5);
    assert_eq!(said(&h, "Players.Count"), "73 / 73");
    assert_eq!(said(&h, "Players0.Name"), "You", "the player's line first");
    assert!(
        h.texts().iter().any(|t| t.starts_with("▼ ") && t.ends_with(" more")),
        "{:?}",
        h.texts()
    );
    let [.., y1] = h.ui.bounds("RoomPlayers").unwrap();
    assert!(y1 <= 680.0, "{y1}");
    // The stage: the player 50th of 73.
    h.key("Enter", "Enter");
    h.run(0.3);
    let order = orders.borrow_mut().pop().expect("a prompt");
    assert_eq!(order.cpus.len(), 72);
    room.borrow_mut().set_ready(true).unwrap();
    room.borrow_mut().tick(4_000);
    let mut ids: Vec<_> = order.cpus.iter().map(|c| c.participant).collect();
    ids.insert(49, order.me);
    let ranking = ids
        .into_iter()
        .map(|participant| {
            vec![Finisher {
                participant,
                finish: Finish::Survived,
            }]
        })
        .collect();
    room.borrow_mut()
        .report_stage(order.stage, ranking, false)
        .unwrap();
    h.run(0.5);
    assert_eq!(top(&h), "room-standings");
    assert!(
        h.texts().iter().any(|t| t.starts_with("Stage 1: place 50 of 73 · +2 pts")),
        "{:?}",
        h.texts()
    );
    // Not an experimental rule any more.
    assert!(!h.texts().iter().any(|t| t.contains("experimental")), "{:?}", h.texts());
    // Nine lines at a time, the player's among them though far down.
    let lines: Vec<usize> = (0..73).filter(|k| there(&h, &format!("Name{k}"))).collect();
    assert_eq!(lines.len(), 9, "{lines:?}");
    let mine = *lines.last().unwrap();
    assert!(mine > 9, "{mine}");
    assert_eq!(said(&h, &format!("Name{mine}")), "You");
}

/// The prompt's players are in the room's order (by place once a stage
/// is confirmed), as the panel and the Standings have them, the local
/// player's line kept in view however far the column is scrolled
/// (2026-11-14: one ordering everywhere, never the player moved first).
#[test]
fn the_prompts_players_keep_the_rooms_order_and_the_player_in_view() {
    use ddi_room::{Finish, Finisher};
    let prompt = |room: &SharedRoom, h: &mut Harness| {
        let s = &library("dance-single").songs[0];
        room.borrow_mut().pick(s, &s.charts[0]).unwrap();
        let order = room.borrow_mut().take_stage_order().unwrap();
        let state = Rc::new(RefCell::new(PromptState::default()));
        h.push(
            PromptScreen::new(env(), state, Rc::new(|_, _| {}))
                .with_room(room.clone(), order.stage, shell(room, &Orders::default())),
        );
        h.run(0.5);
        order
    };
    // A room of four, the player last of stage 1: last in stage 2's list.
    let room: SharedRoom = Rc::new(RefCell::new(RoomClient::new(0, 1)));
    let rules = ddi_room::MatchRules {
        stages: 2,
        ..room.borrow().rules().clone()
    };
    room.borrow_mut().save_rules(rules).unwrap();
    let mut h = harness();
    let order = prompt(&room, &mut h);
    room.borrow_mut().set_ready(true).unwrap();
    room.borrow_mut().tick(4_000);
    let mut ids: Vec<_> = order.cpus.iter().map(|c| c.participant).collect();
    ids.push(order.me);
    let ranking = ids
        .into_iter()
        .map(|participant| vec![Finisher { participant, finish: Finish::Survived }])
        .collect();
    room.borrow_mut().report_stage(order.stage, ranking, true).unwrap();
    let mut h = harness();
    prompt(&room, &mut h);
    assert_eq!(said(&h, "Room.Player3.Name"), "You", "{:?}", h.texts());
    let first = order.cpus[0].name.clone().unwrap();
    assert_eq!(said(&h, "Room.Player0.Name"), first);
    // The Ready card's players part (`ready-card-v1`) at 1280×720: one
    // column for a few, more for many, every one of 24 in view; 73 in
    // two columns, the rest a scroll away (the wheel, its mark, Page
    // Down), the player's own line always in view.
    let column_count = |h: &Harness, n: usize| {
        let mut xs: Vec<i32> = (0..n)
            .filter(|k| there(h, &format!("Room.Player{k}.Name")))
            .map(|k| h.ui.bounds(&format!("Room.Player{k}.Name")).unwrap()[0].round() as i32)
            .collect();
        xs.sort_unstable();
        xs.dedup_by(|a, b| (*a - *b).abs() < 40);
        xs.len()
    };
    let shown = |h: &Harness, n: usize| (0..n).filter(|k| there(h, &format!("Room.Player{k}.Name"))).count();
    for (players, columns, all) in [(8_u16, 1, true), (24, 2, true), (73, 2, false)] {
        let room: SharedRoom = Rc::new(RefCell::new(RoomClient::new(0, 1)));
        let roster = crate::state::cpu_roster::at_level(&[], 4, usize::from(players) - 1, 3);
        let rules = crate::state::cpu_roster::with_roster(room.borrow().rules().clone(), roster, 1);
        room.borrow_mut().save_rules(rules).unwrap();
        let mut h = harness();
        prompt(&room, &mut h);
        let n = usize::from(players);
        assert_eq!(column_count(&h, n), columns, "{players}");
        let before = shown(&h, n);
        if all {
            assert_eq!(before, n, "{players}");
            continue;
        }
        assert!((24..n).contains(&before), "{before}");
        let mine = |h: &Harness| (0..n).find(|k| there(h, &format!("Room.Player{k}.Me"))).is_some();
        assert!(mine(&h));
        // Page Down: a column on; to the end by the wheel.
        h.key("PageDown", "PageDown");
        h.run(0.05);
        assert!(mine(&h), "the player's line kept");
        for _ in 0..30 {
            let at = h.centre("Room.Players");
            h.wheel(at, [0.0, 40.0], crate::event::WheelSource::Pixels);
            h.run(0.05);
        }
        assert!(there(&h, &format!("Room.Player{}.Name", n - 1)), "scrolled to the end");
        assert_eq!(shown(&h, n), before);
        assert!(mine(&h));
    }
}

/// A CPU without a roster name is "CPU n" in the language shown, on the
/// screens and in the debug view alike, never in English written in code.
#[test]
fn a_nameless_cpu_is_named_in_the_language_shown() {
    let mut room = RoomClient::development(0, 1, 3);
    let song = &library("dance-single").songs[0];
    room.pick(song, &song.charts[0]).unwrap();
    let s = room.state();
    let cpu = s.members.values().find(|m| m.is_cpu()).expect("a CPU seated");
    let ddi_room::MemberKind::Cpu { ordinal, opponent } = &cpu.kind else {
        unreachable!()
    };
    assert!(opponent.name.is_none());
    assert!(ddi_l10n::set_language("qps-ploc"));
    let words: String = ddi_l10n::msg::room::cpu_name(*ordinal).into();
    let (named, shown) = (super::room::member_name(cpu), super::room::person_name(cpu));
    ddi_l10n::set_language("en");
    assert_eq!((named.as_str(), shown.as_str()), (words.as_str(), words.as_str()));
    assert!(!words.starts_with("CPU"), "{words}");
}

/// What the text actor named `name` says.
fn said(h: &Harness, name: &str) -> String {
    match &h.actor(name).kind {
        ddi_actor::Kind::Text(t) => t.text().to_string(),
        _ => panic!("{name} is not text"),
    }
}

/// The texts in the header's pills, in order.
fn pills(h: &Harness) -> Vec<String> {
    fn texts(a: &ddi_actor::Actor, out: &mut Vec<String>) {
        match &a.kind {
            ddi_actor::Kind::Text(t) => out.push(t.text().to_string()),
            ddi_actor::Kind::Frame(children) => children.iter().for_each(|c| texts(c, out)),
            _ => {}
        }
    }
    (0..)
        .map_while(|i| h.ui.actor(&format!("Mod{i}")))
        .map(|pill| {
            let mut out = Vec::new();
            texts(pill, &mut out);
            out.join(" ")
        })
        .collect()
}

/// A room's header has singleplayer's pills (the player's own speed and
/// modifiers apply in a stage), but where singleplayer shows the player's
/// ruleset's options it shows the match's rules in their few words:
/// a room plays DDI under the match's rules, whatever the player's own.
#[test]
fn a_rooms_header_shows_the_matchs_rules_in_place_of_the_players_ruleset() {
    let env = env();
    let mut data = library("dance-single");
    data.mods = vec!["×2.5".into(), "Reverse".into()];
    data.rules = vec!["ITG".into(), "Fail off".into()];
    // Singleplayer: the player's own.
    let mut h = harness();
    h.push(super::select::SelectScreen::new(
        env.clone(),
        data.clone(),
        None,
        Rc::new(|_, _, _, _| {}),
        Rc::new(|_, _| {}),
    ));
    h.run(0.5);
    assert_eq!(pills(&h), ["×2.5", "Reverse", "ITG", "Fail off"]);
    // A room: the match's rules (the usual ones: "DDI"), then Hard
    // timing and Boos where the match has them.
    let room = RoomClient::shared(0, 1);
    room.borrow_mut().set_rules_seen(true);
    let orders = Orders::default();
    let mut h = harness();
    h.push(sample::room_select(&env, room.clone(), data.clone()).with_room(room.clone(), shell(&room, &orders)));
    h.run(0.5);
    assert_eq!(pills(&h), ["×2.5", "Reverse", "DDI"]);
    let mut rules = room.borrow().rules().clone();
    rules.difficulty = ddi_room::MatchDifficulty::Hard;
    rules.boos = true;
    room.borrow_mut().save_rules(rules).unwrap();
    let mut h = harness();
    h.push(sample::room_select(&env, room.clone(), data).with_room(room.clone(), shell(&room, &orders)));
    h.run(0.5);
    assert_eq!(pills(&h), ["×2.5", "Reverse", "DDI · Hard timing · Boos"]);
}

/// Feedback round B: the players are always on the room's song select.
/// The wide layout has the song's side short (a row of chart buttons, no
/// ladder, the style button only where there is a style to change to) and
/// the players panel under it: nine rows without scrolling, You marked, a
/// CPU's stars, what each is doing, and, once a stage is confirmed, the
/// place and the match points in the match's order.
#[test]
fn the_players_are_always_beside_the_song() {
    use crate::stars::{fills, shown};
    use ddi_room::{Finish, Finisher};
    let env = env();
    let room = RoomClient::shared(0, 1);
    room.borrow_mut().set_rules_seen(true);
    let orders = Orders::default();
    let mut h = harness();
    h.push(room_select(&env, room.clone(), "dance-single", &orders));
    h.run(0.5);
    press(&mut h, "Room.Rules");
    for _ in 0..5 {
        press(&mut h, "AddRandom");
    }
    press(&mut h, "Apply");
    assert_eq!(top(&h), "select");
    assert_eq!(view(&env).cpus, 8);
    // The panel: beside the song, clear of the list and the footer.
    let [x0, y0, x1, y1] = h.ui.bounds("RoomPlayers").expect("the panel");
    let info = h.ui.bounds("Info").expect("the song");
    assert!(
        x0 >= 32.0 && x1 <= 600.0 && y0 >= info[3] && y1 <= 680.0,
        "{x0} {y0} {x1} {y1}"
    );
    assert_eq!(said(&h, "Players.Count"), "9 / 73");
    assert!(!there(&h, "Players.More"), "nine fit without scrolling");
    assert_eq!(said(&h, "Players0.Name"), "You");
    assert!(there(&h, "Players0.Me"), "You is marked");
    let roster = room.borrow().rules().cpu.roster.clone();
    assert_eq!(roster.len(), 8);
    for (k, spec) in roster.iter().enumerate() {
        let id = format!("Players{}", k + 1);
        assert_eq!(said(&h, &format!("{id}.Name")), spec.name.clone().unwrap());
        assert_eq!(
            shown(h.actor(&format!("{id}.Stars"))),
            fills(spec.level + 1),
            "{id}"
        );
        assert_eq!(said(&h, &format!("{id}.State")), "At start");
        assert!(
            !there(&h, &format!("{id}.Place")),
            "no place before a stage"
        );
        assert!(!there(&h, &format!("{id}.Points")));
    }
    assert!(!there(&h, "Players.Points"));
    // The song's side: chart buttons, no ladder, no tabs; one playable
    // style, so no style button.
    assert!(there(&h, "Chart0") && !there(&h, "Ladder.Row0") && !there(&h, "Tab0"));
    assert!(!h.actor("StyleButton").visible);
    assert!(has(&h, "HARD 9"), "{:?}", h.texts());
    // A row opens the full list (the pop-up stays for the picks).
    press(&mut h, "Players3.Name");
    assert_eq!(top(&h), "room-players");
    h.key("Escape", "Escape");
    h.run(0.5);
    // A stage: the local player second.
    h.key("Enter", "Enter");
    h.run(0.3);
    let order = orders.borrow_mut().pop().expect("a prompt");
    // Ready, on the panel, while the stage gathers.
    room.borrow_mut().set_ready(true).unwrap();
    h.run(0.3);
    assert_eq!(said(&h, "Players0.State"), "Ready");
    room.borrow_mut().tick(20_000);
    let mut ids: Vec<_> = order.cpus.iter().map(|c| c.participant).collect();
    ids.insert(1, order.me);
    let first = ids[0];
    let ranking = ids
        .into_iter()
        .map(|participant| {
            vec![Finisher {
                participant,
                finish: Finish::Survived,
            }]
        })
        .collect();
    room.borrow_mut()
        .report_stage(order.stage, ranking, false)
        .unwrap();
    h.run(0.5);
    press(&mut h, "Continue");
    assert_eq!(top(&h), "select");
    // By place now, with the points: the winner, then You.
    let winner = order
        .cpus
        .iter()
        .find(|c| c.participant == first)
        .and_then(|c| c.name.clone())
        .unwrap();
    assert_eq!(said(&h, "Players.Points"), "Points");
    assert_eq!(
        (said(&h, "Players0.Place"), said(&h, "Players0.Name")),
        ("1".to_string(), winner)
    );
    assert_eq!(
        (said(&h, "Players1.Place"), said(&h, "Players1.Name")),
        ("2".to_string(), "You".to_string())
    );
    assert!(there(&h, "Players1.Me") && !there(&h, "Players0.Me"));
    assert_eq!(said(&h, "Players0.Points"), "12");
    assert_eq!(said(&h, "Players1.Points"), "9");
    // Nine starters: the top-biased table, last earning 1.
    assert_eq!(said(&h, "Players8.Points"), "1");
}

/// Past what fits (a development practice's field), the panel counts the
/// rest on a line that opens the full list.
#[test]
fn the_players_panel_counts_those_that_do_not_fit() {
    let env = env();
    let room = Rc::new(RefCell::new(RoomClient::development(0, 1, 20)));
    room.borrow_mut().set_rules_seen(true);
    let orders = Orders::default();
    let mut h = harness();
    h.push(room_select(&env, room.clone(), "dance-single", &orders));
    h.run(0.5);
    let rows = super::room::player_rows(&room.borrow()).len();
    assert!(rows > 13, "{rows}");
    let shown = (0..rows)
        .filter(|k| there(&h, &format!("Players{k}.Name")))
        .count();
    assert!(shown >= 9 && shown < rows, "{shown} of {rows}");
    assert_eq!(
        h.texts()
            .iter()
            .filter(|t| **t == format!("▼ {} more", rows - shown))
            .count(),
        1,
        "{:?}",
        h.texts()
    );
    let [.., y1] = h.ui.bounds("RoomPlayers").unwrap();
    assert!(y1 <= 680.0, "{y1}");
    press(&mut h, "Players.More");
    assert_eq!(top(&h), "room-players");
}

/// A phone has no room for the panel beside the song and the list: the
/// host's players are one line over the list (their initials, You whole,
/// how many they are), and a tap on it opens the full list.
#[test]
fn a_phone_shows_the_players_as_a_strip_that_opens_the_list() {
    let env = env();
    let room = RoomClient::shared(0, 1);
    room.borrow_mut().set_rules_seen(true);
    let orders = Orders::default();
    for size in [[844.0, 390.0], [390.0, 844.0]] {
        let mut h = Harness::new(super::COMPACT, size, 1.0, TextMode::Own);
        h.push(room_select(&env, room.clone(), "dance-single", &orders));
        h.run(0.5);
        assert!(!there(&h, "Players0.Name"), "no panel on a phone");
        let [x0, y0, x1, y1] = h.ui.bounds("Players.Strip").expect("the strip");
        let [x0, y0] = h.css([x0, y0]);
        let [x1, y1] = h.css([x1, y1]);
        assert!(
            x0 >= 0.0 && y0 >= 0.0 && x1 <= size[0] && y1 <= size[1],
            "{size:?}: {x0} {y0} {x1} {y1}"
        );
        // Clear of the list under it and the status over it.
        let wheel = h.ui.visible_bounds("Wheel").expect("the list");
        let strip = h.ui.bounds("Players.Strip").unwrap();
        assert!(strip[3] <= wheel[1] + 0.5, "{strip:?} {wheel:?}");
        let texts = h.texts();
        assert!(has(&h, "You") && has(&h, "4 / 73"), "{texts:?}");
        let roster = room.borrow().rules().cpu.roster.clone();
        for spec in &roster {
            let name = spec.name.clone().unwrap();
            let initials: String = name
                .split_whitespace()
                .filter_map(|w| w.chars().next())
                .take(2)
                .collect::<String>()
                .to_uppercase();
            assert!(has(&h, &initials), "{initials}: {texts:?}");
        }
        h.tap(PointerKind::Touch, "Players.Strip");
        h.run(0.5);
        assert_eq!(top(&h), "room-players", "{size:?}");
        h.key("Escape", "Escape");
        h.run(0.5);
        assert_eq!(top(&h), "select");
    }
}

#[test]
fn the_room_fits_a_phone_and_more_holds_rules_and_players() {
    let env = env();
    let room = RoomClient::shared(0, 1);
    room.borrow_mut().set_rules_seen(true);
    let orders = Orders::default();
    for (size, design, compact) in [
        ([844.0, 390.0], super::COMPACT, true),
        ([1280.0, 720.0], DESIGN, false),
    ] {
        let mut h = Harness::new(design, size, 1.0, TextMode::Own);
        h.push(room_select(&env, room.clone(), "dance-single", &orders));
        h.run(0.5);
        let names: &[&str] = if compact {
            &["More", "Back"]
        } else {
            &["Room.Rules", "More", "Back"]
        };
        for name in names {
            let [x0, y0, x1, y1] = h.ui.bounds(name).unwrap_or_else(|| panic!("{name}"));
            let [x0, y0] = h.css([x0, y0]);
            let [x1, y1] = h.css([x1, y1]);
            assert!(
                x0 >= 0.0 && y0 >= 0.0 && x1 <= size[0] && y1 <= size[1],
                "{name} {size:?}"
            );
        }
        // Controller: More opens the room's More.
        h.pad(crate::nav::MenuAction::More, true);
        h.pad(crate::nav::MenuAction::More, false);
        h.run(0.3);
        assert!(
            has(&h, "Leave room") && has(&h, "Players"),
            "{:?}",
            h.texts()
        );
        h.key("Escape", "Escape");
        h.run(0.3);
        assert_eq!(top(&h), "select");
        // The players' pop-up: the picks, then the players.
        if !compact {
            open_players(&mut h);
            assert_eq!(top(&h), "room-players");
            assert!(has(&h, "Picks") && has(&h, "You"), "{:?}", h.texts());
            h.key("Escape", "Escape");
            h.run(0.5);
            assert_eq!(top(&h), "select", "Back closes it");
            open_players(&mut h);
            h.key("Enter", "Enter");
            h.run(0.5);
            assert_eq!(top(&h), "select", "Enter closes it");
            let before = view(&env).revision;
            h.key("Enter", "Enter");
            h.run(0.5);
            assert_ne!(view(&env).phase, "lobby", "{before}");
        }
    }
}

#[test]
fn a_stages_results_go_back_to_the_room_and_never_retry() {
    use super::data::ResultsKind;
    use super::results::{ResultsActions, ResultsScreen};
    let env = env();
    let mut data = sample::results();
    data.kind = ResultsKind::Stage;
    let back = Rc::new(RefCell::new(0));
    let retried = Rc::new(RefCell::new(0));
    let (b, r) = (back.clone(), retried.clone());
    let mut h = harness();
    h.push(ResultsScreen::new(
        env.clone(),
        data,
        ResultsActions {
            retry: Some(Rc::new(move |_, _| *r.borrow_mut() += 1)),
            song_list: Some(Rc::new(move |_, _| *b.borrow_mut() += 1)),
            replay: None,
            offset: None,
        },
    ));
    h.run(1.0);
    let texts = h.texts();
    assert!(texts.iter().any(|t| t == "Back to room"), "{texts:?}");
    assert!(!texts.iter().any(|t| t == "Retry"), "{texts:?}");
    h.key("r", "KeyR");
    h.run(0.2);
    assert_eq!(*retried.borrow(), 0, "a stage is played once");
    press(&mut h, "SongList");
    assert_eq!(*back.borrow(), 1);
}

/// An online room's host whose screen is built before the room's first
/// snapshot comes (a real server's latency: the role is not known yet, so
/// they are a follower for a moment) has their own songs to pick from
/// once it comes.
#[test]
fn an_online_rooms_host_has_their_songs_though_the_room_answers_late() {
    use crate::state::online::tests::room::{client, person, song, tick};
    use ddi_room::fake::FakeServer;
    use ddi_room::{Identity, ManualClock, RoomPolicy};
    let clock = ManualClock::new(1_000);
    let server = FakeServer::hosted(
        "Room000001",
        Identity::new("dev:alice"),
        "alice",
        RoomPolicy::default(),
        clock.clone(),
    )
    .shared();
    let alice = Rc::new(RefCell::new(client(
        &server,
        person(&server, "alice"),
        "alice",
        1_000,
    )));
    // The room's first answer (its snapshot) is a second on its way.
    server
        .borrow_mut()
        .fault(ddi_room::ParticipantId(1), ddi_room::fake::Fault::Delay(1_000));
    let ea = env();
    let orders = Orders::default();
    let uploads = Uploads::default();
    let mut data = library("dance-single");
    data.songs.insert(0, song());
    let mut ha = harness();
    ha.push(
        sample::room_select(&ea, alice.clone(), data)
            .with_room(alice.clone(), shell_with(&alice, &orders, &uploads)),
    );
    // Frames before anything came from the room: nobody's role is known.
    tick(&clock, &mut [&mut alice.borrow_mut()], 200);
    ha.run(0.3);
    assert!(!view(&ea).host, "not known as the host yet");
    // The snapshot comes: the host picks, from their own list.
    for _ in 0..4 {
        tick(&clock, &mut [&mut alice.borrow_mut()], 400);
        ha.run(0.3);
    }
    assert!(view(&ea).host);
    assert!(has(&ha, "Single Song"), "{:?}", ha.texts());
    assert!(ha.ui.actor("Wheel").is_some_and(|w| w.visible), "the list shows");
}

/// An online room over the fake room: the host's song select and the
/// member's following one; the member's Rules read only; the host's
/// resting song shown to the member (its card, then the song once read
/// from the share), the picks and players in the member's panel; the
/// link's states on the status line.
#[test]
fn an_online_room_shows_the_host_and_the_follower_and_its_link() {
    use crate::state::online::tests::room::{client, person, song, tick};
    use ddi_room::fake::{FakeServer, Fault};
    use ddi_room::{Identity, Intent, ManualClock, ParticipantId, RoomPolicy};
    let clock = ManualClock::new(1_000);
    let server = FakeServer::hosted(
        "Room000001",
        Identity::new("dev:alice"),
        "alice",
        RoomPolicy::default(),
        clock.clone(),
    )
    .shared();
    server
        .borrow_mut()
        .join(Identity::new("dev:bob"), "bob", Intent::Play)
        .unwrap();
    let alice = Rc::new(RefCell::new(client(
        &server,
        person(&server, "alice"),
        "alice",
        1_000,
    )));
    let bob = Rc::new(RefCell::new(client(
        &server,
        person(&server, "bob"),
        "bob",
        1_000,
    )));
    let step = |ms: u64| {
        tick(
            &clock,
            &mut [&mut alice.borrow_mut(), &mut bob.borrow_mut()],
            ms,
        )
    };
    let (ea, eb) = (env(), env());
    let orders = Orders::default();
    let mut data = library("dance-single");
    data.songs.insert(0, song());
    let uploads = Uploads::default();
    let mut ha = harness();
    ha.push(
        sample::room_select(&ea, alice.clone(), data)
            .with_room(alice.clone(), shell_with(&alice, &orders, &uploads)),
    );
    let mut hb = harness();
    hb.push(
        sample::room_select(&eb, bob.clone(), SelectData::default())
            .with_room(bob.clone(), shell(&bob, &orders)),
    );
    step(400);
    step(400);
    ha.run(0.3);
    hb.run(0.3);
    // Both open on song select, no Rules by themselves. From their
    // button, Bob's are read only (Close alone), Alice's hers to change.
    assert_eq!((top(&ha), top(&hb)), ("select".into(), "select".into()));
    press(&mut hb, "Room.Rules");
    assert_eq!(top(&hb), "room-rules");
    assert!(has(&hb, "Close") && !has(&hb, "Apply"), "{:?}", hb.texts());
    assert_eq!(said(&hb, "Rule.difficulty.Value"), "Normal", "a member reads them");
    assert!(!there(&hb, "Inc.difficulty"), "and changes nothing");
    hb.key("Escape", "Escape");
    hb.run(0.5);
    press(&mut ha, "Room.Rules");
    assert!(has(&ha, "Apply"), "{:?}", ha.texts());
    // An online room's rules: DDI's difficulty and Boos are the host's to
    // set, through the room; Attacks are not offered where they would do
    // nothing yet (no ledger from the members' reports, no shocks
    // delivered to their plays), said plainly.
    assert!(!view(&ea).attacks_offered);
    assert!(!there(&ha, "Inc.attacks") && !there(&ha, "Dec.attacks"));
    assert_eq!(said(&ha, "Rule.attacks.Value"), "Off · not in online rooms yet");
    press(&mut ha, "Inc.difficulty");
    press(&mut ha, "Inc.boos");
    press(&mut ha, "Apply");
    step(200);
    step(200);
    ha.run(0.3);
    hb.run(0.3);
    for v in [view(&ea), view(&eb)] {
        assert_eq!((v.difficulty.as_str(), v.boos, v.attacks), ("hard", true, false));
    }
    press(&mut ha, "Room.Rules");
    press(&mut ha, "Dec.difficulty");
    press(&mut ha, "Dec.boos");
    press(&mut ha, "Apply");
    step(200);
    step(200);
    ha.run(0.3);
    hb.run(0.3);
    assert_eq!(view(&eb).difficulty, "normal");
    assert_eq!((top(&ha), top(&hb)), ("select".into(), "select".into()));
    let (a, b) = (view(&ea), view(&eb));
    let online = a.online.as_ref().unwrap();
    assert_eq!(
        (online.link.as_str(), online.role.as_str()),
        ("connected", "owner")
    );
    assert!(a.host && !b.host);
    assert_eq!(b.status, "alice is choosing the first song.");
    assert!(online.can_invite && online.can_rotate);
    assert!(!b.online.as_ref().unwrap().can_invite);
    assert!(has(&ha, "AFTER HOURS"), "{:?}", ha.texts());
    // Bob's panel: the picks, the players (the host's role).
    assert!(has(&hb, "Picks") && has(&hb, "Host · Picking"), "{:?}", hb.texts());
    // Alice rests on her first song: Bob sees its card, then the song.
    for _ in 0..8 {
        step(500);
        ha.run(0.1);
    }
    hb.run(0.3);
    assert_eq!(view(&eb).spotlight.as_deref(), Some("A"));
    assert!(has(&hb, "A"), "{:?}", hb.texts());
    // Its upload (the shell's), finished as the server does.
    let job = uploads.borrow_mut().pop().expect("the spotlight's upload");
    assert_eq!(job.stage, crate::state::room::SPOTLIGHT);
    server
        .borrow_mut()
        .authority(ddi_room::Command::SetSpotlightShare {
            chart: job.content.chart.clone(),
            share: "abc12345".into(),
        })
        .unwrap();
    alice
        .borrow_mut()
        .upload_finished(job.stage, Ok("abc12345".into()));
    step(400);
    hb.run(0.3);
    step(400);
    hb.run(0.3);
    assert_eq!(
        view(&eb).spotlight_song.as_deref(),
        Some("shared:abc12345:0")
    );
    // Bob's Enter picks nothing.
    hb.key("Enter", "Enter");
    hb.run(0.3);
    assert_eq!(view(&eb).phase, "lobby");
    // Bob's socket drops: his row says Reconnecting to Alice.
    server.borrow_mut().drop_socket(ParticipantId(2));
    step(100);
    hb.run(0.3);
    assert_eq!(view(&eb).status, "Connection lost. Reconnecting…");
    open_players(&mut ha);
    assert!(has(&ha, "Reconnecting"), "{:?}", ha.texts());
    press(&mut ha, "Close");
    step(1_000);
    hb.run(0.3);
    assert_eq!(view(&eb).online.unwrap().link, "connected");
    // 4401: Sign in again, offered in More.
    // The session gone (feedback round AA: a 4401 alone asks for a
    // ticket first, and only a refused one ends the link).
    server
        .borrow_mut()
        .sign_out(&ddi_room::Identity::new("dev:alice"));
    server.borrow_mut().fault(
        ParticipantId(1),
        Fault::Close(ddi_room::online::close::SIGN_IN_AGAIN),
    );
    step(100);
    ha.run(0.3);
    assert_eq!(
        view(&ea).status,
        "Your sign-in ended. Sign in again to return to the room."
    );
    press(&mut ha, "More");
    assert!(has(&ha, "Sign in again"), "{:?}", ha.texts());
}

/// The players panel's lines in an online room, each state in words: the
/// host's role, watching, not ready, ready, loading, why a song did not
/// load, reconnecting, playing and sitting out; the follower's song select
/// shows the same panel beside the song, and the picks in the list's
/// place.
#[test]
fn the_panels_lines_say_what_each_player_is_doing() {
    use super::room::{PlayerRow, Tone, player_rows};
    use crate::state::online::tests::room::{client, person, song, tick, upload};
    use crate::state::room::LoadProblem;
    use ddi_room::fake::FakeServer;
    use ddi_room::{Identity, Intent, ManualClock, ParticipantId, RoomPolicy};
    let clock = ManualClock::new(1_000);
    let server = FakeServer::hosted(
        "Room000001",
        Identity::new("dev:alice"),
        "alice",
        RoomPolicy::default(),
        clock.clone(),
    )
    .shared();
    for (who, intent) in [
        ("bob", Intent::Play),
        ("carol", Intent::Play),
        ("dave", Intent::Watch),
    ] {
        server
            .borrow_mut()
            .join(Identity::new(format!("dev:{who}")), who, intent)
            .unwrap();
    }
    let alice = Rc::new(RefCell::new(client(
        &server,
        person(&server, "alice"),
        "alice",
        1_000,
    )));
    let bob = Rc::new(RefCell::new(client(
        &server,
        person(&server, "bob"),
        "bob",
        1_000,
    )));
    let mut carol = client(&server, person(&server, "carol"), "carol", 1_000);
    let mut dave = client(&server, person(&server, "dave"), "dave", 1_000);
    let mut step = |carol: &mut RoomClient, ms: u64| {
        tick(
            &clock,
            &mut [
                &mut alice.borrow_mut(),
                &mut bob.borrow_mut(),
                carol,
                &mut dave,
            ],
            ms,
        )
    };
    step(&mut carol, 400);
    step(&mut carol, 400);
    let row = |room: &SharedRoom, name: &str| -> PlayerRow {
        player_rows(&room.borrow())
            .into_iter()
            .find(|p| p.name == name)
            .unwrap_or_else(|| panic!("no row {name}"))
    };
    let says = |room: &SharedRoom, name: &str| {
        let p = row(room, name);
        (p.status, p.tone)
    };
    // Before a pick: the host's role, the one watching.
    let host = row(&bob, "alice");
    assert_eq!(
        (host.role.as_str(), host.status.as_str()),
        ("Host · Picking", "")
    );
    // Alice's own row: her signed-in name with the "You" badge.
    let mine = row(&alice, "alice");
    assert!(mine.me && mine.badge);
    assert!(!row(&bob, "alice").badge);
    assert_eq!(says(&alice, "dave"), ("Watching".into(), Tone::Plain));
    assert_eq!(row(&alice, "bob").place, None);
    // The pick: its song shared, then everyone prepares it.
    let s = song();
    alice.borrow_mut().pick(&s, &s.charts[0]).unwrap();
    step(&mut carol, 400);
    upload(&server, &mut alice.borrow_mut(), "abc12345");
    step(&mut carol, 400);
    // Not loaded yet: Loading (`hosted-deadline-v2`: everyone sees who
    // is loading).
    assert_eq!(says(&alice, "bob"), ("Loading".into(), Tone::Plain));
    let a = alice.borrow_mut().take_load_jobs();
    alice.borrow_mut().finish_load(&a[0], Ok(()));
    alice.borrow_mut().set_ready(true).unwrap();
    // Bob is ready and still loading; Carol's download failed.
    let b = bob.borrow_mut().take_load_jobs();
    bob.borrow_mut().set_ready(true).unwrap();
    // (Round P1b: a failed download is tried again twice by itself, the
    // room hearing "loading" meanwhile; the third failure is reported.)
    for (i, wait) in crate::state::room::LOAD_RETRY_V0_MS
        .into_iter()
        .chain([400])
        .enumerate()
    {
        let c = carol.take_load_jobs();
        carol.finish_shared_load(&c[0], Err(LoadProblem::Download));
        if i < 2 {
            step(&mut carol, 400);
            assert_eq!(says(&alice, "carol"), ("Loading".into(), Tone::Plain));
        }
        step(&mut carol, wait);
    }
    assert_eq!(says(&bob, "alice"), ("Ready".into(), Tone::Good));
    assert_eq!(says(&alice, "bob"), ("Loading".into(), Tone::Plain));
    assert_eq!(
        says(&alice, "carol"),
        ("Download failed".into(), Tone::Failed)
    );
    // The same panel for the host and the follower, beside the song; the
    // follower's list's place holds the picks.
    let (ea, eb) = (env(), env());
    let orders = Orders::default();
    alice.borrow_mut().set_rules_seen(true);
    bob.borrow_mut().set_rules_seen(true);
    let mut data = library("dance-single");
    data.songs.insert(0, s.clone());
    let mut ha = harness();
    ha.push(
        sample::room_select(&ea, alice.clone(), data)
            .with_room(alice.clone(), shell(&alice, &orders)),
    );
    let mut hb = harness();
    hb.push(
        sample::room_select(&eb, bob.clone(), SelectData::default())
            .with_room(bob.clone(), shell(&bob, &orders)),
    );
    ha.run(0.4);
    hb.run(0.4);
    // Each one's own row: their name with the "You" badge (alice's the
    // first line, bob's the second).
    for (h, first, other, state, at, own) in [
        (&ha, "alice", "bob", "Loading", 1, 0),
        (&hb, "alice", "bob", "Ready", 0, 1),
    ] {
        // Dave watches: three of them play (fix round G1, ui-screens-4).
        assert_eq!(said(h, "Players.Count"), "3 / 73");
        assert_eq!(said(h, "Players0.Name"), first);
        assert_eq!(said(h, "Players0.Role"), "Host · Picking");
        assert_eq!(said(h, "Players1.Name"), other);
        assert_eq!(said(h, &format!("Players{at}.State")), state);
        assert_eq!(said(h, &format!("Players{own}.Name.BadgeText")), "You");
        super::test_kit::assert_badge_on_line(h, None, &format!("Players{own}.Name"), 15.0);
        assert!(!there(h, &format!("Players{}.Name.Badge", 1 - own)));
        assert_eq!(said(h, "Players2.State"), "Download failed");
        assert_eq!(said(h, "Players3.State"), "Watching");
        assert_eq!(ha.ui.bounds("RoomPlayers"), h.ui.bounds("RoomPlayers"));
    }
    assert!(has(&hb, "Picks"), "{:?}", hb.texts());
    assert_eq!(
        hb.texts().iter().filter(|t| *t == "Players").count(),
        1,
        "the panel's heading (no button since follow-up round F): {:?}",
        hb.texts()
    );
    // Bob's socket drops: reconnecting, his line dimmed.
    server.borrow_mut().drop_socket(ParticipantId(2));
    step(&mut carol, 100);
    assert_eq!(says(&alice, "bob"), ("Reconnecting".into(), Tone::Away));
    step(&mut carol, 2_000);
    bob.borrow_mut().finish_shared_load(
        &b[0],
        Ok(crate::state::room::Picked {
            song: crate::screens::data::SongEntry {
                id: "shared:abc12345:0".into(),
                ..s.clone()
            },
            chart: s.charts[0].clone(),
        }),
    );
    // The deadline starts the stage with the two ready: playing; Carol
    // sits it out.
    step(&mut carol, 30_000);
    step(&mut carol, 4_000);
    assert_eq!(says(&alice, "bob"), ("Playing".into(), Tone::Plain));
    // (With why: her song did not load.)
    assert_eq!(
        says(&alice, "carol"),
        ("Sat out: load failed".into(), Tone::Plain)
    );
}

/// Feedback round N1: the room's order of a player's states for a stage,
/// the same in the row's state and the panel's line: nothing before a
/// pick; Waiting for song once it is picked and until their device says it
/// is loading (never "Not ready" there); Loading; Not ready once loaded;
/// Ready; then Playing, or that they sit it out and why.
#[test]
fn a_players_states_go_in_the_rooms_order() {
    use crate::screens::room::{player_rows, status};
    use crate::state::online::tests::room::{client, person, song, tick, upload};
    use ddi_room::fake::FakeServer;
    use ddi_room::{Identity, Intent, ManualClock, RoomPolicy};
    let clock = ManualClock::new(1_000);
    let server = FakeServer::hosted(
        "Room000001",
        Identity::new("dev:alice"),
        "alice",
        RoomPolicy::default(),
        clock.clone(),
    )
    .shared();
    for who in ["bob", "carol"] {
        server
            .borrow_mut()
            .join(Identity::new(format!("dev:{who}")), who, Intent::Play)
            .unwrap();
    }
    let mut alice = client(&server, person(&server, "alice"), "alice", 1_000);
    let mut bob = client(&server, person(&server, "bob"), "bob", 1_000);
    let mut carol = client(&server, person(&server, "carol"), "carol", 1_000);
    macro_rules! step {
        ($ms:expr) => {
            tick(&clock, &mut [&mut alice, &mut bob, &mut carol], $ms)
        };
    }
    step!(400);
    step!(400);
    // Bob's row as Alice's lists have it: the row's state and the panel's
    // line.
    let says = |room: &RoomClient, name: &str| {
        let p = player_rows(room)
            .into_iter()
            .find(|p| p.name == name)
            .unwrap_or_else(|| panic!("no row {name}"));
        (p.state, p.status)
    };
    let both = |word: &str| (word.to_string(), word.to_string());
    // 1. Nothing picked: nothing said of readiness.
    assert_eq!(says(&alice, "bob"), both(""));
    // 2. Picked, the song on its way to the room: waiting for it.
    let s = song();
    alice.pick(&s, &s.charts[0]).unwrap();
    step!(400);
    assert!(alice.state().content_pending(alice.stages()[0].id));
    assert_eq!(says(&alice, "bob"), both("Waiting for song"));
    assert_eq!(says(&bob, "carol"), both("Waiting for song"));
    // 3. There, and their devices say they load it.
    upload(&server, &mut alice, "abc12345");
    step!(400);
    assert_eq!(says(&alice, "bob"), both("Loading"));
    // 4. Loaded: the Ready screen is theirs to press.
    for j in alice.take_load_jobs() {
        alice.finish_load(&j, Ok(()));
    }
    for j in bob.take_load_jobs() {
        bob.finish_shared_load(&j, Ok(crate::state::online::tests::room::shared_pick(&s)));
    }
    let slow = carol.take_load_jobs();
    step!(400);
    assert_eq!(says(&alice, "bob"), both("Not ready"));
    assert_eq!(says(&alice, "carol"), both("Loading"));
    // 5. Ready.
    bob.set_ready(true).unwrap();
    alice.set_ready(true).unwrap();
    step!(400);
    assert_eq!(says(&alice, "bob"), both("Ready"));
    // 6. The stage starts without Carol, still loading: said with why, in
    // the lists and on her own status line.
    step!(60_000);
    step!(30_000);
    step!(4_000);
    assert_eq!(says(&alice, "bob"), both("Playing"));
    assert_eq!(says(&alice, "carol"), both("Sat out: loading"));
    assert_eq!(
        String::from(status(&carol)),
        "Stage 1's song was still loading here when it started. You play the next one."
    );
    drop(slow);
}

/// M7d: a bot (`bot:<name>`) is a person's seat labeled Bot wherever the
/// players are listed (the room's panel and Players, the prompt's rows,
/// which share `player_rows`; the standings; an invite's host), and in the
/// debug state.
#[test]
fn bots_are_labeled_wherever_players_are_listed() {
    use crate::state::online::tests::room::{client, person, tick};
    use ddi_room::fake::FakeServer;
    use ddi_room::{Identity, Intent, ManualClock, RoomPolicy};
    let clock = ManualClock::new(1_000);
    let server = FakeServer::hosted(
        "Room000001",
        Identity::new("dev:alice"),
        "alice",
        RoomPolicy::default(),
        clock.clone(),
    )
    .shared();
    server
        .borrow_mut()
        .join(Identity::bot("lumen-kite"), "Lumen Kite", Intent::Play)
        .unwrap();
    let alice = Rc::new(RefCell::new(client(
        &server,
        person(&server, "alice"),
        "alice",
        1_000,
    )));
    let ea = env();
    let orders = Orders::default();
    let mut ha = harness();
    ha.push(
        sample::room_select(&ea, alice.clone(), library("dance-single"))
            .with_room(alice.clone(), shell(&alice, &orders)),
    );
    tick(&clock, &mut [&mut alice.borrow_mut()], 400);
    tick(&clock, &mut [&mut alice.borrow_mut()], 400);
    ha.run(0.3);
    assert_eq!(top(&ha), "select", "no Rules by themselves");
    ha.run(0.3);
    let a = view(&ea);
    let bot = a
        .members
        .iter()
        .find(|m| m.name == "Lumen Kite")
        .expect("the bot in the debug state");
    assert!(bot.bot && !bot.cpu && !bot.admin, "{bot:?}");
    assert!(!a.members.iter().find(|m| m.name == "alice").unwrap().bot);
    let rows = super::room::player_rows(&alice.borrow());
    assert!(
        rows.iter().any(|r| r.name == "Lumen Kite (Bot)" && !r.cpu),
        "{:?}",
        rows.iter().map(|r| &r.name).collect::<Vec<_>>()
    );
    let member = alice
        .borrow()
        .state()
        .member_by_identity(&Identity::bot("lumen-kite"))
        .cloned()
        .unwrap();
    assert_eq!(super::room::person_name(&member), "Lumen Kite (Bot)");
    assert_eq!(super::room::shown_name("Alice", false), "Alice");
    assert_eq!(
        String::from(ddi_l10n::msg::room::invite_host(super::room::shown_name(
            "Lumen Kite",
            true
        ))),
        "Hosted by Lumen Kite (Bot)"
    );
}

/// Match points show as the language writes a number, the room keeping
/// the exact fraction: a tied place's share is 1.5, not "3/2".
#[test]
fn points_show_as_decimals() {
    use super::room::points_text;
    use ddi_room::Points;
    assert_eq!(points_text(Points::ratio(3, 2)).as_str(), "1.5");
    assert_eq!(points_text(Points::ratio(28, 3)).as_str(), "9.33");
    assert_eq!(points_text(Points::whole(12)).as_str(), "12");
    assert_eq!(points_text(Points::ZERO).as_str(), "0");
}

/// An online room over the fake room with alice (its owner), bob and
/// carol in it, each with a client; bob a co-host where `co_host`.
pub(super) struct Three {
    pub(super) clock: ddi_room::ManualClock,
    pub(super) server: ddi_room::fake::SharedFake,
    pub(super) alice: SharedRoom,
    pub(super) bob: SharedRoom,
    pub(super) carol: SharedRoom,
}

impl Three {
    pub(super) fn new(co_host: bool) -> Three {
        Three::over(|_| {}, co_host)
    }

    /// [`Three::new`] over the room's state as `change` leaves it.
    fn over(change: impl FnOnce(&mut ddi_room::RoomState), co_host: bool) -> Three {
        use crate::state::online::tests::room::{client, person};
        use ddi_room::fake::FakeServer;
        use ddi_room::{Command, Connection, Identity, Intent, ParticipantId, RoomPolicy};
        let clock = ddi_room::ManualClock::new(1_000);
        let mut state = ddi_room::RoomState::new(
            ddi_room::RoomId::new("Room000001"),
            Identity::new("dev:alice"),
            "alice".into(),
            RoomPolicy::default(),
            ddi_room::MatchRules::hosted(),
            ddi_room::ContentSource::Shared,
        );
        change(&mut state);
        let mut fake = FakeServer::with_state(state, clock.clone());
        fake.authority(Command::SetConnection {
            participant: ParticipantId(1),
            connection: Connection::Joining,
        })
        .unwrap();
        let server = fake.shared();
        for who in ["bob", "carol"] {
            server
                .borrow_mut()
                .join(Identity::new(format!("dev:{who}")), who, Intent::Play)
                .unwrap();
        }
        if co_host {
            server
                .borrow_mut()
                .authority(Command::SetCoHost {
                    participant: ParticipantId(2),
                    on: true,
                })
                .unwrap();
        }
        let room = |who: &'static str| {
            Rc::new(RefCell::new(client(
                &server,
                person(&server, who),
                who,
                1_000,
            )))
        };
        let (alice, bob, carol) = (room("alice"), room("bob"), room("carol"));
        Three {
            clock,
            server,
            alice,
            bob,
            carol,
        }
    }

    /// Time passes for the three clients.
    pub(super) fn step(&self, ms: u64) {
        crate::state::online::tests::room::tick(
            &self.clock,
            &mut [
                &mut self.alice.borrow_mut(),
                &mut self.bob.borrow_mut(),
                &mut self.carol.borrow_mut(),
            ],
            ms,
        );
    }

    /// Time passes and every screen runs.
    pub(super) fn settle(&self, ms: u64, screens: &mut [&mut Harness]) {
        self.step(ms);
        for h in screens.iter_mut() {
            h.run(0.3);
        }
        self.step(200);
        for h in screens.iter_mut() {
            h.run(0.3);
        }
    }
}

/// A library of `titles`, each a Single song with a chart of its own
/// digest.
fn songs_of(titles: &[&str]) -> SelectData {
    SelectData {
        styles: sample::styles(),
        songs: titles
            .iter()
            .enumerate()
            .map(|(i, title)| SongEntry {
                id: title.to_lowercase().replace(' ', "-"),
                title: (*title).into(),
                artist: "Artist".into(),
                bpm_text: "150".into(),
                length: 120.0,
                charts: vec![ChartEntry {
                    digest: Some(ddi_chart::ChartDigest(
                        0x5000 + title.len() as u128 * 16 + i as u128,
                    )),
                    ..chart("dance-single", 0, 9)
                }],
                ..SongEntry::default()
            })
            .collect(),
        style: Some("dance-single".into()),
        ..SelectData::default()
    }
}

/// The room's song select of `room` over `data`, on a harness of its own.
fn online_select(env: &Env, room: &SharedRoom, data: SelectData, orders: &Orders) -> Harness {
    let mut h = harness();
    h.push(
        sample::room_select(env, room.clone(), data).with_room(room.clone(), shell(room, orders)),
    );
    h
}

fn list_shows(h: &Harness) -> bool {
    h.ui.actor("Wheel").is_some_and(|w| w.visible)
}

/// One picker at a time (2026-11-12). The owner picks and everyone else
/// follows their resting song, the co-host included, who alone has Pick
/// songs (the header's button, the footer's T). The co-host takes the
/// pick: their own list comes back and their resting song is the room's;
/// the owner follows it, with the same action, and takes the pick back
/// with T: their list is as they left it (the song, the search). The
/// status line names who chooses and the players' rows mark them; a
/// member has no action.
#[test]
fn a_co_host_takes_the_pick_over_and_the_owner_takes_it_back() {
    let t = Three::new(true);
    let (ea, eb, ec) = (env(), env(), env());
    let orders = Orders::default();
    let mut ha = online_select(
        &ea,
        &t.alice,
        songs_of(&["Alpha One", "Alpha Two", "Alpha Three"]),
        &orders,
    );
    let mut hb = online_select(&eb, &t.bob, songs_of(&["Bravo One", "Bravo Two"]), &orders);
    let mut hc = online_select(&ec, &t.carol, songs_of(&["Charlie One"]), &orders);
    t.settle(400, &mut [&mut ha, &mut hb, &mut hc]);
    // The owner picks; the co-host and the member follow.
    let (a, b, c) = (view(&ea), view(&eb), view(&ec));
    assert!(a.host && !b.host && !c.host);
    assert_eq!((a.picker, b.picker, c.picker), (Some(1), Some(1), Some(1)));
    assert!(!a.can_take_pick && b.can_take_pick && !c.can_take_pick);
    assert!(list_shows(&ha) && !list_shows(&hb) && !list_shows(&hc));
    assert!(has(&ha, "Alpha One") && !has(&hb, "Bravo One") && !has(&hc, "Charlie One"));
    for v in [&b, &c] {
        assert_eq!(v.status, "alice is choosing the first song.");
    }
    assert_eq!(a.status, "Pick a song to start the match.");
    // The action: a following operator's alone.
    assert!(there(&hb, "Room.TakePick") && has(&hb, "Pick songs"));
    assert!(!there(&ha, "Room.TakePick") && !there(&hc, "Room.TakePick"));
    assert!(view(&eb).items.iter().any(|i| i == "Room.TakePick"));
    let hint = |e: &Env| e.state.borrow().hints.iter().any(|h| h.contains("Pick songs"));
    assert!(hint(&eb) && !hint(&ea) && !hint(&ec), "{:?}", eb.state.borrow().hints);
    // The players' rows mark who chooses, quietly.
    assert!(has(&hb, "Host · Picking") && has(&hb, "Co-host"), "{:?}", hb.texts());
    assert!(b.members.iter().any(|m| m.picker && m.name == "alice"));
    // Alice moves to her second song, searches and rests there: the
    // others see it.
    ha.key("Slash", "/");
    ha.send(crate::UiEvent::Text(crate::event::TextEvent::Commit(
        "alpha".into(),
    )));
    ha.run(0.1);
    ha.key("Enter", "Enter");
    ha.run(0.3);
    ha.key("ArrowDown", "ArrowDown");
    ha.run(0.5);
    assert_eq!(ea.state.borrow().song, "Alpha Two");
    assert_eq!(ea.state.borrow().search, "alpha");
    for _ in 0..8 {
        t.step(500);
        ha.run(0.1);
    }
    t.settle(200, &mut [&mut hb, &mut hc]);
    assert_eq!(view(&eb).spotlight.as_deref(), Some("Alpha Two"));
    assert!(has(&hb, "Alpha Two") && has(&hc, "Alpha Two"));
    // A member's T, and their Enter, do nothing.
    hc.key("KeyT", "t");
    hc.key("Enter", "Enter");
    t.settle(200, &mut [&mut ha, &mut hb, &mut hc]);
    assert_eq!(view(&ec).picker, Some(1));
    // The co-host takes the pick, at once: their own list, the owner's
    // resting song gone; the owner follows, with the action.
    press(&mut hb, "Room.TakePick");
    t.settle(400, &mut [&mut ha, &mut hb, &mut hc]);
    let (a, b, c) = (view(&ea), view(&eb), view(&ec));
    assert!(!a.host && b.host && !c.host);
    assert_eq!((a.picker, c.picker), (Some(2), Some(2)));
    assert!(a.can_take_pick && !b.can_take_pick);
    assert!(list_shows(&hb) && !list_shows(&ha));
    assert!(has(&hb, "Bravo One") && !has(&ha, "Alpha One"), "{:?}", ha.texts());
    assert!(there(&ha, "Room.TakePick") && !there(&hb, "Room.TakePick"));
    assert!(hint(&ea) && !hint(&eb));
    assert_eq!(a.spotlight, None, "the last picker's song went with them");
    for v in [&a, &c] {
        assert_eq!(v.status, "bob is choosing the first song.");
    }
    assert!(has(&ha, "Co-host · Picking") && has(&ha, "Host"), "{:?}", ha.texts());
    // What the co-host's song select rests on is the room's now.
    for _ in 0..8 {
        t.step(500);
        hb.run(0.1);
    }
    t.settle(200, &mut [&mut ha, &mut hc]);
    assert_eq!(view(&ea).spotlight.as_deref(), Some("Bravo One"));
    assert!(has(&ha, "Bravo One") && has(&hc, "Bravo One"), "{:?}", ha.texts());
    // The owner's Enter picks nothing while they follow.
    ha.key("Enter", "Enter");
    t.settle(200, &mut [&mut ha, &mut hb, &mut hc]);
    assert_eq!(view(&ea).phase, "lobby");
    // T: the owner takes the pick back, and has their list as they left
    // it: the song they were on, their search.
    ha.key("KeyT", "t");
    t.settle(400, &mut [&mut ha, &mut hb, &mut hc]);
    let (a, b) = (view(&ea), view(&eb));
    assert!(a.host && !b.host && b.can_take_pick);
    assert!(list_shows(&ha) && !list_shows(&hb));
    assert_eq!(ea.state.borrow().song, "Alpha Two");
    assert_eq!(ea.state.borrow().search, "alpha");
    assert!(!has(&hb, "Bravo One"), "the co-host follows again");
    assert_eq!(b.status, "alice is choosing the first song.");
    // Her song, rested on again, is the room's again; her Enter picks it.
    for _ in 0..8 {
        t.step(500);
        ha.run(0.1);
    }
    t.settle(200, &mut [&mut hb]);
    assert_eq!(view(&eb).spotlight.as_deref(), Some("Alpha Two"));
    ha.key("Enter", "Enter");
    t.settle(400, &mut [&mut ha, &mut hb, &mut hc]);
    assert_eq!(view(&ec).phase, "preparing");
    assert_eq!(view(&ec).picks[0], "Stage 1: Alpha Two Hard 9");
}

/// On a phone the following operator's Pick songs is in the room's More
/// (a controller's way to it on every layout); a member's More has none.
#[test]
fn a_phones_more_takes_the_pick() {
    let t = Three::new(true);
    let (eb, ec) = (env(), env());
    let orders = Orders::default();
    let phone = |env: &Env, room: &SharedRoom| {
        let mut h = Harness::new(super::COMPACT, [844.0, 390.0], 1.0, TextMode::Own);
        h.push(
            sample::room_select(env, room.clone(), songs_of(&["Bravo One"]))
                .with_room(room.clone(), shell(room, &orders)),
        );
        h
    };
    let mut hb = phone(&eb, &t.bob);
    let mut hc = phone(&ec, &t.carol);
    t.settle(400, &mut [&mut hb, &mut hc]);
    assert!(!there(&hb, "Room.TakePick"), "no room for a button");
    press(&mut hc, "More");
    assert!(!has(&hc, "Pick songs"), "{:?}", hc.texts());
    hb.pad(crate::nav::MenuAction::More, true);
    hb.pad(crate::nav::MenuAction::More, false);
    hb.run(0.3);
    assert!(has(&hb, "Pick songs"), "{:?}", hb.texts());
    press(&mut hb, "Item.takepick");
    t.settle(400, &mut [&mut hb, &mut hc]);
    assert!(view(&eb).host && list_shows(&hb));
    assert!(has(&hb, "Bravo One"), "{:?}", hb.texts());
    assert_eq!(view(&ec).status, "bob is choosing the first song.");
}

/// The pick's holder gone: it is the owner's again; with the owner gone
/// too it is nobody's, said to everyone, with how to take it to those who
/// may.
#[test]
fn a_pick_left_behind_goes_to_the_owner_or_is_there_to_take() {
    use ddi_room::{Command, Connection, ParticipantId};
    let t = Three::new(true);
    let (ea, eb, ec) = (env(), env(), env());
    let orders = Orders::default();
    let mut ha = online_select(&ea, &t.alice, songs_of(&["Alpha One"]), &orders);
    let mut hb = online_select(&eb, &t.bob, songs_of(&["Bravo One"]), &orders);
    let mut hc = online_select(&ec, &t.carol, songs_of(&["Charlie One"]), &orders);
    t.settle(400, &mut [&mut ha, &mut hb, &mut hc]);
    press(&mut hb, "Room.TakePick");
    t.settle(400, &mut [&mut ha, &mut hb, &mut hc]);
    assert!(view(&eb).host);
    // The co-host loses the role: the owner picks again, their list back.
    t.server
        .borrow_mut()
        .authority(Command::SetCoHost {
            participant: ParticipantId(2),
            on: false,
        })
        .unwrap();
    t.settle(400, &mut [&mut ha, &mut hb, &mut hc]);
    let (a, b) = (view(&ea), view(&eb));
    assert!(a.host && !b.host && !b.can_take_pick);
    assert!(list_shows(&ha) && !list_shows(&hb));
    assert!(has(&ha, "Alpha One") && !there(&hb, "Room.TakePick"));
    // Nobody chooses (its owner away past its patience, a co-host there,
    // before the room has passed on): said to everyone, with how to take
    // the pick to those who may.
    let co_host = |on| {
        t.server
            .borrow_mut()
            .authority(Command::SetCoHost {
                participant: ParticipantId(2),
                on,
            })
            .unwrap();
    };
    co_host(true);
    t.server.borrow_mut().rewrite(|s| {
        s.members.get_mut(&ParticipantId(1)).unwrap().connection = Connection::Disconnected;
        s.picking.picker = None;
    });
    t.settle(400, &mut [&mut hb, &mut hc]);
    let (b, c) = (view(&eb), view(&ec));
    assert_eq!((b.picker, c.picker), (None, None));
    assert_eq!(
        b.status,
        "Nobody is choosing songs. Use Pick songs to choose them."
    );
    assert_eq!(c.status, "Nobody is choosing songs right now.");
    assert!(b.can_take_pick && !c.can_take_pick);
    press(&mut hb, "Room.TakePick");
    t.settle(400, &mut [&mut hb, &mut hc]);
    assert!(view(&eb).host && list_shows(&hb));
    assert_eq!(view(&ec).status, "bob is choosing the first song.");
}

/// A part's alpha as drawn now (0 where it is not there, hidden, or a
/// frame with nothing in it).
fn alpha(h: &Harness, name: &str) -> f32 {
    h.ui.actor(name)
        .filter(|a| a.visible)
        .filter(|a| !matches!(&a.kind, ddi_actor::Kind::Frame(c) if c.is_empty()))
        .map_or(0.0, |a| a.current().diffuse()[3])
}

/// How a part's alpha went over frames: how many times it began to
/// rise from nothing (an entrance begun), and how many times it fell.
fn runs(alphas: &[f32]) -> (usize, usize) {
    let mut rises = 0;
    let mut falls = 0;
    let mut rising = false;
    let mut falling = false;
    for w in alphas.windows(2) {
        if w[1] > w[0] + 1e-4 {
            if !rising && w[0] < 0.05 {
                rises += 1;
            }
            rising = true;
            falling = false;
        } else if w[1] < w[0] - 1e-4 {
            if !falling {
                falls += 1;
            }
            falling = true;
            rising = false;
        }
    }
    (rises, falls)
}

/// Whether nothing of a view shows: no list, no song's side, no status
/// line, no panel, no players, no room buttons.
fn no_view(h: &Harness) -> bool {
    !list_shows(h)
        && h.ui.actor("Song").is_none_or(|s| !s.visible)
        && !there(h, "Room.Status")
        && !there(h, "Panel")
        && !there(h, "Players.Heading")
        && !there(h, "Room.Rules")
        && !there(h, "Room.TakePick")
}

/// An online room alone with its owner, whose first answer is `delay` ms
/// on its way; the owner's song select on a harness.
fn late_room(delay: u64, env: &Env) -> (ddi_room::ManualClock, SharedRoom, Harness) {
    use crate::state::online::tests::room::{client, person};
    use ddi_room::fake::FakeServer;
    use ddi_room::{Identity, RoomPolicy};
    let clock = ddi_room::ManualClock::new(1_000);
    let server = FakeServer::hosted(
        "Room000001",
        Identity::new("dev:alice"),
        "alice",
        RoomPolicy::default(),
        clock.clone(),
    )
    .shared();
    let alice = Rc::new(RefCell::new(client(
        &server,
        person(&server, "alice"),
        "alice",
        1_000,
    )));
    if delay > 0 {
        server
            .borrow_mut()
            .fault(ddi_room::ParticipantId(1), ddi_room::fake::Fault::Delay(delay));
    }
    let h = online_select(env, &alice, songs_of(&["Alpha One", "Alpha Two"]), &Orders::default());
    (clock, alice, h)
}

/// Frames of a 60th of a second for `seconds`, the room's time passing
/// with them; each frame's alpha of `parts`.
fn frames(
    clock: &ddi_room::ManualClock,
    rooms: &[&SharedRoom],
    h: &mut Harness,
    seconds: f32,
    parts: &[&str],
) -> Vec<Vec<f32>> {
    let mut out = vec![Vec::new(); parts.len()];
    for _ in 0..(seconds * 60.0).round() as usize {
        clock.advance(16);
        let now = ddi_room::Clock::now(clock).0;
        for r in rooms {
            r.borrow_mut().tick(now);
        }
        h.frame(1.0 / 60.0);
        for (k, name) in parts.iter().enumerate() {
            out[k].push(alpha(h, name));
        }
    }
    out
}

/// Creating a private room (the maintainer, 2026-11-12: "some animation
/// flicker"). The room's first answer comes after the screen is built:
/// until then nothing of a view shows, only the bars and, after a
/// moment, one quiet line. Then the host's view comes in, once: each of
/// its parts begins one entrance and never falls back, the bars stay as
/// they are, the line goes.
#[test]
fn a_room_made_shows_no_view_until_it_answers_and_then_the_hosts_once() {
    let env = env();
    let (clock, alice, mut h) = late_room(1_600, &env);
    // Before the answer.
    let mut seen = Vec::new();
    for _ in 0..84 {
        seen.extend(frames(&clock, &[&alice], &mut h, 1.0 / 60.0, &["RoomJoining", "Header"]));
        assert!(no_view(&h), "{:?}", h.texts());
        assert!(!view(&env).host);
    }
    assert!(has(&h, "Joining the room…"), "{:?}", h.texts());
    assert_eq!(alpha(&h, "RoomJoining"), 1.0, "shown after a moment");
    assert_eq!(alpha(&h, "Header"), 1.0);
    // The answer comes (1.6 s after the socket opened).
    let parts = ["Wheel", "Song", "RoomParts", "RoomPlayers", "Header", "RoomJoining", "Footer"];
    let after = frames(&clock, &[&alice], &mut h, 1.2, &parts);
    assert!(view(&env).host && list_shows(&h));
    assert!(has(&h, "Alpha One") && there(&h, "Room.Status"), "{:?}", h.texts());
    for (k, name) in parts.iter().enumerate().take(4) {
        assert_eq!(runs(&after[k]), (1, 0), "{name}: {:?}", after[k]);
        assert_eq!(after[k].last(), Some(&1.0), "{name}");
    }
    // The bars never moved; the line went, once.
    assert!(after[4].iter().all(|a| *a == 1.0), "{:?}", after[4]);
    assert!(after[6].iter().all(|a| *a == 1.0), "{:?}", after[6]);
    assert_eq!(runs(&after[5]), (0, 1), "{:?}", after[5]);
    assert_eq!(alpha(&h, "RoomJoining"), 0.0);
    // Nothing after: at rest.
    let rest = frames(&clock, &[&alice], &mut h, 0.5, &parts);
    for (k, name) in parts.iter().enumerate() {
        assert_eq!(runs(&rest[k]), (0, 0), "{name}: {:?}", rest[k]);
    }
}

/// A room that answers at once, or while the screen is still coming in
/// (a server nearby): the line never shows, the bars' own entrance goes
/// on from where it was (never begun again, never cut short), and the
/// view comes in once.
#[test]
fn a_room_that_answers_at_once_shows_no_line_and_keeps_the_bars_coming_in() {
    for delay in [0, 50, 120] {
        let env = env();
        let (clock, alice, mut h) = late_room(delay, &env);
        // Known within the first frame, or a few frames in.
        assert_eq!(no_view(&h), delay > 0, "{delay}");
        let parts = ["Wheel", "Song", "RoomParts", "Header", "RoomJoining"];
        let seen = frames(&clock, &[&alice], &mut h, 1.0, &parts);
        assert!(view(&env).host && list_shows(&h), "{delay}");
        for (k, name) in parts.iter().enumerate().take(3) {
            assert_eq!(runs(&seen[k]), (1, 0), "{delay} {name}: {:?}", seen[k]);
            assert_eq!(seen[k].last(), Some(&1.0), "{delay} {name}");
        }
        // The header: one entrance, as smooth as any screen's.
        let header = &seen[3];
        assert!(runs(header).0 <= 1 && runs(header).1 == 0, "{delay}: {header:?}");
        assert_eq!(header.last(), Some(&1.0));
        assert!(
            header.windows(2).all(|w| w[1] - w[0] < 0.25),
            "{delay}: no jump {header:?}"
        );
        assert!(seen[4].iter().all(|a| *a == 0.0), "{delay}: {:?}", seen[4]);
    }
}

/// A member joining: nothing of a view before the room answers, then
/// the following view, once.
#[test]
fn a_member_joining_gets_the_following_view_once() {
    use ddi_room::fake::Fault;
    let t = Three::new(false);
    t.server
        .borrow_mut()
        .fault(ddi_room::ParticipantId(2), Fault::Delay(600));
    let env = env();
    let mut h = online_select(&env, &t.bob, songs_of(&["Bravo One"]), &Orders::default());
    let rooms = [&t.alice, &t.bob, &t.carol];
    for _ in 0..30 {
        frames(&t.clock, &rooms, &mut h, 1.0 / 60.0, &[]);
        assert!(no_view(&h), "{:?}", h.texts());
    }
    let parts = ["Song", "RoomParts", "RoomPlayers", "Header"];
    let after = frames(&t.clock, &rooms, &mut h, 1.2, &parts);
    assert!(!view(&env).host && !list_shows(&h));
    assert!(there(&h, "Panel") && there(&h, "Room.Status"), "{:?}", h.texts());
    assert_eq!(view(&env).status, "alice is choosing the first song.");
    for (k, name) in parts.iter().enumerate().take(3) {
        assert_eq!(runs(&after[k]), (1, 0), "{name}: {:?}", after[k]);
        assert_eq!(after[k].last(), Some(&1.0), "{name}");
    }
    assert!(after[3].iter().all(|a| *a == 1.0), "{:?}", after[3]);
}

/// A change of hands is one swap: the view's parts leave (the list still
/// the player's own while it does), then the other view's come in, each
/// once; the bars stay. Under reduced motion the swap is at once.
#[test]
fn a_change_of_hands_is_one_swap() {
    let t = Three::new(true);
    let (ea, eb) = (env(), env());
    let orders = Orders::default();
    let mut ha = online_select(&ea, &t.alice, songs_of(&["Alpha One"]), &orders);
    let mut hb = online_select(&eb, &t.bob, songs_of(&["Bravo One"]), &orders);
    t.settle(600, &mut [&mut ha, &mut hb]);
    let rooms = [&t.alice, &t.bob, &t.carol];
    t.bob.borrow_mut().take_pick().unwrap();
    // Alice's list leaves, then the following view comes in.
    let parts = ["Wheel", "RoomParts", "Song", "Header", "Footer"];
    let mut seen = vec![Vec::new(); parts.len()];
    let mut list_gone_at = None;
    for f in 0..60 {
        let step = frames(&t.clock, &rooms, &mut ha, 1.0 / 60.0, &parts);
        for (k, s) in step.into_iter().enumerate() {
            seen[k].extend(s);
        }
        if list_shows(&ha) {
            assert!(has(&ha, "Alpha One"), "her own list while it leaves");
            assert_eq!(list_gone_at, None, "never back");
        } else if list_gone_at.is_none() {
            list_gone_at = Some(f);
            assert!(*seen[0].last().unwrap() == 0.0, "gone only once out");
        }
    }
    assert!(list_gone_at.is_some_and(|f| f > 2), "{list_gone_at:?}");
    assert!(!view(&ea).host && there(&ha, "Panel") && there(&ha, "Room.TakePick"));
    // The list: out, never in again. The room's parts and the song's
    // side: out, then in, once each.
    assert_eq!(runs(&seen[0]), (0, 1), "{:?}", seen[0]);
    for k in [1, 2] {
        assert_eq!(runs(&seen[k]), (1, 1), "{}: {:?}", parts[k], seen[k]);
        assert_eq!(seen[k].last(), Some(&1.0));
    }
    for k in [3, 4] {
        assert!(seen[k].iter().all(|a| *a == 1.0), "{}: {:?}", parts[k], seen[k]);
    }
    // Bob's side of it: the following view out, his list in.
    let seen = frames(&t.clock, &rooms, &mut hb, 1.0, &["Wheel", "RoomParts"]);
    assert!(view(&eb).host && list_shows(&hb) && has(&hb, "Bravo One"));
    assert_eq!(runs(&seen[0]), (1, 0), "{:?}", seen[0]);
    assert_eq!(runs(&seen[1]), (1, 1), "{:?}", seen[1]);
    // Reduced motion: the other view at once, at rest.
    ea.reduced_motion.set(true);
    frames(&t.clock, &rooms, &mut ha, 0.5, &[]);
    t.alice.borrow_mut().take_pick().unwrap();
    let seen = frames(&t.clock, &rooms, &mut ha, 0.1, &["Wheel", "RoomParts", "Song"]);
    assert!(view(&ea).host && list_shows(&ha) && has(&ha, "Alpha One"));
    for (k, s) in seen.iter().enumerate() {
        assert_eq!(s.last(), Some(&1.0), "{k}: {s:?}");
    }
    assert!(seen[1].iter().chain(&seen[2]).all(|a| *a == 1.0), "{seen:?}");
}

/// CPU practice is known from its first frame: its view is built at once
/// and comes in with the screen, once, with no joining line.
#[test]
fn practice_is_known_from_its_first_frame() {
    let env = env();
    let room = RoomClient::shared(0, 1);
    let orders = Orders::default();
    let mut h = harness();
    h.push(room_select(&env, room.clone(), "dance-single", &orders));
    let clock = ddi_room::ManualClock::new(0);
    let parts = ["Wheel", "Song", "RoomParts", "RoomPlayers", "Header"];
    let first = frames(&clock, &[], &mut h, 1.0 / 60.0, &parts);
    assert!(list_shows(&h) && there(&h, "Room.Status") && !there(&h, "RoomJoining"));
    assert!(view(&env).host);
    let seen = frames(&clock, &[], &mut h, 1.0, &parts);
    for (k, name) in parts.iter().enumerate() {
        let all: Vec<f32> = first[k].iter().chain(&seen[k]).copied().collect();
        assert!(runs(&all).0 <= 1 && runs(&all).1 == 0, "{name}: {all:?}");
        assert_eq!(all.last(), Some(&1.0), "{name}");
    }
}

/// A link that ends before the room is known (not a member, the room
/// closed, the sign-in ended) says so where the joining line is: never
/// "Joining the room…" for good. Since feedback round AA a person who was
/// never in the room is told that, with its notice, never "You left".
#[test]
fn a_room_that_cannot_be_joined_says_why() {
    use crate::state::online::tests::room::{client, person};
    let t = Three::new(false);
    let dave: SharedRoom = Rc::new(RefCell::new(client(
        &t.server,
        person(&t.server, "dave"),
        "dave",
        1_000,
    )));
    let env = env();
    let mut h = online_select(&env, &dave, songs_of(&["Delta One"]), &Orders::default());
    frames(&t.clock, &[&dave], &mut h, 1.5, &[]);
    assert_eq!(super::test_kit::top(&h), super::room::NEVER_IN);
    h.tap(PointerKind::Mouse, "Cancel");
    frames(&t.clock, &[&dave], &mut h, 0.5, &[]);
    assert!(no_view(&h), "{:?}", h.texts());
    let never = "You're not in this room. Ask its host for the invite link.";
    assert_eq!(said(&h, "RoomJoining"), never);
    assert_eq!(alpha(&h, "RoomJoining"), 1.0);
    assert_eq!(view(&env).status, never);
}

fn role_of(v: &super::room::RoomView, id: u64) -> String {
    v.members
        .iter()
        .find(|m| m.id == id)
        .map(|m| m.role.clone())
        .unwrap_or_default()
}

/// The host's controls (2026-11-12). An online room's owner alone has
/// Host options in the Players pop-up: Make co-host and Remove co-host,
/// the room's "If the host leaves" setting with each value said in a
/// sentence (said to everyone in their Players pop-up), and Make host,
/// asked once more. After each, the views of the owner, of the one
/// promoted and of a member say the roles as the room has them; the one
/// made a co-host gets Pick songs, and a handed-over room's new owner
/// the controls.
#[test]
fn the_host_makes_co_hosts_sets_what_leaving_does_and_hands_the_room_over() {
    let t = Three::new(false);
    let (ea, eb, ec) = (env(), env(), env());
    let orders = Orders::default();
    let mut ha = online_select(&ea, &t.alice, songs_of(&["Alpha One"]), &orders);
    let mut hb = online_select(&eb, &t.bob, songs_of(&["Bravo One"]), &orders);
    let mut hc = online_select(&ec, &t.carol, songs_of(&["Charlie One"]), &orders);
    t.settle(400, &mut [&mut ha, &mut hb, &mut hc]);
    let (a, b, c) = (view(&ea), view(&eb), view(&ec));
    assert!(a.manages_hosts && !b.manages_hosts && !c.manages_hosts);
    for v in [&a, &b, &c] {
        assert_eq!(v.host_leaves.as_deref(), Some("pass_on"));
    }
    let pass = "If the host leaves, a co-host becomes the host, or else the player who joined first.";
    let close = "If the host leaves, the room closes for everyone.";
    // A member's Players pop-up says what the room does, and has no
    // controls; a tap on someone's row does nothing.
    open_players(&mut hb);
    assert_eq!(top(&hb), "room-players");
    assert!(has(&hb, pass) && !there(&hb, "HostOptions"));
    assert!(!there(&hb, "Person.1") && !there(&hb, "Person.3"));
    press(&mut hb, "Close");
    // The owner's: the others' rows to tap, never their own, and Host
    // options.
    open_players(&mut ha);
    assert!(has(&ha, pass) && there(&ha, "HostOptions"));
    assert!(there(&ha, "Person.2") && there(&ha, "Person.3") && !there(&ha, "Person.1"));
    press(&mut ha, "HostOptions");
    assert_eq!(top(&ha), "room-host");
    assert!(view(&ea).host_open);
    assert!(has(&ha, "bob") && has(&ha, "carol") && has(&ha, "Pass it on"));
    // Make co-host: everyone's rows say so, and Bob may pick songs.
    press(&mut ha, "CoHost.2");
    t.settle(400, &mut [&mut ha, &mut hb, &mut hc]);
    for e in [&ea, &eb, &ec] {
        assert_eq!(role_of(&view(e), 2), "co_host");
    }
    assert!(has(&ha, "Remove co-host") && has(&ha, "Co-host"));
    assert!(there(&hb, "Room.TakePick") && view(&eb).can_take_pick);
    assert!(!view(&eb).manages_hosts, "a co-host has no such controls");
    assert!(has(&hc, "Co-host"), "the member's players panel says it");
    // Remove co-host.
    press(&mut ha, "CoHost.2");
    t.settle(400, &mut [&mut ha, &mut hb, &mut hc]);
    for e in [&ea, &eb, &ec] {
        assert_eq!(role_of(&view(e), 2), "member");
    }
    assert!(!there(&hb, "Room.TakePick") && !has(&ha, "Remove co-host"));
    // The setting, to Close and back: each value in its sentence, told
    // everyone.
    press(&mut ha, "Inc.leaves");
    t.settle(400, &mut [&mut ha, &mut hb, &mut hc]);
    assert!(has(&ha, "Close the room") && has(&ha, close));
    assert_eq!(view(&ec).host_leaves.as_deref(), Some("close"));
    open_players(&mut hc);
    assert!(has(&hc, close));
    press(&mut hc, "Close");
    press(&mut ha, "Dec.leaves");
    t.settle(400, &mut [&mut ha, &mut hb, &mut hc]);
    assert!(has(&ha, "Pass it on") && has(&ha, pass));
    assert_eq!(view(&eb).host_leaves.as_deref(), Some("pass_on"));
    // Make host is asked once more; Cancel changes nothing.
    press(&mut ha, "MakeHost.3");
    assert_eq!(top(&ha), "confirm");
    assert!(has(&ha, "Make carol the host?"));
    assert_eq!(ha.ui.focused(), Some("Cancel"));
    press(&mut ha, "Cancel");
    t.settle(400, &mut [&mut ha, &mut hb, &mut hc]);
    assert_eq!(role_of(&view(&ec), 1), "owner");
    press(&mut ha, "MakeHost.3");
    press(&mut ha, "Confirm");
    t.settle(400, &mut [&mut ha, &mut hb, &mut hc]);
    // Carol owns the room; Alice is a co-host and her Host options are
    // gone with the role; Bob is a member as before.
    for e in [&ea, &eb, &ec] {
        let v = view(e);
        assert_eq!(
            (role_of(&v, 1), role_of(&v, 2), role_of(&v, 3)),
            ("co_host".into(), "member".into(), "owner".into())
        );
    }
    assert_eq!(top(&ha), "room-players");
    assert!(!view(&ea).host_open && !there(&ha, "HostOptions"));
    assert_eq!(ha.ui.focused(), Some("Close"));
    press(&mut ha, "Close");
    assert!(view(&ec).manages_hosts && !view(&ea).manages_hosts);
    // The pick stays with whoever has it: Alice still chooses, her list
    // as it was; the new owner follows, with Pick songs.
    assert!(view(&ea).host && list_shows(&ha) && has(&ha, "Alpha One"));
    assert!(!view(&ec).host && there(&hc, "Room.TakePick"));
    assert_eq!(view(&ec).status, "alice is choosing the first song.");
    // The new owner hands it back with the same controls.
    open_players(&mut hc);
    press(&mut hc, "HostOptions");
    assert!(has(&hc, "Remove co-host"), "Alice, a co-host");
    press(&mut hc, "MakeHost.1");
    press(&mut hc, "Confirm");
    t.settle(400, &mut [&mut ha, &mut hb, &mut hc]);
    assert_eq!(role_of(&view(&eb), 1), "owner");
    assert_eq!(role_of(&view(&eb), 3), "co_host");
}

/// Host options by keyboard and by controller, and a tap on a person's
/// row: the Players pop-up's Host options beside Close, the setting's
/// arrows, a person's two buttons side by side, the question's Cancel
/// first.
#[test]
fn host_options_work_by_keyboard_controller_and_a_tap_on_a_row() {
    use crate::nav::MenuAction;
    let t = Three::new(false);
    let (ea, eb, ec) = (env(), env(), env());
    let orders = Orders::default();
    let mut ha = online_select(&ea, &t.alice, songs_of(&["Alpha One"]), &orders);
    let mut hb = online_select(&eb, &t.bob, songs_of(&["Bravo One"]), &orders);
    let mut hc = online_select(&ec, &t.carol, songs_of(&["Charlie One"]), &orders);
    t.settle(400, &mut [&mut ha, &mut hb, &mut hc]);
    let step = |h: &mut Harness, action: MenuAction, pad: bool| {
        if pad {
            h.pad(action, true);
            h.pad(action, false);
        } else {
            let code = match action {
                MenuAction::Left => "ArrowLeft",
                MenuAction::Right => "ArrowRight",
                MenuAction::Up => "ArrowUp",
                MenuAction::Down => "ArrowDown",
                _ => "Enter",
            };
            h.key(code, code);
        }
        h.run(0.2);
    };
    let walk = |h: &mut Harness, name: &str, action: MenuAction, pad: bool| {
        for _ in 0..12 {
            if h.ui.focused() == Some(name) {
                return;
            }
            step(h, action, pad);
        }
        panic!("never reached {name}: {:?}", h.ui.focused());
    };
    // A keyboard: Players, Left to Host options, Enter.
    open_players(&mut ha);
    assert_eq!(ha.ui.focused(), Some("Close"));
    walk(&mut ha, "HostOptions", MenuAction::Left, false);
    step(&mut ha, MenuAction::Start, false);
    assert_eq!(top(&ha), "room-host");
    let names: Vec<String> = ea.state.borrow().page_items.clone();
    assert_eq!(
        names,
        [
            "Dec.leaves",
            "Inc.leaves",
            "CoHost.2",
            "MakeHost.2",
            "CoHost.3",
            "MakeHost.3",
            "Close"
        ]
    );
    // The setting by its arrows.
    walk(&mut ha, "Inc.leaves", MenuAction::Right, false);
    step(&mut ha, MenuAction::Start, false);
    t.settle(400, &mut [&mut ha, &mut hb, &mut hc]);
    assert_eq!(said(&ha, "HostLeaves.Value"), "Close the room");
    assert_eq!(ha.ui.focused(), Some("Inc.leaves"), "the focus stays");
    walk(&mut ha, "Dec.leaves", MenuAction::Left, false);
    step(&mut ha, MenuAction::Start, false);
    t.settle(400, &mut [&mut ha, &mut hb, &mut hc]);
    assert_eq!(said(&ha, "HostLeaves.Value"), "Pass it on");
    // Down to Bob's row: Make co-host, then along it to Make host.
    walk(&mut ha, "CoHost.2", MenuAction::Down, false);
    step(&mut ha, MenuAction::Start, false);
    t.settle(400, &mut [&mut ha, &mut hb, &mut hc]);
    assert_eq!(role_of(&view(&ec), 2), "co_host");
    assert_eq!(ha.ui.focused(), Some("CoHost.2"));
    // A controller: along the row to Make host, Start, then the question
    // (Cancel first; Right, Start).
    walk(&mut ha, "MakeHost.2", MenuAction::Right, true);
    step(&mut ha, MenuAction::Start, true);
    assert_eq!(top(&ha), "confirm");
    assert_eq!(ha.ui.focused(), Some("Cancel"));
    walk(&mut ha, "Confirm", MenuAction::Right, true);
    step(&mut ha, MenuAction::Start, true);
    t.settle(400, &mut [&mut ha, &mut hb, &mut hc]);
    assert_eq!(role_of(&view(&ec), 2), "owner");
    assert_eq!(top(&ha), "room-players");
    // Back in the pop-up closes it, as before.
    step(&mut ha, MenuAction::Back, true);
    ha.run(0.3);
    assert_eq!(top(&ha), "select");
    // Touch: the new owner taps Carol's name in their Players pop-up,
    // which covers most of her row, and Host options opens on her; then
    // the row's plate does the same.
    open_players(&mut hb);
    let name = (0..12)
        .map(|k| format!("Panel{k}.Name"))
        .find(|n| there(&hb, n) && said(&hb, n) == "carol")
        .expect("Carol's name in the pop-up");
    hb.tap(PointerKind::Touch, &name);
    hb.run(0.5);
    assert_eq!(top(&hb), "room-host", "a tap on the name opens Host options");
    assert_eq!(hb.ui.focused(), Some("CoHost.3"));
    step(&mut hb, MenuAction::Back, true);
    hb.run(0.3);
    assert_eq!(top(&hb), "room-players");
    hb.tap(PointerKind::Touch, "Person.3");
    hb.run(0.5);
    assert_eq!(top(&hb), "room-host");
    assert_eq!(hb.ui.focused(), Some("CoHost.3"));
    hb.tap(PointerKind::Touch, "CoHost.3");
    hb.run(0.3);
    t.settle(400, &mut [&mut ha, &mut hb, &mut hc]);
    assert_eq!(role_of(&view(&ea), 3), "co_host");
}

/// Who may be promoted and where: never a bot (its row has no controls),
/// and someone away cannot be handed the room.
#[test]
fn host_options_leave_out_bots_and_the_away() {
    use ddi_room::{Command, Connection, Identity, Intent, ParticipantId};
    let t = Three::new(false);
    t.server
        .borrow_mut()
        .join(Identity::new("bot:robo"), "robo", Intent::Play)
        .unwrap();
    let (ea, eb) = (env(), env());
    let orders = Orders::default();
    let mut ha = online_select(&ea, &t.alice, songs_of(&["Alpha One"]), &orders);
    let mut hb = online_select(&eb, &t.bob, songs_of(&["Bravo One"]), &orders);
    t.settle(400, &mut [&mut ha, &mut hb]);
    // Carol's connection is lost past the room's patience.
    t.server
        .borrow_mut()
        .authority(Command::SetConnection {
            participant: ParticipantId(3),
            connection: Connection::Disconnected,
        })
        .unwrap();
    t.settle(400, &mut [&mut ha, &mut hb]);
    open_players(&mut ha);
    assert!(there(&ha, "Person.2") && there(&ha, "Person.3"));
    assert!(!there(&ha, "Person.4"), "a bot's row has no controls");
    press(&mut ha, "HostOptions");
    assert!(there(&ha, "CoHost.2") && there(&ha, "CoHost.3") && !there(&ha, "CoHost.4"));
    assert!(!has(&ha, "robo"));
    // Away: a co-host still, never the host.
    press(&mut ha, "MakeHost.3");
    assert_eq!(top(&ha), "room-host", "nothing is asked");
    press(&mut ha, "CoHost.3");
    t.settle(400, &mut [&mut ha, &mut hb]);
    assert_eq!(role_of(&view(&eb), 3), "co_host");
    press(&mut ha, "Close");
    press(&mut ha, "Close");
}

/// The succession on the screens: the owner leaves and the room passes
/// to the co-host, whose view changes hands by the one swap (their own
/// list coming in, the bars kept); the member's rows name the new host;
/// the one before comes back a co-host, following, with Pick songs.
#[test]
fn a_room_passed_on_changes_hands_on_everyones_screens() {
    let t = Three::new(true);
    let (ea, eb, ec) = (env(), env(), env());
    let orders = Orders::default();
    let mut ha = online_select(&ea, &t.alice, songs_of(&["Alpha One"]), &orders);
    let mut hb = online_select(&eb, &t.bob, songs_of(&["Bravo One"]), &orders);
    let mut hc = online_select(&ec, &t.carol, songs_of(&["Charlie One"]), &orders);
    t.settle(400, &mut [&mut ha, &mut hb, &mut hc]);
    assert!(view(&ea).host && !view(&eb).host && !list_shows(&hb));
    t.alice.borrow_mut().leave();
    t.settle(400, &mut [&mut hb, &mut hc]);
    t.settle(400, &mut [&mut hb, &mut hc]);
    let (b, c) = (view(&eb), view(&ec));
    assert_eq!(b.online.as_ref().unwrap().role, "owner");
    assert!(b.host && b.manages_hosts && list_shows(&hb) && has(&hb, "Bravo One"));
    assert_eq!(c.status, "bob is choosing the first song.");
    assert_eq!((role_of(&c, 2), c.picker), ("owner".into(), Some(2)));
    assert!(!c.members.iter().any(|m| m.id == 1), "alice is gone");
    assert!(has(&hc, "Host · Picking"));
    // Bob's link is as it was: the room did not close.
    assert_eq!(b.online.as_ref().unwrap().link, "connected");
}

/// What a room's screen asked of its shell about leaving: `leave` (the
/// home, the room kept) or `discard` (the room forgotten).
type Calls = Rc<RefCell<Vec<&'static str>>>;

/// [`shell`], its leaving kept in `calls` (the screen stays up: a test
/// looks at it after).
fn leaving_shell(room: &SharedRoom, orders: &Orders, calls: &Calls) -> RoomShell {
    let inner = shell(room, orders);
    let calls = calls.clone();
    Rc::new(move |call, cx| match call {
        RoomCall::Leave => calls.borrow_mut().push("leave"),
        RoomCall::Discard => calls.borrow_mut().push("discard"),
        other => inner(other, cx),
    })
}

fn asked(h: &Harness) -> bool {
    top(h) == crate::LEAVE_QUESTION
}

/// CPU practice never asks before it is left (the maintainer,
/// 2026-11-12: "even silently dropping the match is fine … a prompt
/// would be annoying"). Between matches Back leaves at once, the room
/// kept, as it was. With a match running every way out leaves at once
/// too, with no question, and the practice and its unfinished match are
/// forgotten: Escape, Backspace, the header's Back, a controller's Back,
/// More's Leave room.
#[test]
fn practice_never_asks_and_forgets_a_match_left_running() {
    use crate::nav::MenuAction;
    let env = env();
    let room = RoomClient::shared(0, 1);
    room.borrow_mut().set_rules_seen(true);
    let (orders, calls) = (Orders::default(), Calls::default());
    let screen = |room: &SharedRoom| {
        sample::room_select(&env, room.clone(), library("dance-single"))
            .with_room(room.clone(), leaving_shell(room, &orders, &calls))
    };
    let mut h = harness();
    h.push(screen(&room));
    h.run(0.5);
    // No match: nothing guarded, Back leaves at once, the room kept.
    assert_eq!(view(&env).leave_cost, None);
    assert!(!h.ui.guards_leaving());
    h.key("Escape", "Escape");
    h.run(0.3);
    assert!(!asked(&h));
    assert_eq!(*calls.borrow(), ["leave"]);
    // A match running: still nothing asked or guarded (the window's
    // close, the tab's and the browser's Back are then as anywhere), and
    // each way out forgets the practice.
    let s = &library("dance-single").songs[0];
    type Way = fn(&mut Harness);
    let ways: [(&str, Way); 5] = [
        ("Escape", |h| {
            h.key("Escape", "Escape");
        }),
        ("Backspace", |h| {
            h.key("Backspace", "Backspace");
        }),
        ("the header's Back", |h| {
            h.tap(PointerKind::Mouse, "Back");
        }),
        ("a controller's Back", |h| {
            h.pad(MenuAction::Back, true);
            h.pad(MenuAction::Back, false);
        }),
        ("More's Leave room", |h| {
            h.tap(PointerKind::Mouse, "More");
            h.run(0.5);
            h.tap(PointerKind::Mouse, "Item.leave");
        }),
    ];
    for (how, leave) in ways {
        calls.borrow_mut().clear();
        let room = RoomClient::shared(0, 1);
        room.borrow_mut().set_rules_seen(true);
        room.borrow_mut().pick(s, &s.charts[0]).unwrap();
        assert!(room.borrow().state().phase.kind().match_running());
        let mut h = harness();
        h.push(screen(&room));
        h.run(0.5);
        let v = view(&env);
        assert_eq!(v.leave_cost, None, "{how}");
        assert_eq!(v.leave_text, None, "{how}");
        assert!(!h.ui.guards_leaving(), "{how}");
        leave(&mut h);
        h.run(0.5);
        assert!(!asked(&h) && !view(&env).leave_open, "{how}: asked");
        assert!(!has(&h, "Leave the room?"), "{how}: {:?}", h.texts());
        assert_eq!(*calls.borrow(), ["discard"], "{how}: forgotten");
    }
    // After the match (stopped): kept on Back, as between matches.
    calls.borrow_mut().clear();
    let room = RoomClient::shared(0, 1);
    room.borrow_mut().set_rules_seen(true);
    room.borrow_mut().pick(s, &s.charts[0]).unwrap();
    room.borrow_mut().stop_match().unwrap();
    assert_eq!(room.borrow().leave_cost(), None);
    let mut h = harness();
    h.push(screen(&room));
    h.run(0.5);
    // (A match stopped before any stage was finished has no results to
    // pop up; closed if they do.)
    for _ in 0..3 {
        if top(&h) == "select" {
            break;
        }
        h.key("Escape", "Escape");
        h.run(0.5);
    }
    assert_eq!(top(&h), "select");
    assert!(calls.borrow().is_empty());
    h.key("Escape", "Escape");
    h.run(0.3);
    assert_eq!(*calls.borrow(), ["leave"]);
}

/// Asking before leaving, an online room: its host always (what the room
/// will do, by name where there is one), a member only while a match is
/// running; each case's sentence; Leave tells the room and the room does
/// what was said; nothing is asked once the link has ended.
#[test]
fn an_online_room_asks_its_host_always_and_the_others_during_a_match() {
    use ddi_room::fake::Fault;
    use ddi_room::{Command, ParticipantId};
    let t = Three::new(false);
    let (ea, eb, ec) = (env(), env(), env());
    let (orders, calls) = (Orders::default(), Calls::default());
    let select = |e: &Env, room: &SharedRoom, title: &str| {
        let mut h = harness();
        h.push(
            sample::room_select(e, room.clone(), songs_of(&[title]))
                .with_room(room.clone(), leaving_shell(room, &orders, &calls)),
        );
        h
    };
    let mut ha = select(&ea, &t.alice, "Alpha One");
    let mut hb = select(&eb, &t.bob, "Bravo One");
    let mut hc = select(&ec, &t.carol, "Charlie One");
    t.settle(400, &mut [&mut ha, &mut hb, &mut hc]);
    let cost = |e: &Env| view(e).leave_cost;
    let text = |e: &Env| view(e).leave_text.unwrap_or_default();
    // The host, between matches: the room passes to whoever joined first.
    assert_eq!(cost(&ea).as_deref(), Some("host_pass"));
    assert_eq!(text(&ea), "You are the host. bob will become the host.");
    assert!(ha.ui.guards_leaving());
    // A member between matches: nothing, and nothing guarded.
    assert_eq!((cost(&eb), cost(&ec)), (None, None));
    assert!(!hb.ui.guards_leaving());
    // A co-host comes first.
    t.alice.borrow_mut().set_co_host(ParticipantId(3), true).unwrap();
    t.settle(400, &mut [&mut ha, &mut hb, &mut hc]);
    assert_eq!(text(&ea), "You are the host. carol will become the host.");
    assert_eq!(cost(&ec), None, "a co-host between matches is not asked");
    // The setting says Close.
    t.alice
        .borrow_mut()
        .set_host_leaves(ddi_room::HostLeaves::Close)
        .unwrap();
    t.settle(400, &mut [&mut ha, &mut hb, &mut hc]);
    assert_eq!(cost(&ea).as_deref(), Some("host_close"));
    assert_eq!(text(&ea), "You are the host. The room will close for everyone.");
    t.alice
        .borrow_mut()
        .set_host_leaves(ddi_room::HostLeaves::PassOn)
        .unwrap();
    t.settle(400, &mut [&mut ha, &mut hb, &mut hc]);

    // A match running (the host picked; the song is being shared).
    let song = songs_of(&["Alpha One"]).songs.remove(0);
    t.alice.borrow_mut().pick(&song, &song.charts[0]).unwrap();
    t.settle(400, &mut [&mut ha, &mut hb, &mut hc]);
    assert!(t.server.borrow().state().phase.kind().match_running());
    assert_eq!(
        text(&ea),
        "You are the host. carol will become the host, and the match goes on without you."
    );
    assert_eq!(cost(&eb).as_deref(), Some("playing"));
    assert_eq!(
        text(&eb),
        "You are in the match in progress. It goes on without you, and you score nothing for the stages you miss."
    );
    assert!(hb.ui.guards_leaving());
    // Someone who is not in the match: the one choosing its songs, a
    // co-host, and a member who only watches.
    t.server.borrow_mut().rewrite(|s| {
        let m = s.current.as_mut().unwrap();
        m.roster.remove(&ParticipantId(2));
        m.roster.remove(&ParticipantId(3));
        // Nobody seated during this match (revision 15's joining during
        // a match would seat them again for its stage not begun).
        m.rules.mid_match_entry = false;
    });
    t.settle(400, &mut [&mut ha, &mut hb, &mut hc]);
    assert_eq!(cost(&eb), None, "a member not in the match");
    assert_eq!(cost(&ec).as_deref(), Some("co_host"));
    assert_eq!(
        text(&ec),
        "You are a co-host. The match in progress goes on without you."
    );
    t.carol.borrow_mut().take_pick().unwrap();
    t.settle(400, &mut [&mut ha, &mut hb, &mut hc]);
    t.settle(400, &mut [&mut ha, &mut hb, &mut hc]);
    assert_eq!(cost(&ec).as_deref(), Some("picking"));
    assert_eq!(
        text(&ec),
        "You are choosing the songs. The host will choose them instead, and the match goes on without you."
    );
    // The member's Back leaves at once, the room told; the co-host's
    // asks, and Stay keeps them in it.
    hb.key("Escape", "Escape");
    t.settle(400, &mut [&mut ha, &mut hb, &mut hc]);
    assert!(!asked(&hb));
    assert_eq!(*calls.borrow(), ["discard"]);
    assert!(t.server.borrow().state().member(ParticipantId(2)).unwrap().left);
    calls.borrow_mut().clear();
    hc.key("Escape", "Escape");
    hc.run(0.3);
    assert!(asked(&hc));
    assert_eq!(hc.ui.focused(), Some("Cancel"));
    hc.key("Escape", "Escape");
    t.settle(400, &mut [&mut ha, &mut hb, &mut hc]);
    assert!(!asked(&hc) && calls.borrow().is_empty());
    assert!(!t.server.borrow().state().member(ParticipantId(3)).unwrap().left);
    assert_eq!(view(&ec).picker, Some(3), "still choosing");
    // The host leaves: asked, then the room does what was said.
    let revision = t.server.borrow().state().revision;
    ha.key("Backspace", "Backspace");
    ha.run(0.3);
    assert!(asked(&ha));
    assert!(
        ha.texts()
            .iter()
            .any(|t| t.starts_with("You are the host. carol will become")),
        "{:?}",
        ha.texts()
    );
    assert_eq!(t.server.borrow().state().revision, revision, "nothing sent yet");
    press(&mut ha, "Confirm");
    t.settle(400, &mut [&mut hc]);
    assert_eq!(*calls.borrow(), ["discard"]);
    let s = t.server.borrow().state().clone();
    assert!(s.member(ParticipantId(1)).unwrap().left);
    assert_eq!(s.owner().map(|m| m.id), Some(ParticipantId(3)));
    assert!(s.phase.kind().match_running(), "the match goes on");
    assert_eq!(view(&ec).online.unwrap().role, "owner");
    // The new host, alone now: the room will close; and it does.
    assert_eq!(cost(&ec).as_deref(), Some("host_alone"));
    assert_eq!(text(&ec), "You are the only one here. The room will close.");
    calls.borrow_mut().clear();
    press(&mut hc, "More");
    press(&mut hc, "Item.leave");
    assert!(asked(&hc));
    press(&mut hc, "Confirm");
    t.step(400);
    assert_eq!(*calls.borrow(), ["discard"]);
    assert_eq!(
        t.server.borrow().state().closed,
        Some(ddi_room::Closure::HostLeft)
    );

    // A link that ended asks nothing: the room closed under its host.
    let t = Three::new(false);
    let ea = env();
    let calls = Calls::default();
    let mut ha = harness();
    ha.push(
        sample::room_select(&ea, t.alice.clone(), songs_of(&["Alpha One"]))
            .with_room(t.alice.clone(), leaving_shell(&t.alice, &orders, &calls)),
    );
    t.settle(400, &mut [&mut ha]);
    assert!(ha.ui.guards_leaving());
    t.server.borrow_mut().fault(
        ParticipantId(1),
        Fault::Close(ddi_room::online::close::ROOM_CLOSED),
    );
    t.settle(400, &mut [&mut ha]);
    assert_eq!(view(&ea).online.unwrap().link, "closed");
    assert_eq!(cost(&ea), None);
    assert!(!ha.ui.guards_leaving());
    ha.key("Escape", "Escape");
    ha.run(0.3);
    assert!(!asked(&ha));
    assert_eq!(*calls.borrow(), ["discard"]);
    let _ = Command::Leave;
}

/// The window asked to close (desktop's close button and Quit,
/// `CLOSE_REQUESTED`): the room's screen asks, the stage's prompt too;
/// Stay keeps the game; Leave tells the room and quits once it has
/// answered. Where nothing guards the screen the shell is not held.
#[test]
fn a_close_request_asks_and_quits_only_on_leave() {
    use crate::UiRequest;
    let quits = |h: &Harness| h.requested(|r| matches!(r, UiRequest::Quit));
    let close = |h: &mut Harness| {
        h.ui.broadcast(crate::Message::new(crate::CLOSE_REQUESTED));
        h.run(0.3);
    };
    // Practice with a match running, on its song select: nothing guards
    // it (the shell quits at once, as anywhere); told all the same, it
    // asks nothing and quits.
    let env = env();
    let room = RoomClient::shared(0, 1);
    room.borrow_mut().set_rules_seen(true);
    let (orders, calls) = (Orders::default(), Calls::default());
    let s = &library("dance-single").songs[0];
    room.borrow_mut().pick(s, &s.charts[0]).unwrap();
    let mut h = harness();
    h.push(
        sample::room_select(&env, room.clone(), library("dance-single"))
            .with_room(room.clone(), leaving_shell(&room, &orders, &calls)),
    );
    h.run(0.5);
    assert!(!h.ui.guards_leaving());
    close(&mut h);
    assert!(!asked(&h));
    assert_eq!(quits(&h), 1);

    // Its stage's prompt: the same.
    let room = RoomClient::shared(0, 1);
    room.borrow_mut().pick(s, &s.charts[0]).unwrap();
    let order = room.borrow_mut().take_stage_order().unwrap();
    let state = Rc::new(RefCell::new(PromptState::default()));
    let mut h = harness();
    h.push(
        PromptScreen::new(env.clone(), state, Rc::new(|_, _| {}))
            .with_room(room.clone(), order.stage, shell(&room, &orders)),
    );
    h.run(0.5);
    assert!(!h.ui.guards_leaving());
    close(&mut h);
    assert!(!asked(&h));
    assert_eq!(quits(&h), 1);

    // An online room's host: asked; Stay keeps the room; asked again
    // while the question is open, it stays one (the shell closes without
    // it); Leave tells the room, then the game quits.
    let t = Three::new(true);
    let ea = env.clone();
    let mut ha = harness();
    ha.push(
        sample::room_select(&ea, t.alice.clone(), songs_of(&["Alpha One"]))
            .with_room(t.alice.clone(), leaving_shell(&t.alice, &orders, &calls)),
    );
    t.settle(400, &mut [&mut ha]);
    assert!(ha.ui.guards_leaving());
    close(&mut ha);
    assert!(asked(&ha) && has(&ha, "You are the host. bob will become the host."));
    assert_eq!(quits(&ha), 0);
    close(&mut ha);
    assert_eq!(
        ha.ui
            .layers()
            .iter()
            .filter(|l| l.name == crate::LEAVE_QUESTION)
            .count(),
        1
    );
    press(&mut ha, "Cancel");
    assert_eq!((quits(&ha), top(&ha).as_str()), (0, "select"));
    close(&mut ha);
    press(&mut ha, "Confirm");
    t.settle(400, &mut [&mut ha]);
    ha.run(1.0);
    assert_eq!(quits(&ha), 1, "once the room answered (or a moment passed)");
    assert!(calls.borrow().is_empty(), "the game quits; no home");
    let s = t.server.borrow().state().clone();
    assert!(s.member(ddi_room::ParticipantId(1)).unwrap().left);
    assert_eq!(s.owner().map(|m| m.id), Some(ddi_room::ParticipantId(2)));
}

/// Watching (`watch-v0`): the player chooses Play or Watch for
/// themselves, from More (a controller's way too) and from the Players
/// pop-up's button; their line says Watching; a pick then starts a stage
/// of the CPUs alone at once, its prompt says Watching in Ready's place
/// and readies nothing, has no attack policy, and starts with the room;
/// its order says the player watches.
#[test]
fn a_player_chooses_to_watch_and_the_stage_starts_without_them() {
    let env = env();
    let room = RoomClient::shared(0, 1);
    room.borrow_mut().set_rules_seen(true);
    let orders = Orders::default();
    let mut h = harness();
    h.push(room_select(&env, room.clone(), "dance-single", &orders));
    h.run(0.5);
    let v = view(&env);
    assert!(!v.watching && v.can_choose_intent, "{v:?}");
    // More, by a controller: Watch only.
    h.pad(crate::nav::MenuAction::More, true);
    h.pad(crate::nav::MenuAction::More, false);
    h.run(0.3);
    assert!(has(&h, "Watch only"), "{:?}", h.texts());
    press(&mut h, "Item.intent");
    assert!(room.borrow().watching());
    assert!(view(&env).watching);
    let me = view(&env)
        .members
        .iter()
        .find(|m| !m.cpu)
        .unwrap()
        .participation
        .clone();
    assert_eq!(me, "watching");
    // The Players pop-up: their line says so, and its button plays again.
    open_players(&mut h);
    assert_eq!(top(&h), "room-players");
    assert!(has(&h, "Watching") && has(&h, "Play"), "{:?}", h.texts());
    // By keys: Left from Close reaches the button, Enter presses it.
    h.key("ArrowLeft", "ArrowLeft");
    h.key("Enter", "Enter");
    h.run(0.3);
    assert!(!room.borrow().watching());
    assert_eq!(top(&h), "room-players", "the pop-up stays");
    assert!(has(&h, "Watch only"), "{:?}", h.texts());
    press(&mut h, super::room_dialogs::INTENT);
    assert!(room.borrow().watching());
    h.key("Escape", "Escape");
    h.run(0.5);
    assert_eq!(top(&h), "select");
    // The watching host picks: the match starts with the CPUs alone,
    // committed at once (nobody is waited for), and its prompt opens.
    h.key("Enter", "Enter");
    h.run(0.5);
    let v = view(&env);
    assert_eq!(v.phase, "countdown", "{v:?}");
    assert_eq!(orders.borrow().len(), 1, "the stage's prompt opened");
    let order = orders.borrow()[0].clone();
    assert!(order.watching);
    assert_eq!(order.cpus.len(), 3);
    assert_eq!(order.others, 0);

    // Its prompt: Watching where Ready is, no policy row, the CPUs listed.
    let seen: Rc<RefCell<Vec<PromptChoice>>> = Rc::default();
    let sink = seen.clone();
    let state = Rc::new(RefCell::new(PromptState::default()));
    state.borrow_mut().info.title = "Single Song".into();
    let mut p = harness();
    p.push(
        PromptScreen::new(
            env.clone(),
            state.clone(),
            Rc::new(move |c, _| sink.borrow_mut().push(c)),
        )
        .with_room(room.clone(), order.stage, shell(&room, &orders)),
    );
    p.run(0.3);
    assert!(has(&p, "Watching"), "{:?}", p.texts());
    assert!(!has(&p, "Ready") && !has(&p, "Targeting"), "{:?}", p.texts());
    let cpu = order.cpus[0].name.clone().unwrap();
    assert!(has(&p, &cpu), "{cpu}: {:?}", p.texts());
    // A press readies nothing; the shell is told (its sound).
    press(&mut p, "Start");
    assert!(!room.borrow().ready());
    assert_eq!(*seen.borrow(), [PromptChoice::Ready]);
    // (This harness's clock starts behind the room's: past it, and the
    // countdown.)
    p.run(12.0);
    assert_eq!(
        seen.borrow().last(),
        Some(&PromptChoice::Start),
        "the room started it"
    );
    assert!(room.borrow().stage_started(order.stage));
}

/// Leaving a watched stage's prompt before its start cancels the start
/// (no stage runs unwatched); and a room where everyone watches and no
/// CPU plays says so in a plain sentence.
#[test]
fn a_watcher_leaving_the_prompt_cancels_the_start_and_all_watching_is_refused_plainly() {
    use crate::state::room::{PromptExit, Refusal, RoomView};
    let mut room = RoomClient::new(0, 1);
    room.set_intent(ddi_room::Intent::Watch).unwrap();
    let s = &library("dance-single").songs[0];
    room.pick(s, &s.charts[0]).unwrap();
    let first = room.stages()[0].id;
    assert!(matches!(room.state().phase, Phase::Countdown { .. }));
    assert_eq!(room.view(), RoomView::Prompt(first));
    assert!(room.take_stage_order().unwrap().watching);
    assert_eq!(room.leave_prompt(first), PromptExit::Withdraw);
    assert!(matches!(
        room.state().phase,
        Phase::Preparing { window: None, .. }
    ));
    assert_eq!(room.view(), RoomView::Select);
    // Nothing runs behind song select.
    room.tick(10_000);
    assert!(matches!(
        room.state().phase,
        Phase::Preparing { window: None, .. }
    ));
    // The sentence for a room where nobody plays.
    let text: String = super::room::refusal_text(&Refusal::Room(
        ddi_room::Rejection::NoPlayingHumans,
    ))
    .into();
    assert_eq!(text, "Everyone is watching. Someone has to play.");
}

/// A close request during a stage's play forfeits the stage and comes to
/// its results (the shells: nothing can be asked over a running play);
/// there the question is asked, saying first that the stage was left,
/// then what leaving the room does. Stay keeps the results (the forfeit
/// is not undone); Leave tells the room and the game quits. A stage's
/// results reached the ordinary way ask too, without the first sentence.
/// CPU practice's results ask nothing.
#[test]
fn a_stages_results_ask_before_the_game_closes_and_say_the_stage_was_left() {
    use super::data::ResultsKind;
    use super::results::{ResultsActions, ResultsScreen};
    use crate::UiRequest;
    let quits = |h: &Harness| h.requested(|r| matches!(r, UiRequest::Quit));
    let close = |h: &mut Harness| {
        h.ui.broadcast(crate::Message::new(crate::CLOSE_REQUESTED));
        h.run(0.3);
    };
    let env = env();
    let results = |room: &SharedRoom, quit: bool| {
        let mut data = sample::results();
        data.kind = ResultsKind::Stage;
        data.quit = quit;
        ResultsScreen::new(env.clone(), data, ResultsActions::default())
            .with_room(room.clone(), ddi_room::StageId(1))
    };
    // An online room's host, on the results of a stage they left.
    let t = Three::new(true);
    let mut h = harness();
    h.push(results(&t.alice, true));
    t.settle(400, &mut [&mut h]);
    assert!(h.ui.guards_leaving());
    close(&mut h);
    assert!(asked(&h), "{}", top(&h));
    let texts = h.texts();
    let line = |want: &str| texts.iter().position(|t| t == want);
    let (left, cost) = (
        line("You left this stage: no points for it."),
        line("You are the host. bob will become the host."),
    );
    assert!(left.is_some() && cost.is_some() && left < cost, "{texts:?}");
    assert_eq!(h.ui.focused(), Some("Cancel"), "Stay is chosen first");
    // Stay: the results as they were.
    h.key("Escape", "Escape");
    h.run(0.3);
    assert_eq!((quits(&h), top(&h).as_str()), (0, "results"));
    // Leave: the room told, then the game quits.
    close(&mut h);
    press(&mut h, "Confirm");
    t.settle(400, &mut [&mut h]);
    h.run(1.0);
    assert_eq!(quits(&h), 1);
    let s = t.server.borrow().state().clone();
    assert!(s.member(ddi_room::ParticipantId(1)).unwrap().left);
    // Reached the ordinary way: asked without the stage's sentence.
    let t = Three::new(true);
    let mut h = harness();
    h.push(results(&t.alice, false));
    t.settle(400, &mut [&mut h]);
    close(&mut h);
    assert!(asked(&h));
    assert!(!has(&h, "You left this stage: no points for it."));
    assert!(has(&h, "You are the host. bob will become the host."));
    // CPU practice: nothing guarded, nothing asked.
    let room = RoomClient::shared(0, 1);
    let s = &library("dance-single").songs[0];
    room.borrow_mut().pick(s, &s.charts[0]).unwrap();
    let mut h = harness();
    h.push(results(&room, true));
    h.run(0.5);
    assert!(!h.ui.guards_leaving());
    close(&mut h);
    assert!(!asked(&h));
    assert_eq!(quits(&h), 1);
}

/// The browser's Back during an online stage's play (the web): the stage
/// is forfeited and watched on, and once its results are up the room's
/// question is asked by itself, saying first that the stage was left
/// (`ResultsScreen::after_back`). Stay keeps the results (and they are
/// not asked again); Leave tells the room and leaves it through the
/// shell (the home). CPU practice's results ask nothing.
#[test]
fn a_stages_results_after_the_browsers_back_ask_and_leave_only_on_leave() {
    use super::data::ResultsKind;
    use super::results::{ResultsActions, ResultsScreen};
    let env = env();
    let left: Rc<RefCell<u32>> = Rc::default();
    let shell = |left: &Rc<RefCell<u32>>| -> RoomShell {
        let left = left.clone();
        Rc::new(move |call, _cx| {
            if matches!(call, RoomCall::Leave | RoomCall::Discard) {
                *left.borrow_mut() += 1;
            }
        })
    };
    let results = |room: &SharedRoom| {
        let mut data = sample::results();
        data.kind = ResultsKind::Stage;
        data.quit = true;
        ResultsScreen::new(env.clone(), data, ResultsActions::default())
            .with_room(room.clone(), ddi_room::StageId(1))
            .after_back(shell(&left))
    };
    let t = Three::new(true);
    let mut h = harness();
    h.push(results(&t.alice));
    t.settle(400, &mut [&mut h]);
    h.run(0.3);
    assert!(asked(&h), "asked by itself: {}", top(&h));
    let texts = h.texts();
    let line = |want: &str| texts.iter().position(|t| t == want);
    assert!(
        line("You left this stage: no points for it.") < line("You are the host. bob will become the host."),
        "{texts:?}"
    );
    assert_eq!(h.ui.focused(), Some("Cancel"), "Stay is chosen first");
    // Stay: the results, not asked again.
    h.key("Escape", "Escape");
    h.run(0.5);
    assert_eq!(top(&h), "results");
    assert!(!asked(&h));
    assert_eq!(*left.borrow(), 0);
    // Asked again by a second Back's way: the results' own question (the
    // close request's path) and Leave: the room told and left.
    h.ui.broadcast(crate::Message::new(crate::CLOSE_REQUESTED));
    h.run(0.3);
    assert!(asked(&h));
    let t2 = Three::new(true);
    let mut h = harness();
    h.push(results(&t2.alice));
    t2.settle(400, &mut [&mut h]);
    h.run(0.3);
    press(&mut h, "Confirm");
    t2.settle(400, &mut [&mut h]);
    h.run(0.5);
    assert_eq!(*left.borrow(), 1, "left through the shell");
    let s = t2.server.borrow().state().clone();
    assert!(s.member(ddi_room::ParticipantId(1)).unwrap().left, "the room told");
    // CPU practice: nothing asked.
    let room = RoomClient::shared(0, 1);
    let s = &library("dance-single").songs[0];
    room.borrow_mut().pick(s, &s.charts[0]).unwrap();
    let mut h = harness();
    h.push(results(&room));
    h.run(0.5);
    assert!(!asked(&h));
    assert_eq!(*left.borrow(), 1);
}

/// A private online room that plays live (the maintainer, 2026-11-13):
/// its Rules show Attacks On by default, the host turns them Off (the
/// summary then says "No attacks") and Reset (the Rules' first form)
/// brings them back On. A room
/// that does not play live keeps its row as before (tested above).
#[test]
fn a_live_online_rooms_attacks_are_on_by_default_and_reset_brings_them_back() {
    use super::room::rules_words;
    use crate::state::online::tests::room::{client, person, song, tick};
    use crate::state::room::StageRules;
    use ddi_room::fake::FakeServer;
    use ddi_room::{Identity, Intent, ManualClock, RoomPolicy};
    let clock = ManualClock::new(1_000);
    let server = FakeServer::hosted(
        "Room000001",
        Identity::new("dev:alice"),
        "alice",
        RoomPolicy::default().with_live(),
        clock.clone(),
    )
    .shared();
    server
        .borrow_mut()
        .join(Identity::new("dev:bob"), "bob", Intent::Play)
        .unwrap();
    assert!(server.borrow().state().next.rules.attacks());
    let alice = Rc::new(RefCell::new(client(
        &server,
        person(&server, "alice"),
        "alice",
        1_000,
    )));
    let step = |ms: u64| tick(&clock, &mut [&mut alice.borrow_mut()], ms);
    let ea = env();
    let orders = Orders::default();
    let mut data = library("dance-single");
    data.songs.insert(0, song());
    let uploads = Uploads::default();
    let mut ha = harness();
    ha.push(
        sample::room_select(&ea, alice.clone(), data)
            .with_room(alice.clone(), shell_with(&alice, &orders, &uploads)),
    );
    step(400);
    step(400);
    ha.run(0.3);
    assert!(alice.borrow().plays_live());
    assert!(view(&ea).attacks_offered);
    assert!(alice.borrow().attacks_on());
    press(&mut ha, "Room.Rules");
    assert_eq!(said(&ha, "Rule.attacks.Value"), "On");
    press(&mut ha, "Dec.attacks");
    assert_eq!(said(&ha, "Rule.attacks.Value"), "Off");
    press(&mut ha, "Apply");
    step(200);
    step(200);
    ha.run(0.3);
    assert!(!alice.borrow().attacks_on());
    assert!(!server.borrow().state().next.rules.attacks());
    let words = {
        let r = alice.borrow();
        String::from(rules_words(
            &StageRules::of(r.rules()),
            r.attacks_offered().then(|| r.rules().attacks()),
        ))
    };
    assert_eq!(words, "DDI · No attacks");
    // Reset (the Rules' first form, under the hidden option): On again.
    ha.push(super::room_dialogs::RulesDialog::first(ea.clone(), alice.clone()));
    ha.run(0.5);
    assert_eq!(said(&ha, "Rule.attacks.Value"), "Off");
    press(&mut ha, "Reset");
    assert_eq!(said(&ha, "Rule.attacks.Value"), "On");
    press(&mut ha, "Ok");
    step(200);
    step(200);
    ha.run(0.3);
    assert!(alice.borrow().attacks_on());
}

/// Walks the focus with the arrow keys (or a pad) until `name` has it.
fn focus_on(h: &mut Harness, name: &str, steps: &[(crate::nav::MenuAction, &str)], pad: bool) {
    for _ in 0..40 {
        if h.ui.focused() == Some(name) {
            return;
        }
        for (action, code) in steps {
            if h.ui.focused() == Some(name) {
                return;
            }
            if pad {
                h.pad(*action, true);
                h.pad(*action, false);
            } else {
                h.key(code, code);
            }
            h.run(0.05);
        }
    }
    panic!("never reached {name}: {:?}", h.ui.focused());
}

/// The Rules pop-up during a match (2026-11-13): before a match it is as
/// it was; once the match runs it says so at its top (the stage, the
/// rules it keeps, that the rows are the next match's), its title is
/// the next match's, a row changed from the running match's is marked,
/// and Apply and restart asks once (Cancel first) and then saves the
/// rules and stops the match: the status line says the next match is
/// under the new rules, the pop-up is back to its between-matches form,
/// and the next pick plays them. By keyboard and controller.
#[test]
fn the_rules_pop_up_during_a_match_says_so_and_restarts_it_on_request() {
    use crate::nav::MenuAction;
    let env = env();
    let room = RoomClient::shared(0, 1);
    room.borrow_mut().set_rules_seen(true);
    let orders = Orders::default();
    let mut h = harness();
    h.push(room_select(&env, room.clone(), "dance-single", &orders));
    h.run(0.5);
    // Between matches: as it was.
    press(&mut h, "Room.Rules");
    let v = view(&env);
    assert!(v.rules_open && !v.rules_running && !v.restart_offered);
    assert!(v.rules_changed.is_empty());
    assert!(has(&h, "Rules") && has(&h, "Next match"), "{:?}", h.texts());
    assert!(!has(&h, "Rules for the next match") && !there(&h, "Restart"));
    assert!(!there(&h, "Running.0"));
    press(&mut h, "Cancel");
    // A match: the host's pick starts it.
    h.key("Enter", "Enter");
    h.run(0.3);
    orders.borrow_mut().clear();
    assert_eq!(view(&env).phase, "preparing");
    press(&mut h, "Room.Rules");
    assert_eq!(top(&h), "room-rules");
    let v = view(&env);
    assert!(v.rules_running && v.restart_offered, "{v:?}");
    assert!(v.rules_changed.is_empty());
    // The edits go from stage 2 first (2026-11-13); the next match's
    // rows one step away.
    assert_eq!(v.rules_destination.as_deref(), Some("stage"));
    assert!(has(&h, "Rules from stage 2"), "{:?}", h.texts());
    press(&mut h, "Inc.dest");
    assert_eq!(view(&env).rules_destination.as_deref(), Some("next"));
    assert!(has(&h, "Rules for the next match"), "{:?}", h.texts());
    assert!(has(&h, "This match is on stage 1 of 4, under these rules:"), "{:?}", h.texts());
    assert!(
        has(&h, "4 stages · 3 CPUs · DDI"),
        "{:?}",
        h.texts()
    );
    assert!(has(&h, "The rows below are for the next match."));
    // The old locked section and its note are the statement now.
    assert!(!there(&h, "Locked.Line") && !there(&h, "Next.Heading"));
    assert!(!has(&h, "Changes apply to the next match; this one keeps its rules."));
    assert!(has(&h, "Apply and restart") && has(&h, "Cancel") && has(&h, "Apply"));
    // A change is marked, and the sentence says what the mark means.
    press(&mut h, "Inc.difficulty");
    assert_eq!(view(&env).rules_changed, vec!["difficulty".to_string()]);
    assert!(there(&h, "Changed.difficulty") && !there(&h, "Changed.boos"));
    assert!(has(
        &h,
        "The rows below are for the next match. ● marks what differs from this one."
    ));
    press(&mut h, "AddRandom");
    assert_eq!(
        view(&env).rules_changed,
        vec!["difficulty".to_string(), "cpus".to_string()]
    );
    press(&mut h, "Remove.3");
    // By keyboard: Left from Apply reaches it; asked once, Cancel first.
    for _ in 0..30 {
        if matches!(h.ui.focused(), Some("Restart" | "Cancel" | "Apply")) {
            break;
        }
        h.key("ArrowDown", "ArrowDown");
        h.run(0.05);
    }
    focus_on(&mut h, "Restart", &[(MenuAction::Left, "ArrowLeft")], false);
    h.key("Enter", "Enter");
    h.run(0.3);
    assert_eq!(top(&h), "room-restart");
    assert_eq!(h.ui.focused(), Some("Cancel"));
    assert!(has(&h, "Restart the match?") && has(&h, "Restart match"));
    h.key("Enter", "Enter");
    h.run(0.3);
    assert_eq!(top(&h), "room-rules", "Cancel changes nothing");
    assert_eq!(view(&env).phase, "preparing");
    assert_eq!(
        view(&env).rules_changed,
        vec!["difficulty".to_string()],
        "the draft kept"
    );
    // By a controller: Restart, then Right to Restart match, Start.
    focus_on(&mut h, "Restart", &[(MenuAction::Left, "ArrowLeft")], true);
    h.pad(MenuAction::Start, true);
    h.pad(MenuAction::Start, false);
    h.run(0.3);
    assert_eq!(top(&h), "room-restart");
    focus_on(&mut h, "Confirm", &[(MenuAction::Right, "ArrowRight")], true);
    h.pad(MenuAction::Start, true);
    h.pad(MenuAction::Start, false);
    h.run(0.5);
    // No stage of the stopped match was finished: no standings pop up
    // (a table of zeros), the room's song select at once.
    assert_eq!(top(&h), "select", "{:?}", h.texts());
    assert!(!has(&h, "Match stopped · finished stages only"));
    let v = view(&env);
    assert_eq!(v.phase, "interrupted");
    assert!(!v.rules_open);
    assert_eq!(
        v.status,
        "Match stopped. Pick a song to start the next match under the new rules."
    );
    assert_eq!(
        room.borrow().state().next.rules.difficulty,
        ddi_room::MatchDifficulty::Hard
    );
    // The stopped match is no running one: the pop-up as between matches.
    press(&mut h, "Room.Rules");
    let v = view(&env);
    assert!(!v.rules_running && !v.restart_offered && v.rules_changed.is_empty());
    assert!(has(&h, "Next match") && !there(&h, "Restart"));
    press(&mut h, "Cancel");
    // The next pick starts a new match under the new rules.
    h.key("Enter", "Enter");
    h.run(0.3);
    let r = room.borrow();
    assert_eq!(r.state().phase.kind(), ddi_room::PhaseKind::Preparing);
    assert_eq!(
        r.locked_rules().unwrap().difficulty,
        ddi_room::MatchDifficulty::Hard
    );
    assert_eq!(r.state().history.len(), 1, "the stopped match recorded");
}

/// The match started on stage 1 with nothing gathering yet: the
/// picker's pick, its window then canceled (the picker backed out of
/// its prompt), as a state rewrite.
fn start_match(s: &mut ddi_room::RoomState) {
    use ddi_room::{Actor, Command, Millis, ParticipantId, StageId};
    let me = Actor::Member(ParticipantId(1));
    let rev = s.revision;
    let content = ddi_room::ContentId {
        song: ddi_room::SongKey::new("song-1"),
        chart: ddi_room::ChartKey::new("digest-1"),
    };
    s.apply(me, Some(rev), &Command::Pick { content, card: None }, Millis(1_000))
        .unwrap();
    let rev = s.revision;
    s.apply(me, Some(rev), &Command::CancelStart { stage: StageId(1) }, Millis(1_000))
        .unwrap();
}

/// An online room during a match: a member sees the same statement with
/// the rows read only and no restart; a co-host restarts it for
/// everyone.
#[test]
fn an_online_rooms_restart_is_for_its_operators() {
    let start = start_match;
    let t = Three::new(true);
    let (ea, eb, ec) = (env(), env(), env());
    let orders = Orders::default();
    let mut ha = online_select(&ea, &t.alice, songs_of(&["Alpha One"]), &orders);
    let mut hb = online_select(&eb, &t.bob, songs_of(&["Bravo One"]), &orders);
    let mut hc = online_select(&ec, &t.carol, songs_of(&["Charlie One"]), &orders);
    t.settle(400, &mut [&mut ha, &mut hb, &mut hc]);
    t.server.borrow_mut().rewrite(start);
    t.settle(400, &mut [&mut ha, &mut hb, &mut hc]);
    assert_eq!(view(&ec).phase, "preparing");
    // The member: the statement, the rows read only, Close alone.
    press(&mut hc, "Room.Rules");
    let v = view(&ec);
    assert!(v.rules_running && !v.restart_offered, "{v:?}");
    assert!(has(&hc, "Rules for the next match"));
    assert!(has(&hc, "This match is on stage 1 of 4 and keeps these rules:"), "{:?}", hc.texts());
    assert!(!there(&hc, "Restart") && !there(&hc, "Inc.boos") && !there(&hc, "Apply"));
    press(&mut hc, "Cancel");
    // The co-host: Boos on, Apply and restart, Restart match.
    press(&mut hb, "Room.Rules");
    assert!(view(&eb).restart_offered);
    press(&mut hb, "Inc.boos");
    assert_eq!(view(&eb).rules_changed, vec!["boos".to_string()]);
    press(&mut hb, "Restart");
    assert_eq!(top(&hb), "room-restart");
    press(&mut hb, "Confirm");
    t.settle(400, &mut [&mut ha, &mut hb, &mut hc]);
    assert!(t.server.borrow().state().next.rules.boos);
    for e in [&ea, &eb, &ec] {
        assert_eq!(view(e).phase, "interrupted");
    }
    assert_eq!(
        view(&ea).status,
        "Match stopped. Pick a song to start the next match under the new rules."
    );
}

/// The Rules pop-up during a match whose room takes changes from a later
/// stage on (2026-11-13): its three destinations. From stage N (first):
/// only the per-stage rows change, the change lands on the stage named,
/// the status line says it for everyone until that stage begins, and
/// that stage's order carries its rules while the earlier one keeps the
/// match's; Next match saves the draft and leaves the match alone; Apply
/// and restart takes what the pop-up shows. The Standings mark the stages
/// played under other rules. By keyboard as well.
#[test]
fn the_rules_pop_up_changes_rules_from_the_next_stage_next_match_or_by_restart() {
    use ddi_room::{Finish, Finisher, MatchDifficulty};
    let env = env();
    let room = RoomClient::shared(0, 1);
    room.borrow_mut().set_rules_seen(true);
    let orders = Orders::default();
    let mut h = harness();
    h.push(room_select(&env, room.clone(), "dance-single", &orders));
    h.run(0.5);
    // Between matches: no destination, the pop-up as it was.
    press(&mut h, "Room.Rules");
    assert_eq!(view(&env).rules_destination, None);
    assert!(!there(&h, "Inc.dest"));
    press(&mut h, "Cancel");
    // Stage 1 picked: its window open, a change lands on stage 2.
    h.key("Enter", "Enter");
    h.run(0.3);
    let order1 = orders.borrow_mut().pop().expect("a prompt");
    press(&mut h, "Room.Rules");
    let v = view(&env);
    assert_eq!(v.rules_destination.as_deref(), Some("stage"));
    assert_eq!(v.rules_from, Some(2));
    assert!(has(&h, "Rules from stage 2") && has(&h, "Changes apply") && has(&h, "From stage 2"));
    assert!(h.texts().concat().contains("Timing, Boos, zero life, attacks and Points below apply from stage 2 on.") || h.texts().join(" ").contains("Timing, Boos, zero life, attacks and Points below apply from stage 2 on."), "{:?}", h.texts());
    assert!(
        h.texts()
            .iter()
            .any(|t| t.starts_with("Stages and CPUs can't change during a match")),
        "{:?}",
        h.texts()
    );
    // The rows that never change mid-match are shown, not offered (Points
    // may, since 2026-11-13).
    assert!(!there(&h, "Inc.stages") && !there(&h, "AddRandom") && there(&h, "Inc.points"));
    assert!(there(&h, "Inc.difficulty") && there(&h, "Inc.attacks"));
    press(&mut h, "Inc.difficulty");
    assert_eq!(view(&env).rules_changed, vec!["difficulty".to_string()]);
    assert!(h.texts().concat().contains("Timing, Boos, zero life, attacks and Points below apply from stage 2 on. ● marks what you changed.") || h.texts().join(" ").contains("Timing, Boos, zero life, attacks and Points below apply from stage 2 on. ● marks what you changed."), "{:?}", h.texts());
    press(&mut h, "Apply");
    assert_eq!(top(&h), "select");
    {
        let r = room.borrow();
        let s = r.state();
        let m = s.current.as_ref().unwrap();
        assert_eq!(m.rules_for(m.stages[1].id).difficulty, MatchDifficulty::Hard);
        assert_eq!(m.rules_for(order1.stage).difficulty, MatchDifficulty::Normal);
        assert_eq!(s.next.rules.difficulty, MatchDifficulty::Normal, "the next match's alone");
    }
    assert_eq!(order1.rules.difficulty, MatchDifficulty::Normal);
    // About stage 1 (its readiness gathering): that first, the change for
    // stage 2 after it.
    let status = view(&env).status;
    assert!(status.ends_with(" · From stage 2 on: Hard timing"), "{status}");
    // Opened again: the stage's rules as they now are, nothing marked.
    press(&mut h, "Room.Rules");
    assert!(view(&env).rules_changed.is_empty());
    assert_eq!(
        view(&env).stage_draft_rules.map(|r| r.0),
        Some("hard".to_string())
    );
    // Next match, by keyboard: the destination's ›, then Boos on, Apply.
    use crate::nav::MenuAction;
    focus_on(
        &mut h,
        "Inc.dest",
        &[(MenuAction::Up, "ArrowUp"), (MenuAction::Right, "ArrowRight")],
        false,
    );
    h.key("Enter", "Enter");
    h.run(0.3);
    assert_eq!(view(&env).rules_destination.as_deref(), Some("next"));
    assert!(has(&h, "Rules for the next match"));
    press(&mut h, "Inc.boos");
    press(&mut h, "Apply");
    {
        let r = room.borrow();
        let s = r.state();
        assert!(s.next.rules.boos, "saved for the next match");
        let m = s.current.as_ref().unwrap();
        assert!(!m.rules_for(m.stages[1].id).boos, "the match left alone");
    }
    // A change for a stage that has begun since: refused, said.
    let s1 = order1.stage;
    let hard = room.borrow().rules_ahead().unwrap();
    assert!(room.borrow_mut().change_stage_rules(s1, &hard).is_err());
    h.run(0.3);
    assert_eq!(
        view(&env).status,
        "The stage these changes were for has begun since. Check where they apply now, then Apply again."
    );
    room.borrow_mut().clear_refusal();
    // Stage 1 played under the match's rules, stage 2 under Hard timing.
    let finish = |order: &StageOrder| {
        // (Ready already, or counting down.)
        let _ = room.borrow_mut().set_ready(true);
        room.borrow_mut().clear_refusal();
        let later = room.borrow().now_ms() + 4_000;
        room.borrow_mut().tick(later);
        let mut ids: Vec<_> = order.cpus.iter().map(|c| c.participant).collect();
        ids.insert(0, order.me);
        let ranking = ids
            .into_iter()
            .map(|participant| {
                vec![Finisher {
                    participant,
                    finish: Finish::Survived,
                }]
            })
            .collect();
        room.borrow_mut()
            .report_stage(order.stage, ranking, false)
            .unwrap();
    };
    finish(&order1);
    h.run(0.5);
    assert!(!room.borrow().stage_table(s1).unwrap().rules.changed);
    press(&mut h, "Continue");
    h.run(0.3);
    assert!(
        view(&env).status.starts_with("From stage 2: Hard timing. "),
        "said until stage 2 begins: {}",
        view(&env).status
    );
    h.key("Enter", "Enter");
    h.run(0.3);
    let order2 = orders.borrow_mut().pop().expect("stage 2's prompt");
    assert_eq!(order2.rules.difficulty, MatchDifficulty::Hard);
    assert!(order2.rules.changed);
    assert!(!view(&env).status.starts_with("From stage 2"), "stage 2 begun");
    finish(&order2);
    h.run(0.5);
    let table = room.borrow().stage_table(order2.stage).unwrap();
    assert!(table.rules.changed && table.rules.difficulty == MatchDifficulty::Hard);
    // The match's standings mark it.
    assert!(
        h.texts()
            .iter()
            .any(|t| t == "● From stage 2: DDI · Hard timing"),
        "{:?}",
        h.texts()
    );
    assert!(there(&h, "S1.Changed") && !there(&h, "S0.Changed"));
    press(&mut h, "Continue");
    // Apply and restart takes what the pop-up shows: Attacks off from
    // stage 3 becomes the next match's rule.
    press(&mut h, "Room.Rules");
    assert_eq!(view(&env).rules_from, Some(3));
    press(&mut h, "Dec.attacks");
    press(&mut h, "Restart");
    press(&mut h, "Confirm");
    let r = room.borrow();
    assert!(!r.state().next.rules.attacks());
    // The per-stage rows as shown (stage 3's: Hard timing, no Boos).
    assert_eq!(r.state().next.rules.difficulty, MatchDifficulty::Hard);
    assert!(!r.state().next.rules.boos);
    assert_eq!(r.state().phase.kind(), ddi_room::PhaseKind::Interrupted);
}

/// An online room over the fake server (2026-11-13): a co-host changes
/// the running match's rules from the stage nothing has begun for, through
/// the Rules pop-up; every device's mirror keeps the change and every
/// status line says it; a member is never offered it.
#[test]
fn an_online_rooms_operator_changes_rules_from_the_next_stage_for_everyone() {
    let start = start_match;
    let t = Three::new(true);
    let (ea, eb, ec) = (env(), env(), env());
    let orders = Orders::default();
    let mut ha = online_select(&ea, &t.alice, songs_of(&["Alpha One"]), &orders);
    let mut hb = online_select(&eb, &t.bob, songs_of(&["Bravo One"]), &orders);
    let mut hc = online_select(&ec, &t.carol, songs_of(&["Charlie One"]), &orders);
    t.settle(400, &mut [&mut ha, &mut hb, &mut hc]);
    t.server.borrow_mut().rewrite(start);
    t.settle(400, &mut [&mut ha, &mut hb, &mut hc]);
    // Stage 1 gathers nobody yet: a change lands on it.
    press(&mut hc, "Room.Rules");
    assert_eq!(view(&ec).rules_destination, None, "a member: not offered");
    press(&mut hc, "Cancel");
    press(&mut hb, "Room.Rules");
    assert_eq!(view(&eb).rules_destination.as_deref(), Some("stage"));
    assert_eq!(view(&eb).rules_from, Some(1));
    press(&mut hb, "Inc.boos");
    press(&mut hb, "Apply");
    t.settle(400, &mut [&mut ha, &mut hb, &mut hc]);
    let server = t.server.borrow().state().clone();
    let m = server.current.as_ref().unwrap();
    assert!(m.rules_for(m.stages[0].id).boos);
    assert!(!m.rules.boos && !server.next.rules.boos);
    for (room, e) in [(&t.alice, &ea), (&t.bob, &eb), (&t.carol, &ec)] {
        assert_eq!(room.borrow().state(), &server, "the mirror is the room");
        let status = view(e).status;
        assert!(status.starts_with("From stage 1: Boos. "), "{status}");
    }
}

/// CPUs in an online room (`ddi_room::live::cpus`, `online-cpus-v0`): the
/// Rules pop-up offers the roster as practice's where the room plays live;
/// the roster saved keeps the room's seats for people still to come; a
/// room that does not play live is offered none and sent none.
#[test]
fn an_online_rooms_rules_offer_the_roster_where_it_plays_live() {
    use ddi_room::RoomPolicy;
    for (policy, offered) in [
        (RoomPolicy::default().with_live(), true),
        (RoomPolicy::default(), false),
    ] {
        let p = policy.clone();
        let t = Three::over(move |s| s.policy = p, false);
        let (ea, eb, ec) = (env(), env(), env());
        let orders = Orders::default();
        let mut ha = online_select(&ea, &t.alice, songs_of(&["Alpha One"]), &orders);
        let mut hb = online_select(&eb, &t.bob, songs_of(&["Bravo One"]), &orders);
        let mut hc = online_select(&ec, &t.carol, songs_of(&["Charlie One"]), &orders);
        t.settle(400, &mut [&mut ha, &mut hb, &mut hc]);
        press(&mut ha, "Room.Rules");
        assert_eq!(top(&ha), "room-rules");
        assert_eq!(there(&ha, "AddRandom"), offered, "{policy:?}");
        if !offered {
            press(&mut ha, "Cancel");
            continue;
        }
        press(&mut ha, "AddRandom");
        press(&mut ha, "AddRandom");
        press(&mut ha, "Apply");
        t.settle(400, &mut [&mut ha, &mut hb, &mut hc]);
        let server = t.server.borrow().state().clone();
        assert_eq!(server.next.rules.cpu.roster.len(), 2);
        assert_eq!(
            server.next.rules.seats,
            RoomPolicy::INITIAL_COMPETITOR_CAP,
            "the seats kept for people still to come"
        );
        // Everyone sees them, as stars, never as Bot.
        for (room, h) in [(&t.alice, &ha), (&t.bob, &hb), (&t.carol, &hc)] {
            assert!(room.borrow().has_cpus());
            assert!(!h.texts().iter().any(|x| x.contains("Bot")), "{:?}", h.texts());
        }
    }
}

/// Points from a later stage (the maintainer, 2026-11-13): in the Rules
/// pop-up's From-stage mode the Points row is editable; where it differs
/// from the stage before, a quiet sentence under it says the totals will
/// add stages awarded on different curves (and Apply and restart is
/// there). Applied, the status line names it in the rules' words and the
/// stage's order, its Standings and the match's standings mark it.
#[test]
fn points_change_from_the_next_stage_with_the_host_told_the_totals_shift() {
    use ddi_room::{Finish, Finisher, PointsBias};
    let env = env();
    let room = RoomClient::shared(0, 1);
    room.borrow_mut().set_rules_seen(true);
    let orders = Orders::default();
    let mut h = harness();
    h.push(room_select(&env, room.clone(), "dance-single", &orders));
    h.run(0.5);
    h.key("Enter", "Enter");
    h.run(0.3);
    let order1 = orders.borrow_mut().pop().expect("a prompt");
    press(&mut h, "Room.Rules");
    assert_eq!(view(&env).stage_draft_points.as_deref(), Some("strong"));
    assert_eq!(view(&env).points_shift, None);
    assert!(!h.texts().iter().any(|t| t.contains("different curves")));
    press(&mut h, "Inc.points");
    let v = view(&env);
    assert_eq!(v.stage_draft_points.as_deref(), Some("medium"));
    assert_eq!(v.points_shift.as_deref(), Some("strong"));
    assert_eq!(v.rules_changed, vec!["points".to_string()]);
    let words = h.texts().join(" ");
    assert!(
        words.contains("Stages before stage 2 keep Strong points")
            && words.contains("different curves")
            && words.contains("Apply and restart"),
        "{words}"
    );
    // The next match's draft is left alone.
    assert_eq!(view(&env).draft_points.as_deref(), Some("strong"));
    press(&mut h, "Apply");
    {
        let r = room.borrow();
        let s = r.state();
        let m = s.current.as_ref().unwrap();
        assert_eq!(m.rules_for(m.stages[1].id).points, PointsBias::Medium);
        assert_eq!(m.rules_for(order1.stage).points, PointsBias::Strong);
        assert_eq!(m.rules.points, PointsBias::Strong);
        assert_eq!(s.next.rules.points, PointsBias::Strong);
        // Kept in the one form: the bias named, the rest the match's.
        assert_eq!(m.changes[0].rules.points, Some(PointsBias::Medium));
    }
    // The line is about stage 1 (its readiness gathering): it comes first,
    // the change for stage 2 after it (2026-11-13).
    let status = view(&env).status;
    assert!(status.ends_with(" · From stage 2 on: Medium points"), "{status}");
    assert!(!status.starts_with("From stage 2"), "{status}");
    // Opened again: Medium is stage 2's now, nothing marked, and the
    // sentence still says the stages before keep Strong.
    press(&mut h, "Room.Rules");
    assert!(view(&env).rules_changed.is_empty());
    assert_eq!(view(&env).points_shift.as_deref(), Some("strong"));
    press(&mut h, "Cancel");
    let finish = |order: &StageOrder| {
        let _ = room.borrow_mut().set_ready(true);
        room.borrow_mut().clear_refusal();
        let later = room.borrow().now_ms() + 4_000;
        room.borrow_mut().tick(later);
        let mut ids: Vec<_> = order.cpus.iter().map(|c| c.participant).collect();
        ids.insert(0, order.me);
        let ranking = ids
            .into_iter()
            .map(|participant| {
                vec![Finisher {
                    participant,
                    finish: Finish::Survived,
                }]
            })
            .collect();
        room.borrow_mut()
            .report_stage(order.stage, ranking, false)
            .unwrap();
    };
    finish(&order1);
    h.run(0.5);
    // Between stages, the line about stage 2: the change first.
    let status = view(&env).status;
    assert!(
        status.starts_with("From stage 2: Medium points. ") && status.contains("stage 2"),
        "{status}"
    );
    press(&mut h, "Continue");
    h.run(0.3);
    h.key("Enter", "Enter");
    h.run(0.3);
    let order2 = orders.borrow_mut().pop().expect("stage 2's prompt");
    assert_eq!(order2.rules.points, PointsBias::Medium);
    assert!(order2.rules.changed);
    // Points alone leave the battle plan as it was.
    assert_eq!(order2.rules.changed_tag(), None);
    finish(&order2);
    h.run(0.5);
    let table = room.borrow().stage_table(order2.stage).unwrap();
    assert!(table.rules.changed && table.rules.points == PointsBias::Medium);
    assert!(
        h.texts()
            .iter()
            .any(|t| t == "● From stage 2: DDI · Medium points"),
        "{:?}",
        h.texts()
    );
    assert!(there(&h, "S1.Changed") && !there(&h, "S0.Changed"));
    // A later change of the timing alone keeps Medium (it never puts
    // Points back to the match's).
    press(&mut h, "Continue");
    h.run(0.3);
    press(&mut h, "Room.Rules");
    assert_eq!(view(&env).stage_draft_points.as_deref(), Some("medium"));
    press(&mut h, "Inc.difficulty");
    press(&mut h, "Apply");
    let r = room.borrow();
    let (from, _) = r.rules_land_on().unwrap();
    let m = r.state().current.clone().unwrap();
    assert_eq!(m.rules_for(from).points, PointsBias::Medium);
    assert_eq!(m.rules_for(from).difficulty, ddi_room::MatchDifficulty::Hard);
}

/// A row that grows stays in view (2026-11-13): Points changed with the
/// mouse at the foot of the Rules pop-up's scrolled rows brings the
/// sentence that appears under it into view too.
#[test]
fn a_rows_new_sentence_is_scrolled_into_view() {
    let env = env();
    let room = RoomClient::shared(0, 1);
    room.borrow_mut().set_rules_seen(true);
    // Enough CPUs that the rows scroll.
    {
        let mut r = room.borrow_mut();
        let roster = crate::state::cpu_roster::at_level(&[], 4, 12, 3);
        let rules = crate::state::cpu_roster::with_roster(r.rules().clone(), roster, 1);
        r.save_rules(rules).unwrap();
    }
    let orders = Orders::default();
    let mut h = harness();
    h.push(room_select(&env, room.clone(), "dance-single", &orders));
    h.run(0.5);
    h.key("Enter", "Enter");
    h.run(0.3);
    press(&mut h, "Room.Rules");
    assert!(!h.texts().iter().any(|t| t.contains("different curves")));
    // The Points row brought just into view at the rows' foot, by the
    // wheel (as a mouse would), then pressed.
    let shown = |h: &Harness, name: &str| {
        let (Some(b), Some(v)) = (h.ui.bounds(name), h.ui.visible_bounds(name)) else {
            return false;
        };
        b.iter().zip(v).all(|(a, b)| (a - b).abs() < 0.5)
    };
    for _ in 0..200 {
        if shown(&h, "Inc.points") {
            break;
        }
        let at = h.centre("Title");
        h.wheel([at[0], at[1] + 80.0], [0.0, 10.0], crate::event::WheelSource::Pixels);
        h.run(0.05);
    }
    assert!(shown(&h, "Inc.points"));
    h.tap(PointerKind::Mouse, "Inc.points");
    h.run(0.5);
    assert!(h.texts().iter().any(|t| t.contains("different curves")), "{:?}", h.texts());
    for k in 0..3 {
        let name = format!("Rule.points.About{k}");
        if h.ui.bounds(&name).is_some() {
            assert!(shown(&h, &name), "{name} under the visible rows");
        }
    }
}

/// An online room's Points from a later stage: a co-host's Points reach
/// every mirror and every status line.
#[test]
fn an_online_rooms_points_change_from_a_later_stage() {
    use ddi_room::PointsBias;
    let start = start_match;
    let orders = Orders::default();
    // The co-host's Mild from stage 1 reaches everyone.
    let t = Three::new(true);
    let (ea, eb, ec) = (env(), env(), env());
    let mut ha = online_select(&ea, &t.alice, songs_of(&["Alpha One"]), &orders);
    let mut hb = online_select(&eb, &t.bob, songs_of(&["Bravo One"]), &orders);
    let mut hc = online_select(&ec, &t.carol, songs_of(&["Charlie One"]), &orders);
    t.settle(400, &mut [&mut ha, &mut hb, &mut hc]);
    t.server.borrow_mut().rewrite(start);
    t.settle(400, &mut [&mut ha, &mut hb, &mut hc]);
    press(&mut hb, "Room.Rules");
    assert_eq!(view(&eb).rules_from, Some(1));
    press(&mut hb, "Inc.points");
    press(&mut hb, "Inc.points");
    assert_eq!(view(&eb).stage_draft_points.as_deref(), Some("mild"));
    press(&mut hb, "Apply");
    t.settle(400, &mut [&mut ha, &mut hb, &mut hc]);
    let server = t.server.borrow().state().clone();
    let m = server.current.as_ref().unwrap();
    assert_eq!(m.rules_for(m.stages[0].id).points, PointsBias::Mild);
    assert_eq!(m.rules.points, PointsBias::Strong);
    for (room, e) in [(&t.alice, &ea), (&t.bob, &eb), (&t.carol, &ec)] {
        assert_eq!(room.borrow().state(), &server, "the mirror is the room");
        let status = view(e).status;
        assert!(status.starts_with("From stage 1: Mild points. "), "{status}");
    }
}

/// Whether `name` is on view whole (not cut by the Rules pop-up's rows'
/// clip).
fn whole(h: &Harness, name: &str) -> bool {
    match (h.ui.bounds(name), h.ui.visible_bounds(name)) {
        (Some(b), Some(v)) => b.iter().zip(v).all(|(a, b)| (a - b).abs() < 0.5),
        _ => false,
    }
}

/// The Rules pop-up fits every screen (2026-11-13): its rows scroll
/// inside it between its title and its buttons, which stay on view;
/// focus moved by keys or a controller scrolls its row into view, and the
/// wheel scrolls them. Its notes show only where they apply: the stars'
/// with CPUs in the roster, never "Applying clears readiness" with nobody
/// ready.
#[test]
fn the_rules_pop_up_scrolls_its_rows_with_its_title_and_buttons_on_view() {
    use crate::nav::MenuAction;
    for (size, pad) in [([1280.0, 720.0], false), ([844.0, 390.0], true)] {
        let env = env();
        let room = RoomClient::shared(0, 1);
        room.borrow_mut().set_rules_seen(true);
        let orders = Orders::default();
        let mut h = Harness::new(DESIGN, size, 1.0, TextMode::Own);
        h.push(room_select(&env, room.clone(), "dance-single", &orders));
        h.run(0.5);
        press(&mut h, "Room.Rules");
        assert_eq!(top(&h), "room-rules");
        assert!(has(&h, "A CPU's stars show its level: the charts it keeps up with, not a skill rating."));
        assert!(!has(&h, "Applying clears readiness."), "nobody is ready");
        // Twelve CPUs: more rows than the screen holds.
        while view(&env).draft[1] < 12 {
            press(&mut h, "AddRandom");
        }
        for name in ["Title", "Cancel", "Apply"] {
            assert!(whole(&h, name), "{name} on view at {size:?}");
        }
        let panel_bottom = h.ui.bounds("Cancel").unwrap()[3];
        let screen_h = h.ui.viewport().screen.height;
        assert!(panel_bottom <= screen_h, "{panel_bottom} of {screen_h} at {size:?}");
        // Some row is scrolled away, and focus brings each into view.
        assert!(
            !whole(&h, "Inc.stages") || !whole(&h, "Inc.points"),
            "the rows scroll at {size:?}"
        );
        let (down, up) = ((MenuAction::Down, "ArrowDown"), (MenuAction::Up, "ArrowUp"));
        focus_on(&mut h, "Dec.points", &[down], pad);
        h.run(0.1);
        assert!(whole(&h, "Dec.points"), "scrolled to Points at {size:?}");
        assert!(!whole(&h, "Dec.stages"));
        focus_on(&mut h, "Dec.stages", &[up], pad);
        h.run(0.1);
        assert!(whole(&h, "Dec.stages"), "scrolled back at {size:?}");
        // The wheel scrolls them too.
        let at = h.centre("Title");
        h.wheel([at[0], at[1] + 80.0], [0.0, 4000.0], crate::event::WheelSource::Pixels);
        h.run(0.1);
        assert!(!whole(&h, "Dec.stages") && whole(&h, "Dec.points"));
        // No CPUs: no stars' note.
        for k in (0..12).rev() {
            press(&mut h, &format!("Remove.{k}"));
        }
        assert_eq!(view(&env).draft[1], 0);
        assert!(!has(&h, "A CPU's stars show its level: its strength relative to the chart, not a skill rating."));
    }
}

/// The player's own name and its "You" badge at narrow widths: both where
/// both fit; the badge dropped first, the name whole; only then the name
/// cut (round D, item 5).
#[test]
fn the_you_badge_goes_before_the_name_is_cut() {
    let env = env();
    let name = "alice_the_long";
    let full = crate::look::mono_width(name, 15.0);
    let chip = super::room::badge_width(15.0);
    let named = |room: f32| {
        let (parts, w) =
            super::room::name_and_badge(&env, "N", name, true, (15.0, 600, "Text"), [0.0, 0.0], room);
        let names: Vec<String> = parts.iter().map(|a| a.name.clone()).collect();
        (names, w)
    };
    let (both, w) = named(full + chip + 1.0);
    assert_eq!(both, ["N", "N.Badge", "N.BadgeText"]);
    assert!((w - (full + chip)).abs() < 0.01);
    let (dropped, w) = named(full + chip - 1.0);
    assert_eq!(dropped, ["N"], "the badge goes first");
    assert!((w - full).abs() < 0.01, "the name whole");
    let (cut, w) = named(full - 20.0);
    assert_eq!(cut, ["N"]);
    assert!(w < full, "then the name is cut");
    // Nobody else's row has one.
    let (none, _) =
        super::room::name_and_badge(&env, "N", name, false, (15.0, 600, "Text"), [0.0, 0.0], 999.0);
    assert_eq!(none.len(), 1);
}

/// A pick backed out of (follow-up round F, item 2): the picker's prompt
/// left, the room back to choosing with stage 1 not played, the Rules
/// pop-up's change lands on stage 1 (it said stage 2 before the fix), and
/// stage 1's prompt then carries the changed rules; picked again, a
/// change lands on stage 2 as before.
#[test]
fn a_pick_backed_out_of_leaves_the_rules_for_stage_one() {
    let env = env();
    let room = RoomClient::shared(0, 1);
    room.borrow_mut().set_rules_seen(true);
    let orders = Orders::default();
    let mut h = harness();
    h.push(room_select(&env, room.clone(), "dance-single", &orders));
    h.run(0.5);
    h.key("Enter", "Enter");
    h.run(0.3);
    let order1 = orders.borrow_mut().pop().expect("a prompt");
    press(&mut h, "Room.Rules");
    assert_eq!(view(&env).rules_from, Some(2), "picked: the stage after");
    press(&mut h, "Cancel");
    h.run(0.3);
    // Backed out of the prompt: the match runs, stage 1 unplayed.
    room.borrow_mut().leave_prompt(order1.stage);
    h.run(0.3);
    assert!(room.borrow().state().current.is_some());
    press(&mut h, "Room.Rules");
    assert_eq!(view(&env).rules_from, Some(1));
    assert!(has(&h, "From stage 1"), "{:?}", h.texts());
    press(&mut h, "Inc.difficulty");
    press(&mut h, "Apply");
    h.run(0.3);
    let r = room.borrow();
    let m = r.state().current.clone().unwrap();
    assert_eq!(m.rules_for(order1.stage).difficulty, ddi_room::MatchDifficulty::Hard);
    drop(r);
    // Picked again: its prompt plays under them; a change lands on 2.
    h.key("Enter", "Enter");
    h.run(0.3);
    let again = orders.borrow_mut().pop().expect("a prompt again");
    assert_eq!(again.stage, order1.stage);
    assert_eq!(again.rules.difficulty, ddi_room::MatchDifficulty::Hard);
    assert_eq!(room.borrow().rules_land_on().map(|(_, n)| n), Some(2));
}

/// The same online (over the fake server): stage 1 picked, its share up,
/// a member loaded and ready for it, then the picker backed out and
/// another song rests under the spotlight, shared: a co-host's Rules say
/// From stage 1, and the change lands there for everyone.
#[test]
fn an_online_pick_backed_out_of_leaves_the_rules_for_stage_one() {
    use ddi_room::{
        Actor, ChartKey, Command, ContentId, ContentStatus, Loading, Millis, ParticipantId,
        SongKey,
    };
    let content = |n: u32| ContentId {
        song: SongKey::new(format!("song-{n}")),
        chart: ChartKey::new(format!("digest-{n}")),
    };
    let t = Three::new(true);
    let (ea, eb, ec) = (env(), env(), env());
    let orders = Orders::default();
    let mut ha = online_select(&ea, &t.alice, songs_of(&["Alpha One"]), &orders);
    let mut hb = online_select(&eb, &t.bob, songs_of(&["Bravo One"]), &orders);
    let mut hc = online_select(&ec, &t.carol, songs_of(&["Charlie One"]), &orders);
    t.settle(400, &mut [&mut ha, &mut hb, &mut hc]);
    t.server.borrow_mut().rewrite(move |s| {
        let by = |s: &mut ddi_room::RoomState, who: u64, cmd: Command| {
            let rev = s.revision;
            s.apply(Actor::Member(ParticipantId(who)), Some(rev), &cmd, Millis(1_000))
                .unwrap();
        };
        by(s, 1, Command::Pick { content: content(1), card: None });
        let stage = s.upcoming_stage().unwrap();
        s.apply(
            Actor::Authority,
            None,
            &Command::SetContentStatus {
                stage,
                status: ContentStatus::Available,
                share: Some("share-1".into()),
            },
            Millis(1_000),
        )
        .unwrap();
        let rev = s.stage(stage).unwrap().readiness_rev;
        by(s, 3, Command::ReportLoading { stage, readiness_rev: rev, loading: Loading::Loaded });
        by(s, 3, Command::SetReady { stage, readiness_rev: rev, ready: true });
        by(s, 1, Command::CancelStart { stage });
        by(s, 1, Command::Spotlight { content: Some(content(2)), card: None });
    });
    t.settle(400, &mut [&mut ha, &mut hb, &mut hc]);
    press(&mut hb, "Room.Rules");
    assert_eq!(view(&eb).rules_destination.as_deref(), Some("stage"));
    assert_eq!(view(&eb).rules_from, Some(1));
    press(&mut hb, "Inc.boos");
    press(&mut hb, "Apply");
    t.settle(400, &mut [&mut ha, &mut hb, &mut hc]);
    let server = t.server.borrow().state().clone();
    let m = server.current.as_ref().unwrap();
    assert!(m.rules_for(m.stages[0].id).boos, "stage 1 under them");
}

/// `hosted-deadline-v2` (follow-up round F, item 3) on everyone's screens:
/// after the pick the status line says how many the room waits for to
/// load and the grace's seconds, a slow loader's row says Loading, the
/// prompt's rows too; once everyone has loaded the line is the Ready
/// countdown's 15 s.
#[test]
fn everyone_sees_who_is_loading_and_then_the_ready_countdown() {
    use crate::state::online::tests::room::{client, person, prepare, song, tick, upload};
    use ddi_room::fake::FakeServer;
    use ddi_room::{Identity, Intent, ManualClock, RoomPolicy};
    let clock = ManualClock::new(1_000);
    let mut policy = RoomPolicy::default();
    policy.readiness.window_ms = 15_000;
    let server = FakeServer::hosted(
        "Room000001",
        Identity::new("dev:alice"),
        "alice",
        policy,
        clock.clone(),
    )
    .shared();
    for who in ["bob", "carol"] {
        server
            .borrow_mut()
            .join(Identity::new(format!("dev:{who}")), who, Intent::Play)
            .unwrap();
    }
    let mk = |who: &'static str| Rc::new(RefCell::new(client(&server, person(&server, who), who, 1_000)));
    let (alice, bob, carol) = (mk("alice"), mk("bob"), mk("carol"));
    let step = |ms: u64| {
        tick(
            &clock,
            &mut [&mut alice.borrow_mut(), &mut bob.borrow_mut(), &mut carol.borrow_mut()],
            ms,
        )
    };
    step(400);
    let s = song();
    alice.borrow_mut().pick(&s, &s.charts[0]).unwrap();
    step(400);
    upload(&server, &mut alice.borrow_mut(), "abc12345");
    step(400);
    // Alice and bob load; carol's device is slow.
    prepare(&mut alice.borrow_mut());
    prepare(&mut bob.borrow_mut());
    step(400);
    let line: String = super::room::status(&bob.borrow()).into();
    assert!(line.starts_with("Waiting for 1 player to load · "), "{line}");
    assert!(line.ends_with(" s"), "{line}");
    let seconds = super::room::seconds_left(&bob.borrow()).unwrap();
    assert!((43..=45).contains(&seconds), "{seconds}");
    let rows = super::room::player_rows(&bob.borrow());
    let carol_row = rows.iter().find(|r| r.name == "carol").expect("carol's row");
    assert_eq!(carol_row.state, "Loading", "{rows:?}");
    // The prompt says so too.
    let order = bob.borrow_mut().take_stage_order().unwrap();
    let orders = Orders::default();
    let eb = env();
    let mut hb = harness();
    hb.push(
        PromptScreen::new(
            eb.clone(),
            Rc::new(RefCell::new(PromptState::default())),
            Rc::new(|_, _| {}),
        )
        .with_room(bob.clone(), order.stage, shell(&bob, &orders)),
    );
    hb.run(0.3);
    assert!(has(&hb, "Loading"), "{:?}", hb.texts());
    // Carol loads: the Ready countdown, 15 s.
    prepare(&mut carol.borrow_mut());
    step(400);
    let line: String = super::room::status(&bob.borrow()).into();
    assert!(line.starts_with("Stage 1 starts once everyone is ready · "), "{line}");
    let seconds = super::room::seconds_left(&bob.borrow()).unwrap();
    assert!((14..=15).contains(&seconds), "{seconds}");
}

/// Round D1: a room the server's operator closed says so to its members
/// ("The room was closed.", not "This room has closed."), on the status
/// line of everyone in it, and asks nothing more of them; a room that
/// ended under its host says what it said before.
#[test]
fn a_room_closed_by_the_operator_says_so_to_its_members() {
    let t = Three::new(false);
    t.step(400);
    t.server
        .borrow_mut()
        .authority(ddi_room::Command::CloseRoom {
            reason: ddi_room::Closure::ByAdministrator,
        })
        .unwrap();
    t.step(400);
    for room in [&t.alice, &t.bob, &t.carol] {
        let r = room.borrow();
        assert_eq!(r.link_state(), ddi_room::LinkState::Closed);
        assert_eq!(String::from(super::room::status(&r)), "The room was closed.");
    }
    // Under its host (the setting Close, the host leaving): as before.
    let t = Three::new(false);
    t.step(400);
    t.alice
        .borrow_mut()
        .set_host_leaves(ddi_room::HostLeaves::Close)
        .unwrap();
    t.step(400);
    t.alice.borrow_mut().leave();
    t.step(400);
    assert_eq!(String::from(super::room::status(&t.bob.borrow())), "This room has closed.");
}

/// Round D1: why a private room could not be made, each of the server's
/// operator's switches and caps in a sentence of its own (no bare code),
/// any other reason with its code.
#[test]
fn a_room_that_could_not_be_made_says_why_in_a_sentence() {
    use super::online_home::create_problem;
    let said = |code: &str| String::from(create_problem(code));
    assert_eq!(said("new_rooms_paused"), "New rooms are paused for now. Try again later.");
    assert_eq!(
        said("server_busy"),
        "The server has as many rooms as it can hold right now. Try again later."
    );
    assert_eq!(
        said("too_many_rooms"),
        "You already host as many rooms as one person can at once. Close one, or try again later."
    );
    assert_eq!(said("room_name"), "Couldn't make the room (room_name).");
}

/// PIN (simplify, round 2): the room's song select and its pop-ups (Rules,
/// Add CPUs, Players, Standings, an online host's options) draw exactly
/// what they drew before their parts were shared: a hash of each draw
/// list (its fractions rounded, `crate::testing::rounded`: a CPU's stars
/// are drawn round), recorded on the code before. The online ones again
/// in feedback round H: the player's "You" badge moved onto its name's
/// line; the Rules pop-up's (and the Add CPU pop-up's over it) for
/// Adaptive's sentence (`adaptive-v3`, and again for `adaptive-v4`'s in
/// round D1), nothing else. Every one but the follower's again in round W
/// (favorites): the picker's song select under them draws the song's
/// heart and the footer's F; a follower's draws neither.
#[test]
fn the_rooms_screens_draw_as_pinned() {
    use ddi_room::{Finish, Finisher};
    use std::hash::{DefaultHasher, Hash, Hasher};
    let hash = |h: &Harness| {
        let mut s = DefaultHasher::new();
        crate::testing::rounded(&format!("{:?}", h.drawn)).hash(&mut s);
        s.finish()
    };
    let mut got: Vec<(&str, u64)> = Vec::new();
    let e = env();
    let room: SharedRoom = Rc::new(RefCell::new(RoomClient::new(0, 1)));
    room.borrow_mut().set_rules_seen(true);
    let orders = Orders::default();
    let mut h = harness();
    h.push(room_select(&e, room.clone(), "dance-single", &orders));
    h.run(2.0);
    got.push(("select", hash(&h)));
    press(&mut h, "Room.Rules");
    h.run(2.0);
    got.push(("rules", hash(&h)));
    press(&mut h, "AddCustom");
    h.run(2.0);
    got.push(("add", hash(&h)));
    press(&mut h, "Inc.level");
    h.run(2.0);
    got.push(("add-level", hash(&h)));
    press(&mut h, "Close");
    h.run(2.0);
    press(&mut h, "Cancel");
    h.run(2.0);
    open_players(&mut h);
    h.run(2.0);
    got.push(("players", hash(&h)));
    press(&mut h, "Close");
    h.run(2.0);
    // A stage played: its standings.
    h.key("Enter", "Enter");
    h.run(0.3);
    let order = orders.borrow_mut().pop().expect("a prompt");
    room.borrow_mut().set_ready(true).unwrap();
    h.run(4.0);
    let mut ids: Vec<_> = order.cpus.iter().map(|c| c.participant).collect();
    ids.insert(1, order.me);
    let ranking = ids
        .into_iter()
        .map(|participant| {
            vec![Finisher {
                participant,
                finish: Finish::Survived,
            }]
        })
        .collect();
    room.borrow_mut()
        .report_stage(order.stage, ranking, false)
        .unwrap();
    h.run(2.0);
    got.push(("standings", hash(&h)));
    // An online room's host: its options.
    let t = Three::new(false);
    let (ea, eb, ec) = (env(), env(), env());
    let mut ha = online_select(&ea, &t.alice, songs_of(&["Alpha One"]), &orders);
    let mut hb = online_select(&eb, &t.bob, songs_of(&["Bravo One"]), &orders);
    let mut hc = online_select(&ec, &t.carol, songs_of(&["Charlie One"]), &orders);
    t.settle(400, &mut [&mut ha, &mut hb, &mut hc]);
    ha.run(2.0);
    got.push(("online-select", hash(&ha)));
    hb.run(2.0);
    got.push(("online-follower", hash(&hb)));
    open_players(&mut ha);
    ha.run(2.0);
    got.push(("online-players", hash(&ha)));
    press(&mut ha, "HostOptions");
    ha.run(2.0);
    got.push(("host", hash(&ha)));
    assert_eq!(got, PINNED);
}

/// Round AA (revision 15) moved "rules" (its new rows: Joining during a
/// match; under Adaptive, On the easiest chart) and the two pop-ups drawn
/// over it, "add" and "add-level".
const PINNED: [(&str, u64); 10] = [
    ("select", 16001433830104325910),
    ("rules", 5322785676097785853),
    ("add", 10960918902752896039),
    ("add-level", 9652554814654506447),
    ("players", 8610800191167024236),
    ("standings", 16950822547352780372),
    ("online-select", 15460780746050739119),
    ("online-follower", 17435691216428764643),
    ("online-players", 7738760287648516505),
    ("host", 1098231176107377334),
];

/// The first `make(k)` whose "You" badge is drawn (the local player's
/// own line), for [`super::test_kit::assert_badge_on_line`].
fn badged(h: &Harness, make: impl Fn(usize) -> String) -> String {
    (0..80)
        .map(make)
        .find(|id| there(h, &format!("{id}.Badge")))
        .unwrap_or_else(|| panic!("no badge drawn: {:?}", h.texts()))
}

/// The "You" badge on its name's line wherever the player's own line has
/// it (feedback round H, item 1): the players panel beside the song, the
/// Players pop-up, the stage's prompt's players and the stage's
/// Standings pop-up, each read from the frame's rects (the results'
/// Standings tab: `results_stage_tests`).
#[test]
fn the_you_badge_sits_on_its_names_line_wherever_players_are_listed() {
    use super::test_kit::assert_badge_on_line;
    use ddi_room::{Finish, Finisher};
    let env = env();
    let room = RoomClient::shared(0, 1);
    room.borrow_mut().set_rules_seen(true);
    room.borrow_mut().set_own_name(Some("alice".into()));
    let orders = Orders::default();
    let mut h = harness();
    h.push(room_select(&env, room.clone(), "dance-single", &orders));
    h.run(0.5);
    let own = badged(&h, |k| format!("Players{k}.Name"));
    assert_badge_on_line(&h, None, &own, 15.0);
    open_players(&mut h);
    assert_eq!(top(&h), "room-players");
    let own = badged(&h, |k| format!("Panel{k}.Name"));
    assert_badge_on_line(&h, None, &own, 15.0);
    h.key("Escape", "Escape");
    h.run(0.5);
    // The stage's prompt.
    h.key("Enter", "Enter");
    h.run(0.3);
    let order = orders.borrow_mut().pop().expect("a prompt");
    let mut hp = harness();
    hp.push(
        PromptScreen::new(env.clone(), Rc::new(RefCell::new(PromptState::default())), Rc::new(|_, _| {}))
            .with_room(room.clone(), order.stage, shell(&room, &Orders::default())),
    );
    hp.run(0.5);
    let own = badged(&hp, |k| format!("Room.Player{k}.Name"));
    assert_badge_on_line(&hp, None, &own, 15.0);
    // The stage's Standings, popped up on song select.
    // (Ready already: the prompt's countdown runs.)
    let _ = room.borrow_mut().set_ready(true);
    let now = room.borrow().now_ms();
    room.borrow_mut().tick(now + 10_000);
    let mut ids: Vec<_> = order.cpus.iter().map(|c| c.participant).collect();
    ids.insert(0, order.me);
    let ranking = ids
        .into_iter()
        .map(|participant| {
            vec![Finisher {
                participant,
                finish: Finish::Survived,
            }]
        })
        .collect();
    room.borrow_mut().report_stage(order.stage, ranking, false).unwrap();
    h.run(0.5);
    assert_eq!(top(&h), "room-standings");
    let own = badged(&h, |k| format!("Name{k}"));
    assert_badge_on_line(&h, None, &own, 14.0);
}

/// A notice the room's status line says (More's Copy audience link) lives
/// its few seconds once (feedback round H, item 2): gone from the room's
/// song select after [`crate::state::online::link::NOTICE_MS`]; a stage's
/// prompt opens with none from before it, whether it is still showing
/// when the prompt opens or has faded, the prompt a page of its own (the
/// web: a `Ui` a page) or on the same stack (desktop); and one raised
/// while the prompt is open is not on the room's song select after the
/// stage.
#[test]
fn a_notice_lives_once_and_never_crosses_a_stages_prompt() {
    use crate::state::online::link::{LinkNotice, NOTICE_MS};
    use crate::state::online::tests::room::{client, person, prepare, song, tick, upload};
    use ddi_room::fake::FakeServer;
    use ddi_room::{Identity, Intent, ManualClock, RoomPolicy, Run};
    const COPIED: &str = "Audience link copied: anyone with it can watch.";
    for desktop in [false, true] {
        let clock = ManualClock::new(1_000);
        let server = FakeServer::hosted(
            "Room000001",
            Identity::new("dev:alice"),
            "alice",
            RoomPolicy::default(),
            clock.clone(),
        )
        .shared();
        server
            .borrow_mut()
            .join(Identity::new("dev:bob"), "bob", Intent::Play)
            .unwrap();
        let alice = Rc::new(RefCell::new(client(&server, person(&server, "alice"), "alice", 1_000)));
        let bob = Rc::new(RefCell::new(client(&server, person(&server, "bob"), "bob", 1_000)));
        let step = |ms: u64| tick(&clock, &mut [&mut alice.borrow_mut(), &mut bob.borrow_mut()], ms);
        step(400);
        step(400);
        let mut rules = alice.borrow().rules().clone();
        rules.stages = 2;
        alice.borrow_mut().save_rules(rules).unwrap();
        alice.borrow_mut().set_rules_seen(true);
        step(400);
        let e = env();
        let orders = Orders::default();
        let mut data = library("dance-single");
        data.songs.insert(0, song());
        let mut h = harness();
        h.push(sample::room_select(&e, alice.clone(), data).with_room(alice.clone(), shell(&alice, &orders)));
        h.run(0.3);
        let shows = |h: &Harness| h.texts().iter().any(|t| t.contains(COPIED));
        let status = |r: &SharedRoom| String::from(super::room::status_line(&r.borrow()));
        // Raised: said, then gone after its few seconds.
        alice.borrow_mut().set_link_notice(Some(LinkNotice::AudienceInviteCopied));
        h.run(0.3);
        assert!(shows(&h), "{:?}", h.texts());
        step(400);
        h.run(0.1);
        assert!(shows(&h), "still within its time");
        step(NOTICE_MS);
        h.run(0.3);
        assert!(!shows(&h), "{:?}", h.texts());
        // Faded: the stage's prompt does not bring it back. Raised again
        // just before the pick: the prompt opens without it.
        for (k, fresh) in [(0usize, false), (1, true)] {
            if fresh {
                alice.borrow_mut().set_link_notice(Some(LinkNotice::AudienceInviteCopied));
                step(100);
                h.run(0.1);
                assert!(shows(&h), "{:?}", h.texts());
            }
            let s = song();
            alice.borrow_mut().pick(&s, &s.charts[0]).unwrap();
            step(400);
            upload(&server, &mut alice.borrow_mut(), &format!("abc1234{k}"));
            step(400);
            prepare(&mut alice.borrow_mut());
            prepare(&mut bob.borrow_mut());
            step(400);
            let order = alice.borrow_mut().take_stage_order().expect("the prompt's stage");
            assert!(!status(&alice).contains(COPIED), "{}", status(&alice));
            let prompt = PromptScreen::new(e.clone(), Rc::new(RefCell::new(PromptState::default())), Rc::new(|_, _| {}))
                .with_room(alice.clone(), order.stage, shell(&alice, &orders));
            // The web: the prompt a page (a `Ui`) of its own; desktop:
            // pushed over the room's song select on one stack.
            let mut page = harness();
            if desktop {
                page.push(
                    sample::room_select(&e, alice.clone(), library("dance-single"))
                        .with_room(alice.clone(), shell(&alice, &orders)),
                );
                page.run(0.3);
            }
            page.push(prompt);
            let on = &mut page;
            on.run(0.5);
            assert_eq!(top(on), "play-prompt");
            assert!(!shows(on), "{k}: {:?}", on.texts());
            // A notice raised on the prompt: said there, not after the
            // stage.
            alice.borrow_mut().set_link_notice(Some(LinkNotice::InviteCopied));
            on.run(0.2);
            assert!(status(&alice).starts_with("Invite link copied"), "{}", status(&alice));
            alice.borrow_mut().set_ready(true).unwrap();
            bob.borrow_mut().set_ready(true).unwrap();
            step(400);
            step(4_000);
            for r in [&alice, &bob] {
                let stage = order.stage;
                r.borrow_mut()
                    .report_run(stage, Run::Live { life: 0.8, score: 0, steps: 0 }, None)
                    .unwrap();
            }
            step(400);
            step(400);
            drop(page);
            h.run(0.5);
            assert!(alice.borrow().link_notice().is_none(), "{k}");
            assert!(!h.texts().iter().any(|t| t.starts_with("Invite link copied")), "{:?}", h.texts());
            // Its Standings (popped up on song select) closed.
            if top(&h) == "room-standings" {
                h.key("Escape", "Escape");
                h.run(0.5);
            }
        }
    }
}

/// ui-screens-1 (fix round G1): the co-host's Rules pop-up held open
/// while the owner applies their own change: the co-host's Apply does
/// not put back what the pop-up opened with. It says the rules were
/// changed meanwhile and shows them as they are now; Apply again saves
/// the co-host's edit over them.
#[test]
fn an_apply_over_rules_changed_meanwhile_says_so_and_shows_them() {
    let t = Three::new(true);
    let (ea, eb) = (env(), env());
    let orders = Orders::default();
    let mut ha = online_select(&ea, &t.alice, songs_of(&["Alpha One"]), &orders);
    let mut hb = online_select(&eb, &t.bob, songs_of(&["Bravo One"]), &orders);
    t.settle(400, &mut [&mut ha, &mut hb]);
    let boos = t.server.borrow().state().next.rules.boos;
    let difficulty = t.server.borrow().state().next.rules.difficulty;
    // The co-host opens the Rules; the owner changes the timing.
    press(&mut hb, "Room.Rules");
    press(&mut ha, "Room.Rules");
    press(&mut ha, "Inc.difficulty");
    press(&mut ha, "Apply");
    t.settle(400, &mut [&mut ha, &mut hb]);
    let owners = t.server.borrow().state().next.rules.difficulty;
    assert_ne!(owners, difficulty);
    // The co-host's Boos, applied over the pop-up they opened before.
    press(&mut hb, "Inc.boos");
    press(&mut hb, "Apply");
    t.settle(400, &mut [&mut ha, &mut hb]);
    let next = t.server.borrow().state().next.rules.clone();
    assert_eq!(next.difficulty, owners, "the owner's change stands");
    assert_eq!(next.boos, boos, "nothing applied over it unseen");
    assert_eq!(top(&hb), "room-rules", "the pop-up stays open");
    let said: String = ddi_l10n::msg::room::refused_rules_changed().into();
    let first = hb.texts().into_iter().find(|t| said.starts_with(t.as_str()) && t.len() > 20);
    assert!(first.is_some(), "{:?}", hb.texts());
    // As the rules are now; Apply again saves the co-host's edit.
    press(&mut hb, "Inc.boos");
    press(&mut hb, "Apply");
    t.settle(400, &mut [&mut ha, &mut hb]);
    let next = t.server.borrow().state().next.rules.clone();
    assert_eq!((next.difficulty, next.boos), (owners, !boos));
}

/// ui-screens-4 (fix round G1): the rooms' counts are of who can play.
/// With one of three watching, the players panel says two of the cap and
/// the stage's Ready card counts the two who play it, so that it
/// completes once both are ready.
#[test]
fn the_counts_are_of_who_can_play() {
    use crate::state::online::tests::room::{prepare, song, upload};
    let t = Three::new(false);
    let (ea, eb) = (env(), env());
    let orders = Orders::default();
    let mut ha = online_select(&ea, &t.alice, songs_of(&["Alpha One"]), &orders);
    t.carol.borrow_mut().set_intent(ddi_room::Intent::Watch).unwrap();
    t.settle(400, &mut [&mut ha]);
    assert_eq!(said(&ha, "Players.Count"), "2 / 73", "the watcher not counted");
    let s = song();
    t.alice.borrow_mut().pick(&s, &s.charts[0]).unwrap();
    t.step(400);
    upload(&t.server, &mut t.alice.borrow_mut(), "abc12345");
    t.step(400);
    for r in [&t.alice, &t.bob, &t.carol] {
        prepare(&mut r.borrow_mut());
    }
    t.step(400);
    let order = t.alice.borrow_mut().take_stage_order().unwrap();
    t.alice.borrow_mut().set_ready(true).unwrap();
    t.step(400);
    let state = Rc::new(RefCell::new(PromptState::default()));
    let mut hp = harness();
    hp.push(
        PromptScreen::new(eb, state, Rc::new(|_, _| {})).with_room(
            t.alice.clone(),
            order.stage,
            shell(&t.alice, &Orders::default()),
        ),
    );
    hp.run(0.5);
    assert_eq!(said(&hp, "Room.Ready"), "1 / 2", "{:?}", hp.texts());
}

/// ui-screens-5, ui-screens-9 and the life line (fix round G4): a stage's
/// prompt names the room's CPUs beside the other people, says what
/// running out of life does under the match's Charts rule (out, or an
/// easier chart under Adaptive) and no longer that the others play on
/// their own devices, and names the Charts rule only where the room
/// offers Adaptive. Before: "against 2 other players, each on their own
/// device · out when your life runs out" for all three, with "Charts as
/// picked" where the room offers no Adaptive.
#[test]
fn a_stage_prompt_says_whom_it_is_against_and_what_zero_life_does() {
    use super::prompt::{PromptInfo, StageLine};
    let adaptive = crate::state::room::StageRules {
        charts: ddi_room::Charts::Adaptive,
        easiest: ddi_room::AtEasiest::Out,
        ..Default::default()
    };
    let as_picked = crate::state::room::StageRules::default();
    let said = |cpus, others, rules, attacks, adaptive_offered| {
        let mut info = PromptInfo::default();
        info.set_stage(StageLine {
            number: 1,
            stages: 3,
            cpus,
            others,
            rules,
            attacks,
            adaptive_offered,
        });
        info.mods
    };
    assert_eq!(
        said(4, 2, &adaptive, None, true),
        "Stage 1 of 3 · against 2 other players and 4 CPUs · \
         the chart changes with your skill · Rules: DDI"
    );
    assert_eq!(
        said(4, 0, &adaptive, Some(true), true),
        "Stage 1 of 3 · against 4 CPUs · the chart changes with your skill · Rules: DDI"
    );
    // Under Adaptive nothing is said of life running out, nor of how the
    // chart changes (the maintainer, 2026-11-15).
    for word in ["life", "easier", "harder", "bar"] {
        assert!(!said(4, 0, &adaptive, Some(true), true).contains(word), "{word}");
    }
    assert_eq!(
        said(1, 0, &as_picked, Some(true), true),
        "Stage 1 of 3 · against one CPU · out when your life runs out · \
         Rules: DDI · Charts as picked"
    );
    assert_eq!(
        said(0, 2, &as_picked, None, false),
        "Stage 1 of 3 · against 2 other players · out when your life runs out · Rules: DDI"
    );
}

/// ui-screens-8: the standings dismissed with Back (Escape) are seen as
/// with Continue: a room's song select built afterwards does not pop them
/// up again.
#[test]
fn standings_closed_with_back_are_seen() {
    use ddi_room::{Finish, Finisher};
    let env = env();
    let room = RoomClient::shared(0, 1);
    room.borrow_mut().set_rules_seen(true);
    let orders = Orders::default();
    let mut h = harness();
    h.push(room_select(&env, room.clone(), "dance-single", &orders));
    h.run(0.5);
    h.key("Enter", "Enter");
    h.run(0.3);
    let order = orders.borrow_mut().pop().expect("a prompt");
    room.borrow_mut().set_ready(true).unwrap();
    room.borrow_mut().tick(4_000);
    let mut ids: Vec<_> = order.cpus.iter().map(|c| c.participant).collect();
    ids.insert(0, order.me);
    let ranking = ids
        .into_iter()
        .map(|participant| {
            vec![Finisher {
                participant,
                finish: Finish::Survived,
            }]
        })
        .collect();
    room.borrow_mut()
        .report_stage(order.stage, ranking, false)
        .unwrap();
    h.run(0.5);
    assert_eq!(top(&h), "room-standings");
    h.key("Escape", "Escape");
    h.run(0.5);
    assert_eq!(top(&h), "select");
    assert_eq!(room.borrow().unseen_results(), None, "seen");
    // A new song select of the room (the web builds one per visit).
    let mut again = harness();
    again.push(room_select(&env, room.clone(), "dance-single", &orders));
    again.run(0.5);
    assert_eq!(top(&again), "select", "not popped up again");
}

/// ui-screens-7: a leave question left open when the room hands the
/// player a stage's prompt is closed as the prompt opens (its cost was
/// for song select; under the prompt it would be buried, and a window's
/// close would find it and quit without the prompt's own question).
#[test]
fn a_leave_question_open_when_a_stage_opens_is_closed() {
    use crate::state::online::tests::room::upload;
    let t = Three::new(false);
    let eb = env();
    let (orders, calls) = (Orders::default(), Calls::default());
    let mut hb = harness();
    hb.push(
        sample::room_select(&eb, t.bob.clone(), songs_of(&["Bravo One"]))
            .with_room(t.bob.clone(), leaving_shell(&t.bob, &orders, &calls)),
    );
    t.settle(400, &mut [&mut hb]);
    let song = songs_of(&["Alpha One"]).songs.remove(0);
    t.alice.borrow_mut().pick(&song, &song.charts[0]).unwrap();
    t.settle(400, &mut [&mut hb]);
    hb.key("Escape", "Escape");
    hb.run(0.3);
    assert!(asked(&hb), "a player in the match is asked");
    upload(&t.server, &mut t.alice.borrow_mut(), "abc12345");
    for _ in 0..4 {
        t.settle(400, &mut [&mut hb]);
    }
    assert_eq!(orders.borrow().len(), 1, "the stage's prompt handed over");
    let open = hb
        .ui
        .layers()
        .iter()
        .filter(|l| l.name == crate::LEAVE_QUESTION)
        .count();
    assert_eq!(open, 0, "the question went with it");
    assert!(!t.bob.borrow().asking_leave().get());
    assert!(calls.borrow().is_empty(), "nobody left");
}

/// Fix round G4: the field's band is not drawn behind a room's Ready card
/// (through its translucent plate it showed as a dark stripe); the
/// backdrop is the song's picture alone until the play starts. A single
/// play's prompt keeps it.
#[test]
fn a_rooms_ready_card_has_no_field_band_behind_it() {
    use super::prompt::PromptPicture;
    let picture = PromptPicture {
        source: "play:picture".into(),
        aspect: 4.0 / 3.0,
        frame: Some(4.0 / 3.0),
        brightness: 0.75,
        filter: Some(([0.5, 0.3], 0.4)),
    };
    let room = RoomClient::shared(0, 1);
    let s = &library("dance-single").songs[0];
    room.borrow_mut().pick(s, &s.charts[0]).unwrap();
    let order = room.borrow_mut().take_stage_order().unwrap();
    let state = Rc::new(RefCell::new(PromptState::default()));
    state.borrow_mut().set_picture(Some(picture.clone()));
    let mut h = harness();
    h.push(
        PromptScreen::new(env(), state.clone(), Rc::new(|_, _| {})).with_room(
            room.clone(),
            order.stage,
            shell(&room, &Orders::default()),
        ),
    );
    h.run(1.0);
    assert!(there(&h, "Picture"), "the song's picture");
    assert!(!there(&h, "Filter"), "no band behind the Ready card");
    // A single play's prompt: the band, as the play will have it.
    let single = Rc::new(RefCell::new(PromptState::default()));
    single.borrow_mut().set_picture(Some(picture));
    let mut h = harness();
    h.push(PromptScreen::new(env(), single, Rc::new(|_, _| {})));
    h.run(1.0);
    assert!(there(&h, "Filter"));
}

/// The room's screens tick it on the wall clock the shell gives
/// (`Ui::set_wall_ms`), never on the live art's (review3 desktop-1): with
/// the art's clock a minute behind (the host clock stood still while the
/// computer slept), the room still moves on with the wall clock.
#[test]
fn the_rooms_screens_tick_it_on_the_wall_clock() {
    let env = env();
    let wall = 1_800_000_000_000u64;
    let room = RoomClient::shared(wall, 1);
    room.borrow_mut().set_rules_seen(true);
    let orders = Orders::default();
    let mut h = harness();
    h.push(room_select(&env, room.clone(), "dance-single", &orders));
    for frame in 0..30u64 {
        h.ui.set_clock((wall - 60_000) as f64 / 1000.0 + frame as f64 / 60.0);
        h.ui.set_wall_ms(wall + frame * 100);
        h.run(1.0 / 60.0);
    }
    assert_eq!(room.borrow().now_ms(), wall + 29 * 100);
}

/// Bob, a member of the online room, on its song select with Options
/// opened over it; alice picks, and the stage's song is shared: the
/// stage is due for bob.
fn bob_on_options(t: &Three, eb: &Env, orders: &Orders) -> Harness {
    use crate::state::online::tests::room::upload;
    let mut hb = harness();
    hb.push(
        sample::room_select(eb, t.bob.clone(), songs_of(&["Bravo One"]))
            .with_room(t.bob.clone(), shell(&t.bob, orders)),
    );
    t.settle(400, &mut [&mut hb]);
    hb.push(sample::options_screen(eb, None));
    hb.run(0.5);
    assert_eq!(top(&hb), "options");
    let song = songs_of(&["Alpha One"]).songs.remove(0);
    t.alice.borrow_mut().pick(&song, &song.charts[0]).unwrap();
    t.settle(400, &mut [&mut hb]);
    upload(&t.server, &mut t.alice.borrow_mut(), "abc12345");
    for _ in 0..4 {
        t.settle(400, &mut [&mut hb]);
    }
    hb
}

fn notices(h: &Harness) -> usize {
    h.ui
        .layers()
        .iter()
        .filter(|l| l.name == super::room::DUE_NOTICE)
        .count()
}

/// Decisions round D2, item 1 (the maintainer, 2026-11-15: "Tell them"):
/// a member on Options when their room's stage is due is told so over
/// Options, with the room's countdown and a button that goes to the
/// stage's Ready screen; Options is never closed for them. Desktop's
/// path: the room's screen under Options on the UI's stack.
#[test]
fn a_member_on_options_is_told_their_stage_is_due_and_goes_to_ready() {
    let t = Three::new(false);
    let eb = env();
    let orders = Orders::default();
    let mut hb = bob_on_options(&t, &eb, &orders);
    assert!(orders.borrow().is_empty(), "no prompt opens under Options");
    assert_eq!(top(&hb), super::room::DUE_NOTICE, "{:?}", hb.ui.layers());
    assert_eq!(notices(&hb), 1, "told once");
    assert_eq!(view(&eb).due_open, Some((1, false)));
    assert!(has(&hb, "Stage 1 is starting"), "{:?}", hb.texts());
    assert!(
        has(&hb, "Your room's next stage is starting. Go to Ready to play it."),
        "{:?}",
        hb.texts()
    );
    // The room's status line under it, counting.
    let counted = |h: &Harness| {
        h.texts()
            .iter()
            .find(|t| t.contains(" · ") && t.ends_with(" s"))
            .cloned()
            .unwrap_or_else(|| panic!("no countdown in {:?}", h.texts()))
    };
    let before = counted(&hb);
    for _ in 0..3 {
        t.settle(800, &mut [&mut hb]);
    }
    assert_ne!(counted(&hb), before, "the seconds count down");
    assert_eq!(notices(&hb), 1);
    assert!(hb.ui.layers().iter().any(|l| l.name == "options"), "Options stays");
    // Go to Ready is chosen first: Options leaves by its own Back, and
    // the prompt opens once the room's screen shows again.
    assert_eq!(hb.ui.focused(), Some("Go"));
    hb.key("Enter", "Enter");
    for _ in 0..4 {
        t.settle(200, &mut [&mut hb]);
    }
    assert_eq!(notices(&hb), 0);
    assert!(!hb.ui.layers().iter().any(|l| l.name == "options"), "{:?}", hb.ui.layers());
    assert_eq!(orders.borrow().len(), 1, "the stage's prompt handed over");
    assert_eq!(view(&eb).due_open, None);
    assert_eq!(eb.state.borrow().over, 0, "Options left as its Back leaves it");
}

/// Decisions round D4, item C (the maintainer, 2026-11-15: "during a
/// key bind it waits until the bind finishes or is canceled"): a member
/// binding a control in Options when their stage comes due is not told
/// over the binding (the notice would take the press it waits for, and
/// its Go would cancel what was bound); once the binding is canceled (or
/// saved: the dialog gone either way) the notice opens. Desktop's path
/// (the room's screen under Options) and what the web's asks first
/// (`Ui::notices_wait`).
#[test]
fn the_due_notice_waits_for_a_control_being_bound() {
    use super::options_dialogs::{BindDialog, KeyDialog};
    use crate::state::bind::BindFlow;
    use ddi_platform::DeviceId;
    for one_key in [false, true] {
        let t = Three::new(false);
        let eb = env();
        let orders = Orders::default();
        let mut hb = harness();
        hb.push(
            sample::room_select(&eb, t.bob.clone(), songs_of(&["Bravo One"]))
                .with_room(t.bob.clone(), shell(&t.bob, &orders)),
        );
        t.settle(400, &mut [&mut hb]);
        hb.push(sample::options_screen(&eb, None));
        hb.run(0.5);
        assert!(!hb.ui.notices_wait());
        // The guided binding of the keyboard, or one more key for a lane.
        let opened = if one_key {
            hb.ui.push_dialog(KeyDialog::new(eb.clone(), "Left", Rc::new(|_, _| {})))
        } else {
            let layout = ddi_chart::Layout::builtin("dance-single").unwrap();
            let flow = BindFlow::new(
                Some(DeviceId::Keyboard),
                &layout,
                Default::default(),
                Vec::new(),
                Default::default(),
            );
            hb.ui
                .push_dialog(BindDialog::new(eb.clone(), flow, "Single", Rc::new(|_, _| {})))
        };
        assert!(opened);
        hb.run(0.5);
        let binding = if one_key { "key" } else { "bind" };
        assert_eq!(hb.ui.layers().last().unwrap().name, binding);
        assert!(hb.ui.notices_wait());
        // The stage comes due meanwhile.
        let song = songs_of(&["Alpha One"]).songs.remove(0);
        t.alice.borrow_mut().pick(&song, &song.charts[0]).unwrap();
        t.settle(400, &mut [&mut hb]);
        crate::state::online::tests::room::upload(&t.server, &mut t.alice.borrow_mut(), "abc12345");
        for _ in 0..6 {
            t.settle(400, &mut [&mut hb]);
        }
        assert!(t.bob.borrow().stage_due().is_some(), "{:?}", t.bob.borrow().state().phase);
        assert_eq!(notices(&hb), 0, "{one_key}: not over the binding: {:?}", hb.ui.layers());
        assert_eq!(hb.ui.layers().last().unwrap().name, binding);
        assert!(t.bob.borrow().telling_due().get().is_none());
        assert!(orders.borrow().is_empty());
        // The binding canceled: told at once.
        hb.tap(PointerKind::Mouse, "Cancel");
        t.settle(300, &mut [&mut hb]);
        assert!(!hb.ui.notices_wait());
        assert_eq!(notices(&hb), 1, "{one_key}: {:?}", hb.ui.layers());
        assert_eq!(hb.ui.layers().last().unwrap().name, super::room::DUE_NOTICE);
        assert!(has(&hb, "Stage 1 is starting"), "{:?}", hb.texts());
    }
}

/// Decisions round D4, item C ("Tell them the same way"): a member still
/// on a stage's results when the room's next stage comes due is told
/// over the results, as over Options; the notice's button goes back to
/// the room as the results' own does, and the stage's prompt opens
/// there. On one stack over the room's song select (desktop) and as a
/// page of its own, the room run beside it (the web); Stay keeps the
/// results and is not asked again.
#[test]
fn a_member_on_a_stages_results_is_told_the_next_stage_is_due() {
    use super::data::ResultsKind;
    use super::results::{ResultsActions, ResultsScreen};
    use crate::state::online::tests::room::{prepare, upload};
    use ddi_room::Run;
    for (desktop, stay) in [(true, false), (false, false), (true, true)] {
        let t = Three::new(false);
        let eb = env();
        let orders = Orders::default();
        let everyone = [&t.alice, &t.bob, &t.carol];
        t.step(400);
        t.step(400);
        let mut rules = t.alice.borrow().rules().clone();
        rules.stages = 2;
        t.alice.borrow_mut().save_rules(rules).unwrap();
        t.step(400);
        // Stage 1, played by everyone.
        let song = songs_of(&["Alpha One"]).songs.remove(0);
        t.alice.borrow_mut().pick(&song, &song.charts[0]).unwrap();
        t.step(400);
        upload(&t.server, &mut t.alice.borrow_mut(), "abc12340");
        t.step(400);
        for r in everyone {
            prepare(&mut r.borrow_mut());
        }
        t.step(400);
        let first = t.bob.borrow_mut().take_stage_order().expect("stage 1's prompt");
        for r in everyone {
            let _ = r.borrow_mut().take_stage_order();
            r.borrow_mut().set_ready(true).unwrap();
        }
        t.step(400);
        t.step(4_000);
        for r in everyone {
            r.borrow_mut()
                .report_run(first.stage, Run::Live { life: 0.8, score: 0, steps: 0 }, None)
                .unwrap();
        }
        t.step(400);
        t.step(400);
        // Bob stays on its results.
        let went = Rc::new(std::cell::Cell::new(0));
        let mut hb = harness();
        if desktop {
            hb.push(
                sample::room_select(&eb, t.bob.clone(), songs_of(&["Bravo One"]))
                    .with_room(t.bob.clone(), shell(&t.bob, &orders)),
            );
            hb.run(0.3);
        }
        let mut data = sample::results();
        data.kind = ResultsKind::Stage;
        let actions = if desktop {
            ResultsActions::default()
        } else {
            // The web's button navigates to the room's song select.
            let went = went.clone();
            ResultsActions {
                song_list: Some(Rc::new(move |_, _| went.set(went.get() + 1))),
                ..Default::default()
            }
        };
        hb.push(ResultsScreen::new(eb.clone(), data, actions).with_room(t.bob.clone(), first.stage));
        t.settle(400, &mut [&mut hb]);
        assert_eq!(top(&hb), "results");
        assert_eq!(notices(&hb), 0);
        // Stage 2 picked while he is there.
        let song = songs_of(&["Alpha One", "Alpha Two"]).songs.remove(1);
        t.alice.borrow_mut().pick(&song, &song.charts[0]).unwrap();
        t.settle(400, &mut [&mut hb]);
        upload(&t.server, &mut t.alice.borrow_mut(), "abc12341");
        for _ in 0..6 {
            t.step(400);
            if !desktop {
                // (The web's timer under the page prepares the song.)
                prepare(&mut t.bob.borrow_mut());
            }
            t.settle(200, &mut [&mut hb]);
        }
        assert!(t.bob.borrow().stage_due().is_some(), "{:?}", t.bob.borrow().state().phase);
        assert_eq!(notices(&hb), 1, "{desktop}: told once: {:?}", hb.ui.layers());
        assert_eq!(hb.ui.layers().last().unwrap().name, super::room::DUE_NOTICE);
        assert!(has(&hb, "Stage 2 is starting"), "{:?}", hb.texts());
        assert!(orders.borrow().is_empty(), "no prompt under the results");
        if stay {
            hb.key("Escape", "Escape");
            for _ in 0..4 {
                t.settle(300, &mut [&mut hb]);
            }
            assert_eq!(notices(&hb), 0, "not told again");
            assert_eq!(top(&hb), "results");
            assert!(orders.borrow().is_empty());
            continue;
        }
        assert_eq!(hb.ui.focused(), Some("Go"));
        hb.key("Enter", "Enter");
        for _ in 0..4 {
            t.settle(200, &mut [&mut hb]);
        }
        assert_eq!(notices(&hb), 0);
        if desktop {
            // (Stage 1's standings, where the results' Graphs tab was
            // the one shown: closed as the player closes them.)
            if hb.ui.layers().last().unwrap().name == "room-standings" {
                hb.key("Escape", "Escape");
                for _ in 0..4 {
                    t.settle(200, &mut [&mut hb]);
                }
            }
            assert_eq!(top(&hb), "select", "{:?}", hb.ui.layers());
            assert_eq!(orders.borrow().len(), 1, "stage 2's prompt handed over");
        } else {
            assert_eq!(went.get(), 1, "the results' own way back to the room");
        }
    }
}

/// Stay here keeps Options and is not asked again for the stage; its
/// prompt still opens by itself once the player is back on the room.
#[test]
fn staying_on_options_is_told_once_and_the_prompt_opens_on_the_way_back() {
    let t = Three::new(false);
    let eb = env();
    let orders = Orders::default();
    let mut hb = bob_on_options(&t, &eb, &orders);
    assert_eq!(top(&hb), super::room::DUE_NOTICE);
    // Escape is Stay.
    hb.key("Escape", "Escape");
    for _ in 0..4 {
        t.settle(300, &mut [&mut hb]);
    }
    assert_eq!(notices(&hb), 0, "not told again");
    assert_eq!(top(&hb), "options");
    assert!(orders.borrow().is_empty());
    assert!(t.bob.borrow().stage_due().is_none());
    hb.key("Escape", "Escape");
    for _ in 0..4 {
        t.settle(300, &mut [&mut hb]);
    }
    assert_eq!(orders.borrow().len(), 1, "back on the room: its prompt");
}

/// The notice goes by itself when the stage is no longer theirs to go
/// to (the pick backed out of); Options stays.
#[test]
fn the_due_notice_goes_when_the_stage_is_no_longer_due() {
    let t = Three::new(false);
    let eb = env();
    let orders = Orders::default();
    let mut hb = bob_on_options(&t, &eb, &orders);
    assert_eq!(top(&hb), super::room::DUE_NOTICE);
    let stage = t.alice.borrow().state().upcoming_stage().unwrap();
    let _ = t.alice.borrow_mut().take_stage_order();
    let _ = t.alice.borrow_mut().leave_prompt(stage);
    for _ in 0..4 {
        t.settle(300, &mut [&mut hb]);
    }
    assert!(t.bob.borrow().stage_due().is_none(), "{:?}", t.bob.borrow().state().phase);
    assert_eq!(notices(&hb), 0);
    assert_eq!(top(&hb), "options");
    assert!(!t.bob.borrow().telling_due().get().is_some());
}

/// The web's path: Options is a page of its own (no room screen under
/// it); the shell pushes the notice over it from the kept room, and its
/// button asks the shell to go (the web navigates to the room's route).
/// A stage the player only watches says so.
#[test]
fn a_shell_pushes_the_due_notice_over_a_page_of_its_own() {
    use super::room::DueDialog;
    let t = Three::new(false);
    let eb = env();
    let orders = Orders::default();
    // Bob's room runs without a screen (the web's timer under the page).
    let mut hb = harness();
    hb.push(sample::options_screen(&eb, None));
    hb.run(0.5);
    let went = Rc::new(std::cell::Cell::new(0));
    let go = |went: &Rc<std::cell::Cell<u32>>| -> super::room::DueGo {
        let went = went.clone();
        Rc::new(move |_| went.set(went.get() + 1))
    };
    t.step(400);
    t.step(200);
    assert!(DueDialog::due(&eb, &t.bob, go(&went)).is_none(), "nothing due yet");
    let song = songs_of(&["Alpha One"]).songs.remove(0);
    t.alice.borrow_mut().pick(&song, &song.charts[0]).unwrap();
    t.step(400);
    t.step(200);
    crate::state::online::tests::room::upload(&t.server, &mut t.alice.borrow_mut(), "abc12345");
    for _ in 0..4 {
        t.step(400);
        crate::state::online::tests::room::prepare(&mut t.bob.borrow_mut());
        t.step(200);
    }
    assert!(
        t.bob.borrow().stage_due().is_some(),
        "{:?} {:?}",
        t.bob.borrow().state().phase,
        t.bob.borrow().view()
    );
    assert!(orders.borrow().is_empty());
    let d = DueDialog::due(&eb, &t.bob, go(&went)).expect("due");
    assert!(DueDialog::due(&eb, &t.bob, go(&went)).is_none(), "told once while it is open");
    assert!(hb.ui.push_dialog(d));
    hb.run(0.5);
    assert_eq!(top(&hb), super::room::DUE_NOTICE);
    assert!(has(&hb, "Go to Ready") && has(&hb, "Stay here"), "{:?}", hb.texts());
    hb.key("Enter", "Enter");
    hb.run(0.5);
    assert_eq!(went.get(), 1);
    assert_eq!(top(&hb), "options", "the shell goes; the notice only asks");
    assert!(t.bob.borrow().telling_due().get().is_none());
    assert!(t.bob.borrow().stage_due().is_some(), "still due until the prompt takes it");

    // A stage to watch: CPU practice with the player watching.
    let room = RoomClient::shared(0, 1);
    room.borrow_mut().set_rules_seen(true);
    room.borrow_mut().set_intent(ddi_room::Intent::Watch).unwrap();
    let data = library("dance-single");
    room.borrow_mut().pick(&data.songs[0], &data.songs[0].charts[0]).unwrap();
    let jobs = room.borrow_mut().take_load_jobs();
    for job in jobs {
        room.borrow_mut().finish_load(&job, Ok(()));
    }
    let due = room.borrow().stage_due().expect("a watcher's stage");
    assert!(due.watching);
    let ew = env();
    let mut hw = harness();
    hw.push(sample::options_screen(&ew, None));
    hw.run(0.5);
    assert!(hw.ui.push_dialog(DueDialog::due(&ew, &room, go(&went)).unwrap()));
    hw.run(0.5);
    assert!(
        has(&hw, "A stage you are not playing is starting. You can watch it."),
        "{:?}",
        hw.texts()
    );
    assert!(has(&hw, "Watch"), "{:?}", hw.texts());
    assert_eq!(view(&ew).due_open, Some((1, true)));
}

/// Feedback round P2: the line that tells the player why they sit a
/// stage out says what comes next too: whether this device shows them
/// the stage (practice, or a room that plays live) and whether they play
/// the next one (there is one, the match brings everyone back each stage,
/// they had not chosen Watch).
#[test]
fn sitting_out_says_what_comes_next() {
    use super::room::{SatOutThen, sat_out_line};
    use ddi_room::SitOutReason::{Left, LoadFailed, NotConnected, NotReady, StillLoading, Watching};
    let line = |reason, then| String::from(sat_out_line(2, reason, then));
    let next = SatOutThen::Next { watching: true };
    for (reason, why) in [
        (NotReady, "You weren't ready when stage 2 started."),
        (StillLoading, "Stage 2's song was still loading here when it started."),
        (LoadFailed, "Stage 2's song couldn't be loaded here."),
        (NotConnected, "Your connection was lost when stage 2 started."),
        (Left, "You're sitting out stage 2; the others are playing it."),
    ] {
        assert_eq!(line(reason, next), format!("{why} You're watching this stage and play the next one."));
    }
    for (then, says) in [
        (SatOutThen::Next { watching: false }, "You play the next one."),
        (SatOutThen::Last { watching: true }, "You're watching this stage, the match's last."),
        (SatOutThen::Last { watching: false }, "It's the match's last stage."),
        (SatOutThen::Out { watching: true }, "You're watching this stage; missing it ends the match for you."),
        (SatOutThen::Out { watching: false }, "Missing it ends the match for you."),
        (SatOutThen::Chosen { watching: true }, "You're watching this stage; choose Play to play the next one."),
        (SatOutThen::Chosen { watching: false }, "Choose Play to play the next one."),
    ] {
        assert_eq!(line(NotReady, then), format!("You weren't ready when stage 2 started. {says}"));
    }
    // Decided from the room: practice shows the stage on this device.
    let of = |stages: u16, back: bool, reason| {
        let room = RoomClient::shared(0, 1);
        let mut rules = room.borrow().rules().clone();
        rules.stages = stages;
        rules.return_each_stage = back;
        // Persistent elimination never combines with joining during a
        // match (revision 15's default On).
        rules.mid_match_entry = back;
        room.borrow_mut().save_rules(rules).unwrap();
        let s = &library("dance-single").songs[0];
        room.borrow_mut().pick(s, &s.charts[0]).unwrap();
        let stage = room.borrow().stages()[0].id;
        SatOutThen::of(&room.borrow(), stage, reason)
    };
    assert_eq!(of(4, true, NotReady), SatOutThen::Next { watching: true });
    assert_eq!(of(1, true, NotReady), SatOutThen::Last { watching: true }, "the match's last stage");
    assert_eq!(of(4, false, StillLoading), SatOutThen::Out { watching: true }, "a missed stage ends it");
    assert_eq!(of(4, true, Watching), SatOutThen::Chosen { watching: true });
    assert_eq!(of(1, true, Watching), SatOutThen::Last { watching: true });
}

/// Found by round P1b (2026-11-16), fixed 2026-11-18: a press right after
/// the one that readied was read as a second Ready while the room had not
/// answered the first (an online room's round trip: 160 ms here), so it
/// never counted as the first press of taking Ready back; and one right
/// after taking it back was read as a second take-back instead of a Ready.
/// What the player asked last counts (`RoomClient::ready_as_asked`): the
/// press that readies is never also the first of the gesture, and every
/// press after it counts.
#[test]
fn a_press_within_the_rooms_round_trip_after_ready_is_never_lost() {
    use crate::state::online::tests::room::{client, person, prepare, song, tick};
    use ddi_room::fake::{FakeServer, Latency};
    use ddi_room::{Identity, Intent, ManualClock, RoomPolicy};
    let clock = ManualClock::new(1_000);
    let server = FakeServer::hosted(
        "Room000001",
        Identity::new("dev:alice"),
        "alice",
        RoomPolicy::default(),
        clock.clone(),
    )
    .shared();
    server
        .borrow_mut()
        .join(Identity::new("dev:bob"), "bob", Intent::Play)
        .unwrap();
    let alice = Rc::new(RefCell::new(client(&server, person(&server, "alice"), "alice", 1_000)));
    let bob = Rc::new(RefCell::new(client(&server, person(&server, "bob"), "bob", 1_000)));
    let step = |ms: u64| tick(&clock, &mut [&mut alice.borrow_mut(), &mut bob.borrow_mut()], ms);
    step(400);
    let s = song();
    alice.borrow_mut().pick(&s, &s.charts[0]).unwrap();
    step(400);
    crate::state::online::tests::room::upload(&server, &mut alice.borrow_mut(), "abc12345");
    step(400);
    for r in [&alice, &bob] {
        prepare(&mut r.borrow_mut());
    }
    step(400);
    let order = bob.borrow_mut().take_stage_order().unwrap();
    let orders = Orders::default();
    let seen: Rc<RefCell<Vec<PromptChoice>>> = Rc::default();
    let sink = seen.clone();
    let mut hb = harness();
    hb.push(
        PromptScreen::new(
            env(),
            Rc::new(RefCell::new(PromptState::default())),
            Rc::new(move |c, _| sink.borrow_mut().push(c)),
        )
        .with_room(bob.clone(), order.stage, shell(&bob, &orders)),
    );
    hb.run(0.3);
    let me = bob.borrow().me();
    server.borrow_mut().set_latency(
        me,
        Latency {
            down_ms: 80,
            down_jitter_ms: 0,
            up_ms: 80,
            up_jitter_ms: 0,
            seed: 1,
        },
    );
    // Frames of 16 ms with the room's clock along.
    let frames = |hb: &mut Harness, n: usize| {
        for _ in 0..n {
            step(16);
            hb.frame(0.016);
        }
    };
    // Ready, and in the same frame a first press of taking it back; the
    // room answers the Ready 160 ms later.
    hb.key("Enter", "Enter");
    hb.key("Enter", "Enter");
    assert!(!bob.borrow().ready(), "the room has not answered yet");
    assert!(bob.borrow().ready_as_asked());
    hb.frame(0.0);
    assert!(has(&hb, "Cancel ready"), "{:?}", hb.texts());
    frames(&mut hb, 15);
    assert!(bob.borrow().ready(), "answered");
    assert_eq!(*seen.borrow(), [PromptChoice::Ready], "one Ready told the shell");
    // The second press of the gesture, 0.24 s after the first: taken back.
    hb.key("Enter", "Enter");
    assert!(!bob.borrow().ready_as_asked(), "taken back as asked");
    // At once a press again, the take-back not answered: it readies.
    hb.key("Enter", "Enter");
    assert!(bob.borrow().ready_as_asked(), "ready again as asked");
    frames(&mut hb, 30);
    assert!(bob.borrow().ready(), "the room says ready");
    assert!(bob.borrow().refusal().is_none(), "{:?}", bob.borrow().refusal());
    assert_eq!(seen.borrow().len(), 2, "{:?}", seen.borrow());
    // Then the gesture as ever: twice in time, answered late, not ready.
    hb.key("Enter", "Enter");
    frames(&mut hb, 5);
    hb.key("Enter", "Enter");
    frames(&mut hb, 30);
    assert!(!bob.borrow().ready());
    assert!(has(&hb, "Ready"), "{:?}", hb.texts());
}
