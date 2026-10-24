//! The start prompt's play options (`docs/plans/start-prompt.md`): changed
//! on the prompt they are this play's (its skin read again for Reverse,
//! the record keeping them) and its retry's, never the stored settings';
//! the next song's prompt starts from those; Save as default stores them.
//! On this machine's GPU offscreen and a fake clock with no sound device;
//! skipped where there is no GPU adapter (CI).

mod common;
use std::cell::RefCell;
use std::rc::Rc;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

use ddi_desktop::gpu;
use ddi_desktop::play::Request;
use ddi_desktop::play::host::{Device, Host};
use ddi_desktop::play::view::Formats;
use ddi_platform::HostTime;
use ddi_platform::history::{MemoryPlayStore, PlayStore};
use ddi_ui::event::{KeyEvent, UiEvent};
use ddi_ui::state::history::Plays;
use ddi_ui::state::settings::Settings;

const SIZE: [u32; 2] = [960, 540];

fn key(ui: &mut ddi_ui::Ui, code: &str, k: &str) {
    ui.handle(UiEvent::Key(KeyEvent::down(code, k)));
    ui.handle(UiEvent::Key(KeyEvent::up(code, k)));
}

#[test]
fn play_options_are_this_plays_kept_with_it_and_saved_when_asked() {
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
    let options = |env: &ddi_ui::screens::Env| {
        env.state
            .borrow()
            .prompt
            .as_ref()
            .map(|p| p.options.clone())
            .unwrap_or_default()
    };
    let until_ready = |host: &mut Host, ui: &mut ddi_ui::Ui| {
        let mut polls = 0;
        while !host.prompt_ready() || options(&env).len() < 6 {
            host.poll(ui, &env, Some(dev()), 48_000);
            ui.update(0.02);
            std::thread::sleep(Duration::from_millis(10));
            polls += 1;
            assert!(polls < 2000, "never ready: {:?}", host.prompt_waiting());
        }
        ui.update(0.3);
    };
    let stored = Settings::load();
    assert!(!stored.reverse);

    *host.requested.borrow_mut() = Some(Request::song("beatucada", 2, true));
    until_ready(&mut host, &mut ui);
    assert_eq!(options(&env)[1], "reverse=Off");
    // Up from Start to the last row, then up to Reverse; Right turns it
    // on; once more up, Speed one step faster.
    for _ in 0..5 {
        key(&mut ui, "ArrowUp", "ArrowUp");
    }
    assert_eq!(ui.focused(), Some("Opt.reverse"));
    key(&mut ui, "ArrowRight", "ArrowRight");
    key(&mut ui, "ArrowUp", "ArrowUp");
    key(&mut ui, "ArrowRight", "ArrowRight");
    ui.update(0.05);
    assert_eq!(options(&env)[..2], ["speed=×2.75", "reverse=On"]);
    // The skin is read again (its Lua told the new direction); the
    // summary says them; the settings are as they were.
    host.poll(&mut ui, &env, Some(dev()), 48_000);
    assert!(
        host.prompt_waiting().iter().any(|w| w == "noteskin"),
        "{:?}",
        host.prompt_waiting()
    );
    assert_eq!(Settings::load().speed, stored.speed);
    assert!(!Settings::load().reverse);
    until_ready(&mut host, &mut ui);
    // Enter starts (from the row: Start is what Enter does).
    key(&mut ui, "Enter", "Enter");
    host.poll(&mut ui, &env, Some(dev()), 48_000);
    let mut polls = 0;
    while !host.playing() {
        host.poll(&mut ui, &env, Some(dev()), 48_000);
        ui.update(0.02);
        polls += 1;
        assert!(polls < 2000, "never played");
    }
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
    // Kept as played: the engine's options and the settings' snapshot.
    let records: Rc<RefCell<Vec<serde_json::Value>>> = Rc::default();
    let r = records.clone();
    store.export(
        10,
        Box::new(move |v| {
            r.borrow_mut().extend(
                v.into_iter()
                    .map(|e| serde_json::from_str(&e.record).unwrap()),
            )
        }),
        Box::new(|_| {}),
    );
    let records = records.borrow();
    assert_eq!(records.len(), 1);
    let rec = &records[0];
    assert_eq!(rec["settings"]["reverse"], true, "{}", rec["settings"]);
    assert_eq!(rec["settings"]["speed"], 2.75);
    assert_eq!(
        rec["options"]["scroll"]["reverse"], true,
        "{}",
        rec["options"]
    );
    assert!(
        host.log[0].contains("[77, 0, 0, 0, 0, 0]"),
        "{:?}",
        host.log
    );
    // Retry: the same options.
    ui.update(3.0);
    key(&mut ui, "KeyR", "r");
    ui.update(0.5);
    host.poll(&mut ui, &env, Some(dev()), 48_000);
    ui.update(0.5);
    assert!(host.prompting());
    assert_eq!(options(&env)[..2], ["speed=×2.75", "reverse=On"]);
    // Save as default: the rows' values stored.
    for _ in 0..1 {
        key(&mut ui, "ArrowUp", "ArrowUp");
    }
    assert_eq!(ui.focused(), Some("SaveOptions"));
    key(&mut ui, "Enter", "Enter");
    ui.update(0.05);
    let s = Settings::load();
    assert!(s.reverse && (s.speed - 2.75).abs() < 1e-9);
    assert!(
        env.state
            .borrow()
            .prompt
            .as_ref()
            .is_some_and(|p| p.options_saved && !p.options_differ)
    );
    assert!(!host.playing());
    // Put back, and the next song's prompt starts from what is stored.
    s.clone().save();
    let mut back = Settings::load();
    back.reverse = false;
    back.speed = stored.speed;
    back.save();
    key(&mut ui, "Escape", "Escape");
    host.poll(&mut ui, &env, Some(dev()), 48_000);
    ui.update(0.5);
    assert!(host.idle());
    *host.requested.borrow_mut() = Some(Request::song("beatucada", 2, true));
    until_ready(&mut host, &mut ui);
    assert_eq!(options(&env)[..2], ["speed=×2.5", "reverse=Off"]);
}
