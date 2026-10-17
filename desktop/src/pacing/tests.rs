//! The pacer on a fake clock: its grid, then whole runs against the
//! simulated display (`sim`) at 60, 120 and 144 Hz with jittery costs, a
//! missed refresh, a compositor's deadline it is not told, a late timer
//! and a variable refresh rate, each against blocking frames.

use super::sim::{self, Display, Noise, Surface};
use super::*;

/// What really happened from frame `from` on.
#[derive(Debug)]
struct Truth {
    /// The clock read to the frame on the glass (ms).
    latency: f64,
    /// On the glass minus the drawn-for time (ms): mean and sd.
    error: (f64, f64),
    /// Refreshes that showed no new frame.
    repeats: usize,
    /// Share of the time spent in the acquire.
    blocked: f64,
    /// Glass to glass (ms): mean and sd.
    interval: (f64, f64),
}

fn mean_sd(v: &[f64]) -> (f64, f64) {
    let n = v.len() as f64;
    let m = v.iter().sum::<f64>() / n;
    let var = v.iter().map(|x| (x - m) * (x - m)).sum::<f64>() / n;
    (m, var.sqrt())
}

/// The truth of a run; the drawn-for time is the pacer's when late
/// pacing gave one, else the play's own (the clock read plus the period,
/// what `PlaySession::predicted_present` gives at a steady rate).
fn truth(frames: &[sim::Frame], surface: &Surface, from: usize, period: f64) -> Truth {
    let fs = &frames[from..];
    let lat: Vec<f64> = fs
        .iter()
        .map(|f| (f.glass.unwrap() - f.record.sample) * 1000.0)
        .collect();
    let err: Vec<f64> = fs
        .iter()
        .map(|f| {
            let drawn = if f.record.mode == Mode::Late && f.record.locked {
                f.record.predicted
            } else {
                f.record.sample + period
            };
            (f.glass.unwrap() - drawn) * 1000.0
        })
        .collect();
    let first = fs[0].glass.unwrap();
    let last = fs[fs.len() - 1].glass.unwrap();
    let repeats = surface
        .refreshes
        .iter()
        .filter(|(g, id)| *g > first && *g < last && id.is_none())
        .count();
    let span = fs[fs.len() - 1].record.end - fs[0].record.begin;
    let blocked = fs.iter().map(|f| f.record.blocked).sum::<f64>() / span;
    let glass: Vec<f64> = fs
        .windows(2)
        .map(|w| (w[1].glass.unwrap() - w[0].glass.unwrap()) * 1000.0)
        .collect();
    Truth {
        latency: mean_sd(&lat).0,
        error: mean_sd(&err),
        repeats,
        blocked,
        interval: mean_sd(&glass),
    }
}

fn fixed(hz: f64) -> Surface {
    Surface::new(Display::Fixed {
        period: 1.0 / hz,
        phase: 0.003_7,
    })
}

/// Builds of about 1.5 ms, sd 0.3 ms.
fn jittery(seed: u64) -> impl FnMut(usize) -> f64 {
    let mut n = Noise::new(seed);
    move |_| (0.001_5 + 0.000_3 * n.normal()).max(0.000_3)
}

/// A timer that wakes up to `max` late.
fn lateness(seed: u64, max: f64) -> impl FnMut(usize) -> f64 {
    let mut n = Noise::new(seed);
    move |_| n.uniform() * max
}

fn run_mode(
    mode: Mode,
    mut surface: Surface,
    hint: Option<f64>,
    seconds: f64,
    cost: impl FnMut(usize) -> f64,
    late: impl FnMut(usize) -> f64,
) -> (Vec<sim::Frame>, Surface, Pacer) {
    let mut pacer = Pacer::new(mode);
    pacer.set_refresh_hint(hint);
    let hz = hint.unwrap_or(60.0);
    let frames = (seconds * hz) as usize;
    let out = sim::run(&mut pacer, &mut surface, frames, cost, late, None);
    (out, surface, pacer)
}

#[test]
fn the_grid_locks_and_measures_the_period() {
    // A 119.88 Hz display the OS calls 120; returns up to 0.2 ms after
    // each refresh.
    let p = 1.0 / 119.88;
    let mut g = Grid::default();
    g.set_hint(Some(120.0));
    let mut n = Noise::new(7);
    for k in 0..240 {
        g.observe(0.010 + k as f64 * p + n.uniform() * 0.000_2);
    }
    assert!(g.locked_at(2.0));
    assert!((g.period() - p).abs() < p * 1e-4, "{}", g.period());
    let at = g.next_after(1.5);
    let true_next = 0.010 + ((1.5 - 0.010) / p).ceil() * p;
    assert!((at - true_next).abs() < 0.000_2, "{at} {true_next}");
    // Stale: nothing seen for over two seconds.
    assert!(!g.locked_at(5.0));
}

#[test]
fn a_wrong_hint_gives_way_to_what_is_measured() {
    let p = 1.0 / 120.0;
    let mut g = Grid::default();
    g.set_hint(Some(60.0));
    for k in 0..60 {
        g.observe(0.002 + k as f64 * p);
    }
    assert!((g.period() - p).abs() < 1e-6, "{}", g.period());
    assert!(g.locked_at(0.5));
}

#[test]
fn stray_returns_do_not_move_the_grid() {
    let p = 1.0 / 60.0;
    let mut g = Grid::default();
    g.set_hint(Some(60.0));
    let mut n = Noise::new(3);
    for k in 0..120 {
        let o = if k % 10 == 5 {
            // An acquire held up by something else.
            0.001 + k as f64 * p + n.uniform() * p
        } else {
            0.001 + k as f64 * p + n.uniform() * 0.000_1
        };
        g.observe(o);
    }
    assert!(g.locked_at(2.0));
    let at = g.next_after(1.0);
    let true_next = 0.001 + ((1.0 - 0.001) / p).ceil() * p;
    assert!((at - true_next).abs() < 0.000_2, "{at} {true_next}");
}

#[test]
fn returns_with_no_fixed_rhythm_are_no_grid() {
    let mut g = Grid::default();
    g.set_hint(Some(60.0));
    let mut n = Noise::new(11);
    let mut t = 0.0;
    for _ in 0..60 {
        t += 0.008 + n.uniform() * 0.02;
        g.observe(t);
    }
    assert!(!g.locked_at(t));
}

#[test]
fn blocking_frames_never_sleep() {
    let (frames, _, pacer) = run_mode(
        Mode::Block,
        fixed(60.0),
        Some(60.0),
        3.0,
        jittery(1),
        |_| 0.0,
    );
    assert!(frames.iter().all(|f| f.record.late.is_none()));
    assert!(pacer.next(10.0) == Next::Now);
    // They wait in the acquire most of the time.
    let t = truth(&frames, &fixed(60.0), 30, 1.0 / 60.0);
    assert!(t.blocked > 0.7, "{t:?}");
}

#[test]
fn late_pacing_reads_input_closer_to_the_refresh() {
    for hz in [60.0, 120.0, 144.0] {
        let p = 1.0 / hz;
        let (bf, bs, _) = run_mode(Mode::Block, fixed(hz), Some(hz), 20.0, jittery(2), |_| 0.0);
        let (lf, ls, pacer) = run_mode(
            Mode::Late,
            fixed(hz),
            Some(hz),
            20.0,
            jittery(2),
            lateness(5, 0.000_3),
        );
        let from = (2.0 * hz) as usize;
        let b = truth(&bf, &bs, from, p);
        let l = truth(&lf, &ls, from, p);
        eprintln!(
            "{hz} Hz: block {b:?}\n        late {l:?}\n        margin {:.2} ms, budget {:.2} ms",
            pacer.margin() * 1000.0,
            pacer.budget() * 1000.0
        );
        assert_eq!(b.repeats, 0, "{hz}");
        assert_eq!(l.repeats, 0, "{hz}");
        assert_eq!(pacer.latch_misses, 0, "{hz}");
        // A refresh's worth less, but for the budget.
        assert!(
            l.latency < b.latency - 0.3 * p * 1000.0,
            "{hz}: {} vs {}",
            l.latency,
            b.latency
        );
        assert!(l.latency < pacer.budget() * 1000.0 + 0.5, "{hz}");
        // Drawn for the same moment the blocking frames' prediction
        // gives, so a calibrated visual offset holds in both modes.
        assert!((l.error.0 - b.error.0).abs() < 0.3, "{hz}: {l:?} {b:?}");
        assert!(l.error.1 < 0.2, "{hz}: {l:?}");
        // Hardly ever waiting in the acquire (the probes only).
        assert!(l.blocked < 0.02 && b.blocked > 0.5, "{hz}: {l:?} {b:?}");
        // One frame a refresh.
        assert!((l.interval.0 - p * 1000.0).abs() < 0.01, "{hz}: {l:?}");
        assert!(l.interval.1 < 0.01, "{hz}: {l:?}");
    }
}

#[test]
fn a_missed_refresh_costs_one_repeat_and_no_more() {
    let hz = 60.0;
    let p = 1.0 / hz;
    let mut jitter = jittery(9);
    let cost = move |i: usize| if i == 600 { 1.2 * p } else { jitter(i) };
    let (frames, surface, pacer) = run_mode(Mode::Late, fixed(hz), Some(hz), 20.0, cost, |_| 0.0);
    let t = truth(&frames, &surface, 120, p);
    assert_eq!(t.repeats, 1, "{t:?}");
    assert!(frames[600].record.overrun);
    assert_eq!(pacer.latch_misses, 0);
    // Back on time at once: the frames after it as close as before.
    let after = truth(&frames, &surface, 610, p);
    let before = truth(&frames[..590], &surface, 120, p);
    assert!(
        (after.latency - before.latency).abs() < 0.5,
        "{after:?} {before:?}"
    );
    assert_eq!(after.repeats, 0);
}

#[test]
fn a_compositor_deadline_is_learned() {
    // Frames must be presented 3 ms before the refresh; the pacer is not
    // told, and starts with a 2 ms margin.
    let hz = 60.0;
    let p = 1.0 / hz;
    let mut surface = fixed(hz);
    surface.latch = 0.003;
    let (frames, surface, pacer) =
        run_mode(Mode::Late, surface, Some(hz), 30.0, jittery(4), |_| 0.0);
    assert!(
        (1..=5).contains(&pacer.latch_misses),
        "{}",
        pacer.latch_misses
    );
    let last = truth(&frames, &surface, (20.0 * hz) as usize, p);
    assert_eq!(last.repeats, 0, "{last:?}");
    let missed: Vec<usize> = frames
        .iter()
        .enumerate()
        .filter(|(_, f)| f.record.latch_miss)
        .map(|(i, _)| i)
        .collect();
    eprintln!(
        "deadline: misses at frames {missed:?}, floor {:.2} ms, margin {:.2} ms",
        pacer.floor() * 1000.0,
        pacer.margin() * 1000.0
    );
    assert!(
        missed.iter().all(|i| *i < (20.0 * hz) as usize),
        "{missed:?}"
    );
    assert!(pacer.margin() > 0.003);
    // Still well ahead of blocking frames.
    let (bf, bs, _) = run_mode(
        Mode::Block,
        {
            let mut s = fixed(hz);
            s.latch = 0.003;
            s
        },
        Some(hz),
        30.0,
        jittery(4),
        |_| 0.0,
    );
    let b = truth(&bf, &bs, (15.0 * hz) as usize, p);
    assert!(last.latency < b.latency - 5.0, "{last:?} {b:?}");
}

#[test]
fn a_late_timer_is_budgeted() {
    for hz in [60.0, 144.0] {
        let (frames, surface, pacer) = run_mode(
            Mode::Late,
            fixed(hz),
            Some(hz),
            20.0,
            jittery(6),
            lateness(8, 0.002),
        );
        let t = truth(&frames, &surface, (2.0 * hz) as usize, 1.0 / hz);
        assert_eq!(t.repeats, 0, "{hz}: {t:?}");
        assert!(pacer.latch_misses <= 1, "{hz}");
    }
}

#[test]
fn a_variable_refresh_rate_stays_smooth() {
    let hz = 144.0;
    let p = 1.0 / hz;
    let vrr = || {
        Surface::new(Display::Vrr {
            min: p,
            max: 1.0 / 48.0,
        })
    };
    let (lf, ls, pacer) = run_mode(
        Mode::Late,
        vrr(),
        Some(hz),
        20.0,
        jittery(12),
        lateness(13, 0.000_3),
    );
    let (bf, bs, _) = run_mode(Mode::Block, vrr(), Some(hz), 20.0, jittery(12), |_| 0.0);
    let from = (2.0 * hz) as usize;
    let l = truth(&lf, &ls, from, p);
    let b = truth(&bf, &bs, from, p);
    eprintln!("VRR: block {b:?}\n     late {l:?}");
    assert_eq!(l.repeats, 0, "{l:?}");
    assert!((l.interval.0 - p * 1000.0).abs() < 0.05, "{l:?}");
    assert!(l.interval.1 < 0.3, "{l:?}");
    assert!(l.latency < b.latency, "{l:?} {b:?}");
    assert!(l.error.1 < 0.3, "{l:?}");
    assert!(pacer.latch_misses <= 1);
}

#[test]
fn a_new_play_looks_for_the_grid_again() {
    let (_, _, mut pacer) = run_mode(Mode::Late, fixed(60.0), Some(60.0), 3.0, jittery(1), |_| {
        0.0
    });
    assert!(pacer.grid().locked_at(2.9));
    pacer.restart();
    assert_eq!(pacer.next(3.0), Next::Now);
    assert!(!pacer.grid().locked_at(3.0));
    // The period stays measured.
    assert!(pacer.grid().measured());
}

#[test]
fn the_report_compares_both_modes_in_one_run() {
    let hz = 120.0;
    let mut surface = fixed(hz);
    let mut pacer = Pacer::new(Mode::Block);
    pacer.set_refresh_hint(Some(hz));
    let mut report = Report::alternating(
        2.0,
        &[Column::new(Mode::Block, 1), Column::new(Mode::Late, 1)],
    );
    report.hint = Some(hz);
    sim::run(
        &mut pacer,
        &mut surface,
        (20.0 * hz) as usize,
        jittery(21),
        lateness(22, 0.000_3),
        Some(&mut report),
    );
    let b = report.figures(Column::new(Mode::Block, 1));
    let l = report.figures(Column::new(Mode::Late, 1));
    let text = report.text("simulated, 120 Hz");
    eprintln!("{text}");
    assert!(b.frames > 800 && l.frames > 800, "{b:?} {l:?}");
    assert!(l.latency[0] < b.latency[0] - 2.0, "{b:?} {l:?}");
    assert_eq!(l.repeats, 0, "{l:?}");
    assert_eq!(b.repeats, 0, "{b:?}");
    assert!(l.blocked_share < 0.05 && b.blocked_share > 0.5);
    assert!(text.contains("input read → shown"));
}

#[test]
fn compositor_hiccups_do_not_raise_the_floor() {
    // Ten minutes at 60 Hz; every 20 s one refresh wants its frame 5 ms
    // early (a compositor's hiccup): each costs a miss, none a lasting
    // margin.
    let hz = 60.0;
    let mut pacer = Pacer::new(Mode::Late);
    pacer.set_refresh_hint(Some(hz));
    let mut surface = fixed(hz);
    let mut l = sim::Loop::default();
    let mut cost = jittery(31);
    let mut late = |_| 0.000_1;
    let mut t = 0.0;
    while t < 600.0 {
        t += 20.0;
        l.until(t - 0.02, &mut pacer, &mut surface, &mut cost, &mut late);
        surface.latch = 0.005;
        l.until(t, &mut pacer, &mut surface, &mut cost, &mut late);
        surface.latch = 0.0;
    }
    l.until(605.0, &mut pacer, &mut surface, &mut cost, &mut late);
    let frames = l.finish(&mut surface);
    let at = |s: f64| frames.iter().position(|f| f.record.begin >= s).unwrap();
    let first = truth(&frames[..at(60.0)], &surface, at(2.0), 1.0 / hz);
    let last = truth(&frames[..at(600.0)], &surface, at(540.0), 1.0 / hz);
    eprintln!(
        "hiccups: {} misses, floor {:.2} ms; first minute {first:?}\n         last minute {last:?}",
        pacer.latch_misses,
        pacer.floor() * 1000.0
    );
    assert!(pacer.latch_misses >= 20, "{}", pacer.latch_misses);
    assert!(
        pacer.floor() <= MARGIN + FLOOR_STEP + 1e-9,
        "{}",
        pacer.floor()
    );
    assert!(last.latency < first.latency + 0.5, "{first:?} {last:?}");
}

#[test]
fn the_floor_comes_down_without_misses() {
    // A 3 ms deadline learned, then gone: the floor steps down a minute
    // at a time without misses.
    let hz = 60.0;
    let mut pacer = Pacer::new(Mode::Late);
    pacer.set_refresh_hint(Some(hz));
    let mut surface = fixed(hz);
    surface.latch = 0.003;
    let mut l = sim::Loop::default();
    let mut cost = jittery(41);
    let mut late = |_| 0.000_1;
    l.until(30.0, &mut pacer, &mut surface, &mut cost, &mut late);
    let learned = pacer.floor();
    assert!(learned > 0.003, "{learned}");
    surface.latch = 0.0;
    l.until(160.0, &mut pacer, &mut surface, &mut cost, &mut late);
    assert!(
        pacer.floor() <= learned - 2.0 * FLOOR_STEP + 1e-9,
        "{}",
        pacer.floor()
    );
    // A new play starts from the floor, which keeps coming down a step a
    // hold (counted from its last move) until it is the first one.
    let before = pacer.floor();
    pacer.restart();
    assert_eq!(pacer.margin(), before);
    l.until(400.0, &mut pacer, &mut surface, &mut cost, &mut late);
    assert_eq!(pacer.floor(), MARGIN);
}

#[test]
fn late_pacing_finds_a_faster_refresh_by_itself() {
    // 60 Hz, then 120 Hz with the OS's rate never looked at again.
    let mut pacer = Pacer::new(Mode::Late);
    pacer.set_refresh_hint(Some(60.0));
    let mut surface = fixed(60.0);
    let mut l = sim::Loop::default();
    let mut cost = jittery(51);
    let mut late = |_| 0.000_1;
    l.until(10.0, &mut pacer, &mut surface, &mut cost, &mut late);
    assert!(l.records.last().unwrap().measured);
    surface.set_period(1.0 / 120.0);
    l.until(30.0, &mut pacer, &mut surface, &mut cost, &mut late);
    assert!(pacer.faster_found >= 1);
    assert!(
        (pacer.grid().period() - 1.0 / 120.0).abs() < 1e-5,
        "{}",
        pacer.grid().period()
    );
    let frames = l.finish(&mut surface);
    let from = frames.iter().position(|f| f.record.begin >= 20.0).unwrap();
    let t = truth(&frames, &surface, from, 1.0 / 120.0);
    eprintln!("60 → 120 Hz: {t:?}");
    assert!((t.interval.0 - 1000.0 / 120.0).abs() < 0.05, "{t:?}");
    assert_eq!(t.repeats, 0, "{t:?}");
    assert!(t.latency < 5.0, "{t:?}");
}

/// A run of `seconds` through the report's four columns on a 120 Hz
/// display whose compositor keeps a replaced drawable `release` and wants
/// frames `latch` before the refresh, with or without a display link.
fn promotion(release: f64, latch: f64, link: bool, seconds: f64) -> Report {
    let hz = 120.0;
    let mut surface = fixed(hz);
    surface.release = release;
    surface.latch = latch;
    let mut pacer = Pacer::new(Mode::Block);
    pacer.set_refresh_hint(Some(hz));
    let mut report = Report::alternating(2.0, &COLUMNS);
    report.hint = Some(hz);
    report.link = link.then_some("simulated");
    let mut l = sim::Loop {
        link,
        ..sim::Loop::default()
    };
    let mut cost = jittery(61);
    let mut late = lateness(62, 0.000_3);
    while l.t < seconds {
        l.step(
            &mut pacer,
            &mut surface,
            &mut cost,
            &mut late,
            Some(&mut report),
        );
    }
    report
}

#[test]
fn a_display_link_is_the_grid_from_the_first_frame() {
    let hz = 120.0;
    let mut surface = fixed(hz);
    let mut pacer = Pacer::new(Mode::Late);
    pacer.set_refresh_hint(Some(hz));
    let mut l = sim::Loop {
        link: true,
        ..sim::Loop::default()
    };
    let mut cost = jittery(71);
    let mut late = |_| 0.000_1;
    l.until(5.0, &mut pacer, &mut surface, &mut cost, &mut late);
    let frames = l.finish(&mut surface);
    // Planned from the second frame on: no blocking frames to lock, no
    // probes.
    assert!(frames[1..].iter().all(|f| f.record.late.is_some()));
    assert!(frames.iter().all(|f| !f.record.probe));
    assert!(frames[1..].iter().all(|f| f.record.source == Source::Link));
    let t = truth(&frames, &surface, 10, 1.0 / hz);
    assert_eq!(t.repeats, 0, "{t:?}");
    assert!(t.latency < 5.0, "{t:?}");
    assert!(t.error.1 < 0.05, "{t:?}");
}

#[test]
fn a_compositor_that_keeps_drawables_needs_two_in_flight() {
    // The maintainer's ProMotion report: block repeats refreshes with
    // builds of 1–2 ms. A compositor that keeps a replaced drawable 6 ms
    // and wants frames 1 ms before the refresh does that to one frame in
    // flight; two keep every refresh, and late pacing on a display link
    // keeps them with less latency.
    let report = promotion(0.006, 0.001, true, 40.0);
    eprintln!(
        "{}",
        report.text("simulated ProMotion: release 6 ms, latch 1 ms")
    );
    let f = |m, n| report.figures(Column::new(m, n));
    let (b1, b2, l2) = (f(Mode::Block, 1), f(Mode::Block, 2), f(Mode::Late, 2));
    assert!(b1.repeats > 100, "{b1:?}");
    // (a few at the switches into it, where the queue fills)
    assert!(b2.repeats <= 5, "{b2:?}");
    assert!(l2.repeats <= 2, "{l2:?}");
    assert!(l2.polled[0] < b2.polled[0] - 4.0, "{l2:?} {b2:?}");
    assert!(l2.from_link > 0.99);
    // Released promptly, one in flight is enough for late pacing too.
    let report = promotion(0.000_1, 0.001, true, 24.0);
    let l1 = report.figures(Column::new(Mode::Late, 1));
    let b1 = report.figures(Column::new(Mode::Block, 1));
    assert!(l1.repeats <= 2, "{l1:?}");
    assert_eq!(b1.repeats, 0, "{b1:?}");
}

/// The maintainer's M1 Max as the 2026-10-17 report has it: 120 Hz on a
/// display link, builds of about 1.1 ms, a timer late by 0.5 ms on
/// average (an exponential tail, p95 1.5 ms), a compositor that keeps a
/// replaced drawable `release` ± `sd` and wants frames `latch` before
/// the refresh, `hiccups` of its refreshes wanting them `hiccup` before.
#[derive(Clone, Copy, Debug)]
struct Mac {
    release: f64,
    sd: f64,
    latch: f64,
    hiccups: f64,
    hiccup: f64,
}

const MAC: Mac = Mac {
    release: 0.0045,
    sd: 0.0007,
    latch: 0.001,
    hiccups: 0.004,
    hiccup: 0.006,
};

fn mac_report(m: Mac, columns: &[Column], seconds: f64, seed: u64) -> Report {
    let hz = 120.0;
    let mut surface = fixed(hz);
    surface.release = m.release;
    surface.release_sd = m.sd;
    surface.latch = m.latch;
    surface.hiccups = m.hiccups;
    surface.hiccup_latch = m.hiccup;
    let mut pacer = Pacer::new(columns[0].mode);
    pacer.set_in_flight(columns[0].in_flight);
    surface.images = columns[0].in_flight as usize + 1;
    pacer.set_refresh_hint(Some(hz));
    let mut report = Report::alternating(4.0, columns);
    report.hint = Some(hz);
    report.link = Some("simulated");
    let mut l = sim::Loop {
        link: true,
        ..sim::Loop::default()
    };
    let mut n = Noise::new(seed);
    let mut cost = move |_| (0.0011 + 0.0003 * n.normal()).max(0.0003);
    let mut w = Noise::new(seed + 1);
    let mut late = move |_| -0.0005 * (1.0 - w.uniform()).ln();
    while l.t < seconds {
        l.step(
            &mut pacer,
            &mut surface,
            &mut cost,
            &mut late,
            Some(&mut report),
        );
    }
    report
}

#[test]
#[ignore = "prints the simulated report with the Mac's parameters"]
fn the_macs_report_simulated() {
    let r = mac_report(MAC, &COLUMNS, 100.0, 81);
    eprintln!("{}", r.text("simulated M1 Max, windowed"));
    for c in [Column::new(Mode::Late, 2), Column::new(Mode::Block, 1)] {
        let r = mac_report(MAC, &[c], 100.0, 82);
        eprintln!(
            "{}",
            r.text(&format!("simulated M1 Max, {} alone", c.name()))
        );
    }
}

#[test]
#[ignore = "prints late·2's misses over five minutes of the Mac's parameters"]
fn late_two_on_the_mac_over_five_minutes() {
    let c = Column::new(Mode::Late, 2);
    let r = mac_report(MAC, &[c], 600.0, 83);
    eprintln!("{}", r.text("simulated M1 Max, late·2, 10 min"));
    let m = r.misses();
    let per_minute: Vec<usize> = (0..10)
        .map(|k| m.iter().filter(|t| (**t / 60.0) as usize == k).count())
        .collect();
    eprintln!("misses a minute: {per_minute:?}");
}

#[test]
fn late_two_learns_the_macs_compositor_and_keeps_it_for_the_next_song() {
    // The Mac's parameters (`MAC`): late pacing with two in flight on a
    // display link. Misses while the margin is learned, few after; the
    // next song, half a minute later, starts from what was learned.
    let hz = 120.0;
    let mut surface = fixed(hz);
    surface.release = MAC.release;
    surface.release_sd = MAC.sd;
    surface.latch = MAC.latch;
    surface.hiccups = MAC.hiccups;
    surface.hiccup_latch = MAC.hiccup;
    surface.images = 3;
    let mut pacer = Pacer::new(Mode::Late);
    pacer.set_in_flight(2);
    pacer.set_refresh_hint(Some(hz));
    let mut l = sim::Loop {
        link: true,
        ..sim::Loop::default()
    };
    let mut n = Noise::new(91);
    let mut cost = move |_| (0.0011 + 0.0003 * n.normal()).max(0.0003);
    let mut w = Noise::new(92);
    let mut late = move |_| -0.0005 * (1.0 - w.uniform()).ln();
    l.until(120.0, &mut pacer, &mut surface, &mut cost, &mut late);
    let first: Vec<f64> = l
        .records
        .iter()
        .filter(|r| r.latch_miss)
        .map(|r| r.begin)
        .collect();
    // The results, song select: half a minute with no frames.
    l.t += 30.0;
    surface.advance(l.t);
    pacer.restart();
    let from = l.records.len();
    l.until(270.0, &mut pacer, &mut surface, &mut cost, &mut late);
    let second: Vec<f64> = l.records[from..]
        .iter()
        .filter(|r| r.latch_miss)
        .map(|r| r.begin)
        .collect();
    let early = |m: &[f64], start: f64| m.iter().filter(|t| **t < start + 30.0).count();
    let late_misses = |m: &[f64], start: f64| m.iter().filter(|t| **t >= start + 30.0).count();
    eprintln!(
        "first song: {} misses in its first 30 s, {} after; second: {} and {}; floor {:.2} ms",
        early(&first, 0.0),
        late_misses(&first, 0.0),
        early(&second, 150.0),
        late_misses(&second, 150.0),
        pacer.floor() * 1000.0
    );
    // About two a minute once learned (the compositor's stalls past the
    // margin), against 18 in 21 s on the Mac with the margin capped.
    assert!(late_misses(&first, 0.0) <= 6, "{first:?}");
    assert!(
        early(&second, 150.0) <= early(&first, 0.0) / 2 + 1,
        "{first:?} {second:?}"
    );
    assert!(late_misses(&second, 150.0) <= 6, "{second:?}");
}

#[test]
fn the_shown_estimate_agrees_with_the_glass_on_the_macs_parameters() {
    // What the report says a frame waits (its start to the refresh the
    // grid gives it) against when the simulated display showed it, for
    // each column alone, mean over 20 s after 2 s.
    for c in COLUMNS {
        let hz = 120.0;
        let mut surface = fixed(hz);
        surface.release = MAC.release;
        surface.release_sd = MAC.sd;
        surface.latch = MAC.latch;
        surface.images = c.in_flight as usize + 1;
        let mut pacer = Pacer::new(c.mode);
        pacer.set_in_flight(c.in_flight);
        pacer.set_refresh_hint(Some(hz));
        let mut l = sim::Loop {
            link: true,
            ..sim::Loop::default()
        };
        let mut n = Noise::new(101);
        let mut cost = move |_| (0.0011 + 0.0003 * n.normal()).max(0.0003);
        let mut w = Noise::new(102);
        let mut late = move |_| -0.0005 * (1.0 - w.uniform()).ln();
        l.until(22.0, &mut pacer, &mut surface, &mut cost, &mut late);
        let frames = l.finish(&mut surface);
        let fs: Vec<&sim::Frame> = frames
            .iter()
            .filter(|f| f.record.begin > 2.0 && f.glass.is_some())
            .collect();
        let n = fs.len() as f64;
        let est = fs
            .iter()
            .map(|f| f.record.target - f.record.begin)
            .sum::<f64>()
            / n
            * 1000.0;
        let glass = fs
            .iter()
            .map(|f| f.glass.unwrap() - f.record.begin)
            .sum::<f64>()
            / n
            * 1000.0;
        eprintln!(
            "{}: frame start → shown, estimated {est:.2} ms, on the glass {glass:.2} ms",
            c.name()
        );
        // Within half a refresh where nothing misses its refresh (block·1
        // and late·1 do here, the estimate not knowing the deadline).
        if c.in_flight == 2 || c.mode == Mode::Block {
            assert!((est - glass).abs() < 4.2, "{}: {est} {glass}", c.name());
        }
    }
}
