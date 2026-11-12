//! The announcer in a practice stage with attacks as the desktop window
//! runs it (`docs/announcer-lines.md`, "In battle";
//! `docs/plans/multiplayer-prototype-progress.md`, M4 and M8 round 3), on
//! this machine's GPU offscreen and a fake clock with no sound device:
//! an announcer chosen in the settings, its stage's sounds decoded behind
//! the prompt, an autoplayed stage against CPUs whose full bars launch
//! attacks; what the play said of the battle's moments and the attacks',
//! and what it showed in the status line instead, read from the host's
//! log. Skipped where there is no GPU adapter (CI).

mod common;
use std::path::PathBuf;
use std::rc::Rc;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

use ddi_desktop::gpu;
use ddi_desktop::play::host::{Device, Host};
use ddi_desktop::play::view::Formats;
use ddi_platform::HostTime;
use ddi_platform::history::MemoryPlayStore;
use ddi_platform::storage::{self, MemoryStore};
use ddi_ui::event::{KeyEvent, UiEvent};
use ddi_ui::screens::data::{ChartEntry, Difficulty, SongEntry};
use ddi_ui::state::history::Plays;
use ddi_ui::state::settings::Settings;

const SIZE: [u32; 2] = [1280, 720];

fn key(ui: &mut ddi_ui::Ui, code: &str, k: &str) {
    ui.handle(UiEvent::Key(KeyEvent::down(code, k)));
    ui.handle(UiEvent::Key(KeyEvent::up(code, k)));
}

/// What a stage said and showed.
struct Heard {
    /// The lines spoken, by folder, in order.
    spoken: Vec<String>,
    /// The player's attack moments, each its line's folder and whether
    /// it was said (else shown).
    attacks: Vec<(String, bool)>,
    /// The battle's moments, each its line's folder and whether a line
    /// said it.
    moments: Vec<(String, bool)>,
}

/// A one-stage practice match under the attacks' battle on Beatucada's
/// Medium chart, autoplayed against the CPUs with announcer `key` chosen
/// (`folder`: a song folder it is found in; ours need none): what the
/// play said. `None` without a GPU adapter.
fn stage_with(key_of: &str, folder: Option<PathBuf>) -> Option<Heard> {
    common::indexes();
    let instance = gpu::instance();
    let adapter = gpu::adapter(&instance, None)?;
    storage::install(Rc::new(MemoryStore::default()));
    Settings {
        announcer: Some(key_of.into()),
        ..Settings::default()
    }
    .save();
    if let Some(folder) = &folder {
        ddi_desktop::library::set_extra_folders(std::slice::from_ref(folder));
    }
    let (device, queue) = gpu::device(&adapter).unwrap();
    let fold: ddi_ui::dev_widgets::FoldFn = Rc::new(|s: &str| s.to_lowercase());
    let (mut ui, env, _) = ddi_desktop::shell::theme_ui("start", fold);
    env.reduced_motion.set(true);
    ui.handle(UiEvent::Resize {
        size: [SIZE[0] as f32, SIZE[1] as f32],
        ratio: 1.0,
    });
    let store = Rc::new(MemoryPlayStore::default());
    let mut host = Host::new(Rc::default(), Some(Plays::new(store)));
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
    request.auto = true;
    *host.requested.borrow_mut() = Some(request);
    // Read, and a moment for the voice to decode (it never holds Start).
    let mut waited = 0;
    while !host.prompt_ready() || waited < 50 {
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
    while host.playing() {
        let now = clock.fetch_add(1_000_000_000 / 10, Ordering::SeqCst) + 1_000_000_000 / 10;
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
        if frames % 16 == 0 {
            let _ = device.poll(wgpu::PollType::wait_indefinitely());
        }
        frames += 1;
        assert!(frames < 10 * 400, "the play never ended");
    }
    let _ = device.poll(wgpu::PollType::wait_indefinitely());
    ui.update(1.0);
    // The results' line, once the room confirmed the stage.
    host.poll(&mut ui, &env, Some(dev()), 48_000);
    ui.update(0.5);
    eprintln!("{key_of}: {:#?}", host.log);
    let list = |prefix: &str| -> Vec<String> {
        host.log
            .iter()
            .filter_map(|l| l.strip_prefix(prefix))
            .flat_map(|l| l.split(" · ").next().unwrap_or("").split(", "))
            .map(str::to_string)
            .filter(|l| !l.is_empty())
            .collect()
    };
    let marked = |items: Vec<String>| -> Vec<(String, bool)> {
        items
            .into_iter()
            .map(|i| {
                let (line, how) = i.split_once(" (").unwrap_or((i.as_str(), ""));
                (line.to_string(), how.starts_with("said"))
            })
            .collect()
    };
    Some(Heard {
        spoken: list("announcer: spoke "),
        attacks: marked(list("attacks: ")),
        moments: marked(list("battle: ")),
    })
}

/// `seconds` of a 16-bit mono WAV tone at 48 kHz.
fn wav(seconds: f64) -> Vec<u8> {
    let n = (48_000.0 * seconds) as usize;
    let samples: Vec<f32> = (0..n)
        .map(|i| ((i as f64 * 440.0 * std::f64::consts::TAU / 48_000.0).sin() * 0.25) as f32)
        .collect();
    ddi_library::wav::write(&[samples], 48_000)
}

/// A pack made in a scratch folder with the attack lines' two folders
/// (and nothing else of the battle's), found in a song folder's
/// `Announcers/` and chosen: a practice stage says `battle attack
/// incoming` and `battle attack launched` for the player's attack
/// moments, and each moment is said or shown, never neither; the
/// battle's other moments, which the pack lacks, are not said.
#[test]
fn a_scratch_pack_s_attack_lines_are_said_in_a_practice_stage() {
    let root = std::env::temp_dir()
        .join("ddi-desktop-tests")
        .join(format!("stage-voice-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    let tone = wav(0.6);
    for line in ["battle attack incoming", "Battle Attack Launched"] {
        let dir = root.join("Announcers/Tones").join(line);
        std::fs::create_dir_all(&dir).unwrap();
        for take in ["a.wav", "b.wav"] {
            std::fs::write(dir.join(take), &tone).unwrap();
        }
    }
    let Some(heard) = stage_with("tones", Some(root.clone())) else {
        eprintln!("no GPU adapter: skipped");
        return;
    };
    let _ = std::fs::remove_dir_all(&root);
    let said = |line: &str| heard.spoken.iter().filter(|l| *l == line).count();
    let told = |line: &str, how: bool| {
        heard
            .attacks
            .iter()
            .filter(|(l, said)| l == line && *said == how)
            .count()
    };
    assert!(said("battle attack launched") > 0, "{:?}", heard.spoken);
    assert!(said("battle attack incoming") > 0, "{:?}", heard.spoken);
    for line in ["battle attack incoming", "battle attack launched"] {
        assert_eq!(said(line), told(line, true), "{line}");
    }
    // An attack against the player is always said here: nothing that
    // matters more speaks in this pack.
    assert_eq!(
        told("battle attack incoming", false),
        0,
        "{:?}",
        heard.attacks
    );
    // Nothing but the two lines: the pack has no other.
    assert!(
        heard.spoken.iter().all(|l| l.starts_with("battle attack ")),
        "{:?}",
        heard.spoken
    );
    assert!(
        heard.moments.iter().all(|(_, said)| !said),
        "{:?}",
        heard.moments
    );
}
