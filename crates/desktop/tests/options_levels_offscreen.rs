//! Options at Basic and at Advanced (`docs/plans/ui-levels.md`, stage 1),
//! the real screen on the player's settings drawn offscreen on this
//! machine's GPU as the desktop window draws it, in English and Japanese
//! (a phone's layout is the web's): Basic (Show advanced options Off)
//! shows fewer rows and a section's count of its advanced options changed
//! at its end (Play: Speed, Reverse and its plain assists, then that
//! count), every section of the tree drawn at both (stage 3), the switch
//! first in Interface. Skipped where there is no GPU adapter (CI).
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
            let mut shown: Vec<(Level, String, usize, Option<String>)> = Vec::new();
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
                // Every section the level shows, from the first, by Tab.
                let sections = env.state.borrow().option_sections.clone();
                assert_eq!(env.state.borrow().section, sections[0]);
                for (i, section) in sections.iter().enumerate() {
                    if i > 0 {
                        key(&mut ui, &mut gpu, "Tab");
                    }
                    assert_eq!(&env.state.borrow().section, section);
                    for _ in 0..30 {
                        tick(&mut ui, &mut gpu);
                    }
                    let pixels = tick(&mut ui, &mut gpu);
                    let name = format!(
                        "options-{}-{lang}-{}x{}-{:02}-{}.png",
                        format!("{level:?}").to_lowercase(),
                        size[0],
                        size[1],
                        i,
                        section.to_lowercase().replace(' ', "-")
                    );
                    common::shot(&name, &pixels, size);
                    let rows = env.state.borrow().option_rows.clone();
                    shown.push((level, section.clone(), rows.len(), rows.last().cloned()));
                    // Infinito's Note colors, at both levels, its words the
                    // game's messages (`ddi_ui::skin_text`): focused, so its
                    // description shows.
                    if section == "Arrows" {
                        let at = rows.iter().position(|r| r == "skin.Colors");
                        let at = at.unwrap_or_else(|| panic!("{rows:?}"));
                        let from = rows.iter().position(|r| *r == env.state.borrow().option_id);
                        for _ in from.unwrap_or(0)..at {
                            key(&mut ui, &mut gpu, "ArrowDown");
                        }
                        assert_eq!(env.state.borrow().option_id, "skin.Colors");
                        for _ in 0..30 {
                            tick(&mut ui, &mut gpu);
                        }
                        let pixels = tick(&mut ui, &mut gpu);
                        let name = name.replace(".png", "-note-colors.png");
                        common::shot(&name, &pixels, size);
                    }
                }
                // The switch first in Interface.
                if let Some(at) = sections.iter().position(|s| s == "Interface") {
                    // Tab past the last section comes back to the first.
                    for _ in 0..=at {
                        key(&mut ui, &mut gpu, "Tab");
                    }
                    assert_eq!(env.state.borrow().section, "Interface");
                    assert_eq!(env.state.borrow().option_id, "show_advanced");
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
            for section in ["Play", "Screen"] {
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
