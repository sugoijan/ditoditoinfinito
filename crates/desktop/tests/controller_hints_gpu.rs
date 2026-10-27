//! Render controller atlases through the real painter and pre-start skin preview.
mod common;
use ddi_desktop::gpu::{self, Gpu};
use ddi_desktop::play::Request;
use ddi_desktop::play::host::{Device, Host};
use ddi_desktop::play::view::Formats;
use ddi_ui::controller_hints::{self as hints, Style};
use ddi_ui::event::UiEvent;
use std::rc::Rc;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
const SIZE: [u32; 2] = [1280, 720];

fn draw(gpu: &mut Gpu, ui: &mut ddi_ui::Ui) -> Vec<u8> {
    let mut pixels = Vec::new();
    for _ in 0..200 {
        ui.update(0.0);
        for request in ui.take_requests() {
            if let ddi_ui::UiRequest::ReceptorPreview(p) = request {
                gpu.field.set_receptors(p.as_ref());
            }
        }
        pixels = gpu.draw_offscreen(ui, SIZE, wgpu::TextureFormat::Rgba8Unorm);
        if !gpu.loading() && !gpu.images_ready() && gpu.field.ready() {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(25));
    }
    for _ in 0..3 {
        pixels = gpu.draw_offscreen(ui, SIZE, wgpu::TextureFormat::Rgba8Unorm);
    }
    pixels
}
fn save(name: &str, pixels: &[u8]) {
    if let Some(dir) = std::env::var_os("DDI_CONTROLLER_PROOFS") {
        std::fs::create_dir_all(&dir).unwrap();
        image::save_buffer(
            std::path::Path::new(&dir).join(format!("{name}.png")),
            pixels,
            SIZE[0],
            SIZE[1],
            image::ColorType::Rgba8,
        )
        .unwrap();
    }
}
#[test]
fn controller_art_and_pressed_states_reach_the_real_prompt() {
    common::indexes();
    let instance = gpu::instance();
    let Some(adapter) = gpu::adapter(&instance, None) else {
        eprintln!("no GPU adapter: skipped");
        return;
    };
    let (device, queue) = gpu::device(&adapter).unwrap();
    let format = wgpu::TextureFormat::Rgba8Unorm;
    let mut gpu = Gpu::new(device, queue, format);
    let fold: ddi_ui::dev_widgets::FoldFn =
        Rc::new(|s: &str| ddi_library::search::Fold::nfkd(&ddi_desktop::Nfkd, s));
    let (mut ui, env, _) = ddi_desktop::shell::theme_ui("select", fold);
    env.reduced_motion.set(true);
    ui.handle(UiEvent::Resize {
        size: [SIZE[0] as f32, SIZE[1] as f32],
        ratio: 1.0,
    });
    ui.push(ddi_ui::screens::sample::select_screen(&env, None));
    ui.update(0.5);
    let controls = ddi_engine::ControlBindings::default();
    hints::devices(vec![ddi_engine::ConnectedPad {
        id: "Xbox Controller".into(),
        index: 0,
        standard: true,
        vendor_product: None,
    }]);
    hints::pad_press(0);
    for (style, name) in [
        (Style::Xbox, "xbox"),
        (Style::PlayStation, "playstation"),
        (Style::Nintendo, "nintendo"),
        (Style::Generic, "generic"),
    ] {
        hints::configure(
            &controls,
            &std::collections::BTreeMap::from([("Xbox Controller".into(), style)]),
        );
        ui.update(0.3);
        let bounds = ui
            .visible_bounds("Hint0")
            .expect("controller footer visible");
        assert!(
            bounds[1] >= 0.0 && bounds[3] <= SIZE[1] as f32,
            "{bounds:?}"
        );
        save(&format!("select-{name}"), &draw(&mut gpu, &mut ui));
    }
    let mut host = Host::new(Rc::default(), None);
    let clock = Arc::new(AtomicU64::new(1_000_000_000));
    let c = clock.clone();
    host.test_clock = Some(Arc::new(move || c.load(Ordering::SeqCst)));
    *host.requested.borrow_mut() = Some(Request::song("some-things-must", 5, true));
    let mut waited = 0;
    loop {
        let dev = Device {
            device: &gpu.device,
            queue: &gpu.queue,
            formats: Formats::of(format, true),
        };
        host.poll(&mut ui, &env, Some(dev), 48_000);
        ui.update(0.05);
        let s = env.state.borrow();
        if host.prompt_ready() && s.backdrop.is_some() {
            break;
        }
        drop(s);
        std::thread::sleep(std::time::Duration::from_millis(20));
        waited += 1;
        assert!(waited < 1000, "never ready");
    }

    let controls = ddi_engine::ControlBindings::default();
    hints::devices(vec![ddi_engine::ConnectedPad {
        id: "Xbox Controller".into(),
        index: 0,
        standard: true,
        vendor_product: None,
    }]);
    hints::pad_press(0);
    for (style, name, expected) in [
        (Style::Xbox, "xbox", ["LB", "LT", "RT", "RB"]),
        (Style::PlayStation, "playstation", ["L1", "L2", "R2", "R1"]),
        (Style::Nintendo, "nintendo", ["L", "ZL", "ZR", "R"]),
        (Style::Generic, "generic", ["LB", "LT", "RT", "RB"]),
    ] {
        hints::configure(
            &controls,
            &std::collections::BTreeMap::from([("Xbox Controller".into(), style)]),
        );
        ui.update(0.0);
        assert_eq!(
            env.state.borrow().prompt.as_ref().unwrap().bindings,
            expected
        );
        let released = draw(&mut gpu, &mut ui);
        save(&format!("prompt-{name}"), &released);
        // Run the actual demonstration until its first controller cap presses.
        env.reduced_motion.set(false);
        fn pressed(a: &ddi_actor::Actor) -> bool {
            (a.name == "Cap0" && matches!(&a.kind,ddi_actor::Kind::Sprite(s) if s.state()==1))
                || a.children().iter().any(pressed)
        }
        for _ in 0..100 {
            ui.update(0.03);
            let dev = Device {
                device: &gpu.device,
                queue: &gpu.queue,
                formats: Formats::of(format, true),
            };
            host.poll(&mut ui, &env, Some(dev), 48_000);
            let _ = draw(&mut gpu, &mut ui);
            if pressed(ui.actor("BindingKey0").unwrap()) {
                break;
            }
        }
        assert!(
            pressed(ui.actor("BindingKey0").unwrap()),
            "demo must press its first cap"
        );
        let pressed = gpu.draw_offscreen(&mut ui, SIZE, format);
        save(&format!("prompt-{name}-pressed"), &pressed);
        assert_ne!(released, pressed, "pressed atlas must change pixels");
        env.reduced_motion.set(true);
        assert!(ui.problems().is_empty(), "{:?}", ui.problems());
    }
}
