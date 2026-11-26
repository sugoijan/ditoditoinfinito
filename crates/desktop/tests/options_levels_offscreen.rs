//! Options at Basic and at Advanced (`docs/plans/ui-levels.md`, stage 1),
//! the real screen on the player's settings drawn offscreen on this
//! machine's GPU as the desktop window draws it, in English and Japanese
//! (a phone's layout is the web's): Basic (Show advanced options Off)
//! shows fewer rows and a section's count of its advanced options changed
//! at its end (Notes, with no Basic row, only that), and the switch under
//! MENUS. Skipped where there is no GPU adapter (CI).
//! `DDI_OFFSCREEN_PPM=<dir>` keeps the pictures
//! (`options-<level>-<lang>-<w>x<h>-<section>.png`).

mod common;

use std::rc::Rc;

use ddi_desktop::gpu::{self, Gpu};
use ddi_ui::event::{KeyEvent, UiEvent};
use ddi_ui::state::settings::{Level, Settings};
use ddi_ui::{Ui, UiRequest};

#[test]
fn options_at_both_levels() {
    common::indexes();
    let Some(adapter) = common::gpu::adapter_or_skip() else {
        return;
    };
    let (device, queue) = gpu::device(&adapter).unwrap();
    let format = wgpu::TextureFormat::Rgba8Unorm;
    let mut gpu = Gpu::new(device, queue, format);
    for lang in ["en", "ja"] {
        assert!(ddi_l10n::set_language(lang));
        for size in [[1280, 720]] {
            let mut shown: Vec<(Level, &str, usize, Option<String>)> = Vec::new();
            for level in [Level::Basic, Level::Advanced] {
                // Changed at Advanced: a turn and the hits' timing.
                ddi_platform::storage::install(Rc::new(
                    ddi_platform::storage::MemoryStore::default(),
                ));
                let mut settings = Settings {
                    level,
                    show_deltas: true,
                    language: Some(lang.into()),
                    ..Settings::default()
                };
                settings.transform.turn = ddi_engine::Turn::Mirror;
                settings.save();
                let fold: ddi_ui::dev_widgets::FoldFn = Rc::new(|s: &str| s.to_string());
                let (mut ui, env, _) = ddi_desktop::shell::theme_ui("start", fold);
                ui.handle(UiEvent::Resize {
                    size: [size[0] as f32, size[1] as f32],
                    ratio: 1.0,
                });
                let (screen, shared) = ddi_desktop::options::screen(&env, None, None);
                ui.push(screen);
                let apply = |gpu: &mut Gpu, requests: Vec<UiRequest>| {
                    for r in requests {
                        if let UiRequest::FieldPreview(rect) = r {
                            gpu.field.set_rect(rect);
                            if rect.is_some() {
                                gpu.field.set_settings(&Settings::load());
                            }
                        }
                    }
                };
                let tick = |ui: &mut Ui, gpu: &mut Gpu| {
                    ui.update(1.0 / 60.0);
                    apply(gpu, ui.take_requests());
                    if let Some(s) = shared.take_edited() {
                        gpu.field.set_settings(&s);
                    }
                    gpu.draw_offscreen(ui, size, format)
                };
                let key = |ui: &mut Ui, gpu: &mut Gpu, code: &str| {
                    apply(gpu, ui.handle(UiEvent::Key(KeyEvent::down(code, code))).requests);
                    apply(gpu, ui.handle(UiEvent::Key(KeyEvent::up(code, code))).requests);
                    tick(ui, gpu);
                };
                let mut frames = 0;
                while gpu.field.skin_name().as_deref() != Some("Infinito") || gpu.field.reading() {
                    tick(&mut ui, &mut gpu);
                    frames += 1;
                    assert!(frames < 500, "{:?}", gpu.field.skin_name());
                    std::thread::sleep(std::time::Duration::from_millis(5));
                }
                assert_eq!(env.state.borrow().options_level, format!("{level:?}"));
                for (section, row) in [("Notes", None), ("Display", Some("show_advanced"))] {
                    for _ in 0..12 {
                        if env.state.borrow().section == section {
                            break;
                        }
                        key(&mut ui, &mut gpu, "Tab");
                    }
                    assert_eq!(env.state.borrow().section, section);
                    if let Some(row) = row {
                        for _ in 0..60 {
                            if env.state.borrow().option_id == row {
                                break;
                            }
                            key(&mut ui, &mut gpu, "ArrowDown");
                        }
                        assert_eq!(env.state.borrow().option_id, row);
                    }
                    for _ in 0..30 {
                        tick(&mut ui, &mut gpu);
                    }
                    let pixels = tick(&mut ui, &mut gpu);
                    let name = format!(
                        "options-{}-{lang}-{}x{}-{}.png",
                        format!("{level:?}").to_lowercase(),
                        size[0],
                        size[1],
                        section.to_lowercase()
                    );
                    common::shot(&name, &pixels, size);
                    let rows = env.state.borrow().option_rows.clone();
                    shown.push((level, section, rows.len(), rows.last().cloned()));
                }
            }
            eprintln!("{lang} {size:?}: {shown:?}");
            let count = |level: Level, section: &str| {
                shown
                    .iter()
                    .find(|(l, s, _, _)| *l == level && *s == section)
                    .unwrap()
                    .clone()
            };
            for section in ["Notes", "Display"] {
                assert!(count(Level::Basic, section).2 < count(Level::Advanced, section).2);
                assert_eq!(
                    count(Level::Basic, section).3,
                    Some(format!("advanced.changed.{section}")),
                    "{section}: its count last"
                );
            }
        }
    }
    let _ = ddi_l10n::set_language("en");
}
