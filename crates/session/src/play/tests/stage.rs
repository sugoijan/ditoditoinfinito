//! Multiplayer stages against CPU opponents (`crate::battle`;
//! `docs/plans/multiplayer-prototype-progress.md`, M3): plays judged by
//! the engine on the fake clock, their outcomes reported into a practice
//! room, a whole match from its first stage to the next match's lobby.

use super::*;
use crate::battle::StageSetup;
use ddi_room::{Finish, MatchRules, Phase, Points};
use ddi_ui::screens::data::{ChartEntry, Difficulty, SongEntry};
use ddi_ui::state::room::{RoomClient, StageOrder, check_chart};

const CHART: usize = 1;

/// A practice room of `stages` stages on beatucada's chart, `cpus` CPUs
/// at `level`, every stage's song chosen and prepared (its chart read
/// and the CPUs' plan checked, as a shell does).
fn room(song: &Song, stages: u16, cpus: u16, level: u8) -> RoomClient {
    room_seeded(song, stages, cpus, level, 7)
}

/// [`room`] whose CPUs' seeds come from `seed`: a fixed count of CPUs at
/// one level, each seat its own seed from the room's.
fn room_seeded(_song: &Song, stages: u16, cpus: u16, level: u8, seed: u64) -> RoomClient {
    let mut room = RoomClient::new(0, seed);
    let mut rules = MatchRules {
        stages,
        seats: cpus + 1,
        ..room.rules().clone()
    };
    rules.cpu.mode = ddi_room::CpuMode::Fixed { count: cpus };
    rules.cpu.roster.clear();
    rules.cpu.opponent.level = level;
    // Survival alone (the Attacks rule, On by default in practice,
    // turned off): what these stages' tests are about; `attacks_room`
    // turns it on again.
    rules.set_attacks(false);
    room.save_rules(rules).unwrap();
    room
}

/// Beatucada's chart `index` as the library lists it.
fn entry_at(song: &Song, index: usize) -> SongEntry {
    SongEntry {
        id: "beatucada".into(),
        title: "Beatucada".into(),
        charts: vec![ChartEntry {
            style: "dance-single".into(),
            difficulty: Difficulty::Hard,
            index,
            digest: Some(song.chart_digest(index)),
            ..ChartEntry::default()
        }],
        ..SongEntry::default()
    }
}

/// The pick (its song prepared: its chart read and the CPUs' plan
/// checked, as a shell does), Ready, the countdown: the stage the room
/// hands over.
fn begin(song: &Song, room: &mut RoomClient, now: &mut u64) -> StageOrder {
    begin_at(song, room, now, CHART)
}

/// [`begin`] on chart `index`.
fn begin_at(song: &Song, room: &mut RoomClient, now: &mut u64, index: usize) -> StageOrder {
    let e = entry_at(song, index);
    room.pick(&e, &e.charts[0]).unwrap();
    let order = room.take_stage_order().expect("its prompt");
    for job in room.take_load_jobs() {
        let checked = check_chart(song, job.chart, job.digest, &mut |e| panic!("{e}"));
        room.finish_load(&job, checked);
    }
    room.set_ready(true).unwrap();
    *now += 3_000;
    room.tick(*now);
    assert!(room.stage_started(order.stage), "the stage is on");
    order
}

/// The stage's ruleset as [`play`] gives it: the match's (D0: DDI with
/// its difficulty and Boos). Where a test sets StepMania options (a
/// battery of one life, so that CPUs go out when the test needs them to:
/// DDI has no such option), a TESTS-ONLY ruleset in its place
/// (`SessionConfig::stage_rules`): ITG FA+ under those options, its fail
/// immediate, as these tests ran before rooms played DDI. What they test
/// is the stage's flow, not the ruleset.
fn rules_of(settings: &Settings, order: &StageOrder, song: &Song) -> ddi_engine::rules::Ruleset {
    if settings.stepmania.is_default() {
        return crate::battle::stage_ruleset(&order.rules);
    }
    let mut r = ddi_engine::rules::presets::stepmania(
        "itg-fa+",
        ddi_engine::rules::presets::HoldStyle::ByGame,
        &song.charts[order.chart.index],
    )
    .with_stepmania(&settings.stepmania);
    r.fail = ddi_engine::FailPolicy::Immediate;
    r
}

/// The stage's session on the fake clock (`auto`: autoplayed), what it
/// ended with and its outcome.
fn play(
    song: &Song,
    settings: &Settings,
    order: &StageOrder,
    auto: bool,
    keys: &[(f64, &str, bool)],
) -> (
    PlaySession<FakeAudio, FakeInputs>,
    SessionEvent,
    crate::battle::StageOutcome,
) {
    let clock: Clock = Rc::new(RefCell::new(0.0));
    let mut inputs = FakeInputs::new(&clock);
    for &(at, code, pressed) in keys {
        inputs.key(at, code, pressed);
    }
    let mut config = SessionConfig::from_settings(settings, auto, None);
    config.opponents = Some(StageSetup::of(order));
    if !settings.stepmania.is_default() {
        config.stage_rules = Some(rules_of(settings, order, song));
    }
    let mut s = PlaySession::start(
        song,
        order.chart.index,
        settings,
        config,
        FakeAudio::new(&clock, 0.0),
        110.0,
        inputs,
    )
    .unwrap();
    assert_eq!(s.opponents().len(), order.cpus.len());
    let event = run(&mut s, &clock, 60.0, 300.0).expect("the stage ends");
    let outcome = s.take_stage_outcome().expect("its outcome");
    assert!(s.take_stage_outcome().is_none(), "once");
    (s, event, outcome)
}

fn ids(o: &crate::battle::StageOutcome) -> Vec<Vec<(u64, Finish)>> {
    o.ranking
        .iter()
        .map(|g| g.iter().map(|f| (f.participant.0, f.finish)).collect())
        .collect()
}

#[test]
fn a_whole_match_of_judged_stages_reaches_its_results_and_the_next_lobby() {
    let song = bundled("beatucada");
    let settings = Settings::default();
    let mut room = room(&song, 2, 3, 2);
    let mut now = 0;
    let mut totals = Vec::new();
    for stage in 1..=2 {
        let order = begin(&song, &mut room, &mut now);
        assert_eq!(order.number, stage);
        let (s, event, outcome) = play(&song, &settings, &order, true, &[]);
        let SessionEvent::Finished { results, .. } = event else {
            panic!("an autoplayed stage plays to its end");
        };
        assert!(!results.failed);
        // The CPUs were judged by the engine: each as it plays alone,
        // to the same tally and life.
        for (view, seat) in s.opponents().iter().zip(&order.cpus) {
            let mut alone = ddi_engine::cpu::Opponent::new(
                &song,
                CHART,
                rules_of(&settings, &order, &song),
                usize::from(seat.level),
                seat.seed,
            )
            .unwrap();
            alone.start(0.0);
            alone.finish();
            assert_eq!(view.life, alone.life());
            assert_eq!(view.out, alone.out_at().is_some());
            assert_eq!(view.score, crate::battle::accuracy(&alone.score()));
            assert!(view.name.starts_with("CPU"));
        }
        // Everyone ranked; the autoplayed player's full life comes first
        // or ties.
        let ranked: Vec<_> = ids(&outcome).into_iter().flatten().collect();
        assert_eq!(ranked.len(), 4);
        assert!(ids(&outcome)[0].iter().any(|(p, _)| *p == order.me.0));
        assert!(!outcome.confirmed_early);
        room.report_stage(outcome.stage, outcome.ranking, outcome.confirmed_early)
            .unwrap();
        let mark = room.mark_of(order.stage).unwrap();
        totals.push(mark.placement.points);
        if stage == 1 {
            assert!(matches!(room.state().phase, Phase::Intermission { .. }));
            assert!(!room.ready(), "the next stage asks for Ready again");
        }
    }
    assert_eq!(room.state().phase, Phase::MatchResults);
    let standings = room.state().standings().unwrap();
    let mine = standings
        .iter()
        .find(|s| s.participant == room.me())
        .unwrap();
    assert_eq!(mine.total, totals.into_iter().sum::<Points>());
    room.open_next_match().unwrap();
    assert_eq!(room.state().phase, Phase::Lobby);
    assert!(!room.ready());
    assert_eq!(room.state().history.len(), 1);
}

#[test]
fn a_quit_is_a_forfeit_and_a_stage_never_restarts() {
    let song = bundled("beatucada");
    let settings = Settings::default();
    let mut room = room(&song, 1, 3, 2);
    let mut now = 0;
    let order = begin(&song, &mut room, &mut now);
    // R twice: no restart in a stage. Escape twice: a quit.
    let keys = [
        (5.0, "KeyR", true),
        (5.05, "KeyR", false),
        (5.2, "KeyR", true),
        (5.25, "KeyR", false),
        (8.0, "Escape", true),
        (8.05, "Escape", false),
        (8.2, "Escape", true),
    ];
    let (_, event, outcome) = play(&song, &settings, &order, true, &keys);
    assert!(matches!(event, SessionEvent::Aborted { .. }));
    let ranking = ids(&outcome);
    assert_eq!(
        ranking.last().unwrap(),
        &vec![(order.me.0, Finish::Forfeit)]
    );
    assert_eq!(ranking.iter().flatten().count(), 4);
    room.report_stage(outcome.stage, outcome.ranking, false)
        .unwrap();
    let mark = room.mark_of(order.stage).unwrap();
    assert_eq!(mark.placement.points, Points::ZERO);
    assert_eq!(room.state().phase, Phase::MatchResults);
}

#[test]
fn out_of_life_the_player_is_out_and_a_last_cpu_wins_early() {
    let song = bundled("beatucada");
    let settings = Settings::default();
    let mut room = room(&song, 1, 1, 4);
    let mut now = 0;
    let order = begin(&song, &mut room, &mut now);
    // Nobody presses: the gauge empties, the play fails.
    let (s, event, outcome) = play(&song, &settings, &order, false, &[]);
    let SessionEvent::Finished { results, .. } = event else {
        panic!("a failed play's results");
    };
    assert!(results.failed);
    assert!(s.battle().unwrap().decided());
    assert!(!s.battle().unwrap().won());
    assert!(outcome.confirmed_early);
    let ranking = ids(&outcome);
    assert_eq!(
        ranking[0],
        vec![(order.cpus[0].participant.0, Finish::Survived)]
    );
    assert_eq!(ranking[1], vec![(order.me.0, Finish::Eliminated)]);
    room.report_stage(outcome.stage, outcome.ranking, true)
        .unwrap();
    assert_eq!(room.mark_of(order.stage).unwrap().placement.place, 2);
}

#[test]
fn the_winner_finishes_the_song_whatever_the_gauge_does_after() {
    let song = bundled("beatucada");
    // One life each: the CPU's first slip puts it out.
    let mut settings = Settings::default();
    settings.stepmania.life_type = ddi_engine::rules::LifeType::Battery;
    settings.stepmania.lives = 1;
    // A room whose CPU slips before the end (its seed decides; the
    // first such seed, found as the CPU plays alone).
    let slips = |seed: u64| {
        let mut probe = room_seeded(&song, 1, 1, 0, seed);
        let mut now = 0;
        let order = begin(&song, &mut probe, &mut now);
        let seat = order.cpus[0].clone();
        let rules = rules_of(&settings, &order, &song);
        let mut alone = ddi_engine::cpu::Opponent::new(&song, CHART, rules, 0, seat.seed).unwrap();
        alone.start(0.0);
        alone.finish();
        alone.out_at().is_some()
    };
    let seed = (1..64).find(|&s| slips(s)).expect("a CPU that slips");
    let mut room = room_seeded(&song, 1, 1, 0, seed);
    let mut now = 0;
    let order = begin(&song, &mut room, &mut now);
    let (s, event, outcome) = play(&song, &settings, &order, true, &[]);
    assert!(matches!(event, SessionEvent::Finished { .. }));
    let b = s.battle().unwrap();
    assert!(s.opponents()[0].out, "the CPU slipped once");
    assert!(b.won() && outcome.confirmed_early);
    // Won: the play's fail is off to the song's end.
    assert_eq!(s.player.ruleset().fail, ddi_engine::FailPolicy::Off);
    assert_eq!(
        ids(&outcome),
        vec![
            vec![(order.me.0, Finish::Survived)],
            vec![(order.cpus[0].participant.0, Finish::Eliminated)]
        ]
    );
}

#[test]
fn a_stage_is_never_a_replay_a_test_or_the_cpu_demo() {
    let song = bundled("beatucada");
    let settings = Settings::default();
    let mut room = room(&song, 1, 1, 2);
    let mut now = 0;
    let order = begin(&song, &mut room, &mut now);
    let clock: Clock = Rc::new(RefCell::new(0.0));
    let mut config = SessionConfig::from_settings(&settings, false, None);
    config.opponents = Some(StageSetup::of(&order));
    config.cpu = Some(Vec::new());
    assert!(
        PlaySession::start(
            &song,
            CHART,
            &settings,
            config,
            FakeAudio::new(&clock, 0.0),
            110.0,
            FakeInputs::new(&clock),
        )
        .is_err()
    );
    // Turns and rate are the player's own: a stage plays the chart as is.
    let mut config = SessionConfig::from_settings(&settings, false, None);
    config.options.clock.rate = 1.5;
    config.options.transform.turn = ddi_engine::Turn::Mirror;
    config.opponents = Some(StageSetup::of(&order));
    let (rules, options) = config.player_spec(&song, CHART, &settings).unwrap();
    assert_eq!(options.clock.rate, 1.0);
    assert_eq!(options.transform, Default::default());
    assert_eq!(rules.fail, ddi_engine::FailPolicy::Immediate);
}

/// A voice of `folders` (any line's), each one sound 0.8 s long.
fn voice_of(folders: &[&str]) -> crate::announcer::SharedVoice<f64> {
    let mut v = crate::announcer::Voice::default();
    for f in folders {
        let line = ddi_library::announcer::Line::parse(f).expect(f);
        v.expect([line]);
        v.add(line, 0.8);
    }
    Rc::new(RefCell::new(v))
}

/// [`play`] with the announcer's `voice` (`None`: the announcer off), the
/// outcome reported to `room` and the session told how it went: the
/// stage's session, its outcome and the folders said.
fn play_voiced(
    song: &Song,
    settings: &Settings,
    room: &mut RoomClient,
    order: &StageOrder,
    auto: bool,
    voice: Option<crate::announcer::SharedVoice<f64>>,
) -> (PlaySession<FakeAudio, FakeInputs>, Vec<String>) {
    let clock: Clock = Rc::new(RefCell::new(0.0));
    let mut config = SessionConfig::from_settings(settings, auto, None);
    config.opponents = Some(StageSetup::of(order));
    if !settings.stepmania.is_default() {
        config.stage_rules = Some(rules_of(settings, order, song));
    }
    let mut s = PlaySession::start(
        song,
        order.chart.index,
        settings,
        config,
        FakeAudio::new(&clock, 0.0),
        110.0,
        FakeInputs::new(&clock),
    )
    .unwrap();
    if let Some(v) = voice {
        s.set_voice(v);
    }
    run(&mut s, &clock, 60.0, 300.0).expect("the stage ends");
    let o = s.take_stage_outcome().expect("its outcome");
    room.report_stage(o.stage, o.ranking, o.confirmed_early)
        .unwrap();
    s.stage_reported(room.match_end());
    let said = s
        .announcer_report()
        .spoken
        .iter()
        .map(|(f, _)| f.clone())
        .collect();
    (s, said)
}

/// What the stage told, by event.
fn told(s: &PlaySession<FakeAudio, FakeInputs>) -> Vec<Battle> {
    s.battle_told().iter().map(|t| t.event).collect()
}

use ddi_library::announcer::Battle;

/// Every play line and every battle line, as a full pack would have.
const FULL: &[&str] = &[
    "gameplay ready",
    "gameplay here we go normal",
    "gameplay 20 combo",
    "gameplay 100 combo",
    "gameplay comment hot",
    "gameplay comment good",
    "gameplay cleared",
    "gameplay failed",
    "gameplay full combo",
    "evaluation aa",
    "evaluation aaa",
    "evaluation aaaa",
    "evaluation cheer",
    "evaluation new record",
    "battle match begin",
    "battle stage begin",
    "battle final stage",
    "battle life critical",
    "battle survivors 2",
    "battle eliminated",
    "battle stage won",
    "battle stage not won",
    "battle stage drawn",
    "battle song finished",
    "battle match won",
    "battle match not won",
    "battle match drawn",
];

/// A two-stage match with a pack of every line: each stage opens with the
/// battle's line in ready's place and here we go before the first note;
/// a play's routine lines (the combos, the comments, cleared, the
/// grades) are never said in a stage, though the pack has them and the
/// combo passes one of their counts; each stage's end is said once on its
/// results, after the room confirmed it, the match's in the last one's
/// place; nothing shows beside the field, everything having been said.
#[test]
fn a_stage_speaks_its_battle_moments_once_and_none_of_a_play_s_routine() {
    let song = bundled("beatucada");
    let settings = Settings::default();
    let mut room = room(&song, 2, 3, 2);
    let mut now = 0;
    let mut heard = Vec::new();
    for stage in 1..=2 {
        let order = begin(&song, &mut room, &mut now);
        let (s, said) = play_voiced(
            &song,
            &settings,
            &mut room,
            &order,
            true,
            Some(voice_of(FULL)),
        );
        let opening = if stage == 1 {
            "battle match begin"
        } else {
            "battle final stage"
        };
        assert_eq!(
            said[..2],
            [opening, "gameplay here we go normal"],
            "{said:?}"
        );
        for routine in ["combo", "comment", "cleared", "failed", "evaluation"] {
            assert!(
                !said.iter().any(|l| l.contains(routine)),
                "{routine}: {said:?}"
            );
        }
        assert!(
            s.player.results().max_combo >= 20,
            "the combo passed the pack's 20"
        );
        let t = told(&s);
        assert_eq!(t[0], s.battle().unwrap().setup().opening());
        // Each moment once.
        for e in &t {
            assert_eq!(t.iter().filter(|x| *x == e).count(), 1, "{t:?}");
        }
        assert!(s.battle_told().iter().all(|t| !t.shown(&BATTLE_VOICE)));
        heard.push((t, said));
    }
    // The stages' ends: the first's said as its own, the last's in the
    // match's place.
    let (first, first_said) = &heard[0];
    let end = *first.last().unwrap();
    assert!(
        matches!(
            end,
            Battle::StageWon | Battle::StageDrawn | Battle::StageNotWon | Battle::SongFinished
        ),
        "{first:?}"
    );
    assert_eq!(
        first_said.last().unwrap(),
        &ddi_library::announcer::Line::Battle(end).folder()
    );
    let (last, last_said) = &heard[1];
    let ending = match room.match_end().unwrap() {
        ddi_ui::state::room::MatchEnd::Won => Battle::MatchWon,
        ddi_ui::state::room::MatchEnd::Drawn => Battle::MatchDrawn,
        ddi_ui::state::room::MatchEnd::NotWon => Battle::MatchNotWon,
    };
    assert!(last.contains(&ending), "{last:?}");
    assert_eq!(
        last_said.last().unwrap(),
        &ddi_library::announcer::Line::Battle(ending).folder()
    );
    assert_eq!(
        last_said
            .iter()
            .filter(|l| l.starts_with("battle match"))
            .count(),
        1,
        "one outcome, never two stacked: {last_said:?}"
    );
}

use crate::announcer::battle::BATTLE_VOICE_V0 as BATTLE_VOICE;

/// The player out of life against one CPU: out is said as it happens
/// (where the pack has it), before the results; the stage decided for the
/// CPU says nothing more on the results (out was the player's line). With
/// a pack of the play's lines alone, `gameplay failed` speaks in its
/// place and the moment shows beside the field; with the announcer off,
/// every moment of the play shows, for the policy's seconds.
#[test]
fn out_of_life_is_said_or_shown_and_nothing_follows_it() {
    let song = bundled("beatucada");
    let settings = Settings::default();
    let packs: [Option<&[&str]>; 3] = [
        Some(FULL),
        Some(&["gameplay ready", "gameplay failed", "gameplay 100 combo"]),
        None,
    ];
    for pack in packs {
        let mut room = room(&song, 1, 1, 4);
        let mut now = 0;
        let order = begin(&song, &mut room, &mut now);
        let (s, said) = play_voiced(
            &song,
            &settings,
            &mut room,
            &order,
            false,
            pack.map(voice_of),
        );
        let t: Vec<_> = s.battle_told().to_vec();
        let events: Vec<Battle> = t.iter().map(|t| t.event).collect();
        // A one-stage match: the match's opening, the life entering
        // danger, out, the match not won.
        assert_eq!(
            events,
            [
                Battle::MatchBegin,
                Battle::LifeCritical,
                Battle::Eliminated,
                Battle::MatchNotWon
            ],
            "{pack:?}"
        );
        let out = t[2];
        match pack {
            Some(p) if p.contains(&"battle eliminated") => {
                assert!(out.spoken() && !out.shown(&BATTLE_VOICE));
                assert_eq!(said.last().unwrap(), "battle match not won");
            }
            Some(_) => {
                assert_eq!(out.said, Some(ddi_library::announcer::Line::Failed));
                assert!(out.shown(&BATTLE_VOICE));
                // The opening said by ready: shown, ready naming no match.
                assert!(t[0].shown(&BATTLE_VOICE));
                assert_eq!(said, ["gameplay ready", "gameplay failed"]);
            }
            None => {
                assert!(said.is_empty());
                assert!(t.iter().all(|t| t.said.is_none()));
                assert!(t[0].shown(&BATTLE_VOICE) && out.shown(&BATTLE_VOICE));
            }
        }
        // The cue beside the field: the latest shown, while it lasts.
        if out.shown(&BATTLE_VOICE) {
            assert_eq!(s.battle_cue(out.at).map(|c| c.0), Some(Battle::Eliminated));
            assert_eq!(s.battle_cue(out.at + BATTLE_VOICE.cue_seconds + 0.01), None);
        }
    }
}

/// Players left: with one life each, the CPUs slip out one by one under
/// the autoplayed player; the count reaching two is told once (three is
/// no milestone, one decides the stage), then the stage won as it is
/// decided, and the winner's finished song on the results. A milestone
/// the pack lacks shows beside the field.
#[test]
fn the_players_left_and_an_early_win_are_told_as_they_happen() {
    let song = bundled("beatucada");
    let mut settings = Settings::default();
    settings.stepmania.life_type = ddi_engine::rules::LifeType::Battery;
    settings.stepmania.lives = 1;
    // The CPUs' times out, as each plays alone.
    let outs = |order: &StageOrder| -> Vec<Option<f64>> {
        order
            .cpus
            .iter()
            .map(|seat| {
                let rules = rules_of(&settings, order, &song);
                let mut alone =
                    ddi_engine::cpu::Opponent::new(&song, CHART, rules, 0, seat.seed).unwrap();
                alone.start(0.0);
                alone.finish();
                alone.out_at()
            })
            .collect()
    };
    // A room whose three CPUs all slip, each at its own time.
    let seed = (1..200)
        .find(|&seed| {
            let mut probe = room_seeded(&song, 1, 3, 0, seed);
            let mut now = 0;
            let times = outs(&begin(&song, &mut probe, &mut now));
            let mut t: Vec<f64> = times.iter().flatten().copied().collect();
            t.sort_by(f64::total_cmp);
            t.len() == 3 && t.windows(2).all(|w| w[1] - w[0] > 0.1)
        })
        .expect("three CPUs that slip apart");
    let mut room = room_seeded(&song, 1, 3, 0, seed);
    let mut now = 0;
    let order = begin(&song, &mut room, &mut now);
    // A pack without the milestone.
    let pack: Vec<&str> = FULL
        .iter()
        .copied()
        .filter(|f| *f != "battle survivors 2")
        .collect();
    let (s, said) = play_voiced(
        &song,
        &settings,
        &mut room,
        &order,
        true,
        Some(voice_of(&pack)),
    );
    assert_eq!(
        told(&s),
        [
            Battle::MatchBegin,
            Battle::Survivors(2),
            Battle::StageWon,
            Battle::MatchWon,
            Battle::SongFinished
        ]
    );
    let t = s.battle_told();
    assert!(t[1].shown(&BATTLE_VOICE), "no line: shown");
    assert!(t[2].spoken() && !t[2].shown(&BATTLE_VOICE));
    // One stage: the match's line in the finished song's place.
    assert!(t[3].spoken() && t[4].said.is_none());
    assert_eq!(
        said,
        [
            "battle match begin",
            "gameplay here we go normal",
            "battle stage won",
            "battle match won"
        ]
    );
}

/// An online room's stage (M5d): no CPUs here, another person playing on
/// their own device. Nothing is decided on this device (an autoplayed
/// player alone in the play is no winner, and keeps its fail), the
/// outcome carries the local player's own run for the room, and no stage
/// end is told before the room ranks everyone's.
#[test]
fn an_online_stage_is_never_decided_here_and_reports_the_players_own_run() {
    let song = bundled("beatucada");
    let settings = Settings::default();
    let mut room = room(&song, 1, 1, 2);
    let mut now = 0;
    let mut order = begin(&song, &mut room, &mut now);
    order.cpus.clear();
    order.others = 1;
    let (s, event, outcome) = play(&song, &settings, &order, true, &[]);
    assert!(matches!(event, SessionEvent::Finished { .. }));
    assert!(!outcome.confirmed_early, "never decided on this device");
    let ddi_room::Run::Live { life, accuracy, .. } = outcome.mine else {
        panic!("{:?}", outcome.mine)
    };
    assert!(life > 0.5 && accuracy > 0.99, "{life} {accuracy}");
    assert_eq!(ids(&outcome), vec![vec![(order.me.0, Finish::Survived)]]);
    let setup = StageSetup::of(&order);
    assert!(setup.remote());
    let battle = crate::battle::Battle::new(
        setup,
        &song,
        CHART,
        &rules_of(&settings, &order, &song),
        0.0,
        0.0,
    )
    .unwrap();
    assert_eq!(battle.end_event(&outcome, false), None);
    drop(s);
}

/// Round 3b (`CPU_SCORE_V0`): a CPU plays a tap-only copy, so on a chart
/// with holds the most its notes give is less than the chart's. Its
/// score as the room shows it (the miniature, the standings) and ranks by
/// it is put on the chart's own scale: the same share of the chart's
/// most, its W0s as counted; its percent is its own. The player's is the
/// sum as their HUD has it, and the stage ranks those still in by life,
/// then that score.
#[test]
fn a_cpu_s_score_is_on_the_chart_s_scale_and_the_stage_ranks_by_life_then_the_score() {
    let song = bundled("beatucada");
    let settings = Settings::default();
    let rules = crate::battle::stage_ruleset(&Default::default());
    // A four-lane chart with holds.
    let index = (0..song.charts.len())
        .find(|&i| {
            let c = &song.charts[i];
            c.layout == "dance-single"
                && c.notes.iter().any(|n| n.kind.is_long())
                && ddi_engine::cpu::Opponent::new(&song, i, rules.fresh(), 5, 1).is_ok()
        })
        .expect("a bundled chart with holds");
    let most = crate::battle::chart_most(&song, index, &rules).expect("DDI");
    let mut room = room(&song, 1, 3, 6);
    let mut now = 0;
    let order = begin_at(&song, &mut room, &mut now, index);
    let (s, _event, outcome) = play(&song, &settings, &order, true, &[]);
    let mine = s.player.ruleset().score.view().ddi.expect("DDI");
    assert_eq!(mine.most, most, "the player plays the whole chart");
    let detail = |id: ddi_room::ParticipantId| {
        &outcome
            .details
            .iter()
            .find(|(p, _)| *p == id)
            .expect("its detail")
            .1
    };
    assert_eq!(detail(order.me).score, Some(mine.score), "the sum, as the HUD");
    let mut expect = vec![(s.player.ruleset().gauge.life(), mine.score, order.me.0)];
    for (view, seat) in s.opponents().iter().zip(&order.cpus) {
        let mut alone = ddi_engine::cpu::Opponent::new(
            &song,
            index,
            rules.fresh(),
            usize::from(seat.level),
            seat.seed,
        )
        .unwrap();
        alone.start(0.0);
        alone.finish();
        let own = alone.score().ddi.expect("DDI");
        assert!(own.most < most, "the tap-only copy gives less: {} {most}", own.most);
        let w0 = own.absolute - 100 * own.points;
        let equivalent = ((own.points as f64) * (most as f64) / (own.most as f64)).round() as i64;
        let sum = own.ratio + 100 * equivalent + w0;
        assert_eq!(view.sum, Some(sum));
        assert!(sum > own.score, "more than its own copy's sum");
        // Its percent is its own play's.
        assert_eq!(view.score, own.ratio as f64 / 1_000_000.0);
        let d = detail(view.participant);
        assert_eq!(d.score, Some(sum));
        assert_eq!(d.percent, i32::try_from(own.percent).ok());
        if !view.out {
            expect.push((view.life, sum, view.participant.0));
        }
    }
    // Those still in: by life, then the score.
    expect.sort_by(|a, b| b.0.total_cmp(&a.0).then(b.1.cmp(&a.1)));
    let survived: Vec<u64> = ids(&outcome)
        .into_iter()
        .flatten()
        .filter(|(_, f)| *f == Finish::Survived)
        .map(|(p, _)| p)
        .collect();
    assert_eq!(survived, expect.iter().map(|e| e.2).collect::<Vec<_>>());
    // The player's run carries the score the room ranks by.
    let ddi_room::Run::Live { score, .. } = outcome.mine else {
        panic!("{:?}", outcome.mine)
    };
    assert_eq!(score, Some(mine.score));
}

/// Each CPU's field is fed its own judged moments as the stage plays
/// (`crate::battle::feed`): every tap of the copy it played, in time
/// order, with no delay, and its life as shown is its gauge's.
#[test]
fn each_cpu_s_miniature_is_fed_its_judged_moments() {
    let song = bundled("beatucada");
    let settings = Settings::default();
    let mut room = room(&song, 1, 3, 2);
    let mut now = 0;
    let order = begin(&song, &mut room, &mut now);
    let (s, _, _) = play(&song, &settings, &order, true, &[]);
    let battle = s.battle().expect("a stage's battle");
    let track = battle.track();
    assert!(!track.is_empty());
    let views = battle.views();
    for (feed, view) in battle.feeds().zip(&views) {
        assert_eq!(feed.delay(), crate::battle::feed::LOCAL_DELAY);
        let moments = feed.moments();
        // Every note of the copy judged once, a miss or a hit.
        assert_eq!(moments.len(), track.len(), "{}", view.name);
        assert!(moments.windows(2).all(|w| w[0].at <= w[1].at));
        let end = moments.last().unwrap().at + 1.0;
        let shown = feed.shown(end);
        assert!((shown.life - view.life).abs() < 1e-6, "{shown:?} {view:?}");
        assert_eq!(shown.out, view.out);
        assert_eq!(shown.combo, view.combo);
        // Its field at the end: nothing left to play.
        let frame = feed.frame(track, Default::default(), end + 2.0, 5.3);
        assert!(frame.notes.is_empty());
    }
}

/// The rules are the match's (D0): a stage is played under DDI with the
/// match's difficulty and Boos, whatever the player's own Options say (their
/// ruleset, their DDI difficulty, Boos or Practice), by the player and the
/// CPUs alike; rules that still name an older ruleset are played as DDI.
#[test]
fn a_stage_is_played_under_the_match_s_rules_not_the_player_s() {
    use ddi_engine::rules::ddi::{DdiOptions, Difficulty};
    for id in MatchRules::RULESETS {
        assert!(ddi_engine::rules::presets::by_id(id).is_some(), "{id}");
    }
    let song = bundled("beatucada");
    let mut room = room(&song, 1, 1, 2);
    let rules = MatchRules {
        // As a room stored before, or an authority one build behind.
        ruleset: "sm5".into(),
        difficulty: ddi_room::MatchDifficulty::Hard,
        boos: true,
        ..room.rules().clone()
    };
    room.save_rules(rules).unwrap();
    let mut now = 0;
    let order = begin(&song, &mut room, &mut now);
    assert_eq!(order.rules.ruleset, "ddi");
    assert_eq!(order.rules.difficulty, ddi_room::MatchDifficulty::Hard);
    assert!(order.rules.boos);
    // The player's own options say otherwise, Practice among them.
    let settings = Settings {
        ruleset: "ddr-a".into(),
        ddi: DdiOptions {
            difficulty: Difficulty::Easy,
            boos: false,
            practice: true,
        },
        ..Settings::default()
    };
    let (s, _, _) = play(&song, &settings, &order, true, &[]);
    let hard = DdiOptions {
        difficulty: Difficulty::Hard,
        boos: true,
        practice: false,
    };
    let rules = s.player.ruleset();
    assert_eq!(rules.id, "ddi");
    assert_eq!(rules.ddi, Some(hard), "the match's, never Practice");
    assert_eq!(rules.fail, ddi_engine::FailPolicy::Immediate);
    // The CPUs play under the same rules.
    assert_eq!(s.opponents().len(), 1);
    assert_eq!(crate::battle::stage_ruleset(&order.rules).ddi, Some(hard));
    // New rooms play DDI on Normal without Boos; a practice set to
    // Practice in the player's own settings still never plays it.
    let fresh = RoomClient::new(0, 1);
    assert_eq!(fresh.rules().ruleset, "ddi");
    let normal = crate::battle::stage_ruleset(&ddi_ui::state::room::StageRules::of(fresh.rules()));
    assert_eq!(normal.ddi, Some(DdiOptions::default()));
    assert_eq!(normal.fail, ddi_engine::FailPolicy::Immediate);
}

/// A practice room under the attacks' battle (the Attacks rule On, as a
/// practice room has it by default).
fn attacks_room(song: &Song, cpus: u16, level: u8) -> RoomClient {
    let mut room = room(song, 1, cpus, level);
    let rules = MatchRules {
        battle: ddi_room::BattlePreset::GAUGE_ATTACKS_V0.into(),
        ..room.rules().clone()
    };
    room.save_rules(rules).unwrap();
    room
}

/// The stage of `order` autoplayed at `fps` to its end.
fn play_at(song: &Song, order: &StageOrder, fps: f64) -> PlaySession<FakeAudio, FakeInputs> {
    let clock: Clock = Rc::new(RefCell::new(0.0));
    let settings = Settings::default();
    let mut config = SessionConfig::from_settings(&settings, true, None);
    config.opponents = Some(StageSetup::of(order));
    let mut s = PlaySession::start(
        song,
        order.chart.index,
        &settings,
        config,
        FakeAudio::new(&clock, 0.0),
        110.0,
        FakeInputs::new(&clock),
    )
    .unwrap();
    run(&mut s, &clock, fps, 300.0).expect("the stage ends");
    s
}

/// Attack power (M8a): earned by timing at checkpoints every four beats,
/// the same ledger at any frame rate and again from the play's log and
/// the CPUs' seeds; the bar full for an autoplayed player, never past
/// its cap; none under the survival battle.
#[test]
fn attack_power_is_the_same_at_any_frame_rate_and_from_the_log() {
    let song = bundled("beatucada");
    let mut practice = attacks_room(&song, 3, 2);
    let mut now = 0;
    let order = begin(&song, &mut practice, &mut now);
    assert_eq!(order.battle, ddi_room::BattlePreset::GaugeAttacksV0);
    let a = play_at(&song, &order, 60.0);
    let b = play_at(&song, &order, 144.0);
    let ra = a.battle().unwrap().attacks().unwrap();
    let rb = b.battle().unwrap().attacks().unwrap();
    let (run, of) = ra.checkpoints();
    assert!(run > 20 && run <= of, "{run} of {of}");
    assert_eq!(ra.record(), rb.record(), "60 and 144 frames a second");
    // Autoplayed: every note in the best window, two each (the chart's
    // 38 notes, the last ones after the last checkpoint run), shown as a
    // share of the bar.
    let tapped = a.player.tally().count(ddi_engine::rules::Judgement::W1);
    let power = ra.power(order.me);
    assert!(
        power.is_multiple_of(2) && power >= 2 * (tapped - 2) && power <= 100,
        "{power}"
    );
    assert_eq!(
        a.battle().unwrap().my_power(&a.player),
        Some((power as f32 / 100.0, false)),
    );
    let views = a.opponents();
    assert!(views.iter().all(|v| v.power.is_some()));
    // The record grows by checkpoints, never past the cap.
    assert!(
        ra.record()
            .iter()
            .flat_map(|c| c.power.iter())
            .all(|(_, p)| *p <= 100)
    );
    // Again without frames: the player's log replayed, the CPUs from
    // their seeds, judged in long steps.
    let again = replayed(&song, &order, &a);
    let n = ra.record().len().min(again.len());
    assert!(n + 1 >= ra.record().len(), "{n}");
    assert_eq!(&again[..n], &ra.record()[..n], "from the log");
    // The survival battle has none.
    let mut plain = room(&song, 1, 1, 2);
    let order = begin(&song, &mut plain, &mut now);
    let s = play_at(&song, &order, 60.0);
    assert!(s.battle().unwrap().attacks().is_none());
    assert_eq!(s.battle().unwrap().my_power(&s.player), None);
    assert!(s.opponents().iter().all(|v| v.power.is_none()));
}

/// The stage `s` played, again without frames: the player's log replayed
/// in long steps into a fresh battle (its shocks delivered again as the
/// checkpoints commit them), the CPUs from their seeds. Its record.
fn replayed(
    song: &Song,
    order: &StageOrder,
    s: &PlaySession<FakeAudio, FakeInputs>,
) -> Vec<ddi_room::attacks::CheckpointRecord> {
    let rules = s.player.ruleset().fresh();
    let log = s.player.log().clone();
    let mut human = ddi_engine::player::Player::new(
        song,
        order.chart.index,
        rules.fresh(),
        *s.player.options(),
    );
    human.start(0.0, log.start);
    let mut battle = crate::battle::Battle::new(
        StageSetup::of(order),
        song,
        order.chart.index,
        &rules,
        log.start,
        human.reading_horizon(),
    )
    .unwrap();
    let mut inputs = log.inputs.iter().peekable();
    let mut policies = log.policies.iter().peekable();
    let mut t = log.start - 1.0;
    while t < human.end_time() + 2.0 {
        t += 0.37;
        let mut events = Vec::new();
        while let Some(i) = inputs.next_if(|i| i.time <= t) {
            events.extend(human.input_at(i.lane, i.pressed, i.time));
        }
        // The policy as the log has it changed.
        while let Some(c) = policies.next_if(|c| c.time <= t) {
            battle.retarget(crate::battle::policy_of(c.policy).unwrap(), c.time);
        }
        events.extend(human.update_at(t));
        battle.advance(human.judge().last_time().unwrap_or(t), &mut human, &events);
        if human.finished() {
            break;
        }
    }
    // The shocks put into the replayed play are those the log kept.
    assert_eq!(human.log().hazards, log.hazards);
    battle.attacks().unwrap().record().to_vec()
}

/// DEVELOPMENT ONLY (M8a): a section of battle shocks from a beat put into
/// the player's own play of a stage, judged beside the chart (autoplayed,
/// every one avoided: the placement keeps them clear of every step), the
/// score as without them.
#[test]
fn a_stage_s_development_shocks_are_judged_beside_the_chart() {
    let song = bundled("beatucada");
    let mut practice = room(&song, 2, 1, 2);
    let mut now = 0;
    let plain = begin(&song, &mut practice, &mut now);
    let without = play_at(&song, &plain, 60.0);
    let mut order = plain.clone();
    order.dev_mines_at_beat = Some(16);
    let with = play_at(&song, &order, 60.0);
    let h = with.player.hazards().expect("its shocks");
    assert_eq!(h.mines().len(), 8);
    assert!(h.mines().iter().all(|m| m.tick.beat() >= 16.0));
    let r = with.player.results();
    assert_eq!(
        r.battle_mines,
        Some(ddi_engine::player::BattleMines { hit: 0, avoided: 8 })
    );
    let w = without.player.results();
    assert_eq!((r.score, r.tally), (w.score, w.tally));
    assert_eq!(w.battle_mines, None);
    // In its log, and judged alike again from it.
    assert_eq!(with.player.log().hazards.len(), 8);
    let again = ddi_engine::history::replay_with(
        &song,
        order.chart.index,
        with.player.ruleset().fresh(),
        with.player.options(),
        with.player.log(),
    );
    assert_eq!(again.battle_mines, r.battle_mines);
}

/// M8b: the CPUs receive battle shocks too (the development trigger's
/// section, `Battle::dev_shocks`), judged by their own players and
/// responded to by level alone; their fields draw them from the
/// commitment; the same at 60 and 144 frames a second.
#[test]
fn cpus_receive_shocks_judged_alike_at_any_frame_rate() {
    let song = bundled("beatucada");
    let mut practice = room(&song, 1, 4, 0);
    let mut now = 0;
    let mut order = begin(&song, &mut practice, &mut now);
    order.dev_mines_at_beat = Some(16);
    let a = play_at(&song, &order, 60.0);
    let b = play_at(&song, &order, 144.0);
    let (ba, bb) = (a.battle().unwrap(), b.battle().unwrap());
    assert_eq!(ba.views(), bb.views());
    let mut hit = 0;
    for (fa, fb) in ba.feeds().zip(bb.feeds()) {
        assert_eq!(fa.shocks().len(), 8);
        assert_eq!(fa.moments(), fb.moments());
        let hits = fa
            .moments()
            .iter()
            .filter(|m| m.kind == crate::battle::feed::MomentKind::ShockHit)
            .count();
        // A shock stepped on leaves its field.
        assert_eq!(
            fa.shocks().iter().filter(|s| s.gone.is_finite()).count(),
            hits
        );
        hit += hits;
    }
    // Level 1 avoids poorly; whether any of these 32 is stepped on is
    // the seeds' (the engine's tests check the rates).
    assert!(hit <= 32);
}

/// The DDI ruleset's round 3: in a stage a shock stepped on is a mine,
/// judged under the match's rules: it is in the mines' count, the combo
/// breaks, the life drops by the match's difficulty (−1, −5, −10) and the
/// lane locks for a second, for the player (a key pressed on the first
/// shock of the development section, over the autoplay) as for the CPUs
/// (whose mines' count is their shocks stepped on, their feeds saying
/// the lock).
#[test]
fn a_shock_stepped_on_in_a_stage_is_a_mine_by_the_match_s_difficulty() {
    use ddi_room::MatchDifficulty as D;
    let song = bundled("beatucada");
    for (difficulty, cost) in [(D::Easy, 0.01f32), (D::Normal, 0.05), (D::Hard, 0.10)] {
        let mut practice = room(&song, 1, 4, 0);
        let rules = MatchRules {
            difficulty,
            ..practice.rules().clone()
        };
        practice.save_rules(rules).unwrap();
        let mut now = 0;
        let mut order = begin(&song, &mut practice, &mut now);
        assert_eq!(order.rules.difficulty, difficulty);
        order.dev_mines_at_beat = Some(16);
        let settings = Settings::default();
        // Where the section's first shock is: as an untouched play has it.
        let (plain, _, _) = play(&song, &settings, &order, true, &[]);
        let first = plain.player.hazards().expect("its shocks").mines()[0];
        assert_eq!(plain.player.tally().mine_hit, 0, "autoplay steps on none");
        // Its lane's key pressed 30 ms before it, over the autoplay.
        let key = ["ArrowLeft", "ArrowDown", "ArrowUp", "ArrowRight"][usize::from(first.lane)];
        let at = first.seconds - 0.03;
        // The fake host's second of that song second (the same in every
        // play of the stage: its lead-in).
        let lead = 10.0
            - plain
                .player
                .clock()
                .song_time_at_host(ddi_platform::HostTime(10.0));
        let (s, _, _) = play(
            &song,
            &settings,
            &order,
            true,
            &[(at + lead, key, true), (at + lead + 0.06, key, false)],
        );
        let p = &s.player;
        let r = p.results();
        assert_eq!(r.tally.mine_hit, 1, "{difficulty:?}: in the mines' count");
        assert_eq!(
            r.battle_mines,
            Some(ddi_engine::player::BattleMines { hit: 1, avoided: 7 })
        );
        // The life at the hit, by the difficulty.
        let hit = p
            .log()
            .inputs
            .iter()
            .find(|i| i.pressed && i.lane == first.lane && (i.time - at).abs() < 0.02)
            .map(|i| i.time)
            .expect("the press");
        let drop = p.life_at(hit) - p.life_at(hit + 0.000_5);
        assert!((drop - cost).abs() < 1e-5, "{difficulty:?}: {drop}");
        // The lane locked from the press for the lock's second.
        assert_eq!(p.judge().locked_until(first.lane), Some(hit + 1.0));
        // The combo broke there: the longest run is the notes before it
        // or those after (every one hit), never all of them.
        let before = p.judge().notes().iter().filter(|n| n.seconds < hit).count() as u32;
        let all = plain.player.results().max_combo;
        assert_eq!(r.max_combo, before.max(all - before), "{difficulty:?}");
        // The CPUs: theirs counted as mines too, their fields saying the
        // lock for its second.
        let b = s.battle().unwrap();
        for feed in b.feeds() {
            for m in feed
                .moments()
                .iter()
                .filter(|m| m.kind == crate::battle::feed::MomentKind::ShockHit)
            {
                let lanes = b.track().lanes();
                let locked = feed.locks(lanes, m.at + 0.5);
                assert!(locked[usize::from(m.lane)].is_some(), "{locked:?}");
                assert!(feed.locks(lanes, m.at + 1.01)[usize::from(m.lane)].is_none());
            }
        }
    }
}

/// Beatucada's Medium chart: long enough for full bars (217 notes).
const LONG: usize = 4;

/// M8b: full bars launch attacks of eight shocks at the checkpoints,
/// delivered into the recipients' plays (the player's own, a CPU's and
/// its field); the record the same at 60 and 144 frames a second and
/// again from the play's log and the CPUs' seeds.
#[test]
fn attacks_are_launched_delivered_and_the_same_at_any_frame_rate_and_from_the_log() {
    let song = bundled("beatucada");
    let mut practice = attacks_room(&song, 3, 2);
    let mut now = 0;
    let order = begin_at(&song, &mut practice, &mut now, LONG);
    let a = play_at(&song, &order, 60.0);
    let b = play_at(&song, &order, 144.0);
    let ra = a.battle().unwrap().attacks().unwrap();
    let rb = b.battle().unwrap().attacks().unwrap();
    assert_eq!(ra.record(), rb.record(), "60 and 144 frames a second");
    let attacks = ra.ledger().attacks();
    assert!(!attacks.is_empty(), "full bars launch");
    assert!(
        attacks.iter().all(|x| x.cells.len() == 8),
        "never fewer (D4)"
    );
    // Delivered: the player's own (autoplayed, every one avoided), and
    // each CPU's field shows those sent to it.
    let me = order.me;
    let mine = attacks.iter().filter(|x| x.recipient == me).count();
    assert_eq!(a.player.log().hazards.len(), 8 * mine);
    if mine > 0 {
        assert_eq!(
            a.player.results().battle_mines.map(|m| m.hit),
            Some(0),
            "autoplay steps on none"
        );
    }
    for (feed, seat) in a.battle().unwrap().feeds().zip(&order.cpus) {
        let sent = attacks
            .iter()
            .filter(|x| x.recipient == seat.participant)
            .count();
        assert_eq!(feed.shocks().len(), 8 * sent, "{seat:?}");
    }
    // One attack at a time against anyone, each from a full bar.
    for x in attacks {
        assert_ne!(x.sender, x.recipient);
    }
    let again = replayed(&song, &order, &a);
    let n = ra.record().len().min(again.len());
    assert!(n + 1 >= ra.record().len());
    assert_eq!(&again[..n], &ra.record()[..n], "from the log");
    // The log line of each commitment.
    eprintln!(
        "attacks: {} ({} against the player), stats {:?}",
        attacks.len(),
        mine,
        std::iter::once(me)
            .chain(order.cpus.iter().map(|c| c.participant))
            .map(|p| ra.stats(p))
            .collect::<Vec<_>>()
    );
}

/// The stage of `order` autoplayed at `fps` to its end with `edges`
/// queued (host seconds, the device, its control, pressed) under
/// `settings`, one standard pad connected.
fn play_with(
    song: &Song,
    order: &StageOrder,
    fps: f64,
    settings: &Settings,
    edges: &[(f64, DeviceId, &str, bool)],
) -> PlaySession<FakeAudio, FakeInputs> {
    let clock: Clock = Rc::new(RefCell::new(0.0));
    let mut config = SessionConfig::from_settings(settings, true, None);
    config.opponents = Some(StageSetup::of(order));
    let mut inputs = FakeInputs::new(&clock);
    inputs.pads.push(ConnectedPad {
        vendor_product: None,
        id: PAD.into(),
        index: 0,
        standard: true,
    });
    for (at, device, control, pressed) in edges {
        inputs.queued.push(RawInput {
            device: device.clone(),
            control: (*control).into(),
            pressed: *pressed,
            host_time: HostTime(*at),
            slot: 0,
        });
    }
    let mut s = PlaySession::start(
        song,
        order.chart.index,
        settings,
        config,
        FakeAudio::new(&clock, 0.0),
        110.0,
        inputs,
    )
    .unwrap();
    run(&mut s, &clock, fps, 300.0).expect("the stage ends");
    s
}

const PAD: &str = "Pad (STANDARD GAMEPAD)";

/// M8 round 3 (D8): the attack policy changed during the play. Tab, a
/// tap on its label and a short press of a pad's Back each cycle it
/// (Spread, Retaliate, Leader, Spread); the change is in the play's log
/// at its judged second and goes to the ledger, whose launches follow it
/// from the next checkpoint: the same record at 60 and 144 frames a
/// second and again from the log; the stage's outcome says what the play
/// left it at. A pad's Back held quits as ever; pressed twice it is two
/// steps of the cycle, never a quit.
#[test]
fn a_policy_changed_in_play_applies_from_the_next_checkpoint_at_any_frame_rate_and_from_the_log() {
    use crate::battle::Targeting;
    use ddi_room::attacks::AttackEvent;
    let song = bundled("beatucada");
    let mut practice = attacks_room(&song, 3, 2);
    let mut now = 0;
    let order = begin_at(&song, &mut practice, &mut now, LONG);
    assert_eq!(order.targeting, Targeting::Spread);
    let settings = Settings::default();
    let key = || DeviceId::Keyboard;
    let pad = || DeviceId::Gamepad(PAD.into());
    let back = "button:8";
    let edges = [
        // Tab: Retaliate.
        (12.0, key(), "Tab", true),
        (12.08, key(), "Tab", false),
        // The label tapped: Leader.
        (25.3, DeviceId::Touch, crate::play::TOUCH_POLICY_CONTROL, true),
        (25.4, DeviceId::Touch, crate::play::TOUCH_POLICY_CONTROL, false),
        // A pad's Back pressed twice, quickly: Spread, then Retaliate
        // (each on its release), and no quit.
        (40.0, pad(), back, true),
        (40.1, pad(), back, false),
        (40.25, pad(), back, true),
        (40.35, pad(), back, false),
    ];
    let a = play_with(&song, &order, 60.0, &settings, &edges);
    let b = play_with(&song, &order, 144.0, &settings, &edges);
    let log = a.player.log().clone();
    assert_eq!(
        log.policies.iter().map(|c| c.policy).collect::<Vec<_>>(),
        vec![1, 2, 0, 1],
        "Retaliate, Leader, Spread, Retaliate"
    );
    assert_eq!(log.policies, b.player.log().policies);
    assert_eq!(log.end, None, "two presses of Back are no quit");
    let ra = a.battle().unwrap().attacks().unwrap();
    let rb = b.battle().unwrap().attacks().unwrap();
    assert_eq!(ra.record(), rb.record(), "60 and 144 frames a second");
    assert_eq!(ra.ledger().targeting(order.me), Targeting::Retaliate);
    // Each change is the ledger's at the first checkpoint after it, and
    // not before.
    let heard: Vec<(i64, Targeting)> = ra
        .record()
        .iter()
        .flat_map(|c| {
            c.events.iter().filter_map(move |e| match e {
                AttackEvent::Retargeted {
                    participant,
                    targeting,
                } if *participant == order.me => Some((c.at_us, *targeting)),
                _ => None,
            })
        })
        .collect();
    // What it should hear: at each checkpoint, the policy as it stood
    // just before it, where that is not the one it had.
    let checkpoints = ra.ledger().checkpoints_us();
    let mut expected = Vec::new();
    let mut has = Targeting::Spread;
    let mut changes = log.policies.iter().peekable();
    for &c in checkpoints {
        let mut last = None;
        while let Some(ch) = changes.next_if(|ch| crate::battle::attacks::micros(ch.time) < c) {
            last = crate::battle::policy_of(ch.policy);
        }
        if let Some(t) = last.filter(|t| *t != has) {
            has = t;
            expected.push((c, t));
        }
    }
    assert_eq!(heard, expected);
    assert!(heard.len() >= 3, "{heard:?}");
    assert_eq!(heard[0].1, Targeting::Retaliate);
    // The first change at 12 s is heard at the first checkpoint after
    // it, not at one before.
    let first = crate::battle::attacks::micros(log.policies[0].time);
    assert_eq!(
        heard[0].0,
        checkpoints.iter().copied().find(|c| *c > first).unwrap()
    );
    // Again without frames, from the log.
    let again = replayed(&song, &order, &a);
    let n = ra.record().len().min(again.len());
    assert!(n + 1 >= ra.record().len());
    assert_eq!(&again[..n], &ra.record()[..n], "from the log");
    // The log's own form keeps them (version 5).
    let bytes = log.encode();
    assert_eq!(bytes[0], 5);
    assert_eq!(ddi_engine::history::PlayLog::decode(&bytes).unwrap(), log);
    // The outcome says what the play left the policy at.
    let mut a = a;
    assert_eq!(
        a.take_stage_outcome().unwrap().targeting,
        Targeting::Retaliate
    );
    // Held, a pad's Back quits (a forfeit), and cycles nothing.
    let held = [(20.0, pad(), back, true), (21.5, pad(), back, false)];
    let mut q = play_with(&song, &order, 60.0, &settings, &held);
    assert!(q.player.log().policies.is_empty());
    assert!(q.player.log().end.is_some(), "quit");
    assert_eq!(q.take_stage_outcome().unwrap().mine, ddi_room::Run::Forfeit);
}

/// D8: a control the player's bindings give to a lane is never the
/// policy's: Tab bound to a lane plays the lane, and the policy stays.
/// Without attacks nothing cycles either (no label, no policy).
#[test]
fn a_lane_s_key_is_never_the_policy_s() {
    use crate::battle::Targeting;
    let song = bundled("beatucada");
    let mut practice = attacks_room(&song, 2, 2);
    let mut now = 0;
    let order = begin_at(&song, &mut practice, &mut now, LONG);
    let mut settings = Settings::default();
    let layout = song.layout_of(&song.charts[LONG]).unwrap();
    let mut table = settings.controls.keyboard(&layout);
    table[0].push("Tab".into());
    settings.controls.set_keyboard(&layout, table);
    let edges = [
        (12.0, DeviceId::Keyboard, "Tab", true),
        (12.08, DeviceId::Keyboard, "Tab", false),
    ];
    let s = play_with(&song, &order, 60.0, &settings, &edges);
    assert!(s.player.log().policies.is_empty());
    assert_eq!(
        s.battle().unwrap().attacks().unwrap().ledger().targeting(order.me),
        Targeting::Spread
    );
    // The same key under the default bindings cycles it, and the page
    // never takes Tab for its focus.
    let s = play_with(&song, &order, 60.0, &Settings::default(), &edges);
    assert_eq!(s.player.log().policies.len(), 1);
    assert!(
        s.inputs()
            .begun
            .as_ref()
            .unwrap()
            .0
            .contains(&"Tab".to_string())
    );
    // The survival battle: nothing to cycle.
    let mut plain = room(&song, 1, 2, 2);
    let order = begin_at(&song, &mut plain, &mut now, LONG);
    let s = play_with(&song, &order, 60.0, &Settings::default(), &edges);
    assert!(s.player.log().policies.is_empty());
    assert_eq!(s.battle().unwrap().policy(&s.player), None);
    assert!(!s.inputs().begun.as_ref().unwrap().0.contains(&"Tab".to_string()));
}

/// `attacks-v1` (round 3b): no recovery, and one more attack may be
/// reserved behind the one on its way or passing. Over a long chart with
/// strong CPUs, chains happen: an attack committed against someone while
/// an earlier one against them had not cleared, its first shock after
/// the earlier one's last (never a shared section), never a third at
/// once; both are delivered into the recipient's play (the player's own,
/// a CPU's and its field) as they are committed; and the record is the
/// same at 60 and 144 frames a second.
#[test]
fn chained_attacks_are_delivered_back_to_back_into_plays_and_cpus() {
    assert_eq!(crate::battle::attacks::rules().id, "attacks-v1");
    let song = bundled("beatucada");
    let mut practice = attacks_room(&song, 3, 9);
    let mut now = 0;
    let order = begin_at(&song, &mut practice, &mut now, LONG);
    let a = play_at(&song, &order, 60.0);
    let b = play_at(&song, &order, 144.0);
    let ra = a.battle().unwrap().attacks().unwrap();
    assert_eq!(
        ra.record(),
        b.battle().unwrap().attacks().unwrap().record(),
        "60 and 144 frames a second"
    );
    let attacks = ra.ledger().attacks();
    let checkpoints = ra.ledger().checkpoints_us();
    let (mut chained, mut to_me, mut to_cpu) = (0, 0, 0);
    for x in attacks {
        let at = checkpoints[x.checkpoint];
        // Those against the same recipient still on their way or passing
        // when it was committed.
        let before: Vec<_> = attacks
            .iter()
            .filter(|o| o.id < x.id && o.recipient == x.recipient && o.clear_us >= at)
            .collect();
        assert!(before.len() <= 1, "never more than one waiting: {x:?}");
        if let Some(o) = before.first() {
            chained += 1;
            assert!(
                x.cells.first().unwrap().at_us > o.clear_us,
                "after the one before it has cleared"
            );
            if x.recipient == order.me {
                to_me += 1;
            } else {
                to_cpu += 1;
            }
        }
    }
    assert!(chained > 0, "strong CPUs chain: {} attacks", attacks.len());
    eprintln!(
        "chains: {chained} of {} attacks ({to_me} against the player, {to_cpu} against CPUs)",
        attacks.len()
    );
    // Every commitment is in its recipient's play, chained or not.
    let mine = attacks.iter().filter(|x| x.recipient == order.me).count();
    assert_eq!(a.player.log().hazards.len(), 8 * mine);
    for (feed, seat) in a.battle().unwrap().feeds().zip(&order.cpus) {
        let sent = attacks
            .iter()
            .filter(|x| x.recipient == seat.participant)
            .count();
        assert_eq!(feed.shocks().len(), 8 * sent, "{seat:?}");
    }
}

/// SETTLEMENT_V0 (D6): no Stage won while an attack against the last one
/// in is outstanding. CPUs on a battery of a few lives go out to their
/// misses and to the attacks; the autoplayed player (who never steps on
/// a shock) is the last one in. Whenever the stage is decided early,
/// every attack committed against the player before then had cleared;
/// over these seeds it waits for one at least once.
#[test]
fn an_early_win_waits_for_the_last_attack_to_clear() {
    let song = bundled("beatucada");
    let mut waited = 0;
    let mut early = 0;
    for seed in 1..=60u64 {
        let mut settings = Settings::default();
        settings.stepmania.life_type = ddi_engine::rules::LifeType::Battery;
        settings.stepmania.lives = 1 + (seed % 5) as u8;
        let mut practice = room_seeded(&song, 1, 3 + (seed % 5) as u16, 2 + (seed % 3) as u8, seed);
        let rules = MatchRules {
            battle: ddi_room::BattlePreset::GAUGE_ATTACKS_V0.into(),
            ..practice.rules().clone()
        };
        practice.save_rules(rules).unwrap();
        let mut now = 0;
        let order = begin_at(&song, &mut practice, &mut now, LONG);
        let (s, _, outcome) = play(&song, &settings, &order, true, &[]);
        let b = s.battle().unwrap();
        let Some(at) = b.decided_at() else {
            continue;
        };
        early += 1;
        assert!(outcome.confirmed_early);
        let ledger = b.attacks().unwrap().ledger();
        for x in ledger.attacks() {
            let committed = ledger.checkpoints_us()[x.checkpoint] as f64 / 1e6;
            if x.recipient == order.me && committed < at {
                assert!(
                    x.clear_us as f64 / 1e6 <= at + 0.05,
                    "seed {seed}: decided at {at} before {} cleared",
                    x.clear_us
                );
            }
        }
        // Won alone (the player never steps on one).
        assert_eq!(ids(&outcome)[0], vec![(order.me.0, Finish::Survived)]);
        assert!(!outcome.drawn_by_final_attack);
        waited += usize::from(b.waited_to_settle());
    }
    assert!(early > 0 && waited > 0, "{early} {waited}");
}

/// M8b: once the player has left (a quit: a forfeit), the CPUs and the
/// attacks between them are played out a checkpoint at a time to the
/// song's end, as they would have gone on; a stage left at its prompt
/// plays them out the same way.
#[test]
fn the_player_gone_the_cpus_and_their_attacks_are_played_out() {
    let song = bundled("beatucada");
    let settings = Settings::default();
    let mut practice = attacks_room(&song, 4, 3);
    let mut now = 0;
    let order = begin_at(&song, &mut practice, &mut now, LONG);
    let keys = [
        (20.0, "Escape", true),
        (20.05, "Escape", false),
        (20.2, "Escape", true),
    ];
    let (s, event, outcome) = play(&song, &settings, &order, true, &keys);
    assert!(matches!(event, SessionEvent::Aborted { .. }));
    let ledger = s.battle().unwrap().attacks().unwrap().ledger();
    let (run, of) = s.battle().unwrap().attacks().unwrap().checkpoints();
    assert!(run + 1 >= of, "played out: {run} of {of}");
    // Attacks between CPUs after the player left; none against the
    // player after that.
    let after: Vec<_> = ledger
        .attacks()
        .iter()
        .filter(|x| ledger.checkpoints_us()[x.checkpoint] > 21_000_000)
        .collect();
    assert!(!after.is_empty());
    assert!(
        after
            .iter()
            .all(|x| x.recipient != order.me && x.sender != order.me)
    );
    assert_eq!(
        ids(&outcome).last().unwrap(),
        &vec![(order.me.0, Finish::Forfeit)]
    );
    assert_eq!(ids(&outcome).iter().flatten().count(), 5);
    // Left at the prompt: the same CPUs played out with attacks.
    let f = crate::battle::forfeit(
        StageSetup::of(&order),
        &song,
        LONG,
        &rules_of(&settings, &order, &song),
    )
    .unwrap();
    assert_eq!(f.mine, ddi_room::Run::Forfeit);
    assert_eq!(ids(&f).iter().flatten().count(), 5);
}
