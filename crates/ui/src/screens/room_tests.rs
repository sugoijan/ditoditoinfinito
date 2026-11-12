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
use super::{DESIGN, Env, sample};
use crate::look::Plain;
use crate::state::room::{RoomClient, SharedRoom, StageOrder};
use crate::testing::Harness;
use crate::{PointerKind, TextMode};
use ddi_room::Phase;

fn env() -> Env {
    Env::new(
        Rc::new(Plain),
        Rc::new(|s| ddi_library::search::Fold::nfkd(&ddi_library::search::Nfkd, s)),
    )
}

fn harness() -> Harness {
    Harness::new(DESIGN, [1280.0, 720.0], 1.0, TextMode::Own)
}

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

/// What the shell was asked: the stages' prompts it opened.
type Orders = Rc<RefCell<Vec<StageOrder>>>;
/// The uploads it was handed.
type Uploads = Rc<RefCell<Vec<crate::state::room::UploadJob>>>;

/// The room's shell for tests: songs prepared at once, prompts recorded.
fn shell(room: &SharedRoom, orders: &Orders) -> RoomShell {
    shell_with(room, orders, &Uploads::default())
}

/// [`shell`], its uploads kept in `uploads`.
fn shell_with(room: &SharedRoom, orders: &Orders, uploads: &Uploads) -> RoomShell {
    let shared = room.clone();
    let orders = orders.clone();
    let uploads = uploads.clone();
    Rc::new(move |call, cx| match call {
        RoomCall::Leave | RoomCall::Discard => cx.close(true),
        // A shared song read as the online tests' song, its id the
        // share's; a library song prepared at once.
        RoomCall::Prepare(job) if job.share.is_some() => {
            let s = crate::state::online::tests::room::song();
            let picked = crate::state::room::Picked {
                song: SongEntry {
                    id: job.song.clone(),
                    ..s.clone()
                },
                chart: s.charts[0].clone(),
            };
            shared.borrow_mut().finish_shared_load(&job, Ok(picked));
        }
        RoomCall::Prepare(job) => shared.borrow_mut().finish_load(&job, Ok(())),
        RoomCall::PlayStage(order) => orders.borrow_mut().push(*order),
        RoomCall::Upload(job) => uploads.borrow_mut().push(*job),
        RoomCall::Redirect(_) => {}
    })
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

fn top(h: &Harness) -> String {
    h.ui.layers().last().unwrap().name.clone()
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
    h.tap(PointerKind::Mouse, name);
    h.run(0.5);
}

fn has(h: &Harness, text: &str) -> bool {
    h.texts().iter().any(|t| t == text)
}

#[test]
fn the_start_menu_opens_multiplayer_only_behind_the_switch() {
    for on in [false, true] {
        let calls = Rc::new(RefCell::new(Vec::new()));
        let sink = calls.clone();
        let mut h = harness();
        h.push(
            StartScreen::new(env(), false, Rc::new(move |c, _| sink.borrow_mut().push(c)))
                .with_multiplayer(on),
        );
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
    assert_eq!(said(&h, "Players.Count"), "4 / 9");
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
            .all(|t| !t.contains(" of ") && !t.contains("Lv") && !t.contains("Level "))
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
    // Up to the cap: eight CPUs beside the player, then nothing more.
    press(&mut h, "AddRandom");
    press(&mut h, "AddRandom");
    assert_eq!(view(&env).draft, vec![4, 8]);
    press(&mut h, "AddRandom");
    assert_eq!(view(&env).draft, vec![4, 8], "the cap holds");
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
    assert!(has(&h, "This match · locked"), "{:?}", h.texts());
    assert!(
        h.texts()
            .iter()
            .any(|t| t == "4 stages · 3 CPUs · DDI · Easy timing · Boos · No attacks"),
        "{:?}",
        h.texts()
    );
}

/// The rules' summary says what differs from the usual: attacks are On by
/// default where they are offered, so it says "No attacks" where the
/// match has them turned off, and nothing of them where it has them or
/// where the room offers none (an online room).
#[test]
fn the_rules_summary_says_no_attacks_only_where_they_are_turned_off() {
    use super::room::rules_words;
    let rules = crate::state::room::StageRules::default();
    let words = |r: &crate::state::room::StageRules, attacks| String::from(rules_words(r, attacks));
    assert_eq!(words(&rules, Some(true)), "DDI");
    assert_eq!(words(&rules, Some(false)), "DDI · No attacks");
    assert_eq!(words(&rules, None), "DDI");
    let hard = crate::state::room::StageRules {
        difficulty: ddi_room::MatchDifficulty::Hard,
        boos: true,
        ..Default::default()
    };
    assert_eq!(
        words(&hard, Some(false)),
        "DDI · Hard timing · Boos · No attacks"
    );
    // A stage's prompt: the same words after the stage's line.
    use super::prompt::PromptInfo;
    let mut info = PromptInfo::default();
    info.set_stage(1, 4, 3, 0, &rules, Some(false));
    assert!(info.mods.ends_with("Rules: DDI · No attacks"), "{}", info.mods);
    let mut info = PromptInfo::default();
    info.set_stage(1, 4, 3, 0, &rules, Some(true));
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
    let rows = super::room::panel_rows(&room.borrow());
    let (popup, _) = super::room::panel(&e, &rows, 0, [0.0, 0.0, 420.0, 600.0]);
    let t = texts(&popup);
    assert!(t.contains(&"Retaliate".to_string()) && t.contains(&"Leader".to_string()), "{t:?}");
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
        has(&hb, "You weren't ready in time, so you sit this stage out."),
        "{:?}",
        hb.texts()
    );
    hb.run(3.0);
    assert_eq!(seen.borrow().last(), Some(&PromptChoice::Back));
    assert!(!seen.borrow().contains(&PromptChoice::Start));
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

/// What the text actor named `name` says.
fn said(h: &Harness, name: &str) -> String {
    match &h.actor(name).kind {
        ddi_actor::Kind::Text(t) => t.text().to_string(),
        _ => panic!("{name} is not text"),
    }
}

/// Whether an actor named `name` is in the tree.
fn there(h: &Harness, name: &str) -> bool {
    h.ui.actor(name).is_some()
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
    assert_eq!(said(&h, "Players.Count"), "9 / 9");
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
    assert_eq!(said(&h, "Players8.Points"), "0");
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
        assert!(has(&h, "You") && has(&h, "4 / 9"), "{texts:?}");
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
            &["Room.Rules", "Room.Players", "More", "Back"]
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
            press(&mut h, "Room.Players");
            assert_eq!(top(&h), "room-players");
            assert!(has(&h, "Picks") && has(&h, "You"), "{:?}", h.texts());
            h.key("Escape", "Escape");
            h.run(0.5);
            assert_eq!(top(&h), "select", "Back closes it");
            press(&mut h, "Room.Players");
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
    assert_eq!(b.status, "The host is choosing the first song.");
    assert!(online.can_invite && online.can_rotate);
    assert!(!b.online.as_ref().unwrap().can_invite);
    assert!(has(&ha, "AFTER HOURS"), "{:?}", ha.texts());
    // Bob's panel: the picks, the players (the host's role).
    assert!(has(&hb, "Picks") && has(&hb, "Host"), "{:?}", hb.texts());
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
    press(&mut ha, "Room.Players");
    assert!(has(&ha, "Reconnecting"), "{:?}", ha.texts());
    press(&mut ha, "Close");
    step(1_000);
    hb.run(0.3);
    assert_eq!(view(&eb).online.unwrap().link, "connected");
    // 4401: Sign in again, offered in More.
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
    assert_eq!((host.role.as_str(), host.status.as_str()), ("Host", ""));
    assert!(row(&alice, "You").me);
    assert_eq!(says(&alice, "dave"), ("Watching".into(), Tone::Plain));
    assert_eq!(row(&alice, "bob").place, None);
    // The pick: its song shared, then everyone prepares it.
    let s = song();
    alice.borrow_mut().pick(&s, &s.charts[0]).unwrap();
    step(&mut carol, 400);
    upload(&server, &mut alice.borrow_mut(), "abc12345");
    step(&mut carol, 400);
    assert_eq!(says(&alice, "bob"), ("Not ready".into(), Tone::Plain));
    let a = alice.borrow_mut().take_load_jobs();
    alice.borrow_mut().finish_load(&a[0], Ok(()));
    alice.borrow_mut().set_ready(true).unwrap();
    // Bob is ready and still loading; Carol's download failed.
    let b = bob.borrow_mut().take_load_jobs();
    bob.borrow_mut().set_ready(true).unwrap();
    let c = carol.take_load_jobs();
    carol.finish_shared_load(&c[0], Err(LoadProblem::Download));
    step(&mut carol, 400);
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
    for (h, me, other, state) in [
        (&ha, "You", "bob", "Loading"),
        (&hb, "alice", "You", "Ready"),
    ] {
        assert_eq!(said(h, "Players.Count"), "4 / 9");
        assert_eq!(said(h, "Players0.Name"), me);
        assert_eq!(said(h, "Players0.Role"), "Host");
        assert_eq!(said(h, "Players1.Name"), other);
        assert_eq!(
            said(
                h,
                &format!("Players{}.State", if me == "You" { 1 } else { 0 })
            ),
            state
        );
        assert_eq!(said(h, "Players2.State"), "Download failed");
        assert_eq!(said(h, "Players3.State"), "Watching");
        assert_eq!(ha.ui.bounds("RoomPlayers"), h.ui.bounds("RoomPlayers"));
    }
    assert!(has(&hb, "Picks"), "{:?}", hb.texts());
    assert_eq!(
        hb.texts().iter().filter(|t| *t == "Players").count(),
        2,
        "the header's button and the panel's heading: {:?}",
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
    assert_eq!(says(&alice, "carol"), ("Sitting out".into(), Tone::Plain));
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
