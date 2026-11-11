//! The session on a fake audio backend whose clock the test drives and a
//! fake shell whose edges it scripts (`docs/plans/desktop.md`, section 6):
//! autoplay of a bundled song all top tier at several frame rates and
//! through a test pad, calibration converging on a simulated late player,
//! the quit and restart gestures, an interruption, the facts a play keeps.

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
    /// Of each sound played: its length and the context time it was
    /// asked for at.
    sounds: Vec<(f64, f64)>,
    ramps: Vec<(Option<f32>, f32, Option<f64>, f64)>,
    stopped: u32,
    /// The voice's starts: length, context time, gain.
    voiced: Vec<(f64, f64, f32)>,
    /// `stop_music` calls.
    music_stopped: u32,
}

const EPOCH: f64 = 7.5;

impl FakeAudio {
    fn new(clock: &Clock, latency: f64) -> FakeAudio {
        FakeAudio {
            clock: clock.clone(),
            latency,
            running: true,
            played: Vec::new(),
            sounds: Vec::new(),
            ramps: Vec::new(),
            stopped: 0,
            voiced: Vec::new(),
            music_stopped: 0,
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
    fn play(&mut self, b: &f64, at: f64, offset: f64, volume: f32) -> Result<SoundId, String> {
        self.played.push((at, offset, volume));
        self.sounds.push((*b, self.now().context_time));
        Ok(1)
    }
    fn duration(&self, b: &f64) -> f64 {
        *b
    }
    fn play_voice(&mut self, b: &f64, at: f64, volume: f32) -> Result<SoundId, String> {
        self.voiced.push((*b, at, volume));
        Ok(1)
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
    fn stop_music(&mut self) {
        self.music_stopped += 1;
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
    fn begin(&mut self, capture: Vec<String>, layout: &Layout, _reverse: bool, _place: FieldPlace) {
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

/// Frames that stop for most of a second (desktop's window hidden while
/// macOS moves it to its fullscreen Space) and go on: the notes passed
/// meanwhile are judged at the next frame, autoplay's all top tier.
#[test]
fn autoplay_is_all_top_tier_across_a_gap_in_the_frames() {
    let song = bundled("beatucada");
    let settings = Settings::default();
    let clock: Clock = Rc::new(RefCell::new(100.0));
    let config = SessionConfig::from_settings(&settings, true, None);
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
    assert!(run(&mut s, &clock, 60.0, 20.0).is_none());
    let before = s.player.tally().taps[0];
    *clock.borrow_mut() += 1.9;
    let Some(SessionEvent::Finished { results, .. }) = run(&mut s, &clock, 60.0, 200.0) else {
        panic!("no end after the gap");
    };
    let t = s.player.tally();
    assert!(t.taps[0] > before, "{t:?}");
    assert!(t.taps[1..].iter().all(|&n| n == 0), "{t:?}");
    assert_eq!(t.let_go, 0);
    assert!(!results.failed);
}

#[test]
fn autoplay_through_a_test_pad_is_all_top_tier() {
    let song = bundled("beatucada");
    let settings = Settings::default();
    let clock: Clock = Rc::new(RefCell::new(5.0));
    let mut inputs = FakeInputs::new(&clock);
    inputs.pads.push(ConnectedPad {
        vendor_product: None,
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
fn a_playtest_starts_part_way_with_the_music_fading_in() {
    let song = bundled("beatucada");
    let settings = Settings::default();
    let clock: Clock = Rc::new(RefCell::new(10.0));
    let mut config = SessionConfig::from_settings(&settings, true, None);
    config.start = Some(60.0);
    let mut s = PlaySession::start(
        &song,
        1,
        &settings,
        config,
        FakeAudio::new(&clock, 0.0),
        110.0,
        FakeInputs::new(&clock),
    )
    .unwrap();
    // The music from 60 s in, faded in from silence.
    assert_eq!(s.audio().played[0].1, 60.0);
    assert_eq!(s.audio().ramps[0].0, Some(0.0));
    let Some(SessionEvent::Finished { facts, .. }) = run(&mut s, &clock, 60.0, 200.0) else {
        panic!("no end");
    };
    assert_eq!(facts.log.start, 60.0);
    // Only what comes after the start is judged, all top tier.
    let t = s.player.tally();
    let judged: u32 = t.taps.iter().sum();
    let after = song.charts[1]
        .notes
        .iter()
        .filter(|n| song.timing.seconds_at(n.tick) >= 60.0 && n.kind == ddi_chart::NoteKind::Tap)
        .count() as u32;
    assert!(judged > 0 && judged <= after + 8, "{judged} of {after}");
    assert!(t.taps[1..].iter().all(|&n| n == 0), "{t:?}");
    // Past the music's end: from the start.
    let mut config = SessionConfig::from_settings(&settings, true, None);
    config.start = Some(1e6);
    let s = PlaySession::start(
        &song,
        1,
        &settings,
        config,
        FakeAudio::new(&clock, 0.0),
        110.0,
        FakeInputs::new(&clock),
    )
    .unwrap();
    assert_eq!(s.audio().played[0].1, 0.0);
}

#[test]
fn calibration_converges_on_a_late_player() {
    // No clap may cue the player, whatever the settings.
    let settings = Settings {
        assist_clap: true,
        ..Settings::default()
    };
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
        assert_eq!(s.event_sounds(), (0, 0));
        assert_eq!(s.audio().played.len(), 1, "the click track only");
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
        Some(SessionEvent::Aborted { .. })
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
    let Some(SessionEvent::Aborted { results, facts }) = event else {
        panic!("no quit");
    };
    // Held for CANCEL_HOLD: about 2.7 s.
    assert!((*clock.borrow() - (2.0 + CANCEL_HOLD)).abs() < 0.05);
    // A quit's results: graded QUIT, not failed, judged to the quit with
    // the notes left unjudged (as after a fail: a chart quit at 2 s on
    // autoplay has no miss), the log told where.
    assert!(results.quit && !results.failed);
    assert_eq!(results.grade, "QUIT");
    assert_eq!(
        results.tally.count(ddi_engine::rules::Judgement::Miss),
        0,
        "{:?}",
        results.tally
    );
    let end = facts.log.end.expect("where it quit");
    assert!(end > 0.0 && end < 4.0, "{end}");
    // Kept: judged again from its log it quits there too, and so does
    // its replay, with the same results.
    let replay = kept("beatucada", &song, 1, &results, &facts, false);
    let offline = replay.results(&song).unwrap();
    assert!(offline.quit);
    assert_eq!(without_late(&offline), without_late(&results));
    let clock: Clock = Rc::new(RefCell::new(0.0));
    let (_, event) = watch(&replay, &song, 60.0, 0.0, 0.0, None, &clock);
    let Some(SessionEvent::Aborted {
        results: watched, ..
    }) = event
    else {
        panic!("the replay did not quit");
    };
    assert!(watched.quit && !watched.failed);
    assert_eq!(without_late(&watched), without_late(&results));
    let check = replay.check(&watched, &offline);
    assert!(check.matches, "{check:?}");
}

#[test]
fn r_twice_or_a_held_backslash_restarts_on_the_same_sound() {
    let song = bundled("beatucada");
    let settings = Settings::default();
    let clock: Clock = Rc::new(RefCell::new(0.0));
    let mut inputs = FakeInputs::new(&clock);
    inputs.key(3.0, "KeyR", true);
    inputs.key(3.05, "KeyR", false);
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
    // One tap shows the restart's hint and restarts nothing.
    assert!(run(&mut s, &clock, 60.0, 3.5).is_none());
    assert!(s.view.cancel_progress > 0.0 && s.view.restarting);
    assert_eq!(s.view.cancel_hint, "hold or double-tap R to restart");
    // Escape's hint takes its place.
    let now = *clock.borrow();
    s.inputs.key(now + 0.01, "Escape", true);
    s.inputs.key(now + 0.05, "Escape", false);
    assert!(run(&mut s, &clock, 60.0, 0.2).is_none());
    assert!(!s.view.restarting && s.view.cancel_hint.is_empty());
    // A second R within half a second restarts.
    let now = *clock.borrow() + 1.0;
    s.inputs.key(now, "KeyR", true);
    s.inputs.key(now + 0.05, "KeyR", false);
    s.inputs.key(now + 0.2, "KeyR", true);
    assert!(matches!(
        run(&mut s, &clock, 60.0, 2.0),
        Some(SessionEvent::Restart)
    ));
    // The sound handed back stopped and at full volume, with the music.
    let stopped = s.audio().stopped;
    let (audio, music) = s.into_restart();
    assert!(audio.stopped > stopped);
    assert_eq!(audio.ramps.last(), Some(&(None, 1.0, None, 0.0)));
    assert_eq!(music, 110.0);

    // Held: restarts after as long as a held Escape quits.
    let clock: Clock = Rc::new(RefCell::new(0.0));
    let mut inputs = FakeInputs::new(&clock);
    inputs.key(2.0, "Backslash", true);
    let mut s = PlaySession::start(
        &song,
        1,
        &settings,
        SessionConfig::from_settings(&settings, false, None),
        audio,
        music,
        inputs,
    )
    .unwrap();
    assert!(matches!(
        run(&mut s, &clock, 60.0, 10.0),
        Some(SessionEvent::Restart)
    ));
    assert!((*clock.borrow() - (2.0 + CANCEL_HOLD)).abs() < 0.05);
}

#[test]
fn a_lane_on_r_plays_it_and_a_replay_never_restarts() {
    let song = bundled("beatucada");
    let mut settings = Settings::default();
    // R is the left arrow's key.
    let layout = song.layout_of(&song.charts[1]).unwrap();
    let mut table = settings.controls.keyboard(&layout);
    table[0].push("KeyR".into());
    settings.controls.set_keyboard(&layout, table);
    let clock: Clock = Rc::new(RefCell::new(0.0));
    let mut inputs = FakeInputs::new(&clock);
    inputs.key(2.0, "KeyR", true);
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
    assert!(run(&mut s, &clock, 60.0, 3.0).is_none());
    assert!(s.held()[0], "R holds the left lane");
    assert_eq!(s.view.cancel_progress, 0.0);

    // A replay (of a play quit at 2.7 s): R pressed twice does nothing.
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
    let Some(SessionEvent::Aborted { results, facts }) = run(&mut s, &clock, 60.0, 10.0) else {
        panic!("no quit");
    };
    let replay = kept("beatucada", &song, 1, &results, &facts, false);
    let clock: Clock = Rc::new(RefCell::new(0.0));
    let mut inputs = FakeInputs::new(&clock);
    inputs.key(1.0, "KeyR", true);
    inputs.key(1.1, "KeyR", false);
    inputs.key(1.2, "KeyR", true);
    let (_, event) = watch(&replay, &song, 60.0, 0.0, 0.0, Some(inputs), &clock);
    assert!(matches!(event, Some(SessionEvent::Aborted { .. })));
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
    assert!(matches!(event, Some(SessionEvent::Aborted { .. })));
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
            // A StepMania preset: its options are what is tried.
            ruleset: "itg".into(),
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

/// The assist clap is scheduled ahead on the audio clock where the
/// music plays each row; a mine stepped on sounds at once; neither
/// when switched off.
#[test]
fn the_assist_clap_lands_on_each_row_and_a_mine_sounds() {
    use crate::sounds::{CLAP_LOOKAHEAD, clap, clap_rows, mine};
    let mut song = bundled("beatucada");
    let chart = 1;
    // A mine in the first lane two beats before the first note.
    let first = song.charts[chart].notes[0].tick;
    let at = first - ddi_chart::Tick::from_beats(2);
    song.charts[chart]
        .notes
        .insert(0, ddi_chart::Note::new(at, 0, ddi_chart::NoteKind::Mine));
    let mine_at = song.timing.seconds_at(at);
    let lengths = [clap(48_000.0).len(), mine(48_000.0).len()].map(|n| n as f64 / 48_000.0);
    for (clap_on, mine_on) in [(true, true), (false, false)] {
        let settings = Settings {
            // A StepMania preset: its options are what is tried.
            ruleset: "itg".into(),
            assist_clap: clap_on,
            mine_sound: mine_on,
            stepmania: ddi_engine::rules::StepManiaOptions {
                fail: ddi_engine::rules::FailType::Off,
                ..Default::default()
            },
            ..Settings::default()
        };
        let clock: Clock = Rc::new(RefCell::new(100.0));
        let latency = 0.012;
        let mut inputs = FakeInputs::new(&clock);
        // Heard at song second `t`: host `100 + latency + lookahead + t`.
        let host = |t: f64| 100.0 + latency + START_LOOKAHEAD + t;
        inputs.key(host(mine_at), "ArrowLeft", true);
        inputs.key(host(mine_at) + 0.05, "ArrowLeft", false);
        let mut s = PlaySession::start(
            &song,
            chart,
            &settings,
            SessionConfig::from_settings(&settings, false, None),
            FakeAudio::new(&clock, latency),
            110.0,
            inputs,
        )
        .unwrap();
        let notes = s.player.judge().notes().iter();
        let rows = clap_rows(notes.map(|n| (n.note.kind, n.judgeable, n.seconds)));
        let Some(SessionEvent::Finished { results, .. }) = run(&mut s, &clock, 60.0, 200.0) else {
            panic!("no end");
        };
        assert_eq!(results.tally.mine_hit, 1);
        let music = s.audio().played[0].0;
        let played: Vec<(f64, f64, f64)> = s
            .audio()
            .played
            .iter()
            .zip(&s.audio().sounds)
            .map(|(p, s)| (p.0, s.0, s.1))
            .collect();
        let claps: Vec<&(f64, f64, f64)> = played.iter().filter(|p| p.1 == lengths[0]).collect();
        let mines: Vec<&(f64, f64, f64)> = played.iter().filter(|p| p.1 == lengths[1]).collect();
        if !clap_on {
            assert!(claps.is_empty() && mines.is_empty());
            assert_eq!(s.event_sounds(), (0, 0));
            continue;
        }
        assert_eq!(claps.len(), rows.len());
        assert_eq!(s.event_sounds(), (rows.len() as u32, 1));
        for (c, row) in claps.iter().zip(&rows) {
            // Where the music plays the row, asked for ahead of it.
            assert!((c.0 - (music + row)).abs() < 1e-9, "{c:?} {row}");
            let ahead = c.0 - c.2;
            assert!((0.0..CLAP_LOOKAHEAD).contains(&ahead), "{ahead}");
        }
        // The mine sound as the step was judged.
        assert_eq!(mines.len(), 1);
        assert!((mines[0].0 - mines[0].2).abs() < 1e-9);
        assert!(
            (mines[0].0 - (music + mine_at)).abs() < 0.02,
            "{:?}",
            mines[0]
        );
    }
}

#[test]
fn the_ghost_says_how_far_ahead_of_the_best_a_play_is() {
    use ddi_ui::state::history::{Ghost, GhostDelta};
    let song = bundled("beatucada");
    let chart = 1;
    // ITG: a percent, its lead in percentage points (DDI's below).
    let settings = Settings {
        ruleset: "itg".into(),
        ..Settings::default()
    };
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

/// Under DDI the ghost's lead is in points of the score as shown (the
/// proportional part plus the absolute score), or in percentage points
/// where the score shows as a percent (`Ghost::showing_percent`); a
/// replay of the autoplayed best is graded as the play was, Absolute Bot.
#[test]
fn the_ghost_leads_in_ddi_points_or_percent() {
    let song = bundled("beatucada");
    let chart = 1;
    let settings = Settings::default();
    assert_eq!(settings.ruleset, "ddi");
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
        let mut last = None;
        let end = loop {
            *clock.borrow_mut() += 1.0 / 60.0;
            let (_, event) = s.tick(HostTime(*clock.borrow()));
            last = s.ghost_delta().or(last);
            if let Some(e) = event {
                break e;
            }
        };
        let SessionEvent::Finished { results, facts, .. } = end else {
            panic!("no end");
        };
        (results, facts, last)
    };
    let (results, facts, _) = play(0.0, None);
    assert_eq!(results.grade, "Absolute Bot");
    assert_eq!(facts.ruleset.id, "ddi");
    let best = kept("beatucada", &song, chart, &results, &facts, true);
    assert_eq!(best.results(&song).unwrap().grade, "Absolute Bot");
    let ghost = Ghost::of(&best, &song).unwrap();
    let whole = results.score.ddi.unwrap();
    assert_eq!(whole.ratio, 1_000_000);
    assert_eq!(whole.score, 1_000_000 + whole.absolute);
    assert_eq!(ghost.last(), whole.score as f64);
    // 30 ms late, every tap a W2: 40 points of 50 a note, and no W0.
    let (late, _, points) = play(0.03, Some(ghost.clone()));
    let behind = late.score.ddi.unwrap();
    assert_eq!(behind.ratio, 800_000);
    assert_eq!(behind.absolute, 100 * behind.points);
    assert_eq!(behind.score, 800_000 + behind.absolute);
    // The lead is of the score shown: the proportional part's 200,000
    // and the absolute score's difference.
    let lead = (behind.score - whole.score) as f64;
    assert!(lead < -200_000.0, "{lead}");
    assert!(
        matches!(points, Some(GhostDelta::Points(p)) if p == lead),
        "{points:?}"
    );
    let (_, _, percent) = play(0.03, Some(ghost.showing_percent(true)));
    assert!(
        matches!(percent, Some(GhostDelta::Percent(p)) if (p + 20.0).abs() < 1e-9),
        "{percent:?}"
    );
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

/// The lane covers in play: Page Up held moves Sudden+ on as it repeats,
/// Enter tapped hides it and shows it again, a controller's Start held
/// with the up arrow's lane moves it; the log keeps every change, the
/// settings the heights left, and a replay draws the same.
#[test]
fn lane_covers_move_and_hide_in_play_and_replay_alike() {
    use ddi_engine::cover::{CoverMode, LaneCover};
    let song = bundled("beatucada");
    let settings = Settings {
        lane_cover: LaneCover {
            mode: CoverMode::Sudden,
            sudden: 25,
            hidden: 10,
        },
        ..Settings::default()
    };
    settings.save();
    let clock: Clock = Rc::new(RefCell::new(100.0));
    let mut inputs = FakeInputs::new(&clock);
    let pad = "Pad (STANDARD GAMEPAD)";
    inputs.pads.push(ConnectedPad {
        vendor_product: None,
        id: pad.into(),
        index: 0,
        standard: true,
    });
    inputs.key(102.0, "PageUp", true);
    inputs.key(102.5, "PageUp", false);
    inputs.key(104.0, "Enter", true);
    inputs.key(104.1, "Enter", false);
    let pad_edge = |at: f64, control: &str, pressed: bool| RawInput {
        device: DeviceId::Gamepad(pad.into()),
        control: control.into(),
        pressed,
        host_time: HostTime(at),
        slot: 0,
    };
    // Hidden: the pad's arrows move nothing yet.
    inputs.queued.push(pad_edge(104.5, "button:9", true));
    inputs.queued.push(pad_edge(104.6, "button:12", true));
    inputs.queued.push(pad_edge(104.65, "button:12", false));
    inputs.queued.push(pad_edge(104.7, "button:9", false));
    inputs.key(105.0, "Enter", true);
    inputs.key(105.1, "Enter", false);
    inputs.queued.push(pad_edge(106.0, "button:9", true));
    inputs.queued.push(pad_edge(106.1, "button:12", true));
    inputs.queued.push(pad_edge(106.15, "button:12", false));
    inputs.queued.push(pad_edge(106.3, "button:9", false));
    let config = SessionConfig::from_settings(&settings, false, None);
    let mut s = PlaySession::start(
        &song,
        1,
        &settings,
        config,
        FakeAudio::new(&clock, 0.0),
        110.0,
        inputs,
    )
    .unwrap();
    assert!(
        s.inputs()
            .begun
            .as_ref()
            .unwrap()
            .0
            .iter()
            .any(|k| k == "PageUp")
    );
    let mut seen = Vec::new();
    let event = loop {
        *clock.borrow_mut() += 1.0 / 60.0;
        let now = *clock.borrow();
        let (_, event) = s.tick(HostTime(now));
        let cover = s.frame(HostTime(now)).cover.cover;
        if seen.last() != Some(&cover) {
            seen.push(cover);
        }
        if event.is_some() || now > 400.0 {
            break event;
        }
    };
    let Some(SessionEvent::Finished { results, facts, .. }) = event else {
        panic!("no end");
    };
    let sudden = |n: u8| LaneCover {
        mode: CoverMode::Sudden,
        sudden: n,
        hidden: 10,
    };
    let off = LaneCover {
        mode: CoverMode::Off,
        ..sudden(30)
    };
    // One step, then five more as it repeats (from 0.35 s, every 0.04 s).
    let last = seen.iter().position(|c| *c == off).unwrap();
    assert_eq!(seen[0], sudden(25));
    assert_eq!(seen[last - 1], sudden(30));
    assert_eq!(&seen[last..], &[off, sudden(30), sudden(31)]);
    assert_eq!(results.cover, sudden(31));
    assert_eq!(facts.log.covers.last().unwrap().cover, sudden(31));
    // The pad's up arrow still reached its lane.
    assert!(facts.log.inputs.iter().any(|i| i.lane == 2 && i.pressed));
    // Kept for the next song.
    assert_eq!(Settings::load().lane_cover, sudden(31));
    // A replay draws the covers as the play did, whatever its keys.
    let replay = kept("beatucada", &song, 1, &results, &facts, false);
    let clock: Clock = Rc::new(RefCell::new(3.0));
    let mut keys = FakeInputs::new(&clock);
    keys.key(4.0, "PageDown", true);
    keys.key(9.0, "PageDown", false);
    let (s, event) = watch(&replay, &song, 60.0, 0.0, 0.0, Some(keys), &clock);
    let Some(SessionEvent::Finished {
        results: watched, ..
    }) = event
    else {
        panic!("no replay end");
    };
    assert_eq!(watched.cover, sudden(31));
    assert_eq!(s.player.cover(), sudden(31));
    // Its heights are not kept: the settings stay as the play left them.
    assert_eq!(Settings::load().lane_cover, sudden(31));
}

/// A voice of every line a play speaks, each one sound 0.8 s long.
fn full_voice() -> crate::announcer::SharedVoice<f64> {
    use ddi_library::announcer::{Grade, Line};
    let mut v = crate::announcer::Voice::default();
    let mut lines = vec![
        Line::Ready,
        Line::HereWeGo,
        Line::CommentHot,
        Line::CommentGood,
        Line::CommentDanger,
        Line::ComboStopped,
        Line::Cleared,
        Line::Failed,
        Line::Quit,
        Line::FirstClear,
        Line::NewRecord,
        Line::EvaluationFullCombo(1),
        Line::Cheer,
        Line::Restart,
        Line::FullCombo,
        Line::FullComboW(1),
        Line::LastLife,
        Line::LifeLost,
    ];
    lines.extend((1..=10).map(|n| Line::Combo(n * 100)));
    lines.extend(Grade::ALL.map(Line::Grade));
    for line in lines {
        v.expect([line]);
        v.add(line, 0.8);
    }
    Rc::new(RefCell::new(v))
}

/// The announcer over an autoplayed play: ready and here we go before
/// the first note, the combos and the comments, cleared, the results'
/// full combo cheered, on the voice's bus at its gain; over a play the
/// life runs out of: failed at the fail, not cut when its hold ends, the
/// results' grade; nothing in calibration.
#[test]
fn the_announcer_speaks_over_a_play_and_on_its_results() {
    let song = bundled("beatucada");
    let settings = Settings {
        announcer: Some("voice".into()),
        volume: 0.5,
        announcer_volume: 0.5,
        ..Settings::default()
    };
    let clock: Clock = Rc::new(RefCell::new(100.0));
    let mut config = SessionConfig::from_settings(&settings, true, None);
    // Autoplay: told nothing of the history.
    config.prior = Some(crate::announcer::Prior::default());
    // The Hard chart: 283 notes.
    let mut s = PlaySession::start(
        &song,
        3,
        &settings,
        config,
        FakeAudio::new(&clock, 0.0),
        110.0,
        FakeInputs::new(&clock),
    )
    .unwrap();
    s.set_voice(full_voice());
    let Some(SessionEvent::Finished { .. }) = run(&mut s, &clock, 60.0, 200.0) else {
        panic!("no end");
    };
    let r = s.announcer_report().clone();
    let spoken: Vec<&str> = r.spoken.iter().map(|(f, _)| f.as_str()).collect();
    assert_eq!(
        spoken[..2],
        ["gameplay ready", "gameplay here we go normal"]
    );
    for line in ["gameplay 100 combo", "gameplay comment hot"] {
        assert!(spoken.contains(&line), "{line}: {spoken:?}");
    }
    // The last note keeps the full combo: said, then cleared.
    let end = spoken.iter().position(|l| *l == "gameplay full combo w1");
    assert_eq!(
        end.map(|i| spoken[i + 1]),
        Some("gameplay cleared"),
        "{spoken:?}"
    );
    assert_eq!(
        spoken[spoken.len() - 2..],
        ["evaluation full combo w1", "evaluation cheer"]
    );
    assert!(r.missing.is_empty() && r.absent.is_empty(), "{r:?}");
    // Every line on the voice's bus, at its share of the music's volume.
    let audio = s.audio();
    assert_eq!(audio.voiced.len(), spoken.len());
    assert!(audio.voiced.iter().all(|v| v.2 == 0.25));
    // Ready and here we go before the first note (their starts on the
    // audio clock in order, here we go ending half a second before).
    assert!(r.spoken[0].1 < r.spoken[1].1);

    // The life run out: failed, then the grade.
    let settings = Settings {
        announcer: Some("voice".into()),
        ..Settings::default()
    };
    let mut config = SessionConfig::from_settings(&settings, false, None);
    config.prior = Some(crate::announcer::Prior {
        played: true,
        cleared: true,
        best: 0.9,
    });
    let mut s = PlaySession::start(
        &song,
        1,
        &settings,
        config,
        FakeAudio::new(&clock, 0.0),
        110.0,
        FakeInputs::new(&clock),
    )
    .unwrap();
    s.set_voice(full_voice());
    let Some(SessionEvent::Finished { results, .. }) = run(&mut s, &clock, 60.0, 200.0) else {
        panic!("no end");
    };
    assert!(results.failed);
    let spoken: Vec<String> = s
        .announcer_report()
        .spoken
        .iter()
        .map(|(f, _)| f.clone())
        .collect();
    assert!(
        spoken.contains(&"gameplay failed".to_string()),
        "{spoken:?}"
    );
    assert_eq!(spoken.last().map(String::as_str), Some("evaluation e"));
    // The hold's end stopped the music, not the voice.
    assert_eq!(s.audio().music_stopped, 1);

    // Calibration: nothing.
    let mode = CalMode::Visual;
    let cal = calibration::song(mode);
    let mut config = SessionConfig::from_settings(&settings, true, None);
    config.calibrate(mode);
    let audio = FakeAudio::new(&clock, 0.0);
    let click = audio
        .buffer_from_pcm(vec![calibration::click_samples(48_000.0, &cal)], 48_000.0)
        .unwrap();
    let mut s = PlaySession::start(
        &cal,
        0,
        &settings,
        config,
        audio,
        click,
        FakeInputs::new(&clock),
    )
    .unwrap();
    s.set_voice(full_voice());
    run(&mut s, &clock, 60.0, 200.0);
    assert!(s.audio().voiced.is_empty());
}

/// On StepMania's battery, a play left unplayed loses its lives: the
/// announcer says one lost, the last left, then the fail.
#[test]
fn the_announcer_counts_a_batterys_lives_down() {
    use ddi_engine::rules::{LifeType, StepManiaOptions};
    let song = bundled("beatucada");
    let settings = Settings {
        // A StepMania preset: its options are what is tried.
        ruleset: "itg".into(),
        announcer: Some("voice".into()),
        stepmania: StepManiaOptions {
            life_type: LifeType::Battery,
            lives: 4,
            ..StepManiaOptions::default()
        },
        ..Settings::default()
    };
    let clock: Clock = Rc::new(RefCell::new(100.0));
    let mut s = PlaySession::start(
        &song,
        3,
        &settings,
        SessionConfig::from_settings(&settings, false, None),
        FakeAudio::new(&clock, 0.0),
        110.0,
        FakeInputs::new(&clock),
    )
    .unwrap();
    s.set_voice(full_voice());
    let Some(SessionEvent::Finished { results, .. }) = run(&mut s, &clock, 60.0, 200.0) else {
        panic!("no end");
    };
    assert!(results.failed);
    let spoken: Vec<&str> = s
        .announcer_report()
        .spoken
        .iter()
        .map(|(f, _)| f.as_str())
        .collect();
    let at = |line: &str| spoken.iter().position(|l| *l == line);
    let (lost, last, failed) = (
        at("gameplay life lost"),
        at("gameplay last life"),
        at("gameplay failed"),
    );
    assert!(lost.is_some() && last.is_some(), "{spoken:?}");
    assert!(lost < last && last < failed, "{spoken:?}");
}

#[test]
fn the_dancing_hand_leaves_the_plays_chart_and_options_alone() {
    // A hand (four at once) and a jump over a hold: the hand plays two
    // at once on a copy of its own; the play judges every note.
    let song = ddi_chart::formats::sm::parse_sm("#TITLE:Shared input;#BPMS:0=120;#NOTES:dance-single::Medium:4:0,0,0,0,0:\n1111\n2000\n0111\n3000\n;").unwrap();
    for enabled in [false, true] {
        let settings = Settings {
            dancing_hand: enabled,
            ..Settings::default()
        };
        let clock: Clock = Rc::new(RefCell::new(0.));
        let mut config = SessionConfig::from_settings(&settings, true, None);
        config.seed = 7;
        let mut s = PlaySession::start(
            &song,
            0,
            &settings,
            config,
            FakeAudio::new(&clock, 0.),
            10.,
            FakeInputs::new(&clock),
        )
        .unwrap();
        let expected = ddi_engine::transform::apply(
            &song.charts[0].notes,
            &ddi_chart::Layout::dance_single(),
            &ddi_engine::TransformOptions::default(),
            7,
        )
        .notes;
        assert_eq!(
            s.player
                .judge()
                .notes()
                .iter()
                .map(|n| n.note.clone())
                .collect::<Vec<_>>(),
            expected
        );
        assert_eq!(song.charts[0].notes.len(), 8);
        let Some(SessionEvent::Finished { results, facts, .. }) = run(&mut s, &clock, 60., 20.)
        else {
            panic!("play did not finish")
        };
        assert!(!results.assist);
        assert_eq!(facts.options.transform, settings.transform);
        assert_eq!(results.tally.let_go, 0);
        assert!(results.tally.taps[1..].iter().all(|n| *n == 0));
        if !enabled {
            let replay = kept("before-hand-mode", &song, 0, &results, &facts, false);
            let hand_settings = Settings {
                dancing_hand: true,
                ..Settings::default()
            };
            let replay_clock: Clock = Rc::new(RefCell::new(0.));
            let mut config = SessionConfig::from_settings(&hand_settings, false, None);
            config.set_replay(&replay);
            let replayed = PlaySession::start(
                &song,
                0,
                &hand_settings,
                config,
                FakeAudio::new(&replay_clock, 0.),
                10.,
                FakeInputs::new(&replay_clock),
            )
            .unwrap();
            assert_eq!(replayed.player.options().transform, facts.options.transform);
            assert_eq!(
                replayed.player.judge().notes().len(),
                song.charts[0].notes.len()
            );
            let mut config = SessionConfig::from_settings(&hand_settings, true, None);
            config.calibrate(CalMode::Combined);
            let calibration = PlaySession::start(
                &song,
                0,
                &hand_settings,
                config,
                FakeAudio::new(&replay_clock, 0.),
                10.,
                FakeInputs::new(&replay_clock),
            )
            .unwrap();
            assert_eq!(
                calibration.player.options().transform,
                ddi_engine::TransformOptions::default()
            );
        }
    }
}

#[test]
fn solo_hand_leaves_the_plays_chart_and_options_alone() {
    let song=ddi_chart::formats::sm::parse_sm("#TITLE:Shared Solo;#BPMS:0=120;#NOTES:dance-solo::Medium:4:0,0,0,0,0:\n110011\n020000\n001110\n030000;").unwrap();
    let source = song.charts[0].notes.clone();
    for enabled in [false, true] {
        let settings = Settings {
            dancing_hand: enabled,
            ..Settings::default()
        };
        let clock: Clock = Rc::new(RefCell::new(0.));
        let config = SessionConfig::from_settings(&settings, true, None);
        let mut s = PlaySession::start(
            &song,
            0,
            &settings,
            config,
            FakeAudio::new(&clock, 0.),
            10.,
            FakeInputs::new(&clock),
        )
        .unwrap();
        let expected = ddi_engine::transform::apply(
            &source,
            &ddi_chart::Layout::dance_solo(),
            &settings.transform,
            0,
        )
        .notes;
        assert_eq!(
            s.player
                .judge()
                .notes()
                .iter()
                .map(|n| n.note.clone())
                .collect::<Vec<_>>(),
            expected
        );
        let Some(SessionEvent::Finished { results, facts, .. }) = run(&mut s, &clock, 60., 20.)
        else {
            panic!("play did not finish")
        };
        assert!(!results.assist);
        assert_eq!(facts.options.transform, settings.transform);
        assert_eq!(results.tally.let_go, 0);
        assert!(results.tally.taps[1..].iter().all(|n| *n == 0));
    }
    assert_eq!(song.charts[0].notes, source);
}

#[test]
fn pump_hand_leaves_the_plays_chart_and_options_alone() {
    let song=ddi_chart::formats::sm::parse_sm("#TITLE:Shared Pump;#BPMS:0=120;#NOTES:pump-single::Medium:4:0,0,0,0,0:\n11011\n02000\n10110\n03000;").unwrap();
    let source = song.charts[0].notes.clone();
    for enabled in [false, true] {
        let settings = Settings {
            dancing_hand: enabled,
            ..Settings::default()
        };
        let clock: Clock = Rc::new(RefCell::new(0.));
        let config = SessionConfig::from_settings(&settings, true, None);
        let mut s = PlaySession::start(
            &song,
            0,
            &settings,
            config,
            FakeAudio::new(&clock, 0.),
            10.,
            FakeInputs::new(&clock),
        )
        .unwrap();
        let expected = ddi_engine::transform::apply(
            &source,
            &ddi_chart::Layout::pump_single(),
            &settings.transform,
            0,
        )
        .notes;
        assert_eq!(
            s.player
                .judge()
                .notes()
                .iter()
                .map(|n| n.note.clone())
                .collect::<Vec<_>>(),
            expected
        );
        let Some(SessionEvent::Finished { results, facts, .. }) = run(&mut s, &clock, 60., 20.)
        else {
            panic!("play did not finish")
        };
        assert!(!results.assist);
        assert_eq!(facts.options.transform, settings.transform);
        assert_eq!(results.tally.let_go, 0);
        assert!(results.tally.taps[1..].iter().all(|n| *n == 0));
    }
    assert_eq!(song.charts[0].notes, source);
}

#[test]
fn seven_key_hand_leaves_the_plays_chart_and_options_alone() {
    let song=ddi_chart::formats::sm::parse_sm("#TITLE:Shared seven-key;#BPMS:0=120;#NOTES:danoni-7::Medium:4:0,0,0,0,0:\n1100110\n0200000\n0011100\n0300000;").unwrap();
    let source = song.charts[0].notes.clone();
    for enabled in [false, true] {
        let settings = Settings {
            dancing_hand: enabled,
            ..Settings::default()
        };
        let clock: Clock = Rc::new(RefCell::new(0.));
        let config = SessionConfig::from_settings(&settings, true, None);
        let mut s = PlaySession::start(
            &song,
            0,
            &settings,
            config,
            FakeAudio::new(&clock, 0.),
            10.,
            FakeInputs::new(&clock),
        )
        .unwrap();
        let expected = ddi_engine::transform::apply(
            &source,
            &ddi_chart::Layout::builtin("danoni-7").unwrap(),
            &settings.transform,
            0,
        )
        .notes;
        assert_eq!(
            s.player
                .judge()
                .notes()
                .iter()
                .map(|n| n.note.clone())
                .collect::<Vec<_>>(),
            expected
        );
        let Some(SessionEvent::Finished { results, facts, .. }) = run(&mut s, &clock, 60., 20.)
        else {
            panic!("play did not finish")
        };
        assert!(!results.assist);
        assert_eq!(facts.options.transform, settings.transform);
        assert_eq!(results.tally.let_go, 0);
        assert!(results.tally.taps[1..].iter().all(|n| *n == 0));
    }
    assert_eq!(song.charts[0].notes, source);
}

#[test]
fn techno_hand_leaves_the_plays_chart_and_options_alone() {
    let song=ddi_chart::formats::sm::parse_sm("#TITLE:Shared Techno;#BPMS:0=120;#NOTES:techno-single8::Medium:4:0,0,0,0,0:\n11001100\n02000000\n00111000\n03000000;").unwrap();
    let source = song.charts[0].notes.clone();
    for enabled in [false, true] {
        let settings = Settings {
            dancing_hand: enabled,
            ..Settings::default()
        };
        let clock: Clock = Rc::new(RefCell::new(0.));
        let config = SessionConfig::from_settings(&settings, true, None);
        let mut s = PlaySession::start(
            &song,
            0,
            &settings,
            config,
            FakeAudio::new(&clock, 0.),
            10.,
            FakeInputs::new(&clock),
        )
        .unwrap();
        let expected = ddi_engine::transform::apply(
            &source,
            &ddi_chart::Layout::builtin("techno-single8").unwrap(),
            &settings.transform,
            0,
        )
        .notes;
        assert_eq!(
            s.player
                .judge()
                .notes()
                .iter()
                .map(|n| n.note.clone())
                .collect::<Vec<_>>(),
            expected
        );
        let Some(SessionEvent::Finished { results, facts, .. }) = run(&mut s, &clock, 60., 20.)
        else {
            panic!("play did not finish")
        };
        assert!(!results.assist);
        assert_eq!(facts.options.transform, settings.transform);
        assert_eq!(results.tally.let_go, 0);
        assert!(results.tally.taps[1..].iter().all(|n| *n == 0));
    }
    assert_eq!(song.charts[0].notes, source);
}

#[test]
fn nine_key_hand_leaves_the_plays_chart_and_options_alone() {
    let song=ddi_chart::formats::sm::parse_sm("#TITLE:Shared nine-key;#BPMS:0=120;#NOTES:danoni-9B::Medium:4:0,0,0,0,0:\n110011000\n020000000\n001110000\n030000000;").unwrap();
    let source = song.charts[0].notes.clone();
    for enabled in [false, true] {
        let settings = Settings {
            dancing_hand: enabled,
            ..Settings::default()
        };
        let clock: Clock = Rc::new(RefCell::new(0.));
        let config = SessionConfig::from_settings(&settings, true, None);
        let mut s = PlaySession::start(
            &song,
            0,
            &settings,
            config,
            FakeAudio::new(&clock, 0.),
            10.,
            FakeInputs::new(&clock),
        )
        .unwrap();
        let expected = ddi_engine::transform::apply(
            &source,
            &ddi_chart::Layout::builtin("danoni-9B").unwrap(),
            &settings.transform,
            0,
        )
        .notes;
        assert_eq!(
            s.player
                .judge()
                .notes()
                .iter()
                .map(|n| n.note.clone())
                .collect::<Vec<_>>(),
            expected
        );
        let Some(SessionEvent::Finished { results, facts, .. }) = run(&mut s, &clock, 60., 20.)
        else {
            panic!("play did not finish")
        };
        assert!(!results.assist);
        assert_eq!(facts.options.transform, settings.transform);
        assert_eq!(results.tally.let_go, 0);
        assert!(results.tally.taps[1..].iter().all(|n| *n == 0));
    }
    assert_eq!(song.charts[0].notes, source);
}

#[test]
fn no_stretch_is_played_and_replayed_independently_of_the_hand_setting() {
    use ddi_chart::{Layout, Note, NoteKind, Tick};
    for (id, expected) in [
        ("dance-double", 1),
        ("pump-double", 1),
        ("pump-halfdouble", 2),
        ("danoni-9A", 1),
    ] {
        let mut song = ddi_chart::formats::sm::parse_sm(
            "#BPMS:0=120;#NOTES:dance-single::Easy:1:0,0,0,0,0:1000;",
        )
        .unwrap();
        song.charts[0].layout = id.into();
        let lanes = Layout::builtin(id).unwrap().lane_count() as u8;
        song.charts[0].notes = vec![
            Note::new(Tick(0), 0, NoteKind::Tap),
            Note::new(Tick(0), lanes - 1, NoteKind::Tap),
        ];
        let source = song.charts[0].notes.clone();
        for hand in [false, true] {
            for enabled in [false, true] {
                let mut settings = Settings {
                    dancing_hand: hand,
                    ..Default::default()
                };
                settings.transform.no_stretch = enabled;
                let clock: Clock = Rc::new(RefCell::new(0.));
                let config = SessionConfig::from_settings(&settings, true, None);
                let mut s = PlaySession::start(
                    &song,
                    0,
                    &settings,
                    config,
                    FakeAudio::new(&clock, 0.),
                    10.,
                    FakeInputs::new(&clock),
                )
                .unwrap();
                assert_eq!(
                    s.player.judge().notes().len(),
                    if enabled { expected } else { 2 },
                    "{id}"
                );
                let Some(SessionEvent::Finished { results, facts, .. }) =
                    run(&mut s, &clock, 60., 20.)
                else {
                    panic!("unfinished")
                };
                assert_eq!(results.assist, enabled);
                assert_eq!(facts.options.transform.no_stretch, enabled);
                assert_eq!(results.tally.taps[1..].iter().sum::<u32>(), 0);
                let replay = kept("no-stretch", &song, 0, &results, &facts, true);
                let mut config = SessionConfig::from_settings(&Settings::default(), false, None);
                config.set_replay(&replay);
                let rc: Clock = Rc::new(RefCell::new(0.));
                let mut replayed = PlaySession::start(
                    &song,
                    0,
                    &Settings::default(),
                    config,
                    FakeAudio::new(&rc, 0.),
                    10.,
                    FakeInputs::new(&rc),
                )
                .unwrap();
                assert_eq!(replayed.player.options().transform.no_stretch, enabled);
                let Some(SessionEvent::Finished { results: again, .. }) =
                    run(&mut replayed, &rc, 60., 20.)
                else {
                    panic!("replay unfinished")
                };
                assert_eq!(again.tally, results.tally);
                assert_eq!(again.score, results.score);
                assert_eq!(again.assist, enabled);
            }
        }
        assert_eq!(song.charts[0].notes, source);
    }
}

/// Every line the announcer says in play, in its staged scene
/// (`announcer::scene::in_play`, the announcer studio's step 4), under
/// the player's rules (ITG, DDR A, StepMania 5): the line is said by the
/// real announcer over the real play, before the scene is over; a scene
/// at a play's end plays on into its results; the restart's plays again
/// from the same place, opened by its line.
#[test]
fn each_line_said_in_play_is_ringed_on_something() {
    use ddi_library::announcer::{Line, Spoken};
    for line in Line::all()
        .into_iter()
        .filter(|l| l.spoken() == Spoken::Play)
    {
        assert!(
            crate::announcer::scene::in_play::mark(line).is_some(),
            "{line:?}"
        );
    }
}

#[test]
fn each_play_lines_scene_says_it() {
    use crate::announcer::scene::in_play;
    use ddi_library::announcer::Line;
    let mut lines = vec![
        Line::Ready,
        Line::HereWeGo,
        Line::Restart,
        Line::CommentHot,
        Line::CommentGood,
        Line::CommentDanger,
        Line::CommentOni,
        Line::ComboOverflow,
        Line::ComboStopped,
        Line::LifeLost,
        Line::LastLife,
        Line::FullCombo,
        Line::Cleared,
        Line::Failed,
    ];
    lines.extend((1..=10).map(|n| Line::Combo(n * 100)));
    // A pack's own counts: one sooner than the scene's lead-in, and two
    // not hundreds.
    let own = [Line::Combo(5), Line::Combo(64), Line::Combo(1024)];
    lines.extend(own);
    lines.extend((1..=3).map(Line::FullComboW));
    // Every line, said in 0.8 s (a count of the pack's own only in its
    // scene: 5 would be said in every scene).
    let voice = |line: Line| {
        let mut v = crate::announcer::Voice::default();
        for line in Line::all()
            .into_iter()
            .chain(own.into_iter().filter(|l| *l == line))
        {
            v.expect([line]);
            v.add(line, 0.8);
        }
        Rc::new(RefCell::new(v))
    };
    for ruleset in ["itg", "ddr-a", "sm5"] {
        for &line in &lines {
            let pick = in_play::pick(line).unwrap();
            let song = bundled(pick.song);
            let chart = song
                .charts
                .iter()
                .position(|c| c.layout == in_play::STYLE && c.difficulty == pick.difficulty)
                .unwrap();
            let settings = pick.settings(&Settings {
                announcer: Some("voice".into()),
                ruleset: ruleset.into(),
                ..Settings::default()
            });
            let config = SessionConfig::from_settings(&settings, false, None);
            let (rules, options) = config.player_spec(&song, chart, &settings).unwrap();
            let Some(scene) = in_play::stage(line, &song, chart, &rules, options, &|_| Some(0.8))
            else {
                // A plain full combo is DDR A's, whatever the player's.
                panic!("{ruleset}: no scene for {line:?}");
            };
            let clock: Clock = Rc::new(RefCell::new(100.0));
            let play = |staging: Staging, restart: bool| {
                let mut config = SessionConfig::from_settings(&settings, false, None);
                config.staging = Some(staging);
                config.restart = restart;
                let mut s = PlaySession::start(
                    &song,
                    chart,
                    &settings,
                    config,
                    FakeAudio::new(&clock, 0.0),
                    200.0,
                    FakeInputs::new(&clock),
                )
                .unwrap();
                s.set_voice(voice(line));
                let event = run(&mut s, &clock, 60.0, 200.0);
                let spoken: Vec<String> = s
                    .announcer_report()
                    .spoken
                    .iter()
                    .map(|(f, _)| f.clone())
                    .collect();
                (s, event, spoken)
            };
            let (s, event, mut spoken) = play(scene.staging.clone(), false);
            let elapsed = |s: &PlaySession<FakeAudio, FakeInputs>| {
                s.player
                    .clock()
                    .song_time_at_host(HostTime(*clock.borrow()))
            };
            match (&event, &scene.restarted, scene.results) {
                (Some(SessionEvent::Restart), Some(again), _) => {
                    // In the play already: nothing said before the restart.
                    assert!(spoken.is_empty(), "{ruleset}: {spoken:?}");
                    let (_, event, again) = play(again.clone(), true);
                    assert!(matches!(event, Some(SessionEvent::SceneOver)));
                    assert_eq!(again.first().map(String::as_str), Some("gameplay restart"));
                    spoken = again;
                }
                (Some(SessionEvent::Finished { results, .. }), None, true) => {
                    if let Some(fc) = in_play::full_combo_of(line) {
                        assert_eq!(results.full_combo, fc, "{ruleset}: {line:?}");
                    }
                    assert_eq!(results.failed, line == Line::Failed, "{ruleset}: {line:?}");
                }
                (Some(SessionEvent::SceneOver), None, false) => {
                    // Over soon after the start: a scene, not the song.
                    let length = elapsed(&s) - scene.staging.from;
                    assert!(length < 10.0, "{ruleset}: {line:?} {length}");
                }
                _ => panic!(
                    "{ruleset}: {line:?} ended {:?}",
                    event.as_ref().map(std::mem::discriminant)
                ),
            }
            let folder = match line {
                // Past 1000 the combo's line is the overflow's.
                Line::ComboOverflow => "gameplay combo overflow".to_string(),
                _ => line.folder(),
            };
            assert!(
                spoken.contains(&folder),
                "{ruleset}: {line:?} not said: {spoken:?}"
            );
            // Here we go's alone: no room for ready.
            if line == Line::HereWeGo {
                assert!(
                    !spoken.iter().any(|f| f == "gameplay ready"),
                    "{ruleset}: {spoken:?}"
                );
            }
            // Not through the pre-start screen, no opening (a restart's
            // song starts again, here we go and all).
            if !in_play::opens_on_prompt(line) && line != Line::Restart {
                assert!(
                    !spoken
                        .iter()
                        .any(|f| f == "gameplay ready" || f.starts_with("gameplay here we go")),
                    "{ruleset}: {line:?} {spoken:?}"
                );
            }
            // The play knows what the scene is for.
            assert_eq!(scene.staging.example, Some(line));
        }
    }
}

#[test]
fn cpu_whole_song_matches_native_judge_through_stalls_and_offsets() {
    let original = bundled("beatucada");
    let mut song = ddi_engine::cpu::project(&original, 3).unwrap();
    // Put the first note at zero, exercising a legitimately early first press.
    let c = &song.charts[3];
    let j = ddi_engine::Judge::new(&c.notes, c.timing(&song), ddi_engine::presets::itg().judge);
    let first = j
        .notes()
        .iter()
        .filter(|n| n.judgeable)
        .map(|n| n.seconds)
        .reduce(f64::min)
        .unwrap();
    song.shift_notes(-first);
    let c = &song.charts[3];
    let j = ddi_engine::Judge::new(&c.notes, c.timing(&song), ddi_engine::presets::itg().judge);
    let mut script = Vec::new();
    let mut up = [f64::NEG_INFINITY; 4];
    for (i, n) in j.notes().iter().filter(|n| n.judgeable).enumerate() {
        if i % 11 == 5 {
            continue;
        } // Deliberate omitted taps, plus early/late hits.
        let delta = [-0.17, 0.008, 0.06, -0.035, 0.13][i % 5];
        let press = (n.seconds + delta).max(up[n.note.lane as usize] + 0.0001);
        up[n.note.lane as usize] = press + 0.012;
        script.push(Press {
            press,
            release: press + 0.012,
            lane: n.note.lane,
        });
    }
    script.sort_by(|a, b| a.press.total_cmp(&b.press));
    ddi_engine::cpu::validate_script(&script).unwrap();
    for (fps, offset) in [(60., 0.), (144., 0.04), (2., -0.8)] {
        let settings = Settings::default();
        let clock: Clock = Rc::new(RefCell::new(100.));
        let mut inputs = FakeInputs::new(&clock);
        inputs.key(101., "ArrowLeft", true);
        inputs.key(101.1, "ArrowLeft", false);
        let mut config = SessionConfig::from_settings(&settings, false, None);
        config.cpu = Some(script.clone());
        config.options.clock.audio_offset = offset;
        let mut s = PlaySession::start(
            &song,
            3,
            &settings,
            config,
            FakeAudio::new(&clock, 0.012),
            110.,
            inputs,
        )
        .unwrap();
        let Some(SessionEvent::Finished { results, facts, .. }) = run(&mut s, &clock, fps, 200.)
        else {
            panic!("CPU did not finish at {fps}");
        };
        assert_eq!(
            facts.log.inputs.len(),
            script.len() * 2,
            "Human keys must not enter CPU log"
        );
        // The autoplay scheduler cancels device calibration offsets: CPU laws
        // are in judged chart time, not physical acoustic time.
        let mut expected = script
            .iter()
            .flat_map(|p| {
                [
                    ddi_engine::history::LoggedInput {
                        time: (p.press * 1e6).round() / 1e6,
                        lane: p.lane,
                        pressed: true,
                    },
                    ddi_engine::history::LoggedInput {
                        time: (p.release * 1e6).round() / 1e6,
                        lane: p.lane,
                        pressed: false,
                    },
                ]
            })
            .collect::<Vec<_>>();
        expected.sort_by(|a, b| a.time.total_cmp(&b.time));
        assert_eq!(
            facts.log.inputs, expected,
            "Scheduling at {fps} fps, offset {offset}"
        );
        let mut rules = settings.ruleset_for(&song.charts[3]);
        rules.fail = ddi_engine::FailPolicy::Off;
        let direct = ddi_engine::history::replay_with(&song, 3, rules, &facts.options, &facts.log);
        assert_eq!(results.tally, direct.tally);
        assert_eq!(results.score, direct.score);
        assert_eq!(results.timeline, direct.timeline);
        assert_eq!(results.late_edges, 0);
        assert!(!results.failed);
        assert_eq!(s.auto.as_ref().unwrap().next, script.len());
        assert!(s.auto.as_ref().unwrap().pending_release.is_empty());
    }
}

#[test]
fn cpu_misses_finish_and_cancel_still_works() {
    let song = ddi_engine::cpu::project(&bundled("beatucada"), 3).unwrap();
    for cancel in [false, true] {
        let settings = Settings::default();
        let clock: Clock = Rc::new(RefCell::new(100.));
        let mut config = SessionConfig::from_settings(&settings, false, None);
        config.cpu = Some(vec![]);
        let mut inputs = FakeInputs::new(&clock);
        if cancel {
            inputs.key(101., "Escape", true);
        }
        let mut s = PlaySession::start(
            &song,
            3,
            &settings,
            config,
            FakeAudio::new(&clock, 0.),
            110.,
            inputs,
        )
        .unwrap();
        let end = run(&mut s, &clock, 60., 200.);
        if cancel {
            assert!(matches!(end, Some(SessionEvent::Aborted { .. })));
        } else {
            let Some(SessionEvent::Finished { results, .. }) = end else {
                panic!("no-fail CPU stopped");
            };
            assert!(!results.failed);
            assert!(results.tally.taps[5] > 0);
            assert!(s.player.log().inputs.is_empty());
        }
    }
}

#[test]
fn cpu_timestamps_do_not_pick_up_slewed_clock_drift() {
    let song = ddi_engine::cpu::project(&bundled("beatucada"), 3).unwrap();
    let settings = Settings::default();
    let clock: Clock = Rc::new(RefCell::new(100.));
    let c = &song.charts[3];
    let j = ddi_engine::Judge::new(&c.notes, c.timing(&song), settings.ruleset_for(c).judge);
    let script: Vec<_> = ddi_engine::autoplay::script(j.notes(), 0.025)
        .into_iter()
        .map(|p| Press {
            release: p.press + 0.012,
            ..p
        })
        .collect();
    let mut config = SessionConfig::from_settings(&settings, false, None);
    config.cpu = Some(script.clone());
    let mut s = PlaySession::start(
        &song,
        3,
        &settings,
        config,
        FakeAudio::new(&clock, 0.012),
        110.,
        FakeInputs::new(&clock),
    )
    .unwrap();
    *clock.borrow_mut() += 1.;
    s.tick(HostTime(*clock.borrow()));
    // Change the sampled clock relationship, then deliver the full chart in a
    // long frame. Due inputs must retain their prepared song times exactly.
    s.audio.latency += 0.005;
    *clock.borrow_mut() += 130.;
    let (_, end) = s.tick(HostTime(*clock.borrow()));
    let Some(SessionEvent::Finished { results, facts, .. }) = end else {
        panic!("did not finish");
    };
    let actual: Vec<_> = facts
        .log
        .inputs
        .iter()
        .filter(|i| i.pressed)
        .map(|i| (i.lane, i.time))
        .collect();
    let expected: Vec<_> = script
        .iter()
        .map(|p| (p.lane, (p.press * 1e6).round() / 1e6))
        .collect();
    assert_eq!(actual, expected);
    assert_eq!(results.late_edges, 0);
}

mod stage;
