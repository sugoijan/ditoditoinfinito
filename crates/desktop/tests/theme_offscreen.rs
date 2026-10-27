//! The theme's song select drawn offscreen on this machine's GPU, as the
//! desktop window draws it (`--screen select`): banners and the backdrop
//! decoded from the bundled songs, text in the bundled faces. Skipped
//! where there is no GPU adapter (CI). `DDI_OFFSCREEN_PPM=<file>` keeps
//! the picture.

use std::rc::Rc;

use ddi_desktop::gpu::{self, Gpu};
use ddi_ui::UiEvent;
use ddi_ui::dev_widgets::FoldFn;

#[test]
fn song_select_draws_in_the_theme() {
    let instance = gpu::instance();
    let Some(adapter) = gpu::adapter(&instance, None) else {
        eprintln!("no GPU adapter: skipped");
        return;
    };
    let (device, queue) = gpu::device(&adapter).expect("device");
    let format = wgpu::TextureFormat::Rgba8Unorm;
    let mut gpu = Gpu::new(device, queue, format);
    let fold: FoldFn = Rc::new(|s: &str| ddi_library::search::Fold::nfkd(&ddi_desktop::Nfkd, s));
    let (mut ui, env, _) = ddi_desktop::shell::theme_ui("select", fold);
    env.reduced_motion.set(true);
    ui.handle(UiEvent::Resize {
        size: [1280.0, 720.0],
        ratio: 1.0,
    });
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
    if let Some(path) = std::env::var_os("DDI_OFFSCREEN_PPM") {
        let mut ppm = format!("P6\n{} {}\n255\n", size[0], size[1]).into_bytes();
        ppm.extend(
            pixels
                .as_chunks::<4>()
                .0
                .iter()
                .flat_map(|p| [p[0], p[1], p[2]]),
        );
        std::fs::write(path, ppm).expect("ppm");
    }
    let at = |x: u32, y: u32| {
        let i = ((y * size[0] + x) * 4) as usize;
        [pixels[i], pixels[i + 1], pixels[i + 2]]
    };
    let close = |a: [u8; 3], b: [u8; 3]| a.iter().zip(b).all(|(x, y)| x.abs_diff(y) <= 3);
    // The selected chart's plate (Hard, the fourth row) and the selected
    // song's in the wheel.
    assert!(
        close(at(480, 590), [0x22, 0x1e, 0x2b]),
        "ladder {:?}",
        at(480, 590)
    );
    assert!(
        close(at(1050, 405), [0x24, 0x1f, 0x2d]),
        "wheel {:?}",
        at(1050, 405)
    );
    // The ground between them, tinted by the backdrop but dark.
    let g = at(610, 300);
    assert!(g.iter().all(|&c| c < 70), "ground {g:?}");
    // The banner, decoded from the bundled song: not its placeholder.
    let b = at(300, 170);
    assert!(!close(b, [0x1f, 0x1b, 0x28]), "banner {b:?}");
    assert_eq!(env.state.borrow().song, "Some Things Must");
}

/// The actual title menu's small outline atlas is decoded, painted and animated
/// by the shared painter (rather than a browser-only background).
#[test]
fn start_menu_rain_draws_and_freezes() {
    let instance = gpu::instance();
    let Some(adapter) = gpu::adapter(&instance, None) else {
        eprintln!("no GPU adapter: skipped");
        return;
    };
    let (device, queue) = gpu::device(&adapter).expect("device");
    let format = wgpu::TextureFormat::Rgba8Unorm;
    let mut gpu = Gpu::new(device, queue, format);
    let fold: FoldFn = Rc::new(str::to_string);
    let (mut ui, env, _) = ddi_desktop::shell::theme_ui("start", fold);
    let size = [1280, 720];
    ui.handle(UiEvent::Resize {
        size: [1280.0, 720.0],
        ratio: 1.0,
    });
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
    if let Some(path) = std::env::var_os("DDI_MENU_PPM") {
        let mut ppm = b"P6\n1280 720\n255\n".to_vec();
        ppm.extend(
            still
                .as_chunks::<4>()
                .0
                .iter()
                .flat_map(|p| [p[0], p[1], p[2]]),
        );
        std::fs::write(path, ppm).expect("ppm");
    }
}

#[test]
fn countdown_paints_inside_start_without_covering_its_label() {
    use ddi_ui::screens::{
        DESIGN, Env,
        prompt::{PromptInfo, PromptScreen, PromptState},
    };
    use std::cell::RefCell;
    let instance = gpu::instance();
    let Some(adapter) = gpu::adapter(&instance, None) else {
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
    if let Some(path) = std::env::var_os("DDI_PROMPT_PPM") {
        let mut ppm = b"P6\n1280 720\n255\n".to_vec();
        ppm.extend(
            pixels
                .as_chunks::<4>()
                .0
                .iter()
                .flat_map(|p| [p[0], p[1], p[2]]),
        );
        std::fs::write(path, ppm).expect("ppm");
    }
}
