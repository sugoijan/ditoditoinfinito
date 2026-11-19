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
    select_at(data, [1280.0, 720.0]).0
}

/// Song select on `data` at `size`, and its environment (its state).
fn select_at(data: SelectData, size: [f32; 2]) -> (Harness, Env) {
    let mut h = Harness::new(DESIGN, size, 1.0, TextMode::Mirror);
    let play: PlayFn = Rc::new(|_, _, _: &Env, _| {});
    let open: OpenFn = Rc::new(|_, _| {});
    let e = env();
    h.push(SelectScreen::new(e.clone(), data, Some("s0"), play, open));
    h.run(0.6);
    (h, e)
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
    ratings::set_mode(crate::state::ratings::Source::Song, false);
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
    // The ladder's rows (the radar Off: its side has none).
    ratings::set_mode(crate::state::ratings::Source::Msd, false);
    // Song 0's Hard rated, the rest not yet.
    let mut v = [21.37f32, 22.0, 15.0, 14.0, 20.5, 12.0, 13.0, 18.0];
    ratings::done(
        "s0",
        vec![ratings::Rated {
            index: 1,
            digest: ddi_chart::ChartDigest(1),
            values: Some(v),
            ..Default::default()
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
            ..Default::default()
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
    ratings::set_mode(crate::state::ratings::Source::Song, true);
    ratings::done(
        "s0",
        vec![ratings::Rated {
            index: 1,
            digest: ddi_chart::ChartDigest(1),
            values: Some([20.0, 20.0, 15.0, 14.0, 18.0, 12.0, 13.0, 18.0]),
            ..Default::default()
        }],
    );
    let data = songs();
    let hard = data.songs[0].charts[1].clone();
    let (mut h, e) = select_at(data, [1280.0, 720.0]);
    // The Hard picked (the preferred difficulty): rated, its radar, in
    // the ladder's place (no ladder: a row of chart buttons).
    assert!(h.ui.bounds("Radar").is_some(), "the Hard's radar");
    assert!(h.ui.actor("Ladder.Row0").is_none(), "no ladder");
    assert_eq!(radar_said(&e), "shown");
    // The Medium: not rated yet (its song waits), its line instead.
    h.tap(PointerKind::Mouse, "Chart0");
    h.run(0.5);
    assert!(h.ui.bounds("Radar").is_none());
    assert!(h.ui.bounds("RadarNote").is_some());
    assert_eq!(radar_said(&e), "rating");
    h.tap(PointerKind::Mouse, "Chart1");
    h.run(0.5);
    assert!(h.ui.bounds("Radar").is_some());
    // The numbers stay the song's with Chart ratings on the song's.
    assert_eq!(hard.meter_text(), "9");
    ratings::set_mode(crate::state::ratings::Source::Song, false);
    h.run(0.6);
    assert!(h.ui.bounds("Radar").is_none());
    assert!(h.ui.actor("Ladder.Row0").is_some(), "the ladder again");
    assert_eq!(radar_said(&e), "off");
}

/// What the debug view says of the radar.
fn radar_said(e: &Env) -> String {
    e.state.borrow().radar.clone()
}

/// With "Skill radar: Off" song select draws exactly as it did before the
/// radar took the ladder's place (round Z1): a hash of each draw list
/// (`crate::testing::rounded`), recorded on the code before, at three
/// sizes, with Chart ratings on the song's and on MSD (a rated chart and
/// one not rated yet), and after a chart and a song changed by keys.
#[test]
fn song_select_with_the_radar_off_draws_as_pinned() {
    use std::hash::{DefaultHasher, Hash, Hasher};
    let hash = |h: &Harness| {
        let mut s = DefaultHasher::new();
        crate::testing::rounded(&format!("{:?}", h.drawn)).hash(&mut s);
        s.finish()
    };
    let mut got = Vec::new();
    for msd in [false, true] {
        for size in [[1280.0, 720.0], [1024.0, 768.0], [1440.0, 810.0]] {
            let _store = fresh();
            ratings::set_mode(if msd { crate::state::ratings::Source::Msd } else { crate::state::ratings::Source::Song }, false);
            ratings::done(
                "s0",
                vec![ratings::Rated {
                    index: 1,
                    digest: ddi_chart::ChartDigest(1),
                    values: Some([21.37, 22.0, 15.0, 14.0, 20.5, 12.0, 13.0, 18.0]),
                    ..Default::default()
                }],
            );
            let mut h = Harness::new(DESIGN, size, 1.0, TextMode::Own);
            let play: PlayFn = Rc::new(|_, _, _: &Env, _| {});
            let open: OpenFn = Rc::new(|_, _| {});
            h.push(SelectScreen::new(env(), songs(), Some("s0"), play, open));
            h.run(1.0);
            got.push(hash(&h));
            // The other chart (twice Up), then the next song.
            h.key("ArrowUp", "ArrowUp");
            h.run(0.05);
            h.key("ArrowUp", "ArrowUp");
            h.run(1.0);
            got.push(hash(&h));
            h.key("ArrowDown", "ArrowDown");
            h.run(1.0);
            got.push(hash(&h));
        }
    }
    ratings::set_mode(crate::state::ratings::Source::Song, false);
    assert_eq!(got, OFF_PINNED, "{got:?}");
}

const OFF_PINNED: [u64; 18] = [
    9509906775137216922,
    12502977935626601262,
    16829067834613889425,
    17019633511807470159,
    11812824151733589487,
    974805639749831228,
    9509906775137216922,
    12502977935626601262,
    16829067834613889425,
    969422635511668788,
    9312704301971113282,
    14148402059163631482,
    14478074591779853868,
    10430942526480632923,
    7515529346645683890,
    969422635511668788,
    9312704301971113282,
    14148402059163631482,
];

/// Round Z1, part 1's sizes (CSS pixels of a window or a browser's page,
/// zoomed and with its toolbars): with "Skill radar: On" the radar is on
/// every wide one, in the ladder's place; a window under 480 CSS pixels
/// high (the compact layout) has none, and says so.
#[test]
fn the_radar_shows_at_every_wide_size() {
    for (name, size, wide) in [
        ("1280x720", [1280.0, 720.0], true),
        ("1366x768", [1366.0, 768.0], true),
        ("1440x900", [1440.0, 900.0], true),
        ("1536x864", [1536.0, 864.0], true),
        ("1920x1080", [1920.0, 1080.0], true),
        ("16:10", [1680.0, 1050.0], true),
        ("4:3", [1024.0, 768.0], true),
        ("1440x900 window", [1440.0, 790.0], true),
        ("1440x900 window 110%", [1309.0, 718.0], true),
        ("1440x900 window 125%", [1152.0, 632.0], true),
        ("1280x720 window 125%", [1024.0, 488.0], true),
        ("1280x720 window 133%", [962.0, 459.0], false),
    ] {
        let _store = fresh();
        ratings::set_mode(crate::state::ratings::Source::Song, true);
        ratings::done(
            "s0",
            vec![ratings::Rated {
                index: 1,
                digest: ddi_chart::ChartDigest(1),
                values: Some([20.0, 20.0, 15.0, 14.0, 18.0, 12.0, 13.0, 18.0]),
                ..Default::default()
            }],
        );
        let e = env();
        let design = super::design_for(&*e.look, size);
        let mut h = Harness::new(design, size, 1.0, TextMode::Mirror);
        let play: PlayFn = Rc::new(|_, _, _: &Env, _| {});
        let open: OpenFn = Rc::new(|_, _| {});
        h.push(SelectScreen::new(e.clone(), songs(), Some("s0"), play, open));
        h.run(0.6);
        assert_eq!(h.ui.bounds("Radar").is_some(), wide, "{name}");
        assert_eq!(radar_said(&e), if wide { "shown" } else { "phone" }, "{name}");
        assert!(h.ui.actor("Ladder.Row0").is_none(), "{name}: no ladder");
    }
    ratings::set_mode(crate::state::ratings::Source::Song, false);
}

/// Round Z1, the label rule: at every size the radar's side gives it on a
/// wide screen (16:9 and wider, 16:10, a window a little taller, 4:3),
/// in English, Japanese and the long pseudo-language, every label and the
/// caption inside the radar, apart from each other and from the shape,
/// and the shape large (a radius of 40 at least).
#[test]
fn the_radar_s_labels_never_overlap() {
    use crate::radar::{self, Words, overlap};
    use ddi_l10n::msg::select as m;
    for lang in ["en", "ja", "qps-ploc", "qps-wide"] {
        assert!(ddi_l10n::force(lang));
        let full = [
            m::radar_stream_full(),
            m::radar_jumpstream_full(),
            m::radar_handstream_full(),
            m::radar_stamina_full(),
            m::radar_jackspeed_full(),
            m::radar_chordjack_full(),
            m::radar_technical_full(),
        ]
        .map(String::from);
        let short = [
            m::radar_stream(),
            m::radar_jumpstream(),
            m::radar_handstream(),
            m::radar_stamina(),
            m::radar_jackspeed(),
            m::radar_chordjack(),
            m::radar_technical(),
        ]
        .map(String::from);
        // MSD's seven, and the DDI estimate's eight (Evasion the eighth).
        for spokes in [7usize, 8] {
            let mut full = full.to_vec();
            let mut short = short.to_vec();
            let overall: String = if spokes == 8 {
                full.push(m::radar_evasion_full().into());
                short.push(m::radar_evasion().into());
                m::radar_estimate_overall().into()
            } else {
                m::radar_overall().into()
            };
            let words = Words {
                full: &full,
                short: &short,
                overall: &overall,
            };
            let numbers: Vec<String> =
                (0..=spokes).map(|k| radar::number(28.0 + k as f32 * 1.11)).collect();
            for h in [191.0, 208.0, 271.0, 320.0] {
                let size = [338.0, h];
                let p = radar::place(size, &words, &numbers);
                let what = format!("{lang} {spokes} {size:?}: {p:?}");
                assert!(p.radius >= 40.0, "{what}");
                assert_eq!(p.labels.len(), spokes);
                let mut all = p.labels.to_vec();
                all.push(p.caption);
                for (i, a) in all.iter().enumerate() {
                    assert!(
                        a[0] >= -169.0 && a[2] <= 169.0 && a[1] >= -h / 2.0 && a[3] <= h / 2.0,
                        "{what}: {i} outside"
                    );
                    assert!(!radar::meets_shape(*a, p.radius, spokes), "{what}: {i} on the shape");
                    for (j, b) in all.iter().enumerate().skip(i + 1) {
                        assert!(!overlap(*a, *b), "{what}: {i} and {j} overlap");
                    }
                }
            }
        }
    }
    ddi_l10n::unforce();
}

/// Round Z1: the chart buttons are the chart selector on the radar's side:
/// a tap, the keyboard's twice-Down and a controller's twice-Left change the
/// chart chosen (the same gestures as the ladder's); the radar's shape
/// morphs to the new chart's (at once under reduced motion); the chosen
/// chart's lines say what its row said ("Song 9" where MSD is shown).
#[test]
fn the_chart_buttons_pick_and_the_radar_follows() {
    let _store = fresh();
    ratings::set_mode(crate::state::ratings::Source::Msd, true);
    let v = |x: f32| [x, x, x - 4.0, x - 6.0, x - 1.0, x - 8.0, x - 7.0, x - 2.0];
    ratings::done(
        "s0",
        vec![
            ratings::Rated {
                index: 0,
                digest: ddi_chart::ChartDigest(0),
                values: Some(v(12.0)),
                ..Default::default()
            },
            ratings::Rated {
                index: 1,
                digest: ddi_chart::ChartDigest(1),
                values: Some(v(21.0)),
                ..Default::default()
            },
        ],
    );
    let (mut h, e) = select_at(songs(), [1280.0, 720.0]);
    let chart = |e: &Env| e.state.borrow().chart.clone().unwrap_or_default();
    assert_eq!(chart(&e), "Hard 9");
    let card = |h: &Harness| -> Vec<String> { h.texts() };
    assert!(card(&h).iter().any(|t| t == "Song 9"), "{:?}", card(&h));
    // The radar's shape as drawn now.
    let shape = |h: &Harness| -> String {
        let Some(a) = h.ui.actor("Shape") else {
            return String::new();
        };
        let at: Vec<[f32; 3]> = a.children().iter().map(|c| c.current().pos).collect();
        format!("{at:?}")
    };
    // A controller: twice Left (an easier chart), as on the ladder (Up
    // on the list's first song goes to the search field).
    for _ in 0..2 {
        h.pad(crate::MenuAction::Left, true);
        h.pad(crate::MenuAction::Left, false);
        h.run(0.05);
    }
    h.run(0.6);
    assert_eq!(chart(&e), "Medium 6");
    // The keyboard: twice Down (a harder chart).
    h.key("ArrowDown", "ArrowDown");
    h.run(0.05);
    h.key("ArrowDown", "ArrowDown");
    h.run(0.6);
    assert_eq!(chart(&e), "Hard 9");
    // A tap on the Medium's button.
    h.tap(PointerKind::Mouse, "Chart0");
    h.run(0.05);
    assert_eq!(chart(&e), "Medium 6");
    let a = shape(&h);
    h.run(0.05);
    assert_ne!(a, shape(&h), "the shape morphs");
    h.run(0.5);
    let a = shape(&h);
    h.run(0.05);
    assert_eq!(a, shape(&h), "and settles");
    assert_eq!(e.state.borrow().song_id, "s0", "the same song");
    // Reduced motion: no morph.
    e.reduced_motion.set(true);
    h.run(0.3);
    h.tap(PointerKind::Mouse, "Chart1");
    h.run(0.02);
    assert_eq!(chart(&e), "Hard 9");
    let a = shape(&h);
    h.run(0.05);
    assert_eq!(a, shape(&h), "at once");
    ratings::set_mode(crate::state::ratings::Source::Song, false);
}

/// Round Z1: a chart not four lanes says so in the radar's place, with its
/// lines.
#[test]
fn a_chart_of_another_layout_says_it_has_no_rating() {
    let _store = fresh();
    ratings::set_mode(crate::state::ratings::Source::Song, true);
    let mut data = songs();
    for c in &mut data.songs[0].charts {
        c.style = "dance-double".into();
    }
    data.style = Some("dance-double".into());
    let (h, e) = select_at(data, [1280.0, 720.0]);
    assert_eq!(radar_said(&e), "no-layout");
    assert!(h.ui.bounds("RadarNote").is_some());
    assert!(h.texts().iter().any(|t| t == "No rating for this layout"), "{:?}", h.texts());
    assert!(h.texts().iter().any(|t| t == "300 notes"));
    ratings::set_mode(crate::state::ratings::Source::Song, false);
}

/// Round Z1: on the radar's side every chart of the style has its button
/// (up to eight, as the ladder's rows), so a press never moves the buttons
/// under the pointer: each button tapped in turn chooses its own chart.
#[test]
fn every_chart_keeps_its_button_on_the_radar_s_side() {
    let _store = fresh();
    ratings::set_mode(crate::state::ratings::Source::Song, true);
    let mut data = songs();
    data.songs[0].charts = [
        Difficulty::Beginner,
        Difficulty::Easy,
        Difficulty::Medium,
        Difficulty::Hard,
        Difficulty::Challenge,
        Difficulty::Edit,
    ]
    .into_iter()
    .enumerate()
    .map(|(k, difficulty)| ChartEntry {
        style: "dance-single".into(),
        difficulty,
        meter: 2 + 2 * k as u32,
        notes: 300,
        index: k,
        digest: Some(ddi_chart::ChartDigest(100 + k as u128)),
        ..ChartEntry::default()
    })
    .collect();
    let (mut h, e) = select_at(data, [1280.0, 720.0]);
    for k in 0..6 {
        h.tap(PointerKind::Mouse, &format!("Chart{k}"));
        h.run(0.3);
        let want = format!("{} {}", ["Beginner", "Easy", "Medium", "Hard", "Challenge", "Edit"][k], 2 + 2 * k);
        assert_eq!(e.state.borrow().chart.as_deref(), Some(want.as_str()), "Chart{k}");
        assert_eq!(e.state.borrow().screen, "select", "nothing played");
    }
    assert!(h.ui.bounds("Chart6").is_none());
    ratings::set_mode(crate::state::ratings::Source::Song, false);
}

/// Round Z2: the three choices' numbers on the bundled songs' charts, by
/// the calculators' own values (nothing copied): the song's own; MSD,
/// whole, on four lanes; the DDI estimate to a tenth on original Single
/// charts whose number it qualifies, the song's elsewhere (fainter).
#[test]
fn the_three_choices_number_the_bundled_charts() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../assets/songs");
    let mut dirs: Vec<_> = std::fs::read_dir(&root).unwrap().flatten().map(|e| e.path()).collect();
    dirs.sort();
    let mut checked = 0;
    for dir in dirs.iter().filter(|d| d.is_dir()) {
        let Some(file) = std::fs::read_dir(dir).unwrap().flatten().map(|e| e.path()).find(|p| {
            p.extension().is_some_and(|e| e == "ssc" || e == "sm")
        }) else {
            continue;
        };
        let ext = file.extension().unwrap().to_str().unwrap().to_string();
        let song =
            ddi_library::scan::parse_simfile(&std::fs::read_to_string(&file).unwrap(), &ext).unwrap();
        let _store = fresh();
        let mut entries = Vec::new();
        for index in 0..song.charts.len() {
            let mut c = ChartEntry::of_song(&song, index).unwrap();
            let digest = song.chart_digest(index);
            c.digest = Some(digest);
            if let Some(v) = ratings::rate_song(&song, &[index], &[])[0].values {
                ratings::remember(digest, v);
            }
            if let Some(e) = ratings::estimate_chart(&song, index) {
                ratings::remember_estimate(digest, e);
            }
            entries.push((index, c));
        }
        for (index, c) in &entries {
            let chart = &song.charts[*index];
            let what = format!("{} chart {index} ({} {})", dir.display(), chart.layout, chart.meter);
            ratings::set_mode(ratings::Source::Song, false);
            assert_eq!(c.meter_text(), c.meter_text_fine());
            assert!(!c.meter_is_songs());
            ratings::set_mode(ratings::Source::Msd, false);
            let msd = ddi_msd::chart::four_keys(&chart.layout)
                .then(|| ddi_msd::chart::chart_msd(&song, chart))
                .flatten();
            match msd {
                Some(v) => assert_eq!(c.meter_text(), format!("{}", ((v[0] * 100.0).round() / 100.0).round() as i64), "{what}"),
                None => assert!(c.meter_is_songs(), "{what}"),
            }
            ratings::set_mode(ratings::Source::Estimate, false);
            let est = ddi_rating::estimate(&song, *index).ok().filter(|e| e.within_scalar_domain);
            match est {
                Some(e) => {
                    // To a tenth everywhere (round AD).
                    let tenth = format!("{:.1}", (e.decimal * 100.0).round() / 100.0);
                    assert_eq!(c.meter_text(), tenth, "{what}");
                    assert_eq!(c.meter_text_fine(), tenth, "{what}");
                    assert_eq!(c.shown_rating().unwrap().detail(), tenth, "{what}");
                    checked += 1;
                }
                None => assert!(c.meter_is_songs(), "{what}: the song's, fainter"),
            }
        }
    }
    assert!(checked >= 15, "{checked} estimated");
    ratings::set_mode(ratings::Source::Song, false);
}

/// Round Z2: under the DDI estimate the radar's side draws each state: an
/// eight-spoke profile (its numbers each spoke's cost), a flat one in
/// words, one outside what the estimate was studied on, one waiting, and
/// a chart that is not Single.
#[test]
fn the_estimate_s_radar_draws_each_state() {
    use ddi_library::manifest::ChartEstimate;
    let _store = fresh();
    ratings::set_mode(ratings::Source::Estimate, true);
    let est = |decimal: f32, profile: Option<[f32; 8]>, studied: bool| ChartEstimate {
        decimal,
        level: decimal.round() as u8,
        scalar: true,
        studied,
        profile,
    };
    // Song 0: Medium with a profile, Hard with tiny costs; song 1: Medium outside,
    // Hard waiting.
    ratings::remember_estimate(
        ddi_chart::ChartDigest(0),
        est(9.44, Some([2.86, 0.0, 0.0, 1.45, 12.29, 0.0, 0.0, 0.0]), true),
    );
    ratings::remember_estimate(ddi_chart::ChartDigest(1), est(3.0, Some([0.1, 0.0, 0.0, 0.1, 0.0, 0.0, 0.0, 0.0]), true));
    ratings::remember_estimate(ddi_chart::ChartDigest(10), est(18.2, None, false));
    let mut data = songs();
    // Song 2: Double.
    for c in &mut data.songs[2].charts {
        c.style = "dance-double".into();
    }
    let (mut h, e) = select_at(data, [1280.0, 720.0]);
    let said = |e: &Env| e.state.borrow().radar.clone();
    // Song 0's Hard (chosen first): no skill stands out, a regular
    // octagon (round Z3: no words for it any more).
    assert_eq!(said(&e), "shown");
    assert!(h.ui.bounds("RadarNote").is_none());
    assert_eq!(e.state.borrow().chart.as_deref(), Some("Hard 9"));
    h.tap(PointerKind::Mouse, "Chart0");
    h.run(0.6);
    assert_eq!(said(&e), "shown");
    for k in 0..8 {
        assert!(h.ui.bounds_of("Radar", &format!("Label{k}")).is_some(), "spoke {k}");
    }
    assert!(h.ui.bounds_of("Radar", "Label8").is_none());
    let texts = h.texts();
    for want in ["Jack speed", "Evasion", "DDI", "9.4", "Skills, compared with charts this hard"] {
        assert!(texts.iter().any(|t| t == want), "{want} in {texts:?}");
    }
    // Each spoke's number: its skill's level against charts this hard
    // (round Z4), as `spoke_number` makes it.
    let costs = [2.86, 0.0, 0.0, 1.45, 12.29, 0.0, 0.0, 0.0];
    for (k, c) in costs.iter().enumerate() {
        let want = crate::radar::number(crate::radar::spoke_number(9.44, k, *c));
        assert!(texts.contains(&want), "{want} in {texts:?}");
    }
    // An absent skill a quarter under the chart's own estimate.
    assert!(texts.iter().any(|t| t == "7.3"));
    // The estimate's level in the button, the decimal in the detail.
    assert_eq!(e.state.borrow().chart.as_deref(), Some("Medium 6"));
    h.key("ArrowDown", "ArrowDown");
    h.run(0.6);
    // Song 1's Medium (the difficulty chosen last): outside; its Hard
    // waiting (its song offered, nothing rates it here).
    assert_eq!(said(&e), "outside");
    assert!(h.texts().iter().any(|t| t == "Not available"));
    h.tap(PointerKind::Mouse, "Chart1");
    h.run(0.6);
    assert_eq!(said(&e), "rating");
    assert!(h.texts().iter().any(|t| t == "Rating this chart…"));
    h.key("ArrowDown", "ArrowDown");
    h.run(0.6);
    h.key("Tab", "Tab");
    h.run(0.6);
    if e.state.borrow().radar == "no-layout" {
        assert!(h.texts().iter().any(|t| t == "The DDI estimate is for Single charts"));
    }
    ratings::set_mode(ratings::Source::Song, false);
}
