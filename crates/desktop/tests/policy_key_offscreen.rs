//! Shift changes the attack policy during play (the maintainer,
//! 2026-11-13), as the desktop window draws it, on this machine's GPU
//! offscreen and a fake clock with no sound device: a CPU-practice
//! stage with attacks, its prompt's policy row saying how to change it
//! during play (Shift's cap, "during play"), then the play, where Shift
//! cycles the policy and the label under the life gauge has Shift's
//! small cap after it. Skipped where there is no GPU adapter (CI).
//! `DDI_POLICY_SHOTS=<dir>` keeps `prompt.ppm` and `play.ppm`.

mod common;
use std::rc::Rc;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

use ddi_desktop::gpu::{self, Gpu};
use ddi_desktop::play::host::{Device, Host};
use ddi_desktop::play::view::Formats;
use ddi_platform::HostTime;
use ddi_ui::event::{KeyEvent, UiEvent};
use ddi_ui::screens::data::{ChartEntry, Difficulty, SongEntry};

const SIZE: [u32; 2] = [1280, 720];

fn key(ui: &mut ddi_ui::Ui, code: &str, k: &str) {
    ui.handle(UiEvent::Key(KeyEvent::down(code, k)));
    ui.handle(UiEvent::Key(KeyEvent::up(code, k)));
}

fn ppm(name: &str, pixels: &[u8]) {
    let Some(dir) = std::env::var_os("DDI_POLICY_SHOTS") else {
        return;
    };
    let mut out = format!("P6\n{} {}\n255\n", SIZE[0], SIZE[1]).into_bytes();
    out.extend(
        pixels
            .as_chunks::<4>()
            .0
            .iter()
            .flat_map(|p| [p[0], p[1], p[2]]),
    );
    std::fs::write(std::path::Path::new(&dir).join(format!("{name}.ppm")), out).unwrap();
}

#[test]
fn shift_is_shown_on_the_prompt_and_beside_the_policy_and_cycles_it() {
    common::indexes();
    let instance = gpu::instance();
    let Some(adapter) = gpu::adapter(&instance, None) else {
        eprintln!("no GPU adapter: skipped");
        return;
    };
    let (device, queue) = gpu::device(&adapter).unwrap();
    let (ui_device, ui_queue) = gpu::device(&adapter).unwrap();
    let mut ui_gpu = Gpu::new(ui_device, ui_queue, wgpu::TextureFormat::Rgba8Unorm);
    let fold: ddi_ui::dev_widgets::FoldFn = Rc::new(|s: &str| s.to_lowercase());
    let (mut ui, env, _) = ddi_desktop::shell::theme_ui("start", fold);
    env.reduced_motion.set(true);
    ui.handle(UiEvent::Resize {
        size: [SIZE[0] as f32, SIZE[1] as f32],
        ratio: 1.0,
    });
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
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
        view_formats: &[wgpu::TextureFormat::Rgba8UnormSrgb],
    });
    let srgb = texture.create_view(&wgpu::TextureViewDescriptor {
        format: Some(wgpu::TextureFormat::Rgba8UnormSrgb),
        ..Default::default()
    });
    let plain = texture.create_view(&Default::default());
    let room = ddi_desktop::room::current();
    room.borrow_mut().set_rules_seen(true);
    ui.push(ddi_desktop::room::room_screen_with(
        &env,
        None,
        None,
        Some(host.requested.clone()),
    ));
    ui.update(0.5);
    let chart = 4;
    let song = ddi_desktop::play::load::read_chart("beatucada").unwrap();
    let entry = SongEntry {
        id: "beatucada".into(),
        title: "Beatucada".into(),
        charts: vec![ChartEntry {
            style: "dance-single".into(),
            difficulty: Difficulty::Medium,
            index: chart,
            digest: Some(song.chart_digest(chart)),
            ..ChartEntry::default()
        }],
        ..SongEntry::default()
    };
    {
        let mut r = room.borrow_mut();
        let mut rules = ddi_room::MatchRules {
            stages: 1,
            ..r.rules().clone()
        };
        rules.set_attacks(true);
        r.save_rules(rules).unwrap();
    }
    room.borrow_mut().pick(&entry, &entry.charts[0]).unwrap();
    ui.update(0.1);
    let mut request = host.requested.borrow_mut().take().expect("asked for");
    request.auto = true;
    *host.requested.borrow_mut() = Some(request);
    let mut waited = 0;
    while !host.prompt_ready() {
        host.poll(&mut ui, &env, Some(dev()), 48_000);
        ui.update(0.05);
        std::thread::sleep(std::time::Duration::from_millis(20));
        waited += 1;
        assert!(waited < 1000, "the song was never read");
    }
    ui.update(0.5);
    // The prompt: the policy row, and under it Shift's cap and "during
    // play".
    assert!(ui.actor("Opt.room.targeting").is_some(), "{:?}", ui.layers());
    assert!(ui.actor("Opt.room.targeting.Key").is_some(), "the key's hint");
    std::thread::sleep(std::time::Duration::from_millis(200));
    ui.update(0.1);
    ppm(
        "prompt",
        &ui_gpu.draw_offscreen(&mut ui, SIZE, wgpu::TextureFormat::Rgba8Unorm),
    );
    key(&mut ui, "Enter", "Enter");
    ui.update(0.05);
    {
        let mut r = room.borrow_mut();
        let later = r.now_ms() + 3_000;
        r.tick(later);
    }
    ui.update(0.05);
    host.poll(&mut ui, &env, Some(dev()), 48_000);
    assert!(host.playing(), "{:?}", ui.layers());
    use ddi_room::attacks::Targeting;
    let mut frames = 0;
    let mut shot = false;
    while host.playing() && frames < 200 {
        let now = clock.fetch_add(1_000_000_000 / 4, Ordering::SeqCst) + 1_000_000_000 / 4;
        if frames == 40 {
            let p = host.running().expect("playing");
            assert_eq!(p.session.policy_key(), Some(ddi_session::play::PolicyKey::Key("ShiftLeft")));
            for pressed in [true, false] {
                host.feed.borrow_mut().edges.push(ddi_platform::RawInput {
                    device: ddi_platform::DeviceId::Keyboard,
                    control: "ShiftLeft".into(),
                    pressed,
                    host_time: HostTime(now as f64 / 1e9 - 0.2),
                    slot: 0,
                });
            }
        }
        if frames == 44 {
            let p = host.running().expect("playing");
            let b = p.session.battle().expect("a stage");
            assert_eq!(b.policy(&p.session.player), Some(Targeting::Retaliate), "Shift cycled it");
            assert_eq!(p.session.player.log().policies.len(), 1);
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
        queue.submit(Some(encoder.finish()));
        if frames == 46 && !shot {
            std::thread::sleep(std::time::Duration::from_millis(200));
            let _ = device.poll(wgpu::PollType::wait_indefinitely());
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
            let _ = device.poll(wgpu::PollType::wait_indefinitely());
            ppm("play", &gpu::read_rgba(&device, &queue, &texture));
            shot = true;
        }
        if frames % 16 == 0 {
            let _ = device.poll(wgpu::PollType::wait_indefinitely());
        }
        frames += 1;
    }
    assert!(shot, "the play was drawn past its policy change");
}
