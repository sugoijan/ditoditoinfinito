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
    /// The match's Attacks rule (practice's default, On).
    attacks: bool,
    /// Options' "Up to eight" opponents' fields.
    up_to_eight: bool,
    /// A picture just after the first hold a CPU lets go (the full
    /// chart, `cpu-play-v1`).
    letgo_shot: bool,
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
    /// A large field's ladder as last drawn (its words), and the seats
    /// its slots showed.
    ladder: Option<String>,
    seats: Vec<usize>,
    /// The most miniatures drawn in a frame (each a full feed's frame).
    drawn: usize,
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
    // (The hand plays in a stage only where chosen for multiplayer.)
    settings.dancing_hand_multiplayer = s.hand;
    if s.up_to_eight {
        settings.opponent_fields = ddi_ui::state::settings::OpponentFields::UpToEight;
    }
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

    // A fresh practice room each play: beyond the cap (73 since
    // 2026-11-13, 72 CPUs beside the player), the development-only one.
    ddi_desktop::room::develop(if s.cpus > 72 { s.cpus } else { 0 });
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
        // Survival alone (the Attacks rule, On by default in practice,
        // turned off): the sizes these scenarios are about are the
        // miniatures' own, without the attack warning's place kept
        // clear beside the field (with it, eight at 1280×720 have lanes
        // of 25, not 28).
        rules.set_attacks(s.attacks);
        r.save_rules(rules).unwrap();
        r.pick(&entry, &entry.charts[0]).unwrap();
    }
    // The room's song select prepares it and opens its prompt.
    ui.update(0.1);
    let mut request = host.requested.borrow_mut().take().expect("the stage");
    request.auto = true;
    *host.requested.borrow_mut() = Some(request);
    let mut waited = 0;
    let preparing = Instant::now();
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
    eprintln!(
        "{}: the stage prepared ({} CPUs' plans) in {:.2} s",
        s.name,
        s.cpus,
        preparing.elapsed().as_secs_f64()
    );
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
        ladder: None,
        seats: Vec::new(),
        drawn: 0,
    };
    if s.lyrics {
        let mut with = song.clone();
        ddi_chart::formats::lrc::add_lyrics(&mut with, "[00:02.00]A line of lyrics over the stage");
        host.set_lyrics(Some(ddi_engine::lyrics::LyricTrack::new(
            &with, s.chart, false, true,
        )));
    }
    let mut shots = s.shots.iter().copied().peekable();
    // The first hold a CPU let go, once seen: a picture a little after.
    let mut letgo: Option<f64> = None;
    let mut letgo_taken = !s.letgo_shot;
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
        played.drawn = played.drawn.max(host.miniatures_drawn());
        if let Some(f) = host.opponent_fields() {
            played.slots = played.slots.max(f.slots.len());
            played.lane = f.lane;
            played.rotating = f.rotating;
            played.seats = f.slots.iter().map(|s| s.seat).collect();
            played.ladder = host.ladder_drawn().or(played.ladder.take());
            played.lyrics |= !host.lyrics_shown().is_empty();
        }
        if let Some(p) = host.running() {
            played.out = p.session.opponents().iter().filter(|o| o.out).count();
        }
        if letgo.is_none()
            && !letgo_taken
            && let Some(at) = host.running().and_then(|p| p.session.battle()).and_then(|b| {
                b.feeds()
                    .flat_map(|f| f.moments())
                    .filter(|m| m.kind == ddi_session::battle::feed::MomentKind::HoldEnd(false))
                    .map(|m| m.at)
                    .reduce(f64::min)
            })
        {
            letgo = Some(at + 0.15);
            eprintln!("{}: a CPU let a hold go at {at:.2} s", s.name);
        }
        let letgo_due = !letgo_taken && letgo.is_some_and(|at| song_t >= at);
        if letgo_due || shots.peek().is_some_and(|&t| song_t >= t) {
            let t = if letgo_due {
                letgo_taken = true;
                letgo.unwrap()
            } else {
                shots.next().unwrap()
            };
            // The text's fonts are read on a thread: real time for them.
            std::thread::sleep(std::time::Duration::from_millis(200));
            let _ = device.poll(wgpu::PollType::wait_indefinitely());
            // The same frame drawn with the skins' quads in their own
            // order, then put in fewer batches (`ddi_render`'s
            // batching), then in their order again: the same pixels.
            let mut drawn = Vec::new();
            for batching in [false, true, false] {
                host.set_batching(batching);
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
                let runs = host.frame_stats().map_or(0, |f| f.quad_batches);
                drawn.push((gpu::read_rgba(&device, &queue, &texture), runs));
            }
            host.set_batching(true);
            assert!(drawn[0].0 == drawn[2].0, "{}: the frame drawn twice alike", s.name);
            assert!(
                drawn[1].0 == drawn[0].0,
                "{}: the batched frame differs ({} pixels)",
                s.name,
                drawn[0].0.iter().zip(&drawn[1].0).filter(|(a, b)| a != b).count()
            );
            assert!(drawn[1].1 <= drawn[0].1);
            eprintln!(
                "{} at {t:.0} s: the skins' quads in {} batches, {} in their own order",
                s.name, drawn[1].1, drawn[0].1
            );
            let what = if letgo_due { "letgo" } else { "at" };
            ppm(size, &format!("{}-{what}-{t:.0}s", s.name), &drawn[1].0);
            eprintln!(
                "{} at {t:.0} s: {} slots, {} out",
                s.name, played.slots, played.out
            );
        }
        frames += 1;
        if song_t > last + 0.5 && shots.peek().is_none() && letgo_taken {
            break;
        }
        assert!(frames < 60 * 400, "the play never ended");
    }
    let _ = device.poll(wgpu::PollType::wait_indefinitely());
    Some(played)
}

/// A room of 73 in practice (72 CPUs, the cap) at 1280×720: every
/// opponent a field of its own (the feedback round E1), each drawn from
/// its full feed, in seat order, and the ladder's words over the whole
/// field.
#[test]
fn seventy_two_cpus_each_get_a_field_and_the_ladder() {
    let Some(played) = play(&Scenario {
        name: "seventy-two",
        size: [1280, 720],
        cpus: 72,
        level: 7,
        skin: None,
        lives: None,
        hand: false,
        chart: 0,
        shots: &[8.0],
        measure: None,
        lyrics: false,
        attacks: false,
        up_to_eight: false,
        letgo_shot: false,
    }) else {
        return;
    };
    assert_eq!(played.slots, 72);
    assert!(!played.rotating);
    assert_eq!(played.drawn, 72, "a frame for every field");
    assert_eq!(played.lane, 11.0);
    let words = played.ladder.expect("the ladder");
    assert!(words.ends_with("of 73 · 73 still in"), "{words}");
    assert_eq!(played.seats, (0..72).collect::<Vec<_>>());
}

/// The same room with Options' "Up to eight": the eight slots a room of
/// nine has, those that matter to the player in them, and the ladder
/// over the whole field (`ddi_shell::rivals`), as before.
#[test]
fn seventy_two_cpus_up_to_eight_get_a_room_of_nines_slots() {
    let Some(played) = play(&Scenario {
        name: "seventy-two-up-to-eight",
        size: [1280, 720],
        cpus: 72,
        level: 7,
        skin: None,
        lives: None,
        hand: false,
        chart: 0,
        shots: &[8.0],
        measure: None,
        lyrics: false,
        attacks: false,
        up_to_eight: true,
        letgo_shot: false,
    }) else {
        return;
    };
    assert_eq!(played.slots, 8);
    assert!(played.rotating);
    assert_eq!(played.drawn, 8);
    let words = played.ladder.expect("the ladder");
    assert!(words.ends_with("of 73 · 73 still in"), "{words}");
    let mut seats = played.seats.clone();
    seats.sort_unstable();
    seats.dedup();
    assert_eq!(seats.len(), 8, "eight different rivals: {:?}", played.seats);
    assert!(seats.iter().all(|s| *s < 72));
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
        attacks: false,
        up_to_eight: false,
        letgo_shot: false,
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
        attacks: false,
        up_to_eight: false,
        letgo_shot: false,
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
        attacks: false,
        up_to_eight: false,
        letgo_shot: false,
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
        attacks: false,
        up_to_eight: false,
        letgo_shot: false,
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
        many("practice-16-cpus-1280x720", 16, [1280, 720]),
        many("practice-32-cpus-1280x720", 32, [1280, 720]),
        many("practice-32-cpus-1920x1080", 32, [1920, 1080]),
        many("practice-64-cpus-1920x1080", 64, [1920, 1080]),
        many("practice-72-cpus-1280x720", 72, [1280, 720]),
        many("practice-72-cpus-1920x1080", 72, [1920, 1080]),
        many("development-128-cpus-1920x1080", 128, [1920, 1080]),
        many("development-128-cpus-1280x720", 128, [1280, 720]),
        // The full chart on the miniatures (`cpu-play-v1`): Beatucada's
        // Easy, 34 holds, and its Challenge, holds and 132 mines, where a
        // middling level lets some holds go.
        Scenario {
            name: "holds-eight-cpus-easy-1280x720",
            chart: 3,
            shots: &[18.0, 30.0],
            measure: None,
            ..eight("", [1280, 720], None, false)
        },
        Scenario {
            name: "holds-eight-level-5-cpus-challenge-1280x720",
            chart: 0,
            level: 4,
            shots: &[20.0],
            measure: None,
            ..eight("", [1280, 720], None, false)
        },
        Scenario {
            name: "holds-letgo-eight-level-2-cpus-medium-1280x720",
            chart: 4,
            level: 1,
            shots: &[],
            measure: None,
            letgo_shot: true,
            ..eight("", [1280, 720], None, false)
        },
    ];
    // The feedback round E1's pictures: every opponent shown, attacks on
    // (practice's default), and the costs against "Up to eight".
    let all = |name, cpus, size| Scenario {
        name,
        cpus,
        attacks: true,
        shots: &[20.0],
        measure: Some(22.0),
        ..eight("", size, None, false)
    };
    let scenarios: Vec<Scenario> = scenarios
        .into_iter()
        .chain([
            all("all-12-cpus-1280x720", 12, [1280, 720]),
            all("all-24-cpus-1280x720", 24, [1280, 720]),
            all("all-36-cpus-1280x720", 36, [1280, 720]),
            all("all-72-cpus-1280x720", 72, [1280, 720]),
            all("all-12-cpus-1920x1080", 12, [1920, 1080]),
            all("all-24-cpus-1920x1080", 24, [1920, 1080]),
            all("all-36-cpus-1920x1080", 36, [1920, 1080]),
            all("all-72-cpus-1920x1080", 72, [1920, 1080]),
            all("all-12-cpus-844x390", 12, [844, 390]),
            all("all-24-cpus-844x390", 24, [844, 390]),
            all("all-36-cpus-844x390", 36, [844, 390]),
            all("all-72-cpus-844x390", 72, [844, 390]),
            Scenario {
                up_to_eight: true,
                ..all("upto8-72-cpus-1280x720", 72, [1280, 720])
            },
            // The full chart's cost (`cpu-play-v1`): Beatucada's Easy, 34
            // holds and no mines, beside its Challenge's 20 holds and 132
            // mines above.
            Scenario {
                chart: 3,
                ..all("all-72-cpus-easy-holds-1280x720", 72, [1280, 720])
            },
            Scenario {
                up_to_eight: true,
                ..all("upto8-72-cpus-1920x1080", 72, [1920, 1080])
            },
        ])
        .chain(cost_scenarios())
        .collect();
    let only = std::env::var("DDI_FIELDS_ONLY").ok();
    for s in scenarios.iter() {
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

/// What holds and mines on many small fields cost (follow-up round F,
/// item 1): Beatucada's Hard (few holds, no mines: the baseline), Easy
/// (holds) and Challenge (holds and 132 mines), with 8, 24 and 72 CPUs at
/// 1280×720 and 2560×1440 (`DDI_FIELDS_ONLY=cost-` runs them alone).
fn cost_scenarios() -> Vec<Scenario> {
    let mut out = Vec::new();
    for (chart, name) in [(5, "hard"), (3, "easy"), (0, "challenge")] {
        for size in [[1280, 720], [2560, 1440]] {
            for cpus in [8u16, 24, 72] {
                let name: &'static str = Box::leak(
                    format!("cost-{name}-{cpus}-cpus-{}x{}", size[0], size[1]).into_boxed_str(),
                );
                out.push(Scenario {
                    name,
                    size,
                    cpus,
                    level: 7,
                    skin: None,
                    lives: None,
                    hand: false,
                    chart,
                    shots: &[20.0],
                    measure: Some(22.0),
                    lyrics: false,
                    attacks: true,
                    up_to_eight: false,
                    letgo_shot: false,
                });
            }
        }
    }
    out
}
