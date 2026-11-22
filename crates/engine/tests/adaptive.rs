//! A play that steps down to easier charts at zero life and up to harder
//! ones after full bars (`ddi_engine::ladder`, a match's "Charts:
//! Adaptive"): the same at any frame rate and from its log, nothing
//! judged in a breather nor twice at a boundary, the life as the rule
//! says, and a play without a ladder exactly as before.

mod common;

use ddi_chart::Song;
use ddi_engine::judge::{JudgeEventKind, NoteState};
use ddi_engine::ladder::{self, ADAPTIVE, AdaptiveRule};
use ddi_engine::rules::ddi::{self, DdiOptions};
use ddi_engine::{FailPolicy, PlayOptions, Player, Results, Ruleset};

fn song() -> Song {
    common::bundled("beatucada")
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
        // The boundary: a bar line at least the horizon on (and a
        // second, before `adaptive-v2`).
        assert!(s.boundary >= s.at + ladder::ADAPTIVE.down_lead, "{s:?}");
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
        assert!(r.life.iter().any(|l| (l[1] - ADAPTIVE.life).abs() < 1e-6), "{at:?}");
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
        assert!(l[1] >= ADAPTIVE.floor - 1e-6, "{l:?}");
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

thread_local! {
    /// The ladder rule [`play_on`] and [`replayed_on`] give their plays
    /// (none: the one a stage plays, `ladder::ADAPTIVE`).
    static RULE: std::cell::Cell<Option<AdaptiveRule>> = const { std::cell::Cell::new(None) };
    /// How late [`play_on`]'s presses are, seconds (0: on time).
    static LATE: std::cell::Cell<f64> = const { std::cell::Cell::new(0.0) };
    /// Which of [`play_on`]'s presses are late by `LATE`: every one (0),
    /// or every `EVERY`th of the chart's presses, the rest on time.
    static EVERY: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
    /// The rung [`play_on`] and [`replayed_on`] start their plays on
    /// (`Player::start_on`; 0, the stage's chart).
    static START: std::cell::Cell<u8> = const { std::cell::Cell::new(0) };
}

/// A sloppy play: every sixth press 60 ms late (DDI's W3, which keeps
/// full life), the rest on time, so about 83% of any stretch's taps are
/// in the three best judgments (81 to 88% of any four, six or eight bars
/// of the bundled song's Medium): under `adaptive-v5`'s 90% for an early
/// step up, over its 80% for a normal one.
const SLOPPY: f64 = 0.060;

/// How [`play_on`] presses from here on: on time (0), every sixth
/// press late ([`SLOPPY`]), or every press `late` seconds late.
fn lateness(late: f64) {
    LATE.set(late);
    EVERY.set(if late == SLOPPY { 6 } else { 0 });
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
    if let Some(rule) = RULE.get() {
        p.set_ladder_rule(rule);
    }
    p.start_on(START.get()).unwrap();
    p.start(0.0, 0.0);
    let mut fed = f64::NEG_INFINITY;
    let mut pending: Vec<(f64, u8, bool)> = Vec::new();
    let every = EVERY.get();
    let mut t = -1.0;
    while !p.finished() && t < 600.0 {
        t += step;
        let mut due: Vec<(f64, u8, bool)> = Vec::new();
        let notes: Vec<_> = p.judge().notes().to_vec();
        let bias = if every == 0 { LATE.get() } else { 0.0 };
        for (i, press) in ddi_engine::autoplay::script(&notes, bias).into_iter().enumerate() {
            if press.press > fed && press.press <= t {
                if gaps.iter().any(|g| press.press >= g.0 && press.press < g.1) {
                    continue;
                }
                // By its place among the chart's presses, so the same
                // presses are late at any frame rate.
                let slip = if every > 0 && (i + 1).is_multiple_of(every) { LATE.get() } else { 0.0 };
                pending.push((press.press + slip, press.lane, true));
                pending.push((press.release + slip, press.lane, false));
            }
        }
        fed = fed.max(t);
        pending.retain(|&(at, lane, pressed)| {
            if at <= t {
                due.push((at, lane, pressed));
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
    if let Some(rule) = RULE.get() {
        p.set_ladder_rule(rule);
    }
    p.start_on(START.get()).unwrap();
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
    assert!(s.up && s.early, "on time: the early step up");
    assert_eq!((s.rung, s.from, s.chart), (1, 0, hard));
    // Decided on a bar line, at least four bars of full life in; the
    // boundary a bar line past the horizon and a second.
    let beat = p.timing().beat_at(s.at);
    assert!((beat / 4.0 - (beat / 4.0).round()).abs() < 1e-6, "{beat}");
    assert!(beat >= 16.0);
    assert!(s.boundary >= s.at + ADAPTIVE.lead);
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

/// Up from Medium to Hard, life run out on Hard (down to Medium), and
/// back up to Hard: `bars` bars of full life from the step down's
/// boundary. The steps.
fn up_down_up(rule: AdaptiveRule) -> (Player, Vec<ddi_engine::ladder::Step>) {
    use ddi_chart::Difficulty::*;
    RULE.set(Some(rule));
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
    RULE.set(None);
    let steps = p.results().steps.clone();
    (p, steps)
}

#[test]
fn a_play_steps_up_then_down_and_back_up_takes_sixteen_bars() {
    let (p, steps) = up_down_up(ladder::ADAPTIVE);
    assert_eq!(ladder::ADAPTIVE, ladder::ADAPTIVE_V5);
    assert_eq!(ladder::ADAPTIVE.return_bars, 16);
    assert!(steps.len() >= 3, "{steps:?}");
    assert!(steps[0].up);
    let down = steps[1];
    assert!(!down.up && !down.closes);
    assert_eq!(down.from, 2, "down from Hard, rung 2 (one rung below, then one above)");
    assert_eq!(down.rung, 0, "to the stage's chart");
    // Back up to Hard, where life ran out: sixteen bars of full life.
    let back = steps[2];
    assert!(back.up && back.rung == 2, "{back:?}");
    let beats = p.timing().beat_at(back.at) - p.timing().beat_at(down.boundary);
    assert!(beats >= 64.0 - 1e-6, "{beats}");
}

/// The rule's bars back up are what it says (eight here) and a rule that
/// closes nothing never closes a rung.
#[test]
fn a_rule_of_eight_bars_back_up_and_no_closing_takes_eight_and_closes_nothing() {
    let (p, steps) = up_down_up(AdaptiveRule {
        return_bars: 8,
        closed_after: 0,
        ..ADAPTIVE
    });
    let (down, back) = (steps[1], steps[2]);
    assert!(back.up && back.rung == 2, "{steps:?}");
    let beats = p.timing().beat_at(back.at) - p.timing().beat_at(down.boundary);
    assert!((32.0 - 1e-6..64.0).contains(&beats), "{beats}");
    assert!(steps.iter().all(|s| !s.closes));
}

/// Life run out on Hard twice: Hard is closed for the rest of the stage,
/// however well the rest is played; the step down that closed it says
/// so. The same at 60 and 144 fps and from the log. (The rule with
/// shorter bars, two up early, three normally and four back up, so that
/// the bundled song is long enough for both run-outs and the bars
/// after.)
fn closed_after_twice(late: f64) {
    use ddi_chart::Difficulty::*;
    RULE.set(Some(AdaptiveRule {
        up_bars: 2,
        normal_bars: 3,
        return_bars: 4,
        ..ADAPTIVE
    }));
    lateness(late);
    let song = song();
    let medium = single(&song, Medium);
    let hard = single(&song, Hard);
    let easy = single(&song, Easy);
    let first = alike(&song, medium, &[easy], &[hard], &[]);
    let up = first.results().steps[0];
    let long = play_on(&song, medium, &[easy], &[hard], &[(up.boundary + 2.0, 600.0)], 1.0 / 60.0);
    let gap = (up.boundary + 2.0, long.results().steps[1].at + 0.01);
    let again = alike(&song, medium, &[easy], &[hard], &[gap]);
    let back = again.results().steps[2];
    assert!(back.up && back.rung == 2, "{:?}", again.results().steps);
    let long = play_on(
        &song,
        medium,
        &[easy],
        &[hard],
        &[gap, (back.boundary + 2.0, 600.0)],
        1.0 / 60.0,
    );
    let second = long.results().steps[3];
    assert!(!second.up && second.from == 2, "{second:?}");
    let p = alike(
        &song,
        medium,
        &[easy],
        &[hard],
        &[gap, (back.boundary + 2.0, second.at + 0.01)],
    );
    let steps = p.results().steps.clone();
    assert!(steps.len() >= 4, "{steps:?}");
    let shut = steps[3];
    assert!(!shut.up && shut.from == 2 && shut.closes, "{shut:?}");
    assert!(!steps[1].closes);
    // Played perfectly from there to the end: never up to Hard again.
    assert!(steps[4..].iter().all(|s| !(s.up && s.rung == 2)), "{steps:?}");
    let left = p.timing().beat_at(p.end_time()) - p.timing().beat_at(shut.boundary);
    assert!(left > 32.0, "the song went on long enough to have climbed back: {left}");
    // The first step up: early on time, normal late; a step back up is
    // never early (its own sixteen bars).
    assert_eq!(steps[0].early, late == 0.0, "{steps:?}");
    assert!(steps[1..].iter().all(|s| !s.early), "{steps:?}");
    RULE.set(None);
    lateness(0.0);
}

#[test]
fn a_rung_whose_life_ran_out_twice_is_closed() {
    closed_after_twice(0.0);
}

/// The same by normal steps up (a sloppy play, [`SLOPPY`]: never nine in
/// ten in the three best): closed after life ran out on it twice all the
/// same.
#[test]
fn a_rung_whose_life_ran_out_twice_is_closed_to_normal_steps_up_too() {
    closed_after_twice(SLOPPY);
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
    // Back to Medium, where life ran out: sixteen bars from the boundary.
    let beats = p.timing().beat_at(ups[0].at) - p.timing().beat_at(r.steps[0].boundary);
    assert!(beats >= 64.0 - 1e-6, "{beats}");
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
        ..ADAPTIVE
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

/// The early step up (`adaptive-v4`, as `adaptive-v5`'s): four bars of full life with no
/// miss and nine in ten of their taps in the three best judgments. A play
/// 25 ms late on every note (DDI's W2, which `adaptive-v3`'s two best
/// judgments never let up early) steps up early; a sloppy one
/// ([`SLOPPY`]: about 83% in the three best, full life kept) never does,
/// while the same rule asking no accuracy steps it up; on time it steps
/// up. The same at 60 and 144 fps and from the log.
#[test]
fn an_early_step_up_needs_nine_in_ten_in_the_three_best_judgments() {
    use ddi_chart::Difficulty::*;
    let song = song();
    let medium = single(&song, Medium);
    let hard = single(&song, Hard);
    // `adaptive-v5` without its normal step up.
    let early_only = AdaptiveRule {
        normal_bars: 0,
        ..ADAPTIVE
    };
    let any = AdaptiveRule {
        accuracy: 0.0,
        ..early_only
    };
    let (sloppy_any, _, failed) = steps_of(&song, medium, hard, any, SLOPPY, &[]);
    assert!(!failed, "sloppy: full life");
    assert_eq!(sloppy_any.len(), 1, "steps up where no accuracy is asked: {sloppy_any:?}");
    let (sloppy, _, _) = steps_of(&song, medium, hard, early_only, SLOPPY, &[]);
    assert!(sloppy.is_empty(), "never early: {sloppy:?}");
    let (late, w2, failed) = steps_of(&song, medium, hard, early_only, 0.025, &[]);
    assert!(w2 > 20 && !failed, "late: W2s ({w2}), full life");
    assert_eq!(late.len(), 1, "{late:?}");
    assert!(late[0].up && late[0].early, "W2s are among the three best: {late:?}");
    // `adaptive-v3`'s early step up (95% in the two best) never took it.
    let v3 = AdaptiveRule {
        accuracy: 0.95,
        early_best: 2,
        ..early_only
    };
    let (late_v3, _, _) = steps_of(&song, medium, hard, v3, 0.025, &[]);
    assert!(late_v3.is_empty(), "{late_v3:?}");
    let (on_time, _, _) = steps_of(&song, medium, hard, early_only, 0.0, &[]);
    assert_eq!(on_time.len(), 1, "{on_time:?}");
    assert!(on_time[0].up && on_time[0].early);
}

/// The steps of a play `late` seconds late on every note of whatever
/// chart it is on (but in `gaps`), Medium with Hard above, under `rule`;
/// its W2s and whether it failed. Played at 60 and 144 fps and from its
/// log alike.
fn steps_of(
    song: &Song,
    medium: usize,
    hard: usize,
    rule: AdaptiveRule,
    late: f64,
    gaps: &[(f64, f64)],
) -> (Vec<ddi_engine::ladder::Step>, u32, bool) {
    RULE.set(Some(rule));
    lateness(late);
    let p = alike(song, medium, &[], &[hard], gaps);
    RULE.set(None);
    lateness(0.0);
    let r = p.results();
    (r.steps.clone(), r.tally.count(ddi_engine::Judgement::W2), r.failed)
}

/// The normal step up (`adaptive-v5`: six bars, where `adaptive-v4` and
/// `adaptive-v3` asked eight): the sloppy play, never early, steps up
/// after the rule's longer stretch of full life, four in five of its taps
/// in the three best judgments, at the first bar line past its six bars
/// and a second; not where the rule asks nine in ten of them; not before
/// its bars are all clean; eight bars (`adaptive-v4`'s) later.
#[test]
fn a_normal_step_up_takes_its_longer_bars_and_its_floor() {
    use ddi_chart::Difficulty::*;
    let song = song();
    let medium = single(&song, Medium);
    let hard = single(&song, Hard);
    let (late, _, _) = steps_of(&song, medium, hard, ADAPTIVE, SLOPPY, &[]);
    assert_eq!(late.len(), 1, "{late:?}");
    let s = late[0];
    assert!(s.up && !s.early, "{s:?}");
    let p = Player::new(&song, medium, rules(), PlayOptions::default());
    let beat = p.timing().beat_at(s.at);
    assert_eq!(ADAPTIVE.normal_bars, 6);
    let bars = f64::from(ADAPTIVE.normal_bars) * 4.0;
    assert!(beat >= bars - 1e-6, "{beat}");
    // `adaptive-v4`'s eight bars: two bars later.
    let v4 = AdaptiveRule {
        normal_bars: 8,
        ..ADAPTIVE
    };
    let (eight, _, _) = steps_of(&song, medium, hard, v4, SLOPPY, &[]);
    assert!(
        eight.first().is_some_and(|e| p.timing().beat_at(e.at) >= beat + 8.0 - 1e-6),
        "{eight:?} against {beat}"
    );
    assert!((beat / 4.0 - (beat / 4.0).round()).abs() < 1e-6, "{beat}");
    // Its floor: the same play asked for nine in ten never steps up.
    let strict = AdaptiveRule {
        normal_accuracy: 0.9,
        ..ADAPTIVE
    };
    let (none, _, _) = steps_of(&song, medium, hard, strict, SLOPPY, &[]);
    assert!(none.is_empty(), "{none:?}");
    // Its stretch: a rule of twice the bars steps up no sooner than them.
    let longer = AdaptiveRule {
        normal_bars: ADAPTIVE.normal_bars * 2,
        ..ADAPTIVE
    };
    let (later, _, _) = steps_of(&song, medium, hard, longer, SLOPPY, &[]);
    assert!(
        later.first().is_some_and(|l| p.timing().beat_at(l.at) >= 2.0 * bars - 1e-6),
        "{later:?}"
    );
}

/// No step up across a miss, early or normal: a stretch with nothing
/// pressed (misses, life lost) and the step up comes the rule's bars
/// after life is full again, never sooner.
#[test]
fn no_step_up_across_a_miss() {
    use ddi_chart::Difficulty::*;
    let song = song();
    let medium = single(&song, Medium);
    let hard = single(&song, Hard);
    let p = Player::new(&song, medium, rules(), PlayOptions::default());
    // Misses from a few bars in, before either rule's bars are full.
    let gap = (p.timing().seconds_at_beat(12.0), p.timing().seconds_at_beat(13.0));
    for (late, bars) in [(0.0, ADAPTIVE.up_bars), (SLOPPY, ADAPTIVE.normal_bars)] {
        let (steps, _, failed) = steps_of(&song, medium, hard, ADAPTIVE, late, &[gap]);
        assert!(!failed);
        let s = steps.first().copied().unwrap_or_else(|| panic!("{late}: no step up"));
        assert!(s.up && s.early == (late == 0.0), "{s:?}");
        let since = p.timing().beat_at(s.at) - 13.0;
        assert!(since >= f64::from(bars) * 4.0, "{late}: {since} beats after the misses");
    }
}

/// The return rule against both kinds: back up to a rung whose life ran
/// out takes sixteen bars, by the normal step up's accuracy too (a
/// sloppy play).
#[test]
fn back_up_by_normal_steps_takes_sixteen_bars_too() {
    lateness(SLOPPY);
    let (p, steps) = up_down_up(ADAPTIVE);
    lateness(0.0);
    assert!(steps.len() >= 3, "{steps:?}");
    assert!(steps[0].up && !steps[0].early);
    let (down, back) = (steps[1], steps[2]);
    assert!(!down.up && back.up && back.rung == 2 && !back.early, "{steps:?}");
    let beats = p.timing().beat_at(back.at) - p.timing().beat_at(down.boundary);
    assert!(beats >= 64.0 - 1e-6, "{beats}");
}

/// The life a step down gives back is the life read just after its moment
/// (`Player::life_at`, which a stage's stream and its CPUs' fields take
/// the step's life from), wherever in the song it comes: the record's
/// times are not rounded to an `f32`'s spacing, microseconds apart past
/// 16 s (review finding engine-1). Plays of every chart with a ladder,
/// pressing one lane, two or three, each pressed a little late or not.
#[test]
fn the_life_read_just_after_a_step_down_is_the_life_given_back() {
    let song = song();
    let mut steps = 0;
    let mut rounded_away = 0;
    for stage in 0..song.charts.len() {
        if song.charts[stage].layout != "dance-single" || song.charts[stage].derivation.is_some() {
            continue;
        }
        let rungs = ladder::below(&song, stage);
        if rungs.is_empty() {
            continue;
        }
        let mut charts = vec![stage];
        charts.extend_from_slice(&rungs);
        for lanes in 1..=3u8 {
            for late in [0.0, 0.0123, 0.0371] {
                let mut edges: Vec<(f64, u8, bool)> = Vec::new();
                for &c in &charts {
                    let p = Player::new(&song, c, rules(), PlayOptions::default());
                    for press in ddi_engine::autoplay::script(p.judge().notes(), 0.0) {
                        if press.lane < lanes {
                            edges.push((press.press + late, press.lane, true));
                            edges.push((press.release + late, press.lane, false));
                        }
                    }
                }
                edges.sort_by(|a, b| a.0.total_cmp(&b.0).then(a.1.cmp(&b.1)));
                let p = play(&song, stage, &rungs, &edges, 1.0 / 60.0);
                for s in p.steps().iter().filter(|s| !s.up && s.at > 16.0) {
                    steps += 1;
                    if ((s.at + 1e-6) as f32) <= s.at as f32 {
                        rounded_away += 1;
                    }
                    assert_eq!(p.life_at(s.at + 1e-6), ADAPTIVE.life, "{s:?}");
                }
            }
        }
    }
    assert!(rounded_away > 0, "a step the old record rounded away: {steps}");
}

/// A song made for a step down beside a battle's shocks: 120 BPM, a
/// Challenge of three-lane jumps (lanes 0, 1 and 3) every eighth, which a
/// player who presses lane 2 alone misses, each followed two ticks later
/// by a tap on lane 2; a Medium below it the same.
fn shock_song() -> Song {
    let chart = |name: &str, meter: u32| {
        let body = (0..24)
            .map(|_| {
                (0..192)
                    .map(|r| match r % 24 {
                        0 => "1101",
                        2 => "0010",
                        _ => "0000",
                    })
                    .collect::<Vec<_>>()
                    .join("\n")
            })
            .collect::<Vec<_>>()
            .join("\n,\n");
        format!("#NOTES:\ndance-single:\n:\n{name}:\n{meter}:\n0,0,0,0,0:\n{body}\n;\n")
    };
    let text = format!(
        "#TITLE:t;\n#OFFSET:0;\n#BPMS:0=120;\n{}{}",
        chart("Challenge", 12),
        chart("Medium", 4)
    );
    ddi_chart::formats::sm::parse_sm(&text).expect("parses")
}

/// A press in the same update as the miss that empties life (no frame
/// judged between them) is judged as one a frame after it: against the
/// chart the step down left, so whether a battle's shock beside an old
/// note is stepped on is the same at any frame rate (review finding
/// engine-3). Lane 2's taps pressed on time, everything else missed, a
/// shock on lane 2 on every jump; one more press on lane 2 a little after
/// the step down's moment, judged in frames of 1 ms and of 60 per second
/// at every phase.
#[test]
fn a_press_just_after_life_runs_out_is_judged_the_same_at_any_frame_rate() {
    let song = shock_song();
    let stage = song
        .charts
        .iter()
        .position(|c| c.difficulty == ddi_chart::Difficulty::Challenge)
        .unwrap();
    let rungs = ladder::below(&song, stage);
    assert_eq!(rungs.len(), 1);
    // A shock on lane 2 a tick (10.4 ms) after each of its taps.
    let shocks: Vec<ddi_engine::battle::Hazard> = (1..24 * 8)
        .map(|k| ddi_engine::battle::Hazard {
            tick: ddi_chart::Tick(k * 24 + 3),
            lane: 2,
        })
        .collect();
    let base: Vec<(f64, u8, bool)> = {
        let p = Player::new(&song, stage, rules(), PlayOptions::default());
        ddi_engine::autoplay::script(p.judge().notes(), 0.0)
            .into_iter()
            .filter(|x| x.lane == 2)
            .flat_map(|x| [(x.press, 2, true), (x.press + 0.005, 2, false)])
            .collect()
    };
    let run = |edges: &[(f64, u8, bool)], step: f64, phase: f64| {
        let mut p = player(&song, stage, &rungs);
        p.add_hazards(&shocks).unwrap();
        p.start(0.0, 0.0);
        let mut next = 0;
        let mut t = -1.0 + phase;
        while !p.finished() && t < 120.0 {
            t += step;
            while let Some(&(at, lane, pressed)) = edges.get(next).filter(|e| e.0 <= t) {
                p.input_at(lane, pressed, at);
                next += 1;
            }
            p.update_at(t);
        }
        p
    };
    let first = run(&base, 0.001, 0.0);
    let m = first.steps().first().expect("a step down").at;
    let mut differ = Vec::new();
    for k in 0..6 {
        let at = m + 0.032 + 0.003 * f64::from(k);
        // Nothing else pressed from the step down on.
        let mut edges: Vec<_> = base.iter().copied().filter(|e| e.0 < m).collect();
        edges.push((at, 2, true));
        edges.push((at + 0.005, 2, false));
        edges.sort_by(|a, b| a.0.total_cmp(&b.0).then(a.1.cmp(&b.1)));
        let fine = run(&edges, 0.001, 0.0);
        let rf = fine.results();
        // Judged a frame after the step: no note of the old chart left,
        // the shock beside it stepped on.
        assert_eq!(fine.hazards().map(|h| h.counts().0), Some(1), "{at}");
        for phase in 0..17 {
            let coarse = run(&edges, 1.0 / 60.0, 0.001 * f64::from(phase));
            let rc = coarse.results();
            if rc.tally != rf.tally
                || rc.steps != rf.steps
                || coarse.hazards().map(|h| h.counts()) != fine.hazards().map(|h| h.counts())
            {
                differ.push((at - m, phase, rc.tally, rf.tally));
            }
        }
    }
    assert!(differ.is_empty(), "{} differ: {:?}", differ.len(), differ.first());
}

/// Round OC (room model revision 23): a play started on a rung of its own
/// (`Player::start_on`): that chart's notes, meter and reading horizon
/// from the first, the rungs numbered against the stage's chart as ever,
/// and its steps going on from there, the same at any frame rate and from
/// its log.
#[test]
fn a_play_started_on_its_own_rung_steps_on_from_there() {
    use ddi_chart::Difficulty::*;
    let song = song();
    let medium = single(&song, Medium);
    let hard = single(&song, Hard);
    let easy = single(&song, Easy);
    // Its notes from the first: Easy's, as a play of Easy has them.
    let mut p = player(&song, medium, &[easy]);
    p.add_rung_up(&song, hard).unwrap();
    p.start_on(1).unwrap();
    let alone = Player::new(&song, easy, rules(), PlayOptions::default());
    let notes = |p: &Player| p.judge().notes().iter().map(|n| (n.note.tick, n.note.lane)).collect::<Vec<_>>();
    assert_eq!(notes(&p), notes(&alone));
    assert_eq!(p.rung(), 1);
    assert_eq!(p.chart_now(), easy);
    assert_eq!(p.reading_horizon(), alone.reading_horizon());
    assert_eq!(p.start_on(7), Err("no such rung"));
    // Perfect from Easy: up to Medium (rung 0), then on to Hard (rung 2).
    START.set(1);
    let p = alike(&song, medium, &[easy], &[hard], &[]);
    START.set(0);
    let steps = p.results().steps.clone();
    assert!(steps.len() >= 2, "{steps:?}");
    assert!(steps[0].up && steps[1].up);
    assert_eq!((steps[0].from, steps[0].rung, steps[0].chart), (1, 0, medium));
    assert_eq!((steps[1].from, steps[1].rung, steps[1].chart), (0, 2, hard));
    // Started on Hard, nothing pressed for a while: down to Medium, from 2.
    START.set(2);
    let p = alike(&song, medium, &[easy], &[hard], &[(0.0, 14.0)]);
    START.set(0);
    let first = p.results().steps[0];
    assert!(!first.up, "{first:?}");
    assert_eq!((first.from, first.rung, first.chart), (2, 0, medium));
    // Set once started: refused.
    let mut started = player(&song, medium, &[easy]);
    started.start(0.0, 0.0);
    assert!(started.start_on(1).is_err());
}

/// The ladder seen from another of its rungs (`Ladder::around`): the same
/// charts, that one its own, the easier below the next first, the harder
/// above; a chart's rung number by `rung_of_chart`.
#[test]
fn a_ladder_seen_from_another_rung_has_the_same_charts() {
    use ddi_chart::Difficulty::*;
    let song = song();
    let medium = single(&song, Medium);
    let l = ladder::Ladder::of(&song, medium);
    let line = l.line();
    assert!(line.windows(2).all(|w| {
        let (a, b) = (&song.charts[w[0]], &song.charts[w[1]]);
        (a.difficulty as u8, a.meter) <= (b.difficulty as u8, b.meter)
    }), "easiest to hardest");
    for (i, &c) in line.iter().enumerate() {
        let r = l.rung_of_chart(c).unwrap();
        assert_eq!(l.chart(r), Some(c));
        let a = l.around(r).unwrap();
        assert_eq!(a.stage, c);
        assert_eq!(a.line(), line, "the same charts in the same order");
        assert_eq!(a.below.len(), i);
        assert_eq!(a.below.first().copied(), i.checked_sub(1).map(|k| line[k]));
        assert_eq!(a.above.first().copied(), line.get(i + 1).copied());
    }
    assert_eq!(l.rung_of_chart(usize::MAX), None);
    assert!(l.around(99).is_none());
}

/// `cpu-start-v0` over every bundled song, from every original single
/// chart picked: each level a rung of the ladder, never an easier chart
/// for a stronger level, levels 1 and 2 on the easiest slot there is,
/// 9 and 10 on the hardest, the same pair on one slot. The table (with
/// `--nocapture`) is the round's report's.
#[test]
fn a_cpu_starts_on_the_chart_its_level_is_at_home_on() {
    for id in ["beatucada", "into-my-dream", "some-things-must", "sweeteners"] {
        let song = common::bundled(id);
        let singles: Vec<usize> = (0..song.charts.len())
            .filter(|&c| {
                let ch = &song.charts[c];
                ch.layout == "dance-single" && ch.derivation.is_none() && !ch.notes.is_empty()
            })
            .collect();
        for &stage in &singles {
            let l = ladder::Ladder::of(&song, stage);
            let line = l.line();
            let place = |rung: u8| line.iter().position(|&c| Some(c) == l.chart(rung)).unwrap();
            let starts: Vec<u8> = (0..10).map(|level| ladder::cpu_start(&song, &l, level)).collect();
            let places: Vec<usize> = starts.iter().map(|&r| place(r)).collect();
            assert!(places.windows(2).all(|w| w[0] <= w[1]), "{id} {stage}: {places:?}");
            let slot = |p: usize| song.charts[line[p]].difficulty as u8;
            let slots: Vec<u8> = line.iter().map(|&c| song.charts[c].difficulty as u8).filter(|&s| s < 5).collect();
            if let (Some(&lo), Some(&hi)) = (slots.iter().min(), slots.iter().max()) {
                assert_eq!(slot(places[0]), lo, "{id} {stage}");
                assert_eq!(slot(places[9]), hi, "{id} {stage}");
            }
            for pair in places.chunks(2) {
                assert_eq!(slot(pair[0]), slot(pair[1]), "{id} {stage}: {places:?}");
            }
            if song.charts[stage].difficulty == ddi_chart::Difficulty::Hard {
                let named: Vec<String> = starts
                    .iter()
                    .map(|&r| {
                        let c = &song.charts[l.chart(r).unwrap()];
                        format!("{} {}", c.difficulty.name(), c.meter)
                    })
                    .collect();
                println!("{id}: {}", named.join(" | "));
            }
        }
    }
}

/// Round OC's study (with `--ignored --nocapture`): a perfect play's DDI
/// score (the sum a stage under Adaptive ranks by) of every original
/// single chart of the bundled songs, under DDI Normal: how much more a
/// harder chart can score than an easier one of the same song.
#[test]
#[ignore]
fn perfect_scores_by_chart() {
    for id in ["beatucada", "into-my-dream", "some-things-must", "sweeteners"] {
        let song = common::bundled(id);
        let mut line = Vec::new();
        for (c, chart) in song.charts.iter().enumerate() {
            if chart.layout != "dance-single" || chart.derivation.is_some() || chart.notes.is_empty() {
                continue;
            }
            let mut p = Player::new(&song, c, ddi::ruleset(&DdiOptions::default()), PlayOptions::default());
            let mut edges = ddi_engine::autoplay::edges(ddi_engine::autoplay::script(p.judge().notes(), 0.0));
            ddi_engine::autoplay::sort_edges(&mut edges);
            p.start(0.0, 0.0);
            for (t, lane, pressed) in edges {
                p.update_at(t);
                p.input_at(lane, pressed, t);
            }
            p.update_at(1e6);
            let score = p.ruleset().score.view().ddi.map_or(0, |d| d.score);
            line.push(format!("{} {}: {score}", chart.difficulty.name(), chart.meter));
        }
        println!("{id}: {}", line.join(" | "));
    }
}
