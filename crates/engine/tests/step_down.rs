//! A play that steps down to easier charts at zero life (`ddi_engine::ladder`,
//! a match's "At zero life: Step down"): the same at any frame rate and
//! from its log, nothing judged in a breather, the life as the rule says,
//! and a play without a ladder exactly as before.

use ddi_chart::Song;
use ddi_engine::judge::{JudgeEventKind, NoteState};
use ddi_engine::ladder::{self, STEP_DOWN_V0};
use ddi_engine::rules::ddi::{self, DdiOptions};
use ddi_engine::{FailPolicy, PlayOptions, Player, Results, Ruleset};

fn song() -> Song {
    let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../assets/songs/beatucada");
    let path = std::fs::read_dir(dir)
        .unwrap()
        .flatten()
        .map(|e| e.path())
        .find(|p| p.extension().is_some_and(|e| e == "ssc"))
        .unwrap();
    ddi_library::scan::parse_simfile(&std::fs::read_to_string(path).unwrap(), "ssc").unwrap()
}

fn rules() -> Ruleset {
    let mut r = ddi::ruleset(&DdiOptions::default());
    r.fail = FailPolicy::Immediate;
    r
}

/// The stage's chart: the song's Hard single.
fn hard(song: &Song) -> usize {
    song.charts
        .iter()
        .position(|c| {
            c.layout == "dance-single"
                && c.difficulty == ddi_chart::Difficulty::Hard
                && c.derivation.is_none()
        })
        .unwrap()
}

/// Edges a player gives whatever chart they are on: every note of every
/// chart of the ladder in lanes 0 and 1 pressed on time, nothing else,
/// so that life falls and they step down.
fn edges(song: &Song, charts: &[usize]) -> Vec<(f64, u8, bool)> {
    let mut out = Vec::new();
    for &c in charts {
        let p = Player::new(song, c, rules(), PlayOptions::default());
        for press in ddi_engine::autoplay::script(p.judge().notes(), 0.0) {
            if press.lane < 2 {
                out.push((press.press, press.lane, true));
                out.push((press.release, press.lane, false));
            }
        }
    }
    out.sort_by(|a, b| a.0.total_cmp(&b.0).then(a.1.cmp(&b.1)));
    out
}

fn player(song: &Song, chart: usize, rungs: &[usize]) -> Player {
    let mut p = Player::new(song, chart, rules(), PlayOptions::default());
    for &r in rungs {
        p.add_rung(song, r).unwrap();
    }
    p
}

/// Played in frames of `step` seconds, the edges fed at their own times
/// before each frame's update.
fn play(song: &Song, chart: usize, rungs: &[usize], edges: &[(f64, u8, bool)], step: f64) -> Player {
    let mut p = player(song, chart, rungs);
    p.start(0.0, 0.0);
    let mut next = 0;
    let mut t = -1.0;
    while !p.finished() && t < 600.0 {
        t += step;
        while let Some(&(at, lane, pressed)) = edges.get(next).filter(|e| e.0 <= t) {
            p.input_at(lane, pressed, at);
            next += 1;
        }
        p.update_at(t);
    }
    p
}

/// Judged again from its log alone.
fn replayed(song: &Song, chart: usize, rungs: &[usize], log: &ddi_engine::history::PlayLog) -> Player {
    let mut p = player(song, chart, rungs);
    p.start(0.0, log.start);
    for i in &log.inputs {
        p.input_at(i.lane, i.pressed, i.time);
    }
    let mut t = log.inputs.last().map_or(0.0, |i| i.time);
    while !p.finished() && t < 600.0 {
        t += 1.0;
        p.update_at(t);
    }
    p
}

fn same(a: &Results, b: &Results) {
    assert_eq!(a.tally, b.tally);
    assert_eq!(a.score, b.score);
    assert_eq!(a.steps, b.steps);
    assert_eq!(a.life, b.life);
    assert_eq!(a.timeline, b.timeline);
    assert_eq!(a.failed, b.failed);
    assert_eq!(a.max_combo, b.max_combo);
}

#[test]
fn the_ladder_is_the_easier_charts_of_the_style_the_next_first() {
    let song = song();
    let stage = hard(&song);
    let rungs = ladder::ladder(&song, stage);
    let difficulties: Vec<_> = rungs.iter().map(|&i| song.charts[i].difficulty).collect();
    use ddi_chart::Difficulty::*;
    assert_eq!(difficulties, vec![Medium, Easy, Beginner]);
    for &r in &rungs {
        assert_eq!(song.charts[r].layout, "dance-single");
        assert!(song.charts[r].derivation.is_none());
    }
    // The easiest chart has none below it.
    assert!(ladder::ladder(&song, *rungs.last().unwrap()).is_empty());
}

#[test]
fn a_play_steps_down_twice_the_same_at_any_frame_rate_and_from_its_log() {
    let song = song();
    let stage = hard(&song);
    let rungs = ladder::ladder(&song, stage);
    let two = &rungs[..2];
    let mut charts = vec![stage];
    charts.extend_from_slice(two);
    let edges = edges(&song, &charts);
    let a = play(&song, stage, two, &edges, 1.0 / 60.0);
    let b = play(&song, stage, two, &edges, 1.0 / 144.0);
    let c = replayed(&song, stage, two, a.log());
    let (ra, rb, rc) = (a.results(), b.results(), c.results());
    assert!(ra.steps.len() >= 2, "steps: {:?}", ra.steps);
    same(&ra, &rb);
    same(&ra, &rc);
    for (i, s) in ra.steps.iter().enumerate() {
        assert_eq!(usize::from(s.rung), i + 1);
        assert_eq!(s.chart, two[i]);
        // The boundary: a bar line at least the horizon and a second on.
        assert!(s.boundary >= s.at + STEP_DOWN_V0.lead, "{s:?}");
        let beat = a.timing().beat_at(s.boundary);
        assert!((beat / 4.0 - (beat / 4.0).round()).abs() < 1e-6, "{beat}");
    }
}

#[test]
fn nothing_is_judged_in_a_breather_and_life_is_given_back() {
    let song = song();
    let stage = hard(&song);
    let rungs = ladder::ladder(&song, stage);
    // Nothing pressed at all: every note missed, so life runs out fast.
    let p = play(&song, stage, &rungs, &[], 1.0 / 60.0);
    let r = p.results();
    assert_eq!(r.steps.len(), rungs.len(), "{:?}", r.steps);
    for s in &r.steps {
        // Nothing of a chart judged between the zero and the boundary.
        let inside: Vec<_> = r
            .timeline
            .iter()
            .filter(|j| f64::from(j.time) > s.at + 1e-6 && f64::from(j.time) < s.boundary - 0.5)
            .collect();
        assert!(inside.is_empty(), "{s:?}: {inside:?}");
        // The life given back at the zero: half.
        let at = r
            .life
            .iter()
            .find(|l| (f64::from(l[0]) - s.at).abs() < 1e-4)
            .unwrap();
        assert!(r.life.iter().any(|l| (l[1] - STEP_DOWN_V0.life).abs() < 1e-6), "{at:?}");
    }
    // On the lowest rung, out as ever.
    assert!(r.failed);
    let last = r.steps.last().unwrap();
    assert!(r.gauge_failed_at.unwrap() > last.boundary as f32);
    // The notes taken away were never judged nor drawn.
    assert!(p.judge().notes().iter().any(|n| n.state == NoteState::Removed));
    assert!(
        !r.timeline
            .iter()
            .any(|j| matches!(j.kind, JudgeEventKind::Boo))
    );
}

#[test]
fn the_breather_keeps_life_above_its_floor() {
    let song = song();
    let stage = hard(&song);
    let rungs = ladder::ladder(&song, stage);
    let p = play(&song, stage, &rungs[..1], &[], 1.0 / 60.0);
    let r = p.results();
    let s = r.steps[0];
    for l in r.life.iter().filter(|l| f64::from(l[0]) >= s.at && f64::from(l[0]) < s.boundary) {
        assert!(l[1] >= STEP_DOWN_V0.floor - 1e-6, "{l:?}");
    }
}

#[test]
fn a_play_without_a_ladder_is_as_before() {
    let song = song();
    let stage = hard(&song);
    let edges = edges(&song, &[stage]);
    let a = play(&song, stage, &[], &edges, 1.0 / 60.0);
    let r = a.results();
    assert!(r.steps.is_empty());
    assert!(r.failed, "out at zero life, as ever");
    // The same as a plain replay of its log.
    let b = ddi_engine::history::replay_with(&song, stage, rules(), &PlayOptions::default(), a.log());
    same(&r, &b);
    // And its results' JSON has no word of steps.
    assert!(!serde_json::to_string(&r).unwrap().contains("steps"));
}
