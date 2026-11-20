//! The theme's song select drawn offscreen on this machine's GPU, as the
//! desktop window draws it (`--screen select`): banners and the backdrop
//! decoded from the bundled songs, text in the bundled faces. Skipped
//! where there is no GPU adapter (CI). `DDI_OFFSCREEN_PPM=<dir>` keeps
//! the pictures (`theme.ppm`, `menu.ppm`, `theme-prompt.ppm`).

mod common;

use std::rc::Rc;

use ddi_desktop::gpu::{self, Gpu};

#[test]
fn song_select_draws_in_the_theme() {
    let Some(adapter) = common::gpu::adapter_or_skip() else {
        return;
    };
    let (device, queue) = gpu::device(&adapter).expect("device");
    let format = wgpu::TextureFormat::Rgba8Unorm;
    let mut gpu = Gpu::new(device, queue, format);
    let (mut ui, env) = common::ui::theme("select", common::ui::nfkd(), [1280, 720], true);
    for _ in 0..60 {
        ui.update(1.0 / 60.0);
    }
    let size = [1280, 720];
    let pixels = gpu.draw_offscreen(&mut ui, size, format);
    eprintln!(
        "textures {:?} asked {}",
        gpu.painter.stats(),
        ui.texture_count()
    );
    common::shot("theme.ppm", &pixels, size);
    let at = |x: u32, y: u32| {
        let i = ((y * size[0] + x) * 4) as usize;
        [pixels[i], pixels[i + 1], pixels[i + 2]]
    };
    let close = |a: [u8; 3], b: [u8; 3]| a.iter().zip(b).all(|(x, y)| x.abs_diff(y) <= 3);
    // The selected song's row in the wheel.
    assert!(
        close(at(1050, 405), [0x24, 0x1f, 0x2d]),
        "wheel {:?}",
        at(1050, 405)
    );
    // The ground between them, tinted by the backdrop but dark.
    let g = at(610, 300);
    assert!(g.iter().all(|&c| c < 70), "ground {g:?}");
    // The banner, decoded from the bundled song, in the cover square: not
    // its placeholder.
    let b = at(160, 310);
    assert!(!close(b, [0x1f, 0x1b, 0x28]), "banner {b:?}");
    assert_eq!(env.state.borrow().song, "Some Things Must");
}

/// The actual title menu's small outline atlas is decoded, painted and animated
/// by the shared painter (rather than a browser-only background).
#[test]
fn start_menu_rain_draws_and_freezes() {
    let Some(adapter) = common::gpu::adapter_or_skip() else {
        return;
    };
    let (device, queue) = gpu::device(&adapter).expect("device");
    let format = wgpu::TextureFormat::Rgba8Unorm;
    let mut gpu = Gpu::new(device, queue, format);
    let size = [1280, 720];
    let (mut ui, env) = common::ui::theme("start", common::ui::identity(), [1280, 720], false);
    ui.update(1.0);
    let first = gpu.draw_offscreen(&mut ui, size, format);
    ui.update(1.0);
    let moving = gpu.draw_offscreen(&mut ui, size, format);
    // Away from title, choices and hints: only background rain can move here.
    let changes = (40..600)
        .flat_map(|y| (20..350).map(move |x| (y * 1280 + x) * 4))
        .filter(|&i| first[i..i + 3] != moving[i..i + 3])
        .count();
    assert!(changes > 100, "rain not visible: {changes} changed pixels");
    env.reduced_motion.set(true);
    ui.update(1.0);
    let still = gpu.draw_offscreen(&mut ui, size, format);
    ui.update(1.0);
    assert_eq!(still, gpu.draw_offscreen(&mut ui, size, format));
    common::shot("menu.ppm", &still, [1280, 720]);
}

#[test]
fn countdown_paints_inside_start_without_covering_its_label() {
    use ddi_ui::screens::{
        DESIGN, Env,
        prompt::{PromptInfo, PromptScreen, PromptState},
    };
    use std::cell::RefCell;
    let Some(adapter) = common::gpu::adapter_or_skip() else {
        return;
    };
    let (device, queue) = gpu::device(&adapter).expect("device");
    let format = wgpu::TextureFormat::Rgba8Unorm;
    let mut gpu = Gpu::new(device, queue, format);
    let env = Env::new(Rc::new(ddi_theme::Infinito::new()), Rc::new(str::to_string));
    let state = Rc::new(RefCell::new(PromptState {
        info: PromptInfo {
            title: "Some Things Must".into(),
            artist: "Sevish".into(),
            chart: "Hard 13 · Single".into(),
            ..Default::default()
        },
        countdown: Some(3.0),
        ..Default::default()
    }));
    let mut ui = ddi_ui::Ui::new(DESIGN, ddi_ui::TextMode::Own, false);
    ui.set_focus_ring(false);
    ui.push(PromptScreen::new(env, state, Rc::new(|_, _| {})));
    ui.update(0.5); // Start offered, countdown begins.
    ui.update(1.5); // Halfway filled.
    let pixels = gpu.draw_offscreen(&mut ui, [1280, 720], format);
    let [x0, y0, x1, y1] = ui.bounds("Start").unwrap();
    let sample = |f: f32| {
        let x = (x0 + (x1 - x0) * f) as usize;
        let y = (y0 + (y1 - y0) * 0.2) as usize;
        pixels[(y * 1280 + x) * 4]
    };
    assert!(
        sample(0.25) > sample(0.75) + 20,
        "left half of the button must be filled"
    );
    let mut white_ink = 0;
    for y in ((y0 + (y1 - y0) * 0.3) as usize)..((y0 + (y1 - y0) * 0.7) as usize) {
        for x in ((x0 + (x1 - x0) * 0.25) as usize)..((x0 + (x1 - x0) * 0.49) as usize) {
            let at = (y * 1280 + x) * 4;
            white_ink += usize::from(pixels[at..at + 3].iter().all(|&c| c > 226));
        }
    }
    assert!(
        white_ink > 3,
        "white lettering stays above the filled background"
    );
    assert!(ui.actor("CountdownBar").is_none());
    common::shot("theme-prompt.ppm", &pixels, [1280, 720]);
}
