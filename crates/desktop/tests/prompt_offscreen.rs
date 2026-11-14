//! The start prompt as the window runs it (`play::host` with the UI
//! core's prompt, `ddi_ui::screens::prompt`, as the web's play route
//! shows it): Some Things Must's Hard chart autoplayed, drawn offscreen
//! on this machine's GPU once the song is read and its sound runs (no
//! device, a fake clock): the song, its chart and the mods, over the
//! song's background in its frame; then, on new devices, Start offers
//! their offsets first, and "Play with defaults" keeps them and plays.
//! Skipped where there is no GPU adapter (CI). `DDI_OFFSCREEN_PPM=<dir>`
//! keeps the picture (`prompt.ppm`).

mod common;
use std::rc::Rc;

use ddi_desktop::gpu::{self, Gpu};
use ddi_desktop::play::Request;
use ddi_desktop::play::host::{Device, Host};
use ddi_desktop::play::view::Formats;
use ddi_platform::DeviceFingerprint;
use ddi_ui::state::settings::Settings;
use common::ui::key_step as key;

const SIZE: [u32; 2] = [1280, 720];

#[test]
fn the_prompt_names_the_song_over_its_picture() {
    common::indexes();
    let Some(adapter) = common::gpu::adapter_or_skip() else {
        return;
    };
    let (device, queue) = gpu::device(&adapter).unwrap();
    let format = wgpu::TextureFormat::Rgba8Unorm;
    let mut gpu = Gpu::new(device, queue, format);
    let (mut ui, env) = common::ui::theme("select", common::ui::nfkd(), SIZE, true);
    ui.update(0.5);
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
    // The picture decoded and drawn.
    let mut pixels = Vec::new();
    for _ in 0..40 {
        ui.update(1.0 / 60.0);
        pixels = gpu.draw_offscreen(&mut ui, SIZE, format);
        if !gpu.loading() && !gpu.images_ready() {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(25));
    }
    for _ in 0..3 {
        ui.update(1.0 / 60.0);
        pixels = gpu.draw_offscreen(&mut ui, SIZE, format);
    }
    common::shot("prompt.ppm", &pixels, SIZE);
    {
        let s = env.state.borrow();
        assert_eq!(s.screen, "prompt");
        assert_eq!(s.song, "Some Things Must (OutFox Edit)");
        assert_eq!(s.chart.as_deref(), Some("Hard 13 · Single"));
        assert_eq!(s.status, "ready");
        assert!(
            s.backdrop
                .as_deref()
                .is_some_and(|b| b.ends_with("sevish-bg.png"))
        );
        assert_eq!(s.page_items, ["Start", "Back"]);
    }
    // The picture around the card: lit, not the ground.
    let at = |x: u32, y: u32| {
        let i = ((y * SIZE[0] + x) * 4) as usize;
        [pixels[i], pixels[i + 1], pixels[i + 2]]
    };
    let lit = (0..SIZE[1])
        .step_by(4)
        .flat_map(|y| (0..SIZE[0]).step_by(4).map(move |x| (x, y)))
        .filter(|&(x, y)| !(340..940).contains(&x) || !(190..490).contains(&y))
        .filter(|&(x, y)| at(x, y).iter().any(|&c| c > 60))
        .count();
    assert!(lit > 1000, "the picture: {lit} lit");

    // New devices: nothing is asked; Start plays, and the devices are
    // kept at the default offsets from the play on.
    let fp = |id: &str| Some(DeviceFingerprint::new(id, id));
    host.watcher.probe(fp("audio:test"), fp("display:test"));
    *host.requested.borrow_mut() = None;
    let mut asked = Request::song("some-things-must", 5, false);
    asked.auto = false;
    // The autoplayed prompt does not ask: Back, then a player's.
    key(&mut ui, "Escape", "Escape");
    host.poll(&mut ui, &env, None, 48_000);
    ui.update(0.5);
    assert!(host.idle());
    *host.requested.borrow_mut() = Some(asked);
    let mut waited = 0;
    while !host.prompt_ready() {
        let dev = Device {
            device: &gpu.device,
            queue: &gpu.queue,
            formats: Formats::of(format, true),
        };
        host.poll(&mut ui, &env, Some(dev), 48_000);
        ui.update(0.05);
        std::thread::sleep(std::time::Duration::from_millis(20));
        waited += 1;
        assert!(waited < 1000, "never ready");
    }
    ui.update(0.5);
    key(&mut ui, "Enter", "Enter");
    let dev = Device {
        device: &gpu.device,
        queue: &gpu.queue,
        formats: Formats::of(format, true),
    };
    host.poll(&mut ui, &env, Some(dev), 48_000);
    assert!(host.playing(), "{:?}", host.prompt_step());
    let s = Settings::load();
    assert!(s.audio_profile("audio:test").is_some());
    assert!(s.display_profile("display:test").is_some());
}
