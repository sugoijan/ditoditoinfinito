//! Add songs' desktop URL tab with the real painter/fonts, without a window,
//! runtime helper, network access or audio device. DDI_OFFSCREEN_PPM=<directory>
//! retains PNGs of parameter entry and both kinds of progress.
use ddi_desktop::gpu::{self, Gpu};
use ddi_ui::screens::{
    COMPACT, DESIGN, Env,
    data::{DanoniImportView, ImportView, Platform},
    select::IMPORT_CHANGED,
    select_dialogs::ImportDialog,
};
use ddi_ui::testing::Harness;
use ddi_ui::{Message, PointerKind, TextMode};
use std::{cell::RefCell, rc::Rc};

#[test]
fn url_import_and_progress_draw_at_desktop_and_compact_sizes() {
    let instance = gpu::instance();
    let Some(adapter) = gpu::adapter(&instance, None) else {
        eprintln!("no GPU adapter: skipped");
        return;
    };
    let (device, queue) = gpu::device(&adapter).expect("device");
    let format = wgpu::TextureFormat::Rgba8Unorm;
    let mut gpu = Gpu::new(device, queue, format);
    for size in [[1280, 720], [844, 390], [390, 844]] {
        let compact = size[0] < 600 || size[1] < 480;
        let env = Env::new(
            Rc::new(ddi_theme::Infinito::new()),
            Rc::new(|s: &str| ddi_library::search::Fold::nfkd(&ddi_desktop::Nfkd, s)),
        );
        env.reduced_motion.set(true);
        let view = Rc::new(RefCell::new(ImportView {
            platform: Platform::Desktop,
            danoni_urls: true,
            danoni: DanoniImportView {
                url: "https://example.invalid/work/".into(),
                ..Default::default()
            },
            ..Default::default()
        }));
        let mut h = Harness::with_gestures(
            if compact { COMPACT } else { DESIGN },
            size.map(|n| n as f32),
            1.0,
            TextMode::Own,
            false,
        );
        h.push(ImportDialog::new(
            env,
            view.clone(),
            Some(Rc::new(|_, _| {})),
        ));
        h.run(0.3);
        h.tap(PointerKind::Mouse, "TabDanoni");
        h.run(0.3);
        for state in ["parameters", "downloading", "converting"] {
            if state != "parameters" {
                let mut data = view.borrow_mut();
                data.running = Some(if state == "downloading" {
                    "Downloading the work…".into()
                } else {
                    "Converting music…".into()
                });
                data.danoni_cancel = true;
                data.danoni.stage = data.running.clone();
                data.danoni.progress = (state == "converting").then_some((2, 5));
                drop(data);
                h.ui.broadcast(Message::new(IMPORT_CHANGED));
                h.run(0.3);
                assert!(h.ui.bounds("DanoniProgress.Activity").is_some());
                assert_eq!(
                    h.ui.bounds("DanoniProgress.Fill").is_some(),
                    state == "converting"
                );
            }
            assert!(h.ui.bounds("DanoniOffline").is_none());
            let pixels = gpu.draw_offscreen(&mut h.ui, size, format);
            let light = pixels
                .as_chunks::<4>()
                .0
                .iter()
                .filter(|p| p[..3].iter().map(|&v| u32::from(v)).sum::<u32>() > 500)
                .count();
            assert!(light > 200, "missing dialog text at {size:?}/{state}");
            if let Some(directory) = std::env::var_os("DDI_OFFSCREEN_PPM") {
                let directory = std::path::PathBuf::from(directory);
                std::fs::create_dir_all(&directory).unwrap();
                image::save_buffer(
                    directory.join(format!("danoni-{}x{}-{state}.png", size[0], size[1])),
                    &pixels,
                    size[0],
                    size[1],
                    image::ColorType::Rgba8,
                )
                .unwrap();
            }
        }
    }
}
