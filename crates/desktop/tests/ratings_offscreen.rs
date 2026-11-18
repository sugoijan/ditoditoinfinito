//! Chart ratings on song select (`docs/plans/msd-ratings.md`) drawn
//! offscreen in the game's theme on this machine's GPU, over the sample
//! library: with Chart ratings on MSD and the skill radar On, the bundled
//! song's four-lane charts show their MSD (its real numbers, rated here)
//! and the radar for the chart picked, the numbers listed with the pointer
//! on it; charts without MSD keep the song's number, fainter. Wide, on a
//! phone sideways and upright (no radar there), in English and Japanese.
//! Without a GPU (CI) only the pictures are skipped.
//! `DDI_OFFSCREEN_PPM=<dir>` keeps them (`<scene>-<w>x<h>-<lang>.ppm`).

mod common;
use std::rc::Rc;

use common::gpu::seen;
use common::online::{drawn, keep};
use ddi_ui::event::{Modifiers, PointerEvent, PointerKind, PointerPhase};
use ddi_ui::screens::select::SelectScreen;
use ddi_ui::screens::{Env, sample};
use ddi_ui::state::ratings;
use ddi_ui::{Ui, UiEvent};

/// The sample library with the real MSD of the bundled song's
/// dance-single charts, each given a digest of its own.
fn rated_sample() -> ddi_ui::screens::data::SelectData {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../assets/songs/into-my-dream");
    let file = std::fs::read_dir(&root)
        .unwrap()
        .flatten()
        .map(|e| e.path())
        .find(|p| p.extension().is_some_and(|e| e == "ssc" || e == "sm"))
        .expect("the bundled simfile");
    let ext = file.extension().unwrap().to_str().unwrap().to_string();
    let song = ddi_library::scan::parse_simfile(&std::fs::read_to_string(&file).unwrap(), &ext).unwrap();
    let mut data = sample::select();
    let entry = data
        .songs
        .iter_mut()
        .find(|s| s.id == "into-my-dream")
        .expect("the bundled song");
    // Each sample chart its real chart's numbers, by difficulty and meter.
    for (k, c) in entry
        .charts
        .iter_mut()
        .filter(|c| c.style == "dance-single")
        .enumerate()
    {
        let Some(i) = (0..song.charts.len()).find(|&i| {
            let r = &song.charts[i];
            r.layout == "dance-single" && r.difficulty == c.difficulty && r.meter == c.meter
        }) else {
            continue;
        };
        let rated = ratings::rate_song(&song, &[i]);
        let d = ddi_chart::ChartDigest(0x5eed_0000 + k as u128);
        c.digest = Some(d);
        ratings::remember(d, rated[0].values.expect("rated"));
    }
    data
}

#[test]
fn msd_numbers_and_the_skill_radar_draw_in_the_theme() {
    let mut pictures = common::gpu::Pictures::new();
    for (size, lang) in [
        ([1280u32, 720u32], "en"),
        ([1280, 720], "ja"),
        ([844, 390], "en"),
        ([390, 844], "ja"),
    ] {
        assert!(ddi_l10n::set_language(lang));
        ddi_platform::storage::install(Rc::new(ddi_platform::storage::MemoryStore::default()));
        ratings::set_mode(true, true);
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
            rated_sample(),
            Some("into-my-dream"),
            Rc::new(|_, _, _, _| {}),
            Rc::new(|_, _| {}),
        ));
        ui.handle(UiEvent::Resize {
            size: css,
            ratio: 1.0,
        });
        common::ui::frames(&mut ui, 1.0);
        let wide = size == [1280, 720];
        let radar = ui.bounds("Radar");
        assert_eq!(radar.is_some(), wide, "the radar shows where there is room");
        let shot = pictures
            .draw(&mut ui, size)
            .inspect(|p| keep("ratings", size, lang, p));
        if let Some(b) = radar {
            let v = ui.viewport();
            let (w, h) = (v.screen.width, v.screen.height);
            let frac = [b[0] / w, b[1] / h, b[2] / w, b[3] / h];
            assert!(seen(&shot, |p| drawn(p, size, frac)));
            // The pointer on it: its numbers listed.
            let at = [(b[0] + b[2]) / 2.0, (b[1] + b[3]) / 2.0];
            ui.handle(UiEvent::Pointer(PointerEvent {
                id: 1,
                kind: PointerKind::Mouse,
                phase: PointerPhase::Move,
                pos: at,
                buttons: 0,
                modifiers: Modifiers::default(),
                time: 2000.0,
            }));
            common::ui::frames(&mut ui, 0.3);
            if let Some(p) = pictures.draw(&mut ui, size) {
                keep("ratings-radar-hover", size, lang, &p);
            }
        }
        // Another song (its numbers the song's, fainter; no radar).
        common::ui::key(&mut ui, "ArrowDown", "ArrowDown");
        common::ui::frames(&mut ui, 0.5);
        assert!(ui.bounds("Radar").is_none(), "no radar without MSD");
        if let Some(p) = pictures.draw(&mut ui, size) {
            keep("ratings-unrated", size, lang, &p);
        }
    }
    ratings::set_mode(false, false);
    ddi_l10n::set_language("en");
}
