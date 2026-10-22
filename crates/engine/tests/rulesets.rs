//! Phase 10's rulesets (`docs/plans/rulesets.md`): DDR EXTREME's score,
//! grades and combo, DDR A's other gauges (LIFE4, RISKY, FLARE, FLOATING
//! FLARE) with the FLARE rank and FLARE SKILL a play keeps, DDR X's
//! SuperNOVA2 score and its grades by the chart's difficulty, and that each
//! play's key builds the same rules again, so its replay judges the same.

mod common;

use ddi_chart::{Difficulty, Note, NoteKind, Tick, TimingMap};
use ddi_engine::autoplay;
use ddi_engine::history::replay;
use ddi_engine::player::{PlayOptions, Player, Results};
use ddi_engine::rules::ddr::{DdrClear, DdrGauge, FLARE_EX};
use ddi_engine::rules::presets::{self, HoldStyle, RulesetMode};
use ddi_engine::rules::{
    FailType, FullCombo, Judgement, LifeType, Max2, Ruleset, RulesetKey, ScoreCtx, ScoreRules,
    StepManiaOptions,
};
use ddi_engine::{JudgeEvent, JudgeEventKind};

/// `n` rows one a beat at 120 BPM (row `i` at beat `i + 1`, `(i + 1) / 2`
/// s), lanes in turn; every `jump`th row a jump (lanes 0 and 3) and every
/// `hold`th a half-beat freeze, where not 0. The chart's meter is `meter`.
fn chart_of(n: i64, jump: i64, hold: i64, meter: u32) -> ddi_chart::Song {
    let mut notes = Vec::new();
    for i in 0..n {
        let tick = Tick::from_beats(i + 1);
        if jump > 0 && i % jump == jump - 1 {
            notes.push(Note::new(tick, 0, NoteKind::Tap));
            notes.push(Note::new(tick, 3, NoteKind::Tap));
        } else if hold > 0 && i % hold == hold - 1 {
            let end = Tick(tick.0 + 24);
            notes.push(Note::new(tick, (i % 4) as u8, NoteKind::HoldHead { end }));
        } else {
            notes.push(Note::new(tick, (i % 4) as u8, NoteKind::Tap));
        }
    }
    let mut song = common::song("dance-single", TimingMap::constant(120.0, 0.0), notes);
    song.charts[0].meter = meter;
    song
}

/// [`chart_of`] with taps and jumps only.
fn chart(n: i64, jump: i64, meter: u32) -> ddi_chart::Song {
    chart_of(n, jump, 0, meter)
}

fn ddr_a(gauge: DdrGauge, fail: FailType) -> Ruleset {
    presets::ddr_a().with_stepmania(&StepManiaOptions {
        gauge,
        fail,
        ..StepManiaOptions::default()
    })
}

/// Plays `song` with `edges` (`(song seconds, lane, pressed)`) to the end;
/// checks that its key and log judge the same again.
fn run(song: &ddi_chart::Song, rules: Ruleset, mut edges: Vec<(f64, u8, bool)>) -> Results {
    edges.sort_by(|a, b| a.0.total_cmp(&b.0));
    let mut p = Player::new(song, 0, rules, PlayOptions::default());
    p.start(0.0, 0.0);
    for (t, lane, pressed) in edges {
        p.update_at(t);
        p.input_at(lane, pressed, t);
    }
    p.update_at(1e6);
    let r = p.results();
    let key = p.ruleset().key();
    assert_eq!(
        replay(song, 0, &key, &PlayOptions::default(), p.log()),
        r,
        "{key:?}"
    );
    r
}

/// Plays a chart of taps, pressing note `i` `late(i)` seconds off its time
/// (`None`: not pressed) for 30 ms.
fn play(song: &ddi_chart::Song, rules: Ruleset, late: impl Fn(usize) -> Option<f64>) -> Results {
    let edges = song.charts[0]
        .notes
        .iter()
        .enumerate()
        .filter_map(|(i, n)| Some((n.tick.beat() / 2.0 + late(i)?, n.lane)))
        .flat_map(|(t, lane)| [(t, lane, true), (t + 0.03, lane, false)])
        .collect();
    run(song, rules, edges)
}

/// Autoplayed with the engine's script.
fn autoplayed(song: &ddi_chart::Song, rules: Ruleset) -> Results {
    let probe = Player::new(song, 0, rules.fresh(), PlayOptions::default());
    let edges = autoplay::edges(autoplay::script(probe.judge().notes(), 0.0));
    run(song, rules, edges)
}

#[test]
fn ddr_extreme_scores_and_grades_as_max2() {
    // 12 rows, every third a jump: 16 notes, 12 steps. A 9-foot song:
    // all Perfect is 90,000,000, AAA.
    let song = chart(12, 3, 9);
    let r = play(&song, presets::ddr_extreme(), |_| Some(0.0));
    assert_eq!(r.score.money, Some(90_000_000));
    assert_eq!(r.score.percent, Some(1.0));
    assert_eq!(r.grade, "AAA");
    // No Marvelous in normal play: Perfect is the best tier.
    assert_eq!(r.tally.count(Judgement::W2), 12);
    assert_eq!(r.tally.count(Judgement::W1), 0);
    assert_eq!(r.full_combo, FullCombo::PerfectFC);
    // A jump adds two to the combo, counts once otherwise.
    assert_eq!(r.max_combo, 16);

    // The first step a Great (50 ms late; Great to 76.667 ms late): 5
    // for 10 at n = 1, `floor(B / S)` = 9,000,000 / 78 = 115,384.
    let r = play(&song, presets::ddr_extreme(), |i| {
        Some(if i == 0 { 0.05 } else { 0.0 })
    });
    assert_eq!(r.tally.count(Judgement::W3), 1);
    assert_eq!(r.score.money, Some(90_000_000 - 5 * 115_384));
    // 23 of 24 dance points: 95.83%, AA.
    assert_eq!(r.grade, "AA");
    // A Good (100 ms early) breaks the combo; Boo (150 ms early) and Miss
    // too (180 ms late is a Boo).
    let r = play(&song, presets::ddr_extreme(), |i| match i {
        3 => Some(-0.100),
        6 => Some(-0.150),
        9 => Some(0.180),
        10 => None,
        _ => Some(0.0),
    });
    assert_eq!(
        (
            r.tally.count(Judgement::W4),
            r.tally.count(Judgement::W5),
            r.tally.count(Judgement::Miss)
        ),
        (1, 2, 1),
        "{:?}",
        r.tally
    );
    assert_eq!(r.full_combo, FullCombo::None);
    // A failed play is E.
    let r = play(&song, presets::ddr_extreme(), |_| None);
    assert!(r.failed);
    assert_eq!(r.grade, "E");
}

#[test]
fn ddr_extreme_autoplays_all_perfect() {
    for (jump, hold) in [(3, 5), (2, 3), (0, 2)] {
        let song = chart_of(40, jump, hold, 9);
        let seed = (jump, hold);
        let r = autoplayed(&song, presets::ddr_extreme());
        assert!(!r.failed);
        assert_eq!(
            r.tally.count(Judgement::W2),
            r.tally.total_taps(),
            "{seed:?}"
        );
        assert_eq!((r.tally.let_go, r.tally.mine_hit), (0, 0));
        // 9 feet: every step Perfect and freeze O.K. is 90,000,000.
        assert_eq!(r.score.money, Some(90_000_000), "{seed:?}");
        assert_eq!(r.grade, "AAA");
    }
}

#[test]
fn ddr_a_keeps_a_flare_rank_and_flare_skill() {
    let song = chart_of(40, 3, 5, 9);
    // Autoplayed, all Marvelous: every gauge keeps its rank. The chart is
    // a 9 (base 335).
    for (gauge, rank, points) in [
        (DdrGauge::Normal, 0, 335),
        (DdrGauge::Life4, 0, 335),
        (DdrGauge::Risky, 0, 335),
        (DdrGauge::Flare(1), 1, 355),
        (DdrGauge::Flare(FLARE_EX), FLARE_EX, 536),
        (DdrGauge::Floating, FLARE_EX, 536),
    ] {
        let r = autoplayed(&song, ddr_a(gauge, FailType::Immediate));
        assert_eq!(r.tally.count(Judgement::W1), r.tally.total_taps());
        assert_eq!(
            r.ddr,
            Some(DdrClear {
                gauge,
                flare: Some(rank),
                points: Some(points)
            }),
            "{gauge:?}"
        );
        assert_eq!(
            r.ddr.unwrap().life4(),
            matches!(gauge, DdrGauge::Life4 | DdrGauge::Risky)
        );
    }
    // Under the other presets, or a battery, none.
    let r = autoplayed(&song, presets::itg());
    assert_eq!(r.ddr, None);
    let battery = presets::ddr_a().with_stepmania(&StepManiaOptions {
        life_type: LifeType::Battery,
        gauge: DdrGauge::Flare(3),
        ..StepManiaOptions::default()
    });
    assert_eq!(battery.stepmania.gauge, DdrGauge::Normal);
    assert_eq!(autoplayed(&song, battery).ddr, None);
}

#[test]
fn flare_gauges_fail_and_float_in_play() {
    // 60 steps of a 12; 50 ms late is a Great under DDR A (to 91.667 ms).
    let song = chart(60, 0, 12);
    // FLARE IX: the 50th Great fails the play, which ends there.
    let r = play(
        &song,
        ddr_a(DdrGauge::Flare(9), FailType::Immediate),
        |_| Some(0.05),
    );
    assert!(r.failed);
    assert_eq!(r.tally.count(Judgement::W3), 50);
    assert_eq!(r.ddr.unwrap().flare, None);
    assert_eq!(r.ddr.unwrap().points, None);
    // FLARE VIII lives through 60 Greats (1.64% each): VIII kept, 510 ×
    // 1.48 = 754 points.
    let r = play(
        &song,
        ddr_a(DdrGauge::Flare(8), FailType::Immediate),
        |_| Some(0.05),
    );
    assert!(!r.failed);
    assert_eq!(r.ddr.unwrap().flare, Some(8));
    assert_eq!(r.ddr.unwrap().points, Some(754));
    // FLOATING FLARE: four misses, then all Marvelous, end on VII.
    let r = play(&song, ddr_a(DdrGauge::Floating, FailType::Immediate), |i| {
        (i >= 4).then_some(0.0)
    });
    assert!(!r.failed);
    assert_eq!(r.ddr.unwrap().flare, Some(7));
    assert!((r.life.last().unwrap()[1] - 0.12).abs() < 1e-6);
    // Under fail off, a FLARE gauge that emptied keeps no rank, the play
    // no points.
    let r = play(&song, ddr_a(DdrGauge::Flare(9), FailType::Off), |_| {
        Some(0.05)
    });
    assert!(!r.failed);
    assert_eq!(r.tally.count(Judgement::W3), 60);
    assert_eq!(r.ddr.unwrap().flare, None);
}

#[test]
fn life4_and_risky_in_play() {
    let song = chart(20, 0, 5);
    // Goods keep LIFE4's lives (120 ms late: a Good under DDR A).
    let r = play(&song, ddr_a(DdrGauge::Life4, FailType::Immediate), |i| {
        Some(if i < 10 { 0.12 } else { 0.0 })
    });
    assert!(!r.failed);
    assert_eq!(r.tally.count(Judgement::W4), 10);
    assert!(r.ddr.unwrap().life4());
    // Three misses clear; the fourth fails.
    let r = play(&song, ddr_a(DdrGauge::Life4, FailType::Immediate), |i| {
        (i >= 3).then_some(0.0)
    });
    assert!(!r.failed && r.ddr.unwrap().life4());
    let r = play(&song, ddr_a(DdrGauge::Life4, FailType::Immediate), |i| {
        (i >= 4).then_some(0.0)
    });
    assert!(r.failed);
    assert_eq!(r.tally.count(Judgement::Miss), 4);
    assert_eq!(r.tally.total_taps(), 4);
    // RISKY: the first miss.
    let r = play(&song, ddr_a(DdrGauge::Risky, FailType::Immediate), |i| {
        (i != 7).then_some(0.0)
    });
    assert!(r.failed);
    assert_eq!(r.tally.total_taps(), 8);
}

#[test]
fn the_gauge_is_kept_with_the_play_but_not_its_best() {
    let song = chart(8, 0, 5);
    let rules = ddr_a(DdrGauge::Flare(6), FailType::Immediate);
    let key = rules.key();
    assert_eq!(key.stepmania.gauge, DdrGauge::Flare(6));
    let json = serde_json::to_string(&key).unwrap();
    let back: RulesetKey = serde_json::from_str(&json).unwrap();
    assert_eq!(back, key);
    assert_eq!(
        back.build(&song.charts[0]).stepmania.gauge,
        DdrGauge::Flare(6)
    );
    // Bests are kept with every gauge's plays.
    assert_eq!(key.best_key(), presets::ddr_a().key().best_key());
    // Other presets take no DDR gauge: their key keeps the default, and
    // an older key without one reads as the default.
    let itg = presets::itg().with_stepmania(&StepManiaOptions {
        gauge: DdrGauge::Life4,
        ..StepManiaOptions::default()
    });
    assert_eq!(itg.stepmania.gauge, DdrGauge::Normal);
    assert!(itg.key().stepmania.is_default());
    let old: RulesetKey = serde_json::from_str(r#"{"id":"ddr-a","holds":"Dance"}"#).unwrap();
    assert_eq!(old.stepmania.gauge, DdrGauge::Normal);
    // An unknown gauge a later version saved reads as the default.
    let later: StepManiaOptions = serde_json::from_str(r#"{"gauge":"Galaxy","judge":5}"#).unwrap();
    assert_eq!((later.gauge, later.judge), (DdrGauge::Normal, 5));
    // The mods line names the gauge.
    assert_eq!(rules.stepmania.summary(), ["FLARE VI"]);
    assert_eq!(presets::by_id("ddr-extreme").unwrap().id, "ddr-extreme");
}

fn event(kind: JudgeEventKind) -> JudgeEvent {
    JudgeEvent {
        note_index: None,
        lane: 0,
        tick: Tick::ZERO,
        kind,
        song_time: 0.0,
    }
}

#[test]
fn max2_adds_what_the_floor_left_out_on_the_last_step() {
    // A 1-foot song of 2 steps and a freeze: S = 6, floor(1,000,000 / 6)
    // = 166,666; all Perfect and O.K. is 10 × 166,666 × (1 + 2 + 3) and
    // 10 × (1,000,000 − 999,996) on the last: 10,000,000.
    let ctx = ScoreCtx {
        steps: 2,
        holds: 1,
        meter: 1,
        ..ScoreCtx::default()
    };
    let perfect = event(JudgeEventKind::Tap(Judgement::W2, 0.0));
    let mut s = Max2::new();
    s.begin(&ctx);
    s.on_event(&perfect, &ctx);
    s.on_event(&perfect, &ctx);
    assert_eq!(s.view().money, Some(10 * 166_666 * 3));
    s.on_event(&event(JudgeEventKind::Held), &ctx);
    assert_eq!(s.view().money, Some(10_000_000));
    assert_eq!(s.view().percent, Some(1.0));
    // The last an N.G.: no bonus, 0 for it, and 6 of 10 dance points.
    let mut s = Max2::new();
    s.begin(&ctx);
    s.on_event(&perfect, &ctx);
    s.on_event(&perfect, &ctx);
    s.on_event(&event(JudgeEventKind::LetGo), &ctx);
    assert_eq!(s.view().money, Some(10 * 166_666 * 3));
    assert_eq!(s.view().percent, Some(0.4));
    // A meter past 10 counts as 10 (StepMania 3.9's clamp), 0 as 1.
    let mut s = Max2::new();
    s.begin(&ScoreCtx { meter: 14, ..ctx });
    for kind in [
        JudgeEventKind::Tap(Judgement::W1, 0.0),
        JudgeEventKind::Tap(Judgement::W2, 0.0),
        JudgeEventKind::Held,
    ] {
        s.on_event(&event(kind), &ScoreCtx { meter: 14, ..ctx });
    }
    assert_eq!(s.view().money, Some(100_000_000));
}

#[test]
fn ddr_extremes_bar_is_harsh_when_full() {
    // StepMania 3.9's bar: from full, a Boo takes at least 10%, and
    // MercifulDrain scales it by 0.5 + 0.5 × life (full: × 1).
    let rules = presets::ddr_extreme();
    let mut bar = rules.gauge.fresh();
    let ctx = ScoreCtx::default();
    bar.begin(&ctx);
    for _ in 0..70 {
        bar.on_event(&event(JudgeEventKind::Tap(Judgement::W2, 0.0)), &ctx);
    }
    assert_eq!(bar.life(), 1.0);
    bar.on_event(&event(JudgeEventKind::Tap(Judgement::W5, 0.0)), &ctx);
    assert!((bar.life() - 0.9).abs() < 1e-6, "{}", bar.life());
    // Below full: a Boo's 4%, scaled by 0.95.
    bar.on_event(&event(JudgeEventKind::Tap(Judgement::W5, 0.0)), &ctx);
    assert!(
        (bar.life() - (0.9 - 0.04 * 0.95)).abs() < 1e-6,
        "{}",
        bar.life()
    );
}

/// DDR X's rules for `song`'s chart, as a play builds them.
fn ddr_x(song: &ddi_chart::Song) -> Ruleset {
    presets::stepmania("ddr-x", HoldStyle::ByGame, &song.charts[0])
}

#[test]
fn ddr_x_scores_as_supernova2() {
    // 20 steps (every fifth a jump, 24 notes): SC = 50,000.
    let mut song = chart(20, 5, 9);
    song.charts[0].difficulty = Difficulty::Challenge;
    let r = play(&song, ddr_x(&song), |_| Some(0.0));
    assert_eq!(r.score.money, Some(1_000_000));
    assert_eq!((r.score.ex, r.score.max_ex), (None, None));
    assert_eq!(r.tally.count(Judgement::W1), 20);
    assert_eq!(r.full_combo, FullCombo::MarvelousFC);
    // A jump is one step and one to the combo.
    assert_eq!(r.max_combo, 20);
    assert_eq!(r.grade, "AAA");
    // A Perfect (25 ms late) costs 10, a Great (50 ms) SC/2 + 10.
    let r = play(&song, ddr_x(&song), |i| {
        Some(match i {
            0 => 0.025,
            1 => 0.05,
            _ => 0.0,
        })
    });
    assert_eq!(
        (r.tally.count(Judgement::W2), r.tally.count(Judgement::W3)),
        (1, 1)
    );
    assert_eq!(r.score.money, Some(1_000_000 - 10 - 25_010));
    assert_eq!(r.full_combo, FullCombo::GreatFC);
    assert_eq!(r.grade, "AA");
    // A Good (120 ms) scores nothing and breaks the combo, as do an
    // Almost (200 ms early) and a Boo (not pressed), X's miss.
    let r = play(&song, ddr_x(&song), |i| match i {
        6 => Some(0.12),
        12 => Some(-0.2),
        18 => None,
        _ => Some(0.0),
    });
    assert_eq!(
        [Judgement::W4, Judgement::W5, Judgement::Miss].map(|j| r.tally.count(j)),
        [1, 1, 1],
        "{:?}",
        r.tally
    );
    assert_eq!(r.score.money, Some(850_000));
    assert_eq!(r.full_combo, FullCombo::None);
    assert_eq!(r.max_combo, 5);
    // 850,000 on a Challenge chart: B (A from 900,000).
    assert_eq!(r.grade, "B");
    let names = &ddr_x(&song).names;
    assert_eq!(
        [names.name(Judgement::W5), names.name(Judgement::Miss)],
        ["Almost", "Boo"]
    );
    // A failed play is E.
    let r = play(&song, ddr_x(&song), |_| None);
    assert!(r.failed);
    assert_eq!(r.grade, "E");
}

#[test]
fn ddr_x_drops_the_remainder_to_ten() {
    // 3 steps: SC = 333,333.3…; all Marvelous is 1,000,000, a Perfect
    // 999,990, a Great 833,323.3… − its 10, dropped to 833,320.
    let mut song = chart(3, 0, 5);
    song.charts[0].difficulty = Difficulty::Easy;
    assert_eq!(
        play(&song, ddr_x(&song), |_| Some(0.0)).score.money,
        Some(1_000_000)
    );
    let r = play(&song, ddr_x(&song), |i| {
        Some(if i == 0 { 0.025 } else { 0.0 })
    });
    assert_eq!(r.score.money, Some(999_990));
    let r = play(&song, ddr_x(&song), |i| {
        Some(if i == 2 { 0.05 } else { 0.0 })
    });
    assert_eq!(r.score.money, Some(833_320));
    // 833,320 on a Basic chart: A (from 800,000).
    assert_eq!(r.grade, "A");
}

#[test]
fn ddr_x_grades_by_the_charts_difficulty() {
    use ddi_engine::rules::ScoreView;
    let tally = {
        let mut t = ddi_engine::rules::Tally::default();
        t.taps[2] = 1;
        t
    };
    let grade = |difficulty, money: u64| {
        presets::ddr_x_grades(difficulty).grade(
            &ScoreView {
                money: Some(money),
                ..ScoreView::default()
            },
            &tally,
            false,
        )
    };
    // RemyWiki's and BEMANIWiki's table: AAA and AA the same everywhere,
    // A, B and C by the difficulty.
    for (difficulty, [a, b, c]) in [
        (Difficulty::Beginner, [800_000, 700_000, 500_000]),
        (Difficulty::Easy, [800_000, 700_000, 500_000]),
        (Difficulty::Medium, [850_000, 750_000, 600_000]),
        (Difficulty::Hard, [900_000, 800_000, 700_000]),
        (Difficulty::Challenge, [900_000, 800_000, 700_000]),
        (Difficulty::Edit, [900_000, 800_000, 700_000]),
    ] {
        for (money, expected) in [
            (1_000_000, "AAA"),
            (990_000, "AAA"),
            (989_990, "AA"),
            (950_000, "AA"),
            (949_990, "A"),
            (a, "A"),
            (a - 10, "B"),
            (b, "B"),
            (b - 10, "C"),
            (c, "C"),
            (c - 10, "D"),
            (0, "D"),
        ] {
            assert_eq!(grade(difficulty, money), expected, "{difficulty:?} {money}");
        }
        assert_eq!(
            presets::ddr_x_grades(difficulty).grade(&ScoreView::default(), &tally, true),
            "E"
        );
    }
    // A play's rules see its chart's difficulty, as does its key's.
    let mut song = chart(4, 0, 3);
    song.charts[0].difficulty = Difficulty::Beginner;
    let rules = ddr_x(&song);
    assert_eq!(rules.grades, presets::ddr_x_grades(Difficulty::Beginner));
    assert_eq!(rules.key().build(&song.charts[0]).grades, rules.grades);
    assert_eq!(
        presets::for_chart(
            RulesetMode::AllStepMania,
            "ddr-x",
            &Default::default(),
            HoldStyle::ByGame,
            &song.charts[0]
        )
        .grades,
        rules.grades
    );
    assert_eq!(presets::by_id("ddr-x").unwrap().id, "ddr-x");
    // In the list, after DDR A.
    let ids: Vec<_> = presets::all().into_iter().map(|r| r.id).collect();
    assert_eq!(ids[3..5], ["ddr-a", "ddr-x"]);
}

#[test]
fn ddr_x_autoplays_all_marvelous() {
    for (jump, hold) in [(3, 5), (2, 3), (0, 2)] {
        let song = chart_of(40, jump, hold, 9);
        let seed = (jump, hold);
        let r = autoplayed(&song, ddr_x(&song));
        assert!(!r.failed);
        assert_eq!(
            r.tally.count(Judgement::W1),
            r.tally.total_taps(),
            "{seed:?}"
        );
        assert_eq!((r.tally.let_go, r.tally.mine_hit), (0, 0));
        assert_eq!(r.score.money, Some(1_000_000), "{seed:?}");
        assert_eq!(r.full_combo, FullCombo::MarvelousFC);
        assert_eq!(r.grade, "AAA");
        // A freeze held adds nothing to the combo.
        assert_eq!(r.max_combo, r.tally.total_taps(), "{seed:?}");
        assert_eq!(r.ddr, None);
    }
}

#[test]
fn ddr_x_freezes_and_shocks() {
    // 8 steps, every fourth a two-beat freeze; a shock (a full row of
    // mines) at beat 12: SC = 1,000,000 / (8 + 2 + 1).
    let mut song = chart_of(8, 0, 4, 9);
    let notes = &mut song.charts[0].notes;
    for n in notes.iter_mut() {
        if let NoteKind::HoldHead { end } = &mut n.kind {
            *end = Tick(n.tick.0 + 96);
        }
    }
    notes.extend((0..4).map(|lane| Note::new(Tick::from_beats(12), lane, NoteKind::Mine)));
    notes.sort_by_key(|n| (n.tick, n.lane));
    // Autoplayed: the shock avoided, an O.K. adding one to the combo.
    let r = autoplayed(&song, ddr_x(&song));
    assert_eq!(r.score.money, Some(1_000_000));
    assert_eq!(r.tally.held, 3);
    assert_eq!(r.max_combo, 9);
    // Each freeze let go at once and the shock stepped on: three N.G.s
    // for nothing, each breaking the combo.
    let mut edges: Vec<(f64, u8, bool)> = song.charts[0]
        .notes
        .iter()
        .filter(|n| n.kind != NoteKind::Mine)
        .flat_map(|n| {
            let t = n.tick.beat() / 2.0;
            [(t, n.lane, true), (t + 0.03, n.lane, false)]
        })
        .collect();
    edges.extend([(6.0, 1, true), (6.03, 1, false)]);
    let r = run(&song, ddr_x(&song), edges);
    assert_eq!((r.tally.held, r.tally.let_go), (0, 3));
    assert_eq!(r.score.money, Some(8 * 1_000_000 / 11 / 10 * 10));
    assert_eq!(r.full_combo, FullCombo::None);
    assert_eq!(r.max_combo, 4);
}

#[test]
fn ddr_xs_bar_shows_no_danger() {
    let rules = ddr_x(&chart(4, 0, 1));
    let mut bar = rules.gauge.fresh();
    let ctx = ScoreCtx::default();
    bar.begin(&ctx);
    assert_eq!(bar.life(), 0.5);
    for _ in 0..7 {
        bar.on_event(&event(JudgeEventKind::Tap(Judgement::Miss, 0.0)), &ctx);
        assert!(!bar.danger());
    }
    assert!(bar.life() < 0.2 && !bar.failed());
    assert_eq!(bar.danger_line(), None);
    // A Good leaves it as it is.
    let life = bar.life();
    bar.on_event(&event(JudgeEventKind::Tap(Judgement::W4, 0.0)), &ctx);
    assert_eq!(bar.life(), life);
}
