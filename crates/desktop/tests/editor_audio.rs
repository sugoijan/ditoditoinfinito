//! The chart editor's audio view on desktop (`docs/plans/editor.md`,
//! "The audio view"), headless: a song of clicks on known beats (128
//! BPM from an offset, 170 from beat 16, a stop at beat 24) read from a
//! song folder, its music analyzed on the editor's thread and drawn
//! offscreen on this machine's GPU (skipped without an adapter): each
//! click's onset is on its beat's grid line in the picture, in the edit
//! view and the play view; listening paints it each frame, and what that
//! costs is printed. `DDI_OFFSCREEN_PPM=<dir>` keeps the pictures.

mod common;
use std::cell::RefCell;
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::sync::Arc;
use std::time::{Duration, Instant};

use ddi_chart::{Tick, TimingMap};
use ddi_desktop::gpu::{self, Gpu};
use ddi_ui::UiEvent;
use ddi_ui::event::KeyEvent;

const RATE: u32 = 48_000;
const BEATS: [i64; 7] = [2, 3, 5, 9, 15, 16, 21];

const SSC: &str = "#VERSION:0.83;\n#TITLE:Clicks;\n#ARTIST:Test;\n#MUSIC:clicks.wav;\n\
#OFFSET:-0.123;\n#BPMS:0=128,16=170;\n#STOPS:24=0.4;\n\
#NOTEDATA:;\n#STEPSTYPE:dance-single;\n#DIFFICULTY:Easy;\n#METER:1;\n#NOTES:\n\
1000\n0000\n0000\n0000\n;\n";

fn key(ui: &mut ddi_ui::Ui, code: &str, k: &str) {
    ui.handle(UiEvent::Key(KeyEvent::down(code, k)));
    ui.handle(UiEvent::Key(KeyEvent::up(code, k)));
}

/// A mono 16-bit WAV: quiet, a short decaying burst at each second of
/// `at`.
fn wav(seconds: f64, at: &[f64]) -> Vec<u8> {
    let n = (seconds * f64::from(RATE)) as usize;
    let mut s = vec![0.0f64; n];
    for &t in at {
        let i0 = (t * f64::from(RATE)).ceil() as usize;
        for i in 0..(RATE / 50) as usize {
            let x = i as f64 / f64::from(RATE);
            s[i0 + i] += (std::f64::consts::TAU * 1500.0 * x + 1.2).sin() * (-x * 300.0).exp();
        }
    }
    let data: Vec<u8> = s
        .iter()
        .flat_map(|v| ((v * 0.9 * 32767.0) as i16).to_le_bytes())
        .collect();
    let mut body = b"WAVE".to_vec();
    body.extend(b"fmt ");
    body.extend(16u32.to_le_bytes());
    body.extend(1u16.to_le_bytes());
    body.extend(1u16.to_le_bytes());
    body.extend(RATE.to_le_bytes());
    body.extend((RATE * 2).to_le_bytes());
    body.extend(2u16.to_le_bytes());
    body.extend(16u16.to_le_bytes());
    body.extend(b"data");
    body.extend((data.len() as u32).to_le_bytes());
    body.extend(&data);
    let mut out = b"RIFF".to_vec();
    out.extend((body.len() as u32).to_le_bytes());
    out.extend(body);
    out
}

fn scratch() -> PathBuf {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../target/scratch/editor-audio");
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(dir.join("songs/Clicks/Clicks")).unwrap();
    dir
}

/// The rows of the picture's waveform where it lights after at least
/// five dark rows, at column `x`, in rows `y0..y1`.
fn onsets(pixels: &[u8], width: u32, x: u32, y0: u32, y1: u32) -> Vec<u32> {
    let lit = |y: u32| {
        let i = ((y * width + x) * 4) as usize;
        let (r, b) = (pixels[i], pixels[i + 2]);
        b > 150 && b > r.saturating_add(40)
    };
    let mut dark = 0;
    let mut out = Vec::new();
    for y in y0..y1 {
        if lit(y) {
            if dark >= 5 {
                out.push(y);
            }
            dark = 0;
        } else {
            dark += 1;
        }
    }
    out
}

#[test]
fn clicks_are_on_their_beats_lines() {
    common::indexes();
    let dir = scratch();
    // SAFETY: set before any thread of this test reads the environment.
    unsafe { std::env::set_var("DDI_HOME", dir.join("home")) };
    ddi_desktop::audio::set_headless(true);
    let instance = gpu::instance();
    let Some(adapter) = gpu::adapter(&instance, None) else {
        eprintln!("no GPU adapter: skipped");
        return;
    };
    let timing: TimingMap = ddi_chart::formats::sm::parse_ssc(SSC).unwrap().timing;
    let at: Vec<f64> = BEATS
        .iter()
        .map(|b| timing.seconds_at(Tick::from_beats(*b)))
        .collect();
    let song = dir.join("songs/Clicks/Clicks");
    std::fs::write(song.join("clicks.ssc"), SSC).unwrap();
    std::fs::write(song.join("clicks.wav"), wav(12.0, &at)).unwrap();
    let songs = ddi_desktop::songs::Songs::new(vec![dir.join("songs")]);
    songs.skip_offer();
    songs.start(Arc::new(|| {}));
    if let Some(l) = songs.library.borrow_mut().as_mut() {
        l.wait();
    }
    let id = ddi_library::pack::song_id("Clicks", "Clicks");
    assert!(ddi_desktop::library::song(&id).is_some(), "in the library");

    let (device, queue) = gpu::device(&adapter).expect("device");
    let format = wgpu::TextureFormat::Rgba8Unorm;
    let mut gpu = Gpu::new(device, queue, format);
    let fold: ddi_ui::dev_widgets::FoldFn = Rc::new(|s: &str| s.to_lowercase());
    let sound = Rc::new(RefCell::new(ddi_desktop::preview::Player::with(
        ddi_desktop::audio::Audio::silent(0.0),
        ddi_desktop::preview::PreviewSong::bundled,
    )));
    let (mut ui, env, _) = ddi_desktop::shell::library_ui("select", fold, sound, &songs);
    env.reduced_motion.set(true);
    ui.handle(UiEvent::Resize {
        size: [1280.0, 720.0],
        ratio: 1.0,
    });
    let screen = ddi_desktop::editor::screen(&env, &songs, &id, 0).expect("editor");
    ui.push(screen);
    let view = || env.state.borrow().editor.clone().expect("the view");
    let started = Instant::now();
    while !view().audio.ready {
        assert!(started.elapsed().as_secs() < 60, "no analysis");
        std::thread::sleep(Duration::from_millis(20));
        ui.update(0.02);
    }
    let made = started.elapsed();
    // Its transition in over, so keys reach it.
    for _ in 0..30 {
        ui.update(1.0 / 60.0);
    }
    let keep = |name: &str, pixels: &[u8]| {
        if let Some(dir) = std::env::var_os("DDI_OFFSCREEN_PPM") {
            let mut ppm = b"P6\n1280 720\n255\n".to_vec();
            ppm.extend(
                pixels
                    .as_chunks::<4>()
                    .0
                    .iter()
                    .flat_map(|p| [p[0], p[1], p[2]]),
            );
            std::fs::write(Path::new(&dir).join(name), ppm).expect("ppm");
        }
    };
    let v = view();
    assert_eq!(v.audio.layout, "column", "{:?}", v.audio);
    let [cx, cy, cw, ch] = v.audio.rect;
    // At the waveform's middle, right of it: lit where it is loud.
    let x = (cx + cw * 0.55 + (cw * 0.45) * 0.8) as u32;
    let rows = (cy as u32 + 2, (cy + ch) as u32 - 2);
    let cursor = |ui: &ddi_ui::Ui| {
        let c = ui.bounds("Cursor").expect("the cursor line");
        (c[1] + c[3]) / 2.0
    };
    // The edit view, then the play view (the player's 1x scroll).
    for (name, scale) in [("edit", 1.0), ("play", 1.0)] {
        if name == "play" {
            key(&mut ui, "KeyV", "v");
            for _ in 0..5 {
                ui.update(1.0 / 60.0);
            }
            assert_eq!(view().view, "play");
        }
        let pixels = gpu.draw_offscreen(&mut ui, [1280, 720], format);
        keep(&format!("editor-audio-{name}.ppm"), &pixels);
        let found = onsets(&pixels, 1280, x, rows.0, rows.1);
        let line = cursor(&ui);
        let beat_h = view().beat_h * scale;
        // Beats 2, 3 and 5 below the cursor at beat 0, at 128 BPM
        // (the play view's rows at the cursor's tempo too).
        for beat in [2.0f32, 3.0, 5.0] {
            let y = line + beat * beat_h;
            if y > rows.1 as f32 - 4.0 {
                continue;
            }
            assert!(
                found.iter().any(|f| (*f as f32 - y).abs() <= 1.5),
                "{name}: beat {beat} at {y}: {found:?}"
            );
        }
        // The grid's beat lines across the column (beats 1 and 4 have no
        // click): a lighter row than those round it.
        for beat in [1.0f32, 4.0] {
            let y = line + beat * beat_h;
            if y > rows.1 as f32 - 8.0 {
                continue;
            }
            let gx = (cx + cw * 0.3) as u32;
            let at = |yy: u32| pixels[((yy * 1280 + gx) * 4) as usize];
            let row = y.round() as u32;
            let light = (row - 1..=row + 1).map(at).max().unwrap();
            assert!(
                light > at(row - 6).max(at(row + 6)) + 15,
                "{name}: no line at {y}"
            );
        }
        // The debug view's onsets are the picture's.
        let debug = view().audio.onsets;
        for f in found.iter().filter(|f| **f < rows.1 - 20) {
            assert!(
                debug.iter().any(|d| (d - *f as f32).abs() <= 1.5),
                "{name}: {f} not in {debug:?}"
            );
        }
    }
    key(&mut ui, "KeyV", "v");
    // Listening: painted each frame; what it costs.
    let painted = view().audio.painted;
    key(&mut ui, "Space", " ");
    let mut costs = Vec::new();
    let started = Instant::now();
    while started.elapsed() < Duration::from_millis(1500) {
        std::thread::sleep(Duration::from_millis(16));
        ui.update(1.0 / 60.0);
        costs.push(view().audio.micros);
    }
    key(&mut ui, "Space", " ");
    ui.update(0.02);
    let v = view();
    assert!(!v.listening);
    assert!(
        v.audio.painted > painted + 40,
        "{} {painted}",
        v.audio.painted
    );
    costs.sort_by(f64::total_cmp);
    let median = costs[costs.len() / 2];
    let worst = costs[costs.len() - 1];
    eprintln!(
        "audio view: analysis made in {made:?}; a {}x{} picture painted in {median:.0} µs \
         (median), {worst:.0} µs at worst, while listening",
        cw as u32, ch as u32
    );
}
