//! Plays on desktop with no device (`Audio::headless` on a fake clock):
//! every chart of every bundled song autoplayed to its end at 60 and 144
//! frames a second, all top tier, kept in the history with its log; and
//! the same through the test pad (`--auto=pad`). `DDI_PLAY_DUMP=<file>`
//! writes each play's song, chart, frame rate and logged inputs as JSON
//! lines (to compare with the web's own plays of the same charts).

mod common;
use std::cell::RefCell;
use std::io::Write;
use std::rc::Rc;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

use ddi_desktop::audio::Audio;
use ddi_desktop::play::{Ended, Feed, Play, Request, load};
use ddi_platform::HostTime;
use ddi_platform::history::MemoryPlayStore;
use ddi_ui::state::history::{PlayLog, Plays};
use ddi_ui::state::settings::Settings;

/// Plays chart `chart` of `song` to its end at `fps` frames a second;
/// the results and the log kept.
fn play(
    loaded: &Rc<load::Loaded>,
    song: &str,
    chart: usize,
    fps: f64,
    auto_pad: bool,
) -> (ddi_engine::player::Results, PlayLog) {
    let clock = Arc::new(AtomicU64::new(1_000_000_000));
    let c = clock.clone();
    let audio = Audio::headless(1.0, Some(Arc::new(move || c.load(Ordering::SeqCst))));
    let feed = Rc::new(RefCell::new(Feed::default()));
    let store = Rc::new(MemoryPlayStore::default());
    let plays = Plays::new(store.clone());
    let mut p = Play::start(
        Request {
            auto_pad,
            ..Request::song(song, chart, true)
        },
        loaded.clone(),
        &Settings::default(),
        audio,
        feed,
        None,
        fps,
    )
    .unwrap();
    let step = (1e9 / fps) as u64;
    let results = loop {
        let now = clock.fetch_add(step, Ordering::SeqCst) + step;
        match p.tick(HostTime(now as f64 / 1e9), Some(&plays)) {
            Some(Ended::Finished { results, .. }) => break *results,
            Some(_) => panic!("{song} {chart}: the play did not finish"),
            None => {}
        }
        assert!(now < 1_000_000_000 * 1000, "{song} {chart}: never ends");
    };
    assert_eq!(store.len(), 1, "{song} {chart}: kept once");
    let history = plays.history();
    assert_eq!(history.len(), 1);
    drop(history);
    let log = p.session.player.log().clone();
    (results, log)
}

#[test]
fn bundled_songs_autoplay_all_top_tier() {
    common::indexes();
    let dump =
        std::env::var_os("DDI_PLAY_DUMP").map(|p| std::fs::File::create(p).expect("dump file"));
    let dump = RefCell::new(dump);
    let songs = ddi_desktop::library::bundled();
    assert!(!songs.is_empty(), "no bundled songs (xtask gen-songs)");
    for entry in songs {
        let loaded = Rc::new(load::load(&entry.id, 48_000).unwrap());
        for chart in 0..loaded.song.charts.len() {
            if loaded.song.layout_of(&loaded.song.charts[chart]).is_none() {
                continue;
            }
            for fps in [60.0, 144.0] {
                let (r, log) = play(&loaded, &entry.id, chart, fps, false);
                let t = &r.tally;
                let below: u32 = t.taps[1..].iter().sum();
                assert_eq!(
                    below, 0,
                    "{} chart {chart} at {fps}: {:?}",
                    entry.id, t.taps
                );
                assert_eq!(t.let_go, 0, "{} chart {chart} at {fps}", entry.id);
                assert!(!r.failed);
                if let Some(f) = dump.borrow_mut().as_mut() {
                    let line = serde_json::json!({
                        "song": entry.id,
                        "chart": chart,
                        "fps": fps,
                        "taps": t.taps,
                        "inputs": log.inputs,
                    });
                    writeln!(f, "{line}").unwrap();
                }
            }
        }
    }
}

#[test]
fn autoplay_through_the_test_pad() {
    common::indexes();
    let loaded = Rc::new(load::load("beatucada", 48_000).unwrap());
    let (r, plain) = play(&loaded, "beatucada", 1, 60.0, false);
    let (rp, pad) = play(&loaded, "beatucada", 1, 60.0, true);
    assert_eq!(rp.tally.taps, r.tally.taps);
    assert_eq!(rp.tally.taps[1..].iter().sum::<u32>(), 0);
    // The pad's presses come in on the frame after they are due: the
    // same lanes, judged at their own times.
    assert_eq!(pad.inputs.len(), plain.inputs.len());
}

/// The web's plays of the same charts, judged the same: `DDI_WEB_PLAYS`
/// names a file of the web's plays (one a line, `{song, chart, record,
/// log}` with the log's bytes as the history keeps them, from a Chrome
/// autoplay with no sound output); each chart is played here at 60 frames
/// a second and every logged press and release must be the same, lane,
/// edge and judged time.
#[test]
#[ignore = "needs DDI_WEB_PLAYS"]
fn the_same_judgements_as_the_web() {
    common::indexes();
    let Some(path) = std::env::var_os("DDI_WEB_PLAYS") else {
        return;
    };
    let text = std::fs::read_to_string(path).unwrap();
    let mut compared = 0;
    for line in text.lines().filter(|l| !l.trim().is_empty()) {
        let v: serde_json::Value = serde_json::from_str(line).unwrap();
        let song = v["song"].as_str().unwrap().to_string();
        let chart = v["chart"].as_u64().unwrap() as usize;
        let bytes: Vec<u8> = v["log"]
            .as_array()
            .unwrap()
            .iter()
            .map(|b| b.as_u64().unwrap() as u8)
            .collect();
        let web = PlayLog::decode(&bytes).unwrap();
        let loaded = Rc::new(load::load(&song, 48_000).unwrap());
        let (r, ours) = play(&loaded, &song, chart, 60.0, false);
        assert_eq!(ours.start, web.start, "{song} {chart}");
        assert_eq!(ours.inputs.len(), web.inputs.len(), "{song} {chart}");
        let mut worst = 0.0f64;
        for (a, b) in ours.inputs.iter().zip(&web.inputs) {
            assert_eq!((a.lane, a.pressed), (b.lane, b.pressed), "{song} {chart}");
            worst = worst.max((a.time - b.time).abs());
        }
        // Each log judged again under the web's record (its ruleset and
        // options): every note the same judgement, in the same order.
        let record: ddi_ui::state::history::PlayRecord =
            serde_json::from_str(v["record"].as_str().unwrap()).unwrap();
        let judge = |log: &PlayLog| {
            ddi_ui::state::history::replay(
                &loaded.song,
                chart,
                &record.ruleset,
                &record.options,
                log,
            )
        };
        let (a, b) = (judge(&ours), judge(&web));
        assert_eq!(a.tally.taps, b.tally.taps, "{song} {chart}");
        assert_eq!(a.tally.held, b.tally.held, "{song} {chart}");
        assert_eq!(
            (a.score.money, a.score.ex),
            (b.score.money, b.score.ex),
            "{song} {chart}"
        );
        // Events of one instant may come in either order: compared by
        // lane and what was judged, in time order, each at the same time
        // within a millisecond.
        let events = |r: &ddi_engine::player::Results| {
            let mut v: Vec<(u8, String, f64)> = r
                .timeline
                .iter()
                .map(|j| {
                    let kind = match j.kind {
                        ddi_engine::judge::JudgeEventKind::Tap(t, _) => format!("tap {t:?}"),
                        other => format!("{other:?}"),
                    };
                    (j.lane, kind, f64::from(j.time))
                })
                .collect();
            v.sort_by(|x, y| (x.0, &x.1).cmp(&(y.0, &y.1)).then(x.2.total_cmp(&y.2)));
            v
        };
        let (ea, eb) = (events(&a), events(&b));
        assert_eq!(ea.len(), eb.len(), "{song} {chart}");
        for (x, y) in ea.iter().zip(&eb) {
            assert!(
                x.0 == y.0 && x.1 == y.1 && (x.2 - y.2).abs() <= 1e-3,
                "{song} {chart}: {x:?} against the web's {y:?}"
            );
        }
        assert_eq!(r.tally.taps, b.tally.taps, "{song} {chart}");
        println!(
            "{song} chart {chart}: {} inputs, the same edges and {} judgements; judged times \
             apart by {:.0} µs at most (the web's audio clock); taps {:?}",
            ours.inputs.len(),
            a.timeline.len(),
            worst * 1e6,
            r.tally.taps
        );
        // Both judge on the timeline: what differs is each clock's reading
        // of when an edge came (the web's estimates its audio clock).
        assert!(worst <= 1e-3, "{song} {chart}: {worst}");
        compared += 1;
    }
    assert!(compared > 0);
}

/// A play on the machine's sound device, in real time, autoplayed:
/// `DDI_AUDIO_DEVICE_TESTS=1` (a test run does not open the device
/// otherwise), at master volume 0.005 (−46 dBFS): all top tier, kept,
/// and the clock's latency and jitter over the play printed.
#[test]
#[ignore = "opens the sound device: DDI_AUDIO_DEVICE_TESTS=1"]
fn a_play_on_the_sound_device() {
    common::indexes();
    if std::env::var_os("DDI_AUDIO_DEVICE_TESTS").is_none() {
        return;
    }
    let song = std::env::var("DDI_PLAY_SONG").unwrap_or_else(|_| "beatucada".into());
    let chart: usize = std::env::var("DDI_PLAY_CHART")
        .ok()
        .and_then(|c| c.parse().ok())
        .unwrap_or(1);
    let audio = Audio::new(0.005);
    let rate = audio.sample_rate();
    let probe = audio.probe().expect("a stream");
    let loaded = Rc::new(load::load(&song, rate).unwrap());
    let store = Rc::new(MemoryPlayStore::default());
    let plays = Plays::new(store.clone());
    // The device starts: its first callbacks before the play.
    let start = std::time::Instant::now();
    while !audio.running() && start.elapsed().as_secs() < 5 {
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
    assert!(audio.running(), "{}", audio.status_line());
    let stop = Arc::new(std::sync::atomic::AtomicBool::new(false));
    let flag = stop.clone();
    let sampler = std::thread::spawn(move || {
        let mut samples = Vec::new();
        while !flag.load(Ordering::Relaxed) {
            if let Some(s) = probe.now() {
                samples.push(s);
            }
            std::thread::sleep(std::time::Duration::from_millis(1));
        }
        samples
    });
    let mut p = Play::start(
        Request::song(&song, chart, true),
        loaded,
        &Settings::default(),
        audio,
        Rc::new(RefCell::new(Feed::default())),
        None,
        60.0,
    )
    .unwrap();
    let results = loop {
        std::thread::sleep(std::time::Duration::from_micros(16_667));
        match p.tick(ddi_desktop::clock::now(), Some(&plays)) {
            Some(Ended::Finished { results, .. }) => break *results,
            Some(_) => panic!(
                "the play did not finish: {}",
                p.session.audio().status_line()
            ),
            None => {}
        }
    };
    stop.store(true, Ordering::Relaxed);
    let samples = sampler.join().unwrap();
    let skip = samples.len().min(500);
    let s = ddi_desktop::audio::ClockStats::of(&samples[skip..]);
    println!("{}", p.session.audio().status_line());
    println!(
        "{song} chart {chart}: taps {:?} · late edges {} · xruns {} · clock: {} samples · jitter sd {:.1} µs, max {:.1} µs · drift {:+.1} ppm · latency {:.2} ms ({:.2}–{:.2})",
        results.tally.taps,
        results.late_edges,
        p.xruns(),
        s.samples,
        s.jitter_sd * 1e6,
        s.jitter_max * 1e6,
        s.drift_ppm,
        s.latency_mean * 1e3,
        s.latency_min * 1e3,
        s.latency_max * 1e3,
    );
    assert_eq!(
        results.tally.taps[1..].iter().sum::<u32>(),
        0,
        "{:?}",
        results.tally.taps
    );
    assert_eq!(store.len(), 1);
}
