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

/// CPU practice opens the room's song select with the Rules pop-up first
/// (Reset and OK); OK leaves it closed until the player leaves the room
/// and comes back.
#[test]
fn practice_opens_on_the_rules_then_song_select_and_again_after_leaving() {
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
    let v = view(&env);
    assert!(!v.rules_open);
    assert_eq!((v.view.as_str(), v.phase.as_str()), ("select", "lobby"));
    assert!(v.host);
    assert_eq!(v.status, "Pick a song to start the match.");
    assert!(
        has(&h, "Pick a song to start the match."),
        "{:?}",
        h.texts()
    );
    assert!(has(&h, "CPU PRACTICE") || h.texts().iter().any(|t| t.contains("PRACTICE")));
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

/// The host's Start on a song is the pick: the CPUs' unplayable chart is
/// refused in a pop-up; a playable one starts the match and the stage's
/// prompt opens.
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
    assert!(
        h.texts().iter().any(|t| t.ends_with("Pick for stage 1")),
        "{:?}",
        h.texts()
    );
    h.key("Enter", "Enter");
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
    press(&mut h, "AddRandom");
    assert_eq!(view(&env).draft, vec![4, 4]);
    press(&mut h, "Remove.0");
    assert_eq!(view(&env).draft, vec![4, 3]);
    assert!(!has(&h, &first));
    // Add custom: two at level 5, then one by name.
    press(&mut h, "AddCustom");
    assert_eq!(top(&h), "room-add-cpus");
    for _ in 0..3 {
        press(&mut h, "Inc.level");
    }
    press(&mut h, "Inc.count");
    assert!(has(&h, "Add 2 at level 5"), "{:?}", h.texts());
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
    press(&mut h, "Apply");
    assert_eq!(top(&h), "select");
    let v = view(&env);
    assert_eq!((v.stage_count, v.cpus), (2, 8));
    let at_five = room
        .borrow()
        .rules()
        .cpu
        .roster
        .iter()
        .filter(|c| c.level == 4)
        .count();
    assert!(at_five >= 2, "{at_five}");
    // Reset: the defaults, the pop-up still open.
    press(&mut h, "Room.Rules");
    press(&mut h, "Cancel");
    assert_eq!(top(&h), "select");
    room.borrow_mut().set_rules_seen(false);
    h.run(0.5);
    assert_eq!(top(&h), "room-rules");
    press(&mut h, "Reset");
    assert_eq!(top(&h), "room-rules");
    assert_eq!(view(&env).draft, vec![4, 3]);
    press(&mut h, "Ok");
    assert_eq!(view(&env).stage_count, 4);
    // A running match: its rules locked, shown in a line.
    let s = &library("dance-single").songs[0];
    room.borrow_mut().pick(s, &s.charts[0]).unwrap();
    h.run(0.5);
    assert_eq!(view(&env).phase, "preparing");
    press(&mut h, "Room.Rules");
    assert!(has(&h, "This match · locked"), "{:?}", h.texts());
    assert!(has(&h, "4 stages · 3 CPUs"), "{:?}", h.texts());
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
    // Both see the Rules first: Bob's read only.
    assert_eq!(top(&hb), "room-rules");
    assert!(has(&hb, "OK") && !has(&hb, "Reset"), "{:?}", hb.texts());
    assert!(has(&ha, "Reset"), "{:?}", ha.texts());
    press(&mut ha, "Ok");
    press(&mut hb, "Ok");
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
