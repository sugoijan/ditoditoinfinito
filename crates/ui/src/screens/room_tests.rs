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
    assert!(has(&h, "This match is on stage 1 of 4 and keeps these rules:"), "{:?}", h.texts());
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
/// Strong; a room whose authority predates it shows the fixed row.
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

/// The At zero life row ("Step down", `step-down-v0`): Out first, each
/// choice with its sentence; applied, it is the match's and the summary
/// names it (Out is never named); Reset gives Out; a room whose authority
/// cannot honor it, or an online room, shows it fixed at Out.
#[test]
fn the_at_zero_row_chooses_step_down_where_the_room_offers_it() {
    let env = env();
    let room = RoomClient::shared(0, 1);
    room.borrow_mut().set_rules_seen(true);
    let orders = Orders::default();
    let mut h = harness();
    h.push(room_select(&env, room.clone(), "dance-single", &orders));
    h.run(0.5);
    press(&mut h, "Room.Rules");
    assert_eq!(said(&h, "Rule.at_zero.Value"), "Out");
    assert_eq!(view(&env).draft_at_zero.as_deref(), Some("out"));
    assert!(has(&h, "A player whose life runs out is out until the next stage."));
    press(&mut h, "Inc.at_zero");
    assert_eq!(said(&h, "Rule.at_zero.Value"), "Step down");
    assert_eq!(view(&env).draft_at_zero.as_deref(), Some("step_down"));
    assert!(
        h.texts().iter().any(|t| t.starts_with("A player whose life runs out gets half")),
        "{:?}",
        h.texts()
    );
    press(&mut h, "Apply");
    assert_eq!(room.borrow().rules().at_zero, ddi_room::AtZero::StepDown);
    let words = |r: &ddi_room::MatchRules| {
        String::from(super::room::rules_words(
            &crate::state::room::StageRules::of(r),
            None,
        ))
    };
    assert_eq!(words(room.borrow().rules()), "DDI · Step down");
    assert_eq!(words(&ddi_room::MatchRules::hosted()), "DDI");
    // Reset's default is Out.
    let mut settings = crate::state::settings::Settings::load();
    settings.multiplayer_rules_first = true;
    settings.save();
    room.borrow_mut().set_rules_seen(false);
    h.run(0.5);
    press(&mut h, "Reset");
    assert_eq!(view(&env).draft_at_zero.as_deref(), Some("out"));
    press(&mut h, "Ok");
    assert_eq!(room.borrow().rules().at_zero, ddi_room::AtZero::Out);
    settings.multiplayer_rules_first = false;
    settings.save();
    // An authority that cannot honor it: fixed at Out, nothing offered.
    let older = ddi_room::RoomPolicy {
        step_down: false,
        ..ddi_room::RoomPolicy::default()
    };
    let room = std::rc::Rc::new(std::cell::RefCell::new(RoomClient::practice_with_policy(
        0, 1, older,
    )));
    room.borrow_mut().set_rules_seen(true);
    let mut h = harness();
    h.push(room_select(&env, room.clone(), "dance-single", &orders));
    h.run(0.5);
    press(&mut h, "Room.Rules");
    assert!(!there(&h, "Inc.at_zero"));
    assert_eq!(said(&h, "Rule.at_zero.Value"), "Out · not in online rooms yet");
    assert_eq!(view(&env).draft_at_zero, None);
}

/// An authority that predates the Points choice: the row is fixed, as it
/// was, and nothing is offered it cannot honor.
#[test]
fn a_room_on_an_older_server_shows_points_fixed() {
    let env = env();
    let older = ddi_room::RoomPolicy {
        points_bias: false,
        ..ddi_room::RoomPolicy::default()
    };
    let room = std::rc::Rc::new(std::cell::RefCell::new(RoomClient::practice_with_policy(
        0, 1, older,
    )));
    room.borrow_mut().set_rules_seen(true);
    let orders = Orders::default();
    let mut h = harness();
    h.push(room_select(&env, room.clone(), "dance-single", &orders));
    h.run(0.5);
    press(&mut h, "Room.Rules");
    assert!(!there(&h, "Inc.points"));
    assert_eq!(said(&h, "Fixed.0.Value"), "By place · experimental");
    assert_eq!(view(&env).draft_points, None);
    assert!(has(&h, "Points for fields other than eight players follow an experimental rule."));
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
    assert_eq!(
        (host.role.as_str(), host.status.as_str()),
        ("Host · Picking", "")
    );
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
        assert_eq!(said(h, "Players.Count"), "4 / 73");
        assert_eq!(said(h, "Players0.Name"), me);
        assert_eq!(said(h, "Players0.Role"), "Host · Picking");
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

/// An online room over the fake room with alice (its owner), bob and
/// carol in it, each with a client; bob a co-host where `co_host`.
struct Three {
    clock: ddi_room::ManualClock,
    server: ddi_room::fake::SharedFake,
    alice: SharedRoom,
    bob: SharedRoom,
    carol: SharedRoom,
}

impl Three {
    fn new(co_host: bool) -> Three {
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
    fn step(&self, ms: u64) {
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
    fn settle(&self, ms: u64, screens: &mut [&mut Harness]) {
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
    // Nobody chooses (a room whose authority does not pass it on: its
    // owner away past its patience, a co-host there): said to everyone,
    // with how to take the pick to those who may.
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
        s.hosting = None;
        s.members.get_mut(&ParticipantId(1)).unwrap().connection = Connection::Disconnected;
        s.picking.as_mut().unwrap().picker = None;
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

/// A room whose authority predates the picker (no `picking` in its
/// state): as it was. Every operator has their own list, nobody is offered
/// the pick, and the others are told the host chooses.
#[test]
fn a_room_from_before_the_picker_shows_every_operator_their_list() {
    let t = Three::over(|state| state.picking = None, true);
    let (ea, eb, ec) = (env(), env(), env());
    let orders = Orders::default();
    let mut ha = online_select(&ea, &t.alice, songs_of(&["Alpha One"]), &orders);
    let mut hb = online_select(&eb, &t.bob, songs_of(&["Bravo One"]), &orders);
    let mut hc = online_select(&ec, &t.carol, songs_of(&["Charlie One"]), &orders);
    t.settle(400, &mut [&mut ha, &mut hb, &mut hc]);
    let (a, b, c) = (view(&ea), view(&eb), view(&ec));
    assert!(a.host && b.host && !c.host);
    assert!(!a.can_take_pick && !b.can_take_pick && !c.can_take_pick);
    assert!(list_shows(&ha) && list_shows(&hb) && !list_shows(&hc));
    for h in [&ha, &hb, &hc] {
        assert!(!there(h, "Room.TakePick") && !has(h, "Pick songs"));
        assert!(!has(h, "Host · Picking"), "{:?}", h.texts());
    }
    assert_eq!(c.status, "The host is choosing the first song.");
    hb.key("KeyT", "t");
    hb.pad(crate::nav::MenuAction::More, true);
    hb.pad(crate::nav::MenuAction::More, false);
    hb.run(0.3);
    assert!(!has(&hb, "Pick songs"), "{:?}", hb.texts());
    t.settle(400, &mut [&mut ha, &mut hb, &mut hc]);
    assert!(!t.server.borrow().state().one_picker());
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
    let (clock, alice, mut h) = late_room(1_000, &env);
    // Before the answer.
    let mut seen = Vec::new();
    for _ in 0..48 {
        seen.extend(frames(&clock, &[&alice], &mut h, 1.0 / 60.0, &["RoomJoining", "Header"]));
        assert!(no_view(&h), "{:?}", h.texts());
        assert!(!view(&env).host);
    }
    assert!(has(&h, "Joining the room…"), "{:?}", h.texts());
    assert_eq!(alpha(&h, "RoomJoining"), 1.0, "shown after a moment");
    assert_eq!(alpha(&h, "Header"), 1.0);
    // The answer comes (a second after the socket opened).
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

/// A link that ends before the room is known (not a member any more, the
/// room closed, the sign-in ended) says so where the joining line is:
/// never "Joining the room…" for good.
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
    frames(&t.clock, &[&dave], &mut h, 1.0, &[]);
    assert!(no_view(&h), "{:?}", h.texts());
    assert_eq!(said(&h, "RoomJoining"), "You left this room.");
    assert_eq!(alpha(&h, "RoomJoining"), 1.0);
    assert_eq!(view(&env).status, "You left this room.");
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
    press(&mut hb, "Room.Players");
    assert_eq!(top(&hb), "room-players");
    assert!(has(&hb, pass) && !there(&hb, "HostOptions"));
    assert!(!there(&hb, "Person.1") && !there(&hb, "Person.3"));
    press(&mut hb, "Close");
    // The owner's: the others' rows to tap, never their own, and Host
    // options.
    press(&mut ha, "Room.Players");
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
    press(&mut hc, "Room.Players");
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
    press(&mut hc, "Room.Players");
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
    press(&mut ha, "Room.Players");
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
    // Touch: the new owner taps Carol's row in their Players pop-up, and
    // Host options opens on her.
    press(&mut hb, "Room.Players");
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
/// someone away cannot be handed the room, and a room whose authority
/// does not pass rooms on offers co-hosts alone: no setting, no Make
/// host, nothing sent that it could not read.
#[test]
fn host_options_leave_out_bots_the_away_and_what_an_older_room_cannot_do() {
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
    press(&mut ha, "Room.Players");
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

    // A room from before the succession: co-hosts alone.
    let t = Three::new(false);
    let ea = env();
    let mut ha = online_select(&ea, &t.alice, songs_of(&["Alpha One"]), &orders);
    t.settle(400, &mut [&mut ha]);
    // Everyone is connected; from here the room is as an older authority
    // has it, and nothing is decided there that would say otherwise.
    t.server.borrow_mut().rewrite(|s| s.hosting = None);
    t.step(200);
    ha.run(0.5);
    let v = view(&ea);
    assert!(v.manages_hosts);
    assert_eq!(v.host_leaves, None);
    press(&mut ha, "Room.Players");
    assert!(!has(
        &ha,
        "If the host leaves, a co-host becomes the host, or else the player who joined first."
    ));
    press(&mut ha, "HostOptions");
    assert!(there(&ha, "CoHost.2") && !there(&ha, "MakeHost.2") && !there(&ha, "Dec.leaves"));
    let before = t.server.borrow().state().revision;
    assert!(t.alice.borrow_mut().make_host(ParticipantId(2)).is_ok());
    assert!(
        t.alice
            .borrow_mut()
            .set_host_leaves(ddi_room::HostLeaves::Close)
            .is_ok()
    );
    t.step(200);
    assert_eq!(t.server.borrow().state().revision, before, "nothing sent");
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
    // A room whose authority does not pass on.
    t.settle(400, &mut [&mut ha, &mut hb, &mut hc]);
    let hosting = t.server.borrow().state().hosting.clone();
    t.server.borrow_mut().rewrite(|s| s.hosting = None);
    t.step(200);
    ha.run(0.3);
    assert_eq!(
        text(&ea),
        "You are the host. The room stays open without a host, and you can come back by its link."
    );
    t.server.borrow_mut().rewrite(|s| s.hosting = hosting);
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
    press(&mut h, "Room.Players");
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
    assert!(has(&h, "Rules for the next match"), "{:?}", h.texts());
    assert!(has(&h, "This match is on stage 1 of 4 and keeps these rules:"), "{:?}", h.texts());
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

/// An online room during a match: a member sees the same statement with
/// the rows read only and no restart; a co-host restarts it for
/// everyone; a room whose authority predates the restart (no
/// `RoomPolicy::restart`) never offers it, and its Apply still saves.
#[test]
fn an_online_rooms_restart_is_for_its_operators_and_never_sent_to_an_older_room() {
    use ddi_room::{Actor, Command, Millis, ParticipantId};
    let start = |s: &mut ddi_room::RoomState| {
        let rev = s.revision;
        s.apply(
            Actor::Member(ParticipantId(1)),
            Some(rev),
            &Command::StartMatch,
            Millis(1_000),
        )
        .unwrap();
    };
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
    // A room whose authority predates the restart.
    let t = Three::over(|s| s.policy.restart = false, true);
    let (ea, eb) = (env(), env());
    let mut ha = online_select(&ea, &t.alice, songs_of(&["Alpha One"]), &orders);
    let mut hb = online_select(&eb, &t.bob, songs_of(&["Bravo One"]), &orders);
    t.settle(400, &mut [&mut ha, &mut hb]);
    t.server.borrow_mut().rewrite(start);
    t.settle(400, &mut [&mut ha, &mut hb]);
    press(&mut hb, "Room.Rules");
    let v = view(&eb);
    assert!(v.rules_running && !v.restart_offered, "{v:?}");
    assert!(!there(&hb, "Restart"));
    let rules = t.bob.borrow().rules().clone();
    assert!(t.bob.borrow_mut().restart_match(rules).is_err(), "never sent");
    t.bob.borrow_mut().clear_refusal();
    press(&mut hb, "Inc.boos");
    press(&mut hb, "Apply");
    t.settle(400, &mut [&mut ha, &mut hb]);
    let s = t.server.borrow().state().clone();
    assert!(s.next.rules.boos, "saved for the next match");
    assert_eq!(s.phase.kind(), ddi_room::PhaseKind::Preparing, "the match goes on");
}
