//! Judging does not depend on how often the play is updated
//! (`docs/plans/play-history.md`, step 1): the same presses and releases,
//! fed with updates every millisecond, at 60 and 144 Hz and at uneven
//! random intervals of 1–50 ms, give the same judge events (kind, lane,
//! time), tally, score, grade and life, under every ruleset, both hold
//! styles and the transforms. A chart and a press script are generated to
//! reach every rule judged over time: misses, mines crossed while held,
//! shocks, holds released and taken again, rolls, checkpoints, a stop and
//! a change of tempo.

use ddi_chart::{Chart, Song};
use ddi_engine::judge::{JudgeEvent, JudgeEventKind};
use ddi_engine::player::{JUDGE_LAG, PlayOptions, Player, Results};
use ddi_engine::rules::danoni::{DanoniOptions, JudgeRange};
use ddi_engine::rules::presets::{self, HoldStyle, RulesetMode};
use ddi_engine::rules::{FailPolicy, Ruleset};
use ddi_engine::{InputEvent, TimingCut, TransformOptions, Turn};
use ddi_platform::{ClockSample, HostTime};

mod common;

use common::{Rng, chart, script};

/// What one play gave: every judge event in order, and the results.
#[derive(Debug, PartialEq)]
struct Outcome {
    events: Vec<(Option<usize>, u8, String, f64)>,
    results: Results,
}

fn describe(e: &JudgeEvent) -> (Option<usize>, u8, String, f64) {
    let kind = match e.kind {
        JudgeEventKind::Tap(j, d) => format!("{j:?} {d:.9}"),
        other => format!("{other:?}"),
    };
    (e.note_index, e.lane, kind, e.song_time)
}

/// Plays `script` with an update at each of `updates` (song seconds,
/// increasing), the presses due by an update fed before it, until the
/// play finishes.
fn play(
    song: &Song,
    ruleset: &dyn Fn() -> Ruleset,
    options: PlayOptions,
    script: &[(f64, u8, bool)],
    updates: &mut dyn FnMut() -> f64,
) -> Outcome {
    let mut player = Player::new(song, 0, ruleset(), options);
    player.clock_sample(ClockSample {
        context_time: 0.0,
        host_time: HostTime(0.0),
        output_latency: 0.0,
    });
    player.start(0.0, 0.0);
    let mut events = Vec::new();
    let mut next = 0;
    let end = player.song_end_time() + 2.0;
    let mut now = 0.0;
    while !player.finished() && now < end {
        now = updates();
        while next < script.len() && script[next].0 <= now {
            let (t, lane, pressed) = script[next];
            events.extend(player.input(InputEvent {
                lane,
                pressed,
                host_time: HostTime(t),
            }));
            next += 1;
        }
        events.extend(player.update(HostTime(now)));
    }
    assert!(player.finished(), "the play finishes");
    Outcome {
        events: events.iter().map(describe).collect(),
        results: player.results(),
    }
}

/// The cadences: every millisecond, 60 Hz, 144 Hz, and uneven random
/// steps of 1–50 ms (two seeds).
/// Makes a cadence's clock: each call, the next update's song second.
type Cadence = Box<dyn Fn() -> Box<dyn FnMut() -> f64>>;

fn cadences() -> Vec<(&'static str, Cadence)> {
    let steady = |step: f64| -> Cadence {
        Box::new(move || {
            let mut i = 0u64;
            Box::new(move || {
                i += 1;
                i as f64 * step
            })
        })
    };
    let uneven = |seed: u64| -> Cadence {
        Box::new(move || {
            let mut rng = Rng(seed);
            let mut t = 0.0;
            Box::new(move || {
                t += rng.range(0.001, 0.05);
                t
            })
        })
    };
    vec![
        ("1 ms", steady(0.001)),
        ("60 Hz", steady(1.0 / 60.0)),
        ("144 Hz", steady(1.0 / 144.0)),
        ("uneven", uneven(0x1234_5678)),
        ("uneven 2", uneven(0x0bad_cafe)),
    ]
}

/// Plays every script seed under every cadence and expects one outcome,
/// with the ruleset's fail policy (most scripts fail part-way) and with
/// none (the whole chart judged).
fn same_at_every_cadence(
    name: &str,
    song: &Song,
    ruleset: &dyn Fn() -> Ruleset,
    options: PlayOptions,
) {
    same_with(name, song, ruleset, options);
    let never = || Ruleset {
        fail: FailPolicy::Off,
        ..ruleset()
    };
    let o = same_with(&format!("{name}, no fail"), song, &never, options);
    // The scripts reach the rules judged over time.
    let t = o.results.tally;
    assert!(t.total_taps() > 25, "{name}: {t:?}");
    assert!(t.mine_hit > 0, "{name}: {t:?}");
    if !options.transform.no_holds {
        assert!(t.held > 0 && t.let_go > 0, "{name}: {t:?}");
    }
}

/// [`same_at_every_cadence`] for one ruleset; the last script's outcome.
fn same_with(
    name: &str,
    song: &Song,
    ruleset: &dyn Fn() -> Ruleset,
    options: PlayOptions,
) -> Outcome {
    let mut last = None;
    for seed in 1..=4u64 {
        let probe = Player::new(song, 0, ruleset(), options);
        let script = script(&probe, seed);
        // Updates judging exactly at every press and release, every note's
        // time, tail and window edges, the checkpoints' grid, and every
        // event's own time as a fine cadence judges it (supersessions,
        // drops): where rules meet at an instant. The player judges
        // `JUDGE_LAG` behind an update, so each is also an update time
        // that far later.
        let table = probe.ruleset().judge.clone();
        let fine = {
            let mut i = 0u64;
            play(song, ruleset, options, &script, &mut || {
                i += 1;
                i as f64 * 0.001
            })
        };
        let mut at: Vec<f64> = script.iter().map(|e| e.0).collect();
        at.extend(fine.events.iter().map(|e| e.3));
        for n in probe.judge().notes() {
            at.extend([
                n.seconds,
                n.end_seconds,
                n.seconds + table.miss_after(),
                n.seconds + table.mine_window_seconds(),
                n.seconds - table.max_early(),
            ]);
            let mut t = n.seconds;
            while t < n.end_seconds {
                t += 0.1;
                at.push(t);
            }
        }
        let mut edges: Vec<f64> = at
            .iter()
            .flat_map(|&t| [t, t + JUDGE_LAG, (t * 1e6).round() / 1e6 + JUDGE_LAG])
            .collect();
        edges.retain(|&t| t > 0.0);
        edges.sort_by(f64::total_cmp);
        edges.dedup();
        let edges = std::rc::Rc::new(edges);
        let mut cadences = cadences();
        cadences.push((
            "edges",
            Box::new(move || {
                let edges = edges.clone();
                let mut i = 0;
                Box::new(move || {
                    i += 1;
                    match edges.get(i - 1) {
                        Some(&t) => t,
                        None => edges[edges.len() - 1] + (i - edges.len()) as f64 * 0.05,
                    }
                })
            }),
        ));
        let mut first: Option<(&str, Outcome)> = None;
        for (cadence, make) in cadences {
            let out = play(song, ruleset, options, &script, &mut make());
            match &first {
                None => first = Some((cadence, out)),
                Some((c0, o0)) => {
                    if *o0 != out {
                        let at = o0
                            .events
                            .iter()
                            .zip(&out.events)
                            .position(|(a, b)| a != b)
                            .unwrap_or(o0.events.len().min(out.events.len()));
                        panic!(
                            "{name}, script {seed}: {cadence} differs from {c0} at event {at}: {:?} vs {:?} \
                             (events {} vs {}; tally {:?} vs {:?})",
                            o0.events.get(at),
                            out.events.get(at),
                            o0.events.len(),
                            out.events.len(),
                            o0.results.tally,
                            out.results.tally,
                        );
                    }
                }
            }
        }
        last = first.map(|(_, o)| o);
    }
    last.expect("a script")
}

fn stepmania(id: &'static str, holds: HoldStyle) -> impl Fn() -> Ruleset {
    move || {
        let chart = Chart {
            layout: "dance-single".into(),
            ..chart(1, "dance-single").charts.remove(0)
        };
        presets::for_chart(
            RulesetMode::AllStepMania,
            id,
            &DanoniOptions::default(),
            holds,
            &chart,
        )
    }
}

#[test]
fn stepmania_presets_judge_alike_at_every_cadence() {
    for (seed, id) in [(11, "itg"), (12, "sm5"), (13, "ddr-a")] {
        let song = chart(seed, "dance-single");
        for holds in [HoldStyle::Dance, HoldStyle::Ticked] {
            same_at_every_cadence(
                &format!("{id} {holds:?}"),
                &song,
                &stepmania(id, holds),
                PlayOptions::default(),
            );
        }
    }
}

#[test]
fn pump_charts_tick_their_holds_alike_at_every_cadence() {
    let song = chart(21, "pump-single");
    same_at_every_cadence(
        "itg pump",
        &song,
        &|| {
            presets::for_chart(
                RulesetMode::AllStepMania,
                "itg",
                &DanoniOptions::default(),
                HoldStyle::ByGame,
                &song.charts[0],
            )
        },
        PlayOptions::default(),
    );
}

#[test]
fn dancing_onigiri_rules_judge_alike_at_every_cadence() {
    let song = chart(31, "dance-single");
    for options in [
        DanoniOptions::default(),
        DanoniOptions {
            range: JudgeRange::Hard,
            excessive: Some(true),
            gauge: None,
        },
    ] {
        same_at_every_cadence(
            &format!("danoni {options:?}"),
            &song,
            &|| {
                presets::for_chart(
                    RulesetMode::AllDanoni,
                    "itg",
                    &options,
                    HoldStyle::ByGame,
                    &song.charts[0],
                )
            },
            PlayOptions::default(),
        );
    }
}

#[test]
fn transforms_judge_alike_at_every_cadence() {
    let song = chart(41, "dance-single");
    for transform in [
        TransformOptions {
            turn: Turn::Shuffle,
            ..TransformOptions::default()
        },
        TransformOptions {
            turn: Turn::Mirror,
            cut: TimingCut::Eighths,
            no_jumps: true,
            no_holds: false,
        },
        TransformOptions {
            no_holds: true,
            ..TransformOptions::default()
        },
    ] {
        same_at_every_cadence(
            &format!("sm5 {transform:?}"),
            &song,
            &stepmania("sm5", HoldStyle::Dance),
            PlayOptions {
                transform,
                seed: 7,
                ..PlayOptions::default()
            },
        );
    }
}

/// A Dancing☆Onigiri chart on danoniplus's frame grid (every note on a
/// 1/60 s frame, at 225 BPM: a beat is 16 frames), where its windows,
/// counted in frames, end exactly on other notes' times.
#[test]
fn dancing_onigiri_on_its_frame_grid_judges_alike_at_every_cadence() {
    let mut song = chart(32, "dance-single");
    song.timing = ddi_chart::TimingMap::constant(225.0, 0.0);
    for options in [
        DanoniOptions::default(),
        DanoniOptions {
            range: JudgeRange::Narrow,
            excessive: Some(true),
            gauge: None,
        },
    ] {
        same_at_every_cadence(
            &format!("danoni frames {options:?}"),
            &song,
            &|| {
                presets::for_chart(
                    RulesetMode::AllDanoni,
                    "itg",
                    &options,
                    HoldStyle::ByGame,
                    &song.charts[0],
                )
            },
            PlayOptions::default(),
        );
    }
}
