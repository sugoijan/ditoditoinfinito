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
