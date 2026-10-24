//! A pump chart's version without its center (`docs/plans/chart-simplification.md`)
//! played as the window runs it, offscreen on a fake clock with no sound
//! device: Beatucada's first pump-single chart, turned onto Single, listed
//! with the bundled songs, autoplayed all top tier with its holds ticked,
//! its results marked as assisted and kept so. Skipped where there is no
//! GPU adapter (CI).

mod common;
use std::rc::Rc;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

use ddi_actor::Drawn;
use ddi_actor::text::FixedMeasure;
use ddi_chart::simplify::Derivation;
use ddi_desktop::gpu;
use ddi_desktop::play::Request;
use ddi_desktop::play::host::{Device, Host};
use ddi_desktop::play::view::Formats;
use ddi_platform::HostTime;
use ddi_platform::history::MemoryPlayStore;
use ddi_ui::event::{KeyEvent, UiEvent};
use ddi_ui::screens::prompt::PromptStep;
use ddi_ui::state::history::Plays;

const SIZE: [u32; 2] = [960, 540];

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

#[test]
fn a_center_free_pump_chart_autoplays_all_top_tier_and_assisted() {
    common::indexes();
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../assets/songs");
    // The version as the library reads it, and as the bundled list has it.
    let text = std::fs::read_to_string(root.join("beatucada/Beatucada.ssc")).unwrap();
    let song = ddi_library::scan::parse_simfile(&text, "ssc").unwrap();
    let (chart, converted) = song
        .charts
        .iter()
        .enumerate()
        .find(|(_, c)| {
            matches!(c.derivation, Some(Derivation::WithoutCenter { source })
                if song.charts[source].layout == "pump-single")
        })
        .expect("Beatucada has a pump-single chart");
    assert_eq!(converted.layout, "dance-single");
    let listed: ddi_library::manifest::Manifest = serde_json::from_str(
        &std::fs::read_to_string(root.join("../../target/songs/index.json")).unwrap(),
    )
    .unwrap();
    let entry = listed.songs.iter().find(|s| s.id == "beatucada").unwrap();
    assert!(
        entry
            .charts
            .iter()
            .any(|c| c.index == chart && c.derivation.is_some() && c.layout == "dance-single"),
        "the bundled list is stale: `cargo run -p xtask -- gen-songs`"
    );
    let instance = gpu::instance();
    let Some(adapter) = gpu::adapter(&instance, None) else {
        eprintln!("no GPU adapter: skipped");
        return;
    };
    let (device, queue) = gpu::device(&adapter).unwrap();
    let fold: ddi_ui::dev_widgets::FoldFn = Rc::new(|s: &str| s.to_lowercase());
    let (mut ui, env, _) = ddi_desktop::shell::theme_ui("select", fold);
    ui.handle(UiEvent::Resize {
        size: [SIZE[0] as f32, SIZE[1] as f32],
        ratio: 1.0,
    });
    ui.update(0.5);
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
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
        view_formats: &[wgpu::TextureFormat::Rgba8UnormSrgb],
    });
    let srgb = texture.create_view(&wgpu::TextureViewDescriptor {
        format: Some(wgpu::TextureFormat::Rgba8UnormSrgb),
        ..Default::default()
    });
    let plain = texture.create_view(&Default::default());

    // Song select's call: the prompt, then "Press Start" once read.
    *host.requested.borrow_mut() = Some(Request::song("beatucada", chart, true));
    let mut waited = 0;
    loop {
        host.poll(&mut ui, &env, Some(dev()), 48_000);
        ui.update(0.05);
        if host.prompt_step() == Some(PromptStep::Ready) {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(20));
        waited += 1;
        assert!(waited < 1000, "the song was never read");
    }
    ui.update(0.5);
    assert_eq!(top(&ui), "play-prompt");
    // Enter starts it; the prompt goes.
    key(&mut ui, "Enter", "Enter");
    ui.update(0.05);
    host.poll(&mut ui, &env, Some(dev()), 48_000);
    assert!(host.playing(), "{:?}", ui.layers());
    let mut frames = 0;
    while host.playing() {
        let now = clock.fetch_add(1_000_000_000 / 60, Ordering::SeqCst) + 1_000_000_000 / 60;
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
    let _ = device.poll(wgpu::PollType::wait_indefinitely());
    ui.update(0.05);
    assert_eq!(top(&ui), "results");
    assert_eq!(store.len(), 1, "kept once");
    // Every step at the top tier.
    let played = host
        .log
        .iter()
        .find(|l| l.starts_with("played"))
        .unwrap()
        .clone();
    let taps = played
        .split_once('[')
        .and_then(|(_, rest)| rest.split_once(']'))
        .unwrap()
        .0
        .split(", ")
        .map(|n| n.parse::<u32>().unwrap())
        .collect::<Vec<_>>();
    assert!(taps[0] > 0 && taps[1..].iter().all(|&n| n == 0), "{played}");
    // Kept as assisted, with its holds ticked and every tick hit.
    let history = plays.history();
    let (_, summary) = history.last().unwrap();
    let worked = summary.worked.clone().unwrap();
    assert!(worked.assist);
    assert!(worked.tally.checkpoint_hit > 0 && worked.tally.checkpoint_miss == 0);
    assert_eq!(summary.chart.digest, song.chart_digest(chart));
    drop(history);
    // The results say so.
    ui.update(1.0);
    let texts: Vec<String> = ui
        .draw(&mut FixedMeasure)
        .into_iter()
        .filter_map(|d| match d {
            Drawn::Text(t) => Some(t.text.to_string()),
            _ => None,
        })
        .collect();
    assert!(
        texts.iter().any(|t| t.contains("AUTOPLAY · ASSIST")),
        "{texts:?}"
    );
}
