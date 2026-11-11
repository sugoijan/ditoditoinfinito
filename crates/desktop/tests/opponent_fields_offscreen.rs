//! A CPU-practice stage's opponents in miniature
//! (`docs/plans/multiplayer-prototype-progress.md`, M6), as the desktop
//! window draws them, on this machine's GPU offscreen and a fake clock
//! with no sound device: eight CPUs beside a play each get a miniature
//! field in a stable slot, drawn by the play's own skin, at 1280×720 and
//! on a phone held upright (the list that stood in there is gone).
//! Skipped where there is no GPU adapter (CI).
//!
//! `cargo test -p ddi-desktop --test opponent_fields_offscreen -- --ignored
//! --nocapture` (under the watchdog, a scratch `DDI_HOME`) plays the
//! look's scenarios instead, keeping their pictures in
//! `DDI_FIELDS_SHOTS=<dir>` (PPM), and measures a frame's cost with the
//! CPUs in miniature, up to 128 CPUs in the development-only practice
//! beyond the cap (`room::develop`).

mod common;
use std::rc::Rc;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Instant;

use ddi_desktop::gpu;
use ddi_desktop::play::host::{Device, Host};
use ddi_desktop::play::view::Formats;
use ddi_platform::HostTime;
use ddi_room::CpuMode;
use ddi_ui::event::{KeyEvent, UiEvent};
use ddi_ui::screens::data::{ChartEntry, Difficulty, SongEntry};
use ddi_ui::state::settings::Settings;

fn key(ui: &mut ddi_ui::Ui, code: &str, k: &str) {
    ui.handle(UiEvent::Key(KeyEvent::down(code, k)));
    ui.handle(UiEvent::Key(KeyEvent::up(code, k)));
}

fn ppm(size: [u32; 2], name: &str, pixels: &[u8]) {
    let Some(dir) = std::env::var_os("DDI_FIELDS_SHOTS") else {
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

/// One play of a stage.
struct Scenario {
    name: &'static str,
    size: [u32; 2],
    cpus: u16,
    /// The CPUs' level, 0 the weakest.
    level: u8,
    /// A bundled skin's key (`None`: Infinito, the default).
    skin: Option<&'static str>,
    /// A battery of this many lives instead of the ruleset's gauge (the
    /// CPUs' too: some go out).
    lives: Option<u8>,
    /// The dancing hand plays (on by default; it keeps its side).
    hand: bool,
    chart: usize,
    /// Song seconds pictures are kept at.
    shots: &'static [f64],
    /// Measures frames from this song second.
    measure: Option<f64>,
    /// `.lrc` lyrics over the play.
    lyrics: bool,
}

/// A frame's cost, milliseconds: recording it, and recording it with
/// the GPU's work done.
#[derive(Debug, Default)]
struct Cost {
    cpu: Vec<f64>,
    total: Vec<f64>,
}

impl Cost {
    fn line(&self) -> String {
        let stat = |v: &[f64]| {
            let mut v = v.to_vec();
            v.sort_by(f64::total_cmp);
            let mean = v.iter().sum::<f64>() / v.len().max(1) as f64;
            let p95 = v.get(v.len() * 95 / 100).copied().unwrap_or(0.0);
            format!("mean {mean:.2} p95 {p95:.2}")
        };
        format!(
            "{} frames: record {}; with GPU {}",
            self.cpu.len(),
            stat(&self.cpu),
            stat(&self.total)
        )
    }
}

struct Played {
    slots: usize,
    /// The miniatures' lane, logical pixels, and whether they took turns.
    lane: f32,
    rotating: bool,
    out: usize,
    cost: Cost,
    /// Lyrics were drawn in a frame that drew the miniatures.
    lyrics: bool,
}

fn play(s: &Scenario) -> Option<Played> {
    common::indexes();
    let instance = gpu::instance();
    let Some(adapter) = gpu::adapter(&instance, None) else {
        eprintln!("no GPU adapter: skipped");
        return None;
    };
    let size = s.size;
    let (device, queue) = gpu::device(&adapter).unwrap();
    let mut settings = Settings {
        noteskin: s.skin.map(str::to_string),
        ..Settings::default()
    };
    settings.dancing_hand = s.hand;
    if let Some(lives) = s.lives {
        settings.stepmania.life_type = ddi_engine::rules::LifeType::Battery;
        settings.stepmania.lives = lives;
    }
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

    // A fresh practice room each play: beyond the cap, the
    // development-only one.
    ddi_desktop::room::develop(if s.cpus > 8 { s.cpus } else { 0 });
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
            difficulty: Difficulty::Challenge,
            index: s.chart,
            digest: Some(song.chart_digest(s.chart)),
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
        rules.seats = rules.seats.max(s.cpus + 1);
        rules.cpu.mode = CpuMode::Fixed { count: s.cpus };
        rules.cpu.opponent.level = s.level;
        r.save_rules(rules).unwrap();
        r.pick(&entry, &entry.charts[0]).unwrap();
    }
    // The room's song select prepares it and opens its prompt.
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
    // Ready; the room counts down and starts the stage.
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

    let mut played = Played {
        slots: 0,
        lane: 0.0,
        rotating: false,
        out: 0,
        cost: Cost::default(),
        lyrics: false,
    };
    if s.lyrics {
        let mut with = song.clone();
        ddi_chart::formats::lrc::add_lyrics(&mut with, "[00:02.00]A line of lyrics over the stage");
        host.set_lyrics(Some(ddi_engine::lyrics::LyricTrack::new(
            &with, s.chart, false, true,
        )));
    }
    let mut shots = s.shots.iter().copied().peekable();
    let measuring = 240;
    let mut measured = 0;
    let mut frames = 0;
    let last = s
        .shots
        .iter()
        .copied()
        .chain(s.measure.map(|m| m + 10.0))
        .fold(0.0, f64::max);
    while host.playing() {
        let host_now = HostTime(clock.load(Ordering::SeqCst) as f64 / 1e9);
        let song_t = host
            .running()
            .map_or(0.0, |p| p.session.player.judge_time(host_now));
        let in_measure = s.measure.is_some_and(|m| song_t >= m) && measured < measuring;
        // Coarse steps between what is looked at, a 60th close to it.
        let near = shots.peek().is_some_and(|&t| t - song_t < 2.0) || in_measure;
        let step = if near { 1e9 / 60.0 } else { 1e9 / 10.0 } as u64;
        let now = clock.fetch_add(step, Ordering::SeqCst) + step;
        let start = Instant::now();
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
        let recorded = start.elapsed().as_secs_f64() * 1000.0;
        queue.submit(Some(encoder.finish()));
        if in_measure {
            let _ = device.poll(wgpu::PollType::wait_indefinitely());
            let total = start.elapsed().as_secs_f64() * 1000.0;
            // The first frames settle (pipelines, uploads).
            if measured >= 20 {
                played.cost.cpu.push(recorded);
                played.cost.total.push(total);
            }
            measured += 1;
        } else if frames % 16 == 0 {
            let _ = device.poll(wgpu::PollType::wait_indefinitely());
        }
        if let Some(f) = host.opponent_fields() {
            played.slots = played.slots.max(f.slots.len());
            played.lane = f.lane;
            played.rotating = f.rotating;
            played.lyrics |= !host.lyrics_shown().is_empty();
        }
        if let Some(p) = host.running() {
            played.out = p.session.opponents().iter().filter(|o| o.out).count();
        }
        if shots.peek().is_some_and(|&t| song_t >= t) {
            let t = shots.next().unwrap();
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
                &mut ui,
                &env,
            );
            queue.submit(Some(encoder.finish()));
            let _ = device.poll(wgpu::PollType::wait_indefinitely());
            ppm(
                size,
                &format!("{}-at-{t:.0}s", s.name),
                &gpu::read_rgba(&device, &queue, &texture),
            );
            eprintln!(
                "{} at {t:.0} s: {} slots, {} out",
                s.name, played.slots, played.out
            );
        }
        frames += 1;
        if song_t > last + 0.5 && shots.peek().is_none() {
            break;
        }
        assert!(frames < 60 * 400, "the play never ended");
    }
    let _ = device.poll(wgpu::PollType::wait_indefinitely());
    Some(played)
}

#[test]
fn eight_cpus_get_miniature_fields_beside_the_play() {
    let Some(played) = play(&Scenario {
        name: "eight",
        size: [1280, 720],
        cpus: 8,
        level: 7,
        skin: None,
        lives: None,
        hand: true,
        chart: 0,
        shots: &[12.0],
        measure: None,
        lyrics: false,
    }) else {
        return;
    };
    assert_eq!(played.slots, 8);
    assert!(!played.rotating);
    // In the play's field's proportion, ten lanes tall, two rows of two a
    // side: lanes of 28 (42 on fields six lanes tall before part C of
    // the feedback round, 32 on seven before the round).
    assert_eq!(played.lane, 28.0);
}

#[test]
fn a_phone_held_upright_shows_all_eight_in_its_margins() {
    let Some(played) = play(&Scenario {
        name: "eight-upright",
        size: [390, 844],
        cpus: 8,
        level: 7,
        skin: None,
        lives: None,
        hand: true,
        chart: 0,
        shots: &[12.0],
        measure: None,
        lyrics: false,
    }) else {
        return;
    };
    assert_eq!(played.slots, 8);
    assert!(!played.rotating);
}

#[test]
fn lyrics_over_a_stage_keep_the_miniatures() {
    let Some(played) = play(&Scenario {
        name: "eight-lyrics",
        size: [1280, 720],
        cpus: 8,
        level: 7,
        skin: None,
        lives: None,
        hand: true,
        chart: 0,
        shots: &[12.0],
        measure: None,
        lyrics: true,
    }) else {
        return;
    };
    assert_eq!(played.slots, 8);
    assert!(played.lyrics, "the lyrics were drawn with the miniatures");
}

#[test]
#[ignore = "pictures and costs for the look's review"]
fn miniature_scenarios_pictures_and_costs() {
    let eight = |name, size, skin, hand| Scenario {
        name,
        size,
        cpus: 8,
        level: 7,
        skin,
        lives: None,
        hand,
        chart: 0,
        shots: &[20.0],
        measure: Some(22.0),
        lyrics: false,
    };
    // Two pictures a turn apart: the opponents taking turns in the slots
    // (`ddi_shell::opponents::ROTATION_V0`, eight seconds).
    let many = |name, cpus, size| Scenario {
        name,
        cpus,
        shots: &[12.0, 21.0],
        ..eight("", size, None, false)
    };
    let scenarios = [
        eight(
            "infinito-eight-cpus-hand-off-1280x720",
            [1280, 720],
            None,
            false,
        ),
        eight(
            "infinito-eight-cpus-hand-behind-field-1280x720",
            [1280, 720],
            None,
            true,
        ),
        eight(
            "infinito-eight-cpus-hand-off-1920x1080",
            [1920, 1080],
            None,
            false,
        ),
        eight(
            "infinito-eight-cpus-hand-behind-field-1920x1080",
            [1920, 1080],
            None,
            true,
        ),
        eight(
            "infinito-eight-cpus-hand-behind-field-844x390",
            [844, 390],
            None,
            true,
        ),
        eight(
            "infinito-eight-cpus-hand-behind-field-390x844",
            [390, 844],
            None,
            true,
        ),
        eight(
            "peters-ddr-eight-cpus-hand-off-1280x720",
            [1280, 720],
            Some(ddi_noteskin::bundled::PETERS_DDR),
            false,
        ),
        Scenario {
            name: "infinito-two-cpus-hand-off-1280x720",
            cpus: 2,
            measure: None,
            ..eight("", [1280, 720], None, false)
        },
        Scenario {
            name: "infinito-eight-weak-cpus-some-out-hand-off-1280x720",
            level: 0,
            lives: Some(2),
            shots: &[30.0, 60.0],
            measure: None,
            ..eight("", [1280, 720], None, false)
        },
        many("development-16-cpus-1280x720", 16, [1280, 720]),
        many("development-32-cpus-1280x720", 32, [1280, 720]),
        many("development-32-cpus-1920x1080", 32, [1920, 1080]),
        many("development-64-cpus-1920x1080", 64, [1920, 1080]),
        many("development-128-cpus-1920x1080", 128, [1920, 1080]),
        many("development-128-cpus-1280x720", 128, [1280, 720]),
    ];
    let only = std::env::var("DDI_FIELDS_ONLY").ok();
    for s in &scenarios {
        if only.as_deref().is_some_and(|o| !s.name.contains(o)) {
            continue;
        }
        let Some(p) = play(s) else {
            return;
        };
        eprintln!(
            "{}: {} CPUs, {} slots, {} out at the end",
            s.name, s.cpus, p.slots, p.out
        );
        if s.measure.is_some() {
            eprintln!("  miniatures: {}", p.cost.line());
        }
    }
}
