//! The session on a fake audio backend whose clock the test drives and a
//! fake shell whose edges it scripts (`docs/plans/desktop.md`, section 6):
//! autoplay of a bundled song all top tier at several frame rates and
//! through a test pad, calibration converging on a simulated late player,
//! the quit gesture, an interruption, the facts a play keeps.

use std::cell::RefCell;
use std::rc::Rc;

use ddi_engine::ConnectedPad;
use ddi_platform::{ClockSample, DeviceFingerprint, HostTime, RawInput, SoundId};

use super::*;

/// The host clock all fakes read, seconds.
type Clock = Rc<RefCell<f64>>;

/// An audio backend whose context time is the host's plus a fixed epoch;
/// what was asked of it is kept.
struct FakeAudio {
    clock: Clock,
    latency: f64,
    running: bool,
    played: Vec<(f64, f64, f32)>,
    ramps: Vec<(Option<f32>, f32, Option<f64>, f64)>,
    stopped: u32,
}

const EPOCH: f64 = 7.5;

impl FakeAudio {
    fn new(clock: &Clock, latency: f64) -> FakeAudio {
        FakeAudio {
            clock: clock.clone(),
            latency,
            running: true,
            played: Vec::new(),
            ramps: Vec::new(),
            stopped: 0,
        }
    }
}

impl AudioBackend for FakeAudio {
    /// A buffer is its length in seconds.
    type Buffer = f64;
    type Error = String;

    fn now(&self) -> ClockSample {
        let host = *self.clock.borrow();
        ClockSample {
            context_time: host + EPOCH + self.latency,
            host_time: HostTime(host),
            output_latency: self.latency,
        }
    }
    fn play(&mut self, _b: &f64, at: f64, offset: f64, volume: f32) -> Result<SoundId, String> {
        self.played.push((at, offset, volume));
        Ok(1)
    }
    fn duration(&self, b: &f64) -> f64 {
        *b
    }
    fn sample_rate(&self) -> f64 {
        48_000.0
    }
    fn master_ramp(&mut self, from: Option<f32>, to: f32, at: Option<f64>, seconds: f64) {
        self.ramps.push((from, to, at, seconds));
    }
    fn stop_all(&mut self) {
        self.stopped += 1;
    }
    fn running(&self) -> bool {
        self.running
    }
    fn buffer_from_pcm(&self, c: Vec<Vec<f32>>, rate: f64) -> Result<f64, String> {
        Ok(c.first().map_or(0, Vec::len) as f64 / rate)
    }
    fn fingerprint(&self) -> Option<DeviceFingerprint> {
        None
    }
}

/// A shell: edges the test queues (handed over once their time has come),
/// one standard pad, a test pad that presses at the scheduled times.
#[derive(Default)]
struct FakeInputs {
    clock: Clock,
    queued: Vec<RawInput>,
    pads: Vec<ConnectedPad>,
    begun: Option<(Vec<String>, usize)>,
    fast: Option<bool>,
    released: u32,
    interrupted: bool,
}

impl FakeInputs {
    fn new(clock: &Clock) -> FakeInputs {
        FakeInputs {
            clock: clock.clone(),
            ..FakeInputs::default()
        }
    }

    fn key(&mut self, at: f64, code: &str, pressed: bool) {
        self.queued.push(RawInput {
            device: DeviceId::Keyboard,
            control: code.into(),
            pressed,
            host_time: HostTime(at),
            slot: 0,
        });
    }
}

impl Inputs for FakeInputs {
    fn begin(&mut self, capture: Vec<String>, layout: &Layout, _reverse: bool) {
        self.begun = Some((capture, layout.lanes.len()));
    }
    fn poll(&mut self, out: &mut Vec<RawInput>) {
        let now = *self.clock.borrow();
        let (due, later): (Vec<_>, Vec<_>) =
            self.queued.drain(..).partition(|r| r.host_time.0 <= now);
        self.queued = later;
        out.extend(due);
    }
    fn connected_pads(&self) -> Vec<ConnectedPad> {
        self.pads.clone()
    }
    fn pads_running(&mut self, fast: Option<bool>) {
        self.fast = fast;
    }
    fn release(&mut self) {
        self.released += 1;
    }
    fn interrupted(&self) -> bool {
        self.interrupted
    }
    fn device_changed(&self) -> bool {
        false
    }
    fn press_test_pad(&mut self, host: HostTime, control: &str, pressed: bool) {
        self.queued.push(RawInput {
            device: DeviceId::Gamepad(self.pads[0].id.clone()),
            control: control.into(),
            pressed,
            host_time: host,
            slot: 0,
        });
    }
}

fn bundled(id: &str) -> Song {
    let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../assets/songs")
        .join(id);
    let ssc = std::fs::read_dir(&dir)
        .unwrap()
        .flatten()
        .map(|e| e.path())
        .find(|p| p.extension().is_some_and(|e| e == "ssc"))
        .unwrap();
    ddi_chart::formats::sm::parse_simfile(&std::fs::read_to_string(ssc).unwrap(), "ssc").unwrap()
}

/// Runs `session` at `fps` until it ends (or `limit` seconds).
fn run<A: AudioBackend, I: Inputs>(
    session: &mut PlaySession<A, I>,
    clock: &Clock,
    fps: f64,
    limit: f64,
) -> Option<SessionEvent> {
    let start = *clock.borrow();
    loop {
        *clock.borrow_mut() += 1.0 / fps;
        let now = *clock.borrow();
        let (_, event) = session.tick(HostTime(now));
        if event.is_some() {
            return event;
        }
        if now - start > limit {
            return None;
        }
    }
}

#[test]
fn autoplay_is_all_top_tier_at_any_frame_rate() {
    let song = bundled("beatucada");
    let settings = Settings::default();
    for fps in [60.0, 144.0, 37.0] {
        let clock: Clock = Rc::new(RefCell::new(100.0));
        let mut config = SessionConfig::from_settings(&settings, true, None);
        config.seed = 7;
        let mut s = PlaySession::start(
            &song,
            1,
            &settings,
            config,
            FakeAudio::new(&clock, 0.012),
            110.0,
            FakeInputs::new(&clock),
        )
        .unwrap();
        // The music starts START_LOOKAHEAD after now, on the audio clock.
        let (at, offset, volume) = s.audio().played[0];
        assert!((at - (100.0 + EPOCH + 0.012 + START_LOOKAHEAD)).abs() < 1e-9);
        assert_eq!((offset, volume), (0.0, settings.volume));
        assert_eq!(s.inputs().fast, Some(settings.fast_pad_poll));
        // The bound keys and the page's scrolling keys are captured.
        let (capture, lanes) = s.inputs().begun.clone().unwrap();
        assert!(
            capture.iter().any(|k| k == "ArrowLeft") && capture.iter().any(|k| k == "Backspace")
        );
        assert_eq!(lanes, 4);
        let Some(SessionEvent::Finished { results, facts, .. }) = run(&mut s, &clock, fps, 200.0)
        else {
            panic!("no end at {fps} fps");
        };
        let t = s.player.tally();
        assert!(s.player.skin_events_since(0).all(|e| e.autoplay));
        assert!(t.taps[0] > 0, "{fps}: {t:?}");
        assert!(t.taps[1..].iter().all(|&n| n == 0), "{fps}: {t:?}");
        assert_eq!(t.let_go, 0);
        assert!(!results.failed);
        assert_eq!(facts.options.seed, 7);
        assert_eq!(facts.output_latency, 0.012);
        assert_eq!(facts.sample_rate, 48_000.0);
        // Over: the keys let go, the pads' fast loop off.
        assert_eq!(s.inputs().fast, None);
        assert!(s.inputs().released >= 1);
    }
}

#[test]
fn autoplay_through_a_test_pad_is_all_top_tier() {
    let song = bundled("beatucada");
    let settings = Settings::default();
    let clock: Clock = Rc::new(RefCell::new(5.0));
    let mut inputs = FakeInputs::new(&clock);
    inputs.pads.push(ConnectedPad {
        id: "Test pad (STANDARD GAMEPAD)".into(),
        index: 0,
        standard: true,
    });
    let mut config = SessionConfig::from_settings(&settings, true, None);
    config.auto_pad = true;
    let mut s = PlaySession::start(
        &song,
        1,
        &settings,
        config,
        FakeAudio::new(&clock, 0.02),
        110.0,
        inputs,
    )
    .unwrap();
    let Some(SessionEvent::Finished { facts, .. }) = run(&mut s, &clock, 60.0, 200.0) else {
        panic!("no end");
    };
    let t = s.player.tally();
    assert!(s.player.skin_events_since(0).all(|e| e.autoplay));
    assert!(
        t.taps[0] > 0 && t.taps[1..].iter().all(|&n| n == 0),
        "{t:?}"
    );
    // The presses came from the pad.
    assert!(
        facts
            .inputs
            .iter()
            .all(|d| matches!(d, InputDevice::Pad { .. }))
    );
}

#[test]
fn calibration_converges_on_a_late_player() {
    let settings = Settings::default();
    for (mode, bias) in [(CalMode::Visual, 0.03), (CalMode::Audio, -0.02)] {
        let song = calibration::song(mode);
        let clock: Clock = Rc::new(RefCell::new(1.0));
        let mut config = SessionConfig::from_settings(&settings, true, None);
        config.ruleset = ddi_engine::rules::presets::calibration();
        config.calibration = Some(mode);
        config.auto_bias = bias;
        config.hide_notes = mode.hide_notes();
        let audio = FakeAudio::new(&clock, 0.01);
        let length =
            audio.buffer_from_pcm(vec![calibration::click_samples(48_000.0, &song)], 48_000.0);
        let mut s = PlaySession::start(
            &song,
            0,
            &settings,
            config,
            audio,
            length.unwrap(),
            FakeInputs::new(&clock),
        )
        .unwrap();
        assert_eq!(s.view.hide_notes, mode.hide_notes());
        let Some(SessionEvent::Finished {
            calibration: Some(o),
            ..
        }) = run(&mut s, &clock, 60.0, 120.0)
        else {
            panic!("{mode:?}: no outcome");
        };
        assert!(o.converged, "{mode:?}: {o:?}");
        assert!(
            (o.total() - bias).abs() < 0.004,
            "{mode:?}: {} for {bias}",
            o.total()
        );
        assert!(s.view.hide_notes);
    }
}

#[test]
fn escape_twice_quits_and_an_interruption_ends_the_play() {
    let song = bundled("beatucada");
    let settings = Settings::default();
    let clock: Clock = Rc::new(RefCell::new(0.0));
    let mut inputs = FakeInputs::new(&clock);
    inputs.key(3.0, "Escape", true);
    inputs.key(3.05, "Escape", false);
    let mut s = PlaySession::start(
        &song,
        1,
        &settings,
        SessionConfig::from_settings(&settings, false, None),
        FakeAudio::new(&clock, 0.0),
        110.0,
        inputs,
    )
    .unwrap();
    // One tap shows the hint and quits nothing.
    assert!(run(&mut s, &clock, 60.0, 3.5).is_none());
    // The hint shows for a while after the tap.
    assert!(s.view.cancel_progress > 0.0);
    assert!(run(&mut s, &clock, 60.0, 1.5).is_none());
    // A second within half a second quits.
    let now = *clock.borrow();
    s.inputs.key(now + 0.01, "Escape", true);
    s.inputs.key(now + 0.2, "Escape", true);
    assert!(matches!(
        run(&mut s, &clock, 60.0, 2.0),
        Some(SessionEvent::Aborted)
    ));
    assert!(s.audio().stopped >= 1);

    let clock: Clock = Rc::new(RefCell::new(0.0));
    let mut s = PlaySession::start(
        &song,
        1,
        &settings,
        SessionConfig::from_settings(&settings, false, None),
        FakeAudio::new(&clock, 0.0),
        110.0,
        FakeInputs::new(&clock),
    )
    .unwrap();
    assert!(run(&mut s, &clock, 60.0, 1.0).is_none());
    s.inputs.interrupted = true;
    assert!(matches!(
        run(&mut s, &clock, 60.0, 1.0),
        Some(SessionEvent::Interrupted)
    ));
}

#[test]
fn a_held_escape_quits() {
    let song = bundled("beatucada");
    let settings = Settings::default();
    let clock: Clock = Rc::new(RefCell::new(0.0));
    let mut inputs = FakeInputs::new(&clock);
    inputs.key(2.0, "Escape", true);
    let mut s = PlaySession::start(
        &song,
        1,
        &settings,
        SessionConfig::from_settings(&settings, false, None),
        FakeAudio::new(&clock, 0.0),
        110.0,
        inputs,
    )
    .unwrap();
    let event = run(&mut s, &clock, 60.0, 10.0);
    assert!(matches!(event, Some(SessionEvent::Aborted)));
    // Held for CANCEL_HOLD: about 2.7 s.
    assert!((*clock.borrow() - (2.0 + CANCEL_HOLD)).abs() < 0.05);
}

/// Every bundled song with its charts.
fn bundled_songs() -> Vec<(String, Song)> {
    let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../assets/songs");
    let mut ids: Vec<String> = std::fs::read_dir(&dir)
        .unwrap()
        .flatten()
        .filter(|e| e.path().is_dir())
        .map(|e| e.file_name().to_string_lossy().into_owned())
        .collect();
    ids.sort();
    ids.into_iter()
        .map(|id| (id.clone(), bundled(&id)))
        .collect()
}

/// The play kept as the shells keep it, read back for replaying.
fn kept(
    id: &str,
    song: &Song,
    chart: usize,
    results: &Results,
    facts: &PlayFacts,
    auto: bool,
) -> ddi_ui::state::history::Replay {
    use ddi_ui::state::history::{Autoplay, Replay, new_entry, play_id};
    let settings = Settings::default();
    let played = crate::record::Played {
        song_id: id,
        song,
        chart,
        settings: &settings,
        stored_settings: None,
        devices: None,
        refresh_hz: 60.0,
        graphics: "test".into(),
        song_offset: 0.0,
        autoplay: auto.then_some(Autoplay::default()),
        time: 1_760_000_000_000.0,
        app_version: "test",
    };
    let record = crate::record::play_record(&played, results, facts).unwrap();
    let (entry, _) = new_entry(play_id(record.time, 1), &record, &facts.log, results).unwrap();
    Replay::from_entry(&entry).unwrap()
}

/// Replays `replay` on `song` at `fps` with `latency` (another clock, as
/// another device), `jitter` seconds of uneven frames; its end.
fn watch(
    replay: &ddi_ui::state::history::Replay,
    song: &Song,
    fps: f64,
    latency: f64,
    jitter: f64,
    inputs: Option<FakeInputs>,
    clock: &Clock,
) -> (PlaySession<FakeAudio, FakeInputs>, Option<SessionEvent>) {
    let settings = replay.settings(&Settings::default());
    let mut config = SessionConfig::from_settings(&settings, true, None);
    config.replay = Some(replay.replaying(None, config.options.clock));
    let chart = replay.chart(song).unwrap();
    let played = replay.played_song(song);
    let mut s = PlaySession::start(
        &played,
        chart,
        &settings,
        config,
        FakeAudio::new(clock, latency),
        played.charts[chart].notes.last().map_or(30.0, |_| 600.0),
        inputs.unwrap_or_else(|| FakeInputs::new(clock)),
    )
    .unwrap();
    assert!(s.replaying());
    let mut k = 0u64;
    let start = *clock.borrow();
    let event = loop {
        // Uneven frames: a fixed pattern of longer and shorter ones.
        k += 1;
        let wobble = jitter * (((k * 7919) % 13) as f64 / 6.0 - 1.0);
        *clock.borrow_mut() += (1.0 / fps + wobble).max(0.0005);
        let now = *clock.borrow();
        let (_, event) = s.tick(HostTime(now));
        assert!(s.take_fail_report().is_none(), "a replay reported a fail");
        if event.is_some() {
            break event;
        }
        if now - start > 700.0 {
            break None;
        }
    };
    (s, event)
}

fn without_late(r: &Results) -> Results {
    Results {
        late_edges: 0,
        ..r.clone()
    }
}

#[test]
fn replays_of_every_bundled_autoplay_judge_as_the_plays_did() {
    let mut charts = 0;
    for (id, song) in bundled_songs() {
        for chart in 0..song.charts.len() {
            if song.layout_of(&song.charts[chart]).is_none() {
                continue;
            }
            let settings = Settings::default();
            let clock: Clock = Rc::new(RefCell::new(100.0));
            let mut config = SessionConfig::from_settings(&settings, true, None);
            config.seed = 11;
            let mut s = PlaySession::start(
                &song,
                chart,
                &settings,
                config,
                FakeAudio::new(&clock, 0.012),
                600.0,
                FakeInputs::new(&clock),
            )
            .unwrap();
            let Some(SessionEvent::Finished { results, facts, .. }) =
                run(&mut s, &clock, 60.0, 700.0)
            else {
                panic!("{id} {chart}: no end");
            };
            let replay = kept(&id, &song, chart, &results, &facts, true);
            let offline = replay.results(&song).unwrap();
            for (fps, latency, jitter) in [(144.0, 0.03, 0.0), (37.0, 0.005, 0.01)] {
                let clock: Clock = Rc::new(RefCell::new(3.0));
                let (_, event) = watch(&replay, &song, fps, latency, jitter, None, &clock);
                let Some(SessionEvent::Finished {
                    results: watched, ..
                }) = event
                else {
                    panic!("{id} {chart} at {fps}: no end");
                };
                assert_eq!(
                    without_late(&watched),
                    without_late(&results),
                    "{id} {chart} {fps}"
                );
                let check = replay.check(&watched, &offline);
                assert!(
                    check.matches && check.with_record,
                    "{id} {chart}: {check:?}"
                );
            }
            charts += 1;
        }
    }
    assert!(charts >= 8, "{charts} charts");
}

#[test]
fn a_played_replay_with_misses_judges_as_the_play_did() {
    let song = bundled("beatucada");
    let chart = 1;
    let settings = Settings::default();
    let clock: Clock = Rc::new(RefCell::new(50.0));
    let config = SessionConfig::from_settings(&settings, false, None);
    let mut inputs = FakeInputs::new(&clock);
    let latency = 0.02;
    // The presses a player makes: the notes' times on the host clock,
    // off by up to ±70 ms, every seventh note let go, every eleventh a
    // stray press too, holds let go early now and then.
    let player = ddi_engine::player::Player::new(
        &song,
        chart,
        settings.ruleset_for(&song.charts[chart]),
        PlayOptions::default(),
    );
    let script = ddi_engine::autoplay::script(player.judge().notes(), 0.0);
    let layout = song.layout_of(&song.charts[chart]).unwrap();
    // Song second 0 is heard START_LOOKAHEAD after the start.
    let host_of = |t: f64| 50.0 + START_LOOKAHEAD + t;
    let mut seed = 0x2545_f491_4f6c_dd1du64;
    let mut rand = move || {
        seed ^= seed << 13;
        seed ^= seed >> 7;
        seed ^= seed << 17;
        (seed % 10_000) as f64 / 10_000.0
    };
    for (n, p) in script.iter().enumerate() {
        if n % 7 == 3 {
            continue;
        }
        let key = layout.lanes[p.lane as usize].default_keys[0].clone();
        let off = (rand() - 0.5) * 0.14;
        let mut release = p.release + off;
        if n % 5 == 0 && p.release - p.press > 0.3 {
            release = p.press + (p.release - p.press) * 0.5;
        }
        inputs.key(host_of(p.press + off), &key, true);
        inputs.key(host_of(release.max(p.press + off + 0.03)), &key, false);
        if n % 11 == 0 {
            let other =
                layout.lanes[(p.lane as usize + 2) % layout.lanes.len()].default_keys[0].clone();
            inputs.key(host_of(p.press + 0.21), &other, true);
            inputs.key(host_of(p.press + 0.25), &other, false);
        }
    }
    inputs
        .queued
        .sort_by(|a, b| a.host_time.0.total_cmp(&b.host_time.0));
    let mut s = PlaySession::start(
        &song,
        chart,
        &settings,
        config,
        FakeAudio::new(&clock, latency),
        110.0,
        inputs,
    )
    .unwrap();
    let Some(SessionEvent::Finished { results, facts, .. }) = run(&mut s, &clock, 60.0, 200.0)
    else {
        panic!("no end");
    };
    let t = &results.tally;
    assert!(t.taps[5] > 0, "no misses: {t:?}");
    assert!(
        t.taps[1..5].iter().any(|&n| n > 0),
        "no lesser tiers: {t:?}"
    );
    let replay = kept("beatucada", &song, chart, &results, &facts, false);
    let offline = replay.results(&song).unwrap();
    assert_eq!(without_late(&offline), without_late(&results));
    // Watched while keys are pressed: they reach no lane.
    let clock: Clock = Rc::new(RefCell::new(9.0));
    let mut noise = FakeInputs::new(&clock);
    for k in 0..40 {
        let key = layout.lanes[k % 4].default_keys[0].clone();
        noise.key(9.0 + 2.0 + k as f64 * 0.37, &key, true);
        noise.key(9.0 + 2.1 + k as f64 * 0.37, &key, false);
    }
    let (_, event) = watch(&replay, &song, 75.0, 0.004, 0.006, Some(noise), &clock);
    let Some(SessionEvent::Finished {
        results: watched, ..
    }) = event
    else {
        panic!("no end");
    };
    assert_eq!(without_late(&watched), without_late(&results));
    let check = replay.check(&watched, &offline);
    assert!(check.matches, "{check:?}");
}

#[test]
fn a_failed_play_replays_to_its_fail_and_a_replay_can_be_quit() {
    let song = bundled("beatucada");
    let chart = 1;
    let settings = Settings::default();
    let clock: Clock = Rc::new(RefCell::new(1.0));
    // No presses: the life runs out.
    let mut s = PlaySession::start(
        &song,
        chart,
        &settings,
        SessionConfig::from_settings(&settings, false, None),
        FakeAudio::new(&clock, 0.0),
        110.0,
        FakeInputs::new(&clock),
    )
    .unwrap();
    let mut failed = None;
    let mut end = None;
    for _ in 0..(200 * 60) {
        *clock.borrow_mut() += 1.0 / 60.0;
        let (_, event) = s.tick(HostTime(*clock.borrow()));
        if let Some(SessionEvent::Failed { results, facts }) = s.take_fail_report() {
            failed = Some((results, facts));
        }
        if event.is_some() {
            end = event;
            break;
        }
    }
    let (results, facts) = failed.expect("the play failed");
    assert!(results.failed);
    assert!(matches!(end, Some(SessionEvent::Finished { .. })));
    let replay = kept("beatucada", &song, chart, &results, &facts, false);
    let offline = replay.results(&song).unwrap();
    let clock: Clock = Rc::new(RefCell::new(2.0));
    let (_, event) = watch(&replay, &song, 60.0, 0.01, 0.0, None, &clock);
    let Some(SessionEvent::Finished {
        results: watched, ..
    }) = event
    else {
        panic!("no end");
    };
    assert!(watched.failed);
    assert!(replay.check(&watched, &offline).matches);

    // Escape twice mid-replay leaves it.
    let clock: Clock = Rc::new(RefCell::new(2.0));
    let mut quit = FakeInputs::new(&clock);
    quit.key(6.0, "Escape", true);
    quit.key(6.1, "Escape", true);
    let (s, event) = watch(&replay, &song, 60.0, 0.01, 0.0, Some(quit), &clock);
    assert!(matches!(event, Some(SessionEvent::Aborted)));
    assert!(s.replay_progress().is_some_and(|(fed, _)| fed == 0));
}

/// Under a delayed fail, or with fail off, a play the life runs out of
/// plays on to the end: no fade, no early report; the delayed one ends
/// failed, kept by its rules, and its replay judges the same.
#[test]
fn a_delayed_fail_or_fail_off_plays_to_the_end() {
    use ddi_engine::rules::{FailType, StepManiaOptions};
    let song = bundled("beatucada");
    let chart = 1;
    for fail in [FailType::ImmediateContinue, FailType::Off] {
        let settings = Settings {
            stepmania: StepManiaOptions {
                fail,
                ..StepManiaOptions::default()
            },
            ..Settings::default()
        };
        let clock: Clock = Rc::new(RefCell::new(1.0));
        let mut s = PlaySession::start(
            &song,
            chart,
            &settings,
            SessionConfig::from_settings(&settings, false, None),
            FakeAudio::new(&clock, 0.0),
            110.0,
            FakeInputs::new(&clock),
        )
        .unwrap();
        let mut end = None;
        for _ in 0..(200 * 60) {
            *clock.borrow_mut() += 1.0 / 60.0;
            let (_, event) = s.tick(HostTime(*clock.borrow()));
            if event.is_some() {
                end = event;
                break;
            }
            // Not over before the last note: no fade, no fail reported.
            if !s.player.finished() {
                assert!(s.audio().ramps.is_empty(), "{fail:?}");
                assert!(s.take_fail_report().is_none(), "{fail:?}");
            }
        }
        let Some(SessionEvent::Finished { results, facts, .. }) = end else {
            panic!("{fail:?}: no end");
        };
        assert_eq!(results.failed, fail == FailType::ImmediateContinue);
        assert!(results.gauge_failed_at.is_some());
        let misses = results.tally.count(ddi_engine::rules::Judgement::Miss);
        assert_eq!(misses, results.tally.total_taps(), "{fail:?}");
        assert!(misses > 30, "{fail:?}: {misses}");
        assert_eq!(facts.ruleset.stepmania.fail, fail);
        let replay = kept("beatucada", &song, chart, &results, &facts, false);
        assert_eq!(
            without_late(&replay.results(&song).unwrap()),
            without_late(&results)
        );
    }
}

#[test]
fn the_ghost_says_how_far_ahead_of_the_best_a_play_is() {
    use ddi_ui::state::history::{Ghost, GhostDelta};
    let song = bundled("beatucada");
    let chart = 1;
    let settings = Settings::default();
    // The best: an autoplay all top tier.
    let play = |bias: f64, ghost: Option<Ghost>| {
        let clock: Clock = Rc::new(RefCell::new(10.0));
        let mut config = SessionConfig::from_settings(&settings, true, None);
        config.auto_bias = bias;
        config.ghost = ghost;
        let mut s = PlaySession::start(
            &song,
            chart,
            &settings,
            config,
            FakeAudio::new(&clock, 0.01),
            110.0,
            FakeInputs::new(&clock),
        )
        .unwrap();
        let mut deltas = Vec::new();
        let end = loop {
            *clock.borrow_mut() += 1.0 / 60.0;
            let (_, event) = s.tick(HostTime(*clock.borrow()));
            if let Some(d) = s.ghost_delta() {
                deltas.push(d);
            }
            if let Some(e) = event {
                break e;
            }
        };
        let SessionEvent::Finished { results, facts, .. } = end else {
            panic!("no end");
        };
        (results, facts, deltas)
    };
    let (results, facts, none) = play(0.0, None);
    assert!(none.is_empty());
    let best = kept("beatucada", &song, chart, &results, &facts, false);
    let ghost = Ghost::of(&best, &song).unwrap();
    assert_eq!(
        ghost.last(),
        ddi_ui::state::history::score_value(&results.score)
    );
    // The same play: level all along.
    let (_, _, same) = play(0.0, Some(ghost.clone()));
    assert!(same.len() > 1000);
    assert!(
        same.iter()
            .all(|d| matches!(d, GhostDelta::Percent(p) if p.abs() < 1e-9)),
        "{:?}",
        same.iter()
            .find(|d| !matches!(d, GhostDelta::Percent(p) if p.abs() < 1e-9))
    );
    // Pressing 35 ms late: behind it, more and more, never ahead.
    let (late, _, behind) = play(0.035, Some(ghost.clone()));
    let p = |d: &GhostDelta| match d {
        GhostDelta::Percent(p) => *p,
        GhostDelta::Points(p) => *p,
    };
    assert!(behind.iter().all(|d| p(d) <= 1e-9));
    let last = p(behind.last().unwrap());
    let expected = (ddi_ui::state::history::score_value(&late.score) - ghost.last()) * 100.0;
    assert!((last - expected).abs() < 1e-9, "{last} {expected}");
    assert!(last < -1.0, "{last}");
}

/// A chart with rolls (kept and dropped), holds, mines, a stop and a
/// tempo change, none of which the bundled charts have all of.
const ROLLS: &str = "#TITLE:Rolls;#OFFSET:0;#BPMS:0=120,8=150;#STOPS:12=0.5;
#NOTES:
     dance-single:
     :
     Hard:
     8:
     0,0,0,0,0:
1000
0400
0000
0300
,
4000
0010
3000
00M0
,
0200
0000
0300
0001
,
0040
0000
0030
1000
,
0004
M000
0003
0100
,
4000
0000
0000
3000
;
";

#[test]
fn rolls_kept_and_dropped_replay_to_the_same_results() {
    let song = ddi_chart::formats::sm::parse_simfile(ROLLS, "sm").unwrap();
    let rolls = song.charts[0]
        .notes
        .iter()
        .filter(|n| matches!(n.kind, ddi_chart::NoteKind::RollHead { .. }))
        .count();
    assert_eq!(rolls, 5);
    let settings = Settings::default();
    let layout = song.layout_of(&song.charts[0]).unwrap();
    let player = ddi_engine::player::Player::new(
        &song,
        0,
        settings.ruleset_for(&song.charts[0]),
        PlayOptions::default(),
    );
    let script = ddi_engine::autoplay::script(player.judge().notes(), 0.0);
    for manual in [false, true] {
        let clock: Clock = Rc::new(RefCell::new(20.0));
        let mut inputs = FakeInputs::new(&clock);
        if manual {
            // Every press held from its first tap to its tap's release
            // (rolls are never tapped again: they drop), a little late,
            // a mine stepped on.
            let host_of = |t: f64| 20.0 + START_LOOKAHEAD + t;
            let mut firsts: Vec<(u8, f64)> = Vec::new();
            for p in &script {
                if firsts
                    .iter()
                    .any(|&(l, t)| l == p.lane && p.press - t < 0.3)
                {
                    continue;
                }
                firsts.push((p.lane, p.press));
                let key = layout.lanes[p.lane as usize].default_keys[0].clone();
                inputs.key(host_of(p.press + 0.012), &key, true);
                inputs.key(host_of(p.release + 0.012), &key, false);
            }
            let key = layout.lanes[0].default_keys[0].clone();
            inputs.key(host_of(10.0), &key, true);
            inputs.key(host_of(10.4), &key, false);
            inputs
                .queued
                .sort_by(|a, b| a.host_time.0.total_cmp(&b.host_time.0));
        }
        let mut s = PlaySession::start(
            &song,
            0,
            &settings,
            SessionConfig::from_settings(&settings, !manual, None),
            FakeAudio::new(&clock, 0.015),
            40.0,
            inputs,
        )
        .unwrap();
        let Some(SessionEvent::Finished { results, facts, .. }) = run(&mut s, &clock, 60.0, 60.0)
        else {
            panic!("no end");
        };
        let t = &results.tally;
        assert!(t.held > 0, "{manual}: {t:?}");
        if manual {
            assert!(t.let_go > 0, "rolls dropped: {t:?}");
        } else {
            assert_eq!(t.let_go, 0, "{t:?}");
        }
        let replay = kept("rolls", &song, 0, &results, &facts, !manual);
        let offline = replay.results(&song).unwrap();
        for (fps, jitter) in [(144.0, 0.0), (29.0, 0.012)] {
            let clock: Clock = Rc::new(RefCell::new(4.0));
            let (_, event) = watch(&replay, &song, fps, 0.04, jitter, None, &clock);
            let Some(SessionEvent::Finished {
                results: watched, ..
            }) = event
            else {
                panic!("no end");
            };
            assert_eq!(
                without_late(&watched),
                without_late(&results),
                "{manual} {fps}"
            );
            assert!(replay.check(&watched, &offline).matches);
        }
    }
}
