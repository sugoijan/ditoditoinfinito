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
    room.save_rules(rules).unwrap();
    room
}

/// Beatucada's chart as the library lists it.
fn entry(song: &Song) -> SongEntry {
    SongEntry {
        id: "beatucada".into(),
        title: "Beatucada".into(),
        charts: vec![ChartEntry {
            style: "dance-single".into(),
            difficulty: Difficulty::Hard,
            index: CHART,
            digest: Some(song.chart_digest(CHART)),
            ..ChartEntry::default()
        }],
        ..SongEntry::default()
    }
}

/// The pick (its song prepared: its chart read and the CPUs' plan
/// checked, as a shell does), Ready, the countdown: the stage the room
/// hands over.
fn begin(song: &Song, room: &mut RoomClient, now: &mut u64) -> StageOrder {
    let e = entry(song);
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

/// The stage's ruleset as [`play`] gives it: the match's (D0), under
/// `settings`' StepMania options where a test sets them (a gauge of one
/// life, so that CPUs go out), its fail immediate.
fn rules_of(settings: &Settings, order: &StageOrder, song: &Song) -> ddi_engine::rules::Ruleset {
    let r = crate::battle::stage_ruleset(&order.ruleset, &song.charts[order.chart.index]);
    if settings.stepmania.is_default() {
        return r;
    }
    let mut r = r.with_stepmania(&settings.stepmania);
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
    let ddi_room::Run::Live { life, accuracy } = outcome.mine else {
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
    )
    .unwrap();
    assert_eq!(battle.end_event(&outcome, false), None);
    drop(s);
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

/// The ruleset is a match rule (D0): the stage is played under the
/// match's, whatever the player's own Options say; every ruleset a match
/// may name is one of the engine's presets.
#[test]
fn a_stage_is_played_under_the_match_s_ruleset_not_the_player_s() {
    for id in MatchRules::RULESETS {
        assert!(ddi_engine::rules::presets::by_id(id).is_some(), "{id}");
    }
    let song = bundled("beatucada");
    let mut room = room(&song, 1, 1, 2);
    let rules = MatchRules {
        ruleset: "sm5".into(),
        ..room.rules().clone()
    };
    room.save_rules(rules).unwrap();
    let mut now = 0;
    let order = begin(&song, &mut room, &mut now);
    assert_eq!(order.ruleset, "sm5");
    let settings = Settings {
        ruleset: "ddr-a".into(),
        ..Settings::default()
    };
    let (s, _, _) = play(&song, &settings, &order, true, &[]);
    assert_eq!(s.player.ruleset().id, "sm5");
    assert_eq!(s.player.ruleset().fail, ddi_engine::FailPolicy::Immediate);
    // New rooms play ITG FA+.
    let fresh = RoomClient::new(0, 1);
    assert_eq!(fresh.rules().ruleset, "itg-fa+");
}

/// A practice room under the attacks' battle (M8a's development switch's).
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
    let rules = a.player.ruleset().fresh();
    let log = a.player.log().clone();
    let mut human =
        ddi_engine::player::Player::new(&song, CHART, rules.fresh(), *a.player.options());
    human.start(0.0, log.start);
    let mut cpus: Vec<_> = order
        .cpus
        .iter()
        .map(|seat| {
            let mut o = ddi_engine::cpu::Opponent::with_skills(
                &song,
                CHART,
                rules.fresh(),
                usize::from(seat.level),
                seat.seed,
                seat.skills
                    .as_deref()
                    .and_then(|v| ddi_engine::cpu::Skills::parse(v).ok())
                    .unwrap_or_default(),
            )
            .unwrap();
            o.start(0.0);
            o
        })
        .collect();
    let mut runner = crate::battle::attacks::AttackRunner::new(
        &song,
        CHART,
        &order.ruleset,
        std::iter::once(order.me).chain(order.cpus.iter().map(|c| c.participant)),
    );
    let mut inputs = log.inputs.iter().peekable();
    let mut t = -1.0;
    while t < human.end_time() + 2.0 {
        t += 0.37;
        let mut events = Vec::new();
        while let Some(i) = inputs.next_if(|i| i.time <= t) {
            events.extend(human.input_at(i.lane, i.pressed, i.time));
        }
        events.extend(human.update_at(t));
        runner.judged(order.me, &events);
        for (o, seat) in cpus.iter_mut().zip(&order.cpus) {
            o.advance(t);
            runner.judged(seat.participant, &o.take_judged());
        }
        let until = std::iter::once(human.judge().last_time())
            .chain(cpus.iter().map(|o| o.player().judge().last_time()))
            .map(|l| l.unwrap_or(f64::NEG_INFINITY))
            .reduce(f64::min)
            .unwrap();
        runner.run_due(until);
    }
    let n = ra.record().len();
    assert_eq!(&runner.record()[..n], ra.record(), "from the log");
    // The survival battle has none.
    let mut plain = room(&song, 1, 1, 2);
    let order = begin(&song, &mut plain, &mut now);
    let s = play_at(&song, &order, 60.0);
    assert!(s.battle().unwrap().attacks().is_none());
    assert_eq!(s.battle().unwrap().my_power(&s.player), None);
    assert!(s.opponents().iter().all(|v| v.power.is_none()));
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
