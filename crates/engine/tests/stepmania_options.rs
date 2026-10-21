//! The StepMania presets' player options (`docs/plans/rules-options.md`):
//! the fail type, life and judge difficulty, and that a play's key builds
//! the same rules again, so its replay judges the same.

mod common;

use ddi_chart::{Note, NoteKind, Tick, TimingMap};
use ddi_engine::history::replay;
use ddi_engine::player::{PlayOptions, Player, Results};
use ddi_engine::rules::danoni::DanoniOptions;
use ddi_engine::rules::presets::{self, HoldStyle, RulesetMode};
use ddi_engine::rules::{
    FailPolicy, FailType, JUDGE_SCALES, Judgement, LIFE_SCALES, Ruleset, RulesetKey,
    StepManiaOptions,
};

/// Taps one a beat at 120 BPM (beat `i` at `i / 2` s), lanes in turn.
fn taps(n: i64) -> ddi_chart::Song {
    let notes = (0..n)
        .map(|i| Note::new(Tick::from_beats(i), (i % 4) as u8, NoteKind::Tap))
        .collect();
    common::song("dance-single", TimingMap::constant(120.0, 0.0), notes)
}

fn options(fail: FailType, life: u8, judge: u8) -> StepManiaOptions {
    StepManiaOptions {
        fail,
        life,
        judge,
        ..StepManiaOptions::default()
    }
}

fn itg(o: StepManiaOptions) -> Ruleset {
    presets::itg().with_stepmania(&o)
}

/// Plays `song`, pressing note `i` on time when `hit(i)` (the rest
/// missed), offset by `late` seconds; to the end.
fn play(song: &ddi_chart::Song, rules: Ruleset, hit: impl Fn(usize) -> bool, late: f64) -> Player {
    let mut p = Player::new(song, 0, rules, PlayOptions::default());
    p.start(0.0, 0.0);
    for (i, n) in song.charts[0].notes.iter().enumerate() {
        if hit(i) {
            let t = n.tick.beat() / 2.0 + late;
            p.input_at(n.lane, true, t);
            p.input_at(n.lane, false, t + 0.05);
        }
    }
    p.update_at(1e6);
    p
}

/// The play judged again from its log and its rules' key.
fn replayed(song: &ddi_chart::Song, p: &Player) -> Results {
    replay(
        song,
        0,
        &p.ruleset().key(),
        &PlayOptions::default(),
        p.log(),
    )
}

#[test]
fn fail_types_as_stepmania_has_them() {
    let song = taps(20);
    // Eight misses empty ITG's bar (0.5, −0.1 a miss, the regen lockout),
    // then twelve Fantastics.
    let first_eight_missed = |i: usize| i >= 8;
    let run = |fail| {
        let p = play(&song, itg(options(fail, 4, 4)), first_eight_missed, 0.0);
        let r = p.results();
        assert_eq!(replayed(&song, &p), r, "{fail:?}");
        r
    };

    // Immediate: failed and ended at the fifth miss (beat 4, 2 s, judged
    // once the Way Off window, 181.5 ms, has passed).
    let r = run(FailType::Immediate);
    assert!(r.failed);
    assert_eq!(r.tally.count(Judgement::Miss), 5);
    assert_eq!(r.tally.total_taps(), 5);
    let emptied = r.gauge_failed_at.expect("the gauge emptied");
    assert!((emptied - 2.1815).abs() < 1e-3, "{emptied}");

    // Delayed: failed there too, played on, and the bar stays empty.
    let r = run(FailType::ImmediateContinue);
    assert!(r.failed);
    assert_eq!(r.grade, "F");
    assert_eq!(r.tally.total_taps(), 20);
    assert_eq!(r.tally.count(Judgement::W1), 12);
    assert_eq!(r.life.last().map(|l| l[1]), Some(0.0));
    assert_eq!(r.gauge_failed_at, Some(emptied));

    // Fail at end: the bar came back, so the play passes.
    let r = run(FailType::EndOfSong);
    assert!(!r.failed);
    assert_eq!(r.tally.total_taps(), 20);
    assert!(r.life.last().unwrap()[1] > 0.0);
    assert_eq!(r.gauge_failed_at, Some(emptied));

    // Off: never failed.
    let r = run(FailType::Off);
    assert!(!r.failed);
    assert_eq!(r.gauge_failed_at, Some(emptied));
    assert_ne!(r.grade, "F");

    // Fail at end with the bar empty at the end: failed, once it ends.
    let p = play(
        &song,
        itg(options(FailType::EndOfSong, 4, 4)),
        |_| false,
        0.0,
    );
    assert!(p.failed() && p.finished());
    assert_eq!(p.tally().count(Judgement::Miss), 20);
    let p = play(&song, itg(options(FailType::Off, 4, 4)), |_| false, 0.0);
    assert!(!p.failed() && p.finished());
}

#[test]
fn life_difficulty_scales_the_life_bar() {
    let song = taps(20);
    let misses_to_fail = |life| {
        let p = play(
            &song,
            itg(options(FailType::Immediate, life, 4)),
            |_| false,
            0.0,
        );
        assert!(p.failed());
        let r = p.results();
        assert_eq!(replayed(&song, &p), r);
        r.tally.count(Judgement::Miss)
    };
    // −0.1 a miss from 0.5, divided by the scale.
    assert_eq!(misses_to_fail(4), 5);
    assert_eq!(misses_to_fail(7), 2); // ÷0.4: −0.25
    assert_eq!(misses_to_fail(1), 8); // ÷1.6: −0.0625
    // Gains multiplied: one Fantastic is +0.008 × 1.6.
    let p = play(&song, itg(options(FailType::Off, 1, 4)), |i| i == 0, 0.0);
    let after_first = p.results().life[1][1];
    assert!(
        (after_first - (0.5 + 0.008 * 1.6)).abs() < 1e-6,
        "{after_first}"
    );
    assert_eq!(LIFE_SCALES[3], 1.0);
}

#[test]
fn judge_difficulty_scales_every_window() {
    let song = taps(8);
    let judged = |judge, late| {
        let p = play(&song, itg(options(FailType::Off, 4, judge)), |_| true, late);
        let r = p.results();
        assert_eq!(replayed(&song, &p), r);
        r
    };
    // ITG's Fantastic: 21.5 ms × scale + 1.5 ms.
    assert_eq!(judged(4, 0.010).tally.count(Judgement::W1), 8);
    assert_eq!(judged(8, 0.010).tally.count(Judgement::W2), 8);
    assert_eq!(judged(1, 0.030).tally.count(Judgement::W1), 8);
    // Justice, 0.20: a Fantastic within 5.8 ms.
    let r = judged(9, 0.005);
    assert_eq!(r.tally.count(Judgement::W1), 8);
    let r = judged(9, 0.007);
    assert_eq!(r.tally.count(Judgement::W2), 8);
    let rules = itg(options(FailType::Immediate, 4, 9));
    assert!(
        (rules.judge.window(Judgement::W1).unwrap().late - (0.0215 * 0.2 + 0.0015)).abs() < 1e-12
    );
    assert!((rules.judge.mine_window_seconds() - (0.07 * 0.2 + 0.0015)).abs() < 1e-12);
    assert_eq!(JUDGE_SCALES[3], 1.0);
    assert_eq!(StepManiaOptions::judge_name(9), "Justice");
}

#[test]
fn keys_keep_the_options_and_bests_only_the_judge() {
    let o = options(FailType::Off, 6, 5);
    let rules = itg(o);
    let key = rules.key();
    assert_eq!(key.stepmania, o);
    assert_eq!(rules.fail, FailPolicy::Off);
    // Built again from the key, through JSON as a record keeps it.
    let back: RulesetKey = serde_json::from_str(&serde_json::to_string(&key).unwrap()).unwrap();
    assert_eq!(back, key);
    let chart = &taps(1).charts[0];
    let again = back.build(chart);
    assert_eq!(again.stepmania, o);
    assert_eq!(again.judge.scale, rules.judge.scale);
    assert_eq!(again.fail, FailPolicy::Off);
    // Bests: the judge level only.
    assert_eq!(key.best_key().stepmania, options(FailType::Immediate, 4, 5));
    // The defaults are not written, so keys kept before them read the
    // same and write the same.
    let plain = presets::itg().key();
    let json = serde_json::to_string(&plain).unwrap();
    assert!(!json.contains("stepmania"), "{json}");
    let old: RulesetKey =
        serde_json::from_str(r#"{"id":"itg","holds":"Dance","danoni":null}"#).unwrap();
    assert_eq!(old, plain);
    // Levels out of range are clamped.
    let wild = options(FailType::Immediate, 0, 200).sanitized();
    assert_eq!((wild.life, wild.judge), (1, 9));
    assert_eq!(
        o.summary(),
        vec!["fail off".to_string(), "life 6".into(), "judge 5".into()]
    );
    assert!(StepManiaOptions::default().summary().is_empty());
}

#[test]
fn other_rules_keep_their_own() {
    let o = options(FailType::Off, 7, 8);
    let chart = &taps(1).charts[0];
    // Dancing☆Onigiri's gauges and judge range are its own.
    let danoni = presets::for_chart(
        RulesetMode::AllDanoni,
        "itg",
        &DanoniOptions::default(),
        HoldStyle::ByGame,
        chart,
    )
    .with_stepmania(&o);
    assert!(danoni.stepmania.is_default());
    assert_eq!(danoni.judge.scale, 1.0);
    assert!(danoni.key().stepmania.is_default());
    // Calibration ignores them.
    let cal = presets::calibration().with_stepmania(&o);
    assert_eq!(cal.fail, FailPolicy::Off);
    assert_eq!(cal.judge.scale, 1.0);
    assert!(cal.key().stepmania.is_default());
    // DDR A's gauge is the arcade's: no life level; the rest applies.
    let ddr = presets::ddr_a().with_stepmania(&o);
    assert_eq!(ddr.stepmania, options(FailType::Off, 4, 8));
    assert_eq!(ddr.fail, FailPolicy::Off);
    assert!((ddr.judge.scale - 0.33).abs() < 1e-12);
}

/// ITG FA+: judged and scored as ITG, every Fantastic within 15 ms a blue
/// one, and Simply Love's EX score.
#[test]
fn fa_plus_splits_the_fantastics_and_scores_ex() {
    let song = taps(8);
    let late = [0.0, 0.010, 0.014, 0.016, 0.020, 0.030, 0.0, -0.012];
    let run = |rules: Ruleset| {
        let mut p = Player::new(&song, 0, rules, PlayOptions::default());
        p.start(0.0, 0.0);
        let mut flashes = Vec::new();
        for (n, d) in song.charts[0].notes.iter().zip(late) {
            let t = n.tick.beat() / 2.0 + d;
            p.input_at(n.lane, true, t);
            flashes.push(
                p.frame(ddi_platform::HostTime(0.0))
                    .judgement
                    .and_then(|f| f.w0),
            );
            p.input_at(n.lane, false, t + 0.05);
        }
        p.update_at(1e6);
        let r = p.results();
        assert_eq!(replayed(&song, &p), r);
        (r, flashes)
    };
    let (r, flashes) = run(presets::itg_fa_plus());
    assert_eq!(r.tally.count(Judgement::W1), 7);
    assert_eq!(r.tally.count(Judgement::W2), 1);
    assert_eq!(r.tally.w0, 5);
    // 5 blue (7 halves) + 2 white (6) + an Excellent (4), of 8 × 7.
    assert_eq!(r.score.ex_percent, Some(0.9107));
    assert_eq!((r.score.ex, r.score.max_ex), (None, None));
    // The percentage and grade are ITG's: 7 × 5 + 4 of 8 × 5.
    assert_eq!(r.score.percent, Some(39.0 / 40.0));
    let (itg, _) = run(presets::itg());
    assert_eq!(itg.score.percent, r.score.percent);
    assert_eq!(itg.grade, r.grade);
    assert_eq!((itg.tally.w0, itg.score.ex_percent), (0, None));
    // The HUD's flash: blue, white, or not split (the Excellent).
    assert_eq!(
        flashes,
        [true, true, true, false, false, false, true, true].map(Some)
    );
    // The judge level scales the FA+ window too: at 8, 13.5 × 0.33 + 1.5.
    let strict = presets::itg_fa_plus().with_stepmania(&options(FailType::Immediate, 4, 8));
    assert_eq!(strict.judge.is_w0(Judgement::W1, 0.0059), Some(true));
    assert_eq!(strict.judge.is_w0(Judgement::W1, 0.0060), Some(false));
    assert_eq!(presets::itg().judge.is_w0(Judgement::W1, 0.0), None);
    // A clean play: 100% EX.
    let song = taps(4);
    let mut p = Player::new(&song, 0, presets::itg_fa_plus(), PlayOptions::default());
    p.start(0.0, 0.0);
    for n in &song.charts[0].notes {
        p.input_at(n.lane, true, n.tick.beat() / 2.0);
    }
    p.update_at(1e6);
    assert_eq!(p.results().score.ex_percent, Some(1.0));
    assert_eq!(presets::by_id("itg-fa+").unwrap().id, "itg-fa+");
}

/// StepMania's battery: a life lost for each tap worse than a Great, the
/// play failed with the last; no life level; bests with the bar's.
#[test]
fn the_battery_loses_a_life_a_miss() {
    use ddi_engine::rules::LifeType;
    let song = taps(12);
    let battery = |lives, life| StepManiaOptions {
        life_type: LifeType::Battery,
        lives,
        life,
        ..StepManiaOptions::default()
    };
    // Every other note missed: two lives last four notes.
    let p = play(&song, itg(battery(2, 7)), |i| i % 2 == 1, 0.0);
    let r = p.results();
    assert!(r.failed);
    assert_eq!(r.tally.count(Judgement::Miss), 2);
    assert_eq!(r.tally.count(Judgement::W1), 1);
    assert_eq!(replayed(&song, &p), r);
    assert_eq!(r.life.last().map(|l| l[1]), Some(0.0));
    // Greats keep their lives (60 ms late: ITG's Great).
    let p = play(&song, itg(battery(1, 4)), |_| true, 0.06);
    let r = p.results();
    assert_eq!(r.tally.count(Judgement::W3), 12);
    assert!(!r.failed);
    assert_eq!(r.life.last().map(|l| l[1]), Some(1.0));
    // The key: lives kept, the life level not (no bar to scale); the best
    // key as the bar's.
    let rules = itg(battery(3, 7));
    assert_eq!(rules.stepmania, battery(3, 4));
    assert!(rules.gauge.danger_line().is_none());
    assert_eq!(rules.key().best_key(), presets::itg().key());
    assert_eq!(battery(3, 4).summary(), vec!["battery 3 lives".to_string()]);
    assert_eq!(battery(1, 4).summary(), vec!["battery 1 life".to_string()]);
    // Under the bar, lives are not kept.
    let bar = itg(StepManiaOptions {
        lives: 9,
        ..StepManiaOptions::default()
    });
    assert!(bar.stepmania.is_default());
    assert_eq!(battery(0, 4).sanitized().lives, 1);
}
