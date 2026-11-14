//! Render controller atlases through the real painter and pre-start skin preview.
mod common;
use ddi_desktop::gpu::{self, Gpu};
use ddi_desktop::play::Request;
use ddi_desktop::play::host::{Device, Host};
use ddi_desktop::play::view::Formats;
use ddi_ui::controller_hints::{self as hints, Style};
use std::rc::Rc;
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
    common::shot(&format!("{name}.png"), pixels, SIZE);
}
#[test]
fn controller_art_and_pressed_states_reach_the_real_prompt() {
    common::indexes();
    let Some(adapter) = common::gpu::adapter_or_skip() else {
        return;
    };
    let (device, queue) = gpu::device(&adapter).unwrap();
    let format = wgpu::TextureFormat::Rgba8Unorm;
    let mut gpu = Gpu::new(device, queue, format);
    let (mut ui, env) = common::ui::theme("select", common::ui::nfkd(), SIZE, true);
    ui.push(ddi_ui::screens::sample::select_screen(&env, None));
    ui.update(0.5);
    let controls = ddi_engine::ControlBindings::default();
    ui.set_pads(vec![ddi_engine::ConnectedPad {
        id: "Xbox Controller".into(),
        index: 0,
        standard: true,
        vendor_product: None,
    }]);
    ui.set_source(hints::Source::Pad {
        slot: 0,
        id: "Xbox Controller".into(),
    });
    for (style, name) in [
        (Style::Xbox, "xbox"),
        (Style::PlayStation, "playstation"),
        (Style::Nintendo, "nintendo"),
        (Style::Generic, "generic"),
    ] {
        ui.set_controls(
            controls.clone(),
            std::collections::BTreeMap::from([("Xbox Controller".into(), style)]),
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
    common::rig::fake_clock(&mut host);
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
    ui.set_pads(vec![ddi_engine::ConnectedPad {
        id: "Xbox Controller".into(),
        index: 0,
        standard: true,
        vendor_product: None,
    }]);
    ui.set_source(hints::Source::Pad {
        slot: 0,
        id: "Xbox Controller".into(),
    });
    for (style, name, expected) in [
        (Style::Xbox, "xbox", ["LB", "LT", "RT", "RB"]),
        (Style::PlayStation, "playstation", ["L1", "L2", "R2", "R1"]),
        (Style::Nintendo, "nintendo", ["L", "ZL", "ZR", "R"]),
        (Style::Generic, "generic", ["LB", "LT", "RT", "RB"]),
    ] {
        ui.set_controls(
            controls.clone(),
            std::collections::BTreeMap::from([("Xbox Controller".into(), style)]),
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
