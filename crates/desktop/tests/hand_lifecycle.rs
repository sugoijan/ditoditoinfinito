//! Consecutive desktop plays on one host, with a cumulative fake audio clock.
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

const SIZE: [u32; 2] = [960, 540];

fn key(ui: &mut ddi_ui::Ui, code: &str, k: &str) {
    ui.handle(UiEvent::Key(KeyEvent::down(code, k)));
    ui.handle(UiEvent::Key(KeyEvent::up(code, k)));
}

#[test]
fn consecutive_completed_and_quit_plays_keep_the_hand_available() {
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

    let dir = std::env::temp_dir().join(format!("ddi-hand-lifecycle-{}", std::process::id()));
    for (name, rows) in [
        ("First", "1000\n0100\n0010\n0001"),
        ("Second", "0010\n0001\n1000\n0100"),
    ] {
        let song = dir.join("Pack").join(name);
        std::fs::create_dir_all(&song).unwrap();
        std::fs::write(song.join("song.sm"),format!("#TITLE:{name};#MUSIC:music.wav;#OFFSET:-1;#BPMS:0=120;#NOTES:dance-single::Easy:2:0,0,0,0,0:\n{rows};")).unwrap();
        std::fs::write(song.join("music.wav"), silence()).unwrap();
    }
    let (songs, report) = ddi_desktop::library::scan_folders(std::slice::from_ref(&dir));
    assert!(report.failed.is_empty(), "{:?}", report.failed);
    assert_eq!(songs.len(), 2);
    ddi_desktop::library::register_for_tests(&songs);
    for round in 0..4 {
        let request = Request::song(&songs[round % 2].entry.id, 0, true);
        *host.requested.borrow_mut() = Some(request);
        let started = Instant::now();
        loop {
            host.poll(&mut ui, &env, Some(dev()), 48_000);
            ui.update(1. / 60.);
            if host.prompt_ready() {
                break;
            }
            assert!(
                started.elapsed() < Duration::from_secs(30),
                "round {round}: {:?}",
                host.prompt_waiting()
            );
            std::thread::sleep(Duration::from_millis(5));
        }
        key(&mut ui, "Enter", "Enter");
        ui.update(0.05);
        host.poll(&mut ui, &env, Some(dev()), 48_000);
        assert!(host.playing());
        let h = host.hand_debug().unwrap();
        assert!(
            h["runtime_error"].is_null() && h["runtime"]["available"] == true,
            "round {round} startup: {h}"
        );
        assert!(h["character"]["drawn"].as_u64().unwrap() > 0, "{h}");
        let log_start = host.log.len();
        let mut frames = 0;
        while host.playing() {
            let now = clock.fetch_add(1_000_000_000 / 60, Ordering::SeqCst) + 1_000_000_000 / 60;
            if let Some(pressed) = match (round, frames) {
                (1, 60) | (1, 70) => Some(true),
                (1, 61) | (1, 71) => Some(false),
                _ => None,
            } {
                host.feed.borrow_mut().edges.push(ddi_platform::RawInput {
                    device: ddi_platform::DeviceId::Keyboard,
                    control: "Escape".into(),
                    pressed,
                    host_time: HostTime(now as f64 / 1e9),
                    slot: 0,
                });
            }
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
            queue.submit([encoder.finish()]);
            let Some(h) = host.hand_debug() else {
                assert!(!host.playing());
                break;
            };
            assert!(
                h["runtime_error"].is_null(),
                "round {round}, frame {frames}: {h}"
            );
            assert_eq!(
                h["runtime"]["starved_frames"], 0,
                "round {round}, frame {frames}: {h}"
            );
            assert_eq!(h["runtime"]["available"], true, "{h}");
            frames += 1;
            assert!(frames < 1200, "play never ended");
        }
        let ending = if round == 1 { "quit " } else { "played " };
        assert!(
            host.log[log_start..].iter().any(|s| s.starts_with(ending)),
            "{:?}",
            host.log
        );
        ui.update(3.);
        key(&mut ui, "Escape", "Escape");
        ui.update(0.5);
        host.poll(&mut ui, &env, Some(dev()), 48_000);
        ui.update(0.5);
        assert!(host.idle(), "round {round} did not return to song select");
    }
    std::fs::remove_dir_all(&dir).unwrap();
}

fn silence() -> Vec<u8> {
    let rate = 44100u32;
    let data = vec![0u8; (rate * 4 * 4) as usize];
    let mut wav = Vec::new();
    wav.extend(b"RIFF");
    wav.extend((36 + data.len() as u32).to_le_bytes());
    wav.extend(b"WAVEfmt ");
    wav.extend(16u32.to_le_bytes());
    wav.extend(1u16.to_le_bytes());
    wav.extend(2u16.to_le_bytes());
    wav.extend(rate.to_le_bytes());
    wav.extend((rate * 4).to_le_bytes());
    wav.extend(4u16.to_le_bytes());
    wav.extend(16u16.to_le_bytes());
    wav.extend(b"data");
    wav.extend((data.len() as u32).to_le_bytes());
    wav.extend(data);
    wav
}
