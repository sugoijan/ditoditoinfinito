use std::cell::RefCell;
use std::rc::Rc;

use ddi_engine::rules::{FullCombo, Judgement};
use ddi_library::announcer::{Comment, Grade, Line, Spoken};
use ddi_ui::screens::select::comment;

use super::super::tests::Fake;
use super::super::{Announcer, CHEER_DELAY, Timing, Voice, cheers, results_lines as tried_lines};
use super::*;

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
        assert_eq!(scene.data.notices.len(), 1);
        assert_eq!(scene.data.notices[0].text, scene.caption);
    }
}

#[test]
fn the_intro_scene_only_shows() {
    let scene = menu(Line::SelectMusicIntro, STARTED, NOW).unwrap();
    assert!(!scene.choose);
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
            assert!(!scene.caption.is_empty());
            assert!(scene.data.note.contains(&scene.caption));
        }
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
