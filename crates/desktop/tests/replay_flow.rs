//! Replays as the window runs them (`play::host`; `docs/plans/
//! play-history.md`, Replays), on this machine's GPU offscreen and on a
//! fake clock with no sound device: a play to its results, V watches its
//! replay (its prompt marked, the play drawn again from its log) to the
//! same results, not kept again; a kept play's results opened at once and
//! its replay from them; Escape twice quits a replay; a play that is not
//! kept is refused on the prompt. Skipped where there is no GPU adapter
//! (CI).

mod common;
use std::rc::Rc;
use std::sync::atomic::Ordering;

use ddi_desktop::gpu;
use ddi_desktop::play::host::{Device, Host};
use ddi_desktop::play::{Request, replays};
use ddi_platform::history::MemoryPlayStore;
use ddi_platform::{DeviceId, HostTime, RawInput};
use ddi_ui::screens::prompt::PromptStep;
use ddi_ui::state::history::Plays;
use common::ui::key;
use common::ui::top;

const SIZE: [u32; 2] = [960, 540];

#[test]
fn a_play_its_replay_its_kept_results_a_quit_and_a_refusal() {
    common::indexes();
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

    let poll_until_ready = |host: &mut Host, ui: &mut ddi_ui::Ui| {
        common::rig::wait_prompt(host, ui, &env, dev);
        ui.update(0.5);
    };
    // Runs the play to its end (or `frames` frames); the frames drawn.
    let run = |host: &mut Host, ui: &mut ddi_ui::Ui, frames: usize| {
        let mut n = 0;
        while host.playing() && n < frames {
            let now = clock.fetch_add(1_000_000_000 / 60, Ordering::SeqCst) + 1_000_000_000 / 60;
            common::rig::draw(host, &dev(), &target, ui, &env, now);
            n += 1;
            assert!(n < 60 * 300, "the play never ended");
        }
        let _ = device.poll(wgpu::PollType::wait_indefinitely());
        ui.update(0.05);
        n
    };

    // A play, kept.
    *host.requested.borrow_mut() = Some(Request::song("beatucada", 2, true));
    poll_until_ready(&mut host, &mut ui);
    key(&mut ui, "Enter", "Enter");
    ui.update(0.05);
    host.poll(&mut ui, &env, Some(dev()), 48_000);
    run(&mut host, &mut ui, usize::MAX);
    assert_eq!(top(&ui), "results");
    assert_eq!(store.len(), 1);
    let id = plays.history().last().unwrap().0.clone();
    // V: its replay's prompt, marked.
    ui.update(3.0);
    key(&mut ui, "KeyV", "v");
    ui.update(0.5);
    host.poll(&mut ui, &env, Some(dev()), 48_000);
    poll_until_ready(&mut host, &mut ui);
    assert_eq!(top(&ui), "play-prompt");
    key(&mut ui, "Enter", "Enter");
    ui.update(0.05);
    host.poll(&mut ui, &env, Some(dev()), 48_000);
    assert!(host.playing());
    run(&mut host, &mut ui, usize::MAX);
    assert_eq!(top(&ui), "results");
    assert!(
        host.log
            .iter()
            .any(|l| l == &format!("replay {id}: the play's results")),
        "{:?}",
        host.log
    );
    // The same counts as the play's, not kept again.
    let played: Vec<&String> = host
        .log
        .iter()
        .filter(|l| l.starts_with("played "))
        .collect();
    assert_eq!(played.len(), 2);
    assert_eq!(played[0], played[1]);
    assert_eq!(store.len(), 1, "a replay is not kept");
    // Back to song select; the kept play's results at once.
    ui.update(3.0);
    key(&mut ui, "Escape", "Escape");
    ui.update(0.5);
    host.poll(&mut ui, &env, Some(dev()), 48_000);
    ui.update(1.0);
    assert!(host.idle());
    *host.requested.borrow_mut() = Some(replays::request(&plays, &id, true));
    host.poll(&mut ui, &env, Some(dev()), 48_000);
    ui.update(0.5);
    assert_eq!(top(&ui), "results");
    assert!(
        host.log
            .last()
            .unwrap()
            .starts_with(&format!("kept play {id}"))
    );
    // Its replay from there; Escape twice quits it.
    ui.update(3.0);
    key(&mut ui, "KeyV", "v");
    ui.update(0.5);
    host.poll(&mut ui, &env, Some(dev()), 48_000);
    poll_until_ready(&mut host, &mut ui);
    key(&mut ui, "Enter", "Enter");
    ui.update(0.05);
    host.poll(&mut ui, &env, Some(dev()), 48_000);
    run(&mut host, &mut ui, 120);
    assert!(host.playing());
    let at = clock.load(Ordering::SeqCst) as f64 / 1e9;
    for (dt, pressed) in [(0.0, true), (0.05, false), (0.1, true)] {
        host.feed.borrow_mut().edges.push(RawInput {
            device: DeviceId::Keyboard,
            control: "Escape".into(),
            pressed,
            host_time: HostTime(at + dt),
            slot: 0,
        });
    }
    clock.fetch_add(200_000_000, Ordering::SeqCst);
    run(&mut host, &mut ui, 30);
    assert!(!host.playing());
    assert!(
        host.log.last().unwrap().starts_with("quit"),
        "{:?}",
        host.log
    );
    assert_eq!(store.len(), 1);
    // A play that is not kept: refused on the prompt.
    ui.update(1.0);
    *host.requested.borrow_mut() =
        Some(replays::request(&plays, "000000000000001-deadbeef", false));
    host.poll(&mut ui, &env, Some(dev()), 48_000);
    ui.update(0.5);
    match host.prompt_step() {
        Some(PromptStep::Error(e)) => assert!(e.contains("no longer kept"), "{e}"),
        other => panic!("{other:?}"),
    }
}
