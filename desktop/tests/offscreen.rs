//! The developer screen drawn offscreen on this machine's GPU (Metal,
//! Vulkan or DX12) by the desktop shell's painter, against what the web
//! draws: pixels sampled from a 1280×720 screenshot of `#/ui-dev` in
//! headless Chrome (WebGPU, device pixel ratio 1) on 2026-10-15, away
//! from text, whose fonts differ (the browser's against cosmic-text's).
//! Skipped where there is no GPU adapter (CI).

use ddi_desktop::gpu::{self, Gpu};
use ddi_ui::dev::{self, DevScreen, Shared};
use ddi_ui::{TextMode, Ui, UiEvent};

/// `(x, y)` in the window and the web's colour there.
const WEB: [((u32, u32), [u8; 3]); 7] = [
    // The background.
    ((5, 700), [12, 12, 18]),
    ((1260, 10), [12, 12, 18]),
    // Beep, focused at the start; Link, not.
    ((60, 135), [64, 92, 191]),
    ((300, 135), [46, 48, 61]),
    // The field's box.
    ((60, 380), [26, 26, 33]),
    // The strip's first two rows.
    ((820, 128), [33, 33, 43]),
    ((820, 190), [41, 41, 51]),
];

#[test]
fn the_developer_screen_draws_as_the_web_does() {
    let instance = gpu::instance();
    let Some(adapter) = gpu::adapter(&instance, None) else {
        // CI runners have none.
        eprintln!("no GPU adapter: skipped");
        return;
    };
    let (device, queue) = gpu::device(&adapter).expect("device");
    let format = wgpu::TextureFormat::Rgba8Unorm;
    let mut gpu = Gpu::new(device, queue, format);
    let state = Shared::default();
    let mut ui = Ui::new(dev::DESIGN, TextMode::Own, false);
    ui.push(DevScreen::new(state));
    ui.handle(UiEvent::Resize {
        size: [1280.0, 720.0],
        ratio: 1.0,
    });
    ui.update(1.0 / 60.0);
    let size = [1280, 720];
    let pixels = gpu.draw_offscreen(&mut ui, size, format);
    assert_eq!(pixels.len(), 1280 * 720 * 4);
    // `DDI_OFFSCREEN_PPM=<file>` keeps the picture, to look at.
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
    for ((x, y), web) in WEB {
        let got = at(x, y);
        let close = got.iter().zip(web).all(|(a, b)| a.abs_diff(b) <= 2);
        assert!(close, "({x}, {y}): {got:?}, the web drew {web:?}");
    }
    // Text was drawn: the title's light pixels.
    let mut lit = 0;
    for y in 37..80 {
        for x in 53..465 {
            if at(x, y).iter().all(|&c| c > 200) {
                lit += 1;
            }
        }
    }
    assert!(lit > 500, "title pixels: {lit}");
    // And measured about as the browser does (412 CSS pixels in Chrome
    // with the system's sans-serif; fonts differ by platform).
    let b = ui.bounds("Title").expect("title");
    let width = (b[2] - b[0]) * 1280.0 / dev::DESIGN[0];
    assert!(
        (width - 412.0).abs() < 412.0 * 0.15,
        "title {width} px wide"
    );
}
