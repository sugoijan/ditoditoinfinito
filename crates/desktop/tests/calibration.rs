//! Calibration's tests on desktop (`docs/plans/desktop.md`, 7.6), with no
//! device on a fake clock: the generated chart and click track played
//! through the shared session as a song is, autoplayed following the
//! physical cue (the heard click, the drawn arrow), so the measured
//! correction is what the offsets are off by: saved, it brings them to
//! the truth (a headless output has no latency), and a late autoplayer
//! reads late.

use std::cell::RefCell;
use std::rc::Rc;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

use ddi_desktop::audio::Audio;
use ddi_desktop::play::{Ended, Feed, Play, Request, load};
use ddi_platform::HostTime;
use ddi_session::calibration::{CalMode, Outcome, report};
use ddi_ui::state::settings::Settings;

/// Test `mode` played to its end on `settings`, the autoplayer `bias`
/// seconds late.
fn run(mode: CalMode, settings: &Settings, bias: f64) -> Outcome {
    let clock = Arc::new(AtomicU64::new(1_000_000_000));
    let c = clock.clone();
    let audio = Audio::headless(1.0, Some(Arc::new(move || c.load(Ordering::SeqCst))));
    let loaded = Rc::new(load::calibration(mode, 48_000));
    let mut request = Request::calibration(mode, true);
    request.bias = bias;
    let mut p = Play::start(
        request,
        loaded,
        settings,
        audio,
        Rc::new(RefCell::new(Feed::default())),
        None,
        60.0,
    )
    .unwrap();
    let step = 1_000_000_000 / 60;
    loop {
        let now = clock.fetch_add(step, Ordering::SeqCst) + step;
        match p.tick(HostTime(now as f64 / 1e9), None) {
            Some(Ended::Finished {
                calibration: Some(o),
                ..
            }) => return o,
            Some(_) => panic!("{mode:?}: ended without an outcome"),
            None => {}
        }
        assert!(now < 1_000_000_000 * 300, "{mode:?}: never ends");
    }
}

#[test]
fn a_late_autoplayer_reads_late_in_every_test() {
    for mode in [CalMode::Visual, CalMode::Audio, CalMode::Combined] {
        let o = run(mode, &Settings::default(), 0.030);
        assert!(
            (o.total() - 0.030).abs() < 0.004,
            "{mode:?}: {:+.1} ms",
            o.total() * 1000.0
        );
        assert!(!o.unreliable() && !o.out_of_range, "{mode:?}");
        let r = report(&o, mode, 60.0);
        assert!(r.meaningful, "{mode:?}: {r:?}");
        assert!(r.headline.starts_with("+3"), "{mode:?}: {}", r.headline);
    }
}

#[test]
fn saving_brings_the_offsets_to_the_truth() {
    // The audio offset 40 ms off: the sound-only test (the autoplayer on
    // the heard click) measures it back; the visual offset is not its.
    let mut s = Settings {
        audio_offset: 0.040,
        visual_offset: 0.025,
        ..Settings::default()
    };
    let o = run(CalMode::Audio, &s, 0.0);
    CalMode::Audio.apply(&mut s, None, o.total());
    assert!(
        s.audio_offset.abs() < 0.004,
        "audio {:+.1} ms",
        s.audio_offset * 1000.0
    );
    assert_eq!(s.visual_offset, 0.025);
    // The arrows-only test (the autoplayer on the drawn arrow) the same
    // for the visual offset.
    let o = run(CalMode::Visual, &s, 0.0);
    CalMode::Visual.apply(&mut s, None, o.total());
    assert!(
        s.visual_offset.abs() < 0.004,
        "visual {:+.1} ms",
        s.visual_offset * 1000.0
    );
    // Both right: the check reads nothing to change.
    let o = run(CalMode::Combined, &s, 0.0);
    assert!(
        !report(&o, CalMode::Combined, 60.0).meaningful,
        "{:+.1} ms",
        o.total() * 1000.0
    );
}
