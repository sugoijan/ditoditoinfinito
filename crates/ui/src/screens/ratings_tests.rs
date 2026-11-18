//! Chart ratings on song select (`docs/plans/msd-ratings.md`): with both
//! options Off nothing is read, written or offered and the numbers are the
//! songs'; with Chart ratings on MSD a rated chart's number is its MSD
//! (whole in its button, to a tenth in its row, the song's own said in
//! the row's line), a chart not rated yet keeps the song's, fainter, and
//! its song is offered for rating, the focused one first; the radar
//! shows for a rated chart with its option On.

use std::rc::Rc;

use ddi_actor::Kind;
use ddi_platform::storage::{self, Area, MemoryStore};

use super::data::{ChartEntry, Difficulty, SelectData, SongEntry};
use super::select::{OpenFn, PlayFn, SelectScreen};
use super::test_kit::env;
use super::{DESIGN, Env, sample};
use crate::{PointerKind, TextMode};
use crate::state::ratings;
use crate::testing::Harness;

/// Four songs of two four-lane charts each (digests `10 s + k`).
fn songs() -> SelectData {
    let songs = (0..4u128)
        .map(|s| SongEntry {
            id: format!("s{s}"),
            title: format!("Song {s}"),
            artist: "Artist".into(),
            bpm_text: "150".into(),
            length: 120.0,
            charts: [(Difficulty::Medium, 6), (Difficulty::Hard, 9)]
                .into_iter()
                .enumerate()
                .map(|(k, (difficulty, meter))| ChartEntry {
                    style: "dance-single".into(),
                    difficulty,
                    meter,
                    notes: 300,
                    index: k,
                    digest: Some(ddi_chart::ChartDigest(10 * s + k as u128)),
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

fn select(data: SelectData) -> Harness {
    let mut h = Harness::new(DESIGN, [1280.0, 720.0], 1.0, TextMode::Mirror);
    let play: PlayFn = Rc::new(|_, _, _: &Env, _| {});
    let open: OpenFn = Rc::new(|_, _| {});
    h.push(SelectScreen::new(env(), data, Some("s0"), play, open));
    h.run(0.6);
    h
}

/// The ladder's row `k`'s part `part`.
fn row_text(h: &Harness, k: usize, part: &str) -> String {
    let Some(row) = h.ui.actor(&format!("Ladder.Row{k}")) else {
        return String::new();
    };
    let mut row = row.clone();
    match crate::widgets::part(&mut row, part).map(|a| &a.kind) {
        Some(Kind::Text(t)) => t.text().to_string(),
        _ => String::new(),
    }
}

fn fresh() -> Rc<MemoryStore> {
    let store = Rc::new(MemoryStore::default());
    storage::install(store.clone());
    ratings::reset();
    store
}

#[test]
fn with_both_options_off_nothing_is_rated_read_or_written() {
    let _store = fresh();
    ratings::set_mode(false, false);
    let data = songs();
    let hard = data.songs[0].charts[1].clone();
    let mut h = select(data);
    h.run(1.0);
    assert_eq!(hard.meter_text(), "9");
    assert_eq!(hard.meter_text_fine(), "9");
    assert!(!hard.meter_is_songs());
    assert_eq!(row_text(&h, 1, "Subtitle"), "300 notes");
    assert_eq!(ratings::status().pending, 0);
    assert_eq!(ratings::next(), None);
    assert!(storage::get(Area::Local, ratings::KEY).is_none());
    assert!(h.ui.bounds("Radar").is_none());
}

#[test]
fn msd_takes_the_numbers_place_and_the_rest_wait_for_rating() {
    let _store = fresh();
    ratings::set_mode(true, true);
    // Song 0's Hard rated, the rest not yet.
    let mut v = [21.37f32, 22.0, 15.0, 14.0, 20.5, 12.0, 13.0, 18.0];
    ratings::done(
        "s0",
        vec![ratings::Rated {
            index: 1,
            digest: ddi_chart::ChartDigest(1),
            values: Some(v),
        }],
    );
    let data = songs();
    let (medium, hard) = (data.songs[0].charts[0].clone(), data.songs[0].charts[1].clone());
    let mut h = select(data);
    // The focused song first.
    let job = ratings::next().expect("a song to rate");
    assert_eq!(job.song, "s0");
    assert_eq!(job.charts, vec![0], "its unrated chart only");
    // The rated Hard: its MSD, whole in its button, to a tenth in its
    // row, the song's 9 in the row's line.
    assert_eq!(hard.meter_text(), "21");
    assert_eq!(hard.meter_text_fine(), "21.4");
    assert!(!hard.meter_is_songs());
    assert_eq!(row_text(&h, 1, "Subtitle"), "Song 9 · 300 notes");
    // The Medium: the song's own, drawn fainter.
    assert_eq!(medium.meter_text(), "6");
    assert!(medium.meter_is_songs());
    assert_eq!(row_text(&h, 0, "Subtitle"), "300 notes");
    // A new rating shows without anything else happening.
    v[0] = 11.04;
    ratings::done(
        "s0",
        vec![ratings::Rated {
            index: 0,
            digest: ddi_chart::ChartDigest(0),
            values: Some(v),
        }],
    );
    h.run(0.6);
    assert_eq!(medium.meter_text_fine(), "11.0");
    assert_eq!(row_text(&h, 0, "Subtitle"), "Song 6 · 300 notes");
    assert!(storage::get(Area::Local, ratings::KEY).is_some(), "kept");
}

#[test]
fn the_radar_shows_for_a_rated_chart_with_its_option_on() {
    let _store = fresh();
    ratings::set_mode(false, true);
    ratings::done(
        "s0",
        vec![ratings::Rated {
            index: 1,
            digest: ddi_chart::ChartDigest(1),
            values: Some([20.0, 20.0, 15.0, 14.0, 18.0, 12.0, 13.0, 18.0]),
        }],
    );
    let data = songs();
    let hard = data.songs[0].charts[1].clone();
    let mut h = select(data);
    // The Hard picked (the preferred difficulty): rated, its radar.
    assert!(h.ui.bounds("Radar").is_some(), "the Hard's radar");
    // The Medium: not rated, none.
    h.tap(PointerKind::Mouse, "Ladder.Row0");
    h.run(0.5);
    assert!(h.ui.bounds("Radar").is_none());
    h.tap(PointerKind::Mouse, "Ladder.Row1");
    h.run(0.5);
    assert!(h.ui.bounds("Radar").is_some());
    // The numbers stay the song's with Chart ratings on the song's.
    assert_eq!(hard.meter_text(), "9");
    ratings::set_mode(false, false);
    h.run(0.6);
    assert!(h.ui.bounds("Radar").is_none());
}
