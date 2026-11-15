//! What a finished play's results say as the window shows them
//! (`play::host`), on this machine's GPU offscreen and on a fake clock
//! with no sound device: a play nobody presses in, failed, whose keeping
//! failed too; an autoplay quit. Without a GPU adapter (CI) the plays
//! run undrawn (`common::rig::Draws`) and all of it is checked all the
//! same.

mod common;
use std::rc::Rc;
use std::sync::atomic::Ordering;

use ddi_desktop::play::Request;
use ddi_desktop::play::host::Host;
use ddi_platform::history::MemoryPlayStore;
use ddi_platform::{DeviceId, HostTime, RawInput};
use ddi_ui::state::history::Plays;
use common::ui::{ResultsShown, key, results_shown, top};

const SIZE: [u32; 2] = [960, 540];

#[test]
fn a_failed_play_unkept_and_a_quit_say_so_on_their_results() {
    common::indexes();
    let (mut ui, env) = common::ui::theme("select", common::ui::lowercase(), SIZE, false);
    ui.update(0.5);
    let store = Rc::new(MemoryPlayStore::default());
    store.failing.set(true);
    let plays = Plays::new(store.clone());
    let mut host = Host::new(Rc::default(), Some(plays.clone()));
    let clock = common::rig::fake_clock(&mut host);
    let draws = common::rig::Draws::here(&mut host, SIZE);
    let run = |host: &mut Host, ui: &mut ddi_ui::Ui, frames: usize| {
        let mut n = 0;
        while host.playing() && n < frames {
            let now = clock.fetch_add(1_000_000_000 / 60, Ordering::SeqCst) + 1_000_000_000 / 60;
            draws.frame(host, ui, &env, now);
            n += 1;
            assert!(n < 60 * 300, "the play never ended");
        }
        draws.wait_gpu();
        ui.update(0.05);
    };
    let start = |host: &mut Host, ui: &mut ddi_ui::Ui, request: Request| {
        *host.requested.borrow_mut() = Some(request);
        draws.wait_prompt(host, ui, &env);
        ui.update(0.5);
        key(ui, "Enter", "Enter");
        ui.update(0.05);
        draws.poll(host, ui, &env);
        assert!(host.playing());
    };
    let leave = |host: &mut Host, ui: &mut ddi_ui::Ui| {
        ui.update(3.0);
        key(ui, "Escape", "Escape");
        ui.update(0.5);
        draws.poll(host, ui, &env);
        ui.update(1.0);
        assert!(host.idle());
    };

    // Nobody presses: failed, and the store refuses it.
    start(&mut host, &mut ui, Request::song("beatucada", 2, false));
    run(&mut host, &mut ui, usize::MAX);
    assert_eq!(top(&ui), "results");
    assert_eq!(store.len(), 0);
    let shown = results_shown(&ui);
    assert!(host.log.last().unwrap().ends_with("failed true"), "{:?}", host.log);
    assert_eq!(
        shown,
        ResultsShown {
            notes: "This play could not be saved: the store is full.".into(),
            retry: true,
            offset: true,
        }
    );
    leave(&mut host, &mut ui);

    // Autoplayed, then quit (Escape twice).
    start(&mut host, &mut ui, Request::song("beatucada", 2, true));
    run(&mut host, &mut ui, 120);
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
    assert_eq!(top(&ui), "results");
    let shown = results_shown(&ui);
    assert!(host.log.last().unwrap().starts_with("quit"), "{:?}", host.log);
    assert_eq!(
        shown,
        ResultsShown {
            notes: "Autoplay: this play does not count toward your bests. \
                    · This play could not be saved: the store is full."
                .into(),
            retry: true,
            offset: true,
        }
    );
}
