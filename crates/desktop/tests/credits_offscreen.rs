//! The theme's credits page drawn offscreen on this machine's GPU, as the
//! desktop window draws it (`--screen credits`): the shared header with
//! its Back button and the wordmark, and the column's scrollbar. Skipped
//! where there is no GPU adapter (CI). `DDI_OFFSCREEN_PPM=<file>` keeps
//! the picture.

use std::rc::Rc;

use ddi_desktop::gpu::{self, Gpu};
use ddi_ui::UiEvent;
use ddi_ui::dev_widgets::FoldFn;

#[test]
fn the_credits_draw_with_the_header_and_a_scrollbar() {
    let instance = gpu::instance();
    let Some(adapter) = gpu::adapter(&instance, None) else {
        eprintln!("no GPU adapter: skipped");
        return;
    };
    let (device, queue) = gpu::device(&adapter).expect("device");
    let format = wgpu::TextureFormat::Rgba8Unorm;
    let mut gpu = Gpu::new(device, queue, format);
    let fold: FoldFn = Rc::new(|s: &str| ddi_library::search::Fold::nfkd(&ddi_desktop::Nfkd, s));
    let (mut ui, env, _) = ddi_desktop::shell::theme_ui("credits", fold);
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
    // The header: the Back button at the left, the wordmark over the
    // title, both inside the header's 64 pixels.
    let back = ui.bounds("Back").expect("Back");
    assert!(back[0] >= 0.0 && back[3] <= 64.0, "Back {back:?}");
    let mark = ui.bounds("Wordmark").expect("Wordmark");
    let title = ui.bounds("Title").expect("Title");
    assert!(
        mark[0] >= back[2],
        "wordmark {mark:?} right of Back {back:?}"
    );
    assert!(
        mark[3] <= title[1],
        "wordmark {mark:?} over the title {title:?}"
    );
    assert!(title[3] <= 64.0, "title {title:?}");
    // The credits are longer than the screen: the scrollbar shows, beside
    // the column, its thumb at the top and shorter than its track.
    let column = ui.bounds("Column").expect("Column");
    let bar = ui.bounds("Column.Bar").expect("Column.Bar");
    let thumb = ui.bounds("Thumb").expect("Thumb");
    assert!(
        bar[0] >= column[2],
        "bar {bar:?} beside the column {column:?}"
    );
    assert!(
        (bar[1] - column[1]).abs() < 1.0,
        "bar {bar:?} at the column's top"
    );
    assert!(
        thumb[1] - bar[1] < 1.0,
        "thumb {thumb:?} at the top of {bar:?}"
    );
    assert!(
        thumb[3] - thumb[1] < (bar[3] - bar[1]) / 2.0,
        "thumb {thumb:?} shorter than {bar:?}"
    );
}
