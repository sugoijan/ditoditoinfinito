//! The theme's options drawn offscreen on this machine's GPU, as the
//! desktop window draws it (`--screen options`): the screen asks for the
//! field preview where its panel is, and the window draws the demo chart
//! there, under the options chosen (Reverse puts the receptors at the
//! panel's bottom). Skipped where there is no GPU adapter (CI).
//! `DDI_OFFSCREEN_PPM=<file>` keeps the picture.

use std::rc::Rc;

use ddi_desktop::gpu::{self, Gpu};
use ddi_render::field_preview::FieldPreviewOptions;
use ddi_ui::dev_widgets::FoldFn;
use ddi_ui::{UiEvent, UiRequest};

#[test]
fn options_show_the_field_preview() {
    let instance = gpu::instance();
    let Some(adapter) = gpu::adapter(&instance, None) else {
        eprintln!("no GPU adapter: skipped");
        return;
    };
    let (device, queue) = gpu::device(&adapter).expect("device");
    let format = wgpu::TextureFormat::Rgba8Unorm;
    let mut gpu = Gpu::new(device, queue, format);
    let fold: FoldFn = Rc::new(|s: &str| s.to_string());
    let (mut ui, env, _) = ddi_desktop::shell::theme_ui("options", fold);
    env.reduced_motion.set(true);
    ui.handle(UiEvent::Resize {
        size: [1280.0, 720.0],
        ratio: 1.0,
    });
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
    let picture = |gpu: &mut Gpu, ui: &mut ddi_ui::Ui, options: FieldPreviewOptions| {
        gpu.field.set_rect(rect);
        gpu.field.set_options(options);
        let pixels = gpu.draw_offscreen(ui, size, format);
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
        pixels
    };
    // Light pixels in rows `y0..y1` of the panel (fractions of it).
    let light = |pixels: &[u8], y0: f32, y1: f32| {
        let rows = (y + h * y0) as u32..(y + h * y1) as u32;
        rows.flat_map(|r| (x as u32..(x + w) as u32).map(move |c| (c, r)))
            .filter(|&(c, r)| {
                let i = ((r * size[0] + c) * 4) as usize;
                pixels[i..i + 3].iter().map(|&v| u32::from(v)).sum::<u32>() > 450
            })
            .count()
    };
    let plain = FieldPreviewOptions {
        background: 0.0,
        play: ddi_engine::player::PlayOptions {
            appearance: ddi_engine::appearance::Appearance::Stealth,
            ..Default::default()
        },
        ..FieldPreviewOptions::default()
    };
    let down = picture(&mut gpu, &mut ui, plain.clone());
    assert!(gpu.field.frames() > 0);
    let mut reverse = plain;
    reverse.play.scroll.reverse = true;
    let up = picture(&mut gpu, &mut ui, reverse);
    // The receptors: near the panel's top, then near its bottom.
    let (top, bottom) = (light(&down, 0.08, 0.3), light(&down, 0.7, 0.98));
    assert!(top > 300 && bottom < top / 4, "{top} {bottom}");
    let (top, bottom) = (light(&up, 0.08, 0.3), light(&up, 0.7, 0.98));
    assert!(bottom > 300 && top < bottom / 4, "{top} {bottom}");
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
