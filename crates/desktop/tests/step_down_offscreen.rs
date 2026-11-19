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
//! is no GPU adapter (CI). `DDI_OFFSCREEN_PPM=<dir>` keeps pictures (PPM).

mod common;
use std::rc::Rc;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

use ddi_desktop::gpu::{self, Gpu};
use ddi_desktop::play::host::{Device, Host};
use ddi_desktop::play::view::Formats;
use ddi_platform::HostTime;
use ddi_platform::history::MemoryPlayStore;
use ddi_ui::screens::data::Difficulty;
use ddi_ui::state::history::Plays;
use common::ui::top;

const SIZE: [u32; 2] = [1280, 720];

fn ppm(name: &str, pixels: &[u8]) {
    common::shot(&format!("{name}.ppm"), pixels, SIZE);
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
    target: common::gpu::Target,
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
    let adapter = common::gpu::adapter_or_skip()?;
    let (device, queue) = gpu::device(&adapter).unwrap();
    // The dancing hand beside the stage (a player's choice for
    // multiplayer, Off by default): what follows checks it plays on.
    // No announcer: the status line says the steps (with one on, as by
    // default since 2026-11-17, its voice says them instead).
    ddi_ui::state::settings::Settings {
        dancing_hand_multiplayer: true,
        announcer: None,
        ..Default::default()
    }
    .save();
    let (ui_device, ui_queue) = gpu::device(&adapter).unwrap();
    let ui_gpu = Gpu::new(ui_device, ui_queue, wgpu::TextureFormat::Rgba8Unorm);
    let (mut ui, env) = common::ui::theme("start", common::ui::lowercase(), SIZE, true);
    let store = Rc::new(MemoryPlayStore::default());
    let mut host = Host::new(Rc::default(), Some(Plays::new(store.clone())));
    let clock = common::rig::fake_clock(&mut host);
    // The hand's work budget on a clock of its own (half a microsecond a
    // reading, as the shell's hand tests): the play's frames run faster
    // than real time, and a loaded machine must not starve its planner.
    let ticks = std::cell::Cell::new(0.0);
    host.test_seconds = Some(Rc::new(move || {
        let t = ticks.get();
        ticks.set(t + 0.000_000_5);
        t
    }));
    let formats = common::gpu::formats();
    let target = common::gpu::Target::new(&device, SIZE);
    ddi_desktop::room::discard();
    let room = common::practice::open(&mut ui, &env, &host);
    let song = ddi_desktop::play::load::read_chart(id).unwrap();
    let entry = common::practice::song_entry(id, &song.title, chart, difficulty);
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
        target,
    };
    let mut waited = 0;
    while !rig.host.prompt_ready() {
        rig.poll();
        rig.ui.update(common::rig::WAIT_STEP);
        std::thread::sleep(std::time::Duration::from_millis(20));
        waited += 1;
        assert!(waited < 1000, "the song was never read");
    }
    let (device, queue, formats) = (&rig.device, &rig.queue, rig.formats);
    common::rig::ready_and_start(&mut rig.host, &mut rig.ui, &rig.env, &room, || Device {
        device,
        queue,
        formats,
    });
    assert!(rig.host.playing(), "{:?}", rig.ui.layers());
    Some(rig)
}

/// The chart line's pictures and checks for the player's first step
/// (feedback round J): the stage's chart named until the step's boundary
/// (decided, not there yet), the new chart's name bright at it, muted
/// after its emphasis. `taken`: which of the three were taken; `stage`
/// and `new`: the two charts as song select names them; `t`, the judged
/// second; `name`, the pictures' prefix.
fn chart_line_shots(
    host: &Host,
    step: &ddi_engine::ladder::Step,
    t: f64,
    (stage, new): (&str, &str),
    taken: &mut [bool; 3],
    name: &str,
) -> Option<String> {
    let line = host.chart_drawn();
    let text = line.as_ref().map(|l| l.0.as_str());
    if !taken[0] && t > step.at + 0.2 && t < step.boundary - 0.3 {
        taken[0] = true;
        assert_eq!(text, Some(stage), "the stage's chart until the boundary");
        assert_eq!(line.as_ref().map(|l| l.1), Some(0.0), "at rest");
        return Some(format!("{name}-before"));
    }
    if !taken[1] && t > step.boundary + 0.05 && t < step.boundary + 0.5 {
        taken[1] = true;
        assert_eq!(text, Some(new), "the new chart from its boundary");
        assert!(line.as_ref().is_some_and(|l| l.1 > 0.5), "bright: {line:?}");
        return Some(format!("{name}-at"));
    }
    if taken[1] && !taken[2] && t > step.boundary + ddi_shell::battle_cue::CHART_EMPHASIS + 0.5 {
        taken[2] = true;
        assert_eq!(text, Some(new));
        assert_eq!(line.as_ref().map(|l| l.1), Some(0.0), "muted again");
        return Some(format!("{name}-after"));
    }
    None
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
        let dev = Device {
            device: &self.device,
            queue: &self.queue,
            formats: self.formats,
        };
        common::rig::draw(&mut self.host, &dev, &self.target, &mut self.ui, &self.env, now);
        now as f64 / 1e9
    }

    fn read(&self) -> Vec<u8> {
        self.target.read(&self.device, &self.queue)
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
    let mut chart_shots = [false; 3];
    let mut chart_shot_names: Vec<String> = Vec::new();
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
                let names = (
                    ddi_session::battle::chart_shown(&song, 5),
                    ddi_session::battle::chart_shown(&song, st.chart),
                );
                if let Some(shot) =
                    chart_line_shots(&rig.host, st, t, (&names.0, &names.1), &mut chart_shots, "chart-line-down")
                {
                    chart_shot_names.push(shot);
                }
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
        for name in shots.into_iter().map(str::to_string).chain(chart_shot_names.drain(..)) {
            ppm(&name, &rig.read());
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
    assert_eq!(chart_shots, [true; 3], "the chart line before, at and after the step");
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
    let mut chart_shots = [false; 3];
    let mut chart_shot_names: Vec<String> = Vec::new();
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
                let names = (
                    ddi_session::battle::chart_shown(&song, medium),
                    ddi_session::battle::chart_shown(&song, st.chart),
                );
                if let Some(shot) =
                    chart_line_shots(&rig.host, st, t, (&names.0, &names.1), &mut chart_shots, "chart-line-up")
                {
                    chart_shot_names.push(shot);
                }
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
        for name in shots.into_iter().map(str::to_string).chain(chart_shot_names.drain(..)) {
            ppm(&name, &rig.read());
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
    assert_eq!(chart_shots, [true; 3], "the chart line before, at and after the step");
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

/// Absolute levels (`cpu-levels-v1`, and `cpu-levels-v2` since round V): under Adaptive charts from Some
/// Things Must's Medium, CPUs of levels 1, 5 and 9 end on different charts
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
    let Some(mut rig) = rig_with(SONG, medium, Difficulty::Medium, Some(&[0, 4, 8]), 0, 0, 7, true)
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
        "level 1 ended below level 9: {:?}",
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
