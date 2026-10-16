//! Judging at an instant and at the edges of time (`docs/plans/play-history.md`,
//! step 1, from an independent review's probes): events that fall at one
//! instant come in one order however the time is cut; a hold hit early
//! and let go before its head; a time that is no number; an edge that
//! reaches the player late; presses snapped to every note's time and
//! window edge; codecs fed noise; and how long a long play takes.

mod common;

use common::{Rng, chart};
use ddi_chart::{Note, NoteKind, Song, StopSegment, Tick, TimingMap};
use ddi_engine::InputEvent;
use ddi_engine::history::{PlayLog, PlayTiming, replay_with};
use ddi_engine::judge::{JudgeEvent, JudgeEventKind};
use ddi_engine::player::{JUDGE_LAG, PlayOptions, Player};
use ddi_engine::rules::danoni::{DanoniOptions, JudgeRange};
use ddi_engine::rules::presets::{self, HoldStyle, RulesetMode};
use ddi_engine::rules::{FailPolicy, Judgement, Ruleset};
use ddi_platform::{ClockSample, HostTime};

fn describe(e: &JudgeEvent) -> String {
    format!(
        "{:?} l{} {:?} @{}",
        e.note_index, e.lane, e.kind, e.song_time
    )
}

/// ITG at 60 BPM: four taps missed, then a hold kept in lane 1 whose tail
/// is exactly the instant a tap in lane 0 is missed (a stop makes them
/// meet). The misses fail the play; whether the hold is kept first must
/// not depend on an update at that instant.
fn tie_song() -> (Song, f64) {
    let miss_after = presets::itg().judge.miss_after();
    let tick = |b: f64| Tick((b * 48.0) as i64);
    let mut stop = miss_after - 1.0 / 48.0;
    loop {
        let mut timing = TimingMap::constant(60.0, 0.0);
        timing.stops.push(StopSegment {
            tick: tick(4.0),
            seconds: stop,
        });
        let mut notes = vec![];
        for (i, b) in [1.0, 1.5, 2.0, 2.5].iter().enumerate() {
            notes.push(Note::new(tick(*b), 2 + (i as u8 % 2), NoteKind::Tap));
        }
        notes.push(Note::new(
            tick(3.5),
            1,
            NoteKind::HoldHead {
                end: Tick(tick(4.0).0 + 1),
            },
        ));
        notes.push(Note::new(tick(4.0), 0, NoteKind::Tap));
        notes.sort_by_key(|n| (n.tick, n.lane));
        let song = common::song("dance-single", timing, notes);
        let p = Player::new(&song, 0, presets::itg(), PlayOptions::default());
        let ns = p.judge().notes();
        let tap = ns.iter().find(|n| n.note.lane == 0).unwrap().seconds;
        let end = ns.iter().find(|n| n.note.lane == 1).unwrap().end_seconds;
        if tap + miss_after == end {
            return (song, end);
        }
        stop = if tap + miss_after > end {
            f64::from_bits(stop.to_bits() + 1)
        } else {
            f64::from_bits(stop.to_bits() - 1)
        };
    }
}

#[test]
fn events_at_one_instant_come_in_one_order() {
    let (song, end) = tie_song();
    let mut results = Vec::new();
    for updates in [vec![end + JUDGE_LAG], vec![end], vec![]] {
        let mut p = Player::new(&song, 0, presets::itg(), PlayOptions::default());
        p.start(0.0, 0.0);
        let mut ev = p.input_at(1, true, 3.5);
        for u in updates {
            ev.extend(p.update_at(u));
        }
        ev.extend(p.input_at(1, false, 4.5));
        ev.extend(p.update_at(10.0));
        let r = p.results();
        let again = replay_with(&song, 0, presets::itg(), &PlayOptions::default(), p.log());
        assert_eq!(again, r);
        let at: Vec<String> = ev
            .iter()
            .filter(|e| e.song_time == end)
            .map(describe)
            .collect();
        results.push((r, at));
    }
    // The press at the hold's tail comes first; at the tail, by lane: the
    // tap's miss (lane 0) fails the play before the hold (lane 1) is kept.
    for (r, at) in &results {
        assert_eq!((r, at), (&results[0].0, &results[0].1));
    }
    let (r, _) = &results[0];
    assert!(r.failed);
    assert_eq!(r.tally.held, 0, "{:?}", results[0].1);
}

/// A hold hit early and let go before its head: StepMania judges a
/// hold's body from its head row on, so it decays from its head.
#[test]
fn a_hold_let_go_before_its_head_decays_from_its_head() {
    // ITG at 60 BPM: a hold from 4.0 to 4.3125; ITG's hold window is
    // 0.32 s (plus 1.5 ms), longer than the hold.
    let notes = vec![Note::new(
        Tick::from_beats(4),
        0,
        NoteKind::HoldHead {
            end: Tick(Tick::from_beats(4).0 + 15),
        },
    )];
    let song = common::song("dance-single", TimingMap::constant(60.0, 0.0), notes);
    let mut p = Player::new(&song, 0, presets::itg(), PlayOptions::default());
    p.start(0.0, 0.0);
    let mut ev = p.input_at(0, true, 3.90);
    ev.extend(p.input_at(0, false, 3.95));
    ev.extend(p.update_at(6.0));
    let kinds: Vec<_> = ev.iter().map(|e| e.kind).collect();
    assert!(
        matches!(
            kinds.as_slice(),
            [JudgeEventKind::Tap(_, _), JudgeEventKind::Held]
        ),
        "{kinds:?}"
    );
}

/// A time that is no number is ignored: it does not stop the next update
/// from seeing a mine crossed while held.
#[test]
fn a_time_that_is_no_number_is_ignored() {
    let notes = vec![
        Note::new(Tick::from_beats(2), 0, NoteKind::Mine),
        Note::new(Tick::from_beats(4), 0, NoteKind::Tap),
    ];
    let song = common::song("dance-single", TimingMap::constant(60.0, 0.0), notes);
    let mut p = Player::new(&song, 0, presets::itg(), PlayOptions::default());
    p.start(0.0, 0.0);
    p.input_at(0, true, 1.0);
    assert!(p.update_at(f64::NAN).is_empty());
    assert!(p.input_at(1, true, f64::INFINITY).is_empty());
    let ev = p.update_at(2.5);
    assert!(
        ev.iter().any(|e| e.kind == JudgeEventKind::HitMine),
        "{:?}",
        ev.iter().map(describe).collect::<Vec<_>>()
    );
    assert_eq!(p.log().inputs.len(), 1);
}

/// An edge that reaches the player after an update for a later time, by
/// less than [`JUDGE_LAG`], is judged at its own time; one later still
/// counts as happening when judged, and is counted.
#[test]
fn a_late_edge_is_judged_at_its_time_within_the_lag() {
    let notes = vec![
        Note::new(Tick::from_beats(4), 0, NoteKind::Tap),
        Note::new(Tick::from_beats(6), 0, NoteKind::Tap),
    ];
    let song = common::song("dance-single", TimingMap::constant(60.0, 0.0), notes);
    let mut p = Player::new(&song, 0, presets::itg(), PlayOptions::default());
    p.clock_sample(ClockSample {
        context_time: 0.0,
        host_time: HostTime(0.0),
        output_latency: 0.010,
    });
    p.start(0.0, 0.0);
    // Heard time 4.002, then the latency rises by 8 ms: the heard time
    // of what comes next steps back.
    p.update(HostTime(4.012));
    p.clock_sample(ClockSample {
        context_time: 4.0201,
        host_time: HostTime(4.0201),
        output_latency: 0.018,
    });
    let ev = p.input(InputEvent {
        lane: 0,
        pressed: true,
        host_time: HostTime(4.016),
    });
    let JudgeEventKind::Tap(j, delta) = ev[0].kind else {
        panic!("{ev:?}")
    };
    assert_eq!(j, Judgement::W1);
    assert!((delta + 0.002).abs() < 1e-6, "{delta}");
    assert_eq!(p.results().late_edges, 0);
    // Later than the lag: judged when judged up to.
    p.input(InputEvent {
        lane: 0,
        pressed: false,
        host_time: HostTime(4.05),
    });
    p.update(HostTime(5.9));
    let ev = p.input(InputEvent {
        lane: 0,
        pressed: true,
        host_time: HostTime(5.8),
    });
    let JudgeEventKind::Tap(_, delta) = ev[0].kind else {
        panic!("{ev:?}")
    };
    assert!(delta > -0.2 + 0.05, "{delta}");
    assert_eq!(p.results().late_edges, 1);
}

/// Presses snapped to note times, mine times and window edges, presses
/// and releases at one instant, a release and a press again at one
/// instant (a script from the review's probes).
fn nasty_script(p: &Player, seed: u64) -> Vec<(f64, u8, bool)> {
    let mut rng = Rng(seed | 1);
    let table = p.ruleset().judge.clone();
    let mut anchors: Vec<f64> = Vec::new();
    for n in p.judge().notes() {
        anchors.extend([
            n.seconds,
            n.end_seconds,
            n.seconds + table.miss_after(),
            n.seconds - table.max_early(),
            n.seconds + table.mine_window_seconds(),
            n.seconds - table.mine_window_seconds(),
        ]);
    }
    let lanes = usize::from(p.lanes());
    let mut per_lane: Vec<Vec<(f64, f64)>> = vec![Vec::new(); lanes];
    for _ in 0..400 {
        let lane = rng.below(lanes as u64) as usize;
        let a = anchors[rng.below(anchors.len() as u64) as usize];
        let len = match rng.below(4) {
            0 => 0.0,
            1 => rng.range(0.0, 0.05),
            2 => rng.range(0.05, 0.6),
            _ => {
                let b = anchors[rng.below(anchors.len() as u64) as usize];
                (b - a).abs()
            }
        };
        per_lane[lane].push((a, a + len));
    }
    let mut out = Vec::new();
    for (lane, mut s) in per_lane.into_iter().enumerate() {
        s.sort_by(|a, b| a.0.total_cmp(&b.0));
        let mut until = f64::NEG_INFINITY;
        for (p, r) in s {
            let p = p.max(until);
            let r = r.max(p);
            out.push((p, lane as u8, true));
            out.push((r, lane as u8, false));
            until = r;
        }
    }
    out.sort_by(|a, b| a.0.total_cmp(&b.0));
    out
}

fn play(
    song: &Song,
    rules: &dyn Fn() -> Ruleset,
    script: &[(f64, u8, bool)],
    mut next_update: impl FnMut() -> f64,
) -> (Vec<String>, ddi_engine::Results, PlayLog) {
    let mut player = Player::new(song, 0, rules(), PlayOptions::default());
    player.clock_sample(ClockSample {
        context_time: 0.0,
        host_time: HostTime(0.0),
        output_latency: 0.0,
    });
    player.start(0.0, 0.0);
    let mut ev = Vec::new();
    let mut i = 0;
    let end = player.song_end_time() + 2.0;
    let mut now = 0.0;
    while !player.finished() && now < end {
        now = next_update();
        while i < script.len() && script[i].0 <= now {
            let (t, lane, pressed) = script[i];
            ev.extend(player.input(InputEvent {
                lane,
                pressed,
                host_time: HostTime(t),
            }));
            i += 1;
        }
        ev.extend(player.update(HostTime(now)));
    }
    (
        ev.iter().map(describe).collect(),
        player.results(),
        player.log().clone(),
    )
}

/// Builds a ruleset for a chart.
type RulesFor = Box<dyn Fn(&ddi_chart::Chart) -> Ruleset>;

#[test]
fn snapped_presses_judge_alike_at_every_cadence_and_replay() {
    let chart_rules: Vec<(&str, RulesFor)> = vec![
        (
            "itg dance",
            Box::new(|c| {
                presets::for_chart(
                    RulesetMode::AllStepMania,
                    "itg",
                    &DanoniOptions::default(),
                    HoldStyle::Dance,
                    c,
                )
            }),
        ),
        (
            "itg ticked",
            Box::new(|c| {
                presets::for_chart(
                    RulesetMode::AllStepMania,
                    "itg",
                    &DanoniOptions::default(),
                    HoldStyle::Ticked,
                    c,
                )
            }),
        ),
        (
            "ddr-a",
            Box::new(|c| {
                presets::for_chart(
                    RulesetMode::AllStepMania,
                    "ddr-a",
                    &DanoniOptions::default(),
                    HoldStyle::Dance,
                    c,
                )
            }),
        ),
        (
            "danoni",
            Box::new(|c| {
                presets::for_chart(
                    RulesetMode::AllDanoni,
                    "itg",
                    &DanoniOptions::default(),
                    HoldStyle::ByGame,
                    c,
                )
            }),
        ),
        (
            "danoni hard excessive",
            Box::new(|c| {
                presets::for_chart(
                    RulesetMode::AllDanoni,
                    "itg",
                    &DanoniOptions {
                        range: JudgeRange::Hard,
                        excessive: Some(true),
                        gauge: None,
                    },
                    HoldStyle::ByGame,
                    c,
                )
            }),
        ),
    ];
    for (name, make) in &chart_rules {
        for seed in 1..=6u64 {
            let song = chart(seed * 7 + 3, "dance-single");
            let c = song.charts[0].clone();
            for fail in [false, true] {
                let rules = || {
                    let r = make(&c);
                    if fail {
                        r
                    } else {
                        Ruleset {
                            fail: FailPolicy::Off,
                            ..r
                        }
                    }
                };
                let probe = Player::new(&song, 0, rules(), PlayOptions::default());
                let script = nasty_script(&probe, seed * 31 + 1);
                let fine = {
                    let mut k = 0u64;
                    play(&song, &rules, &script, || {
                        k += 1;
                        k as f64 * 0.001
                    })
                };
                let coarse = {
                    let mut k = 0u64;
                    play(&song, &rules, &script, || {
                        k += 1;
                        k as f64 / 37.0
                    })
                };
                // Updates judging exactly at the edges' times.
                let edges = {
                    let mut ts: Vec<f64> = script
                        .iter()
                        .map(|e| e.0 + JUDGE_LAG)
                        .filter(|&t| t > 0.0)
                        .collect();
                    ts.dedup();
                    let mut k = 0usize;
                    play(&song, &rules, &script, move || {
                        k += 1;
                        ts.get(k - 1).copied().unwrap_or(1000.0 + k as f64)
                    })
                };
                for (label, other) in [("27 ms", &coarse), ("edges", &edges)] {
                    assert_eq!(fine.0, other.0, "{name} seed {seed} fail {fail}: {label}");
                    assert_eq!(fine.1, other.1, "{name} seed {seed} fail {fail}: {label}");
                }
                let again = replay_with(&song, 0, rules(), &PlayOptions::default(), &fine.2);
                assert_eq!(again, fine.1, "{name} seed {seed} fail {fail}: replay");
                assert_eq!(PlayLog::decode(&fine.2.encode()).as_ref(), Ok(&fine.2));
            }
        }
    }
}

/// Noise never panics the decoders.
#[test]
fn codecs_survive_noise() {
    let mut rng = Rng(0xdead_beef);
    for i in 0..100_000u32 {
        let len = rng.below(64) as usize;
        let mut b: Vec<u8> = (0..len).map(|_| rng.next() as u8).collect();
        if i % 2 == 0 && !b.is_empty() {
            b[0] = 1;
        }
        let _ = PlayLog::decode(&b);
        let _ = PlayTiming::decode(&b);
    }
}

/// A long play (some 16,000 notes) and its replay take little time:
/// judging walks each lane from its first unjudged note, no further than
/// what is due.
#[test]
fn a_long_play_and_its_replay_are_quick() {
    let song = common::chart_of(5, "dance-single", 8000);
    let rules = || Ruleset {
        fail: FailPolicy::Off,
        ..presets::itg()
    };
    let mut p = Player::new(&song, 0, rules(), PlayOptions::default());
    let script = common::script(&p, 3);
    p.start(0.0, 0.0);
    let t0 = std::time::Instant::now();
    for (t, lane, pressed) in &script {
        p.input_at(*lane, *pressed, *t);
    }
    p.update_at(1e9);
    let live = t0.elapsed();
    let t1 = std::time::Instant::now();
    let r = replay_with(&song, 0, rules(), &PlayOptions::default(), p.log());
    let again = t1.elapsed();
    eprintln!(
        "{} notes, {} edges: play {live:?}, replay {again:?}",
        p.judge().notes().len(),
        script.len()
    );
    assert_eq!(r, p.results());
    assert!(p.judge().notes().len() > 15_000);
    assert!(live.as_secs_f64() < 2.0 && again.as_secs_f64() < 2.0);
}
