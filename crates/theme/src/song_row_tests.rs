//! Song select's list rows in the theme: what a row shows of the chart
//! that would be played, and what scrolling a long list costs.

use std::rc::Rc;

use ddi_actor::Drawn;
use ddi_ui::screens::data::{Best, ChartEntry, Difficulty, SelectData, SongEntry};
use ddi_ui::screens::select::SelectScreen;
use ddi_ui::screens::{DESIGN, Env, sample};
use ddi_ui::testing::Harness;
use ddi_ui::TextMode;

use super::*;

fn env(theme: &Rc<Infinito>) -> Env {
    let fold: ddi_ui::dev_widgets::FoldFn =
        Rc::new(|s: &str| ddi_library::search::Fold::nfkd(&ddi_library::search::Nfkd, s));
    Env::new(theme.clone(), fold)
}

fn harness(size: [f32; 2], design: [f32; 2]) -> Harness {
    let mut h = Harness::new(DESIGN, size, 1.0, TextMode::Mirror);
    h.ui.set_design(design);
    h
}

/// `n` songs of five Single charts each, every third with a best on its
/// Hard chart, in one pack.
fn library(n: usize) -> SelectData {
    let songs = (0..n)
        .map(|s| SongEntry {
            id: format!("s{s}"),
            title: format!("Synthetic song number {s} with a long title"),
            artist: format!("Artist {}", s % 37),
            bpm_text: "150".into(),
            length: 120.0,
            charts: Difficulty::ALL[..5]
                .iter()
                .enumerate()
                .map(|(k, &d)| ChartEntry {
                    style: "dance-single".into(),
                    difficulty: d,
                    meter: 2 + 3 * k as u32,
                    notes: 300,
                    index: k,
                    best: (s % 3 == 0 && d == Difficulty::Hard).then(|| Best {
                        grade: "AA".into(),
                        lamp: "FullCombo".into(),
                        lamp_text: "FC".into(),
                        plays: 1,
                        ..Best::default()
                    }),
                    ..ChartEntry::default()
                })
                .collect(),
            ..SongEntry::default()
        })
        .collect();
    SelectData {
        styles: sample::styles(),
        songs,
        style: Some("dance-single".into()),
        ..SelectData::default()
    }
}

/// Scrolling 5,000 songs: Down held for four seconds (up to 24 rows a
/// second), then a flick of the wheel; the frames' draws counted. A
/// measure, printed (`--ignored --nocapture`), for a change to the rows
/// to compare before and after.
#[test]
#[ignore]
fn scrolling_five_thousand_songs_measured() {
    let theme = Rc::new(Infinito::new());
    let env = env(&theme);
    let data = library(5000);
    let mut h = harness([1280.0, 720.0], DESIGN);
    h.push(SelectScreen::new(
        env.clone(),
        data,
        Some("s0"),
        Rc::new(|_, _, _: &Env, _| {}),
        Rc::new(|_, _| {}),
    ));
    h.run(1.0);
    let started = std::time::Instant::now();
    let (mut frames, mut quads, mut texts) = (0usize, 0usize, 0usize);
    let mut count = |h: &Harness| {
        frames += 1;
        for d in &h.drawn {
            match d {
                Drawn::Quad(_) => quads += 1,
                Drawn::Text(_) => texts += 1,
                _ => {}
            }
        }
    };
    h.key_down("ArrowDown", "ArrowDown");
    for _ in 0..240 {
        h.frame(1.0 / 60.0);
        count(&h);
    }
    h.key_up("ArrowDown", "ArrowDown");
    for _ in 0..60 {
        h.frame(1.0 / 60.0);
        count(&h);
    }
    for _ in 0..120 {
        h.wheel([1000.0, 400.0], [0.0, 120.0], ddi_ui::event::WheelSource::Lines);
        h.frame(1.0 / 60.0);
        count(&h);
    }
    for _ in 0..60 {
        h.frame(1.0 / 60.0);
        count(&h);
    }
    let took = started.elapsed();
    let shown: Vec<String> = h.texts().into_iter().filter(|t| t.starts_with("Synthetic")).take(3).collect();
    eprintln!("at the end the rows show {shown:?}");
    eprintln!(
        "5000 songs: {frames} frames, {quads} quads ({:.1} a frame), {texts} texts ({:.1} a frame), {:.2} ms a frame",
        quads as f64 / frames as f64,
        texts as f64 / frames as f64,
        took.as_secs_f64() * 1000.0 / frames as f64
    );
}
