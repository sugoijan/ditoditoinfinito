//! The start prompt's play options (`docs/plans/start-prompt.md`): changed
//! on the prompt they are this play's (its skin read again for Reverse,
//! the record keeping them) and its retry's, never the stored settings';
//! the next song's prompt starts from those; Save as default stores them.
//! On this machine's GPU offscreen and a fake clock with no sound device;
//! skipped where there is no GPU adapter (CI).

mod common;
use std::cell::RefCell;
use std::rc::Rc;
use std::sync::atomic::Ordering;
use std::time::Duration;

use ddi_desktop::gpu;
use ddi_desktop::play::Request;
use ddi_desktop::play::host::{Device, Host};
use ddi_platform::history::{MemoryPlayStore, PlayStore};
use ddi_ui::state::history::Plays;
use ddi_ui::state::settings::Settings;
use common::ui::key;

const SIZE: [u32; 2] = [960, 540];

#[test]
fn play_options_are_this_plays_kept_with_it_and_saved_when_asked() {
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
    let options = |env: &ddi_ui::screens::Env| {
        env.state
            .borrow()
            .prompt
            .as_ref()
            .map(|p| p.options.clone())
            .unwrap_or_default()
    };
    let until_ready = |host: &mut Host, ui: &mut ddi_ui::Ui| {
        let mut polls = 0;
        while !host.prompt_ready() || options(&env).len() < 6 {
            host.poll(ui, &env, Some(dev()), 48_000);
            ui.update(0.02);
            std::thread::sleep(Duration::from_millis(10));
            polls += 1;
            assert!(polls < 2000, "never ready: {:?}", host.prompt_waiting());
        }
        ui.update(0.3);
    };
    // No announcer (on by default since 2026-11-17): the play's own line
    // is the log's first.
    Settings {
        announcer: None,
        ..Settings::load()
    }
    .save();
    let stored = Settings::load();
    assert!(!stored.reverse);

    *host.requested.borrow_mut() = Some(Request::song("beatucada", 2, true));
    until_ready(&mut host, &mut ui);
    assert_eq!(options(&env)[1], "reverse=Off");
    // Modifiers sit below the bindings; Up from Start reaches the last row.
    for _ in 0..5 {
        key(&mut ui, "ArrowUp", "ArrowUp");
    }
    assert_eq!(ui.focused(), Some("Opt.reverse"));
    key(&mut ui, "ArrowRight", "ArrowRight");
    key(&mut ui, "ArrowUp", "ArrowUp");
    key(&mut ui, "ArrowRight", "ArrowRight");
    ui.update(0.05);
    assert_eq!(options(&env)[..2], ["speed=×2.75", "reverse=On"]);
    // The skin is read again (its Lua told the new direction); the
    // summary says them; the settings are as they were.
    host.poll(&mut ui, &env, Some(dev()), 48_000);
    assert!(
        host.prompt_waiting().iter().any(|w| w == "noteskin"),
        "{:?}",
        host.prompt_waiting()
    );
    assert_eq!(Settings::load().speed, stored.speed);
    assert!(!Settings::load().reverse);
    until_ready(&mut host, &mut ui);
    // Enter starts (from the row: Start is what Enter does).
    key(&mut ui, "Enter", "Enter");
    host.poll(&mut ui, &env, Some(dev()), 48_000);
    let mut polls = 0;
    while !host.playing() {
        host.poll(&mut ui, &env, Some(dev()), 48_000);
        ui.update(0.02);
        polls += 1;
        assert!(polls < 2000, "never played");
    }
    let mut frames = 0;
    while host.playing() {
        let now = clock.fetch_add(1_000_000_000 / 60, Ordering::SeqCst) + 1_000_000_000 / 60;
        common::rig::draw(&mut host, &dev(), &target, &mut ui, &env, now);
        frames += 1;
        assert!(frames < 60 * 300, "the play never ended");
    }
    let _ = device.poll(wgpu::PollType::wait_indefinitely());
    // Kept as played: the engine's options and the settings' snapshot.
    let records: Rc<RefCell<Vec<serde_json::Value>>> = Rc::default();
    let r = records.clone();
    store.export(
        10,
        Box::new(move |v| {
            r.borrow_mut().extend(
                v.into_iter()
                    .map(|e| serde_json::from_str(&e.record).unwrap()),
            )
        }),
        Box::new(|_| {}),
    );
    let records = records.borrow();
    assert_eq!(records.len(), 1);
    let rec = &records[0];
    assert_eq!(rec["settings"]["reverse"], true, "{}", rec["settings"]);
    assert_eq!(rec["settings"]["speed"], 2.75);
    assert_eq!(
        rec["options"]["scroll"]["reverse"], true,
        "{}",
        rec["options"]
    );
    assert!(
        host.log[0].contains("[77, 0, 0, 0, 0, 0]"),
        "{:?}",
        host.log
    );
    // Retry: the same options.
    ui.update(3.0);
    key(&mut ui, "KeyR", "r");
    ui.update(0.5);
    host.poll(&mut ui, &env, Some(dev()), 48_000);
    ui.update(0.5);
    assert!(host.prompting());
    assert_eq!(options(&env)[..2], ["speed=×2.75", "reverse=On"]);
    // Save as default: the rows' values stored.
    key(&mut ui, "ArrowUp", "ArrowUp");
    assert_eq!(ui.focused(), Some("SaveOptions"));
    key(&mut ui, "Enter", "Enter");
    ui.update(0.05);
    let s = Settings::load();
    assert!(s.reverse && (s.speed - 2.75).abs() < 1e-9);
    assert!(
        env.state
            .borrow()
            .prompt
            .as_ref()
            .is_some_and(|p| p.options_saved && !p.options_differ)
    );
    assert!(!host.playing());
    // Put back, and the next song's prompt starts from what is stored.
    s.clone().save();
    let mut back = Settings::load();
    back.reverse = false;
    back.speed = stored.speed;
    back.save();
    key(&mut ui, "Escape", "Escape");
    host.poll(&mut ui, &env, Some(dev()), 48_000);
    ui.update(0.5);
    assert!(host.idle());
    *host.requested.borrow_mut() = Some(Request::song("beatucada", 2, true));
    until_ready(&mut host, &mut ui);
    assert_eq!(options(&env)[..2], ["speed=×2.5", "reverse=Off"]);
}
