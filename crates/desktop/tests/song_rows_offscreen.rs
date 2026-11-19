//! Song select's list rows (`docs/plans/song-navigation.md`, "A song's
//! row") drawn offscreen in the game's theme on this machine's GPU, over
//! the sample library: each row the chart a press of Play would play,
//! its level and its best's grade with its lamp's mark, "NEW" for a song
//! never played, the level alone for one played on other charts; then an
//! easier difficulty kept, which every row follows; then the DDI
//! estimate's tenths in the levels' place. Wide, on a phone upright and
//! sideways, in English and Japanese. Without a GPU (CI) only the
//! pictures are skipped. `DDI_OFFSCREEN_PPM=<dir>` keeps them
//! (`rows-<scene>-<w>x<h>-<lang>.png`).

mod common;
use std::rc::Rc;

use ddi_ui::screens::select::SelectScreen;
use ddi_ui::screens::{Env, sample};
use ddi_ui::state::ratings;
use ddi_ui::{Ui, UiEvent};

#[test]
fn song_rows_draw_in_the_theme() {
    let mut pictures = common::gpu::Pictures::new();
    for (size, lang) in [
        ([1280u32, 720u32], "en"),
        ([1280, 720], "ja"),
        ([390, 844], "en"),
        ([390, 844], "ja"),
        ([844, 390], "en"),
    ] {
        assert!(ddi_l10n::set_language(lang));
        ddi_platform::storage::install(Rc::new(ddi_platform::storage::MemoryStore::default()));
        ratings::set_mode(ratings::Source::Song, false);
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
        // The sample library, its charts named by digests, the bundled
        // songs' Single charts given DDI estimates.
        let mut data = sample::select();
        for (s, song) in data.songs.iter_mut().enumerate() {
            for c in &mut song.charts {
                let digest = ddi_chart::ChartDigest(((s as u128) << 16) | c.index as u128);
                c.digest = Some(digest);
                ratings::remember_estimate(
                    digest,
                    ddi_library::manifest::ChartEstimate {
                        decimal: c.meter as f32 + 0.37 + 0.11 * s as f32,
                        level: c.meter as u8,
                        scalar: true,
                        studied: true,
                        profile: None,
                    },
                );
            }
        }
        ui.push(SelectScreen::new(
            env.clone(),
            data,
            Some("into-my-dream"),
            Rc::new(|_, _, _, _| {}),
            Rc::new(|_, _| {}),
        ));
        ui.handle(UiEvent::Resize {
            size: css,
            ratio: 1.0,
        });
        common::ui::frames(&mut ui, 1.0);
        let mut keep = |ui: &mut Ui, scene: &str| {
            if let Some(p) = pictures.draw(ui, size) {
                common::shot(
                    &format!("rows-{scene}-{}x{}-{lang}.png", size[0], size[1]),
                    &p,
                    size,
                );
            }
        };
        assert_eq!(env.state.borrow().chart.as_deref(), Some("Hard 12"));
        keep(&mut ui, "1-hard");
        // An easier chart kept: every row follows.
        common::ui::key(&mut ui, "ArrowUp", "ArrowUp");
        common::ui::frames(&mut ui, 0.05);
        common::ui::key(&mut ui, "ArrowUp", "ArrowUp");
        common::ui::frames(&mut ui, 1.0);
        assert!(
            env.state.borrow().chart.as_deref().is_some_and(|c| c.starts_with("Medium")),
            "{:?}",
            env.state.borrow().chart
        );
        keep(&mut ui, "2-medium");
        // The DDI estimate in the levels' place, to a tenth.
        ratings::set_mode(ratings::Source::Estimate, false);
        common::ui::frames(&mut ui, 1.5);
        keep(&mut ui, "3-estimate");
        ratings::set_mode(ratings::Source::Song, false);
    }
    ddi_l10n::set_language("en");
}
