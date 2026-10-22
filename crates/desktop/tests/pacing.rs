//! Judging does not hang on the pacing (`ddi_desktop::pacing`): plays
//! autoplayed with no device on a fake clock, their frames at the times
//! late pacing gives them against a simulated display (60 and 144 Hz,
//! jittery builds, a late timer, a missed refresh; a variable refresh
//! rate; two frames in flight on a display link, in a window and in
//! fullscreen, where late pacing falls back to blocking frames), judged
//! the same as at a steady 60 frames a second: the same
//! tally, the same presses and releases logged (so a replay of either is
//! the other's), every hit at the same offset.

mod common;
use std::cell::RefCell;
use std::rc::Rc;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

use ddi_desktop::audio::Audio;
use ddi_desktop::pacing::sim::{self, Compositor, Display, Noise, Surface};
use ddi_desktop::pacing::{Mode, Pacer};
use ddi_desktop::play::{Ended, Feed, Play, Request, load};
use ddi_platform::HostTime;
use ddi_platform::history::MemoryPlayStore;
use ddi_ui::state::history::{PlayLog, Plays};
use ddi_ui::state::settings::Settings;

/// Frame times (s) from late pacing on `display`, for `seconds`.
fn paced(display: Display, hz: f64, seconds: f64, seed: u64) -> Vec<f64> {
    let mut pacer = Pacer::new(Mode::Late);
    pacer.set_refresh_hint(Some(hz));
    let mut surface = Surface::new(display);
    let mut cost = Noise::new(seed);
    let mut late = Noise::new(seed + 1);
    let frames = sim::run(
        &mut pacer,
        &mut surface,
        (seconds * hz) as usize,
        |i| {
            if i == 1000 {
                // A missed refresh.
                1.5 / hz
            } else {
                (0.0015 + 0.0004 * cost.normal()).max(0.0003)
            }
        },
        |_| late.uniform() * 0.001,
        None,
    );
    assert!(frames.iter().filter(|f| f.record.late.is_some()).count() > frames.len() / 2);
    frames.iter().map(|f| f.record.sample).collect()
}

/// Frame times (s) from late pacing with two frames in flight on a
/// display link at 120 Hz, the compositor keeping a replaced drawable
/// 6 ms and wanting frames 1 ms before the refresh (the ProMotion case).
fn paced_on_a_link(seconds: f64, seed: u64) -> Vec<f64> {
    let hz = 120.0;
    let mut pacer = Pacer::new(Mode::Late);
    pacer.set_refresh_hint(Some(hz));
    pacer.set_in_flight(2);
    let mut surface = Surface::new(Display::Fixed {
        period: 1.0 / hz,
        phase: 0.002,
    });
    surface.images = 3;
    surface.release = 0.006;
    surface.latch = 0.001;
    let mut l = sim::Loop {
        link: true,
        ..sim::Loop::default()
    };
    let mut cost = Noise::new(seed);
    let mut late = Noise::new(seed + 1);
    let mut cost = |i: usize| {
        if i == 1000 {
            1.5 / hz
        } else {
            (0.0015 + 0.0004 * cost.normal()).max(0.0003)
        }
    };
    let mut late = |_: usize| late.uniform() * 0.001;
    l.until(seconds, &mut pacer, &mut surface, &mut cost, &mut late);
    let frames = l.finish(&mut surface);
    assert!(frames.iter().filter(|f| f.record.late.is_some()).count() > frames.len() / 2);
    frames.iter().map(|f| f.record.sample).collect()
}

/// Frame times (s) from late pacing with two frames in flight on a
/// display link at 120 Hz in fullscreen (`Compositor::DIRECT`), where it
/// falls back to blocking frames on the way.
fn paced_in_fullscreen(seconds: f64, seed: u64) -> Vec<f64> {
    let hz = 120.0;
    let mut pacer = Pacer::new(Mode::Late);
    pacer.set_refresh_hint(Some(hz));
    pacer.set_in_flight(2);
    pacer.set_surface((3024, 1964), true);
    let mut surface = Surface::new(Display::Fixed {
        period: 1.0 / hz,
        phase: 0.002,
    });
    surface.images = 3;
    Compositor::DIRECT.apply(&mut surface);
    let mut l = sim::Loop {
        link: true,
        ..sim::Loop::default()
    };
    let mut cost = Noise::new(seed);
    let mut late = Noise::new(seed + 1);
    let mut cost = |_: usize| (0.000_35 + 0.000_1 * cost.normal()).max(0.000_2);
    let mut late = |_: usize| 0.000_2 - 0.000_35 * (1.0 - late.uniform()).ln();
    l.until(seconds, &mut pacer, &mut surface, &mut cost, &mut late);
    assert_eq!(pacer.fallbacks, 1);
    let frames = l.finish(&mut surface);
    let fell = frames.iter().filter(|f| f.record.fallback).count();
    assert!(fell > frames.len() / 2 && fell < frames.len(), "{fell}");
    frames.iter().map(|f| f.record.sample).collect()
}

/// Plays chart `chart` of `song` with frames at `times` (after the
/// play's start at 1 s), to its end.
fn play(
    loaded: &Rc<load::Loaded>,
    song: &str,
    chart: usize,
    times: &[f64],
) -> (ddi_engine::player::Results, PlayLog) {
    let clock = Arc::new(AtomicU64::new(1_000_000_000));
    let c = clock.clone();
    let audio = Audio::headless(1.0, Some(Arc::new(move || c.load(Ordering::SeqCst))));
    let feed = Rc::new(RefCell::new(Feed::default()));
    let store = Rc::new(MemoryPlayStore::default());
    let plays = Plays::new(store.clone());
    let mut p = Play::start(
        Request::song(song, chart, true),
        loaded.clone(),
        &Settings::default(),
        audio,
        feed,
        None,
        60.0,
    )
    .unwrap();
    for t in times {
        let now = 1_000_000_000 + (t * 1e9) as u64;
        clock.store(now, Ordering::SeqCst);
        match p.tick(HostTime(now as f64 / 1e9), Some(&plays)) {
            Some(Ended::Finished { results, .. }) => {
                assert_eq!(store.len(), 1, "{song} {chart}: kept once");
                return (*results, p.session.player.log().clone());
            }
            Some(_) => panic!("{song} {chart}: the play did not finish"),
            None => {}
        }
    }
    panic!("{song} {chart}: never ended in {} s", times.last().unwrap());
}

#[test]
fn late_pacing_judges_as_steady_frames_do() {
    common::indexes();
    let seconds = 300.0;
    let steady: Vec<f64> = (1..(seconds * 60.0) as usize)
        .map(|i| i as f64 / 60.0)
        .collect();
    let runs = [
        (
            "late, 60 Hz",
            paced(
                Display::Fixed {
                    period: 1.0 / 60.0,
                    phase: 0.004,
                },
                60.0,
                seconds,
                1,
            ),
        ),
        (
            "late, 144 Hz",
            paced(
                Display::Fixed {
                    period: 1.0 / 144.0,
                    phase: 0.001,
                },
                144.0,
                seconds,
                2,
            ),
        ),
        (
            "late, VRR 48–144 Hz",
            paced(
                Display::Vrr {
                    min: 1.0 / 144.0,
                    max: 1.0 / 48.0,
                },
                144.0,
                seconds,
                3,
            ),
        ),
        (
            "late, two in flight, display link, 120 Hz",
            paced_on_a_link(seconds, 4),
        ),
        (
            "late, two in flight, 120 Hz fullscreen, falling back",
            paced_in_fullscreen(seconds, 5),
        ),
    ];
    let songs = ddi_desktop::library::bundled();
    assert!(!songs.is_empty(), "no bundled songs (xtask gen-songs)");
    let mut charts = 0;
    for entry in songs {
        let loaded = Rc::new(load::load(&entry.id, 48_000).unwrap());
        for chart in 0..loaded.song.charts.len() {
            if loaded.song.layout_of(&loaded.song.charts[chart]).is_none() {
                continue;
            }
            charts += 1;
            let (r0, log0) = play(&loaded, &entry.id, chart, &steady);
            let t0 = &r0.tally;
            assert_eq!(t0.taps[1..].iter().sum::<u32>(), 0, "{} {chart}", entry.id);
            for (name, times) in &runs {
                let (r, log) = play(&loaded, &entry.id, chart, times);
                let what = format!("{} chart {chart}, {name}", entry.id);
                assert_eq!(r.tally, r0.tally, "{what}");
                assert_eq!(r.tally.let_go, 0, "{what}");
                assert!(!r.failed, "{what}");
                assert_eq!(r.late_edges, 0, "{what}");
                assert_eq!(log, log0, "{what}: the presses logged");
                // Every hit judged at the same offset.
                assert_eq!(
                    (r.mean_delta, r.stddev_delta, r.fast, r.slow, r.max_combo),
                    (
                        r0.mean_delta,
                        r0.stddev_delta,
                        r0.fast,
                        r0.slow,
                        r0.max_combo
                    ),
                    "{what}"
                );
            }
        }
    }
    assert!(charts > 10, "{charts}");
}
