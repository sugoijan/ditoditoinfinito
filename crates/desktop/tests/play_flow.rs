//! A play as the window runs it (`play::host`), on this machine's GPU
//! offscreen and on a fake clock with no sound device: song select's call
//! brings up the start prompt over it, Enter starts the play once the song
//! is read, the play draws every frame to its end (autoplayed, all top
//! tier), its results come up and it is kept once in the history; R
//! retries (the prompt again), Escape goes back to song select. Skipped
//! where there is no GPU adapter (CI).

mod common;
use std::rc::Rc;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

use ddi_desktop::gpu;
use ddi_desktop::play::Request;
use ddi_desktop::play::host::{Device, Host};
use ddi_desktop::play::view::Formats;
use ddi_platform::HostTime;
use ddi_platform::history::MemoryPlayStore;
use ddi_ui::event::{KeyEvent, UiEvent};
use ddi_ui::state::history::Plays;

const SIZE: [u32; 2] = [960, 540];

fn top(ui: &ddi_ui::Ui) -> String {
    ui.layers()
        .last()
        .map(|l| l.name.clone())
        .unwrap_or_default()
}

fn key(ui: &mut ddi_ui::Ui, code: &str, k: &str) {
    ui.handle(UiEvent::Key(KeyEvent::down(code, k)));
    ui.handle(UiEvent::Key(KeyEvent::up(code, k)));
}

#[test]
fn a_play_from_its_prompt_to_its_results_and_back() {
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
    let store = Rc::new(MemoryPlayStore::default());
    let plays = Plays::new(store.clone());
    let mut host = Host::new(Rc::default(), Some(plays.clone()));
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

    // Song select's call: the prompt, then "Press Start" once read.
    *host.requested.borrow_mut() = Some(Request::song("beatucada", 2, true));
    let mut waited = 0;
    loop {
        host.poll(&mut ui, &env, Some(dev()), 48_000);
        ui.update(0.05);
        if host.prompt_ready() {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(20));
        waited += 1;
        assert!(waited < 1000, "the song was never read");
    }
    ui.update(0.5);
    assert_eq!(top(&ui), "play-prompt");
    // Enter starts it; the prompt goes.
    key(&mut ui, "Enter", "Enter");
    ui.update(0.05);
    host.poll(&mut ui, &env, Some(dev()), 48_000);
    assert!(host.playing(), "{:?}", ui.layers());
    let mut frames = 0;
    while host.playing() {
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
        frames += 1;
        assert!(frames < 60 * 300, "the play never ended");
    }
    let _ = device.poll(wgpu::PollType::wait_indefinitely());
    ui.update(0.05);
    assert_eq!(top(&ui), "results");
    assert_eq!(store.len(), 1, "kept once");
    assert!(
        host.log[0].contains("[77, 0, 0, 0, 0, 0]"),
        "{:?}",
        host.log
    );
    // Retry: the prompt again (the results take keys after a moment).
    ui.update(3.0);
    key(&mut ui, "KeyR", "r");
    ui.update(0.5);
    host.poll(&mut ui, &env, Some(dev()), 48_000);
    ui.update(0.5);
    assert!(host.prompting());
    assert_eq!(top(&ui), "play-prompt");
    // Escape: back to song select, nothing more kept.
    key(&mut ui, "Escape", "Escape");
    ui.update(0.5);
    host.poll(&mut ui, &env, Some(dev()), 48_000);
    ui.update(1.0);
    assert!(!host.prompting() && !host.playing());
    assert_eq!(top(&ui), "select", "{:?}", ui.layers());
    assert_eq!(store.len(), 1);
}
