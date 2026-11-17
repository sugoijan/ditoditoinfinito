//! The theme's options drawn offscreen on this machine's GPU, as the
//! desktop window draws it (`--screen options`): the screen asks for the
//! field preview where its panel is, and the window draws the demo chart
//! there in the default skin under the settings edited (Reverse puts the
//! receptors at the panel's bottom). Skipped where there is no GPU
//! adapter (CI). `DDI_OFFSCREEN_PPM=<dir>` keeps the pictures
//! (`options-down.png`, `options-up.png`).

mod common;

use ddi_desktop::gpu::{self, Gpu};
use ddi_engine::appearance::Appearance;
use ddi_render::{FieldGeometry, field_span};
use ddi_ui::UiRequest;
use ddi_ui::state::settings::Settings;

#[test]
fn options_show_the_field_preview() {
    common::indexes();
    let Some(adapter) = common::gpu::adapter_or_skip() else {
        return;
    };
    let (device, queue) = gpu::device(&adapter).expect("device");
    let format = wgpu::TextureFormat::Rgba8Unorm;
    let mut gpu = Gpu::new(device, queue, format);
    let (mut ui, _env) = common::ui::theme("options", common::ui::identity(), [1280, 720], true);
    let mut rect = None;
    for _ in 0..60 {
        ui.update(1.0 / 60.0);
        for r in ui.take_requests() {
            if let UiRequest::FieldPreview(r) = r {
                rect = r;
            }
        }
    }
    let [x, y, w, h] = rect.expect("the screen asks for the preview");
    assert!(w > 100.0 && h > 100.0, "{rect:?}");
    let size = [1280, 720];
    // The settings as Options hands them over (the skin they name, which
    // is Infinito, readied for the direction they say), drawn once that
    // skin is in.
    let picture = |gpu: &mut Gpu, ui: &mut ddi_ui::Ui, settings: &Settings, name: &str| {
        gpu.field.set_rect(rect);
        gpu.field.set_settings(settings);
        let mut frames = 0;
        let pixels = loop {
            let pixels = gpu.draw_offscreen(ui, size, format);
            if gpu.field.ready() && gpu.field.frames() > 0 {
                break pixels;
            }
            frames += 1;
            assert!(frames < 500, "{name}: {:?}", gpu.field.skin_name());
            std::thread::sleep(std::time::Duration::from_millis(5));
        };
        assert_eq!(gpu.field.skin_name().as_deref(), Some("Infinito"));
        common::shot(name, &pixels, size);
        pixels
    };
    // The notes hidden and no background: the receptors and the HUD alone.
    let plain = Settings {
        appearance: Appearance::Stealth,
        bg_brightness: 0.0,
        ..Settings::default()
    };
    let reversed = Settings {
        reverse: true,
        ..plain.clone()
    };
    // Where the receptors are under each, as the renderer places the
    // field in the preview's texture (the panel's size, one to one).
    let layout = ddi_shell::field_preview_options(&plain).layout();
    let geometry =
        |s: &Settings| FieldGeometry::placed(w, h, field_span(&layout), s.reverse, s.field_place);
    let (top, bottom) = (geometry(&plain), geometry(&reversed));
    assert!(top.receptor_y < h / 2.0 && bottom.receptor_y > h / 2.0);
    // Pixels lit in the field's columns, in the receptors' row of `at`
    // (an arrow's height about it): the HUD's score and the watermark,
    // beside the field, are not counted, nor the life gauge above it.
    let lit = |pixels: &[u8], at: &FieldGeometry| {
        let columns = (x + at.left) as u32..(x + at.left + at.arrow * field_span(&layout)) as u32;
        let rows = (y + at.receptor_y - at.arrow / 2.0) as u32
            ..(y + at.receptor_y + at.arrow / 2.0) as u32;
        rows.flat_map(|r| columns.clone().map(move |c| (c, r)))
            .filter(|&(c, r)| {
                let i = ((r * size[0] + c) * 4) as usize;
                pixels[i..i + 3].iter().map(|&v| u32::from(v)).sum::<u32>() > 150
            })
            .count()
    };
    let down = picture(&mut gpu, &mut ui, &plain, "options-down.png");
    let up = picture(&mut gpu, &mut ui, &reversed, "options-up.png");
    let (down_top, down_bottom) = (lit(&down, &top), lit(&down, &bottom));
    let (up_top, up_bottom) = (lit(&up, &top), lit(&up, &bottom));
    eprintln!(
        "Reverse off: top {down_top} bottom {down_bottom}; on: top {up_top} bottom {up_bottom}"
    );
    // Reverse off: the receptors at the top and nothing like them at the
    // bottom; on, the other way round; and each setting's receptors are
    // the other's, moved (the skin may light them from the other side:
    // about as many pixels, not the same).
    let some = (top.arrow * top.arrow / 4.0) as usize;
    assert!(
        down_top > some && down_bottom * 4 < down_top,
        "off: {down_top} {down_bottom}"
    );
    assert!(
        up_bottom > some && up_top * 4 < up_bottom,
        "on: {up_top} {up_bottom}"
    );
    assert!(
        down_top.min(up_bottom) * 2 > down_top.max(up_bottom),
        "the same receptors: {down_top} {up_bottom}"
    );
    assert_eq!(gpu.painter.stats().externals, 1);
    // Away from options: the painter forgets the image, the preview its
    // textures; back, it draws again.
    gpu.field.set_rect(None);
    gpu.draw_offscreen(&mut ui, size, format);
    assert_eq!(gpu.painter.stats().externals, 0);
    assert!(!gpu.field.holds_targets());
    let frames = gpu.field.frames();
    gpu.field.set_rect(rect);
    gpu.draw_offscreen(&mut ui, size, format);
    assert!(gpu.field.holds_targets() && gpu.field.frames() > frames);
    assert_eq!(gpu.painter.stats().externals, 1);
}
