//! A CPU-practice stage whose zero life steps down (a match's "At zero
//! life: Step down", `ddi_engine::ladder`) as the desktop window plays it,
//! on this machine's GPU offscreen and a fake clock with no sound device:
//! the player presses nothing, so their life runs out and they step down
//! the song's ladder (the status line says so, their field clears and the
//! easier chart's notes come in from the boundary); the weak CPUs step
//! down too, their miniatures drawing the chart they are on; the
//! standings mark who finished on which chart. Skipped where there is no
//! GPU adapter (CI). `DDI_ROOM_SHOTS=<dir>` keeps pictures (PPM).

mod common;
use std::rc::Rc;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

use ddi_desktop::gpu::{self, Gpu};
use ddi_desktop::play::host::{Device, Host};
use ddi_desktop::play::view::Formats;
use ddi_platform::HostTime;
use ddi_platform::history::MemoryPlayStore;
use ddi_ui::event::{KeyEvent, UiEvent};
use ddi_ui::screens::data::{ChartEntry, Difficulty, SongEntry};
use ddi_ui::state::history::Plays;

const SIZE: [u32; 2] = [1280, 720];

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
fn a_practice_stage_steps_down_the_player_and_a_cpu() {
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
    // Beatucada's Hard chart: its ladder is Medium, Easy, Beginner.
    let chart = 5;
    let song = ddi_desktop::play::load::read_chart("beatucada").unwrap();
    assert_eq!(ddi_engine::ladder::ladder(&song, chart), vec![4, 3, 1]);
    let entry = SongEntry {
        id: "beatucada".into(),
        title: "Beatucada".into(),
        charts: vec![ChartEntry {
            style: "dance-single".into(),
            difficulty: Difficulty::Hard,
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
            seats: 4,
            at_zero: ddi_room::AtZero::StepDown,
            ..r.rules().clone()
        };
        rules.set_attacks(true);
        // Three of the weakest CPUs: they step down on Hard.
        rules.cpu.mode = ddi_room::CpuMode::Fixed { count: 3 };
        rules.cpu.roster.clear();
        rules.cpu.opponent.level = 0;
        r.save_rules(rules).unwrap();
    }
    room.borrow_mut().pick(&entry, &entry.charts[0]).unwrap();
    ui.update(0.1);
    let request = host.requested.borrow().clone().expect("asked for");
    assert!(
        request
            .stage
            .as_ref()
            .is_some_and(|s| s.rules.steps_down()),
        "the stage steps down"
    );
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
    let mut seen_cue = false;
    let (mut shot_breather, mut shot_new, mut shot_cpu) = (false, false, false);
    let mut cpu_stepped = false;
    let mut player_steps = 0;
    let mut hand_seen = false;
    let read = |device: &wgpu::Device, queue: &wgpu::Queue| {
        let _ = device.poll(wgpu::PollType::wait_indefinitely());
        gpu::read_rgba(device, queue, &texture)
    };
    while host.playing() {
        let now = clock.fetch_add(1_000_000_000 / 8, Ordering::SeqCst) + 1_000_000_000 / 8;
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
        if let Some(p) = host.running() {
            let s = &p.session;
            let steps = s.player.steps().to_vec();
            player_steps = steps.len();
            let t = s.player.judge().last_time().unwrap_or(0.0);
            if let Some(b) = s.battle() {
                // A CPU's field draws the chart it is on.
                if b.feeds().any(|f| f.steps().count() > 0) {
                    cpu_stepped = true;
                    if !shot_cpu && b.feeds().any(|f| f.steps().any(|st| t > st.1 + 1.0)) {
                        shot_cpu = true;
                        ppm("step-cpu-field", &read(&device, &queue));
                    }
                }
            }
            if let Some(st) = steps.first() {
                if let Some((ddi_session::announcer::battle::StageCue::StepDown(1), _)) =
                    s.stage_cue(now as f64 / 1e9)
                {
                    seen_cue = true;
                    if !shot_breather && t > st.at + 0.5 {
                        shot_breather = true;
                        // The old chart's notes gone from the field.
                        let shown = s.frame(HostTime(now as f64 / 1e9));
                        assert!(
                            shown
                                .notes
                                .iter()
                                .all(|n| n.note_index >= ddi_engine::battle::HAZARD_INDEX_BASE
                                    || f64::from(n.beat) >= st.tick.beat() - 1e-6),
                            "only the new chart's notes and shocks in the breather"
                        );
                        ppm("step-breather-cue", &read(&device, &queue));
                    }
                }
                if !shot_new && t > st.boundary + 0.6 {
                    shot_new = true;
                    ppm("step-new-chart", &read(&device, &queue));
                }
            }
        }
        // The dancing hand plays out what it was given at the step and
        // rests: it never stops (a stopped hand hides for the play).
        if let Some(d) = host.hand_debug() {
            hand_seen |= d["status"] == "active";
            assert!(
                d["runtime_error"].is_null(),
                "the hand stopped at frame {frames}: {}",
                d["runtime"]
            );
        }
        if frames % 16 == 0 {
            let _ = device.poll(wgpu::PollType::wait_indefinitely());
        }
        frames += 1;
        assert!(frames < 8 * 400, "the play never ended");
    }
    let _ = device.poll(wgpu::PollType::wait_indefinitely());
    ui.update(0.05);
    assert_eq!(top(&ui), "results");
    assert!(player_steps >= 1, "the player stepped down");
    assert!(seen_cue, "the status line said it");
    assert!(shot_breather && shot_new, "{shot_breather} {shot_new}");
    assert!(cpu_stepped, "a CPU stepped down");
    assert!(hand_seen, "the dancing hand played beside the stage");
    // The standings mark who finished on which chart.
    ui.update(1.0);
    let view = env.state.borrow().stage.clone().expect("the standings");
    let me = view.rows.iter().find(|r| r.me).expect("the player's line");
    assert!(me.stepped.as_deref().is_some_and(|s| s.starts_with('↓')), "{view:?}");
    assert!(
        view.rows.iter().filter(|r| r.stepped.is_some()).count() >= 2,
        "{view:?}"
    );
    ui.update(0.5);
    ppm(
        "step-standings",
        &ui_gpu.draw_offscreen(&mut ui, SIZE, wgpu::TextureFormat::Rgba8Unorm),
    );
    assert!(store.is_empty(), "a stage is not kept in the history");
}
