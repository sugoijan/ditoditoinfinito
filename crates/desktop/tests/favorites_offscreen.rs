//! Favorite songs on song select (`docs/plans/song-navigation.md`,
//! "Favorites") drawn offscreen in the game's theme on this machine's
//! GPU, over the sample library: the song's heart an outline, then filled
//! by F with the row's small heart and the favorites' pack at the top,
//! then that pack open; wide and on a phone sideways and upright, in
//! English and Japanese. Without a GPU (CI) only the pictures are skipped.
//! `DDI_OFFSCREEN_PPM=<dir>` keeps them (`<scene>-<w>x<h>-<lang>.ppm`).

mod common;
use std::rc::Rc;

use common::gpu::seen;
use common::online::{drawn, keep};
use ddi_ui::screens::select::SelectScreen;
use ddi_ui::screens::{Env, sample};
use ddi_ui::{Ui, UiEvent};

#[test]
fn the_heart_and_the_favorites_pack_draw_in_the_theme() {
    let mut pictures = common::gpu::Pictures::new();
    for (size, lang) in [
        ([1280u32, 720u32], "en"),
        ([1280, 720], "ja"),
        ([844, 390], "en"),
        ([390, 844], "ja"),
    ] {
        assert!(ddi_l10n::set_language(lang));
        // Each size starts with none kept.
        ddi_platform::storage::install(Rc::new(ddi_platform::storage::MemoryStore::default()));
        let css = [size[0] as f32, size[1] as f32];
        let look = Rc::new(ddi_theme::Infinito::new());
        let env = Env::new(look.clone(), Rc::new(|s: &str| s.to_string()));
        env.reduced_motion.set(true);
        let mut ui = Ui::new(
            ddi_ui::screens::design_for(&*look, css),
            ddi_ui::TextMode::Own,
            false,
        );
        ui.set_focus_ring(false);
        ui.push(SelectScreen::new(
            env.clone(),
            sample::select(),
            Some("into-my-dream"),
            Rc::new(|_, _, _, _| {}),
            Rc::new(|_, _| {}),
        ));
        ui.handle(UiEvent::Resize {
            size: css,
            ratio: 1.0,
        });
        common::ui::frames(&mut ui, 1.0);
        let state = || env.state.borrow().clone();
        assert_eq!(state().favorite, Some(false));
        let shot = pictures
            .draw(&mut ui, size)
            .inspect(|p| keep("favorite-outline", size, lang, p));
        // The heart, drawn where it is.
        let b = ui.bounds("Favorite").expect("the heart");
        let box_of = |b: [f32; 4]| {
            let v = ui.viewport();
            let (w, h) = (v.screen.width, v.screen.height);
            [b[0] / w, b[1] / h, b[2] / w, b[3] / h]
        };
        assert!(seen(&shot, |p| drawn(p, size, box_of(b))));
        // F: filled, the row's heart, the pack at the top.
        common::ui::key(&mut ui, "KeyF", "f");
        common::ui::frames(&mut ui, 0.5);
        assert_eq!(state().favorite, Some(true));
        assert_eq!(state().favorites, ["into-my-dream"]);
        if let Some(p) = pictures.draw(&mut ui, size) {
            keep("favorite-filled", size, lang, &p);
        }
        // Another, then the pack opened from the top.
        common::ui::key(&mut ui, "ArrowDown", "ArrowDown");
        common::ui::frames(&mut ui, 0.5);
        common::ui::key(&mut ui, "KeyF", "f");
        common::ui::frames(&mut ui, 0.5);
        assert_eq!(state().favorites.len(), 2);
        for _ in 0..12 {
            if state().header.as_deref() == Some(&*ddi_l10n::msg::select::pack_favorites()) {
                break;
            }
            common::ui::key(&mut ui, "ArrowUp", "ArrowUp");
            common::ui::frames(&mut ui, 0.5);
        }
        assert_eq!(
            state().header.as_deref(),
            Some(&*ddi_l10n::msg::select::pack_favorites())
        );
        if let Some(p) = pictures.draw(&mut ui, size) {
            keep("favorite-header", size, lang, &p);
        }
        common::ui::key(&mut ui, "Enter", "Enter");
        common::ui::frames(&mut ui, 0.5);
        common::ui::key(&mut ui, "ArrowDown", "ArrowDown");
        common::ui::frames(&mut ui, 0.5);
        if let Some(p) = pictures.draw(&mut ui, size) {
            keep("favorite-pack", size, lang, &p);
        }
        assert_eq!(state().favorites, ["into-my-dream", "some-things-must"]);
        assert_eq!(state().song_id, "into-my-dream");
    }
    ddi_l10n::set_language("en");
}
