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
    fn stop(&mut self, _id: SoundId) {}
    fn set_volume(&mut self, _id: SoundId, _volume: f32) {}
    fn duration(&self, b: &f64) -> f64 {
        *b
    }
    fn sample_rate(&self) -> f64 {
        48_000.0
    }
    fn set_master_volume(&mut self, _volume: f32) {}
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
        .join("../assets/songs")
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
