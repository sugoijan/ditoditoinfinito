//! The start prompt never waits for its noteskin (`docs/plans/start-prompt.md`):
//! with the skin's reading slowed down (`DDI_SKIN_DELAY_MS`), Start is
//! offered at once; pressed early it shows the loading screen, whose
//! backdrop settles while the skin is read, and the play begins by itself
//! once it is. Escape leaves the loading screen. On this machine's GPU
//! offscreen and a fake clock with no sound device; skipped where there is
//! no GPU adapter (CI).

mod common;
use std::rc::Rc;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant};

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
}

#[test]
fn start_during_a_slow_skin_shows_the_loading_screen_then_plays() {
    // This binary's own: the play's skin takes 1.5 s longer to read.
    // SAFETY: set before any thread of the test reads the environment.
    unsafe { std::env::set_var("DDI_SKIN_DELAY_MS", "1500") };
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
    let prompt_view = || env.state.borrow().prompt.clone().unwrap_or_default();

    *host.requested.borrow_mut() = Some(Request::song("beatucada", 2, true));
    // Start at once; the skin still reading behind it.
    host.poll(&mut ui, &env, Some(dev()), 48_000);
    ui.update(0.05);
    assert_eq!(host.prompt_step(), Some(PromptStep::Ready));
    assert_eq!(env.state.borrow().page_items, ["Start", "Back"]);
    let asked = Instant::now();
    while !host.prompt_waiting().iter().any(|w| w == "noteskin") {
        host.poll(&mut ui, &env, Some(dev()), 48_000);
        ui.update(0.02);
        std::thread::sleep(Duration::from_millis(10));
        assert!(asked.elapsed() < Duration::from_secs(20), "never read");
    }
    assert!(!host.prompt_ready());
    // Enter: the loading screen, not the play.
    key(&mut ui, "Enter", "Enter");
    host.poll(&mut ui, &env, Some(dev()), 48_000);
    assert!(!host.playing());
    assert!(matches!(host.prompt_step(), Some(PromptStep::Loading(_))));
    ui.update(0.05);
    assert!(prompt_view().loading);
    // Its backdrop settles; "Loading" after the entrance.
    for _ in 0..30 {
        ui.update(1.0 / 60.0);
    }
    assert!(prompt_view().label, "{:?}", prompt_view());
    // The play begins by itself once the skin is read.
    let mut polls = 0;
    while !host.playing() {
        host.poll(&mut ui, &env, Some(dev()), 48_000);
        ui.update(1.0 / 60.0);
        std::thread::sleep(Duration::from_millis(10));
        polls += 1;
        assert!(polls < 2000, "never played: {:?}", host.prompt_waiting());
    }
    assert!(
        asked.elapsed() >= Duration::from_millis(1500),
        "the skin's delay"
    );
    // The prompt went before the play's first frame.
    assert_ne!(
        ui.layers().last().map(|l| l.name.clone()).as_deref(),
        Some("play-prompt")
    );
    for _ in 0..10 {
        let now = clock.fetch_add(1_000_000_000 / 60, Ordering::SeqCst) + 1_000_000_000 / 60;
        let mut encoder = device.create_command_encoder(&Default::default());
        host.draw_play(
            &dev(),
            &mut encoder,
            &srgb,
            Some(&plain),
            SIZE,
            HostTime(now as f64 / 1e9),
            &mut ui,
            &env,
        );
        queue.submit(Some(encoder.finish()));
    }
    assert!(host.playing());
    // Another play's prompt, left from its loading screen.
    drop(host);
    let mut host = Host::new(Rc::default(), None);
    let c = clock.clone();
    host.test_clock = Some(Arc::new(move || c.load(Ordering::SeqCst)));
    *host.requested.borrow_mut() = Some(Request::song("beatucada", 2, true));
    host.poll(&mut ui, &env, Some(dev()), 48_000);
    ui.update(0.05);
    key(&mut ui, "Enter", "Enter");
    host.poll(&mut ui, &env, Some(dev()), 48_000);
    assert!(matches!(host.prompt_step(), Some(PromptStep::Loading(_))));
    key(&mut ui, "Escape", "Escape");
    host.poll(&mut ui, &env, Some(dev()), 48_000);
    assert!(host.idle());
}
