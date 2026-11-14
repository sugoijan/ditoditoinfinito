//! CPU-practice stages under adaptive charts (a match's "Charts:
//! Adaptive", `ddi_engine::ladder`) as the desktop window plays them, on
//! this machine's GPU offscreen and a fake clock with no sound device.
//! Down: the player presses nothing, so their life runs out and they step
//! down the song's ladder (the status line says so, their field clears
//! and the easier chart's notes come in from the boundary); the weak CPUs
//! step down too, their miniatures drawing the chart they are on; the
//! standings mark who finished on which chart. Up: the player, autoplayed
//! from Medium, and strong CPUs climb above it (the cue, the old chart to
//! the boundary, a CPU two rungs up, the standings' ↑). The CPUs' seeds
//! are fixed (a practice room's come from the time it was made: two runs
//! of the same test were two different sets of CPUs). Skipped where there
//! is no GPU adapter (CI). `DDI_ROOM_SHOTS=<dir>` keeps pictures (PPM).

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

/// A practice stage's offscreen window: its GPU, its UI and its host on
/// a fake clock, the stage picked and started.
struct Rig {
    device: wgpu::Device,
    queue: wgpu::Queue,
    ui_gpu: Gpu,
    ui: ddi_ui::Ui,
    env: ddi_ui::screens::Env,
    host: Host,
    store: Rc<MemoryPlayStore>,
    clock: Arc<AtomicU64>,
    formats: Formats,
    texture: wgpu::Texture,
    srgb: wgpu::TextureView,
    plain: wgpu::TextureView,
}

/// The stage on Beatucada's chart `chart` (as `difficulty`) against
/// `cpus` CPUs at `level` (from 0) seeded from `seed`, attacks on, the
/// charts adaptive, the player autoplayed where `auto`. `None` without a
/// GPU adapter.
fn rig(chart: usize, difficulty: Difficulty, cpus: u16, level: u8, seed: u64, auto: bool) -> Option<Rig> {
    rig_with("beatucada", chart, difficulty, None, cpus, level, seed, auto)
}

/// [`rig`] with a roster of CPUs at `roster`'s levels (from 0) instead,
/// where it names one.
#[allow(clippy::too_many_arguments)]
fn rig_with(
    id: &str,
    chart: usize,
    difficulty: Difficulty,
    roster: Option<&[u8]>,
    cpus: u16,
    level: u8,
    seed: u64,
    auto: bool,
) -> Option<Rig> {
    let cpus = roster.map_or(cpus, |r| r.len() as u16);
    common::indexes();
    let instance = gpu::instance();
    let Some(adapter) = gpu::adapter(&instance, None) else {
        eprintln!("no GPU adapter: skipped");
        return None;
    };
    let (device, queue) = gpu::device(&adapter).unwrap();
    // The dancing hand beside the stage (a player's choice for
    // multiplayer, Off by default): what follows checks it plays on.
    ddi_ui::state::settings::Settings {
        dancing_hand_multiplayer: true,
        ..Default::default()
    }
    .save();
    let (ui_device, ui_queue) = gpu::device(&adapter).unwrap();
    let ui_gpu = Gpu::new(ui_device, ui_queue, wgpu::TextureFormat::Rgba8Unorm);
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
    ddi_desktop::room::discard();
    let room = ddi_desktop::room::current();
    room.borrow_mut().set_rules_seen(true);
    ui.push(ddi_desktop::room::room_screen_with(
        &env,
        None,
        None,
        Some(host.requested.clone()),
    ));
    ui.update(0.5);
    let song = ddi_desktop::play::load::read_chart(id).unwrap();
    let entry = SongEntry {
        id: id.into(),
        title: song.title.clone(),
        charts: vec![ChartEntry {
            style: "dance-single".into(),
            difficulty,
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
            seats: cpus + 1,
            charts: ddi_room::Charts::Adaptive,
            ..r.rules().clone()
        };
        rules.set_attacks(true);
        rules.cpu.mode = ddi_room::CpuMode::Fixed { count: cpus };
        rules.cpu.roster.clear();
        if let Some(levels) = roster {
            rules.cpu.mode = ddi_room::CpuMode::Roster;
            rules.cpu.roster = levels
                .iter()
                .enumerate()
                .map(|(i, &level)| ddi_room::OpponentSpec {
                    level,
                    seed: seed + i as u64,
                    personality: None,
                    name: Some(format!("Level {}", level + 1)),
                    targeting: None,
                })
                .collect();
        }
        rules.cpu.opponent.level = level;
        // The same CPUs on every run (a practice room's seed is the time
        // it was made).
        rules.cpu.opponent.seed = seed;
        r.save_rules(rules).unwrap();
    }
    room.borrow_mut().pick(&entry, &entry.charts[0]).unwrap();
    ui.update(0.1);
    let mut request = host.requested.borrow_mut().take().expect("asked for");
    assert!(
        request.stage.as_ref().is_some_and(|s| s.rules.adaptive()),
        "the stage's charts adapt"
    );
    request.auto = auto;
    *host.requested.borrow_mut() = Some(request);
    let mut rig = Rig {
        device,
        queue,
        ui_gpu,
        ui,
        env,
        host,
        store,
        clock,
        formats,
        texture,
        srgb,
        plain,
    };
    let mut waited = 0;
    while !rig.host.prompt_ready() {
        rig.poll();
        rig.ui.update(0.05);
        std::thread::sleep(std::time::Duration::from_millis(20));
        waited += 1;
        assert!(waited < 1000, "the song was never read");
    }
    rig.ui.update(0.5);
    key(&mut rig.ui, "Enter", "Enter");
    rig.ui.update(0.05);
    {
        let mut r = room.borrow_mut();
        let later = r.now_ms() + 3_000;
        r.tick(later);
    }
    rig.ui.update(0.05);
    rig.poll();
    assert!(rig.host.playing(), "{:?}", rig.ui.layers());
    Some(rig)
}

impl Rig {
    fn poll(&mut self) {
        let dev = Device {
            device: &self.device,
            queue: &self.queue,
            formats: self.formats,
        };
        self.host.poll(&mut self.ui, &self.env, Some(dev), 48_000);
    }

    /// One frame at eight a second on the fake clock: its host second.
    fn frame(&mut self) -> f64 {
        let now = self.clock.fetch_add(1_000_000_000 / 8, Ordering::SeqCst) + 1_000_000_000 / 8;
        let mut encoder = self.device.create_command_encoder(&Default::default());
        let dev = Device {
            device: &self.device,
            queue: &self.queue,
            formats: self.formats,
        };
        self.host.draw_play(
            &dev,
            &mut encoder,
            &self.srgb,
            Some(&self.plain),
            SIZE,
            HostTime(now as f64 / 1e9),
            &mut self.ui,
            &self.env,
        );
        self.queue.submit(Some(encoder.finish()));
        now as f64 / 1e9
    }

    fn read(&self) -> Vec<u8> {
        let _ = self.device.poll(wgpu::PollType::wait_indefinitely());
        gpu::read_rgba(&self.device, &self.queue, &self.texture)
    }
}

#[test]
fn a_practice_stage_steps_down_the_player_and_a_cpu() {
    // Beatucada's Hard chart: its ladder is Medium, Easy, Beginner below.
    let song = ddi_desktop::play::load::read_chart("beatucada").unwrap();
    assert_eq!(ddi_engine::ladder::below(&song, 5), vec![4, 3, 1]);
    // Three of the weakest CPUs: they step down on Hard.
    let Some(mut rig) = rig(5, Difficulty::Hard, 3, 0, 7, false) else {
        return;
    };
    let mut frames = 0;
    let mut seen_cue = false;
    let (mut shot_breather, mut shot_new, mut shot_cpu) = (false, false, false);
    let mut cpu_stepped = false;
    let mut player_steps = 0;
    let mut hand_seen = false;
    // What the CPUs did, frame by frame: the same on every run.
    let mut trace: Vec<String> = Vec::new();
    while rig.host.playing() {
        let now = rig.frame();
        let mut shots: Vec<&str> = Vec::new();
        if let Some(p) = rig.host.running() {
            let s = &p.session;
            let steps = s.player.steps().to_vec();
            player_steps = steps.len();
            let t = s.player.judge().last_time().unwrap_or(0.0);
            if let Some(b) = s.battle() {
                if frames % 40 == 0 {
                    let power: Vec<String> = b
                        .views()
                        .iter()
                        .map(|v| format!("{:?}/{:.3}", v.power, v.life))
                        .collect();
                    trace.push(format!("{frames}: {}", power.join(" ")));
                }
                // A CPU's field draws the chart it is on.
                if b.feeds().any(|f| f.steps().count() > 0) {
                    cpu_stepped = true;
                    if !shot_cpu && b.feeds().any(|f| f.steps().any(|st| t > st.1 + 1.0)) {
                        shot_cpu = true;
                        shots.push("step-cpu-field");
                    }
                }
            }
            if let Some(st) = steps.first() {
                if let Some((ddi_session::announcer::battle::StageCue::Step { rung: 1, up: false }, _)) =
                    s.stage_cue(now)
                {
                    seen_cue = true;
                    if !shot_breather && t > st.at + 0.5 {
                        shot_breather = true;
                        // The old chart's notes gone from the field.
                        let shown = s.frame(HostTime(now));
                        assert!(
                            shown
                                .notes
                                .iter()
                                .all(|n| n.note_index >= ddi_engine::battle::HAZARD_INDEX_BASE
                                    || f64::from(n.beat) >= st.tick.beat() - 1e-6),
                            "only the new chart's notes and shocks in the breather"
                        );
                        shots.push("step-breather-cue");
                    }
                }
                if !shot_new && t > st.boundary + 0.6 {
                    shot_new = true;
                    shots.push("step-new-chart");
                }
            }
        }
        for name in shots {
            ppm(name, &rig.read());
        }
        // The dancing hand plays out what it was given at the step and
        // rests: it never stops (a stopped hand hides for the play).
        if let Some(d) = rig.host.hand_debug() {
            hand_seen |= d["status"] == "active";
            assert!(
                d["runtime_error"].is_null(),
                "the hand stopped at frame {frames}: {}",
                d["runtime"]
            );
        }
        if frames % 16 == 0 {
            let _ = rig.device.poll(wgpu::PollType::wait_indefinitely());
        }
        frames += 1;
        assert!(frames < 8 * 400, "the play never ended");
    }
    let _ = rig.device.poll(wgpu::PollType::wait_indefinitely());
    eprintln!("the CPUs, every 40 frames:\n{}", trace.join("\n"));
    let ui = &mut rig.ui;
    ui.update(0.05);
    assert_eq!(top(ui), "results");
    assert!(player_steps >= 1, "the player stepped down");
    assert!(seen_cue, "the status line said it");
    assert!(shot_breather && shot_new, "{shot_breather} {shot_new}");
    assert!(cpu_stepped, "a CPU stepped");
    assert!(hand_seen, "the dancing hand played beside the stage");
    // The standings mark who finished on which chart.
    ui.update(1.0);
    let view = rig.env.state.borrow().stage.clone().expect("the standings");
    let me = view.rows.iter().find(|r| r.me).expect("the player's line");
    assert!(me.stepped.as_deref().is_some_and(|s| s.starts_with('↓')), "{view:?}");
    assert!(
        view.rows.iter().filter(|r| r.stepped.is_some()).count() >= 2,
        "{view:?}"
    );
    ui.update(0.5);
    ppm(
        "step-standings",
        &rig.ui_gpu.draw_offscreen(ui, SIZE, wgpu::TextureFormat::Rgba8Unorm),
    );
    assert!(rig.store.is_empty(), "a stage is not kept in the history");
}

/// The player, autoplayed from Medium, steps up (the cue says so, the old
/// chart is played to the boundary, the harder one's notes come in from
/// it); the strongest CPUs climb too, one at least two rungs; the
/// standings mark the charts finished on with ↑.
#[test]
fn a_practice_stage_steps_up_the_player_and_a_cpu_climbs_two_rungs() {
    let song = ddi_desktop::play::load::read_chart("beatucada").unwrap();
    let medium = song
        .charts
        .iter()
        .position(|c| c.layout == "dance-single" && c.difficulty == ddi_chart::Difficulty::Medium)
        .unwrap();
    assert!(ddi_engine::ladder::above(&song, medium).len() >= 2);
    let Some(mut rig) = rig(medium, Difficulty::Medium, 3, 9, 7, true) else {
        return;
    };
    let mut frames = 0;
    let mut seen_cue = false;
    let (mut shot_cue, mut shot_new, mut shot_cpu) = (false, false, false);
    let mut player_up = 0;
    let mut two_rungs = false;
    while rig.host.playing() {
        let now = rig.frame();
        let mut shots: Vec<&str> = Vec::new();
        if let Some(p) = rig.host.running() {
            let s = &p.session;
            let steps = s.player.steps().to_vec();
            player_up = steps.iter().filter(|s| s.up).count();
            let t = s.player.judge().last_time().unwrap_or(0.0);
            if let Some(b) = s.battle() {
                for i in 0..b.views().len() {
                    let Some(cpu) = b.opponent_player(i) else {
                        continue;
                    };
                    if cpu.steps().iter().filter(|s| s.up).count() >= 2 {
                        two_rungs = true;
                        let last = cpu.steps().last().unwrap();
                        if !shot_cpu && t > last.boundary + 1.0 {
                            shot_cpu = true;
                            shots.push("climb-cpu-two-rungs");
                        }
                    }
                }
            }
            if let Some(st) = steps.first() {
                assert!(st.up, "a perfect play only climbs: {steps:?}");
                if let Some((ddi_session::announcer::battle::StageCue::Step { up: true, .. }, _)) =
                    s.stage_cue(now)
                {
                    seen_cue = true;
                    if !shot_cue && t > st.at + 0.5 {
                        shot_cue = true;
                        // Up to the boundary the old chart's notes, from it
                        // the harder one's.
                        let shown = s.frame(HostTime(now));
                        assert!(
                            shown.notes.iter().any(|n| n.note_index < ddi_engine::battle::HAZARD_INDEX_BASE),
                            "the old chart still plays"
                        );
                        shots.push("climb-cue");
                    }
                }
                if !shot_new && t > st.boundary + 0.6 {
                    shot_new = true;
                    shots.push("climb-new-chart");
                }
            }
        }
        for name in shots {
            ppm(name, &rig.read());
        }
        if frames % 16 == 0 {
            let _ = rig.device.poll(wgpu::PollType::wait_indefinitely());
        }
        frames += 1;
        assert!(frames < 8 * 400, "the play never ended");
    }
    let _ = rig.device.poll(wgpu::PollType::wait_indefinitely());
    let ui = &mut rig.ui;
    ui.update(0.05);
    assert_eq!(top(ui), "results");
    assert!(player_up >= 1, "the player stepped up");
    assert!(seen_cue, "the status line said it");
    assert!(shot_cue && shot_new, "{shot_cue} {shot_new}");
    assert!(two_rungs, "a CPU climbed two rungs");
    ui.update(1.0);
    let view = rig.env.state.borrow().stage.clone().expect("the standings");
    let me = view.rows.iter().find(|r| r.me).expect("the player's line");
    assert!(me.stepped.as_deref().is_some_and(|s| s.starts_with('↑')), "{view:?}");
    ui.update(0.5);
    ppm(
        "climb-standings",
        &rig.ui_gpu.draw_offscreen(ui, SIZE, wgpu::TextureFormat::Rgba8Unorm),
    );
}

/// `cpu-levels-v1` (absolute levels): under Adaptive charts from Some
/// Things Must's Medium, CPUs of levels 2, 5 and 9 end on different charts
/// (the weak one below the pick, the strong one above), where under
/// `cpu-levels-v0` every level from about 4 climbed alike. The
/// standings say where each finished.
#[test]
fn absolute_levels_end_a_stage_on_different_charts() {
    const SONG: &str = "some-things-must";
    let song = ddi_desktop::play::load::read_chart(SONG).unwrap();
    let medium = song
        .charts
        .iter()
        .position(|c| c.layout == "dance-single" && c.difficulty == ddi_chart::Difficulty::Medium)
        .unwrap();
    let Some(mut rig) = rig_with(SONG, medium, Difficulty::Medium, Some(&[1, 4, 8]), 0, 0, 7, true)
    else {
        return;
    };
    let mut frames = 0;
    let mut finals: Vec<Option<usize>> = Vec::new();
    let mut shot = false;
    let mut apart: Option<f64> = None;
    while rig.host.playing() {
        rig.frame();
        if let Some(b) = rig.host.running().and_then(|p| p.session.battle()) {
            finals = (0..b.views().len())
                .filter_map(|i| b.opponent_player(i))
                .map(|cpu| cpu.steps().last().map(|s| s.chart))
                .collect();
            // A picture once the three play three different charts, a
            // few seconds on.
            let now: std::collections::BTreeSet<usize> =
                finals.iter().map(|f| f.unwrap_or(medium)).collect();
            let t = rig
                .host
                .running()
                .and_then(|p| p.session.player.judge().last_time())
                .unwrap_or(0.0);
            if now.len() < 3 {
                apart = None;
            } else if apart.is_none() {
                apart = Some(t);
            }
            if !shot && apart.is_some_and(|a| t > a + 4.0) {
                shot = true;
                ppm("levels-on-their-rungs", &rig.read());
            }
        }
        if frames % 16 == 0 {
            let _ = rig.device.poll(wgpu::PollType::wait_indefinitely());
        }
        frames += 1;
        assert!(frames < 8 * 400, "the play never ended");
    }
    let _ = rig.device.poll(wgpu::PollType::wait_indefinitely());
    // Where each ended: the chart of its last step, the picked one without.
    let ended: Vec<usize> = finals.iter().map(|f| f.unwrap_or(medium)).collect();
    assert_eq!(ended.len(), 3, "{finals:?}");
    let level = |c: usize| song.charts[c].difficulty;
    assert!(
        level(ended[0]) < level(ended[2]),
        "level 2 ended below level 9: {:?}",
        ended.iter().map(|&c| level(c)).collect::<Vec<_>>()
    );
    let distinct: std::collections::BTreeSet<usize> = ended.iter().copied().collect();
    assert_eq!(distinct.len(), 3, "{ended:?}");
    let ui = &mut rig.ui;
    ui.update(0.05);
    assert_eq!(top(ui), "results");
    ui.update(1.0);
    let view = rig.env.state.borrow().stage.clone().expect("the standings");
    eprintln!(
        "ended on {:?}; standings {:?}",
        ended.iter().map(|&c| level(c)).collect::<Vec<_>>(),
        view.rows.iter().map(|r| (r.name.clone(), r.stepped.clone())).collect::<Vec<_>>()
    );
    ui.update(0.5);
    ppm(
        "levels-standings",
        &rig.ui_gpu.draw_offscreen(ui, SIZE, wgpu::TextureFormat::Rgba8Unorm),
    );
}
