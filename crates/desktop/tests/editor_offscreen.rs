//! The chart editor drawn offscreen on this machine's GPU in the theme,
//! on a bundled song (`docs/plans/editor.md`): opened from song select's
//! E on the library, its notes on the field, a note placed by a key and
//! drawn, the menu opened by Tab over it. Skipped where there is no GPU
//! adapter (CI). `DDI_OFFSCREEN_PPM=<dir>` keeps the pictures.

mod common;
use std::rc::Rc;

use ddi_desktop::gpu::{self, Gpu};
use ddi_ui::UiEvent;
use ddi_ui::event::KeyEvent;

fn key(ui: &mut ddi_ui::Ui, code: &str, k: &str) {
    ui.handle(UiEvent::Key(KeyEvent::down(code, k)));
    ui.handle(UiEvent::Key(KeyEvent::up(code, k)));
}

#[test]
fn the_editor_draws_a_bundled_song() {
    common::indexes();
    let home = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../target/scratch/editor-offscreen/home");
    // SAFETY: set before any thread of this test reads the environment.
    unsafe { std::env::set_var("DDI_HOME", &home) };
    // No sound device: the editor's listening runs on the host clock.
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
    for _ in 0..30 {
        ui.update(1.0 / 60.0);
    }
    // Song select's E, as its shell hears it: the editor over it.
    let screen = ddi_desktop::editor::screen(&env, &songs, "beatucada", 1).expect("editor");
    ui.push(screen);
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
            std::fs::write(std::path::Path::new(&dir).join(name), ppm).expect("ppm");
        }
    };
    let view = env
        .state
        .borrow()
        .editor
        .clone()
        .expect("the editor's view");
    assert_eq!(view.song, "Beatucada");
    assert!(view.notes > 10, "{view:?}");
    assert!(!view.dirty);
    // Bundled: no saving in place, a copy only.
    assert!(ui.actor("Button.Save").is_none(), "no Save button");
    assert!(ui.actor("Button.Play").is_some());
    // Down two measures, a note in the first lane.
    key(&mut ui, "PageDown", "PageDown");
    key(&mut ui, "PageDown", "PageDown");
    let before = env.state.borrow().editor.clone().unwrap().notes;
    key(&mut ui, "Digit1", "1");
    for _ in 0..10 {
        ui.update(1.0 / 60.0);
    }
    let after = env.state.borrow().editor.clone().unwrap();
    assert_eq!(after.cursor, 384);
    assert_ne!(after.notes, before, "a note placed or taken away");
    assert!(after.dirty);
    let pixels = gpu.draw_offscreen(&mut ui, [1280, 720], format);
    keep("editor.ppm", &pixels);
    let at = |x: u32, y: u32| {
        let i = ((y * 1280 + x) * 4) as usize;
        [pixels[i], pixels[i + 1], pixels[i + 2]]
    };
    // Something drawn on the field (notes, lines) and the ground dark
    // (the lanes right of the audio view's column).
    let lit = (100..700)
        .step_by(4)
        .flat_map(|y| (60..700).step_by(4).map(move |x| (x, y)))
        .filter(|&(x, y)| at(x, y).iter().any(|&c| c > 150))
        .count();
    assert!(lit > 50, "the field is drawn: {lit}");
    key(&mut ui, "Tab", "Tab");
    for _ in 0..20 {
        ui.update(1.0 / 60.0);
    }
    let pixels = gpu.draw_offscreen(&mut ui, [1280, 720], format);
    keep("editor-menu.ppm", &pixels);
    assert_eq!(ui.layers().last().unwrap().name, "editor-actions");
    // Listening (the music decoded on its thread since the editor
    // opened): the field follows the second heard, then stops on the
    // grid.
    key(&mut ui, "Escape", "Escape");
    for _ in 0..20 {
        ui.update(1.0 / 60.0);
    }
    key(&mut ui, "Space", " ");
    let view = || env.state.borrow().editor.clone().unwrap();
    assert!(view().listening);
    let started = std::time::Instant::now();
    while view().shown < 384.0 + 48.0 {
        assert!(
            started.elapsed().as_secs() < 60,
            "the music never played: {:?}",
            view()
        );
        std::thread::sleep(std::time::Duration::from_millis(20));
        ui.update(0.02);
    }
    key(&mut ui, "Space", " ");
    ui.update(0.02);
    let v = view();
    assert!(
        !v.listening && v.cursor > 384 && v.cursor % 12 == 0,
        "{v:?}"
    );
}
