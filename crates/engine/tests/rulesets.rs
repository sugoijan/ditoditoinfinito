//! Phase 10's rulesets (`docs/plans/rulesets.md`): DDR EXTREME's score,
//! grades and combo, DDR A's other gauges (LIFE4, RISKY, FLARE, FLOATING
//! FLARE) with the FLARE rank and FLARE SKILL a play keeps, and that each
//! play's key builds the same rules again, so its replay judges the same.

mod common;

use ddi_chart::{Note, NoteKind, Tick, TimingMap};
use ddi_engine::autoplay;
use ddi_engine::history::replay;
use ddi_engine::player::{PlayOptions, Player, Results};
use ddi_engine::rules::ddr::{DdrClear, DdrGauge, FLARE_EX};
use ddi_engine::rules::presets;
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
