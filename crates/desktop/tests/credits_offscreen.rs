//! The theme's credits page drawn offscreen on this machine's GPU, as the
//! desktop window draws it (`--screen credits`): the shared header with
//! its Back button and the wordmark, and the column's scrollbar. Skipped
//! where there is no GPU adapter (CI). `DDI_OFFSCREEN_PPM=<dir>` keeps
//! the picture (`credits.ppm`).

mod common;


use ddi_desktop::gpu::{self, Gpu};

#[test]
fn the_credits_draw_with_the_header_and_a_scrollbar() {
    let Some(adapter) = common::gpu::adapter_or_skip() else {
        return;
    };
    let (device, queue) = gpu::device(&adapter).expect("device");
    let format = wgpu::TextureFormat::Rgba8Unorm;
    let mut gpu = Gpu::new(device, queue, format);
    let (mut ui, _env) = common::ui::theme("credits", common::ui::nfkd(), [1280, 720], true);
    for _ in 0..60 {
        ui.update(1.0 / 60.0);
    }
    let size = [1280, 720];
    let pixels = gpu.draw_offscreen(&mut ui, size, format);
    common::shot("credits.ppm", &pixels, size);
    // The header's row: the Back button, the wordmark filling the
    // height, the title after it, all inside the header's 64 pixels.
    let back = ui.bounds("Back").expect("Back");
    assert!(back[0] >= 0.0 && back[3] <= 64.0, "Back {back:?}");
    let mark = ui.bounds("Wordmark").expect("Wordmark");
    let title = ui.bounds("Title").expect("Title");
    assert!(
        mark[0] >= back[2],
        "wordmark {mark:?} right of Back {back:?}"
    );
    assert!(
        mark[1] >= 0.0 && mark[3] <= 64.0 && mark[3] - mark[1] > 40.0,
        "wordmark {mark:?}"
    );
    assert!(
        title[0] >= mark[2],
        "title {title:?} after the wordmark {mark:?}"
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
