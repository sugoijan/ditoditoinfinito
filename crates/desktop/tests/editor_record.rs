//! Recording in the chart editor on desktop (`docs/plans/editor.md`,
//! "Note record"), headless: no sound device, the editor's output on a
//! fake clock; keys stamped as the OS stamps them and a fake controller's
//! edges routed as the shell routes them (raw while recording), at host
//! times worked out from when the music was asked for, with an audio
//! offset in the settings. A pattern pressed when its rows are heard
//! lands on them, through a BPM change and a stop, and the presses'
//! mean off the grid is what they were pressed at: the offset applied
//! once (else every press would come out 50 ms late).

use std::cell::RefCell;
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

use ddi_chart::{NoteKind, Tick};
use ddi_platform::gamepad::standard;
use ddi_platform::storage::{self, MemoryStore};
use ddi_platform::{DeviceId, HostTime, RawInput};
use ddi_ui::event::KeyEvent;
use ddi_ui::screens::DESIGN;
use ddi_ui::state::settings::Settings;
use ddi_ui::testing::Harness;
use ddi_ui::{Modifiers, TextMode, UiEvent};

mod common;

/// The audio offset in the settings, seconds.
const OFFSET: f64 = 0.05;

fn scratch() -> PathBuf {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../target/scratch/record");
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(dir.join("songs/Pack/Song")).unwrap();
    dir
}

/// A mono 16-bit WAV of silence, `seconds` long.
fn wav(seconds: f64) -> Vec<u8> {
    let rate = 44_100u32;
    let n = (seconds * f64::from(rate)) as u32;
    let mut out = b"RIFF".to_vec();
    out.extend((36 + n * 2).to_le_bytes());
    out.extend(b"WAVEfmt ");
    out.extend(16u32.to_le_bytes());
    out.extend(1u16.to_le_bytes());
    out.extend(1u16.to_le_bytes());
    out.extend(rate.to_le_bytes());
    out.extend((rate * 2).to_le_bytes());
    out.extend(2u16.to_le_bytes());
    out.extend(16u16.to_le_bytes());
    out.extend(b"data");
    out.extend((n * 2).to_le_bytes());
    out.resize(out.len() + n as usize * 2, 0);
    out
}

struct Run {
    h: Harness,
    clock: Arc<AtomicU64>,
    input: ddi_desktop::input::Input,
    pad: (String, u32),
}

impl Run {
    fn now(&self) -> f64 {
        self.clock.load(Ordering::SeqCst) as f64 / 1e9
    }

    /// The clock moved to host second `to`, 50 ms at a time (updated,
    /// not drawn: drawing every step is slow here).
    fn until(&mut self, to: f64) {
        while self.now() < to {
            let next = (self.now() + 0.05).min(to);
            self.clock
                .store((next * 1e9).ceil() as u64, Ordering::SeqCst);
            self.h.ui.update(0.05);
        }
    }

    fn key(&mut self, code: &str, down: bool, at: f64) {
        self.until(at);
        let mut e = if down {
            KeyEvent::down(code, code)
        } else {
            KeyEvent::up(code, code)
        };
        e.time = at * 1000.0;
        self.h.send(UiEvent::Key(e));
    }

    /// The fake controller's `button`, routed as the shell routes it.
    fn pad(&mut self, button: u16, down: bool, at: f64) {
        self.until(at);
        let raw = RawInput {
            device: DeviceId::Gamepad(self.pad.0.clone()),
            control: format!("button:{button}"),
            pressed: down,
            host_time: HostTime(at),
            slot: self.pad.1,
        };
        let event = self.input.route(&self.h.ui, raw).expect("routed");
        assert!(matches!(event, UiEvent::Raw(_)), "raw while recording");
        self.h.send(event);
    }
}

#[test]
fn a_pattern_recorded_at_known_seconds_lands_on_its_rows() {
    common::indexes();
    let dir = scratch();
    // SAFETY: set before any thread of this test reads the environment.
    unsafe { std::env::set_var("DDI_HOME", dir.join("home")) };
    ddi_desktop::audio::set_headless(true);
    let clock = Arc::new(AtomicU64::new(1_000_000_000));
    let c = clock.clone();
    ddi_desktop::audio::set_headless_clock(Some(Arc::new(move || c.load(Ordering::SeqCst))));
    storage::install(Rc::new(MemoryStore::default()));
    let mut settings = Settings::load();
    settings.audio_offset = OFFSET;
    settings.save();
    // 120 BPM, 180 from beat 8, a stop of 0.4 s on beat 10.
    let song = dir.join("songs/Pack/Song");
    std::fs::write(
        song.join("song.sm"),
        "#TITLE:Song;#ARTIST:Me;#MUSIC:song.wav;#OFFSET:0;#BPMS:0=120,8=180;\
         #STOPS:10=0.4;#NOTES:dance-single:x:Easy:3:0,0,0,0,0:0000\n0000\n0000\n0000\n;",
    )
    .unwrap();
    std::fs::write(song.join("song.wav"), wav(12.0)).unwrap();
    let songs = ddi_desktop::songs::Songs::new(vec![dir.join("songs")]);
    songs.skip_offer();
    songs.start(Arc::new(|| {}));
    if let Some(l) = songs.library.borrow_mut().as_mut() {
        l.wait();
    }
    let id = ddi_library::pack::song_id("Pack", "Song");
    let timing = ddi_desktop::editor::read(&id, 0).unwrap().timing().clone();
    let at = |beat: f64| timing.seconds_at_beat(beat);

    let fold: ddi_ui::dev_widgets::FoldFn = Rc::new(|s: &str| s.to_lowercase());
    let sound = Rc::new(RefCell::new(ddi_desktop::preview::Player::with(
        ddi_desktop::audio::Audio::silent(0.0),
        ddi_desktop::preview::PreviewSong::bundled,
    )));
    let (ui, env, _) = ddi_desktop::shell::library_ui("select", fold, sound, &songs);
    env.reduced_motion.set(true);
    let mut h = Harness::with_gestures(DESIGN, DESIGN, 1.0, TextMode::Own, false);
    h.ui = ui;
    h.ui.push(ddi_desktop::editor::screen(&env, &songs, &id, 0).expect("the editor"));
    h.run(0.3);
    let view = || env.state.borrow().editor.clone().expect("the editor");
    // The music decoded (its overview made from it) before recording.
    let started = std::time::Instant::now();
    while view().overview == 0 {
        assert!(started.elapsed().as_secs() < 60, "never decoded");
        std::thread::sleep(std::time::Duration::from_millis(20));
        h.frame(0.0);
    }
    // A fake controller, connected as the pads' thread says it.
    let mut input = ddi_desktop::input::Input::default();
    input.start_pads(Box::new(|| {}), Some(""));
    let started = std::time::Instant::now();
    while ddi_desktop::input::connected_pads().is_empty() {
        assert!(started.elapsed().as_secs() < 10, "no fake pad");
        std::thread::sleep(std::time::Duration::from_millis(5));
        input.poll(&mut h.ui);
    }
    let pad = ddi_desktop::input::connected_pads()[0].clone();
    let mut r = Run {
        h,
        clock,
        input,
        pad: (pad.id, pad.index),
    };

    // At beat 4, 8ths; R.
    for code in ["PageDown", "ArrowLeft", "ArrowLeft"] {
        r.h.key(code, code);
    }
    assert_eq!((view().cursor, view().snap), (4 * 48, 8));
    let asked = r.now();
    r.h.key("KeyR", "r");
    assert!(view().listening && view().record.unwrap().active);
    // The music scheduled 0.1 s after it was asked for, from the
    // pre-roll (a measure before beat 4: 0 s); a song second is heard
    // the offset after it plays.
    let from = at(0.0);
    let heard = |s: f64| asked + ddi_session::listen::LOOKAHEAD + (s - from) + OFFSET;
    // Pressed: (lane, by key or by the pad's control, second, held).
    let presses: [(u8, Result<&str, u16>, f64, f64); 8] = [
        (0, Ok("ArrowLeft"), at(4.0) + 0.01, 0.08),
        (1, Err(standard::DPAD_DOWN), at(4.5) + 0.01, 0.08),
        (2, Ok("ArrowUp"), at(5.0) - 0.02, 0.08),
        (3, Ok("ArrowRight"), at(6.0) + 0.01, 0.5),
        (0, Err(standard::DPAD_LEFT), at(8.0) + 0.01, 0.08),
        (1, Ok("ArrowDown"), at(9.0) - 0.01, 0.08),
        // In the stop: on its beat.
        (2, Ok("ArrowUp"), at(10.0) + 0.2, 0.08),
        (3, Ok("ArrowRight"), at(10.5) + 0.01, 0.08),
    ];
    let mut edges: Vec<(f64, u8, Result<&str, u16>, bool)> = Vec::new();
    for &(lane, control, s, held) in &presses {
        edges.push((s, lane, control, true));
        edges.push((s + held, lane, control, false));
    }
    edges.sort_by(|a, b| a.0.total_cmp(&b.0));
    for (s, _, control, down) in edges {
        match control {
            Ok(code) => r.key(code, down, heard(s)),
            Err(button) => r.pad(button, down, heard(s)),
        }
    }
    assert_eq!(view().record.unwrap().notes, 8);
    assert!(
        r.h.ui.bounds("Record.7").is_some(),
        "tinted while recording"
    );
    // The pad's Back stops on beat 11.
    r.pad(standard::BACK, true, heard(at(11.0)));
    r.h.run(0.1);
    let v = view();
    assert!(!v.listening);
    let rows = |b: f64| Tick((b * 48.0).round() as i64);
    let rec = v.record.clone().unwrap();
    assert_eq!(
        (rec.notes, rec.presses, rec.mode.as_str()),
        (8, 8, "replace")
    );
    assert_eq!(v.cursor, rows(11.0).0);
    assert_eq!(v.notes, 8);
    // Every press's offset as pressed: the mean of them, to the
    // microsecond (the offset taken off once).
    let want = presses
        .iter()
        .zip([4.0, 4.5, 5.0, 6.0, 8.0, 9.0, 10.0, 10.5])
        .map(|(p, b)| p.2 - at(b))
        .sum::<f64>()
        / 8.0
        * 1000.0;
    let mean = rec.mean_ms.unwrap();
    assert!(
        (mean - want).abs() < 0.01,
        "mean {mean} ms, pressed {want} ms"
    );
    let status = env.state.borrow().status.clone();
    assert!(status.starts_with("8 notes recorded · "), "{status}");
    let editor = r.h.ui.layers().last().map(|l| l.name.clone());
    assert_eq!(editor.as_deref(), Some("editor"), "Back stopped, not left");
    // Saved (an `.ssc` beside the `.sm`), read back: on their rows.
    r.h.key_with(
        "KeyS",
        "s",
        Modifiers {
            meta: true,
            ..Modifiers::default()
        },
    );
    r.h.run(0.3);
    let text = std::fs::read_to_string(song.join("song.ssc")).expect("saved");
    let saved = ddi_chart::formats::sm::parse_ssc(&text).unwrap();
    let got: Vec<(Tick, u8, NoteKind)> = saved.charts[0]
        .notes
        .iter()
        .map(|n| (n.tick, n.lane, n.kind))
        .collect();
    assert_eq!(
        got,
        vec![
            (rows(4.0), 0, NoteKind::Tap),
            (rows(4.5), 1, NoteKind::Tap),
            (rows(5.0), 2, NoteKind::Tap),
            (rows(6.0), 3, NoteKind::HoldHead { end: rows(7.0) }),
            (rows(8.0), 0, NoteKind::Tap),
            (rows(9.0), 1, NoteKind::Tap),
            (rows(10.0), 2, NoteKind::Tap),
            (rows(10.5), 3, NoteKind::Tap),
        ]
    );
    ddi_desktop::audio::set_headless_clock(None);
}
