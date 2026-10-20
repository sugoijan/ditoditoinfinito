//! Ticked holds (StepMania's pump game, `docs/plans/pump.md`) through a
//! whole play: combo, score, gauge and lamps.
//!
//! Chart (60 BPM, so beat `b` is at `b` seconds), pump-single:
//!
//! | idx | beat | lane | kind          |
//! |-----|------|------|---------------|
//! | 0   | 0    | 0    | tap           |
//! | 1   | 1    | 2    | hold → beat 3 |
//! | 2   | 2    | 1    | hold → beat 3 |
//! | 3   | 2.5  | 4    | tap           |
//! | 4   | 4    | 3    | tap           |
//!
//! 5 steps. Checkpoint rows (4 a beat): 1.25 … 3.0 for the first hold, 2.25
//! … 3.0 for the second; 2.0 (the second hold's head) and 2.5 (a tap) are
//! not judged on their own, so 6 rows are: 1.25, 1.5, 1.75 with one hold,
//! 2.25, 2.75, 3.0 with two.

use ddi_chart::{
    Chart, ComboSegment, Difficulty, DisplayBpm, Note, NoteKind, Song, SourceFormat, SourceInfo,
    Tick, TimingMap,
};
use ddi_engine::InputEvent;
use ddi_engine::player::{PlayOptions, Player, Results};
use ddi_engine::rules::danoni::DanoniOptions;
use ddi_engine::rules::presets::{self, HoldStyle, RulesetMode};
use ddi_engine::rules::{FullCombo, Judgement, Ruleset};
use ddi_platform::{ClockSample, HostTime};

fn song(layout: &str) -> Song {
    let t = Tick::from_beat_f64;
    let notes = vec![
        Note::new(t(0.0), 0, NoteKind::Tap),
        Note::new(t(1.0), 2, NoteKind::HoldHead { end: t(3.0) }),
        Note::new(t(2.0), 1, NoteKind::HoldHead { end: t(3.0) }),
        Note::new(t(2.5), 4, NoteKind::Tap),
        Note::new(t(4.0), 3, NoteKind::Tap),
    ];
    Song {
        title: "Ticks".into(),
        subtitle: String::new(),
        artist: "test".into(),
        title_translit: String::new(),
        subtitle_translit: String::new(),
        artist_translit: String::new(),
        genre: String::new(),
        credit: String::new(),
        music: None,
        preview_start: 0.0,
        preview_length: 0.0,
        banner: None,
        background: None,
        jacket: None,
        cd_title: None,
        timing: TimingMap::constant(60.0, 0.0),
        display_bpm: DisplayBpm::Actual,
        charts: vec![Chart {
            derivation: None,
            layout: layout.into(),
            difficulty: Difficulty::Hard,
            meter: 5,
            name: "ticks".into(),
            description: String::new(),
            credit: String::new(),
            notes,
            timing: None,
            display_bpm: None,
            danoni: None,
        }],
        effects: Vec::new(),
        keysounds: Vec::new(),
        layouts: Vec::new(),
        source: SourceInfo {
            format: SourceFormat::Other("test".into()),
            unknown_tags: Vec::new(),
        },
    }
}

/// Every note on time; holds pressed from their heads to past their tails.
fn perfect() -> Vec<(f64, u8, bool)> {
    let mut s: Vec<(f64, u8, bool)> = vec![
        (0.0, 0, true),
        (0.05, 0, false),
        (1.0, 2, true),
        (2.0, 1, true),
        (2.5, 4, true),
        (2.55, 4, false),
        (3.05, 1, false),
        (3.05, 2, false),
        (4.0, 3, true),
        (4.05, 3, false),
    ];
    s.sort_by(|a, b| a.0.total_cmp(&b.0));
    s
}

/// Plays `script` (`(song seconds, lane, pressed)`, sorted), updating
/// every 10 ms.
fn play(ruleset: Ruleset, song: &Song, script: &[(f64, u8, bool)]) -> Results {
    played(ruleset, song, script).results()
}

/// [`play`], the player at the end.
fn played(ruleset: Ruleset, song: &Song, script: &[(f64, u8, bool)]) -> Player {
    let mut player = Player::new(song, 0, ruleset, PlayOptions::default());
    player.clock_sample(ClockSample {
        context_time: 0.0,
        host_time: HostTime(0.0),
        output_latency: 0.0,
    });
    player.start(0.0, 0.0);
    let mut next = 0;
    for step in 0..=700u32 {
        let now = f64::from(step) / 100.0;
        player.update(HostTime(now));
        while next < script.len() && script[next].0 < now + 0.01 - 1e-9 {
            let (t, lane, pressed) = script[next];
            player.input(InputEvent {
                lane,
                pressed,
                host_time: HostTime(t),
            });
            next += 1;
        }
    }
    assert!(player.finished());
    replays_alike(song, &player);
    player
}

fn ruleset(preset: &str, holds: HoldStyle, song: &Song) -> Ruleset {
    presets::for_chart(
        RulesetMode::AllStepMania,
        preset,
        &DanoniOptions::default(),
        holds,
        &song.charts[0],
    )
}

#[test]
fn pump_charts_tick_by_default_and_dance_charts_do_not() {
    let pump = song("pump-single");
    let dance = song("dance-single");
    assert!(
        ruleset("itg", HoldStyle::ByGame, &pump)
            .judge
            .hold
            .is_ticked()
    );
    assert!(
        !ruleset("itg", HoldStyle::ByGame, &dance)
            .judge
            .hold
            .is_ticked()
    );
    assert!(
        ruleset("sm5", HoldStyle::Ticked, &dance)
            .judge
            .hold
            .is_ticked()
    );
    assert!(
        !ruleset("ddr-a", HoldStyle::Dance, &pump)
            .judge
            .hold
            .is_ticked()
    );
    // Dancing☆Onigiri rules keep their own holds.
    let danoni = presets::for_chart(
        RulesetMode::AllDanoni,
        "itg",
        &DanoniOptions::default(),
        HoldStyle::Ticked,
        &pump.charts[0],
    );
    assert!(!danoni.judge.hold.is_ticked());
}

#[test]
fn a_perfect_ticked_play_scores_everything() {
    let song = song("pump-single");
    let r = play(ruleset("sm5", HoldStyle::ByGame, &song), &song, &perfect());
    assert_eq!((r.tally.checkpoint_hit, r.tally.checkpoint_miss), (6, 0));
    assert_eq!((r.tally.held, r.tally.let_go), (2, 0));
    // 5 steps, then the holds kept at each row: 1 + 1 + 1 + 2 + 2 + 2.
    assert_eq!(r.max_combo, 14);
    assert_eq!(r.score.percent, Some(1.0));
    assert_eq!(r.score.grade_percent, Some(1.0));
    assert_eq!(r.full_combo, FullCombo::MarvelousFC);
    assert_eq!(r.grade, "AAAA");
    // ITG: the same, with its weights (and EX).
    let r = play(ruleset("itg", HoldStyle::ByGame, &song), &song, &perfect());
    assert_eq!(r.score.percent, Some(1.0));
    assert_eq!(r.score.ex, r.score.max_ex);
    // DDR A: checkpoints count as steps, a row one combo.
    let r = play(
        ruleset("ddr-a", HoldStyle::ByGame, &song),
        &song,
        &perfect(),
    );
    assert_eq!(r.score.money, Some(1_000_000));
    assert_eq!(r.max_combo, 11);
}

#[test]
fn a_missed_checkpoint_breaks_combo_and_costs_a_miss() {
    let song = song("pump-single");
    // The first hold let go at 1.3 and taken again at 1.6: 1.5 is missed
    // (0.2 s released), 1.75 hit again.
    let mut script = perfect();
    script.push((1.3, 2, false));
    script.push((1.6, 2, true));
    script.sort_by(|a, b| a.0.total_cmp(&b.0));
    let r = play(ruleset("sm5", HoldStyle::ByGame, &song), &song, &script);
    assert_eq!((r.tally.checkpoint_hit, r.tally.checkpoint_miss), (5, 1));
    assert_eq!((r.tally.held, r.tally.let_go), (1, 1));
    assert_eq!(r.full_combo, FullCombo::None);
    // From 1.75: its hold (1), the second head (1), 2.25 (2), the tap at
    // 2.5 (1), 2.75 and 3.0 (2 each), the tap at 4 (1).
    assert_eq!(r.max_combo, 10);
    // StepMania 5's weights: W1 3, Miss 0, a hold's own outcome 0.
    let expected = (5.0 * 3.0 + 5.0 * 3.0) / (11.0 * 3.0);
    assert!((r.score.percent.unwrap() - expected).abs() < 1e-12);
    // The same play with dance-style holds: one let go, no checkpoints.
    let r = play(ruleset("sm5", HoldStyle::Dance, &song), &song, &script);
    assert_eq!((r.tally.checkpoint_hit, r.tally.checkpoint_miss), (0, 0));
    assert_eq!((r.tally.held, r.tally.let_go), (1, 1));
}

#[test]
fn combo_segments_multiply_combo_and_counts() {
    // As BAD's `#COMBOS:0=1,80=4=1,87=1`: from beat 2 each row counts 4
    // (a miss 2 here), from beat 3.5 one again.
    let mut song = song("pump-single");
    let t = Tick::from_beat_f64;
    song.timing.combos = vec![
        ComboSegment {
            tick: t(0.0),
            combo: 1,
            miss_combo: 1,
        },
        ComboSegment {
            tick: t(2.0),
            combo: 4,
            miss_combo: 2,
        },
        ComboSegment {
            tick: t(3.5),
            combo: 1,
            miss_combo: 1,
        },
    ];
    let r = play(ruleset("sm5", HoldStyle::ByGame, &song), &song, &perfect());
    // Steps at 0, 1 and 4 count once, at 2 and 2.5 four times; the
    // checkpoints at 1.25 … 1.75 once, at 2.25 … 3.0 four times (two
    // holds each).
    assert_eq!(r.tally.count(Judgement::W1), 3 + 2 * 4);
    assert_eq!(r.tally.checkpoint_hit, 3 + 3 * 4);
    assert_eq!(r.max_combo, 3 + 3 + 2 * 4 + 3 * 2 * 4);
    // Dance points are not multiplied.
    assert_eq!(r.score.percent, Some(1.0));
    assert_eq!(r.full_combo, FullCombo::MarvelousFC);
    // The tap at 2.5 missed: a miss counted twice, the combo broken.
    let script: Vec<_> = perfect().into_iter().filter(|e| e.1 != 4).collect();
    let p = played(ruleset("sm5", HoldStyle::ByGame, &song), &song, &script);
    assert_eq!(p.results().tally.count(Judgement::Miss), 2);
    // Before it: 0, 1, 1.25 … 1.75, the head at 2 (4) and 2.25 (8);
    // after it: 2.75 and 3.0 (8 each) and the tap at 4.
    assert_eq!(p.max_combo(), 2 + 3 + 4 + 8);
    assert_eq!(p.combo(), 8 + 8 + 1);
}

/// The play judged again from its log ([`ddi_engine::history`]) gives
/// the same results, under its ruleset and, when the ruleset is one a key
/// builds (not tuned by the test), under its key.
fn replays_alike(song: &Song, player: &Player) {
    use ddi_engine::history::{replay, replay_with};
    assert!(player.finished());
    let ruleset = player.ruleset();
    let again = replay_with(song, 0, ruleset.fresh(), player.options(), player.log());
    assert_eq!(again, player.results());
    let built = ruleset.key().build(&song.charts[0]);
    if format!("{built:?}") == format!("{ruleset:?}") && built.fail == ruleset.fail {
        let again = replay(song, 0, &ruleset.key(), player.options(), player.log());
        assert_eq!(again, player.results());
    }
}
