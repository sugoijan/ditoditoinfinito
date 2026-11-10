//! The multiplayer prototype's screens in the headless harness, bound to
//! the practice authority (`state::room`): the gated start menu, choosing
//! songs through song select, a chart the CPUs cannot play refused, the
//! CPU count's limits, readiness, Start into the countdown and the stage,
//! and the Rules pop-up's locked and next-match rules.

use std::cell::RefCell;
use std::rc::Rc;

use super::data::{ChartEntry, Difficulty, SelectData, SongEntry};
use super::start::{Choice, StartScreen};
use super::{DESIGN, Env, sample};
use crate::look::Plain;
use crate::state::room::{RoomClient, SharedRoom};
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
    // The song with a chart in the style first: song select shows it.
    if style == "dance-double" {
        data.songs.reverse();
    }
    data
}

/// The room screen on `room`, its picker over [`library`] in `style`.
fn room_screen(env: &Env, room: SharedRoom, style: &'static str) -> super::room::RoomScreen {
    use super::room::{RoomCall, RoomScreen};
    let e = env.clone();
    let shared = room.clone();
    RoomScreen::new(
        env.clone(),
        room,
        Rc::new(move |call, cx| match call {
            RoomCall::Leave | RoomCall::Discard => cx.close(true),
            RoomCall::ChooseSong { stage, number } => {
                cx.push(sample::pick_screen(
                    &e,
                    shared.clone(),
                    stage,
                    number,
                    library(style),
                ));
            }
            RoomCall::Prepare(job) => shared.borrow_mut().finish_load(&job, Ok(())),
            RoomCall::PlayStage(_) | RoomCall::Redirect(_) | RoomCall::Upload(_) => {}
        }),
    )
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
            // As it always was: Coming soon, focusable and inert.
            assert!(calls.borrow().is_empty());
            assert!(texts.iter().any(|t| t.contains("Coming soon")), "{texts:?}");
        }
    }
}

#[test]
fn the_home_opens_practice_and_back_returns_through_it() {
    let env = env();
    let mut h = harness();
    h.push(sample::start_screen(&env, false).with_multiplayer(true));
    h.run(0.3);
    h.key("a", "a");
    h.key("ArrowDown", "ArrowDown");
    h.key("Enter", "Enter");
    h.run(0.5);
    assert_eq!(top(&h), "multiplayer");
    let texts = h.texts();
    for t in [
        "Find match · Coming soon",
        "Browse rooms · Coming soon",
        "CPU practice",
        "Private room",
        "Join by link",
        "Sign in with Twitch",
    ] {
        assert!(texts.iter().any(|x| x == t), "{t}: {texts:?}");
    }
    // Focus waits for the first key, as the start menu's; the grayed
    // entries take focus and do nothing.
    assert_eq!(h.ui.focused(), None);
    h.key("a", "a");
    assert_eq!(h.ui.focused(), Some("Choice.find"));
    h.key("Enter", "Enter");
    h.run(0.3);
    assert_eq!(top(&h), "multiplayer");
    // The private room asks to sign in first, in a dialog naming Twitch.
    press(&mut h, "Choice.private");
    assert_eq!(top(&h), "sign-in");
    assert!(
        h.texts()
            .iter()
            .any(|t| t.starts_with("Private rooms are invite-only")),
        "{:?}",
        h.texts()
    );
    press(&mut h, "Confirm");
    assert!(
        h.texts().iter().any(|t| t == "Signing in…"),
        "{:?}",
        h.texts()
    );
    press(&mut h, "Cancel");
    assert_eq!(top(&h), "multiplayer");
    // Join by link: a page over the home, Back to it.
    press(&mut h, "Choice.join");
    assert_eq!(top(&h), "join-link");
    h.key("Escape", "Escape");
    h.run(0.5);
    assert_eq!(top(&h), "multiplayer");
    press(&mut h, "Choice.practice");
    assert_eq!(top(&h), "room");
    assert_eq!(view(&env).phase, "lobby");
    h.key("Escape", "Escape");
    h.run(0.5);
    assert_eq!(top(&h), "multiplayer");
    h.key("Escape", "Escape");
    h.run(0.5);
    assert_eq!(top(&h), "start");
}

#[test]
fn songs_are_chosen_through_song_select_and_unplayable_charts_refused() {
    let env = env();
    let room = RoomClient::shared(0, 1);
    // Song select opens on the Double chart: refused, with its reason.
    let mut h = harness();
    h.push(room_screen(&env, room.clone(), "dance-double"));
    h.run(0.5);
    assert_eq!(view(&env).stages.len(), 4, "four stages by default");
    press(&mut h, "Stage.0");
    assert_eq!(top(&h), "select");
    assert!(
        h.texts().iter().any(|t| t.ends_with("Use for stage 1")),
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
    assert!(room.borrow().stages()[0].content.is_none());
    h.key("Enter", "Enter");
    h.run(0.3);
    assert_eq!(top(&h), "select");
    // Back without choosing: the room as it was.
    h.key("Escape", "Escape");
    h.run(0.5);
    assert_eq!(top(&h), "room");
    assert_eq!(view(&env).stages[0].song, None);

    // On the Single chart: the stage takes it.
    let mut h = harness();
    h.push(room_screen(&env, room.clone(), "dance-single"));
    h.run(0.5);
    press(&mut h, "Stage.1");
    assert_eq!(view(&env).selected, 1);
    press(&mut h, "Stage.1");
    assert_eq!(top(&h), "select");
    h.key("Enter", "Enter");
    h.run(0.5);
    assert_eq!(top(&h), "room");
    let v = view(&env);
    assert_eq!(v.stages[1].song.as_deref(), Some("single"));
    assert_eq!(v.stages[1].chart, Some(0));
    assert!(
        h.texts().iter().any(|t| t == "Single Song"),
        "{:?}",
        h.texts()
    );
    assert!(!v.ready, "choosing never readies");
}

#[test]
fn ready_is_explicit_and_start_counts_down_into_the_stage() {
    let env = env();
    let room = RoomClient::shared(0, 1);
    let mut h = harness();
    h.push(room_screen(&env, room.clone(), "dance-single"));
    h.run(0.5);
    // Start without a song, then without Ready: refused, said, nothing
    // locked.
    press(&mut h, "Start");
    assert_eq!(view(&env).phase, "lobby");
    assert!(
        view(&env).status.contains("Choose a song"),
        "{:?}",
        view(&env)
    );
    press(&mut h, "Choose");
    h.key("Enter", "Enter");
    h.run(0.5);
    assert_eq!(top(&h), "room");
    press(&mut h, "Start");
    assert_eq!(view(&env).phase, "lobby");
    assert_eq!(view(&env).status, "Press Ready first.");
    // Ready, unready, ready.
    press(&mut h, "Ready");
    assert!(view(&env).ready);
    assert!(view(&env).members[0].ready);
    press(&mut h, "Ready");
    assert!(!view(&env).ready);
    press(&mut h, "Ready");
    assert!(view(&env).ready);
    // Start: the match locks (the player and three CPUs) and counts down.
    press(&mut h, "Start");
    let v = view(&env);
    assert_eq!(v.phase, "countdown", "{v:?}");
    assert!(v.locked);
    assert_eq!(v.members.iter().filter(|m| m.cpu && !m.at_start).count(), 3);
    assert_eq!(v.start, "Cancel start");
    assert!(v.countdown.is_some_and(|s| (1..=3).contains(&s)), "{v:?}");
    // Cancel goes back to preparing, readiness kept.
    press(&mut h, "Start");
    assert_eq!(view(&env).phase, "preparing");
    assert!(view(&env).ready);
    assert_eq!(view(&env).start, "Start stage 1");
    press(&mut h, "Start");
    assert_eq!(view(&env).phase, "countdown");
    h.run(3.2);
    let v = view(&env);
    assert_eq!(v.phase, "stage", "{v:?}");
    assert_eq!(v.status, "Stage 1 is being played.", "{v:?}");
    assert_eq!(v.start, "Stop match");
    assert!(v.stages[0].started);
    // Stop, then the next match: fresh readiness, the record kept.
    press(&mut h, "Start");
    assert_eq!(view(&env).phase, "interrupted");
    press(&mut h, "Start");
    let v = view(&env);
    assert_eq!(v.phase, "lobby");
    assert_eq!(v.history, 1);
    assert!(!v.ready);
    assert!(matches!(room.borrow().state().phase, Phase::Lobby));
}

#[test]
fn the_rules_pop_up_keeps_cpus_within_the_cap_and_locks_the_running_match() {
    let env = env();
    let room = RoomClient::shared(0, 1);
    let mut h = harness();
    h.push(room_screen(&env, room.clone(), "dance-single"));
    h.run(0.5);
    press(&mut h, "Rules");
    assert_eq!(top(&h), "room-rules");
    let texts = h.texts();
    assert!(texts.iter().any(|t| t == "Next match"), "{texts:?}");
    assert!(!texts.iter().any(|t| t == "This match · locked"));
    assert!(texts.iter().any(|t| t.contains("provisional")));
    assert_eq!(view(&env).draft, [4, 3, 3]);
    for _ in 0..10 {
        press(&mut h, "Inc.cpus");
    }
    assert_eq!(
        view(&env).draft[1],
        8,
        "the cap of nine, the player included"
    );
    for _ in 0..10 {
        press(&mut h, "Dec.cpus");
    }
    assert_eq!(view(&env).draft[1], 1, "at least one opponent");
    press(&mut h, "Inc.cpus");
    press(&mut h, "Dec.stages");
    press(&mut h, "Inc.level");
    assert_eq!(view(&env).draft, [3, 2, 4]);
    // Cancel keeps the room's rules.
    press(&mut h, "Cancel");
    assert_eq!(top(&h), "room");
    assert_eq!((view(&env).cpus, view(&env).stage_count), (3, 4));
    press(&mut h, "Rules");
    press(&mut h, "Inc.cpus");
    press(&mut h, "Dec.stages");
    press(&mut h, "Apply");
    assert_eq!(top(&h), "room");
    let v = view(&env);
    assert_eq!((v.cpus, v.stage_count, v.level), (4, 3, 3));
    assert_eq!(v.members.iter().filter(|m| m.at_start).count(), 4);
    assert_eq!(v.stages.len(), 3);

    // A running match: its rules locked, the next match's a draft.
    press(&mut h, "Choose");
    h.key("Enter", "Enter");
    h.run(0.5);
    press(&mut h, "Ready");
    press(&mut h, "Start");
    assert_eq!(view(&env).phase, "countdown");
    press(&mut h, "Rules");
    let texts = h.texts();
    assert!(
        texts.iter().any(|t| t == "This match · locked"),
        "{texts:?}"
    );
    assert!(texts.iter().any(|t| t == "Next match"));
    press(&mut h, "Inc.cpus");
    press(&mut h, "Apply");
    let r = room.borrow();
    let running = r.locked_rules().unwrap();
    assert_eq!(r.cpus_of(running), 4, "the running match keeps its CPUs");
    assert_eq!(
        r.cpus_of(&r.state().next.rules),
        5,
        "the next match's draft"
    );
    assert!(matches!(r.state().phase, Phase::Countdown { .. }));
    let first = &r.stages()[0];
    assert!(
        first.prep(r.me()).ready,
        "a draft clears nobody's readiness"
    );
}

#[test]
fn the_room_fits_a_phone_and_is_kept_across_screens() {
    let env = env();
    let room = RoomClient::shared(0, 1);
    for (size, design) in [([844.0, 390.0], super::COMPACT), ([1280.0, 720.0], DESIGN)] {
        let mut h = Harness::new(design, size, 1.0, TextMode::Own);
        h.push(room_screen(&env, room.clone(), "dance-single"));
        h.run(0.5);
        for name in [
            "Stage.0", "Start", "Ready", "Rules", "More", "Back", "Choose",
        ] {
            let [x0, y0, x1, y1] = h.ui.bounds(name).unwrap_or_else(|| panic!("{name}"));
            let [x0, y0] = h.css([x0, y0]);
            let [x1, y1] = h.css([x1, y1]);
            assert!(
                x0 >= 0.0 && y0 >= 0.0 && x1 <= size[0] && y1 <= size[1],
                "{name} {size:?}"
            );
        }
        // Controller: More opens More, which holds Leave room.
        h.pad(crate::nav::MenuAction::More, true);
        h.pad(crate::nav::MenuAction::More, false);
        h.run(0.3);
        assert!(h.texts().iter().any(|t| t == "Leave room"));
        h.key("Escape", "Escape");
        h.run(0.3);
        assert_eq!(top(&h), "room");
    }
    // A song chosen in one screen is there in the next.
    let third = room.borrow().stages()[2].id;
    room.borrow_mut()
        .choose(
            third,
            &library("dance-single").songs[0],
            &chart("dance-single", 0, 9),
        )
        .unwrap();
    let mut h = harness();
    h.push(room_screen(&env, room.clone(), "dance-single"));
    h.run(0.5);
    assert_eq!(view(&env).stages[2].song.as_deref(), Some("single"));
}

/// The room's screen whose shell plays each stage it is handed by
/// reporting `places` (the player's place, from 1, per stage) at once,
/// as a stand-in for the play (the shells' tests play it for real).
fn reporting_room(
    env: &Env,
    room: SharedRoom,
    places: Vec<usize>,
) -> (super::room::RoomScreen, Rc<RefCell<usize>>) {
    use super::room::{RoomCall, RoomScreen};
    use ddi_room::{Finish, Finisher};
    let played = Rc::new(RefCell::new(0));
    let count = played.clone();
    let shared = room.clone();
    let screen = RoomScreen::new(
        env.clone(),
        room,
        Rc::new(move |call, _cx| match call {
            RoomCall::Prepare(job) => shared.borrow_mut().finish_load(&job, Ok(())),
            RoomCall::PlayStage(order) => {
                let mut ids: Vec<_> = order.cpus.iter().map(|c| c.participant).collect();
                let place = places[*count.borrow()].min(ids.len() + 1);
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
                *count.borrow_mut() += 1;
                shared
                    .borrow_mut()
                    .report_stage(order.stage, ranking, false)
                    .unwrap();
            }
            _ => {}
        }),
    );
    (screen, played)
}

#[test]
fn stages_played_return_to_the_intermission_then_the_match_results_and_the_lobby() {
    let env = env();
    let room = RoomClient::shared(0, 1);
    {
        let mut r = room.borrow_mut();
        let rules = ddi_room::MatchRules {
            stages: 2,
            ..r.rules().clone()
        };
        r.save_rules(rules).unwrap();
        let lib = library("dance-single");
        for st in r.stages().iter().map(|s| s.id).collect::<Vec<_>>() {
            r.choose(st, &lib.songs[0], &lib.songs[0].charts[0])
                .unwrap();
        }
    }
    let (screen, played) = reporting_room(&env, room.clone(), vec![1, 4]);
    let mut h = harness();
    h.push(screen);
    h.run(0.5);
    press(&mut h, "Ready");
    press(&mut h, "Start");
    h.run(3.2);
    assert_eq!(*played.borrow(), 1, "handed over once");
    let v = view(&env);
    assert_eq!(v.phase, "intermission", "{v:?}");
    assert_eq!(
        v.mark.as_deref(),
        Some("Stage 1: place 1 of 4 · +4 pts · points experimental")
    );
    assert_eq!(v.status, "Press Ready when you are, then Start stage 2.");
    assert_eq!(v.start, "Start stage 2");
    assert!(!v.ready, "Ready again for the next stage");
    assert!(
        h.texts().iter().any(|t| t == "#1 · 4 pts"),
        "{:?}",
        h.texts()
    );
    // The next stage, the last: the match's results where the song was.
    press(&mut h, "Ready");
    press(&mut h, "Start");
    h.run(3.2);
    assert_eq!(*played.borrow(), 2);
    let v = view(&env);
    assert_eq!(v.phase, "matchresults", "{v:?}");
    assert_eq!(v.start, "Next match");
    let mine = v.standings.iter().find(|s| s.id == 1).unwrap();
    assert_eq!(mine.total, "5");
    assert_eq!(mine.stages, vec![Some("4".into()), Some("1".into())]);
    let texts = h.texts();
    assert!(texts.iter().any(|t| t == "Match results"), "{texts:?}");
    assert!(texts.iter().any(|t| t == "Winner: CPU 1"), "{texts:?}");
    // The next match: the lobby, fresh readiness, the record kept.
    press(&mut h, "Start");
    let v = view(&env);
    assert_eq!(v.phase, "lobby");
    assert!(!v.ready);
    assert_eq!(v.history, 1);
    assert!(v.mark.is_none());
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
    assert!(!texts.iter().any(|t| t == "REPLAY"), "{texts:?}");
    h.key("r", "KeyR");
    h.run(0.2);
    assert_eq!(*retried.borrow(), 0, "a stage is played once");
    press(&mut h, "SongList");
    assert_eq!(*back.borrow(), 1);
}

/// An online room (M5c) over the fake room: the owner and a member see
/// the same room, roles from the shared rule (Start and Choose song the
/// owner's; More's invite items the owner's), each person's connection on
/// their row, and the link's states on the status line.
#[test]
fn an_online_room_shows_roles_connections_and_its_link() {
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
    let mut ha = harness();
    ha.push(sample::room_screen(&ea, alice.clone()));
    let mut hb = harness();
    hb.push(sample::room_screen(&eb, bob.clone()));
    step(400);
    step(400);
    ha.run(0.3);
    hb.run(0.3);
    let (a, b) = (view(&ea), view(&eb));
    let online = a.online.as_ref().unwrap();
    assert_eq!(
        (online.link.as_str(), online.role.as_str()),
        ("connected", "owner")
    );
    assert_eq!(online.name.as_deref(), Some("After Hours"));
    assert!(online.can_invite && online.can_rotate);
    assert_eq!(b.online.as_ref().unwrap().role, "member");
    assert!(!b.online.as_ref().unwrap().can_invite);
    assert!(a.items.contains(&"Start".to_string()));
    assert!(a.items.contains(&"Choose".to_string()));
    assert!(!b.items.contains(&"Start".to_string()), "{:?}", b.items);
    assert!(!b.items.contains(&"Choose".to_string()));
    assert!(b.items.contains(&"Ready".to_string()));
    assert!(ha.texts().iter().any(|t| t == "Host"), "{:?}", ha.texts());
    assert!(
        ha.texts().iter().any(|t| t == "AFTER HOURS"),
        "{:?}",
        ha.texts()
    );
    // The host's song, shared with the room: the host's status says the
    // upload as it goes; Bob sees it as the host's until his device has
    // read it from the room's share, then by its title.
    let s = song();
    let first = alice.borrow().stages()[0].id;
    alice.borrow_mut().choose(first, &s, &s.charts[0]).unwrap();
    step(400);
    step(400);
    hb.run(0.3);
    assert!(
        hb.texts().iter().any(|t| t == "Song chosen by the host"),
        "{:?}",
        hb.texts()
    );
    assert_eq!(
        view(&eb).status,
        "Stage 1's song is being shared with the room…"
    );
    let jobs = alice.borrow_mut().take_upload_jobs();
    assert_eq!(jobs.len(), 1);
    alice.borrow_mut().upload_progress(first, 25.0, 100.0);
    ha.run(0.3);
    assert_eq!(
        view(&ea).status,
        "Sharing stage 1's song with the room… 25%"
    );
    alice
        .borrow_mut()
        .upload_finished(first, Err(ddi_library::share::ShareError::TooLarge));
    ha.run(0.3);
    assert!(
        view(&ea)
            .status
            .starts_with("Couldn't share stage 1's song:")
    );
    press(&mut ha, "More");
    assert!(
        ha.texts().iter().any(|t| t == "Retry sharing"),
        "{:?}",
        ha.texts()
    );
    ha.key("Escape", "Escape");
    ha.run(0.3);
    alice.borrow_mut().retry_sharing(first);
    step(400);
    crate::state::online::tests::room::upload(&server, &mut alice.borrow_mut(), "abc12345");
    step(400);
    crate::state::online::tests::room::prepare(&mut alice.borrow_mut());
    crate::state::online::tests::room::prepare(&mut bob.borrow_mut());
    step(400);
    hb.run(0.3);
    assert!(hb.texts().iter().any(|t| t == "A"), "{:?}", hb.texts());
    // Bob's socket drops: his row says Reconnecting to Alice, his own
    // status says so too.
    server.borrow_mut().drop_socket(ParticipantId(2));
    step(100);
    ha.run(0.3);
    hb.run(0.3);
    assert!(
        ha.texts().iter().any(|t| t == "Reconnecting"),
        "{:?}",
        ha.texts()
    );
    assert_eq!(view(&eb).status, "Connection lost. Reconnecting…");
    assert!(!view(&eb).items.contains(&"Ready".to_string()));
    step(1_000);
    hb.run(0.3);
    assert_eq!(view(&eb).online.unwrap().link, "connected");
    // 4401: Sign in again, offered in More, never opened by itself.
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
    assert!(
        ha.texts().iter().any(|t| t == "Sign in again"),
        "{:?}",
        ha.texts()
    );
}
