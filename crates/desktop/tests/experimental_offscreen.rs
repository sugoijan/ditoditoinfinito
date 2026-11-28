//! The experimental settings screen (`docs/plans/ui-levels.md`, stage 4)
//! drawn offscreen on this machine's GPU as the desktop window draws it
//! (`--screen experimental`), wide and at a phone's sizes, in English and
//! Japanese: the warning first, a switch turned on (marked in gold, its
//! Reset on), and the end of the page with Reset all and Forget this
//! screen; then Options' Interface at Advanced listing it under
//! Troubleshooting. Skipped where there is no GPU adapter (CI).
//! `DDI_OFFSCREEN_PPM=<dir>` keeps the pictures
//! (`experimental-<lang>-<w>x<h>-<part>.png`).

mod common;

use std::rc::Rc;

use ddi_desktop::gpu::{self, Gpu};
use ddi_ui::Ui;
use ddi_ui::screens::data::Platform;
use ddi_ui::screens::{COMPACT, DESIGN, Env};
use ddi_ui::state::settings::{Level, Settings};

fn env() -> Env {
    let mut env = Env::new(
        Rc::new(ddi_theme::Infinito::new()),
        Rc::new(|s: &str| s.to_string()),
    );
    env.platform = Platform::Desktop;
    env.reduced_motion.set(true);
    env
}

fn frames(ui: &mut Ui, n: usize) {
    for _ in 0..n {
        ui.update(1.0 / 60.0);
    }
}

#[test]
fn the_experimental_settings_draw_wide_and_on_a_phone() {
    let Some(adapter) = common::gpu::adapter_or_skip() else {
        return;
    };
    let (device, queue) = gpu::device(&adapter).expect("device");
    let format = wgpu::TextureFormat::Rgba8Unorm;
    let mut gpu = Gpu::new(device, queue, format);
    for lang in ["en", "ja"] {
        assert!(ddi_l10n::set_language(lang));
        for size in [[1280, 720], [390, 844]] {
            ddi_platform::storage::install(Rc::new(ddi_platform::storage::MemoryStore::default()));
            Settings {
                multiplayer_rules_first: true,
                language: Some(lang.into()),
                ..Settings::default()
            }
            .save();
            let compact = size[0] < 600 || size[1] < 480;
            let env = env();
            let mut ui = Ui::new(if compact { COMPACT } else { DESIGN }, ddi_ui::TextMode::Own, false);
            ui.set_focus_ring(false);
            common::ui::resize(&mut ui, size);
            ui.push(ddi_desktop::shell::experimental_screen(&env));
            frames(&mut ui, 30);
            assert_eq!(env.state.borrow().screen, "experimental");
            assert!(Settings::load().experimental_seen, "opening it marks it seen");
            let name = |part: &str| format!("experimental-{lang}-{}x{}-{part}.png", size[0], size[1]);
            let pixels = gpu.draw_offscreen(&mut ui, size, format);
            common::shot(&name("top"), &pixels, size);
            // Down to the page's last button: its end in view.
            let items = env.state.borrow().page_items.clone();
            assert!(items.contains(&"button:turn:rooms.rules_first".to_string()), "{items:?}");
            assert_eq!(items.last().map(String::as_str), Some("button:forget"));
            for _ in 0..items.len() {
                common::ui::key_step(&mut ui, "ArrowDown", "ArrowDown");
            }
            assert_eq!(env.state.borrow().page_focus, items.len() as i32 - 1);
            frames(&mut ui, 30);
            let pixels = gpu.draw_offscreen(&mut ui, size, format);
            common::shot(&name("end"), &pixels, size);
        }
    }
    // Options' Interface at Advanced, the row under Troubleshooting.
    assert!(ddi_l10n::set_language("en"));
    ddi_platform::storage::install(Rc::new(ddi_platform::storage::MemoryStore::default()));
    Settings {
        level: Level::Advanced,
        experimental_seen: true,
        multiplayer_rules_first: true,
        ..Settings::default()
    }
    .save();
    let fold: ddi_ui::dev_widgets::FoldFn = Rc::new(|s: &str| s.to_string());
    let (mut ui, env, _) = ddi_desktop::shell::theme_ui("start", fold);
    common::ui::resize(&mut ui, [1280, 720]);
    let (screen, _shared) = ddi_desktop::options::screen(&env, None, None);
    ui.push(screen);
    frames(&mut ui, 30);
    let sections = env.state.borrow().option_sections.clone();
    let at = sections.iter().position(|s| s == "Interface").expect("Interface");
    for _ in 0..at {
        common::ui::key_step(&mut ui, "Tab", "Tab");
    }
    assert_eq!(env.state.borrow().section, "Interface");
    let rows = env.state.borrow().option_rows.clone();
    assert_eq!(rows.last().map(String::as_str), Some("experimental"), "{rows:?}");
    let mut moves = 0;
    while env.state.borrow().option_id != "experimental" {
        common::ui::key_step(&mut ui, "ArrowDown", "ArrowDown");
        moves += 1;
        assert!(moves <= rows.len(), "{rows:?}");
    }
    frames(&mut ui, 30);
    let pixels = gpu.draw_offscreen(&mut ui, [1280, 720], format);
    common::shot("experimental-options-interface.png", &pixels, [1280, 720]);
    // Its Open: the screen over Options, and Back to Options.
    common::ui::key_step(&mut ui, "Enter", "Enter");
    frames(&mut ui, 30);
    assert_eq!(env.state.borrow().screen, "experimental");
    common::ui::key_step(&mut ui, "Escape", "Escape");
    frames(&mut ui, 30);
    assert_eq!(env.state.borrow().section, "Interface");
}
