//! Desktop's first-start offer of other games' song folders, drawn
//! offscreen on this machine's GPU over the theme's song select as the
//! window draws it. Skipped where there is no GPU adapter (CI).
//! `DDI_OFFSCREEN_PPM=<file>` keeps the picture.

use std::cell::RefCell;
use std::rc::Rc;

use ddi_desktop::gpu::{self, Gpu};
use ddi_ui::message::Message;
use ddi_ui::screens::folders_offer::{AnswerFn, FoldersOfferDialog};
use ddi_ui::screens::select::{FEED, SharedFeed, ShellFn};
use ddi_ui::screens::{Env, sample};
use ddi_ui::{Cx, UiEvent};

#[test]
fn the_folders_offer_draws_over_song_select() {
    let instance = gpu::instance();
    let Some(adapter) = gpu::adapter(&instance, None) else {
        eprintln!("no GPU adapter: skipped");
        return;
    };
    let (device, queue) = gpu::device(&adapter).expect("device");
    let format = wgpu::TextureFormat::Rgba8Unorm;
    let mut gpu = Gpu::new(device, queue, format);
    let look = Rc::new(ddi_theme::Infinito::new());
    let env = Env::new(look, Rc::new(|s: &str| s.to_string()));
    env.reduced_motion.set(true);
    let mut ui = ddi_ui::Ui::new(ddi_ui::screens::DESIGN, ddi_ui::TextMode::Own, false);
    ui.set_focus_ring(false);
    let feed = SharedFeed::default();
    let shell: ShellFn = Rc::new(|_, _| {});
    ui.push(sample::select_screen(&env, None).with_shell(shell, feed.clone()));
    ui.handle(UiEvent::Resize {
        size: [1280.0, 720.0],
        ratio: 1.0,
    });
    for _ in 0..30 {
        ui.update(1.0 / 60.0);
    }
    let answers: Rc<RefCell<Vec<bool>>> = Rc::default();
    let a = answers.clone();
    feed.borrow_mut().overlay = Some(Box::new(move |env: &Env, cx: &mut Cx| {
        let on: AnswerFn = Rc::new(move |add, _| a.borrow_mut().push(add));
        cx.overlay(
            FoldersOfferDialog::new(
                env.clone(),
                vec![
                    "/Users/me/Library/Application Support/StepMania 5".into(),
                    "/Applications/ITGmania".into(),
                ],
                on,
            ),
            true,
        );
    }));
    ui.broadcast(Message::new(FEED));
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
    // The screen is dimmed around the dialog's panel.
    let at = |x: u32, y: u32| {
        let i = ((y * size[0] + x) * 4) as usize;
        u32::from(pixels[i]) + u32::from(pixels[i + 1]) + u32::from(pixels[i + 2])
    };
    assert!(
        at(640, 360) > at(20, 700),
        "{} {}",
        at(640, 360),
        at(20, 700)
    );
    assert!(answers.borrow().is_empty());
}
