//! A play as the window runs it (`play::host`), on this machine's GPU
//! offscreen and on a fake clock with no sound device: song select's call
//! brings up the start prompt over it, Enter starts the play once the song
//! is read, R pressed twice two seconds in starts it again at once, it
//! draws every frame to its end (autoplayed, all top tier), its results
//! come up and it is kept once in the history; R retries (the prompt
//! again), Escape goes back to song select. Skipped where there is no
//! GPU adapter (CI).

mod common;
use std::rc::Rc;
use std::sync::atomic::Ordering;

use ddi_desktop::gpu;
use ddi_desktop::play::Request;
use ddi_desktop::play::host::{Device, Host};
use ddi_platform::HostTime;
use ddi_platform::history::MemoryPlayStore;
use ddi_ui::state::history::Plays;
use common::ui::key;
use common::ui::top;

const SIZE: [u32; 2] = [960, 540];

#[test]
fn a_play_from_its_prompt_to_its_results_and_back() {
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
    // A play's flow alone: without the dancing hand (with it, a quick
    // restart goes back through its prompt to warm a fresh hand, so the
    // play followed here would end at the restart) and without the
    // announcer (on by default since 2026-11-17; its line would come
    // before the play's in the log).
    ddi_ui::state::settings::Settings {
        dancing_hand: false,
        announcer: None,
        ..ddi_ui::state::settings::Settings::load()
    }
    .save();

    // Song select's call: the prompt, then "Press Start" once read.
    *host.requested.borrow_mut() = Some(Request::song("beatucada", 2, true));
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
        // The quit button: drawn while the pointer moved lately (the
        // shell's `Feed::pointer_at`), its place kept for the presses on
        // it; gone once that is longer ago.
        let moved = match frames {
            0 => Some(std::time::Instant::now()),
            1 => std::time::Instant::now().checked_sub(std::time::Duration::from_secs_f64(
                ddi_session::play::QUIT_LINGER * 2.0,
            )),
            _ => None,
        };
        if frames < 2 {
            host.feed.borrow_mut().pointer_at = moved;
        }
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
        if frames == 132 {
            assert!(host.playing(), "a restart plays on at once");
            assert_eq!(host.log, ["restarted beatucada"]);
            assert_eq!(store.len(), 0, "a restarted play is not kept");
        }
        common::rig::draw(&mut host, &dev(), &target, &mut ui, &env, now);
        let quit = host.feed.borrow().quit_at;
        match frames {
            0 => {
                let [x, y, r] = quit.expect("the quit button after the pointer moved");
                assert!(
                    r > 0.0 && x - r >= 0.0 && y + r <= SIZE[1] as f32,
                    "{quit:?}"
                );
            }
            1 => assert!(
                quit.is_none(),
                "{quit:?} two seconds after the pointer moved"
            ),
            _ => {}
        }
        frames += 1;
        assert!(frames < 60 * 300, "the play never ended");
    }
    let _ = device.poll(wgpu::PollType::wait_indefinitely());
    ui.update(0.05);
    assert_eq!(top(&ui), "results");
    assert_eq!(store.len(), 1, "kept once");
    assert!(
        host.log[1].contains("[77, 0, 0, 0, 0, 0]"),
        "{:?}",
        host.log
    );
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
    assert_eq!(store.len(), 1);
}
