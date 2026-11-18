//! Chart ratings on song select (`docs/plans/msd-ratings.md`) drawn
//! offscreen in the game's theme on this machine's GPU, over the sample
//! library: with Chart ratings on MSD and the skill radar On, the bundled
//! song's four-lane charts show their MSD (its real numbers, rated here)
//! and, in the ladder's place (round Z1), a row of chart buttons, the
//! chosen chart's lines and its radar with its numbers, each chart chosen
//! by its button, the labels never overlapping as the platform's text
//! measures them; charts without MSD keep the song's number, fainter, and
//! a line in the radar's place. Wide at three window sizes, on a phone
//! sideways and upright (no radar there), in English, Japanese and the
//! long pseudo-language. Without a GPU (CI) only the pictures are skipped.
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

/// The labels' and the caption's boxes (virtual pixels) as drawn, which
/// must not overlap, and stay inside the radar.
fn radar_boxes(ui: &Ui) -> Vec<(String, [f32; 4])> {
    let mut out: Vec<(String, [f32; 4])> = (0..7)
        .filter_map(|k| ui.bounds_of("Radar", &format!("Label{k}")).map(|b| (format!("Label{k}"), b)))
        .collect();
    out.extend(ui.bounds_of("Radar", "Caption").map(|b| ("Caption".to_string(), b)));
    out
}

fn assert_apart(ui: &Ui, what: &str) {
    let boxes = radar_boxes(ui);
    assert_eq!(boxes.len(), 8, "{what}: every label and the caption");
    let panel = ui.bounds_of("Radar", "Panel").expect("the radar's panel");
    for (i, (a, ra)) in boxes.iter().enumerate() {
        assert!(
            ra[0] >= panel[0] - 0.5 && ra[2] <= panel[2] + 0.5 && ra[1] >= panel[1] - 0.5 && ra[3] <= panel[3] + 0.5,
            "{what}: {a} {ra:?} outside the radar {panel:?}"
        );
        for (b, rb) in &boxes[i + 1..] {
            let apart = ra[2] <= rb[0] || rb[2] <= ra[0] || ra[3] <= rb[1] || rb[3] <= ra[1];
            assert!(apart, "{what}: {a} {ra:?} and {b} {rb:?} overlap");
        }
    }
}

#[test]
fn msd_numbers_and_the_skill_radar_draw_in_the_theme() {
    let mut pictures = common::gpu::Pictures::new();
    for (size, lang) in [
        ([1280u32, 720u32], "en"),
        ([1280, 720], "ja"),
        ([1280, 720], "qps-ploc"),
        ([1280, 800], "en"),
        ([1280, 800], "ja"),
        ([1024, 768], "en"),
        ([1024, 768], "qps-ploc"),
        ([844, 390], "en"),
        ([390, 844], "ja"),
    ] {
        assert!(ddi_l10n::force(lang), "{lang}");
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
        let wide = size[1] >= 480 && size[0] >= 600;
        let radar = ui.bounds("Radar");
        assert_eq!(radar.is_some(), wide, "the radar on every wide screen, none on a phone");
        assert!(ui.actor("Ladder.Row0").is_none(), "no ladder: the radar's side, or a phone's");
        let shot = pictures
            .draw(&mut ui, size)
            .inspect(|p| keep("ratings", size, lang, p));
        let what = format!("{size:?} {lang}");
        if let Some(b) = radar {
            assert_apart(&ui, &what);
            let v = ui.viewport();
            let (w, h) = (v.screen.width, v.screen.height);
            let frac = [b[0] / w, b[1] / h, b[2] / w, b[3] / h];
            assert!(seen(&shot, |p| drawn(p, size, frac)));
            // Each chart chosen by its button: its radar, apart.
            for k in 0..5 {
                let name = format!("Chart{k}");
                let Some(c) = ui.bounds(&name) else { continue };
                // Virtual pixels to the window's.
                let k_css = css[0] / ui.viewport().screen.width;
                let at = [(c[0] + c[2]) / 2.0 * k_css, (c[1] + c[3]) / 2.0 * k_css];
                for phase in [PointerPhase::Down, PointerPhase::Up] {
                    ui.handle(UiEvent::Pointer(PointerEvent {
                        id: 1,
                        kind: PointerKind::Mouse,
                        phase,
                        pos: at,
                        buttons: u32::from(phase == PointerPhase::Down),
                        modifiers: Modifiers::default(),
                        time: 2000.0 + k as f64 * 100.0,
                    }));
                }
                common::ui::frames(&mut ui, 0.3);
                // Measured as a frame is (drawn where there is a GPU).
                let _ = pictures.draw(&mut ui, size);
                let st = env.state.borrow().clone();
                assert_eq!(st.song_id, "into-my-dream", "{what} {name}: the same song");
                assert!(st.chart.as_ref().is_some_and(|c| st.ladder.contains(c)), "{what} {name}");
                // (A chart of the sample without its real chart's numbers
                // waits for a rating no shell makes here.)
                if env.state.borrow().radar == "shown" {
                    assert_apart(&ui, &format!("{what} {name}"));
                }
            }
            if let Some(p) = pictures.draw(&mut ui, size) {
                keep("ratings-chosen", size, lang, &p);
            }
        }
        // Another song (its numbers the song's, fainter; its line in the
        // radar's place).
        common::ui::key(&mut ui, "ArrowDown", "ArrowDown");
        common::ui::frames(&mut ui, 0.5);
        assert!(ui.bounds("Radar").is_none(), "no radar without MSD");
        assert_eq!(ui.bounds("RadarNote").is_some(), wide, "its line on a wide screen");
        if let Some(p) = pictures.draw(&mut ui, size) {
            keep("ratings-unrated", size, lang, &p);
        }
    }
    ratings::set_mode(false, false);
    ddi_l10n::unforce();
}
