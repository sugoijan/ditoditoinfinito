//! A battle's shock under DDI is a mine (`docs/plans/ddi-ruleset.md`,
//! "Mines and shocks": one object, two looks; round 3): stepped on, it is
//! judged, counted and scored exactly as a chart's own mine (the tally,
//! the points, the life by the difficulty, the combo, the grade's floors,
//! the lamp) and locks its lane (presses ignored, a held key released,
//! passing notes missed, passing mines not stepped on, a new press needed
//! after the lock).
//!
//! The proof is a twin: the same chart with its mines written in it, and
//! without them but with the same mines put in as battle shocks, played
//! by the same presses, come to the same results, however time is cut
//! into updates and again from the play's log.
//!
//! Chart (60 BPM, so beat `b` is at `b` seconds), dance-single:
//!
//! | beat | lane | what |
//! | --- | --- | --- |
//! | 0 | 0 | tap |
//! | 2 | 3 | tap |
//! | 3–7 | 2 | hold |
//! | 4 | 1 | mine A: pressed at 4.03, lane 1 locked to 5.03 |
//! | 4.5 | 1 | tap: pressed inside the lock, ignored, missed |
//! | 4.75 | 1 | mine B: passes inside the lock, the key down: not stepped on |
//! | 5 | 2 | mine C: crossed while the hold's key is down, lane 2 locked to 6 |
//! | 5.25 | 1 | mine D: the lock over, the key still down: needs a new press |
//! | 6 | 1 | tap: a new press |
//! | 6.5 | 3 | mine E: nobody near |
//! | 8 | 0 | tap |

use ddi_chart::{Chart, Difficulty as Level, Note, NoteKind, Song, TICKS_PER_BEAT, Tick, TimingMap};
use ddi_engine::JudgeEventKind;
use ddi_engine::battle::{HAZARD_INDEX_BASE, Hazard};
use ddi_engine::frame::SkinEventKind;
use ddi_engine::history::{PlayLog, replay_with};
use ddi_engine::player::{BattleMines, PlayOptions, Player, Results};
use ddi_engine::rules::ddi::{self, DdiOptions, Difficulty, Lamp};
use ddi_engine::rules::{Judgement, Ruleset};

/// A tick at beat `b` (quarters of a beat exact).
fn at(beat: f64) -> Tick {
    Tick((beat * TICKS_PER_BEAT as f64).round() as i64)
}

/// The mines: as the chart's own, or as shocks.
const MINES: [(f64, u8); 5] = [(4.0, 1), (4.75, 1), (5.0, 2), (5.25, 1), (6.5, 3)];

fn song(mines: &[(f64, u8)], extra: &[Note]) -> Song {
    let mut notes = vec![
        Note::new(at(0.0), 0, NoteKind::Tap),
        Note::new(at(2.0), 3, NoteKind::Tap),
        Note::new(at(3.0), 2, NoteKind::HoldHead { end: at(7.0) }),
        Note::new(at(4.5), 1, NoteKind::Tap),
        Note::new(at(6.0), 1, NoteKind::Tap),
        Note::new(at(8.0), 0, NoteKind::Tap),
    ];
    notes.extend_from_slice(extra);
    notes.extend(
        mines
            .iter()
            .map(|&(b, lane)| Note::new(at(b), lane, NoteKind::Mine)),
    );
    notes.sort_by_key(|n| (n.tick, n.lane));
    Song::generated(
        "Shocks",
        TimingMap::constant(60.0, 0.0),
        Chart::plain("dance-single", Level::Challenge, 9, notes),
        "test",
    )
}

fn shocks(mines: &[(f64, u8)]) -> Vec<Hazard> {
    mines
        .iter()
        .map(|&(b, lane)| Hazard { tick: at(b), lane })
        .collect()
}

fn rules(difficulty: Difficulty, boos: bool) -> Ruleset {
    ddi::ruleset(&DdiOptions {
        difficulty,
        boos,
        practice: false,
    })
}

/// The table's presses.
fn script() -> Vec<(f64, u8, bool)> {
    vec![
        (0.0, 0, true),
        (0.05, 0, false),
        (2.0, 3, true),
        (2.05, 3, false),
        // The hold, its key down to past its end.
        (3.0, 2, true),
        // Mine A, 30 ms late; released; then the tap inside the lock,
        // held on across mines B and D.
        (4.03, 1, true),
        (4.1, 1, false),
        (4.5, 1, true),
        (5.6, 1, false),
        // The tap after the lock: a new press.
        (6.0, 1, true),
        (6.05, 1, false),
        (7.2, 2, false),
        (8.0, 0, true),
        (8.05, 0, false),
    ]
}

/// `script` on the chart under `rules`, its mines the chart's own
/// (`in_chart`) or put in as shocks, judged in `step` second updates.
fn play_on(
    mines: &[(f64, u8)],
    extra: &[Note],
    in_chart: bool,
    rules: Ruleset,
    script: &[(f64, u8, bool)],
    step: f64,
) -> Player {
    let song = song(if in_chart { mines } else { &[] }, extra);
    let mut p = Player::new(&song, 0, rules, PlayOptions::default());
    p.start(0.0, 0.0);
    if !in_chart {
        p.add_hazards(&shocks(mines)).unwrap();
    }
    let mut next = 0;
    let mut t = -1.0;
    while !p.finished() && t < 30.0 {
        t += step;
        while next < script.len() && script[next].0 <= t {
            let (at, lane, pressed) = script[next];
            p.input_at(lane, pressed, at);
            next += 1;
        }
        p.update_at(t);
    }
    assert!(p.finished());
    p
}

fn play(in_chart: bool, rules: Ruleset, step: f64) -> Player {
    play_on(&MINES, &[], in_chart, rules, &script(), step)
}

/// What a play came to, without what tells a shock from a chart's mine
/// (the battle's own count, and the note indices nothing here keeps).
fn came_to(r: &Results) -> impl PartialEq + std::fmt::Debug {
    (
        r.tally,
        r.max_combo,
        r.score,
        r.grade.clone(),
        r.failed,
        r.ddi,
        r.life.clone(),
        r.timeline
            .iter()
            .map(|j| (j.time, j.lane, j.kind))
            .collect::<Vec<_>>(),
        (r.fast, r.slow, r.hits),
    )
}

#[test]
fn a_shock_is_judged_scored_and_locks_exactly_as_a_charts_mine() {
    for difficulty in Difficulty::ALL {
        for boos in [false, true] {
            let mine = play(true, rules(difficulty, boos), 1.0 / 60.0).results();
            let shock = play(false, rules(difficulty, boos), 1.0 / 60.0).results();
            assert_eq!(
                format!("{:?}", came_to(&mine)),
                format!("{:?}", came_to(&shock)),
                "{difficulty:?} boos {boos}"
            );
            assert_eq!(mine.battle_mines, None);
            assert_eq!(
                shock.battle_mines,
                Some(BattleMines { hit: 2, avoided: 3 })
            );
        }
    }
}

#[test]
fn a_shock_stepped_on_counts_costs_and_locks_its_lane() {
    let p = play(false, rules(Difficulty::Normal, false), 1.0 / 60.0);
    let r = p.results();
    // The tally: two stepped on (A by a press, C under the held key), in
    // the mines' count; the tap inside the lock missed; the hold dropped
    // by the lock, having earned its units to then.
    assert_eq!(r.tally.mine_hit, 2);
    assert_eq!(r.tally.count(Judgement::Miss), 1);
    assert_eq!(r.tally.let_go, 1);
    assert_eq!(r.tally.count(Judgement::W1), 5, "{:?}", r.tally);
    // The points: five taps at 50, a Miss −10, two mines −10 each, and
    // the hold's units earned before it dropped at 5.3167 s (the lock's
    // release at 5.0 and its grace): two of its four.
    assert_eq!(r.tally.hold_units, 2);
    let view = r.score.ddi.expect("DDI's view");
    assert_eq!(view.points, 5 * 50 - 10 - 2 * 10 + 2 * 50);
    assert_eq!(view.most, 6 * 50 + 4 * 50);
    // The floors: two mines and a Miss and a dropped hold are four notes
    // six tiers down each; the lamp: more than one break.
    assert_eq!(r.ddi.unwrap().lamp, Lamp::Clear);
    assert_ne!(r.grade, ddi::ABSOLUTE_DITO);
    // The combo broke at the first shock: the two taps and the hold's
    // head before it are the longest run.
    assert_eq!(r.max_combo, 3, "tap, tap and the hold's head");
    // The life, by the difficulty: −5 a shock on Normal.
    let drop_at = |r: &Results, t: f32| -> f32 {
        let before = r
            .life
            .iter()
            .take_while(|l| l[0] < t)
            .last()
            .map_or(0.5, |l| l[1]);
        let after = r
            .life
            .iter()
            .take_while(|l| l[0] <= t)
            .last()
            .map_or(before, |l| l[1]);
        before - after
    };
    assert!((drop_at(&r, 4.03) - 0.05).abs() < 1e-6, "{:?}", r.life);
    for (difficulty, cost) in [
        (Difficulty::Easy, 0.01),
        (Difficulty::Normal, 0.05),
        (Difficulty::Hard, 0.10),
    ] {
        let r = play(false, rules(difficulty, false), 1.0 / 60.0).results();
        assert!(
            (drop_at(&r, 4.03) - cost).abs() < 1e-6,
            "{difficulty:?}: {:?}",
            r.life
        );
        // The shock under the held key, at its own second.
        assert!(
            (drop_at(&r, 5.0) - cost).abs() < 1e-6,
            "{difficulty:?}: {:?}",
            r.life
        );
    }
    // The lock, as the judge has it: lane 1 from the press for a second,
    // lane 2 from the crossing.
    assert_eq!(p.judge().locked_until(1), Some(5.03));
    assert_eq!(p.judge().locked_until(2), Some(6.0));
    assert_eq!(p.judge().locked_until(0), None);
    // Which were stepped on: A and C; B (inside the lock, the key down),
    // D (the key still down after it) and E were not.
    let states: Vec<_> = p
        .hazards()
        .unwrap()
        .mines()
        .iter()
        .map(|m| m.state)
        .collect();
    use ddi_engine::battle::HazardState::{Avoided, Hit};
    assert_eq!(states, vec![Hit, Avoided, Hit, Avoided, Avoided]);
    // Its events are mines' events of their own numbering, and the
    // receptor's effect is the mine's, kept up for the lock.
    let effects = p
        .skin_events_since(0)
        .filter(|e| e.lane == 1 && e.kind == SkinEventKind::HitMine)
        .count();
    assert!(effects >= 4, "set off again through the lock: {effects}");
}

/// One shock stepped on in a play otherwise all W0: it is the play's one
/// break (the Break flag), it takes Absolute Dito away, and it costs ten
/// points and a mine's life; the same play that lets the shock pass is
/// Absolute Dito.
#[test]
fn one_shock_stepped_on_is_a_break_and_no_absolute_dito() {
    // The table's steps on time, nothing pressed where its mines were; a
    // shock far from every step, in lane 3 at beat 5.5.
    let steps: Vec<(f64, u8, bool)> = vec![
        (0.0, 0, true),
        (0.05, 0, false),
        (2.0, 3, true),
        (2.05, 3, false),
        (3.0, 2, true),
        (4.5, 1, true),
        (4.55, 1, false),
        (6.0, 1, true),
        (6.05, 1, false),
        (7.2, 2, false),
        (8.0, 0, true),
        (8.05, 0, false),
    ];
    let shock = [(5.5, 3u8)];
    let clean = play_on(
        &shock,
        &[],
        false,
        rules(Difficulty::Normal, false),
        &steps,
        1.0 / 60.0,
    )
    .results();
    assert_eq!(clean.grade, ddi::ABSOLUTE_DITO);
    assert_eq!(clean.ddi.unwrap().lamp, Lamp::AbsoluteDito);
    assert_eq!(
        clean.battle_mines,
        Some(BattleMines { hit: 0, avoided: 1 })
    );
    let mut stepped = steps.clone();
    stepped.extend([(5.52, 3, true), (5.57, 3, false)]);
    stepped.sort_by(|a, b| a.0.total_cmp(&b.0));
    let r = play_on(
        &shock,
        &[],
        false,
        rules(Difficulty::Normal, false),
        &stepped,
        1.0 / 60.0,
    )
    .results();
    assert_eq!(r.tally.mine_hit, 1);
    assert_eq!(r.ddi.unwrap().lamp, Lamp::BreakFlag, "its one break");
    assert_ne!(r.grade, ddi::ABSOLUTE_DITO);
    let (a, b) = (clean.score.ddi.unwrap(), r.score.ddi.unwrap());
    assert_eq!(a.points - b.points, 10, "a mine's points");
    assert_eq!(a.most, b.most, "the most is the chart's");
    // The same again as the chart's own mine.
    let mine = play_on(
        &shock,
        &[],
        true,
        rules(Difficulty::Normal, false),
        &stepped,
        1.0 / 60.0,
    )
    .results();
    assert_eq!(format!("{:?}", came_to(&mine)), format!("{:?}", came_to(&r)));
}

#[test]
fn a_press_steps_on_the_nearer_of_a_shock_and_a_note() {
    // A shock at beat 9.9 and a tap at beat 10 in lane 0 (closer than any
    // placement puts them: the judge's choice is what is tested).
    let extra = [Note::new(at(10.0), 0, NoteKind::Tap)];
    let near = [(9.9, 0u8)];
    // The chart's own steps on time (nothing pressed where the table's
    // mines were: there are none here).
    let base: Vec<(f64, u8, bool)> = vec![
        (0.0, 0, true),
        (0.05, 0, false),
        (2.0, 3, true),
        (2.05, 3, false),
        (3.0, 2, true),
        (4.5, 1, true),
        (4.55, 1, false),
        (6.0, 1, true),
        (6.05, 1, false),
        (7.2, 2, false),
        (8.0, 0, true),
        (8.05, 0, false),
    ];
    let mut early = base.clone();
    // 30 ms after the shock, 70 ms before the note: the shock.
    early.extend([(9.93, 0, true), (9.98, 0, false)]);
    let mut late = base;
    // 30 ms before the note, 70 ms after the shock: the note.
    late.extend([(9.97, 0, true), (10.02, 0, false)]);
    for (name, script, stepped) in [("early", early, true), ("late", late, false)] {
        let rules = || rules(Difficulty::Normal, true);
        let mine = play_on(&near, &extra, true, rules(), &script, 1.0 / 60.0).results();
        let p = play_on(&near, &extra, false, rules(), &script, 1.0 / 60.0);
        let shock = p.results();
        assert_eq!(
            format!("{:?}", came_to(&mine)),
            format!("{:?}", came_to(&shock)),
            "{name}"
        );
        assert_eq!(shock.tally.mine_hit, u32::from(stepped), "{name}");
        // Stepped on: the note passes in the locked lane, missed. Not:
        // the note is hit and the shock passes.
        assert_eq!(
            shock.tally.count(Judgement::Miss),
            u32::from(stepped),
            "{name}"
        );
        assert_eq!(shock.tally.boo, 0, "{name}: never also a Boo");
    }
}

#[test]
fn the_same_however_time_is_cut_and_again_from_the_log() {
    for boos in [false, true] {
        let rules = || rules(Difficulty::Hard, boos);
        let a = play(false, rules(), 1.0 / 60.0);
        let b = play(false, rules(), 1.0 / 144.0);
        let c = play(false, rules(), 0.7);
        let (ra, rb, rc) = (a.results(), b.results(), c.results());
        assert_eq!(
            format!("{:?}", came_to(&ra)),
            format!("{:?}", came_to(&rb)),
            "60 and 144 updates a second"
        );
        assert_eq!(
            format!("{:?}", came_to(&ra)),
            format!("{:?}", came_to(&rc)),
            "long updates"
        );
        assert_eq!(ra.battle_mines, rc.battle_mines);
        // From the log: its shocks kept, judged alike again.
        let bytes = a.log().encode();
        let read = PlayLog::decode(&bytes).unwrap();
        assert_eq!(read.hazards.len(), MINES.len());
        let plain = song(&[], &[]);
        let again = replay_with(&plain, 0, rules(), a.options(), &read);
        assert_eq!(
            format!("{:?}", came_to(&ra)),
            format!("{:?}", came_to(&again)),
            "from the log"
        );
        assert_eq!(ra.battle_mines, again.battle_mines);
    }
}

#[test]
fn a_shocks_events_are_mines_events_numbered_apart() {
    let song = song(&[], &[]);
    let mut p = Player::new(
        &song,
        0,
        rules(Difficulty::Normal, false),
        PlayOptions::default(),
    );
    p.start(0.0, 0.0);
    p.add_hazards(&shocks(&MINES)).unwrap();
    p.update_at(3.5);
    let events = p.input_at(1, true, 4.03);
    let hit: Vec<_> = events
        .iter()
        .filter(|e| e.kind == JudgeEventKind::HitMine)
        .collect();
    assert_eq!(hit.len(), 1, "{events:?}");
    assert_eq!(hit[0].note_index, Some(HAZARD_INDEX_BASE));
    assert_eq!((hit[0].lane, hit[0].song_time), (1, 4.03));
    assert!(p.judge().is_locked(1, 4.5) && !p.judge().is_locked(1, 5.03));
    // Inside the lock a press is nothing: no hit, no Boo, no shock (what
    // the press's time judges on the way is the tap that passed in the
    // locked lane, missed).
    let inside = p.input_at(1, true, 4.74);
    assert!(
        inside
            .iter()
            .all(|e| matches!(e.kind, JudgeEventKind::Tap(Judgement::Miss, _))),
        "{inside:?}"
    );
    assert!(p.input_at(1, false, 4.76).is_empty());
    assert_eq!(p.hazards().unwrap().counts().0, 1);
}
