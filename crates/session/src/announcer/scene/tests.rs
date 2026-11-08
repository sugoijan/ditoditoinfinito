use std::cell::RefCell;
use std::rc::Rc;

use ddi_engine::rules::{FullCombo, Judgement};
use ddi_library::announcer::{Comment, Grade, Line, Spoken};
use ddi_ui::screens::select::comment;

use ddi_ui::screens::results::{ResultsActions, ResultsScreen};
use ddi_ui::screens::select::SelectScreen;
use ddi_ui::screens::{DESIGN, Env};
use ddi_ui::testing::Harness;
use ddi_ui::{Mark, Screen, TextMode};

use super::super::tests::Fake;
use super::super::{Announcer, CHEER_DELAY, Timing, Voice, cheers, results_lines as tried_lines};
use super::*;

fn env() -> Env {
    Env::new(
        Rc::new(ddi_ui::look::Plain),
        Rc::new(|s: &str| s.to_lowercase()),
    )
}

/// What the mark of `mark` rings on `screen`, once it is in.
fn marked(screen: impl Screen + 'static, mark: Mark) -> Option<[f32; 4]> {
    let mut h = Harness::new(DESIGN, [1280.0, 720.0], 1.0, TextMode::Own);
    h.push(screen);
    h.run(1.0);
    h.ui.mark(Some(mark));
    h.ui.mark_bounds()
}

fn select_of(scene: &MenuScene) -> SelectScreen {
    SelectScreen::new(
        env(),
        scene.data.clone(),
        Some(&scene.song),
        Rc::new(|_, _, _, _| {}),
        Rc::new(|_, _| {}),
    )
}

/// A session begun a while ago, and now.
const STARTED: f64 = 1.8e12;
const NOW: f64 = STARTED + 600_000.0;

fn results_lines() -> Vec<Line> {
    Line::all()
        .into_iter()
        .filter(|l| l.spoken() == Spoken::Results)
        .collect()
}

/// What the results say over `scene` with a voice of `lines`, each one
/// sound a second long: the folders and when each starts after the
/// results open.
fn said(scene: &ResultsScene, lines: &[Line]) -> Vec<(String, f64)> {
    let mut v = Voice::default();
    for &l in lines {
        v.expect([l]);
        v.add(l, 1.0);
    }
    let timing = Timing {
        start: 0.0,
        offset: 0.0,
        rate: 1.0,
    };
    let mut a: Announcer<f64> = Announcer::new(true, 0.5, Some(scene.prior), timing, None);
    a.set_voice(Rc::new(RefCell::new(v)));
    let mut audio = Fake::default();
    a.results(
        &mut audio,
        10.0,
        &scene.ending(),
        &scene.grades,
        scene.combo_w4,
    );
    a.report
        .spoken
        .iter()
        .map(|(f, at)| (f.clone(), at - 10.0))
        .collect()
}

#[test]
fn each_comment_scene_opens_on_a_chart_that_gets_it() {
    for c in Comment::ALL {
        let scene = menu(Line::SelectMusic(c), STARTED, NOW).unwrap();
        assert!(scene.choose);
        let song = scene
            .data
            .songs
            .iter()
            .find(|s| s.id == scene.song)
            .unwrap();
        let chart = &song.charts[scene.chart];
        assert_eq!(chart.style, "dance-single");
        assert_eq!(comment(chart, STARTED), c, "{c:?}");
        assert_eq!(
            scene.data.resume,
            Some((scene.song.clone(), Some(scene.chart)))
        );
        assert!(scene.data.notices.is_empty());
        assert!(scene.start.is_none());
        // What it points at is on song select, on the chart's row (the
        // plain look's rows have no parts: the whole row).
        let ring = marked(select_of(&scene), scene.mark)
            .unwrap_or_else(|| panic!("{c:?}: nothing at {:?}", scene.mark));
        let row = marked(select_of(&scene), Mark::Chart).unwrap();
        assert!(ring[2] - ring[0] <= row[2] - row[0] + 0.01, "{c:?}");
    }
}

#[test]
fn the_intro_scene_comes_from_the_start_menu_and_only_shows() {
    let scene = menu(Line::SelectMusicIntro, STARTED, NOW).unwrap();
    assert!(!scene.choose);
    assert!(scene.start.is_some());
    assert_eq!(scene.mark, Mark::Screen);
    assert!(marked(select_of(&scene), scene.mark).is_some());
    assert!(menu(Line::Ready, STARTED, NOW).is_none());
    assert!(menu(Line::Quit, STARTED, NOW).is_none());
}

/// Every results line but aaaaa (no play gets it) is what its scene says
/// first, with every line recorded, under each ruleset the player may
/// have: the precedence picks it.
#[test]
fn each_results_line_is_said_first_in_its_scene() {
    for ruleset in ["itg", "ddr-a", "sm5", "ffr", "nonsense"] {
        for line in results_lines() {
            let scene = results(line, ruleset, &|_| true);
            if line == Line::Grade(Grade::Aaaaa) {
                assert!(scene.is_none());
                continue;
            }
            let scene = scene.unwrap_or_else(|| panic!("{ruleset}: no scene for {line:?}"));
            // A failed play's grade: StepMania's e, and Etterna's f where
            // the pack has no e.
            let voice: Vec<Line> = results_lines()
                .into_iter()
                .filter(|l| line != Line::Grade(Grade::F) || *l != Line::Grade(Grade::E))
                .collect();
            let said = said(&scene, &voice);
            let first = match line {
                Line::Cheer => &said[1],
                _ => &said[0],
            };
            assert_eq!(first.0, line.folder(), "{ruleset}: {said:?}");
            // What an example's scene fetches: the lines tried, the cheer
            // where it follows.
            let tried = tried_lines(
                Some(scene.prior),
                &scene.ending(),
                &scene.grades,
                scene.combo_w4,
            );
            match line {
                Line::Cheer => assert!(cheers(&scene.ending(), &scene.grades)),
                // f is tried as e, which the pack lacks here.
                Line::Grade(Grade::F) => assert!(tried.contains(&Line::Grade(Grade::E))),
                _ => assert!(tried.contains(&line), "{ruleset}: {line:?} in {tried:?}"),
            }
            assert!(!scene.caption.text().is_empty());
            // A record and a first clear show the chart's best before.
            let history = matches!(line, Line::NewRecord | Line::FirstClear);
            assert_eq!(scene.before.is_some(), history, "{line:?}");
        }
    }
}

/// What each results scene points at is on the results screen.
#[test]
fn each_results_scene_points_at_what_its_line_is_said_for() {
    for line in results_lines() {
        let Some(scene) = results(line, "itg", &|_| true) else {
            continue;
        };
        let screen = ResultsScreen::new(env(), scene.data.clone(), ResultsActions::default());
        assert!(
            marked(screen, scene.mark).is_some(),
            "{line:?}: nothing at {:?}",
            scene.mark
        );
    }
}

/// Song select before a record's results shows the best it beats; before
/// a first clear's, the fails.
#[test]
fn a_record_and_a_first_clear_show_the_charts_best_first() {
    let all = |_: Line| true;
    for (line, lamp) in [(Line::NewRecord, "Clear"), (Line::FirstClear, "Failed")] {
        let scene = results(line, "itg", &all).unwrap();
        let before = scene.before.as_ref().unwrap();
        assert!(before.choose);
        assert_eq!(before.mark, Mark::ChartBest);
        let song = before
            .data
            .songs
            .iter()
            .find(|s| s.id == before.song)
            .unwrap();
        let best = song.charts[before.chart].best.as_ref().unwrap();
        assert_eq!(best.lamp, lamp);
        let was = best.score.trim_end_matches('%').parse::<f64>().unwrap();
        let now = scene
            .data
            .score
            .trim_end_matches('%')
            .parse::<f64>()
            .unwrap();
        assert!(was < now, "{was} {now}");
        assert!(scene.prior.best < score_value(&scene.results.score));
        assert!(marked(select_of(before), before.mark).is_some());
    }
}

#[test]
fn the_cheer_follows_a_grade_the_draft_has() {
    let has = |l: Line| l == Line::Grade(Grade::Aaa) || l == Line::Cheer;
    let scene = results(Line::Cheer, "itg", &has).unwrap();
    let said = said(&scene, &[Line::Grade(Grade::Aaa), Line::Cheer]);
    assert_eq!(said[0].0, "evaluation aaa");
    assert_eq!(said[1].0, "evaluation cheer");
    assert!((said[1].1 - said[0].1 - CHEER_DELAY).abs() < 1e-9);
}

#[test]
fn the_cheer_follows_a_record_where_no_grade_says_it() {
    let has = |l: Line| l == Line::NewRecord || l == Line::Cheer;
    let scene = results(Line::Cheer, "itg", &has).unwrap();
    let record = said(&scene, &[Line::NewRecord, Line::Cheer]);
    assert_eq!(record[0].0, "evaluation new record");
    assert_eq!(record[1].0, "evaluation cheer");
    let has = |l: Line| l == Line::FirstClear || l == Line::Cheer;
    let scene = results(Line::Cheer, "itg", &has).unwrap();
    let clear = said(&scene, &[Line::FirstClear, Line::Cheer]);
    assert_eq!(clear[0].0, "evaluation first clear");
    assert_eq!(clear[1].0, "evaluation cheer");
}

/// The plays are the engine's: their counts, lamps and grades are what
/// it judged.
#[test]
fn the_staged_plays_are_judged_by_the_engine() {
    let all = |_: Line| true;
    let taps = |s: &ResultsScene, j: Judgement| s.results.tally.taps[j.index()];
    let fc1 = results(Line::EvaluationFullCombo(1), "itg", &all).unwrap();
    assert_eq!(fc1.results.full_combo, FullCombo::MarvelousFC);
    assert_eq!(taps(&fc1, Judgement::W2), 0);
    let fc2 = results(Line::EvaluationFullCombo(2), "itg", &all).unwrap();
    assert_eq!(fc2.results.full_combo, FullCombo::PerfectFC);
    assert!(taps(&fc2, Judgement::W2) > 0);
    // Under ITG the combo ends at a Decent: the plain full combo is DDR A's.
    let fc4 = results(Line::EvaluationFullCombo(4), "itg", &all).unwrap();
    assert_eq!(fc4.results.full_combo, FullCombo::FC);
    assert!(fc4.combo_w4);
    assert!(fc4.data.line.contains(&presets::ddr_a().name));
    let quit = results(Line::Quit, "itg", &all).unwrap();
    assert!(quit.results.quit);
    assert_eq!(quit.results.grade, "QUIT");
    let failed = results(Line::Grade(Grade::E), "itg", &all).unwrap();
    assert!(failed.results.failed);
    // A grade's play misses one step at least: its line is not a full
    // combo's.
    let b = results(Line::Grade(Grade::B), "itg", &all).unwrap();
    assert!(taps(&b, Judgement::Miss) > 0);
    assert!(!b.results.failed);
    assert_eq!(
        old_grade(&b.grades, &b.results.grade, false),
        Some(Grade::B)
    );
    assert_eq!(b.data.grade, b.results.grade);
    // The same each time.
    let again = results(Line::Grade(Grade::B), "itg", &all).unwrap();
    assert_eq!(again.results.score, b.results.score);
}

/// The player's own rules where they give the line.
#[test]
fn the_players_ruleset_judges_where_it_can() {
    let all = |_: Line| true;
    let sm5 = results(Line::Grade(Grade::A), "sm5", &all).unwrap();
    assert!(sm5.data.line.contains(&presets::sm5().name));
    let ddr = results(Line::NewRecord, "ddr-a", &all).unwrap();
    assert!(ddr.data.line.contains(&presets::ddr_a().name));
}
