//! A play that steps down to easier charts at zero life and up to harder
//! ones after full bars (`ddi_engine::ladder`, a match's "Charts:
//! Adaptive"): the same at any frame rate and from its log, nothing
//! judged in a breather nor twice at a boundary, the life as the rule
//! says, and a play without a ladder exactly as before.

use ddi_chart::Song;
use ddi_engine::judge::{JudgeEventKind, NoteState};
use ddi_engine::ladder::{self, ADAPTIVE_V0, AdaptiveRule};
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
    let rungs = ladder::below(&song, stage);
    let difficulties: Vec<_> = rungs.iter().map(|&i| song.charts[i].difficulty).collect();
    use ddi_chart::Difficulty::*;
    assert_eq!(difficulties, vec![Medium, Easy, Beginner]);
    for &r in &rungs {
        assert_eq!(song.charts[r].layout, "dance-single");
        assert!(song.charts[r].derivation.is_none());
    }
    // The easiest chart has none below it.
    assert!(ladder::below(&song, *rungs.last().unwrap()).is_empty());
}

#[test]
fn a_play_steps_down_twice_the_same_at_any_frame_rate_and_from_its_log() {
    let song = song();
    let stage = hard(&song);
    let rungs = ladder::below(&song, stage);
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
        assert!(s.boundary >= s.at + ADAPTIVE_V0.lead, "{s:?}");
        let beat = a.timing().beat_at(s.boundary);
        assert!((beat / 4.0 - (beat / 4.0).round()).abs() < 1e-6, "{beat}");
    }
}

#[test]
fn nothing_is_judged_in_a_breather_and_life_is_given_back() {
    let song = song();
    let stage = hard(&song);
    let rungs = ladder::below(&song, stage);
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
        assert!(r.life.iter().any(|l| (l[1] - ADAPTIVE_V0.life).abs() < 1e-6), "{at:?}");
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
    let rungs = ladder::below(&song, stage);
    let p = play(&song, stage, &rungs[..1], &[], 1.0 / 60.0);
    let r = p.results();
    let s = r.steps[0];
    for l in r.life.iter().filter(|l| f64::from(l[0]) >= s.at && f64::from(l[0]) < s.boundary) {
        assert!(l[1] >= ADAPTIVE_V0.floor - 1e-6, "{l:?}");
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

/// The song's single chart of difficulty `d`.
fn single(song: &Song, d: ddi_chart::Difficulty) -> usize {
    song.charts
        .iter()
        .position(|c| c.layout == "dance-single" && c.difficulty == d && c.derivation.is_none())
        .unwrap()
}

/// A player who plays whatever chart it is on perfectly (on time, every
/// note), except that it presses nothing between the seconds of each of
/// `gaps`; in frames of `step` seconds, each frame's presses read from
/// the notes it has then.
fn play_on(
    song: &Song,
    chart: usize,
    below: &[usize],
    above: &[usize],
    gaps: &[(f64, f64)],
    step: f64,
) -> Player {
    let mut p = Player::new(song, chart, rules(), PlayOptions::default());
    for &r in below {
        p.add_rung(song, r).unwrap();
    }
    for &r in above {
        p.add_rung_up(song, r).unwrap();
    }
    p.start(0.0, 0.0);
    let mut fed = f64::NEG_INFINITY;
    let mut releases: Vec<(f64, u8)> = Vec::new();
    let mut t = -1.0;
    while !p.finished() && t < 600.0 {
        t += step;
        let mut due: Vec<(f64, u8, bool)> = Vec::new();
        let notes: Vec<_> = p.judge().notes().to_vec();
        for press in ddi_engine::autoplay::script(&notes, 0.0) {
            if press.press > fed && press.press <= t {
                if gaps.iter().any(|g| press.press >= g.0 && press.press < g.1) {
                    continue;
                }
                due.push((press.press, press.lane, true));
                releases.push((press.release, press.lane));
            }
        }
        fed = fed.max(t);
        releases.retain(|&(at, lane)| {
            if at <= t {
                due.push((at, lane, false));
                false
            } else {
                true
            }
        });
        due.sort_by(|a, b| a.0.total_cmp(&b.0).then(a.2.cmp(&b.2)));
        for (at, lane, pressed) in due {
            p.input_at(lane, pressed, at);
        }
        p.update_at(t);
    }
    p
}

/// Judged again from its log alone, on the ladder `below` and `above`.
fn replayed_on(song: &Song, chart: usize, below: &[usize], above: &[usize], log: &ddi_engine::history::PlayLog) -> Player {
    let mut p = Player::new(song, chart, rules(), PlayOptions::default());
    for &r in below {
        p.add_rung(song, r).unwrap();
    }
    for &r in above {
        p.add_rung_up(song, r).unwrap();
    }
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

/// Played at 60 and 144 frames a second and judged again from the log:
/// the same each way. Every note judged once: no note both judged and
/// taken away, and no lane judged twice at a tick.
fn alike(song: &Song, chart: usize, below: &[usize], above: &[usize], gaps: &[(f64, f64)]) -> Player {
    let a = play_on(song, chart, below, above, gaps, 1.0 / 60.0);
    let b = play_on(song, chart, below, above, gaps, 1.0 / 144.0);
    let c = replayed_on(song, chart, below, above, a.log());
    let (ra, rb, rc) = (a.results(), b.results(), c.results());
    same(&ra, &rb);
    same(&ra, &rc);
    let mut seen = std::collections::HashSet::new();
    for n in a.judge().notes() {
        if n.state != NoteState::Removed && n.judgeable {
            assert!(seen.insert((n.note.tick, n.note.lane)), "judged twice: {:?}", n.note);
        }
    }
    a
}

#[test]
fn the_ladder_above_is_the_harder_charts_the_next_first() {
    use ddi_chart::Difficulty::*;
    let song = song();
    let medium = single(&song, Medium);
    let up: Vec<_> = ladder::above(&song, medium).iter().map(|&i| song.charts[i].difficulty).collect();
    assert_eq!(up[0], Hard);
    assert!(ladder::above(&song, single(&song, Beginner)).len() >= 3);
    let l = ladder::Ladder::of(&song, medium);
    assert_eq!(l.chart(0), Some(medium));
    assert_eq!(l.chart(1), Some(l.below[0]));
    assert_eq!(l.chart(l.below.len() as u8 + 1), Some(l.above[0]));
    assert_eq!(l.level(l.below.len() as u8 + 1), 1);
    assert_eq!(l.level(1), -1);
}

#[test]
fn a_perfect_play_steps_up_once_after_four_bars_the_same_at_any_frame_rate() {
    use ddi_chart::Difficulty::*;
    let song = song();
    let medium = single(&song, Medium);
    let hard = single(&song, Hard);
    let p = alike(&song, medium, &[], &[hard], &[]);
    let r = p.results();
    assert_eq!(r.steps.len(), 1, "{:?}", r.steps);
    let s = r.steps[0];
    assert!(s.up);
    assert_eq!((s.rung, s.from, s.chart), (1, 0, hard));
    // Decided on a bar line, at least four bars of full life in; the
    // boundary a bar line past the horizon and a second.
    let beat = p.timing().beat_at(s.at);
    assert!((beat / 4.0 - (beat / 4.0).round()).abs() < 1e-6, "{beat}");
    assert!(beat >= 16.0);
    assert!(s.boundary >= s.at + ADAPTIVE_V0.lead);
    // Life untouched by the step, never out.
    assert!(!r.failed);
    // The old chart played up to the boundary, the new one from it.
    let notes = p.judge().notes();
    for n in notes.iter().filter(|n| n.state != NoteState::Removed && n.judgeable) {
        assert!(!n.state.is_terminal() || n.state != NoteState::Pending);
    }
    let hard_notes = song.charts[hard].notes.iter().filter(|n| n.tick >= s.tick).count();
    assert!(hard_notes > 0);
    assert!(notes.iter().any(|n| n.state == NoteState::Removed && n.note.tick >= s.tick));
    // On the top rung nothing more happens.
}

#[test]
fn a_play_steps_up_then_down_and_back_up_takes_eight_bars() {
    use ddi_chart::Difficulty::*;
    let song = song();
    let medium = single(&song, Medium);
    let hard = single(&song, Hard);
    let easy = single(&song, Easy);
    // Perfect, then nothing pressed for a while (life runs out on Hard),
    // then perfect again.
    let first = alike(&song, medium, &[easy], &[hard], &[]);
    let up = first.results().steps[0];
    let long = play_on(&song, medium, &[easy], &[hard], &[(up.boundary + 2.0, 600.0)], 1.0 / 60.0);
    let zero = long.results().steps[1].at;
    let p = alike(&song, medium, &[easy], &[hard], &[(up.boundary + 2.0, zero + 0.01)]);
    let r = p.results();
    assert!(r.steps.len() >= 3, "{:?}", r.steps);
    assert!(r.steps[0].up);
    let down = r.steps[1];
    assert!(!down.up);
    assert_eq!(down.from, 2, "down from Hard, rung 2 (one rung below, then one above)");
    assert_eq!(down.rung, 0, "to the stage's chart");
    // Back up to Hard, where life ran out: eight bars of full life.
    let back = r.steps[2];
    assert!(back.up && back.rung == 2, "{back:?}");
    let beats = p.timing().beat_at(back.at) - p.timing().beat_at(down.boundary);
    assert!(beats >= 32.0 - 1e-6, "{beats}");
}

#[test]
fn a_play_steps_down_then_up_past_the_picked_chart() {
    use ddi_chart::Difficulty::*;
    let song = song();
    let medium = single(&song, Medium);
    let hard = single(&song, Hard);
    let easy = single(&song, Easy);
    // Nothing pressed for the first stretch: down to Easy; then perfect.
    let p = alike(&song, medium, &[easy], &[hard], &[(0.0, 14.0)]);
    let r = p.results();
    let ups: Vec<_> = r.steps.iter().filter(|s| s.up).collect();
    assert!(!r.steps[0].up, "{:?}", r.steps);
    assert_eq!(r.steps[0].rung, 1);
    assert!(ups.len() >= 2, "back to Medium and on to Hard: {:?}", r.steps);
    assert_eq!(ups[0].rung, 0);
    assert_eq!(ups[1].rung, 2);
    assert_eq!(ups[1].chart, hard);
    // Back to Medium, where life ran out: eight bars from the boundary.
    let beats = p.timing().beat_at(ups[0].at) - p.timing().beat_at(r.steps[0].boundary);
    assert!(beats >= 32.0 - 1e-6, "{beats}");
    // On to Hard: four bars from that boundary.
    let beats = p.timing().beat_at(ups[1].at) - p.timing().beat_at(ups[0].boundary);
    assert!((16.0 - 1e-6..32.0).contains(&beats), "{beats}");
}

#[test]
fn a_rule_without_steps_up_is_the_step_down_alone() {
    use ddi_chart::Difficulty::*;
    let song = song();
    let medium = single(&song, Medium);
    let hard = single(&song, Hard);
    let mut p = Player::new(&song, medium, rules(), PlayOptions::default());
    p.add_rung_up(&song, hard).unwrap();
    p.set_ladder_rule(AdaptiveRule {
        up_bars: 0,
        ..ADAPTIVE_V0
    });
    p.start(0.0, 0.0);
    let script = ddi_engine::autoplay::script(p.judge().notes(), 0.0);
    let mut edges: Vec<(f64, u8, bool)> = script
        .iter()
        .flat_map(|s| [(s.press, s.lane, true), (s.release, s.lane, false)])
        .collect();
    edges.sort_by(|a, b| a.0.total_cmp(&b.0));
    for (at, lane, pressed) in edges {
        p.input_at(lane, pressed, at);
    }
    let mut t = 0.0;
    while !p.finished() && t < 600.0 {
        t += 1.0;
        p.update_at(t);
    }
    assert!(p.results().steps.is_empty());
}
