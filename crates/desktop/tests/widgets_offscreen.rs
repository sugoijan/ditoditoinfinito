//! The widget screen drawn offscreen on this machine's GPU, as the
//! desktop window draws it: the list, the toggle, the panels. Images
//! fail on desktop for now (no loader), so rows show their placeholders.
//! Skipped where there is no GPU adapter (CI).

mod common;

use std::rc::Rc;

use ddi_desktop::gpu::{self, Gpu};
use ddi_ui::dev::{self, DevScreen, Shared};
use ddi_ui::dev_widgets::{FoldFn, SharedWidgets, WidgetScreen};
use ddi_ui::{TextMode, Ui, UiEvent};

#[test]
fn the_widget_screen_draws() {
    let Some(adapter) = common::gpu::adapter_or_skip() else {
        return;
    };
    let (device, queue) = gpu::device(&adapter).expect("device");
    let format = wgpu::TextureFormat::Rgba8Unorm;
    let mut gpu = Gpu::new(device, queue, format);
    let fold: FoldFn = Rc::new(|s: &str| ddi_library::search::Fold::nfkd(&ddi_desktop::Nfkd, s));
    let widgets = SharedWidgets::default();
    let mut ui = Ui::new(dev::DESIGN, TextMode::Own, false);
    ui.push(DevScreen::with_widgets(
        Shared::default(),
        widgets.clone(),
        fold.clone(),
    ));
    ui.push(WidgetScreen::new(widgets.clone(), fold));
    ui.handle(UiEvent::Resize {
        size: [1280.0, 720.0],
        ratio: 1.0,
    });
    for _ in 0..30 {
        ui.update(1.0 / 60.0);
    }
    let size = [1280, 720];
    let pixels = gpu.draw_offscreen(&mut ui, size, format);
    common::shot("widgets.ppm", &pixels, size);
    let at = |x: u32, y: u32| {
        let i = ((y * size[0] + x) * 4) as usize;
        [pixels[i], pixels[i + 1], pixels[i + 2]]
    };
    let close = |a: [u8; 3], b: [u8; 3]| a.iter().zip(b).all(|(x, y)| x.abs_diff(y) <= 3);
    // The plain look's colors (`look.rs`), 1280/960 window pixels a
    // virtual one: the screen's ground, the first pack's header
    // (selected), a row's plate, its banner's placeholder, the toggle's
    // plate, a panel.
    assert!(
        close(at(1270, 715), [12, 12, 18]),
        "ground {:?}",
        at(1270, 715)
    );
    assert!(close(at(700, 80), [12, 12, 18]), "ground {:?}", at(700, 80));
    assert!(close(at(30, 30), [41, 56, 115]), "header {:?}", at(30, 30));
    assert!(close(at(500, 90), [28, 31, 38]), "row {:?}", at(500, 90));
    assert!(
        close(at(40, 90), [46, 48, 59]),
        "placeholder {:?}",
        at(40, 90)
    );
    assert!(
        close(at(568, 104), [28, 31, 38]),
        "toggle {:?}",
        at(568, 104)
    );
    assert!(
        close(at(568, 384), [18, 19, 23]),
        "panel {:?}",
        at(568, 384)
    );
    assert_eq!(widgets.borrow().items, 200 * 51);
    assert!(widgets.borrow().pool <= 16, "{}", widgets.borrow().pool);
}
