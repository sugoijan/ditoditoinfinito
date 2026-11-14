//! An automatic CPU (or DDI_CPU_PROFILE reference) as the window runs it (`play::host`), on this machine's GPU
//! offscreen and on a fake clock with no sound device: song select's call
//! brings up the start prompt over it, Enter starts the play once the song
//! is read, sampled inputs play to the end, results appear and no personal
//! history entry is saved; R
//! retries (the prompt again), Escape goes back to song select. Skipped
//! where there is no GPU adapter (CI).

mod common;
use std::rc::Rc;
use std::sync::atomic::Ordering;

use ddi_desktop::gpu;
use ddi_desktop::play::cpu;
use ddi_desktop::play::host::{Device, Host};
use ddi_platform::history::MemoryPlayStore;
use ddi_ui::state::history::Plays;
use common::ui::key;
use common::ui::top;

const SIZE: [u32; 2] = [960, 540];

#[test]
fn cpu_from_prompt_to_results_and_retry_without_personal_history() {
    let request = if let Ok(profile) = std::env::var("DDI_CPU_PROFILE") {
        cpu::read(&profile, "normal", 1).unwrap()
    } else {
        cpu::arguments(&["--play", "beatucada", "--chart", "3", "--cpu=1"].map(String::from))
            .unwrap()
            .unwrap()
    };
    common::indexes();
    let instance = gpu::instance();
    let Some(adapter) = gpu::adapter(&instance, None) else {
        assert!(
            std::env::var_os("DDI_CPU_REQUIRE_GPU").is_none(),
            "GPU was required"
        );
        eprintln!("no GPU adapter: skipped");
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
    *host.requested.borrow_mut() = Some(request);
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
        let now = clock.fetch_add(1_000_000_000 / 2, Ordering::SeqCst) + 1_000_000_000 / 2;
        common::rig::draw(&mut host, &dev(), &target, &mut ui, &env, now);
        // Bound GPU work in flight in this offline fast-clock test.
        if frames % 16 == 0 {
            let _ = device.poll(wgpu::PollType::wait_indefinitely());
        }
        frames += 1;
        assert!(frames < 60 * 300, "the play never ended");
    }
    let _ = device.poll(wgpu::PollType::wait_indefinitely());
    ui.update(0.05);
    assert_eq!(top(&ui), "results");
    assert_eq!(store.len(), 0, "CPU must not enter personal history");
    // The CPU described under its results; Retry, no song offset.
    let shown = common::ui::results_shown(&ui);
    assert!(
        shown.notes.starts_with("Autoplay: this play does not count toward your bests. · CPU · "),
        "{shown:?}"
    );
    assert!(shown.retry && !shown.offset, "{shown:?}");
    assert!(!host.log.is_empty(), "CPU results were not produced");
    eprintln!("CPU integration: {} frames; {:?}", frames, host.log);
    // Retry: the prompt again (the results take keys after a moment).
    ui.update(3.0);
    key(&mut ui, "KeyR", "r");
    ui.update(0.5);
    host.poll(&mut ui, &env, Some(dev()), 48_000);
    ui.update(0.5);
    assert!(host.prompting());
    assert_eq!(top(&ui), "play-prompt");
    // Escape: back to song select, nothing more kept.
    key(&mut ui, "Escape", "Escape");
    ui.update(0.5);
    host.poll(&mut ui, &env, Some(dev()), 48_000);
    ui.update(1.0);
    assert!(!host.prompting() && !host.playing());
    assert_eq!(top(&ui), "select", "{:?}", ui.layers());
    assert_eq!(store.len(), 0);
}
