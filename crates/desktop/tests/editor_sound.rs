//! The chart editor's sound tools on desktop (`docs/plans/editor.md`,
//! step 5), on a bundled song with no sound device: the music's
//! overview made on its thread and drawn as the strip beside the field
//! (offscreen on this machine's GPU, skipped without an adapter), the
//! metronome on while listening on the headless clock, and a tap on the
//! strip moving the cursor. `DDI_OFFSCREEN_PPM=<dir>` keeps the picture.

mod common;
use std::rc::Rc;
use std::time::{Duration, Instant};

use ddi_desktop::gpu::{self, Gpu};
use ddi_ui::UiEvent;
use ddi_ui::event::{KeyEvent, PointerEvent, PointerKind, PointerPhase};

fn key(ui: &mut ddi_ui::Ui, code: &str, k: &str) {
    ui.handle(UiEvent::Key(KeyEvent::down(code, k)));
    ui.handle(UiEvent::Key(KeyEvent::up(code, k)));
}

/// A click at a virtual point (CSS pixels at a ratio of 1).
fn tap(ui: &mut ddi_ui::Ui, at: [f32; 2], time: f64) {
    let at = ui.viewport().to_physical(at);
    for (phase, t) in [(PointerPhase::Down, time), (PointerPhase::Up, time + 80.0)] {
        ui.handle(UiEvent::Pointer(PointerEvent {
            id: 1,
            kind: PointerKind::Mouse,
            phase,
            pos: at,
            buttons: if phase == PointerPhase::Down { 1 } else { 0 },
            modifiers: Default::default(),
            time: t,
        }));
    }
}

#[test]
fn the_overview_and_the_metronome_on_a_bundled_song() {
    common::indexes();
    let home = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../target/scratch/editor-sound/home");
    // SAFETY: set before any thread of this test reads the environment.
    unsafe { std::env::set_var("DDI_HOME", &home) };
    ddi_desktop::audio::set_headless(true);
    let instance = gpu::instance();
    let Some(adapter) = gpu::adapter(&instance, None) else {
        eprintln!("no GPU adapter: skipped");
        return;
    };
    let (device, queue) = gpu::device(&adapter).expect("device");
    let format = wgpu::TextureFormat::Rgba8Unorm;
    let mut gpu = Gpu::new(device, queue, format);
    let songs = ddi_desktop::songs::Songs::new(Vec::new());
    songs.skip_offer();
    let fold: ddi_ui::dev_widgets::FoldFn = Rc::new(|s: &str| s.to_lowercase());
    let sound = Rc::new(std::cell::RefCell::new(ddi_desktop::preview::Player::with(
        ddi_desktop::audio::Audio::silent(0.0),
        ddi_desktop::preview::PreviewSong::bundled,
    )));
    let (mut ui, env, _) = ddi_desktop::shell::library_ui("select", fold, sound, &songs);
    env.reduced_motion.set(true);
    ui.handle(UiEvent::Resize {
        size: [1280.0, 720.0],
        ratio: 1.0,
    });
    let screen = ddi_desktop::editor::screen(&env, &songs, "beatucada", 1).expect("editor");
    ui.push(screen);
    let view = || env.state.borrow().editor.clone().expect("the view");
    // The overview, once the music is decoded and measured.
    let started = Instant::now();
    while view().overview == 0 {
        assert!(started.elapsed().as_secs() < 60, "no overview");
        std::thread::sleep(Duration::from_millis(20));
        ui.update(0.02);
    }
    assert!(view().overview > 1000, "{}", view().overview);
    let pixels = gpu.draw_offscreen(&mut ui, [1280, 720], format);
    if let Some(dir) = std::env::var_os("DDI_OFFSCREEN_PPM") {
        let mut ppm = b"P6\n1280 720\n255\n".to_vec();
        ppm.extend(
            pixels
                .as_chunks::<4>()
                .0
                .iter()
                .flat_map(|p| [p[0], p[1], p[2]]),
        );
        std::fs::write(std::path::Path::new(&dir).join("editor-sound.ppm"), ppm).expect("ppm");
    }
    // The strip's bars: lit pixels down its middle.
    let strip = ui.bounds("OverviewPlate").expect("the strip");
    let x = ((strip[0] + strip[2]) / 2.0) as u32;
    let lit = (strip[1] as u32..strip[3] as u32)
        .filter(|&y| {
            let i = ((y * 1280 + x) * 4) as usize;
            pixels[i..i + 3].iter().any(|&c| c > 90)
        })
        .count();
    assert!(lit > 100, "the music's loudness drawn: {lit}");
    // The metronome on, listening from the start on the headless clock.
    key(&mut ui, "KeyK", "k");
    assert!(view().metronome);
    key(&mut ui, "Space", " ");
    let started = Instant::now();
    while view().shown < 96.0 {
        assert!(started.elapsed().as_secs() < 60, "never played");
        std::thread::sleep(Duration::from_millis(20));
        ui.update(0.02);
    }
    key(&mut ui, "Space", " ");
    ui.update(0.02);
    assert!(!view().listening);
    // A tap on the strip's lower half: the cursor after the middle.
    let at = [
        (strip[0] + strip[2]) / 2.0,
        strip[1] + (strip[3] - strip[1]) * 0.75,
    ];
    tap(&mut ui, at, 100_000.0);
    ui.update(0.02);
    let v = view();
    assert!(v.cursor > 48 * 32, "{v:?}");
}
