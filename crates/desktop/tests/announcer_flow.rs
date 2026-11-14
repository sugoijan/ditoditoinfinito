//! The announcer over a play as the window runs it (`play::host`,
//! `docs/plans/announcers.md`, step 4), offscreen on a fake clock with no
//! sound device: an announcer of generated tones in a song folder's
//! `Announcers/`, chosen in the settings, its sounds decoded behind the
//! prompt; an autoplayed play, quickly restarted two seconds in, speaks
//! `gameplay restart` (ours, step 6), the full combo as its last note is
//! judged then cleared, and the results' full combo; song select's intro
//! and a chosen song's comment speak on the preview player (step 5).
//! Skipped where there is no GPU adapter (CI).

mod common;
use std::rc::Rc;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

use ddi_desktop::gpu;
use ddi_desktop::play::Request;
use ddi_desktop::play::host::{Device, Host};
use ddi_desktop::play::view::Formats;
use ddi_platform::HostTime;
use ddi_platform::history::MemoryPlayStore;
use ddi_platform::storage::{self, MemoryStore};
use ddi_ui::event::UiEvent;
use ddi_ui::state::history::Plays;
use ddi_ui::state::settings::Settings;

const SIZE: [u32; 2] = [640, 360];

/// `seconds` of a 16-bit mono WAV tone at 48 kHz.
fn wav(seconds: f64) -> Vec<u8> {
    let n = (48_000.0 * seconds) as usize;
    let samples: Vec<f32> = (0..n)
        .map(|i| ((i as f64 * 440.0 * std::f64::consts::TAU / 48_000.0).sin() * 0.25) as f32)
        .collect();
    ddi_library::wav::write(&[samples], 48_000)
}

#[test]
fn an_announcer_speaks_over_a_play_and_its_results() {
    common::indexes();
    let instance = gpu::instance();
    let Some(adapter) = gpu::adapter(&instance, None) else {
        eprintln!("no GPU adapter: skipped");
        return;
    };
    // A song folder with an announcer of tones.
    let root = std::env::temp_dir()
        .join("ddi-desktop-tests")
        .join(format!("announcer-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    let tone = wav(0.3);
    for line in [
        "gameplay ready",
        "gameplay here we go normal",
        "gameplay comment good",
        "gameplay cleared",
        "evaluation full combo w1",
        "evaluation cheer",
        "select music intro",
        "select music comment new",
        "gameplay restart",
        "gameplay full combo w1",
    ] {
        let dir = root.join("Announcers/Tones").join(line);
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("a.wav"), &tone).unwrap();
    }
    storage::install(Rc::new(MemoryStore::default()));
    Settings {
        announcer: Some("tones".into()),
        ..Settings::default()
    }
    .save();
    ddi_desktop::library::set_extra_folders(std::slice::from_ref(&root));

    let (device, queue) = gpu::device(&adapter).unwrap();
    let fold: ddi_ui::dev_widgets::FoldFn = Rc::new(|s: &str| s.to_lowercase());
    let (mut ui, env, _) = ddi_desktop::shell::theme_ui("select", fold);
    ui.handle(UiEvent::Resize {
        size: [SIZE[0] as f32, SIZE[1] as f32],
        ratio: 1.0,
    });
    ui.update(0.5);
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

    // Read, and a moment for the voice to decode (it never holds Start).
    *host.requested.borrow_mut() = Some(Request::song("beatucada", 2, true));
    let mut waited = 0;
    while !host.prompt_ready() || waited < 25 {
        host.poll(&mut ui, &env, Some(dev()), 48_000);
        ui.update(0.05);
        std::thread::sleep(std::time::Duration::from_millis(20));
        waited += 1;
        assert!(waited < 1000, "the song was never read");
    }
    ui.handle(UiEvent::Key(ddi_ui::event::KeyEvent::down(
        "Enter", "Enter",
    )));
    ui.handle(UiEvent::Key(ddi_ui::event::KeyEvent::up("Enter", "Enter")));
    ui.update(0.05);
    host.poll(&mut ui, &env, Some(dev()), 48_000);
    assert!(host.playing(), "{:?}", ui.layers());
    let mut frames = 0;
    while host.playing() {
        let now = clock.fetch_add(1_000_000_000 / 60, Ordering::SeqCst) + 1_000_000_000 / 60;
        // R pressed twice, two seconds in: the play starts again.
        if let Some(pressed) = match frames {
            120 | 130 => Some(true),
            121 | 131 => Some(false),
            _ => None,
        } {
            host.feed.borrow_mut().edges.push(ddi_platform::RawInput {
                device: ddi_platform::DeviceId::Keyboard,
                control: "KeyR".into(),
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
        queue.submit(Some(encoder.finish()));
        frames += 1;
        assert!(frames < 60 * 300, "the play never ended");
    }
    let said = host
        .log
        .iter()
        .find(|l| l.starts_with("announcer:"))
        .unwrap_or_else(|| panic!("nothing said: {:?}", host.log));
    assert!(
        host.log.iter().any(|l| l.starts_with("restarted")),
        "{:?}",
        host.log
    );
    // The play begun again: opened by its own line.
    assert!(
        said.starts_with("announcer: spoke gameplay restart, "),
        "{said}"
    );
    for line in [
        "gameplay full combo w1, gameplay cleared",
        "evaluation full combo w1",
        "evaluation cheer",
    ] {
        assert!(said.contains(line), "{line}: {said}");
    }
    eprintln!("{said}");
    assert!(said.contains("not decoded []"), "{said}");

    // Song select's lines on the preview player: the intro, asked before
    // its sound was decoded, spoken once it is; then a comment at once.
    use ddi_library::announcer::{Comment, Line};
    let mut p = ddi_desktop::preview::Player::with(
        ddi_desktop::audio::Audio::silent(0.8),
        ddi_desktop::preview::PreviewSong::bundled,
    );
    p.announce_menu(Line::SelectMusicIntro);
    let until = std::time::Instant::now() + std::time::Duration::from_secs(10);
    while p.spoken.is_empty() && std::time::Instant::now() < until {
        p.tick();
        std::thread::sleep(std::time::Duration::from_millis(5));
    }
    p.announce_menu(Line::SelectMusic(Comment::New));
    // The pack has no `hard`: nothing.
    p.announce_menu(Line::SelectMusic(Comment::Hard));
    assert_eq!(p.spoken, ["select music intro", "select music comment new"]);
    let _ = std::fs::remove_dir_all(&root);
}

/// Ours, which come with the game, chosen by their keys: song select's
/// lines read from the assets and decoded, with no song folder at all
/// (`docs/plans/announcers.md`, "Packs of our own").
#[test]
fn ours_speak_from_the_assets() {
    use ddi_library::announcer::{Comment, Line};
    common::indexes();
    storage::install(Rc::new(MemoryStore::default()));
    for key in ["bundled:mr. clank", "bundled:ガキンさん"] {
        Settings {
            announcer: Some(key.into()),
            ..Settings::default()
        }
        .save();
        let mut p = ddi_desktop::preview::Player::with(
            ddi_desktop::audio::Audio::silent(0.8),
            ddi_desktop::preview::PreviewSong::bundled,
        );
        p.announce_menu(Line::SelectMusicIntro);
        let until = std::time::Instant::now() + std::time::Duration::from_secs(20);
        while p.spoken.is_empty() && std::time::Instant::now() < until {
            p.tick();
            std::thread::sleep(std::time::Duration::from_millis(5));
        }
        p.announce_menu(Line::SelectMusic(Comment::Hard));
        // (Decoded after the intro's sounds, all of them now: spoken once
        // decoded, while it is due.)
        while p.spoken.len() < 2 && std::time::Instant::now() < until {
            p.tick();
            std::thread::sleep(std::time::Duration::from_millis(5));
        }
        assert_eq!(
            p.spoken,
            ["select music intro", "select music comment hard"],
            "{key}"
        );
        // Each entering of song select speaks its intro once (round D):
        // six more enterings, six more lines on the mixer, never two.
        for n in 1..=6 {
            p.announce_menu(Line::SelectMusicIntro);
            p.tick();
            assert_eq!(p.spoken.len(), 2 + n, "{key}: {:?}", p.spoken);
        }
    }
}
