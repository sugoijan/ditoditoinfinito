//! Song select's list rows in the theme: what a row shows of the chart
//! that would be played, and what scrolling a long list costs.

use std::rc::Rc;

use ddi_actor::Drawn;
use ddi_ui::TextMode;
use ddi_ui::screens::data::{Best, ChartEntry, Difficulty, SelectData, SongEntry};
use ddi_ui::screens::select::SelectScreen;
use ddi_ui::screens::{DESIGN, Env, sample};
use ddi_ui::testing::Harness;

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
        h.wheel(
            [1000.0, 400.0],
            [0.0, 120.0],
            ddi_ui::event::WheelSource::Lines,
        );
        h.frame(1.0 / 60.0);
        count(&h);
    }
    for _ in 0..60 {
        h.frame(1.0 / 60.0);
        count(&h);
    }
    let took = started.elapsed();
    let shown: Vec<String> = h
        .texts()
        .into_iter()
        .filter(|t| t.starts_with("Synthetic"))
        .take(3)
        .collect();
    eprintln!("at the end the rows show {shown:?}");
    eprintln!(
        "5000 songs: {frames} frames, {quads} quads ({:.1} a frame), {texts} texts ({:.1} a frame), {:.2} ms a frame",
        quads as f64 / frames as f64,
        texts as f64 / frames as f64,
        took.as_secs_f64() * 1000.0 / frames as f64
    );
}

/// What a song's row shows, read from its actors: its level, its grade,
/// its flag's color (none hidden), whether "NEW" shows, and its level's
/// alpha.
#[derive(Debug, PartialEq)]
struct Shown {
    level: String,
    grade: String,
    flag: Option<[f32; 4]>,
    new: bool,
}

fn part<'a>(row: &'a ddi_actor::Actor, name: &str) -> &'a ddi_actor::Actor {
    let path = ddi_ui::hit::find(row, name).unwrap_or_else(|| panic!("no {name}"));
    ddi_ui::hit::actor_at(row, &path).unwrap()
}

fn text_of(a: &ddi_actor::Actor) -> String {
    match &a.kind {
        ddi_actor::Kind::Text(t) => t.text().to_string(),
        _ => panic!("{} is no text", a.name),
    }
}

/// Every row the wheel holds, by its title.
fn rows(h: &Harness) -> std::collections::HashMap<String, Shown> {
    let mut out = std::collections::HashMap::new();
    for i in 0..80 {
        let Some(row) = h.ui.actor(&format!("Wheel#{i}")) else {
            continue;
        };
        if ddi_ui::hit::find(row, "Level").is_none() {
            continue;
        }
        let flag = part(row, "Flag");
        let alpha = flag.current().diffuse()[3];
        out.insert(
            text_of(part(row, "Title")),
            Shown {
                level: text_of(part(row, "Level")),
                grade: text_of(part(row, "Grade")),
                flag: (alpha > 0.0).then(|| flag.current().diffuse()),
                new: part(row, "New").current().diffuse()[3] > 0.0,
            },
        );
    }
    out
}

fn chart(d: Difficulty, meter: u32, index: usize, best: Option<(&str, &str)>) -> ChartEntry {
    ChartEntry {
        style: "dance-single".into(),
        difficulty: d,
        meter,
        notes: 200,
        index,
        best: best.map(|(grade, lamp)| Best {
            grade: grade.into(),
            lamp: lamp.into(),
            plays: 1,
            ..Best::default()
        }),
        ..ChartEntry::default()
    }
}

fn song(id: &str, played: bool, charts: Vec<ChartEntry>) -> SongEntry {
    SongEntry {
        id: id.into(),
        title: id.into(),
        artist: "Artist".into(),
        bpm_text: "150".into(),
        length: 100.0,
        charts,
        played,
        ..SongEntry::default()
    }
}

/// Five songs: never played; played on another chart only; played on
/// the Hard chart (its grade and full combo); without a Medium or Hard
/// chart (its Challenge, the chart closest to Hard, with its grade); no
/// Single chart at all.
fn five() -> SelectData {
    use Difficulty::*;
    let all = |best_on: Option<(Difficulty, &str, &str)>| {
        [Beginner, Easy, Medium, Hard, Challenge]
            .iter()
            .enumerate()
            .map(|(k, &d)| {
                let best = best_on.filter(|b| b.0 == d).map(|b| (b.1, b.2));
                chart(d, 2 + 2 * k as u32, k, best)
            })
            .collect::<Vec<_>>()
    };
    let songs = vec![
        song("never", false, all(None)),
        song("other", true, all(Some((Easy, "A", "Clear")))),
        song("mine", true, all(Some((Hard, "AA", "FullCombo")))),
        song(
            "nohard",
            true,
            vec![
                chart(Beginner, 1, 0, None),
                chart(Easy, 3, 1, None),
                chart(Challenge, 12, 2, Some(("S", "Clear"))),
            ],
        ),
        song(
            "double",
            false,
            vec![ChartEntry {
                style: "dance-double".into(),
                ..chart(Hard, 9, 0, None)
            }],
        ),
    ];
    SelectData {
        styles: sample::styles(),
        songs,
        style: Some("dance-single".into()),
        ..SelectData::default()
    }
}

fn shown(level: &str, grade: &str, flag: Option<&str>, new: bool) -> Shown {
    let theme = Infinito::new();
    Shown {
        level: level.into(),
        grade: grade.into(),
        flag: flag.map(|f| theme.c(&format!("Lamp.{f}"))),
        new,
    }
}

/// Round AE: a row shows the chart a press of Play would play (the
/// difficulty kept, else the closest, as the song's side has it): its
/// level, and its best's grade with its lamp's mark, or "NEW" for a song
/// never played, or the level alone; every row follows a change of
/// difficulty; a song without Single charts says so, as before.
#[test]
fn a_row_shows_the_chart_play_would_play_and_its_best() {
    assert!(ddi_l10n::force("en"));
    ddi_ui::state::ratings::set_mode(ddi_ui::state::ratings::Source::Song, false);
    let theme = Rc::new(Infinito::new());
    let env = env(&theme);
    for (size, design) in [
        ([1280.0, 720.0], DESIGN),
        ([390.0, 844.0], ddi_ui::screens::COMPACT),
    ] {
        let mut h = harness(size, design);
        h.push(SelectScreen::new(
            env.clone(),
            five(),
            Some("other"),
            Rc::new(|_, _, _: &Env, _| {}),
            Rc::new(|_, _| {}),
        ));
        h.run(1.0);
        let rows = rows(&h);
        assert_eq!(rows["never"], shown("8", "", None, true), "{size:?}");
        assert_eq!(rows["other"], shown("8", "", None, false), "{size:?}");
        assert_eq!(
            rows["mine"],
            shown("8", "AA", Some("FullCombo"), false),
            "{size:?}"
        );
        assert_eq!(
            rows["nohard"],
            shown("12", "S", Some("Clear"), false),
            "{size:?}"
        );
        assert!(!rows["double"].new, "{size:?}");
        assert!(
            h.texts().iter().any(|t| t == "no Single charts"),
            "{size:?}"
        );
        // The song's side shows the chart its row does.
        assert_eq!(
            env.state.borrow().chart.as_deref(),
            Some("Hard 8"),
            "{size:?}"
        );
        // An easier chart (Up, Up on the second song: on the first, Up
        // goes to the search): every row follows, the songs without a
        // Medium chart to the closest (the easier on a tie).
        h.key("ArrowUp", "ArrowUp");
        h.run(0.05);
        h.key("ArrowUp", "ArrowUp");
        h.run(1.0);
        assert_eq!(
            env.state.borrow().chart.as_deref(),
            Some("Medium 6"),
            "{size:?}"
        );
        let rows = super::song_row_tests::rows(&h);
        assert_eq!(rows["never"], shown("6", "", None, true), "{size:?}");
        assert_eq!(rows["mine"], shown("6", "", None, false), "{size:?}");
        assert_eq!(rows["nohard"], shown("3", "", None, false), "{size:?}");
        // To the song without one: its side shows the row's chart.
        for _ in 0..2 {
            h.key("ArrowDown", "ArrowDown");
            h.run(0.6);
        }
        assert_eq!(env.state.borrow().song_id, "nohard", "{size:?}");
        assert_eq!(
            env.state.borrow().chart.as_deref(),
            Some("Easy 3"),
            "{size:?}"
        );
    }
}

/// A best that comes later (the history read after the list shows, a
/// play just kept) shows on its row where the row is: the list is not
/// moved.
#[test]
fn a_best_that_comes_later_shows_on_its_row_in_place() {
    use ddi_ui::screens::select::{FEED, Feed, SharedFeed};
    assert!(ddi_l10n::force("en"));
    ddi_ui::state::ratings::set_mode(ddi_ui::state::ratings::Source::Song, false);
    let theme = Rc::new(Infinito::new());
    let env = env(&theme);
    let feed: SharedFeed = Rc::new(std::cell::RefCell::new(Feed::default()));
    let mut h = harness([1280.0, 720.0], DESIGN);
    h.push(
        SelectScreen::new(
            env.clone(),
            five(),
            Some("other"),
            Rc::new(|_, _, _: &Env, _| {}),
            Rc::new(|_, _| {}),
        )
        .with_shell(Rc::new(|_, _| {}), feed.clone()),
    );
    h.run(1.0);
    let at = |h: &Harness| {
        (0..20)
            .filter_map(|i| {
                let row = format!("Wheel#{i}");
                let title = h.ui.actor(&row).map(|r| text_of(part(r, "Title")))?;
                Some((title, h.ui.bounds_of(&row, "Plate")?))
            })
            .collect::<std::collections::BTreeMap<_, _>>()
    };
    let before = at(&h);
    assert!(rows(&h)["never"].new);
    let mut data = five();
    data.songs[0].played = true;
    data.songs[0].charts[3].best = Some(Best {
        grade: "AAA".into(),
        lamp: "PerfectFullCombo".into(),
        ..Best::default()
    });
    feed.borrow_mut().data = Some(data);
    h.ui.broadcast(ddi_ui::Message::new(FEED));
    h.frame(1.0 / 60.0);
    assert_eq!(
        rows(&h)["never"],
        shown("8", "AAA", Some("PerfectFullCombo"), false)
    );
    assert_eq!(at(&h), before);
}

/// Round LO: under Charts: Adaptive a room's row shows the chart this
/// player chose for its song this session (on its side or on a stage's
/// Ready card), as its side does; a song with none shows the chart song
/// select would give (the difficulty kept). Before, every row showed the
/// latter.
#[test]
fn a_rooms_row_shows_the_chart_chosen_for_its_song() {
    assert!(ddi_l10n::force("en"));
    ddi_ui::state::ratings::set_mode(ddi_ui::state::ratings::Source::Song, false);
    let theme = Rc::new(Infinito::new());
    let env = env(&theme);
    let mut data = five();
    for (s, song) in data.songs.iter_mut().enumerate() {
        for c in &mut song.charts {
            c.digest = Some(ddi_chart::ChartDigest((s * 10 + c.index) as u128 + 1));
        }
    }
    let mine = data.songs[2].clone();
    let easy = mine.charts.iter().find(|c| c.difficulty == Difficulty::Easy).unwrap().clone();
    let room = ddi_ui::state::room::RoomClient::shared(0, 1);
    room.borrow_mut().set_rules_seen(true);
    assert!(room.borrow().songs_without_charts(), "practice plays Adaptive");
    room.borrow_mut().remember_start(&mine, &easy);
    let mut h = harness([1280.0, 720.0], DESIGN);
    h.push(
        SelectScreen::new(
            env.clone(),
            data,
            Some("never"),
            Rc::new(|_, _, _: &Env, _| {}),
            Rc::new(|_, _| {}),
        )
        .with_room(room.clone(), Rc::new(|_, _| {})),
    );
    h.run(1.0);
    let rows = rows(&h);
    assert_eq!(rows["mine"].level, easy.meter.to_string(), "its own Easy");
    assert_eq!(rows["other"].level, "8", "the Hard kept");
    // Another chart chosen for it later: its row follows.
    let medium = mine.charts.iter().find(|c| c.difficulty == Difficulty::Medium).unwrap().clone();
    room.borrow_mut().remember_start(&mine, &medium);
    h.run(0.2);
    assert_eq!(super::song_row_tests::rows(&h)["mine"].level, medium.meter.to_string());
}
