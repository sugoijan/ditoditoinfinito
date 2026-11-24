//! The window losing the focus during a play (`Shell::focused`): a key
//! held is let go, as its release would go to another window, stamped as
//! the shell stamps a play's edges (`Shell::play_input_time`): on a
//! test's own play clock (`Host::test_clock`, frames stepped faster than
//! real time) at that clock's time, so the hold ends where the focus was
//! lost on the play's clock, never at the machine's time. On a fake clock
//! with no sound device; on this machine's GPU, or undrawn where there is
//! none (`common::rig::Draws`).

mod common;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

use ddi_desktop::play::Request;
use ddi_desktop::shell::Shell;
use ddi_platform::{HostTime, InputSource};
use ddi_ui::event::UiEvent;

const SIZE: [u32; 2] = [640, 360];
const STEP: u64 = 1_000_000_000 / 60;

/// The shell's keyboard's edges handed to the play, as its poll does
/// (`Shell::poll_play`).
fn feed(shell: &mut Shell) {
    let mut f = shell.play.feed.borrow_mut();
    shell.input.keyboard.poll(&mut f.edges);
}

#[test]
fn a_key_held_is_let_go_at_the_focus_loss_on_the_play_s_clock() {
    common::indexes();
    ddi_desktop::audio::set_headless(true);
    let home = std::env::temp_dir().join(format!("ddi-play-focus-{}", std::process::id()));
    // SAFETY: set before any thread of this test reads the environment.
    unsafe { std::env::set_var("DDI_HOME", &home) };
    let mut shell = Shell::named_at("select", Some(0.0));
    let env = shell.theme.clone().expect("the theme").0;
    shell.ui.handle(UiEvent::Resize {
        size: [SIZE[0] as f32, SIZE[1] as f32],
        ratio: 1.0,
    });
    shell.ui.update(0.5);
    let clock = Arc::new(AtomicU64::new(common::rig::START_NS));
    let c = clock.clone();
    shell.play.test_clock = Some(Arc::new(move || c.load(Ordering::SeqCst)));
    let draws = common::rig::Draws::here(&mut shell.play, SIZE);
    *shell.play.requested.borrow_mut() = Some(Request::song("beatucada", 2, false));
    draws.wait_prompt(&mut shell.play, &mut shell.ui, &env);
    shell.ui.update(0.5);
    common::ui::key(&mut shell.ui, "Enter", "Enter");
    draws.poll(&mut shell.play, &mut shell.ui, &env);
    shell.sync_play_mode();
    assert!(shell.play.playing(), "{:?}", shell.ui.layers());
    assert!(shell.input.keyboard.enabled);
    let frames = |shell: &mut Shell, n: usize| {
        for i in 0..n {
            let now = clock.fetch_add(STEP, Ordering::SeqCst) + STEP;
            draws.frame(&mut shell.play, &mut shell.ui, &env, now);
            if i % 16 == 0 {
                draws.wait_gpu();
            }
        }
    };
    let now = || HostTime(clock.load(Ordering::SeqCst) as f64 / 1e9);
    frames(&mut shell, 120);
    // A lane's key held, then the window's focus lost half a second on.
    let held = now();
    shell.input.keyboard.key("ArrowLeft", true, false, held);
    feed(&mut shell);
    frames(&mut shell, 30);
    let lost = now();
    shell.focused(false);
    assert!(shell.input.keyboard.held().is_empty(), "let go");
    feed(&mut shell);
    frames(&mut shell, 30);
    let log = shell
        .play
        .running()
        .expect("still playing")
        .session
        .player
        .log()
        .inputs
        .clone();
    let press = log.iter().find(|i| i.pressed).expect("the press, in the log");
    let release = log
        .iter()
        .find(|i| !i.pressed && i.lane == press.lane)
        .unwrap_or_else(|| panic!("the release, in the log: {log:?}"));
    // Song seconds at rate 1 are the play clock's seconds.
    let kept = release.time - press.time;
    assert!(
        (kept - (lost.0 - held.0)).abs() < 1e-3,
        "held {kept} s on the play's clock, the focus lost {} s after the press",
        lost.0 - held.0
    );
    shell.focused(true);
}
