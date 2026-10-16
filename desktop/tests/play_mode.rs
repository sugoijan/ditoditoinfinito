//! The shell's play mode follows its plays (`Shell::sync_play_mode`, run
//! after each poll and each frame): on while a play runs (its keys read,
//! the present mode the play's), off once it ends, in the frame that
//! ends it; the keys pressed between plays never reach the next one's
//! log. On this machine's GPU offscreen, a fake clock, no sound device.
//! Skipped where there is no GPU adapter (CI).

mod common;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

use ddi_desktop::gpu;
use ddi_desktop::play::Request;
use ddi_desktop::play::host::Device;
use ddi_desktop::play::view::Formats;
use ddi_platform::HostTime;
use ddi_ui::event::{KeyEvent, UiEvent};
use ddi_ui::screens::prompt::PromptStep;

const SIZE: [u32; 2] = [640, 360];

#[test]
fn play_mode_ends_with_the_play() {
    common::indexes();
    ddi_desktop::audio::set_headless(true);
    let home = std::env::temp_dir().join(format!("ddi-play-mode-{}", std::process::id()));
    // SAFETY: set before any thread of this test reads the environment.
    unsafe { std::env::set_var("DDI_HOME", &home) };
    let instance = gpu::instance();
    let Some(adapter) = gpu::adapter(&instance, None) else {
        eprintln!("no GPU adapter: skipped");
        return;
    };
    let (device, queue) = gpu::device(&adapter).unwrap();
    let mut shell = ddi_desktop::shell::Shell::named_at("select", Some(0.0));
    let env = shell.theme.clone().expect("the theme").0;
    shell.ui.handle(UiEvent::Resize {
        size: [SIZE[0] as f32, SIZE[1] as f32],
        ratio: 1.0,
    });
    shell.ui.update(0.5);
    let clock = Arc::new(AtomicU64::new(1_000_000_000));
    let c = clock.clone();
    shell.play.test_clock = Some(Arc::new(move || c.load(Ordering::SeqCst)));
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
    let now = || HostTime(clock.load(Ordering::SeqCst) as f64 / 1e9);

    let mut logs = Vec::new();
    for round in 0..2 {
        *shell.play.requested.borrow_mut() = Some(Request::song("beatucada", 2, true));
        let mut waited = 0;
        while shell.play.prompt_step() != Some(PromptStep::Ready) {
            shell.play.poll(&mut shell.ui, &env, Some(dev()), 48_000);
            shell.sync_play_mode();
            shell.ui.update(0.05);
            std::thread::sleep(std::time::Duration::from_millis(10));
            waited += 1;
            assert!(waited < 1000, "never ready");
        }
        shell.ui.update(0.5);
        assert!(!shell.play_mode_on());
        shell
            .ui
            .handle(UiEvent::Key(KeyEvent::down("Enter", "Enter")));
        shell
            .ui
            .handle(UiEvent::Key(KeyEvent::up("Enter", "Enter")));
        shell.play.poll(&mut shell.ui, &env, Some(dev()), 48_000);
        shell.sync_play_mode();
        assert!(shell.play.playing());
        assert!(shell.play_mode_on() && shell.input.keyboard.enabled);
        let mut frames = 0;
        while shell.play.playing() {
            clock.fetch_add(1_000_000_000 / 60, Ordering::SeqCst);
            let mut encoder = device.create_command_encoder(&Default::default());
            shell.play.draw_play(
                &dev(),
                &mut encoder,
                &srgb,
                Some(&plain),
                SIZE,
                now(),
                &mut shell.ui,
                &env,
            );
            queue.submit(Some(encoder.finish()));
            // As the window's redraw does after each frame.
            shell.sync_play_mode();
            frames += 1;
            assert!(frames < 60 * 300, "the play never ended");
        }
        // Off in the frame that ended it: the menus' keys are not
        // queued for a play.
        assert!(!shell.play_mode_on() && !shell.input.keyboard.enabled);
        for code in ["KeyF", "KeyJ", "ArrowLeft", "ArrowRight"] {
            shell.input.keyboard.key(code, true, false, now());
            shell.input.keyboard.key(code, false, false, now());
        }
        let log = shell.play.log.last().cloned().unwrap_or_default();
        assert!(log.contains("[77, 0, 0, 0, 0, 0]"), "round {round}: {log}");
        logs.push(log);
        // Back to song select from the results.
        shell.ui.update(3.0);
        shell
            .ui
            .handle(UiEvent::Key(KeyEvent::down("Escape", "Escape")));
        shell
            .ui
            .handle(UiEvent::Key(KeyEvent::up("Escape", "Escape")));
        shell.ui.update(0.5);
        shell.play.poll(&mut shell.ui, &env, Some(dev()), 48_000);
        shell.sync_play_mode();
        assert!(shell.play.idle(), "round {round}");
    }
    assert_eq!(logs[0], logs[1]);
    let _ = std::fs::remove_dir_all(home);
}
