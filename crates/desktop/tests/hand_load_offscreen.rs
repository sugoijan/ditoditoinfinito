//! The dancing hand giving way under load (`ddi_session::hand_load`,
//! `hand-load-v0`) as the window plays it, offscreen on a fake clock with
//! no sound device: Beatucada's Challenge autoplayed with the hand on,
//! once quiet (every frame on its refresh: no step over the whole song)
//! and once loaded (the frames come late from the sixth second on, as a
//! busy machine has them: "a frame took 25 ms" is a number on the fake
//! clock): the hand goes lighter (its picture every other frame), then
//! rests (nothing prepared, its picture still), then goes, and the
//! results say so; the play's judgments are the quiet run's. The hand's
//! preparation runs on a clock of the test's own (half a microsecond a
//! reading), so the machine's speed changes nothing. Skipped where there
//! is no GPU adapter (CI).

mod common;
use std::rc::Rc;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

use ddi_actor::Drawn;
use ddi_actor::text::FixedMeasure;
use ddi_desktop::gpu;
use ddi_desktop::play::Request;
use ddi_desktop::play::host::{Device, Host};
use ddi_desktop::play::view::Formats;
use ddi_platform::HostTime;
use ddi_ui::event::{KeyEvent, UiEvent};

const SIZE: [u32; 2] = [960, 540];
/// The quiet frames' interval (60 Hz) and the loaded ones' (40 a second).
const QUIET_NS: u64 = 1_000_000_000 / 60;
const LOADED_NS: u64 = 25_000_000;
/// When the load begins, seconds of the play.
const LOAD_FROM: f64 = 6.0;

fn key(ui: &mut ddi_ui::Ui, code: &str, k: &str) {
    ui.handle(UiEvent::Key(KeyEvent::down(code, k)));
    ui.handle(UiEvent::Key(KeyEvent::up(code, k)));
}

/// What one play showed: its "played" log line, the steps the rule took
/// (name, play seconds), what the hand did in each step (pictures drawn,
/// preparation checkpoints) and the results' texts.
struct Run {
    played: String,
    steps: Vec<(String, f64)>,
    /// Per step: frames, pictures drawn, checkpoints done.
    by_step: Vec<(String, u64, u64, u64)>,
    notes: Vec<String>,
}

fn play(loaded: bool) -> Option<Run> {
    common::indexes();
    let instance = gpu::instance();
    let Some(adapter) = gpu::adapter(&instance, None) else {
        eprintln!("no GPU adapter: skipped");
        return None;
    };
    let (device, queue) = gpu::device(&adapter).unwrap();
    ddi_platform::storage::install(Rc::new(ddi_platform::storage::MemoryStore::default()));
    let settings = ddi_ui::state::settings::Settings::default();
    assert!(settings.dancing_hand, "the hand is on by default");
    settings.save();
    let fold: ddi_ui::dev_widgets::FoldFn = Rc::new(|s: &str| s.to_lowercase());
    let (mut ui, env, _) = ddi_desktop::shell::theme_ui("select", fold);
    ui.handle(UiEvent::Resize {
        size: [SIZE[0] as f32, SIZE[1] as f32],
        ratio: 1.0,
    });
    ui.update(0.5);
    let mut host = Host::new(Rc::default(), None);
    let clock = Arc::new(AtomicU64::new(1_000_000_000));
    let c = clock.clone();
    host.test_clock = Some(Arc::new(move || c.load(Ordering::SeqCst)));
    let ticks = std::cell::Cell::new(0.0);
    host.test_seconds = Some(Rc::new(move || {
        let t = ticks.get();
        ticks.set(t + 0.000_000_5);
        t
    }));
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
    // Beatucada's Challenge (its first chart), autoplayed.
    *host.requested.borrow_mut() = Some(Request::song("beatucada", 0, true));
    let mut waited = 0;
    loop {
        host.poll(&mut ui, &env, Some(dev()), 48_000);
        ui.update(0.05);
        if host.prompt_ready() {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(20));
        waited += 1;
        assert!(waited < 1500, "the song was never read");
    }
    ui.update(0.5);
    key(&mut ui, "Enter", "Enter");
    ui.update(0.05);
    host.poll(&mut ui, &env, Some(dev()), 48_000);
    assert!(host.playing(), "{:?}", ui.layers());
    let started = clock.load(Ordering::SeqCst);
    let mut by_step: Vec<(String, u64, u64, u64)> = Vec::new();
    let mut last: Option<(u64, u64)> = None;
    let mut rule_seen = serde_json::Value::Null;
    let mut frames = 0;
    while host.playing() {
        let seconds = (clock.load(Ordering::SeqCst) - started) as f64 / 1e9;
        let step = if loaded && seconds >= LOAD_FROM {
            LOADED_NS
        } else {
            QUIET_NS
        };
        let now = clock.fetch_add(step, Ordering::SeqCst) + step;
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
        if let (Some(h), Some(rule)) = (host.hand_debug(), host.hand_load_debug()) {
            let drawn = h["character"]["drawn"].as_u64().unwrap_or(0);
            let work = h["runtime"]["planner"]["checkpoints"].as_u64().unwrap_or(0)
                + h["runtime"]["report"]["checkpoints"].as_u64().unwrap_or(0);
            // A frame is the step that was in effect while it was drawn
            // (a step is taken at the end of the hand's part of a frame).
            let name = rule_seen["step"].as_str().unwrap_or("full").to_string();
            rule_seen = rule.clone();
            if by_step.last().is_none_or(|b| b.0 != name) {
                by_step.push((name, 0, 0, 0));
            }
            let b = by_step.last_mut().unwrap();
            let (d0, w0) = last.unwrap_or((drawn, work));
            b.1 += 1;
            b.2 += drawn - d0;
            b.3 += work - w0;
            last = Some((drawn, work));
            assert!(h["runtime_error"].is_null(), "{h}");
        }
        frames += 1;
        assert!(frames < 60 * 300, "the play never ended");
    }
    let _ = device.poll(wgpu::PollType::wait_indefinitely());
    let played = host
        .log
        .iter()
        .find(|l| l.starts_with("played"))
        .unwrap()
        .clone();
    // The steps taken, from the view the results kept.
    let steps = rule_seen["taken"]
        .as_array()
        .unwrap()
        .iter()
        .map(|t| {
            (
                t["step"].as_str().unwrap().to_string(),
                (t["at_ms"].as_f64().unwrap() - started as f64 / 1e6) / 1000.,
            )
        })
        .collect();
    ui.update(1.0);
    let notes = ui
        .draw(&mut FixedMeasure)
        .into_iter()
        .filter_map(|d| match d {
            Drawn::Text(t) => Some(t.text.to_string()),
            _ => None,
        })
        .collect();
    Some(Run {
        played,
        steps,
        by_step,
        notes,
    })
}

#[test]
fn the_hand_gives_way_under_load_and_never_on_a_quiet_play() {
    let Some(quiet) = play(false) else {
        return;
    };
    let Some(loaded) = play(true) else {
        return;
    };
    eprintln!("quiet: {:?}", quiet.by_step);
    eprintln!("loaded: {:?} {:?}", loaded.steps, loaded.by_step);
    // Quiet: no step over the whole song, a picture every frame.
    assert!(quiet.steps.is_empty(), "{:?}", quiet.steps);
    assert_eq!(quiet.by_step.len(), 1);
    let (_, frames, drawn, _) = &quiet.by_step[0];
    // (Its picture repeats only where its time does: the frames before
    // the song.)
    assert!(*drawn + 60 >= *frames, "{:?}", quiet.by_step);
    let says = |r: &Run| r.notes.iter().any(|t| t.contains("dancing hand was turned off"));
    assert!(!says(&quiet));
    // Loaded: lighter, resting, gone, in order, each after its own run
    // of late seconds, none before the load.
    let names: Vec<&str> = loaded.steps.iter().map(|s| s.0.as_str()).collect();
    assert_eq!(names, ["lighter", "resting", "gone"], "{:?}", loaded.steps);
    assert!(loaded.steps[0].1 > LOAD_FROM + 2.5, "{:?}", loaded.steps);
    for w in loaded.steps.windows(2) {
        assert!(w[1].1 - w[0].1 > 3.5, "{:?}", loaded.steps);
    }
    let stage = |name: &str| loaded.by_step.iter().find(|b| b.0 == name).unwrap();
    // Lighter: a picture every other frame, still preparing.
    let (_, frames, drawn, work) = stage("lighter");
    assert!(
        (*drawn as f64 - *frames as f64 / 2.).abs() <= 2.,
        "{:?}",
        loaded.by_step
    );
    assert!(*work > 0);
    // Resting: nothing prepared, nothing drawn anew (the picture kept).
    let (_, _, drawn, work) = stage("resting");
    assert_eq!((*drawn, *work), (0, 0), "{:?}", loaded.by_step);
    // Gone: neither.
    let (_, _, drawn, work) = stage("gone");
    assert_eq!((*drawn, *work), (0, 0), "{:?}", loaded.by_step);
    // The results say so, and the play judged exactly as the quiet one.
    assert!(says(&loaded), "{:?}", loaded.notes);
    assert_eq!(loaded.played, quiet.played);
}
