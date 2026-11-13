//! `attacks-v2` and `settlement-v1` as the desktop window draws them
//! (`docs/plans/multiplayer-prototype-progress.md`, "Attacks, version
//! 2"), on this machine's GPU offscreen and a fake clock with no sound
//! device: a CPU-practice stage whose strong CPUs all aim at the leader
//! (the autoplayed player) chains their attacks against the player, one
//! safe section after another; the warning by the receptors shows the
//! nearest and a smaller glyph for each placed behind it; once the CPUs
//! are out with shocks still to come the status line says "Last
//! standing", then "Stage won" once they have passed. Skipped where there
//! is no GPU adapter (CI).
//!
//! `DDI_AV2_SHOTS=<dir>` keeps the moments' pictures (PPM). Three CPUs
//! of the roster's level 4 (Hard timing, Beatucada's chart 5, roster
//! seed 21, chosen again by a sweep under `cpu-levels-v1`): they fill
//! their bars, chain against the player and go out.

mod common;
use std::rc::Rc;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Instant;

use ddi_desktop::gpu;
use ddi_desktop::play::host::{Device, Host};
use ddi_desktop::play::view::Formats;
use ddi_platform::HostTime;
use ddi_ui::event::{KeyEvent, UiEvent};
use ddi_ui::screens::data::{ChartEntry, Difficulty, SongEntry};
use ddi_ui::state::settings::Settings;

fn key(ui: &mut ddi_ui::Ui, code: &str, k: &str) {
    ui.handle(UiEvent::Key(KeyEvent::down(code, k)));
    ui.handle(UiEvent::Key(KeyEvent::up(code, k)));
}

fn ppm(size: [u32; 2], name: &str, pixels: &[u8]) {
    let Some(dir) = std::env::var_os("DDI_AV2_SHOTS") else {
        return;
    };
    let mut out = format!("P6\n{} {}\n255\n", size[0], size[1]).into_bytes();
    out.extend(
        pixels
            .as_chunks::<4>()
            .0
            .iter()
            .flat_map(|p| [p[0], p[1], p[2]]),
    );
    std::fs::write(std::path::Path::new(&dir).join(format!("{name}.ppm")), out).unwrap();
}

fn env_or<T: std::str::FromStr>(name: &str, or: T) -> T {
    std::env::var(name).ok().and_then(|v| v.parse().ok()).unwrap_or(or)
}

#[test]
fn chained_attacks_then_last_standing_then_the_win() {
    common::indexes();
    let instance = gpu::instance();
    let Some(adapter) = gpu::adapter(&instance, None) else {
        eprintln!("no GPU adapter: skipped");
        return;
    };
    let size = [1280u32, 720];
    let (device, queue) = gpu::device(&adapter).unwrap();
    let settings = Settings {
        dancing_hand: false,
        ..Settings::default()
    };
    settings.save();
    let fold: ddi_ui::dev_widgets::FoldFn = Rc::new(|s: &str| s.to_lowercase());
    let (mut ui, env, _) = ddi_desktop::shell::theme_ui("start", fold);
    env.reduced_motion.set(true);
    ui.handle(UiEvent::Resize {
        size: [size[0] as f32, size[1] as f32],
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
            width: size[0],
            height: size[1],
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

    ddi_desktop::room::develop(0);
    let room = ddi_desktop::room::current();
    room.borrow_mut().set_rules_seen(true);
    ui.push(ddi_desktop::room::room_screen_with(
        &env,
        None,
        None,
        Some(host.requested.clone()),
    ));
    ui.update(0.5);
    let chart = env_or("DDI_AV2_CHART", 5usize);
    let song = ddi_desktop::play::load::read_chart("beatucada").unwrap();
    let entry = SongEntry {
        id: "beatucada".into(),
        title: "Beatucada".into(),
        charts: vec![ChartEntry {
            style: "dance-single".into(),
            difficulty: Difficulty::Challenge,
            index: chart,
            digest: Some(song.chart_digest(chart)),
            ..ChartEntry::default()
        }],
        ..SongEntry::default()
    };
    let level: u8 = env_or("DDI_AV2_LEVEL", 4);
    let seed: u64 = env_or("DDI_AV2_SEED", 21);
    {
        let mut r = room.borrow_mut();
        let mut rules = ddi_room::MatchRules {
            stages: 1,
            ..r.rules().clone()
        };
        rules.set_attacks(true);
        // Hard timing: CPUs go out.
        rules.difficulty = ddi_room::MatchDifficulty::Hard;
        rules.cpu.mode = ddi_room::CpuMode::Roster;
        rules.cpu.roster = ["Ember Ratchet", "Mossy Accordion", "Velvet Anvil"]
            .into_iter()
            .enumerate()
            .map(|(i, name)| ddi_room::OpponentSpec {
                level,
                seed: seed + i as u64,
                personality: None,
                name: Some(name.into()),
                targeting: Some(ddi_room::attacks::Targeting::Leader),
            })
            .collect();
        rules.seats = rules.seats.max(4);
        r.save_rules(rules).unwrap();
        r.pick(&entry, &entry.charts[0]).unwrap();
    }
    ui.update(0.1);
    let mut request = host.requested.borrow_mut().take().expect("the stage");
    request.auto = true;
    *host.requested.borrow_mut() = Some(request);
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
    key(&mut ui, "Enter", "Enter");
    ui.update(0.05);
    {
        let mut r = room.borrow_mut();
        let later = r.now_ms() + 3_000;
        r.tick(later);
    }
    ui.update(0.05);
    host.poll(&mut ui, &env, Some(dev()), 48_000);
    assert!(host.playing());

    let started = Instant::now();
    let mut frames = 0u32;
    let mut most_behind = 0u16;
    let mut shot_behind = 0u16;
    let mut stood_at: Option<f64> = None;
    let mut won_at: Option<f64> = None;
    let last_standing = ddi_l10n::msg::room::play_last_standing().to_string();
    let stage_won = ddi_l10n::msg::room::play_won().to_string();
    let shoot = |host: &mut Host, ui: &mut ddi_ui::Ui, now: u64, name: String| {
        // The text's fonts are read on a thread: real time for them.
        std::thread::sleep(std::time::Duration::from_millis(200));
        let _ = device.poll(wgpu::PollType::wait_indefinitely());
        let mut encoder = device.create_command_encoder(&Default::default());
        host.draw_play(
            &dev(),
            &mut encoder,
            &srgb,
            Some(&plain),
            size,
            HostTime(now as f64 / 1e9),
            ui,
            &env,
        );
        queue.submit(Some(encoder.finish()));
        let _ = device.poll(wgpu::PollType::wait_indefinitely());
        ppm(size, &name, &gpu::read_rgba(&device, &queue, &texture));
        eprintln!("picture: {name}");
    };
    let mut record = None;
    while host.playing() {
        let host_now = HostTime(clock.load(Ordering::SeqCst) as f64 / 1e9);
        let song_t = host
            .running()
            .map_or(0.0, |p| p.session.player.judge_time(host_now));
        let step = (1e9 / 30.0) as u64;
        let now = clock.fetch_add(step, Ordering::SeqCst) + step;
        let mut encoder = device.create_command_encoder(&Default::default());
        host.draw_play(
            &dev(),
            &mut encoder,
            &srgb,
            Some(&plain),
            size,
            HostTime(now as f64 / 1e9),
            &mut ui,
            &env,
        );
        queue.submit(Some(encoder.finish()));
        if frames.is_multiple_of(16) {
            let _ = device.poll(wgpu::PollType::wait_indefinitely());
        }
        let (standing, cue) = host.standing_and_warning();
        if let Some(c) = cue {
            most_behind = most_behind.max(c.behind);
            if c.behind > shot_behind && c.behind >= 2 && c.scale >= 1.0 {
                shot_behind = c.behind;
                shoot(&mut host, &mut ui, now, format!("warning-{}-behind", c.behind));
            }
        }
        if standing.as_deref() == Some(last_standing.as_str()) && stood_at.is_none() {
            stood_at = Some(song_t);
            shoot(&mut host, &mut ui, now, "last-standing".into());
        }
        if standing.as_deref() == Some(stage_won.as_str()) && won_at.is_none() {
            won_at = Some(song_t);
            shoot(&mut host, &mut ui, now, "stage-won".into());
        }
        if let Some(p) = host.running()
            && let Some(b) = p.session.battle()
            && let Some(a) = b.attacks()
        {
            record = Some((
                a.ledger().attacks().to_vec(),
                b.waited_to_settle(),
                p.session.player.judge_time(host_now),
            ));
        }
        frames += 1;
        assert!(frames < 30 * 400, "the play never ended");
    }
    let (attacks, waited_to_settle, _) = record.expect("a battle with attacks");
    let me = attacks
        .iter()
        .map(|a| a.recipient)
        .max_by_key(|p| attacks.iter().filter(|a| a.recipient == *p).count())
        .expect("attacks");
    let mine: Vec<_> = attacks.iter().filter(|a| a.recipient == me).collect();
    eprintln!(
        "{} frames in {:.1} s: {} attacks, {} against the most attacked; at most {most_behind} \
         behind the nearest; last standing at {stood_at:?}, won at {won_at:?} (waited {waited_to_settle})",
        frames,
        started.elapsed().as_secs_f64(),
        attacks.len(),
        mine.len(),
    );
    for a in &mine {
        eprintln!(
            "  attack {} from {:?}: slot {}, {:.2} s to {:.2} s",
            a.id,
            a.sender,
            a.slot,
            a.cells[0].at_us as f64 / 1e6,
            a.clear_us as f64 / 1e6
        );
    }
    // Chained: each at or after the one before it against the same
    // recipient has cleared, never the same region.
    for w in mine.windows(2) {
        if w[1].checkpoint == w[0].checkpoint || w[1].cells[0].at_us < w[0].clear_us + 2_000_000 {
            assert!(w[1].cells[0].at_us >= w[0].clear_us, "{w:?}");
        }
    }
    assert!(most_behind >= 2, "three chained against the player at once");
    // Last standing before the win: the stage waited for shocks (found
    // by a sweep over levels, charts and seeds; `DDI_AV2_*` try others,
    // which need not wait).
    let tuned = ["DDI_AV2_CHART", "DDI_AV2_LEVEL", "DDI_AV2_SEED"]
        .iter()
        .all(|v| std::env::var_os(v).is_none());
    if waited_to_settle || tuned {
        let (stood, won) = (stood_at.expect("last standing shown"), won_at.expect("won"));
        assert!(stood < won, "{stood} {won}");
    }
}
