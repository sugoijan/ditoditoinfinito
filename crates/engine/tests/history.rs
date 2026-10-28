//! A play judged again from its input log gives the same results
//! (`docs/plans/play-history.md`, step 2): autoplays and random plays
//! under every ruleset, hold style and transform, live-style plays with a
//! drifting clock, latency and offsets, a log moved to another audio
//! offset, and the logs' and timings' binary forms.

mod common;

use common::{Rng, chart, chart_of, script};
use ddi_chart::Song;
use ddi_engine::history::{PlayLog, PlayTiming, replay};
use ddi_engine::player::{PlayOptions, Player, Results};
use ddi_engine::rules::danoni::{DanoniOptions, JudgeRange};
use ddi_engine::rules::ddr::DdrGauge;
use ddi_engine::rules::presets::{self, HoldStyle, RulesetMode};
use ddi_engine::rules::{FailType, Ruleset, StepManiaOptions};
use ddi_engine::{AtOnce, ClockOptions, InputEvent, TimingCut, TransformOptions, Turn, autoplay};
use ddi_platform::{ClockSample, HostTime};

/// The rulesets a player can choose for `song`'s chart: each preset
/// with dance and ticked holds, the pump chart's holds by its game (FFR's
/// rules playing them as taps), and Dancing☆Onigiri with its defaults and
/// other options.
fn rulesets(song: &Song) -> Vec<Ruleset> {
    let chart = &song.charts[0];
    let mut out = Vec::new();
    for id in ["itg", "sm5", "ddr-a", "ddr-x", "ddr-extreme", "ffr"] {
        for holds in [HoldStyle::Dance, HoldStyle::Ticked, HoldStyle::ByGame] {
            out.push(presets::for_chart(
                RulesetMode::AllStepMania,
                id,
                &DanoniOptions::default(),
                holds,
                chart,
            ));
        }
    }
    // DDR A under DDR's other gauges, failing at once or playing on.
    for (gauge, fail) in [
        (DdrGauge::Life4, FailType::Immediate),
        (DdrGauge::Risky, FailType::Off),
        (DdrGauge::Flare(5), FailType::Immediate),
        (DdrGauge::Floating, FailType::ImmediateContinue),
    ] {
        out.push(
            presets::stepmania("ddr-a", HoldStyle::ByGame, chart).with_stepmania(
                &StepManiaOptions {
                    gauge,
                    fail,
                    ..StepManiaOptions::default()
                },
            ),
        );
    }
    for options in [
        DanoniOptions::default(),
        DanoniOptions {
            gauge: Some("Original".into()),
            range: JudgeRange::Narrow,
            excessive: Some(true),
        },
    ] {
        out.push(presets::for_chart(
            RulesetMode::AllDanoni,
            "itg",
            &options,
            HoldStyle::ByGame,
            chart,
        ));
    }
    out
}

fn transforms() -> Vec<PlayOptions> {
    [
        TransformOptions::default(),
        TransformOptions {
            turn: Turn::Shuffle,
            ..TransformOptions::default()
        },
        TransformOptions {
            turn: Turn::Left,
            cut: TimingCut::Eighths,
            at_once: AtOnce::One,
            no_holds: true,
        },
        TransformOptions {
            at_once: AtOnce::Two,
            ..TransformOptions::default()
        },
    ]
    .into_iter()
    .map(|transform| PlayOptions {
        transform,
        seed: 3,
        ..PlayOptions::default()
    })
    .collect()
}

/// Plays `script` (`(host seconds, lane, pressed)`, sorted) with the
/// clock `sample` gives at each 60 Hz frame, the frame's edges fed before
/// its update, until the play finishes.
fn live(
    song: &Song,
    ruleset: Ruleset,
    options: PlayOptions,
    script: &[(f64, u8, bool)],
    sample: &mut dyn FnMut(f64) -> ClockSample,
) -> Player {
    let mut player = Player::new(song, 0, ruleset, options);
    player.clock_sample(sample(0.0));
    player.start(0.0, 0.0);
    let end = player.song_end_time() + 3.0;
    let mut next = 0;
    let mut frame = 0u32;
    while !player.finished() {
        let now = f64::from(frame) / 60.0;
        assert!(now < end, "the play finishes");
        if frame.is_multiple_of(6) {
            player.clock_sample(sample(now));
        }
        while next < script.len() && script[next].0 <= now {
            let (t, lane, pressed) = script[next];
            player.input(InputEvent {
                lane,
                pressed,
                host_time: HostTime(t),
            });
            next += 1;
        }
        player.update(HostTime(now));
        frame += 1;
    }
    player
}

/// A steady clock: context time is host time, no latency.
fn steady(t: f64) -> ClockSample {
    ClockSample {
        context_time: t,
        host_time: HostTime(t),
        output_latency: 0.0,
    }
}

/// The play judged again from its log, and from its log's binary form,
/// under its ruleset's key.
fn assert_replays(label: &str, song: &Song, player: &Player) {
    let key = player.ruleset().key();
    let r = player.results();
    let again = replay(song, 0, &key, player.options(), player.log());
    assert_eq!(again, r, "{label}");
    let decoded = PlayLog::decode(&player.log().encode()).expect("decodes");
    assert_eq!(&decoded, player.log(), "{label}");
    let timing = PlayTiming::from_results(&r);
    assert_eq!(
        PlayTiming::decode(&timing.encode()).as_ref(),
        Ok(&timing),
        "{label}"
    );
}

fn autoplay_script(player: &Player) -> Vec<(f64, u8, bool)> {
    let mut script = autoplay::edges(autoplay::script(player.judge().notes(), 0.0));
    autoplay::sort_edges(&mut script);
    script
}

#[test]
fn autoplays_replay_alike_under_every_ruleset_and_option() {
    for layout in ["dance-single", "pump-single"] {
        let song = chart(51, layout);
        for ruleset in rulesets(&song) {
            for options in transforms() {
                let probe = Player::new(&song, 0, ruleset.fresh(), options);
                let script = autoplay_script(&probe);
                let player = live(&song, ruleset.fresh(), options, &script, &mut steady);
                let label = format!("{layout} {:?} {:?}", ruleset.key(), options.transform);
                assert_replays(&label, &song, &player);
            }
        }
    }
}

#[test]
fn random_plays_replay_alike_under_every_ruleset_and_option() {
    let song = chart(52, "dance-single");
    for ruleset in rulesets(&song) {
        for options in transforms() {
            for seed in 1..=3 {
                let probe = Player::new(&song, 0, ruleset.fresh(), options);
                let script = script(&probe, seed);
                let player = live(&song, ruleset.fresh(), options, &script, &mut steady);
                let label = format!("{:?} {:?} script {seed}", ruleset.key(), options.transform);
                assert_replays(&label, &song, &player);
            }
        }
    }
}

/// Live: a clock that drifts and jitters, an output latency and the
/// player's offsets; the log has the judged times, and the replay needs
/// none of it.
#[test]
fn live_plays_with_drift_latency_and_offsets_replay_alike() {
    let song = chart(53, "dance-single");
    let options = PlayOptions {
        clock: ClockOptions {
            audio_offset: 0.0237,
            visual_offset: -0.011,
            ..ClockOptions::default()
        },
        ..PlayOptions::default()
    };
    for ruleset in rulesets(&song) {
        let probe = Player::new(&song, 0, ruleset.fresh(), options);
        // The presses in host time: where the notes are heard, give or
        // take.
        let shift = 0.0237 + 0.042;
        let script: Vec<(f64, u8, bool)> = script(&probe, 9)
            .into_iter()
            .map(|(t, lane, pressed)| (t + shift, lane, pressed))
            .collect();
        let mut rng = Rng(77);
        let mut sample = |t: f64| ClockSample {
            context_time: 5.0 + t * (1.0 + 3e-4) + rng.range(-0.002, 0.002),
            host_time: HostTime(t),
            output_latency: 0.042,
        };
        let player = live(&song, ruleset.fresh(), options, &script, &mut sample);
        assert_replays(&format!("{:?}", ruleset.key()), &song, &player);
    }
}

/// An audio offset raised by `d` judges every press `d` earlier: the
/// log of a play under offset `a` moved by `a − b` replays as the same
/// presses played under `b`.
#[test]
fn a_shifted_log_is_the_play_under_another_audio_offset() {
    let song = chart(54, "dance-single");
    let ruleset = || presets::sm5();
    let probe = Player::new(&song, 0, ruleset(), PlayOptions::default());
    let script = script(&probe, 4);
    let under = |offset: f64| {
        let options = PlayOptions {
            clock: ClockOptions {
                audio_offset: offset,
                ..ClockOptions::default()
            },
            ..PlayOptions::default()
        };
        live(&song, ruleset(), options, &script, &mut steady)
    };
    let (a, b) = (0.031, -0.017);
    let played_a = under(a);
    let played_b = under(b);
    assert_ne!(played_a.results(), played_b.results());
    let moved = played_a.log().shifted(a - b);
    // The same edges, as far as the play under `b` went (it may fail
    // sooner).
    let n = played_b.log().inputs.len();
    assert_eq!(&moved.inputs[..n], &played_b.log().inputs[..]);
    let again = replay(
        &song,
        0,
        &played_a.ruleset().key(),
        played_b.options(),
        &moved,
    );
    assert_eq!(again, played_b.results());
}

/// A play started part-way (`startFrame`, a practice start): its log
/// keeps where.
#[test]
fn a_play_started_part_way_replays_from_there() {
    let song = chart(55, "dance-single");
    let probe = Player::new(&song, 0, presets::itg(), PlayOptions::default());
    let script: Vec<_> = script(&probe, 2)
        .into_iter()
        .filter(|e| e.0 > 12.0)
        .collect();
    let mut player = Player::new(&song, 0, presets::itg(), PlayOptions::default());
    player.clock_sample(steady(0.0));
    player.start(0.0, 12.0);
    let mut next = 0;
    let mut now = 12.0;
    while !player.finished() {
        now += 1.0 / 60.0;
        while next < script.len() && script[next].0 <= now {
            let (t, lane, pressed) = script[next];
            player.input(InputEvent {
                lane,
                pressed,
                host_time: HostTime(t - 12.0),
            });
            next += 1;
        }
        player.update(HostTime(now - 12.0));
    }
    assert_eq!(player.log().start, 12.0);
    assert_replays("part-way", &song, &player);
}

/// A 1,000-note chart's log takes a few KB, and its timing about seven
/// bytes a judgement.
#[test]
fn logs_and_timings_are_compact() {
    let song = chart_of(56, "dance-single", 520);
    let ruleset = || presets::itg();
    let probe = Player::new(&song, 0, ruleset(), PlayOptions::default());
    let notes = probe.judge().notes().iter().filter(|n| n.judgeable).count();
    assert!(notes >= 1000, "{notes} notes");
    let script = script(&probe, 5);
    let mut never_fails = ruleset();
    never_fails.fail = ddi_engine::FailPolicy::Off;
    let player = live(
        &song,
        never_fails,
        PlayOptions::default(),
        &script,
        &mut steady,
    );
    let log = player.log().encode();
    let edges = player.log().inputs.len();
    let timing = PlayTiming::from_results(&player.results()).encode();
    let judged = player.results().timeline.len();
    eprintln!(
        "{notes} notes: {edges} edges in {} bytes ({:.2} an edge); {judged} judgements in {} bytes ({:.2} each)",
        log.len(),
        log.len() as f64 / edges as f64,
        timing.len(),
        timing.len() as f64 / judged as f64,
    );
    assert!(log.len() < 8 * 1024, "{} bytes", log.len());
    assert!((log.len() as f64) < edges as f64 * 4.0);
    assert!((timing.len() as f64) < judged as f64 * 9.0);
}

/// Not the same results when a log is changed: the replay reads it.
#[test]
fn a_quit_play_is_judged_to_its_quit_and_replays_there() {
    let song = chart(23, "dance-single");
    let ruleset = presets::itg();
    let options = PlayOptions::default();
    let probe = Player::new(&song, 0, ruleset.fresh(), options);
    let script = autoplay_script(&probe);
    let mut player = Player::new(&song, 0, ruleset.fresh(), options);
    player.clock_sample(steady(0.0));
    player.start(0.0, 0.0);
    let total = player.judge().notes().len() as u32;
    // Played a third of the way, then quit.
    let at = player.song_end_time() / 3.0;
    let mut next = 0;
    let mut frame = 0u32;
    loop {
        let now = f64::from(frame) / 60.0;
        if now >= at {
            break;
        }
        while next < script.len() && script[next].0 <= now {
            let (t, lane, pressed) = script[next];
            player.input(InputEvent {
                lane,
                pressed,
                host_time: HostTime(t),
            });
            next += 1;
        }
        player.update(HostTime(now));
        frame += 1;
    }
    let before = player.results();
    assert!(!player.finished() && !before.quit);
    player.quit_at(at);
    let r = player.results();
    // Finished, graded QUIT, not failed; every note judged, the ones
    // left as misses; the gauge as it was.
    assert!(player.finished());
    assert!(r.quit && !r.failed, "{r:?}");
    assert_eq!(r.grade, "QUIT");
    assert!(player.judge().all_resolved(), "{total} notes");
    assert!(
        r.tally.count(ddi_engine::rules::Judgement::Miss)
            > before.tally.count(ddi_engine::rules::Judgement::Miss)
    );
    assert_eq!(
        r.life.last().map(|l| l[1]),
        before.life.last().map(|l| l[1])
    );
    assert!((player.log().end.expect("where it quit") - at).abs() < 1e-6);
    // Judged again from its log (and its log's binary form): the same.
    assert_replays("quit", &song, &player);
    // A second quit, or more time, changes nothing.
    player.quit_at(at + 5.0);
    assert!(player.update(HostTime(at + 10.0)).is_empty());
    assert_eq!(player.results(), r);
}

#[test]
fn a_changed_log_changes_the_replay() {
    let song = chart(57, "dance-single");
    let probe = Player::new(&song, 0, presets::itg(), PlayOptions::default());
    let script = autoplay_script(&probe);
    let player = live(
        &song,
        presets::itg(),
        PlayOptions::default(),
        &script,
        &mut steady,
    );
    let mut log = player.log().clone();
    let first = log.inputs.iter().position(|i| i.pressed).expect("a press");
    log.inputs[first].time += 0.03;
    let again = replay(&song, 0, &player.ruleset().key(), player.options(), &log);
    assert_ne!(again, player.results());
}

/// Results kept by a summary compare equal after a round trip through
/// JSON too (what the desktop store writes).
#[test]
fn logs_and_timings_round_trip_through_json() {
    let song = chart(58, "dance-single");
    let probe = Player::new(&song, 0, presets::sm5(), PlayOptions::default());
    let script = script(&probe, 1);
    let player = live(
        &song,
        presets::sm5(),
        PlayOptions::default(),
        &script,
        &mut steady,
    );
    let log: PlayLog = serde_json::from_str(&serde_json::to_string(player.log()).unwrap()).unwrap();
    assert_eq!(&log, player.log());
    let timing = PlayTiming::from_results(&player.results());
    let back: PlayTiming = serde_json::from_str(&serde_json::to_string(&timing).unwrap()).unwrap();
    assert_eq!(back, timing);
    let key = player.ruleset().key();
    let k: ddi_engine::RulesetKey =
        serde_json::from_str(&serde_json::to_string(&key).unwrap()).unwrap();
    assert_eq!(k, key);
    let _: Results = player.results();
}

/// A Dancing☆Onigiri key holds the excessive setting in effect, so the
/// chart's own and the player's same choice are one key, and one best.
#[test]
fn dancing_onigiri_keys_hold_the_rule_in_effect() {
    let song = chart(59, "dance-single");
    let chart = &song.charts[0];
    let key = |excessive| {
        presets::for_chart(
            RulesetMode::AllDanoni,
            "itg",
            &DanoniOptions {
                gauge: Some("Original".into()),
                excessive,
                ..DanoniOptions::default()
            },
            HoldStyle::ByGame,
            chart,
        )
        .key()
    };
    // The chart has no excessive header: off unless chosen.
    assert_eq!(key(None), key(Some(false)));
    assert_ne!(key(None), key(Some(true)));
    let best = key(None).best_key();
    assert_eq!(best.danoni.as_ref().unwrap().gauge, None);
    // Rebuilt from its key, the same ruleset.
    let k = key(Some(true));
    assert_eq!(k.build(chart).key(), k);
}

/// Lane covers changed during a play: kept in the log, drawn by the
/// frames, the results' the last shown, and judging untouched; the
/// replay changes them at the same times.
#[test]
fn lane_cover_changes_are_kept_and_judge_nothing() {
    use ddi_engine::cover::{CoverMode, LaneCover};
    let song = chart(54, "dance-single");
    let ruleset = rulesets(&song).remove(0);
    let sudden = LaneCover {
        mode: CoverMode::Sudden,
        sudden: 30,
        hidden: 25,
    };
    let options = PlayOptions {
        cover: sudden,
        ..PlayOptions::default()
    };
    let probe = Player::new(&song, 0, ruleset.fresh(), options);
    let script = autoplay_script(&probe);
    let plain = live(
        &song,
        ruleset.fresh(),
        PlayOptions::default(),
        &script,
        &mut steady,
    );

    let mut player = Player::new(&song, 0, ruleset.fresh(), options);
    player.clock_sample(steady(0.0));
    player.start(0.0, 0.0);
    assert_eq!(player.frame(HostTime(0.5)).cover.cover, sudden);
    assert_eq!(player.frame(HostTime(0.5)).cover.changed, None);
    let raised = sudden.moved(10);
    player.set_cover(raised, HostTime(1.0));
    let f = player.frame(HostTime(1.5));
    assert_eq!(f.cover.cover, raised);
    assert!(f.cover.changed.is_some_and(|a| (a - 0.5).abs() < 1e-3));
    assert_eq!(player.frame(HostTime(3.0)).cover.changed, None);
    // Toggled off near the end: the results keep the last shown.
    let off = LaneCover {
        mode: CoverMode::Off,
        ..raised
    };
    player.set_cover(off, HostTime(2.0));
    for &(t, lane, pressed) in &script {
        player.input(InputEvent {
            lane,
            pressed,
            host_time: HostTime(t),
        });
    }
    player.update(HostTime(player.end_time() + 1.0));
    assert!(player.finished());
    let r = player.results();
    assert_eq!(r.cover, raised);
    assert_eq!(player.log().covers.len(), 2);
    assert_eq!(player.log().covers[0].time, 1.0);
    // The same judgements as without covers.
    let p = plain.results();
    assert_eq!(
        (r.tally, r.score, r.timeline.len()),
        (p.tally, p.score, p.timeline.len())
    );
    assert_replays("covers", &song, &player);
}
