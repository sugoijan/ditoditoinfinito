//! A pump chart's version without its center (`docs/plans/chart-simplification.md`)
//! played as the window runs it, offscreen on a fake clock with no sound
//! device: Beatucada's first pump-single chart, turned onto Single, listed
//! with the bundled songs, autoplayed all top tier with its holds ticked,
//! its results marked as assisted and kept so. Skipped where there is no
//! GPU adapter (CI).

mod common;
use std::rc::Rc;
use std::sync::atomic::Ordering;

use ddi_actor::Drawn;
use ddi_actor::text::FixedMeasure;
use ddi_chart::simplify::Derivation;
use ddi_desktop::gpu;
use ddi_desktop::play::Request;
use ddi_desktop::play::host::{Device, Host};
use ddi_platform::history::MemoryPlayStore;
use ddi_ui::state::history::Plays;
use common::ui::key;
use common::ui::top;

const SIZE: [u32; 2] = [960, 540];

#[test]
fn a_center_free_pump_chart_autoplays_all_top_tier_and_assisted() {
    common::indexes();
    // A StepMania preset: its holds are ticked on a pump chart's version
    // (DDI, the default, keeps its own on every chart).
    ddi_platform::storage::install(Rc::new(ddi_platform::storage::MemoryStore::default()));
    ddi_ui::state::settings::Settings {
        ruleset: "itg".into(),
        ..Default::default()
    }
    .save();
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
    let Some(adapter) = common::gpu::adapter_or_skip() else {
        return;
    };
    let (device, queue) = gpu::device(&adapter).unwrap();
    let (mut ui, env) = common::ui::theme("select", common::ui::lowercase(), SIZE, false);
    ui.update(0.5);
    let store = Rc::new(MemoryPlayStore::default());
    let plays = Plays::new(store.clone());
    let mut host = Host::new(Rc::default(), Some(plays.clone()));
    let clock = common::rig::fake_clock(&mut host);
    let formats = common::gpu::formats();
    let dev = || Device {
        device: &device,
        queue: &queue,
        formats,
    };
    let target = common::gpu::Target::new(&device, SIZE);

    // Song select's call: the prompt, then "Press Start" once read.
    *host.requested.borrow_mut() = Some(Request::song("beatucada", chart, true));
    common::rig::wait_prompt(&mut host, &mut ui, &env, dev);
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
        common::rig::draw(&mut host, &dev(), &target, &mut ui, &env, now);
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
