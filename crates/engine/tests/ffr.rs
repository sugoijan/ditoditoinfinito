//! FFR's rules (`docs/plans/rulesets.md`, "FFR"): R^3's windows, its
//! miss at the sixth frame, Boos from the first note on, the raw score,
//! the life and what a play comes to, taps only, the keys; every play
//! judged again from its key and log to the same results.

mod common;

use ddi_chart::formats::ffr::{LevelInfo, parse_beatbox_text, song_from_beatbox};
use ddi_chart::{Note, NoteKind, Song, Tick, TimingMap};
use ddi_engine::autoplay;
use ddi_engine::history::replay;
use ddi_engine::player::{PlayOptions, Player, Results};
use ddi_engine::rules::danoni::DanoniOptions;
use ddi_engine::rules::ffr::{FfrLife, Flag};
use ddi_engine::rules::presets::{self, HoldStyle, RulesetMode};
use ddi_engine::rules::{
    FailPolicy, FailType, FullCombo, Judgement, LifeType, Ruleset, RulesetKey, StepManiaOptions,
};
use ddi_engine::{JudgeEvent, JudgeEventKind};

/// R^3's frame.
const FRAME: f64 = 1.0 / 30.0;

/// Taps at `(beat, lane)`, 120 BPM: beat `b` at `b / 2` s.
fn taps(at: &[(f64, u8)]) -> Song {
    let notes = at
        .iter()
        .map(|&(beat, lane)| Note::new(Tick((beat * 48.0).round() as i64), lane, NoteKind::Tap))
        .collect();
    common::song("dance-single", TimingMap::constant(120.0, 0.0), notes)
}

/// Plays `song` under `rules` with `edges` (`(song seconds, lane,
/// pressed)`) to the end; its events in order, and its key and log
/// judged again to the same results.
fn run(song: &Song, rules: Ruleset, mut edges: Vec<(f64, u8, bool)>) -> (Results, Vec<JudgeEvent>) {
    edges.sort_by(|a, b| a.0.total_cmp(&b.0).then(a.2.cmp(&b.2)));
    let mut p = Player::new(song, 0, rules, PlayOptions::default());
    p.start(0.0, 0.0);
    let mut events = Vec::new();
    for (t, lane, pressed) in edges {
        events.extend(p.update_at(t));
        events.extend(p.input_at(lane, pressed, t));
    }
    events.extend(p.update_at(1e6));
    let r = p.results();
    let key = p.ruleset().key();
    assert_eq!(
        replay(song, 0, &key, &PlayOptions::default(), p.log()),
        r,
        "{key:?}"
    );
    (r, events)
}

/// A press and its release 30 ms later.
fn tap(t: f64, lane: u8) -> [(f64, u8, bool); 2] {
    [(t, lane, true), (t + 0.03, lane, false)]
}

fn autoplayed(song: &Song, rules: Ruleset) -> Results {
    let probe = Player::new(song, 0, rules.fresh(), PlayOptions::default());
    let edges = autoplay::edges(autoplay::script(probe.judge().notes(), 0.0));
    run(song, rules, edges).0
}

#[test]
fn windows_are_r3s_default_judge() {
    // A first note hit on time (Boos count from it on), then the note
    // under test at 1 s, pressed half a millisecond inside or outside
    // each bound (an exact bound is a float's rounding away).
    let song = taps(&[(1.0, 1), (2.0, 0)]);
    let cases = [
        (-0.0175, Some(Judgement::W1)),
        (0.0165, Some(Judgement::W1)),
        (-0.0185, Some(Judgement::W2)),
        (0.0175, Some(Judgement::W2)),
        (-0.0505, Some(Judgement::W2)),
        (0.0495, Some(Judgement::W2)),
        (-0.0515, Some(Judgement::W3)),
        (-0.0845, Some(Judgement::W3)),
        (0.0505, Some(Judgement::W3)),
        (0.1165, Some(Judgement::W3)),
        // Average: early only.
        (-0.0855, Some(Judgement::W4)),
        (-0.1175, Some(Judgement::W4)),
        // Outside: a Boo, and the note missed.
        (-0.1185, None),
        (0.1175, None),
        (0.180, None),
    ];
    for (off, want) in cases {
        let mut edges = tap(0.5, 1).to_vec();
        edges.extend(tap(1.0 + off, 0));
        let (r, _) = run(&song, presets::ffr(), edges);
        let t = &r.tally;
        match want {
            Some(j) => {
                let first = u32::from(j == Judgement::W1);
                assert_eq!(t.count(j), 1 + first, "{off}: {t:?}");
                assert_eq!((t.boo, t.count(Judgement::Miss)), (0, 0), "{off}");
            }
            None => {
                assert_eq!(t.count(Judgement::W1), 1, "{off}: {t:?}");
                assert_eq!((t.boo, t.count(Judgement::Miss)), (1, 1), "{off}: {t:?}");
            }
        }
    }
}

#[test]
fn a_note_is_missed_at_its_sixth_frame_and_a_late_press_goes_on() {
    let rules = presets::ffr();
    assert!((rules.judge.miss_after() - 5.5 * FRAME).abs() < 1e-12);
    // Not pressed: missed 183.3 ms after it, not as the hits end.
    let song = taps(&[(2.0, 0)]);
    let (r, events) = run(&song, presets::ffr(), Vec::new());
    let miss: Vec<_> = events
        .iter()
        .filter(|e| matches!(e.kind, JudgeEventKind::Tap(Judgement::Miss, _)))
        .collect();
    assert_eq!(miss.len(), 1);
    assert!((miss[0].song_time - (1.0 + 5.5 * FRAME)).abs() < 1e-9);
    assert_eq!(r.score.money, Some(0));
    // A jack: a press 150 ms after the first note is past its hits and
    // takes the second (25 ms late); the first is missed after it, so
    // the combo ends broken.
    let song = taps(&[(2.0, 0), (2.25, 0)]);
    let (r, _) = run(&song, presets::ffr(), tap(1.15, 0).to_vec());
    assert_eq!(r.tally.count(Judgement::W2), 1);
    assert_eq!(r.tally.count(Judgement::Miss), 1);
    assert_eq!(r.tally.boo, 0);
    let order: Vec<_> = r
        .timeline
        .iter()
        .filter_map(|j| match j.kind {
            JudgeEventKind::Tap(t, _) => Some(t),
            _ => None,
        })
        .collect();
    assert_eq!(order, [Judgement::W2, Judgement::Miss]);
    assert_eq!(r.max_combo, 1);
}

#[test]
fn boos_count_from_the_first_note_and_keep_the_combo() {
    let song = taps(&[(2.0, 0), (4.0, 1)]);
    let mut edges = Vec::new();
    // Before the first note: nothing.
    edges.extend(tap(0.2, 2));
    edges.extend(tap(0.999, 3));
    // From it on: Boos, between the notes too.
    edges.extend(tap(1.0, 0));
    edges.extend(tap(1.0, 2));
    edges.extend(tap(1.5, 3));
    edges.extend(tap(2.0, 1));
    let (r, events) = run(&song, presets::ffr(), edges);
    assert_eq!(r.tally.boo, 2);
    assert_eq!(
        events
            .iter()
            .filter(|e| e.kind == JudgeEventKind::Boo)
            .map(|e| e.song_time)
            .collect::<Vec<_>>(),
        [1.0, 1.5]
    );
    // Two Amazings, two Boos: 100 − 10 raw; the combo unbroken.
    assert_eq!(r.score.money, Some(90));
    assert_eq!(r.max_combo, 2);
    assert_eq!(r.full_combo, FullCombo::MarvelousFC);
    // The life: 50 + 5 − 5 − 5 + 5.
    assert_eq!(r.life.last().map(|l| l[1]), Some(0.5));
    assert_eq!(r.ffr.unwrap().flag, Flag::SingleDigitGoods);
}

#[test]
fn ten_losses_from_the_start_end_the_play() {
    // Ten notes not pressed: 50 − 10 × 5, failed at the tenth Miss.
    let at: Vec<_> = (0..12).map(|i| (2.0 + f64::from(i), 0)).collect();
    let song = taps(&at);
    let (r, _) = run(&song, presets::ffr(), Vec::new());
    assert!(r.failed);
    assert_eq!(r.tally.count(Judgement::Miss), 10);
    let tenth = 1.0 + 9.0 * 0.5 + 5.5 * FRAME;
    assert!((f64::from(r.gauge_failed_at.unwrap()) - tenth).abs() < 1e-4);
    // Boos fail too, and hits refill at most to 100.
    let mut edges = Vec::new();
    for i in 0..12 {
        edges.extend(tap(1.0 + 0.5 * f64::from(i), 0));
    }
    let (r, _) = run(&song, presets::ffr(), edges.clone());
    assert!(!r.failed);
    assert_eq!(r.life.last().map(|l| l[1]), Some(1.0));
    // Boos after a hit: 55, failed at the eleventh.
    let song = taps(&[(2.0, 0), (40.0, 0)]);
    let mut edges = tap(1.0, 0).to_vec();
    for i in 0..19 {
        edges.extend(tap(1.1 + 0.05 * f64::from(i), 2));
    }
    let (r, _) = run(&song, presets::ffr(), edges);
    assert!(r.failed);
    let boos = (FfrLife::START + FfrLife::STEP) / FfrLife::STEP;
    assert_eq!(r.tally.boo, boos as u32);
    assert_eq!(boos, 11);
}

#[test]
fn autoplay_is_aaa_and_r3s_totals() {
    let song = common::chart(7, "dance-single");
    let r = autoplayed(&song, presets::ffr());
    let n = r.tally.count(Judgement::W1);
    assert!(n > 50, "{n}");
    assert_eq!(r.tally.total_taps(), n);
    assert_eq!((r.tally.boo, r.tally.held, r.tally.mine_hit), (0, 0, 0));
    assert_eq!(r.score.money, Some(50 * u64::from(n)));
    let ffr = r.ffr.unwrap();
    assert_eq!(ffr.flag, Flag::Aaa);
    // R^3's playlist gives a level's best total as its notes × 1,550.
    assert_eq!(ffr.total, 1550 * u64::from(n));
    assert_eq!(ffr.raw_goods_text(), "0.0");
    assert_eq!(r.grade, "");
    assert!(!r.failed);
}

#[test]
fn only_taps_are_played() {
    let t = |beat: i64| Tick::from_beats(beat);
    let notes = vec![
        Note::new(t(1), 0, NoteKind::HoldHead { end: t(2) }),
        Note::new(Tick(72), 2, NoteKind::Mine),
        Note::new(t(2), 1, NoteKind::RollHead { end: t(3) }),
        Note::new(t(3), 3, NoteKind::Lift),
        Note::new(t(4), 0, NoteKind::Tap),
        Note::new(t(4), 3, NoteKind::Fake),
    ];
    let song = common::song("dance-single", TimingMap::constant(120.0, 0.0), notes);
    let p = Player::new(&song, 0, presets::ffr(), PlayOptions::default());
    let kinds: Vec<_> = p.judge().notes().iter().map(|n| n.note.kind).collect();
    assert_eq!(kinds, [NoteKind::Tap; 3]);
    let ctx = p.score_ctx();
    assert_eq!((ctx.steps, ctx.holds, ctx.mines), (3, 0, 0));
    let r = autoplayed(&song, presets::ffr());
    assert_eq!(r.tally.count(Judgement::W1), 3);
    assert_eq!(r.ffr.unwrap().flag, Flag::Aaa);
    // Pressing down through where the mine was costs nothing.
    let (r, _) = run(
        &song,
        presets::ffr(),
        vec![
            (0.5, 0, true),
            (0.7, 2, true),
            (0.8, 2, false),
            (1.0, 0, false),
        ],
    );
    assert_eq!(r.tally.mine_hit, 0);
    // The same chart's holds as ticked: still taps, and the key says
    // dance (nothing ticked).
    let ticked = presets::stepmania("ffr", HoldStyle::Ticked, &song.charts[0]);
    assert_eq!(ticked.key(), presets::ffr().key());
}

#[test]
fn keys_build_ffr_and_its_options_are_its_own() {
    let song = taps(&[(2.0, 0)]);
    let key = presets::ffr().key();
    assert_eq!(
        key,
        RulesetKey {
            id: "ffr".into(),
            holds: HoldStyle::Dance,
            danoni: None,
            stepmania: StepManiaOptions::default(),
        }
    );
    assert_eq!(key.build(&song.charts[0]).id, "ffr");
    assert_eq!(key.best_key(), key);
    assert_eq!(presets::by_id("ffr").unwrap().name, "FFR (R^3)");
    assert_eq!(presets::all().last().unwrap().id, "ffr");
    // The StepMania options do not apply.
    let rules = presets::ffr().with_stepmania(&StepManiaOptions {
        fail: FailType::Off,
        life: 7,
        judge: 9,
        life_type: LifeType::Battery,
        lives: 1,
        ..StepManiaOptions::default()
    });
    assert_eq!(rules.fail, FailPolicy::Immediate);
    assert_eq!(rules.judge.scale, 1.0);
    assert_eq!(rules.key(), key);
    assert!(!rules.is_stepmania());
    // An FFR level by its game's rules where each chart is left to its
    // game; the preset chosen otherwise.
    let mut level = song.charts[0].clone();
    level.ffr = true;
    let pick = |mode, chart: &ddi_chart::Chart| {
        presets::for_chart(
            mode,
            "sm5",
            &DanoniOptions::default(),
            HoldStyle::ByGame,
            chart,
        )
        .id
    };
    assert_eq!(pick(RulesetMode::Original, &level), "ffr");
    assert_eq!(pick(RulesetMode::Original, &song.charts[0]), "sm5");
    assert_eq!(pick(RulesetMode::AllStepMania, &level), "sm5");
    assert_eq!(pick(RulesetMode::AllDanoni, &level), "danoni");
}

#[test]
fn an_imported_level_plays_by_its_frames() {
    // Notes at frames 30, 45, 60 and 61 (two lanes), the last by its
    // milliseconds.
    let text = r#"_root.beatBox = [[30, "L", "red"], [45, "D", "blue"], [60, "U", "red"],
        [61, "R", "white", 2050]];"#;
    let song =
        song_from_beatbox(&parse_beatbox_text(text).unwrap(), &LevelInfo::default()).unwrap();
    assert!(song.charts[0].ffr);
    let rules = presets::for_chart(
        RulesetMode::Original,
        "itg",
        &DanoniOptions::default(),
        HoldStyle::ByGame,
        &song.charts[0],
    );
    assert_eq!(rules.id, "ffr");
    let r = autoplayed(&song, rules);
    assert_eq!(r.tally.count(Judgement::W1), 4);
    assert_eq!(r.ffr.unwrap().flag, Flag::Aaa);
    // A frame late on the third note: a Perfect (+33.3 ms), still AAA;
    // two frames late a Good (+66.7 ms), a Blackflag.
    let p = Player::new(&song, 0, presets::ffr(), PlayOptions::default());
    let times: Vec<_> = p
        .judge()
        .notes()
        .iter()
        .map(|n| (n.seconds, n.note.lane))
        .collect();
    assert_eq!(times[3], (2.05, 3));
    for (late, flag) in [(1.0, Flag::Aaa), (2.0, Flag::Blackflag)] {
        let mut edges = Vec::new();
        for (i, &(s, lane)) in times.iter().enumerate() {
            let off = if i == 2 { late * FRAME } else { 0.0 };
            edges.extend(tap(s + off, lane));
        }
        let (r, _) = run(&song, presets::ffr(), edges);
        assert_eq!(r.ffr.unwrap().flag, flag, "{late}");
    }
}
