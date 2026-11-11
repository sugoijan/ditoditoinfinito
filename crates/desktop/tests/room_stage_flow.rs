//! A whole CPU-practice match as the desktop window runs it
//! (`docs/plans/multiplayer-prototype-progress.md`, M3 and M7b's flow), on
//! this machine's GPU offscreen and a fake clock with no sound device: the
//! room's song select (its pick), the stage's song prepared (the chart
//! read, the CPUs' plan checked), its prompt opened by the room over it
//! (Ready, then the room starting the play against the CPUs drawn with
//! their column, its outcome reported, results going back to the room),
//! the standings, the match's results, the next match's pick; then a
//! stage's prompt left before its start withdraws (no forfeit). Nothing
//! is kept in the history.
//! Skipped where there is no GPU adapter (CI). `DDI_ROOM_SHOTS=<dir>`
//! keeps pictures of the play, the intermission and the results (PPM).

mod common;
use std::rc::Rc;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

use ddi_desktop::gpu::{self, Gpu};
use ddi_desktop::play::host::{Device, Host};
use ddi_desktop::play::view::Formats;
use ddi_platform::HostTime;
use ddi_platform::history::MemoryPlayStore;
use ddi_room::Phase;
use ddi_ui::event::{KeyEvent, UiEvent};
use ddi_ui::screens::data::{ChartEntry, Difficulty, SongEntry};
use ddi_ui::state::history::Plays;

const SIZE: [u32; 2] = [1280, 720];
const CHART: usize = 1;

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

fn ppm(name: &str, pixels: &[u8]) {
    let Some(dir) = std::env::var_os("DDI_ROOM_SHOTS") else {
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
fn a_practice_match_plays_its_stages_and_comes_back_to_the_room() {
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
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
        view_formats: &[wgpu::TextureFormat::Rgba8UnormSrgb],
    });
    let srgb = texture.create_view(&wgpu::TextureViewDescriptor {
        format: Some(wgpu::TextureFormat::Rgba8UnormSrgb),
        ..Default::default()
    });
    let plain = texture.create_view(&Default::default());

    // The room over the start menu, its stages played through the host.
    let room = ddi_desktop::room::current();
    room.borrow_mut().set_rules_seen(true);
    ui.push(ddi_desktop::room::room_screen_with(
        &env,
        None,
        None,
        Some(host.requested.clone()),
    ));
    ui.update(0.5);
    let song = ddi_desktop::play::load::read_chart("beatucada").unwrap();
    let entry = SongEntry {
        id: "beatucada".into(),
        title: "Beatucada".into(),
        charts: vec![ChartEntry {
            style: "dance-single".into(),
            difficulty: Difficulty::Hard,
            index: CHART,
            digest: Some(song.chart_digest(CHART)),
            ..ChartEntry::default()
        }],
        ..SongEntry::default()
    };
    {
        let mut r = room.borrow_mut();
        let rules = ddi_room::MatchRules {
            stages: 2,
            ..r.rules().clone()
        };
        r.save_rules(rules).unwrap();
    }

    let mut play_stage = |ui: &mut ddi_ui::Ui, shot: Option<&str>| {
        // The host's pick: the room's update prepares it (the chart read,
        // the plan checked) and hands its prompt to the host.
        room.borrow_mut().pick(&entry, &entry.charts[0]).unwrap();
        ui.update(0.1);
        let stage = room.borrow().state().upcoming_stage().unwrap();
        assert!(!room.borrow().loading(stage), "prepared");
        assert_eq!(room.borrow().load_problem(stage), None);
        let mut request = host
            .requested
            .borrow_mut()
            .take()
            .expect("the stage asked for");
        assert!(request.stage.is_some());
        request.auto = true;
        *host.requested.borrow_mut() = Some(request);
        let mut waited = 0;
        loop {
            host.poll(ui, &env, Some(dev()), 48_000);
            ui.update(0.05);
            if host.prompt_ready() {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(20));
            waited += 1;
            assert!(waited < 1000, "the song was never read");
        }
        ui.update(0.5);
        assert_eq!(top(ui), "play-prompt");
        // Ready: the room counts down (alone with CPUs, all ready), then
        // starts the stage, the prompt pressing Start for the host.
        key(ui, "Enter", "Enter");
        ui.update(0.05);
        assert!(room.borrow().ready());
        {
            let mut r = room.borrow_mut();
            let later = r.now_ms() + 3_000;
            r.tick(later);
        }
        ui.update(0.05);
        host.poll(ui, &env, Some(dev()), 48_000);
        assert!(host.playing(), "{:?}", ui.layers());
        let mut frames = 0;
        let mut columns = 0;
        while host.playing() {
            let now = clock.fetch_add(1_000_000_000 / 4, Ordering::SeqCst) + 1_000_000_000 / 4;
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
            if let Some(p) = host.running() {
                let opponents = p.session.opponents();
                if !opponents.is_empty() {
                    columns += 1;
                    assert_eq!(opponents.len(), 3);
                }
            }
            // The column's fonts are read on a thread: real time for it
            // before the picture.
            if shot.is_some() && (60..120).contains(&frames) {
                std::thread::sleep(std::time::Duration::from_millis(20));
            }
            if frames == 120
                && let Some(name) = shot
            {
                ppm(name, &gpu::read_rgba(&device, &queue, &texture));
            }
            if frames % 16 == 0 {
                let _ = device.poll(wgpu::PollType::wait_indefinitely());
            }
            frames += 1;
            assert!(frames < 4 * 400, "the play never ended");
        }
        assert!(columns > 0, "the opponents were shown");
        let _ = device.poll(wgpu::PollType::wait_indefinitely());
        ui.update(0.05);
        assert_eq!(top(ui), "results");
        // Back to room, never Retry: a stage is played once.
        assert!(ui.bounds("SongList").is_some());
        assert!(ui.bounds("Retry").is_none());
        ui.update(3.0);
        // Where the graphs are: the Standings tab first (feedback round
        // B), the stage confirmed, the player and the three CPUs each
        // with how they performed, as their engine-judged plays say.
        let view = env.state.borrow().stage.clone().expect("the standings");
        assert_eq!(view.tab, ddi_ui::screens::results::Tab::Standings);
        assert!(view.confirmed, "{view:?}");
        assert_eq!(view.rows.len(), 4);
        assert_eq!(view.rows.iter().filter(|r| r.me).count(), 1);
        for row in &view.rows {
            assert!(row.place.is_some() && row.award.is_some(), "{row:?}");
            // DDI's score as shown everywhere: the sum (round 3b), a
            // CPU's on the chart's own scale.
            assert!(row.score.parse::<i64>().is_ok(), "{row:?}");
            assert!(!row.grade.is_empty() && !row.life.is_empty(), "{row:?}");
            assert!(row.tiers.iter().sum::<u32>() > 0, "{row:?}");
            assert_eq!(row.level.is_some(), !row.me, "a CPU's stars");
        }
        // The sum, never the proportional part alone (at most a million).
        assert!(
            view.rows
                .iter()
                .any(|r| r.score.parse::<i64>().is_ok_and(|s| s > 1_000_000)),
            "{view:?}"
        );
        // The match's result on the last stage's, not before.
        assert_eq!(view.number == view.of, !view.winners.is_empty(), "{view:?}");
        assert!(ui.bounds("Panel.Standings").is_some() && ui.bounds("Panel.Timing").is_none());
        // Right: the graphs; Left: the standings again.
        key(ui, "ArrowRight", "ArrowRight");
        ui.update(0.3);
        assert!(ui.bounds("Panel.Timing").is_some() && ui.bounds("Panel.Standings").is_none());
        key(ui, "ArrowLeft", "ArrowLeft");
        ui.update(0.3);
        assert!(ui.bounds("Panel.Standings").is_some());
        // Back to the room (Enter: its button has focus).
        key(ui, "Enter", "Enter");
        ui.update(0.5);
        host.poll(ui, &env, Some(dev()), 48_000);
        ui.update(1.0);
        assert!(!host.prompting() && !host.playing());
        // The room's song select: the standings were seen on the
        // results, so they do not pop up again.
        assert_eq!(top(ui), "select", "{:?}", ui.layers());
        assert_eq!(room.borrow().unseen_results(), None);
        assert!(env.state.borrow().stage.is_none(), "gone with the results");
    };

    play_stage(&mut ui, Some("play"));
    assert!(matches!(
        room.borrow().state().phase,
        Phase::Intermission { .. }
    ));
    let view = env.state.borrow().room.clone().unwrap();
    assert!(
        view.mark
            .as_deref()
            .is_some_and(|m| m.starts_with("Stage 1:")),
        "{view:?}"
    );
    assert!(!view.ready, "Ready again for stage 2");
    ui.update(0.5);
    ppm(
        "intermission",
        &ui_gpu.draw_offscreen(&mut ui, SIZE, wgpu::TextureFormat::Rgba8Unorm),
    );
    play_stage(&mut ui, None);
    assert_eq!(room.borrow().state().phase, Phase::MatchResults);
    ui.update(0.5);
    ppm(
        "results",
        &ui_gpu.draw_offscreen(&mut ui, SIZE, wgpu::TextureFormat::Rgba8Unorm),
    );
    let standings = room.borrow().state().standings().unwrap();
    assert_eq!(standings.len(), 4);
    assert!(store.is_empty(), "a stage is not kept in the history");
    eprintln!("room flow: {:?}", host.log);

    // The next match's pick: fresh readiness; its prompt left before the
    // start withdraws (the pick reopened, no forfeit).
    room.borrow_mut().pick(&entry, &entry.charts[0]).unwrap();
    assert_eq!(room.borrow().state().history.len(), 1);
    assert!(!room.borrow().ready());
    ui.update(0.1);
    let mut waited = 0;
    while !host.prompt_ready() {
        host.poll(&mut ui, &env, Some(dev()), 48_000);
        ui.update(0.05);
        std::thread::sleep(std::time::Duration::from_millis(20));
        waited += 1;
        assert!(waited < 1000, "the song was never read");
    }
    ui.update(0.5);
    key(&mut ui, "Escape", "Escape");
    ui.update(0.5);
    host.poll(&mut ui, &env, Some(dev()), 48_000);
    ui.update(1.0);
    assert_eq!(top(&ui), "select", "{:?}", ui.layers());
    assert!(matches!(
        room.borrow().state().phase,
        Phase::Preparing { window: None, .. }
    ));
    assert!(room.borrow().last_mark().is_none(), "no stage played");
    assert_eq!(store.len(), 0);
}

/// M8b: a practice stage under the attacks' battle (the development
/// switch's) through the window: full bars launch attacks at the
/// checkpoints, each commitment logged, the shocks delivered into the
/// recipients' plays (the player's own among them, avoided by autoplay),
/// and the play ends on its results as any stage's.
#[test]
fn a_practice_stage_with_attacks_launches_them_through_the_window() {
    common::indexes();
    let instance = gpu::instance();
    let Some(adapter) = gpu::adapter(&instance, None) else {
        eprintln!("no GPU adapter: skipped");
        return;
    };
    let (device, queue) = gpu::device(&adapter).unwrap();
    let fold: ddi_ui::dev_widgets::FoldFn = Rc::new(|s: &str| s.to_lowercase());
    let (mut ui, env, _) = ddi_desktop::shell::theme_ui("start", fold);
    env.reduced_motion.set(true);
    ui.handle(UiEvent::Resize {
        size: [SIZE[0] as f32, SIZE[1] as f32],
        ratio: 1.0,
    });
    let store = Rc::new(MemoryPlayStore::default());
    let mut host = Host::new(Rc::default(), Some(Plays::new(store.clone())));
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
    // Beatucada's Medium chart: long enough for full bars.
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
        let rules = ddi_room::MatchRules {
            stages: 1,
            battle: ddi_room::BattlePreset::GAUGE_ATTACKS_V0.into(),
            ..r.rules().clone()
        };
        r.save_rules(rules).unwrap();
    }
    room.borrow_mut().pick(&entry, &entry.charts[0]).unwrap();
    ui.update(0.1);
    let mut request = host.requested.borrow_mut().take().expect("asked for");
    assert_eq!(
        request.stage.as_ref().map(|s| s.battle),
        Some(ddi_room::BattlePreset::GaugeAttacksV0)
    );
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
    let mut frames = 0;
    let mut attacks = 0;
    let mut against_me = 0;
    while host.playing() {
        let now = clock.fetch_add(1_000_000_000 / 4, Ordering::SeqCst) + 1_000_000_000 / 4;
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
        if let Some(p) = host.running()
            && let Some(b) = p.session.battle()
            && let Some(a) = b.attacks()
        {
            attacks = a.ledger().attacks().len();
            against_me = p.session.player.log().hazards.len() / 8;
        }
        if frames % 16 == 0 {
            let _ = device.poll(wgpu::PollType::wait_indefinitely());
        }
        frames += 1;
        assert!(frames < 4 * 400, "the play never ended");
    }
    let _ = device.poll(wgpu::PollType::wait_indefinitely());
    ui.update(0.05);
    assert_eq!(top(&ui), "results");
    assert!(attacks > 0, "full bars launched");
    // The standings count them: every line says its attacks and shocks,
    // and those launched add up to the ledger's.
    ui.update(1.0);
    let view = env.state.borrow().stage.clone().expect("the standings");
    assert!(view.rows.iter().all(|r| r.attacks.is_some()), "{view:?}");
    let launched: u32 = view
        .rows
        .iter()
        .filter_map(|r| r.attacks)
        .map(|a| a.0)
        .sum();
    assert_eq!(launched as usize, attacks, "{view:?}");
    let logged = host
        .log
        .iter()
        .filter(|l| l.starts_with("attack: committed"))
        .count();
    assert!(logged >= attacks, "{:?}", host.log);
    assert!(
        host.log.iter().filter(|l| l.contains("→you")).count() >= against_me,
        "{:?}",
        host.log
    );
    assert!(store.is_empty(), "a stage is not kept in the history");
    eprintln!("attacks: {attacks}, {against_me} against the player");
}
