//! Calibration as the window runs it (`docs/plans/desktop.md`, 7.6), on
//! this machine's GPU offscreen and on a fake clock with no sound device:
//! calibration's menu starts the arrows-only test (autoplayed 30 ms late,
//! as `--auto --bias 30` asks), its prompt, the play to its end, its
//! outcome settled with no card (the visual offset moved by what was
//! measured, a line for the menu), the sound-only test's prompt next,
//! and Escape there goes back to the menu. Skipped where there is no GPU
//! adapter (CI).

mod common;

use std::rc::Rc;
use std::sync::atomic::Ordering;

use ddi_desktop::gpu;
use ddi_desktop::play::host::{Device, Host};
use ddi_ui::screens::Env;
use ddi_ui::state::settings::Settings;
use common::ui::key_step as key;

const SIZE: [u32; 2] = [960, 540];

fn top(ui: &ddi_ui::Ui) -> String {
    ui.layers()
        .iter()
        .rev()
        .find(|l| !l.overlay)
        .map(|l| l.name.clone())
        .unwrap_or_default()
}

/// Presses the screen's item `want` (a page's `button:<id>`, the
/// prompt's `Action.<id>`) from the keys.
fn press(ui: &mut ddi_ui::Ui, env: &Env, want: &str) {
    for _ in 0..40 {
        let (items, focus) = {
            let s = env.state.borrow();
            (s.page_items.clone(), s.page_focus)
        };
        let Some(at) = items.iter().position(|i| *i == want) else {
            panic!("no {want} in {items:?}");
        };
        if focus == at as i32 {
            key(ui, "Enter", "Enter");
            return;
        }
        if focus < at as i32 {
            key(ui, "ArrowDown", "ArrowDown");
        } else {
            key(ui, "ArrowUp", "ArrowUp");
        }
    }
    panic!("{want} never focused");
}

#[test]
fn a_test_from_the_menu_to_its_result_and_the_next() {
    let home = std::env::temp_dir().join(format!("ddi-calibration-flow-{}", std::process::id()));
    // SAFETY: set before any thread of this test reads the environment.
    unsafe { std::env::set_var("DDI_HOME", &home) };
    let Some(adapter) = common::gpu::adapter_or_skip() else {
        return;
    };
    let (device, queue) = gpu::device(&adapter).unwrap();
    let (mut ui, env) = common::ui::theme("select", common::ui::lowercase(), SIZE, false);
    ui.update(0.5);
    let songs = ddi_desktop::songs::Songs::new(Vec::new());
    songs.testing.set((true, 0.030));
    let mut host = Host::new(songs.play.clone(), None);
    {
        let songs = songs.clone();
        host.menu = Some(Rc::new(move |env: &Env, ui: &mut ddi_ui::Ui| {
            ui.push(ddi_desktop::calibrate::menu(env, &songs, None))
        }));
    }
    let clock = common::rig::fake_clock(&mut host);
    let formats = common::gpu::formats();
    let dev = || Device {
        device: &device,
        queue: &queue,
        formats,
    };
    let target = common::gpu::Target::new(&device, SIZE);

    // The menu; its first button starts the arrows-only test.
    ui.push(ddi_desktop::calibrate::menu(&env, &songs, None));
    ui.update(0.5);
    assert_eq!(top(&ui), "calibration");
    press(&mut ui, &env, "button:run:visual");
    // Its prompt, ready once its skin is read.
    let mut waited = 0;
    while !host.prompt_ready() {
        host.poll(&mut ui, &env, Some(dev()), 48_000);
        ui.update(0.05);
        std::thread::sleep(std::time::Duration::from_millis(10));
        waited += 1;
        assert!(waited < 1000, "{:?}", host.prompt_step());
    }
    ui.update(0.5);
    assert_eq!(top(&ui), "play-prompt", "{:?}", ui.layers());
    // The arrows-only test is muted by design: no volume on its card.
    let prompt_volume = |env: &Env| env.state.borrow().prompt.as_ref().map(|p| p.volume);
    assert_eq!(prompt_volume(&env), Some(None));
    key(&mut ui, "Enter", "Enter");
    host.poll(&mut ui, &env, Some(dev()), 48_000);
    assert!(host.playing(), "{:?}", ui.layers());
    let mut frames = 0;
    while host.playing() {
        let now = clock.fetch_add(1_000_000_000 / 60, Ordering::SeqCst) + 1_000_000_000 / 60;
        common::rig::draw(&mut host, &dev(), &target, &mut ui, &env, now);
        frames += 1;
        assert!(frames < 60 * 120, "the test never ended");
    }
    let _ = device.poll(wgpu::PollType::wait_indefinitely());
    ui.update(0.5);
    // Its outcome settled with no card: the visual offset (no devices:
    // the default's) moved by what the late autoplayer measured, the
    // menu told in a line, and the guided flow's next test's prompt up.
    assert!(
        host.log
            .iter()
            .any(|l| l.starts_with("calibration visual: +30")),
        "{:?}",
        host.log
    );
    let visual = Settings::load().visual_offset;
    assert!((visual - 0.030).abs() < 0.004, "{visual}");
    // Said in a new player's plain words (Basic, `docs/plans/ui-levels.md`,
    // stage 2; at Advanced "the visual offset").
    let lines = ddi_ui::screens::calibration::last_runs();
    assert!(
        lines
            .iter()
            .any(|l| l.starts_with("The arrows-only test set the picture's delay to +3")),
        "{lines:?}"
    );
    host.poll(&mut ui, &env, Some(dev()), 48_000);
    ui.update(0.5);
    host.poll(&mut ui, &env, Some(dev()), 48_000);
    assert!(host.prompting(), "{:?}", ui.layers());
    assert!(env.state.borrow().song.starts_with("Calibration 2/3"));
    assert_eq!(top(&ui), "play-prompt", "{:?}", ui.layers());
    // The sound-only test's card has the volume (its click track plays at
    // it; round AD).
    ui.update(0.1);
    assert!(
        matches!(prompt_volume(&env), Some(Some(_))),
        "{:?}",
        prompt_volume(&env)
    );
    // Escape: back to the menu.
    key(&mut ui, "Escape", "Escape");
    ui.update(0.5);
    host.poll(&mut ui, &env, Some(dev()), 48_000);
    ui.update(0.5);
    assert!(!host.prompting() && !host.playing());
    assert_eq!(top(&ui), "calibration", "{:?}", ui.layers());
    let _ = std::fs::remove_dir_all(home);
}
