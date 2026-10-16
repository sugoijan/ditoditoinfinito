//! Calibration as the window runs it (`docs/plans/desktop.md`, 7.6), on
//! this machine's GPU offscreen and on a fake clock with no sound device:
//! calibration's menu starts the arrows-only test (autoplayed 30 ms late,
//! as `--auto --bias 30` asks), its prompt, the play to its end, its
//! outcome on the prompt's card (the web's); saving moves the visual
//! offset by what was measured,
//! "next" brings the sound-only test's prompt, and Escape there goes
//! back to the menu. Skipped where there is no GPU adapter (CI).

use std::rc::Rc;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

use ddi_desktop::gpu;
use ddi_desktop::play::host::{Device, Host};
use ddi_desktop::play::view::Formats;
use ddi_platform::HostTime;
use ddi_ui::event::{KeyEvent, UiEvent};
use ddi_ui::screens::Env;
use ddi_ui::screens::prompt::PromptStep;
use ddi_ui::state::settings::Settings;

const SIZE: [u32; 2] = [960, 540];

fn top(ui: &ddi_ui::Ui) -> String {
    ui.layers()
        .iter()
        .rev()
        .find(|l| !l.overlay)
        .map(|l| l.name.clone())
        .unwrap_or_default()
}

fn key(ui: &mut ddi_ui::Ui, code: &str, k: &str) {
    ui.handle(UiEvent::Key(KeyEvent::down(code, k)));
    ui.handle(UiEvent::Key(KeyEvent::up(code, k)));
    ui.update(0.05);
}

/// Presses the screen's item `want` (a page's `button:<id>`, the
/// prompt's `Action.<id>`) from the keys.
fn press(ui: &mut ddi_ui::Ui, env: &Env, want: &str) {
    for _ in 0..40 {
        let (items, focus) = {
            let s = env.state.borrow();
            (s.page_items.clone(), s.page_focus)
        };
        let Some(at) = items.iter().position(|i| *i == want) else {
            panic!("no {want} in {items:?}");
        };
        if focus == at as i32 {
            key(ui, "Enter", "Enter");
            return;
        }
        if focus < at as i32 {
            key(ui, "ArrowDown", "ArrowDown");
        } else {
            key(ui, "ArrowUp", "ArrowUp");
        }
    }
    panic!("{want} never focused");
}

#[test]
fn a_test_from_the_menu_to_its_result_and_the_next() {
    let home = std::env::temp_dir().join(format!("ddi-calibration-flow-{}", std::process::id()));
    // SAFETY: set before any thread of this test reads the environment.
    unsafe { std::env::set_var("DDI_HOME", &home) };
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
    let songs = ddi_desktop::songs::Songs::new(Vec::new());
    songs.testing.set((true, 0.030));
    let mut host = Host::new(songs.play.clone(), None);
    {
        let songs = songs.clone();
        host.menu = Some(Rc::new(move |env: &Env, ui: &mut ddi_ui::Ui| {
            ui.push(ddi_desktop::calibrate::menu(env, &songs, None))
        }));
    }
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

    // The menu; its first button starts the arrows-only test.
    ui.push(ddi_desktop::calibrate::menu(&env, &songs, None));
    ui.update(0.5);
    assert_eq!(top(&ui), "calibration");
    press(&mut ui, &env, "button:run:visual");
    // Its prompt, ready once its skin is read.
    let mut waited = 0;
    while host.prompt_step() != Some(PromptStep::Ready) {
        host.poll(&mut ui, &env, Some(dev()), 48_000);
        ui.update(0.05);
        std::thread::sleep(std::time::Duration::from_millis(10));
        waited += 1;
        assert!(waited < 1000, "{:?}", host.prompt_step());
    }
    ui.update(0.5);
    assert_eq!(top(&ui), "play-prompt", "{:?}", ui.layers());
    key(&mut ui, "Enter", "Enter");
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
        assert!(frames < 60 * 120, "the test never ended");
    }
    let _ = device.poll(wgpu::PollType::wait_indefinitely());
    ui.update(0.5);
    // Its outcome on the prompt's card, as the web shows it.
    assert_eq!(top(&ui), "play-prompt", "{:?}", ui.layers());
    assert!(
        matches!(&host.prompt_step(), Some(PromptStep::Card { heading, .. }) if heading.starts_with("+30")),
        "{:?}",
        host.prompt_step()
    );
    assert!(
        host.log
            .iter()
            .any(|l| l.starts_with("calibration visual: +30")),
        "{:?}",
        host.log
    );
    // Saving moves the visual offset (no devices: the default's) by
    // what the late autoplayer measured.
    press(&mut ui, &env, "Action.save");
    host.poll(&mut ui, &env, Some(dev()), 48_000);
    ui.update(0.5);
    let visual = Settings::load().visual_offset;
    assert!((visual - 0.030).abs() < 0.004, "{visual}");
    assert!(
        env.state
            .borrow()
            .page_items
            .contains(&"Action.next".to_string())
    );
    // Next: the sound-only test's prompt.
    press(&mut ui, &env, "Action.next");
    host.poll(&mut ui, &env, Some(dev()), 48_000);
    ui.update(0.5);
    host.poll(&mut ui, &env, Some(dev()), 48_000);
    assert!(host.prompting());
    assert!(env.state.borrow().song.starts_with("Calibration 2/3"));
    assert_eq!(top(&ui), "play-prompt", "{:?}", ui.layers());
    // Escape: back to the menu.
    key(&mut ui, "Escape", "Escape");
    ui.update(0.5);
    host.poll(&mut ui, &env, Some(dev()), 48_000);
    ui.update(0.5);
    assert!(!host.prompting() && !host.playing());
    assert_eq!(top(&ui), "calibration", "{:?}", ui.layers());
    let _ = std::fs::remove_dir_all(home);
}
