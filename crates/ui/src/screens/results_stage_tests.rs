//! A multiplayer stage's results in the headless harness (feedback round
//! B, `results_stage.rs`): the Standings and Graphs tabs, their keys, the
//! table for two, four and nine starters, a tie, someone out, someone who
//! left, a result not come yet, the match's result after its last stage,
//! and a phone's fewer columns.

use std::cell::RefCell;
use std::rc::Rc;

use ddi_actor::{Actor, Kind};
use ddi_room::{Finish, Finisher, OpponentSpec, ParticipantId, RunDetail};

use super::data::{ChartEntry, Difficulty, ResultsKind, SongEntry};
use super::results::{ResultsActions, ResultsScreen, Tab};
use super::{DESIGN, Env, sample};
use crate::hit;
use crate::look::Plain;
use crate::nav::MenuAction;
use crate::state::room::{RoomClient, SharedRoom, StageOrder};
use crate::testing::Harness;
use crate::{PointerKind, TextMode};

fn env() -> Env {
    Env::new(
        Rc::new(Plain),
        Rc::new(|s| ddi_library::search::Fold::nfkd(&ddi_library::search::Nfkd, s)),
    )
}

fn harness() -> Harness {
    Harness::new(DESIGN, [1280.0, 720.0], 1.0, TextMode::Own)
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
            digest: Some(ddi_chart::ChartDigest(0xabc0)),
            ..ChartEntry::default()
        }],
        ..SongEntry::default()
    }
}

/// A practice room of `stages` stages against `cpus` CPUs, its Attacks
/// rule turned off (the standings as survival has them).
fn practice(stages: u16, cpus: usize) -> SharedRoom {
    practice_with(stages, cpus, false)
}

/// A practice room of `stages` stages against `cpus` CPUs, with or
/// without attacks.
fn practice_with(stages: u16, cpus: usize, attacks: bool) -> SharedRoom {
    let room = RoomClient::shared(0, 1);
    room.borrow_mut().set_rules_seen(true);
    let mut rules = room.borrow().rules().clone();
    rules.set_attacks(attacks);
    rules.stages = stages;
    rules.seats = rules.seats.max(cpus as u16 + 1);
    let first = rules.cpu.roster[0].clone();
    rules.cpu.roster.truncate(cpus);
    while rules.cpu.roster.len() < cpus {
        let n = rules.cpu.roster.len() as u64;
        rules.cpu.roster.push(OpponentSpec {
            name: Some(format!("Cpu {n}")),
            seed: 100 + n,
            level: (n % 10) as u8,
            ..first.clone()
        });
    }
    room.borrow_mut().save_rules(rules).unwrap();
    room
}

/// How a competitor of a test's stage went.
#[derive(Clone)]
struct Went {
    finish: Finish,
    detail: RunDetail,
}

fn went(percent: i32, grade: &str, life: f64, combo: u32, tiers: [u32; 7]) -> Went {
    Went {
        finish: Finish::Survived,
        detail: RunDetail {
            percent: Some(percent),
            grade: grade.into(),
            life_ppm: (life * 1e6) as u32,
            max_combo: combo,
            tiers: tiers.to_vec(),
            ..RunDetail::default()
        },
    }
}

fn out(mut w: Went, at_ms: u64) -> Went {
    w.finish = Finish::Eliminated;
    w.detail.life_ppm = 0;
    w.detail.out_at_ms = Some(at_ms);
    w
}

/// The room's next stage picked, started and reported as a stage runner
/// does: `groups` are the ranking's groups, best first, each the indexes
/// of its starters (0 the local player, then the CPUs as seated), with
/// how each went.
fn play(room: &SharedRoom, groups: &[&[usize]], how: &dyn Fn(usize) -> Went) -> StageOrder {
    let mut r = room.borrow_mut();
    let s = song();
    r.pick(&s, &s.charts[0]).unwrap();
    for job in r.take_load_jobs() {
        r.finish_load(&job, Ok(()));
    }
    r.set_ready(true).unwrap();
    let now = r.now_ms() + 3_000;
    r.tick(now);
    let order = r.take_stage_order().expect("the stage's order");
    let id = |i: usize| match i {
        0 => order.me,
        i => order.cpus[i - 1].participant,
    };
    let ranking: Vec<Vec<Finisher>> = groups
        .iter()
        .map(|g| {
            g.iter()
                .map(|&i| Finisher {
                    participant: id(i),
                    finish: how(i).finish,
                })
                .collect()
        })
        .collect();
    let details: Vec<(ParticipantId, RunDetail)> = (0..=order.cpus.len())
        .map(|i| (id(i), how(i).detail))
        .collect();
    r.report_played(order.stage, ddi_room::Run::Forfeit, ranking, false, details)
        .unwrap();
    order
}

/// A stage's results over `room`.
fn results(env: &Env, room: &SharedRoom, order: &StageOrder) -> ResultsScreen {
    let mut data = sample::results();
    data.kind = ResultsKind::Stage;
    ResultsScreen::new(env.clone(), data, ResultsActions::default())
        .with_room(room.clone(), order.stage)
}

/// The actor named `part` inside the one named `owner`.
fn inside<'a>(h: &'a Harness, owner: &str, part: &str) -> Option<&'a Actor> {
    let owner = h.ui.actor(owner)?;
    let path = hit::find(owner, part)?;
    hit::actor_at(owner, &path)
}

/// What the text `part` of `owner` says ("" where there is none).
fn said(h: &Harness, owner: &str, part: &str) -> String {
    match inside(h, owner, part).map(|a| &a.kind) {
        Some(Kind::Text(t)) => t.text().to_string(),
        _ => String::new(),
    }
}

fn there(h: &Harness, name: &str) -> bool {
    h.ui.actor(name).is_some()
}

fn view(env: &Env) -> super::results::StageView {
    env.state
        .borrow()
        .stage
        .clone()
        .expect("the standings said what they show")
}

/// By place: 0 best. Everyone still in, a little worse each.
fn descending(i: usize) -> Went {
    let i = i as u32;
    went(
        9_900 - 300 * i as i32,
        ["AAA", "AA", "A", "B", "C"][(i as usize).min(4)],
        (1.0 - 0.1 * f64::from(i)).max(0.05),
        280u32.saturating_sub(20 * i),
        [
            200u32.saturating_sub(10 * i),
            60,
            10 + 5 * i,
            5,
            2,
            1,
            2 + i,
        ],
    )
}

/// Nine starters after a middle stage: the two tabs (Standings first),
/// every starter on a line in the stage's order with the award, the
/// match's total and place, the score as the ruleset counts it, grade,
/// life, combo and the judgments' bar; the pointed line's counts under
/// the table; Left and Right, the shoulders and taps change the tab,
/// remembered for the next stage's results; the room's song select does
/// not pop the standings up again.
#[test]
fn a_stages_results_have_the_standings_and_the_graphs() {
    let env = env();
    let room = practice(3, 8);
    // The local player third.
    let order = play(
        &room,
        &[&[1], &[2], &[0], &[3], &[4], &[5], &[6], &[7], &[8]],
        &descending,
    );
    let mut h = harness();
    h.push(results(&env, &room, &order));
    h.run(1.0);
    let v = view(&env);
    assert_eq!(
        (v.tab, v.number, v.of, v.confirmed),
        (Tab::Standings, 1, 3, true)
    );
    assert_eq!(
        v.columns,
        [
            "award",
            "total",
            "match",
            "score",
            "grade",
            "life",
            "combo",
            "judgments"
        ]
    );
    assert_eq!(v.rows.len(), 9);
    assert!(v.winners.is_empty() && v.result.is_empty(), "not the last");
    // The tabs, inside the panels' place; the standings where the graphs
    // were.
    assert!(there(&h, "Tab.Standings") && there(&h, "Tab.Graphs"));
    assert!(there(&h, "Panel.Standings") && !there(&h, "Panel.Timing"));
    let panel = h.ui.bounds("Panel.Standings").unwrap();
    let bottom = h.ui.bounds("Bottom").unwrap();
    assert!(
        panel[0] >= 464.0 && panel[2] <= 1248.5 && panel[3] <= bottom[1],
        "{panel:?}"
    );
    // Nine lines without scrolling, in the stage's order.
    for k in 0..9 {
        assert!(there(&h, &format!("Stand{k}")), "line {k}");
        assert_eq!(said(&h, &format!("Stand{k}"), "Place"), (k + 1).to_string());
    }
    assert!(!there(&h, "Stand.More"));
    let first = order.cpus[0].name.clone().unwrap();
    assert_eq!(said(&h, "Stand0", "Name"), first);
    assert!(inside(&h, "Stand0", "Stars").is_some(), "a CPU's stars");
    assert_eq!(
        crate::stars::shown(inside(&h, "Stand0", "Stars").unwrap()),
        crate::stars::fills(order.cpus[0].level + 1)
    );
    // The local player's line: marked, third, its award and total, the
    // percent (the match's ruleset counts no score), the rest.
    assert_eq!(said(&h, "Stand2", "Name"), "You");
    assert!(inside(&h, "Stand2", "Me").is_some() && inside(&h, "Stand0", "Me").is_none());
    assert!(inside(&h, "Stand2", "Stars").is_none());
    assert_eq!(said(&h, "Stand2", "Award"), "+7");
    assert_eq!(said(&h, "Stand2", "Total"), "7");
    assert_eq!(said(&h, "Stand2", "Rank"), "3");
    assert_eq!(said(&h, "Stand2", "Move"), "", "no move on the first stage");
    assert_eq!(said(&h, "Stand2", "Score"), "99.00%");
    assert_eq!(said(&h, "Stand2", "Grade"), "AAA");
    assert_eq!(said(&h, "Stand2", "Life"), "100%");
    assert_eq!(said(&h, "Stand2", "Combo"), "280");
    assert!(inside(&h, "Stand2", "Strip").is_some());
    assert_eq!(v.rows[2].tiers, vec![200, 60, 10, 5, 2, 1, 2]);
    // Short lines: the counts of one line under the table, the local
    // player's until the pointer is over another.
    assert!(inside(&h, "Stand2", "Counts").is_none());
    assert!(there(&h, "Stand.Detail"));
    let texts = h.texts();
    assert!(texts.iter().any(|t| t == "200"), "{texts:?}");
    let at = h.centre("Stand5");
    h.hover(at);
    h.run(0.2);
    let texts = h.texts();
    assert!(texts.iter().any(|t| t == "150"), "the sixth's: {texts:?}");
    // The footer says how to change the tab.
    assert!(texts.iter().any(|t| t.ends_with("Change tab")), "{texts:?}");
    // Seen: the room's song select has nothing to pop up.
    assert_eq!(room.borrow().unseen_results(), None);
    // Right, Left; the shoulders; a tap.
    h.key("ArrowRight", "ArrowRight");
    h.run(0.2);
    assert_eq!(view(&env).tab, Tab::Graphs);
    assert!(there(&h, "Panel.Timing") && !there(&h, "Panel.Standings"));
    assert!(there(&h, "Panel.Life") && there(&h, "Panel.Offsets"));
    h.key("ArrowLeft", "ArrowLeft");
    h.run(0.2);
    assert_eq!(view(&env).tab, Tab::Standings);
    for (action, tab) in [
        (MenuAction::Next, Tab::Graphs),
        (MenuAction::Previous, Tab::Standings),
    ] {
        h.pad(action, true);
        h.pad(action, false);
        h.run(0.2);
        assert_eq!(view(&env).tab, tab, "{action:?}");
    }
    h.tap(PointerKind::Mouse, "Tab.Graphs");
    h.run(0.2);
    assert_eq!(view(&env).tab, Tab::Graphs);
    // Remembered for the session: the next stage's results open on the
    // graphs; the last stage's on the standings, whatever was chosen.
    let order = play(
        &room,
        &[&[0], &[1], &[2], &[3], &[4], &[5], &[6], &[7], &[8]],
        &descending,
    );
    let mut h = harness();
    h.push(results(&env, &room, &order));
    h.run(1.0);
    assert_eq!(view(&env).tab, Tab::Graphs);
    // The second stage moved the places: the local player up to first.
    h.tap(PointerKind::Mouse, "Tab.Standings");
    h.run(0.2);
    assert_eq!(said(&h, "Stand0", "Name"), "You");
    assert_eq!(said(&h, "Stand0", "Award"), "+12");
    assert_eq!(said(&h, "Stand0", "Total"), "19");
    assert_eq!(
        (said(&h, "Stand0", "Rank"), said(&h, "Stand0", "Move")),
        ("2".into(), "▲".into())
    );
    assert_eq!(view(&env).rows[0].moved.as_deref(), Some("up"));
    assert_eq!(view(&env).rows[2].moved.as_deref(), Some("down"));
    h.tap(PointerKind::Mouse, "Tab.Graphs");
    h.run(0.2);
    let order = play(
        &room,
        &[&[0], &[1], &[2], &[3], &[4], &[5], &[6], &[7], &[8]],
        &descending,
    );
    let mut h = harness();
    h.push(results(&env, &room, &order));
    h.run(1.0);
    assert_eq!(view(&env).tab, Tab::Standings, "the last stage's");
    // The match's result on top: the winner, everyone's place and total.
    let v = view(&env);
    assert_eq!(v.winners, ["You"]);
    assert_eq!(v.result.len(), 9);
    assert_eq!(v.result[0], ("You".to_string(), 1, "31".to_string()));
    assert_eq!(said(&h, "Panel.Standings", "Final.Winners"), "Winner: You");
    assert_eq!(said(&h, "Panel.Standings", "Final0.Name"), "You");
    assert_eq!(said(&h, "Panel.Standings", "Final0.Total"), "31");
    assert!(there(&h, "Final8.Name"));
    // Still inside its panel, every line of the stage under it.
    let panel = h.ui.bounds("Panel.Standings").unwrap();
    for k in 0..9 {
        let line = h.ui.bounds(&format!("Stand{k}")).unwrap();
        assert!(line[3] <= panel[3] + 0.5, "line {k}: {line:?} in {panel:?}");
    }
    assert_eq!(room.borrow().unseen_results(), None);
    crate::screens::results::forget_tab();
}

/// Two and four starters: taller lines with the counts on a second line;
/// a tie shares its place and points; one whose life ran out says when;
/// one who left says so and earns nothing.
#[test]
fn the_table_says_ties_those_out_and_those_who_left() {
    crate::screens::results::forget_tab();
    let env = env();
    let room = practice(2, 3);
    let how = |i: usize| match i {
        0 => went(9_512, "AA", 0.64, 120, [100, 40, 9, 0, 0, 0, 1]),
        1 => went(9_512, "AA", 0.64, 118, [99, 41, 9, 0, 0, 0, 1]),
        2 => out(went(4_100, "F", 0.0, 30, [20, 20, 20, 5, 5, 0, 30]), 62_400),
        _ => Went {
            finish: Finish::Forfeit,
            ..went(1_000, "F", 0.3, 8, [5, 2, 0, 0, 0, 0, 0])
        },
    };
    let order = play(&room, &[&[0, 1], &[2], &[3]], &how);
    let mut h = harness();
    h.push(results(&env, &room, &order));
    h.run(1.0);
    let v = view(&env);
    assert_eq!(v.rows.len(), 4);
    // The tie: both first, the two places' points shared.
    assert_eq!((v.rows[0].place, v.rows[1].place), (Some(1), Some(1)));
    assert_eq!(v.rows[0].award, v.rows[1].award);
    assert_eq!(said(&h, "Stand0", "Place"), "1");
    assert_eq!(said(&h, "Stand1", "Place"), "1");
    assert_eq!(said(&h, "Stand2", "Place"), "3");
    assert_eq!(said(&h, "Stand0", "Score"), "95.12%");
    // Out, and when; left, and nothing for it.
    assert_eq!(said(&h, "Stand2", "Life"), "Out 1:02");
    assert_eq!(said(&h, "Stand3", "Life"), "Left");
    assert_eq!(said(&h, "Stand3", "Award"), "+0");
    assert_eq!(said(&h, "Stand0", "Life"), "64%");
    // Room for the counts under each line; none under the table.
    assert!(inside(&h, "Stand0", "Counts").is_some());
    assert!(!there(&h, "Stand.Detail"));
    let texts = h.texts();
    for count in ["100", "40", "9", "99", "41", "30"] {
        assert!(texts.iter().any(|t| t == count), "{count}: {texts:?}");
    }
    // Two starters: the same, taller.
    let room = practice(1, 1);
    let order = play(&room, &[&[1], &[0]], &descending);
    let mut h = harness();
    h.push(results(&env, &room, &order));
    h.run(1.0);
    let v = view(&env);
    assert_eq!(v.rows.len(), 2);
    assert_eq!(said(&h, "Stand1", "Name"), "You");
    assert!(inside(&h, "Stand1", "Counts").is_some());
    // One stage: its results are the match's.
    assert_eq!(v.winners.len(), 1);
    assert_eq!(v.winners[0], order.cpus[0].name.clone().unwrap());
    // A match tied at the top: both named.
    let room = practice(1, 2);
    let order = play(&room, &[&[0, 2], &[1]], &descending);
    let mut h = harness();
    h.push(results(&env, &room, &order));
    h.run(1.0);
    let v = view(&env);
    assert_eq!(v.winners.len(), 2);
    let line = said(&h, "Panel.Standings", "Final.Winners");
    assert!(line.starts_with("Winners: You, "), "{line}");
}

/// Past what fits (a development practice's field) the lines scroll with
/// Up and Down, the rest counted.
#[test]
fn a_long_table_scrolls() {
    crate::screens::results::forget_tab();
    let env = env();
    let room = Rc::new(RefCell::new(RoomClient::development(0, 1, 20)));
    room.borrow_mut().set_rules_seen(true);
    let groups: Vec<Vec<usize>> = (0..21).map(|i| vec![i]).collect();
    let groups: Vec<&[usize]> = groups.iter().map(Vec::as_slice).collect();
    let order = play(&room, &groups, &descending);
    let mut h = harness();
    h.push(results(&env, &room, &order));
    h.run(1.0);
    assert_eq!(view(&env).rows.len(), 21);
    assert!(there(&h, "Stand0") && !there(&h, "Stand20"));
    assert!(there(&h, "Stand.More"));
    let panel = h.ui.bounds("Panel.Standings").unwrap();
    let more = h.ui.bounds("Stand.More").unwrap();
    assert!(more[3] <= panel[3], "{more:?} in {panel:?}");
    for _ in 0..21 {
        h.key("ArrowDown", "ArrowDown");
        h.run(0.05);
    }
    // The player's own line (first here) stays in view, at the top.
    assert!(there(&h, "Stand20") && !there(&h, "Stand1"));
    assert!(there(&h, "Stand0"), "the player's line kept");
    assert_eq!(said(&h, "Stand0", "Name"), "You");
}

/// An online stage: the lines fill in as the reports come. Before the
/// room confirms, the starters are as seated, one still playing has a
/// quiet line and the panel says it waits; the confirmed order, places
/// and awards come with the last report, with what each said of their
/// play (one said nothing: what the run itself tells).
#[test]
fn an_online_stages_lines_fill_in_as_the_reports_come() {
    use crate::state::online::tests::room::{client, person, song, tick, upload};
    use ddi_room::fake::FakeServer;
    use ddi_room::{Identity, Intent, ManualClock, RoomPolicy, Run};
    crate::screens::results::forget_tab();
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
    step(400);
    // One stage: picked, shared, prepared, both ready, started.
    let mut rules = alice.borrow().rules().clone();
    rules.stages = 1;
    alice.borrow_mut().save_rules(rules).unwrap();
    step(400);
    let s = song();
    alice.borrow_mut().pick(&s, &s.charts[0]).unwrap();
    step(400);
    upload(&server, &mut alice.borrow_mut(), "abc12345");
    step(400);
    crate::state::online::tests::room::prepare(&mut alice.borrow_mut());
    crate::state::online::tests::room::prepare(&mut bob.borrow_mut());
    alice.borrow_mut().set_ready(true).unwrap();
    bob.borrow_mut().set_ready(true).unwrap();
    step(400);
    step(4_000);
    let order = alice.borrow_mut().take_stage_order().expect("alice's");
    let stage = order.stage;
    assert!(alice.borrow().stage_started(stage));
    // Alice's play is over: her report says how she performed; her
    // results show while Bob still plays.
    let detail = went(9_650, "AA", 0.8, 200, [150, 40, 5, 0, 0, 0, 3]).detail;
    alice
        .borrow_mut()
        .report_run(
            stage,
            Run::Live {
                life: 0.8,
                accuracy: 0.965,
                absolute: None,
                score: None,
                steps: 0,
            },
            Some(detail),
        )
        .unwrap();
    let env = env();
    let mut h = harness();
    h.push(results(&env, &alice, &order));
    step(400);
    h.run(1.0);
    let v = view(&env);
    assert!(!v.confirmed);
    assert_eq!(v.rows.len(), 2);
    assert_eq!(said(&h, "Stand0", "Name"), "You");
    assert_eq!(
        said(&h, "Stand0", "Place"),
        "",
        "no place before the room's"
    );
    assert_eq!(said(&h, "Stand0", "Award"), "");
    assert_eq!(said(&h, "Stand0", "Score"), "96.50%");
    assert_eq!(said(&h, "Stand1", "Name"), "bob");
    assert_eq!(said(&h, "Stand1", "Waiting"), "Still playing…");
    assert_eq!(said(&h, "Stand1", "Score"), "");
    assert!(
        room_unseen(&alice).is_none(),
        "nothing confirmed to see yet"
    );
    // Bob's report (an older build's: his run alone) settles the stage:
    // the screen, which keeps the room's messages coming, shows it.
    bob.borrow_mut()
        .report_run(stage, Run::Out { at: 45.0 }, None)
        .unwrap();
    step(400);
    h.run(0.5);
    let v = view(&env);
    assert!(v.confirmed);
    assert_eq!(said(&h, "Stand0", "Place"), "1");
    assert_eq!(said(&h, "Stand1", "Place"), "2");
    assert_eq!(said(&h, "Stand1", "Waiting"), "");
    assert_eq!(said(&h, "Stand1", "Life"), "Out 0:45");
    assert_eq!(said(&h, "Stand1", "Grade"), "", "he said nothing more");
    assert_eq!(v.winners, ["You"], "one stage: the match's result too");
    assert_eq!(room_unseen(&alice), None, "seen on the results");
    // Bob, who never opened the standings (the graphs): his song select
    // still pops them up.
    assert!(room_unseen(&bob).is_some());
}

/// With attacks (a practice room's default), the standings have three
/// small columns at the end (attacks launched, attacks received, shocks
/// stepped on) in the combo's place, a number a line, nine lines fitting
/// at 1280×720; and under the lines the local player's own battle in
/// short: whom their attacks went to, who attacked them, the shocks they
/// stepped on, a final attack's draw. Without attacks, none of it.
#[test]
fn with_attacks_the_lines_count_them_and_the_player_s_battle_is_summed_up() {
    use crate::state::room::AttackSummary;
    crate::screens::results::forget_tab();
    let env = env();
    let room = practice_with(2, 8, true);
    let how = |i: usize| {
        let mut w = descending(i);
        w.detail.attacks = Some(8 - i as u32);
        w.detail.received = Some(i as u32);
        w.detail.shocks = Some(1 + 7 * i as u32);
        w
    };
    let groups: Vec<Vec<usize>> = (0..9).map(|i| vec![i]).collect();
    let groups: Vec<&[usize]> = groups.iter().map(Vec::as_slice).collect();
    let order = play(&room, &groups, &how);
    assert!(order.battle.attacks());
    // The player's own attacks, as the stage's runner sums them up.
    let cpu = |k: usize| order.cpus[k].participant;
    room.borrow_mut().set_stage_summary(
        order.stage,
        Some(AttackSummary {
            sent: vec![(cpu(0), 2), (cpu(3), 1)],
            taken: vec![(cpu(1), 1)],
            shocks: 3,
            drawn_by_final_attack: true,
        }),
    );
    let mut h = harness();
    h.push(results(&env, &room, &order));
    h.run(1.0);
    let v = view(&env);
    assert_eq!(
        v.columns,
        [
            "award",
            "total",
            "match",
            "score",
            "grade",
            "life",
            "judgments",
            "sent",
            "taken",
            "shocks"
        ]
    );
    assert_eq!(v.rows[0].attacks, Some((8, 0, 1)));
    assert_eq!(v.rows[8].attacks, Some((0, 8, 57)));
    // Nine lines, each with its three numbers under their headings.
    for k in 0..9 {
        assert!(there(&h, &format!("Stand{k}")), "line {k}");
    }
    assert_eq!(said(&h, "Stand0", "Sent"), "8");
    assert_eq!(said(&h, "Stand0", "Taken"), "0");
    assert_eq!(said(&h, "Stand0", "Shocks"), "1");
    assert_eq!(said(&h, "Stand8", "Shocks"), "57");
    for (head, word) in [("sent", "Sent"), ("taken", "Taken"), ("shocks", "Shocks")] {
        let a = h.actor(&format!("Head.{head}"));
        let Kind::Text(t) = &a.kind else {
            panic!("a heading");
        };
        assert_eq!(t.text(), word);
    }
    // The names keep room beside ten columns.
    let name = h.ui.bounds("Stand0").unwrap();
    let head = h.ui.bounds("Head.award").unwrap();
    assert!(head[0] - name[0] > 180.0, "{name:?} {head:?}");
    // The summary, under the lines, inside the panel.
    let names: Vec<String> = [0, 3, 1]
        .iter()
        .map(|k| order.cpus[*k].name.clone().unwrap())
        .collect();
    assert_eq!(
        v.summary,
        [
            format!("Attacks sent: {} ×2, {}", names[0], names[1]),
            format!("Attacks taken: {}", names[2]),
            "Shocks stepped on: 3".to_string(),
            "Drawn: final attack".to_string(),
        ]
    );
    let panel = h.ui.bounds("Panel.Standings").unwrap();
    let texts = h.texts();
    assert!(
        texts.iter().any(|t| t.starts_with("Attacks sent: ")),
        "{texts:?}"
    );
    let mut k = 0;
    while let Some(b) = h.ui.bounds(&format!("Stand.Battle{k}")) {
        assert!(b[1] >= panel[1] && b[3] <= panel[3] + 0.5, "line {k}: {b:?} {panel:?}");
        assert!(b[2] <= panel[2] + 0.5, "line {k}: {b:?} {panel:?}");
        k += 1;
    }
    assert!((1..=4).contains(&k), "{k} summary lines");
    let last = h.ui.bounds("Stand8").unwrap();
    assert!(last[3] <= panel[3] + 0.5, "the ninth line in the panel");
    // None sent, none taken: said so.
    room.borrow_mut().set_stage_summary(
        order.stage,
        Some(AttackSummary::default()),
    );
    h.run(0.5);
    assert_eq!(
        view(&env).summary,
        ["No attacks sent", "No attacks taken", "Shocks stepped on: 0"]
    );
    // Without attacks nothing is said of them, whatever a report says.
    let room = practice(2, 1);
    let order = play(&room, &[&[0], &[1]], &how);
    assert!(!order.battle.attacks());
    room.borrow_mut()
        .set_stage_summary(order.stage, Some(AttackSummary::default()));
    let mut h = harness();
    h.push(results(&env, &room, &order));
    h.run(1.0);
    let v = view(&env);
    assert_eq!(v.rows[0].attacks, None);
    assert!(v.summary.is_empty());
    assert!(v.columns.contains(&"combo".to_string()) && !v.columns.contains(&"sent".to_string()));
    assert!(!h.texts().iter().any(|t| t.starts_with("Attacks")));
}

fn room_unseen(room: &SharedRoom) -> Option<Option<u16>> {
    room.borrow().unseen_results()
}

/// A phone: the tabs over the scrolling column, the table with fewer
/// columns (the judgments, the combo and life go first), its text never
/// smaller; the graphs one under another on the other tab.
#[test]
fn a_phones_table_drops_columns() {
    crate::screens::results::forget_tab();
    let env = env();
    let room = practice(2, 3);
    let order = play(&room, &[&[1], &[0], &[2], &[3]], &descending);
    for (size, columns) in [
        ([844.0, 390.0], vec!["award", "total", "score", "grade"]),
        ([390.0, 844.0], vec!["award", "total", "score", "grade"]),
    ] {
        let mut h = Harness::new(super::COMPACT, size, 1.0, TextMode::Own);
        h.push(results(&env, &room, &order));
        h.run(1.0);
        let v = view(&env);
        assert_eq!(v.columns, columns, "{size:?}");
        assert!(there(&h, "Tab.Standings") && there(&h, "Panel.Standings"));
        for k in 0..4 {
            assert!(there(&h, &format!("Stand{k}")), "{size:?} line {k}");
        }
        assert_eq!(said(&h, "Stand1", "Name"), "You");
        assert_eq!(said(&h, "Stand1", "Score"), "99.00%");
        assert_eq!(said(&h, "Stand1", "Life"), "");
        // Inside the screen's width.
        let panel = h.ui.bounds("Panel.Standings").unwrap();
        let [x0, _] = h.css([panel[0], panel[1]]);
        let [x1, _] = h.css([panel[2], panel[3]]);
        assert!(x0 >= 0.0 && x1 <= size[0] + 0.5, "{size:?}: {x0} {x1}");
        h.tap(PointerKind::Touch, "Tab.Graphs");
        h.run(0.3);
        assert_eq!(view(&env).tab, Tab::Graphs);
        assert!(there(&h, "Panel.Timing") && !there(&h, "Panel.Standings"));
        h.tap(PointerKind::Touch, "Tab.Standings");
        h.run(0.3);
    }
    crate::screens::results::forget_tab();
}

/// A stage the player watched (`watch-v0`): its results have no play of
/// their own. The left side names the stage's winner where the grade and
/// the judgments would be and says they watched; the standings list the
/// competitors alone; there is no Graphs tab, and Right changes nothing.
#[test]
fn a_watched_stages_results_name_the_winner_and_have_no_graphs() {
    super::results::forget_tab();
    let env = env();
    let room = practice(2, 3);
    room.borrow_mut()
        .set_intent(ddi_room::Intent::Watch)
        .unwrap();
    // The stage as a watched play reports it: the CPUs alone.
    let order = {
        let mut r = room.borrow_mut();
        let s = song();
        r.pick(&s, &s.charts[0]).unwrap();
        let now = r.now_ms() + 3_000;
        r.tick(now);
        let order = r.take_stage_order().expect("the stage's order");
        assert!(order.watching);
        let ranking: Vec<Vec<Finisher>> = [1, 0, 2]
            .iter()
            .map(|&i: &usize| {
                vec![Finisher {
                    participant: order.cpus[i].participant,
                    finish: Finish::Survived,
                }]
            })
            .collect();
        let details: Vec<(ParticipantId, RunDetail)> = order
            .cpus
            .iter()
            .enumerate()
            .map(|(i, c)| (c.participant, descending(i).detail))
            .collect();
        r.report_played(order.stage, ddi_room::Run::Forfeit, ranking, true, details)
            .unwrap();
        order
    };
    let winner = order.cpus[1].name.clone().unwrap();
    let mut h = harness();
    h.push(results(&env, &room, &order).watched());
    h.run(0.5);
    let v = view(&env);
    assert!(v.watched && v.confirmed, "{v:?}");
    assert_eq!(v.tab, Tab::Standings);
    assert_eq!(v.stage_winners, std::slice::from_ref(&winner));
    // The competitors alone: no line of the viewer's.
    assert_eq!(v.rows.len(), 3);
    assert!(v.rows.iter().all(|r| !r.me));
    assert_eq!(v.rows[0].name, winner);
    assert!(v.summary.is_empty());
    // The left side: the winner under its heading, and that they watched.
    let texts = h.texts();
    for want in ["Stage winner", winner.as_str(), "You watched this stage."] {
        assert!(texts.iter().any(|t| t == want), "{want}: {texts:?}");
    }
    assert!(there(&h, "Watched.Winner0.Stars"), "a CPU's stars");
    // No grade, no judgments of their own.
    assert!(!there(&h, "ScoreBox") && !there(&h, "Judgement0"));
    // One tab: the standings; Right and the shoulders change nothing.
    assert!(there(&h, "Tab.Standings") && !there(&h, "Tab.Graphs"));
    h.key("ArrowRight", "ArrowRight");
    h.run(0.3);
    h.pad(MenuAction::Next, true);
    h.pad(MenuAction::Next, false);
    h.run(0.3);
    assert_eq!(view(&env).tab, Tab::Standings);
    assert!(!there(&h, "Panel.Life"));
    // A phone's: the same, in its one column.
    let mut phone = Harness::new(super::COMPACT, [390.0, 844.0], 1.0, TextMode::Own);
    phone.push(results(&env, &room, &order).watched());
    phone.run(0.5);
    let texts = phone.texts();
    assert!(texts.iter().any(|t| t == "Stage winner"), "{texts:?}");
    assert!(!there(&phone, "Tab.Graphs"));
    // The match goes on: the next stage is the room's to pick.
    assert!(matches!(
        room.borrow().state().phase,
        ddi_room::Phase::Intermission { .. }
    ));
    assert!(room.borrow().mark_of(order.stage).is_none());
}
