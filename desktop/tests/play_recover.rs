//! A play after the GPU device was lost (`Host::device_lost`, what the
//! window does with a new device) and after an interruption: the prompt
//! waits for the song's drawing to be made again (its pictures read
//! again: the background shows), and Retry brings the prompt's Start
//! back rather than playing at once. On this machine's GPU offscreen, a
//! fake clock, no sound device. Skipped where there is no GPU adapter.

mod common;
use std::rc::Rc;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

use ddi_desktop::gpu;
use ddi_desktop::play::Request;
use ddi_desktop::play::host::{Device, Host};
use ddi_desktop::play::view::Formats;
use ddi_platform::HostTime;
use ddi_ui::event::{KeyEvent, UiEvent};
use ddi_ui::screens::prompt::PromptStep;

const SIZE: [u32; 2] = [960, 540];

fn key(ui: &mut ddi_ui::Ui, code: &str, k: &str) {
    ui.handle(UiEvent::Key(KeyEvent::down(code, k)));
    ui.handle(UiEvent::Key(KeyEvent::up(code, k)));
    ui.update(0.05);
}

#[test]
fn a_lost_device_and_an_interruption_ready_the_play_again() {
    common::indexes();
    let instance = gpu::instance();
    let Some(adapter) = gpu::adapter(&instance, None) else {
        eprintln!("no GPU adapter: skipped");
        return;
    };
    let (device, queue) = gpu::device(&adapter).unwrap();
    let fold: ddi_ui::dev_widgets::FoldFn = Rc::new(|s: &str| s.to_lowercase());
    let (mut ui, env, _) = ddi_desktop::shell::theme_ui("select", fold);
    ui.handle(UiEvent::Resize {
        size: [SIZE[0] as f32, SIZE[1] as f32],
        ratio: 1.0,
    });
    ui.update(0.5);
    let mut host = Host::new(Rc::default(), None);
    let clock = Arc::new(AtomicU64::new(1_000_000_000));
    let c = clock.clone();
    host.test_clock = Some(Arc::new(move || c.load(Ordering::SeqCst)));
    let formats = Formats::of(wgpu::TextureFormat::Rgba8Unorm, true);
    let dev = || Device {
        device: &device,
        queue: &queue,
        formats,
    };
    let texture = device.create_texture(&wgpu::TextureDescriptor {
        label: None,
        size: wgpu::Extent3d {
            width: SIZE[0],
            height: SIZE[1],
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::Rgba8Unorm,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
        view_formats: &[wgpu::TextureFormat::Rgba8UnormSrgb],
    });
    let srgb = texture.create_view(&wgpu::TextureViewDescriptor {
        format: Some(wgpu::TextureFormat::Rgba8UnormSrgb),
        ..Default::default()
    });
    let plain = texture.create_view(&Default::default());
    let until_ready = |host: &mut Host, ui: &mut ddi_ui::Ui| {
        let mut waited = 0;
        while host.prompt_step() != Some(PromptStep::Ready) {
            host.poll(ui, &env, Some(dev()), 48_000);
            ui.update(0.05);
            std::thread::sleep(std::time::Duration::from_millis(10));
            waited += 1;
            assert!(waited < 1000, "never ready: {:?}", host.prompt_step());
        }
        // The pictures read again too.
        let mut waited = 0;
        while host.backgrounds() == 0 {
            host.poll(ui, &env, Some(dev()), 48_000);
            std::thread::sleep(std::time::Duration::from_millis(10));
            waited += 1;
            assert!(waited < 500, "no background");
        }
        ui.update(0.5);
    };
    *host.requested.borrow_mut() = Some(Request::song("beatucada", 2, true));
    until_ready(&mut host, &mut ui);
    // Lost at the prompt: it loads again before Start is offered.
    host.device_lost();
    assert!(matches!(host.prompt_step(), Some(PromptStep::Loading(_))));
    assert_eq!(host.backgrounds(), 0);
    until_ready(&mut host, &mut ui);
    key(&mut ui, "Enter", "Enter");
    host.poll(&mut ui, &env, Some(dev()), 48_000);
    assert!(host.playing());
    let frame = |host: &mut Host, ui: &mut ddi_ui::Ui| {
        let now = clock.fetch_add(1_000_000_000 / 60, Ordering::SeqCst) + 1_000_000_000 / 60;
        let mut encoder = device.create_command_encoder(&Default::default());
        host.draw_play(
            &dev(),
            &mut encoder,
            &srgb,
            Some(&plain),
            SIZE,
            HostTime(now as f64 / 1e9),
            ui,
            &env,
        );
        queue.submit(Some(encoder.finish()));
    };
    for _ in 0..30 {
        frame(&mut host, &mut ui);
    }
    // Lost in play: stopped as interrupted; Retry readies it again and
    // offers Start, it does not play at once.
    host.device_lost();
    frame(&mut host, &mut ui);
    assert_eq!(host.prompt_step(), Some(PromptStep::Interrupted));
    ui.update(0.5);
    key(&mut ui, "Enter", "Enter");
    host.poll(&mut ui, &env, Some(dev()), 48_000);
    assert!(!host.playing());
    assert!(matches!(host.prompt_step(), Some(PromptStep::Loading(_))));
    until_ready(&mut host, &mut ui);
    key(&mut ui, "Enter", "Enter");
    host.poll(&mut ui, &env, Some(dev()), 48_000);
    assert!(host.playing());
    frame(&mut host, &mut ui);
    assert!(host.playing());
}
