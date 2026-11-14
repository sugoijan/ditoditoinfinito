//! DDI's rules (`docs/plans/ddi-ruleset.md`): the windows on each
//! difficulty's grid, the early and late words, the combo, the Boos and
//! their exemptions, the lane lock, hold units, the life, the score's
//! views (the score shown the proportional part plus the absolute
//! score), the grades with their floors and the lamps; every play judged
//! again from its key and log to the same results, and the bundled
//! charts autoplayed to Absolute Dito.

mod common;

use common::run;

use ddi_chart::{Note, NoteKind, Song, StopSegment, Tick, TimingMap};
use ddi_engine::autoplay;
use ddi_engine::history::replay;
use ddi_engine::player::{PlayOptions, Player, Results};
use ddi_engine::rules::ddi::{self, DdiOptions, Difficulty, Lamp};
use ddi_engine::rules::presets::HoldStyle;
use ddi_engine::rules::{FailPolicy, HoldRules, Judgement, Ruleset, RulesetKey, Tally};
use ddi_engine::{ClockOptions, JudgeEventKind};

/// 60 BPM: beat `b` at `b` seconds, on the 48th grid.
fn at(seconds: f64) -> Tick {
    Tick((seconds * 48.0).round() as i64)
}

fn song(mut notes: Vec<Note>) -> Song {
    notes.sort_by_key(|n| (n.tick, n.lane));
    common::song("dance-single", TimingMap::constant(60.0, 0.0), notes)
}

fn note(seconds: f64, lane: u8, kind: NoteKind) -> Note {
    Note::new(at(seconds), lane, kind)
}

fn tap(seconds: f64, lane: u8) -> Note {
    note(seconds, lane, NoteKind::Tap)
}

fn hold(seconds: f64, end: f64, lane: u8) -> Note {
    note(seconds, lane, NoteKind::HoldHead { end: at(end) })
}

fn roll(seconds: f64, end: f64, lane: u8) -> Note {
    note(seconds, lane, NoteKind::RollHead { end: at(end) })
}

fn mine(seconds: f64, lane: u8) -> Note {
    note(seconds, lane, NoteKind::Mine)
}

/// A press at `t` and its release 50 ms later.
fn hit(t: f64, lane: u8) -> [(f64, u8, bool); 2] {
    [(t, lane, true), (t + 0.05, lane, false)]
}

fn ddi(options: DdiOptions) -> Ruleset {
    ddi::ruleset(&options)
}

fn normal() -> Ruleset {
    ddi(DdiOptions::default())
}

fn with_boos() -> Ruleset {
    ddi(DdiOptions {
        boos: true,
        ..DdiOptions::default()
    })
}

fn edges(parts: &[[(f64, u8, bool); 2]]) -> Vec<(f64, u8, bool)> {
    parts.iter().flatten().copied().collect()
}

/// Taps in lane 1 every second from 1 s to `last`.
fn steady(last: u32) -> Vec<Note> {
    (1..=last).map(|s| tap(f64::from(s), 1)).collect()
}

/// Each of `steady`'s taps hit on time.
fn on_time(last: u32) -> Vec<[(f64, u8, bool); 2]> {
    (1..=last).map(|s| hit(f64::from(s), 1)).collect()
}

#[test]
fn windows_are_whole_grid_steps_inclusive() {
    let tiers = [
        Judgement::W1,
        Judgement::W2,
        Judgement::W3,
        Judgement::W4,
        Judgement::W5,
    ];
    for d in Difficulty::ALL {
        let r = ddi(DdiOptions {
            difficulty: d,
            ..DdiOptions::default()
        });
        let t = &r.judge;
        let hz = d.hz();
        // W0 inside W1, its bound included.
        assert_eq!(t.is_w0(Judgement::W1, 1.0 / hz), Some(true), "{d:?}");
        assert_eq!(t.is_w0(Judgement::W1, -1.0 / hz), Some(true), "{d:?}");
        assert_eq!(t.is_w0(Judgement::W1, 1.5 / hz), Some(false), "{d:?}");
        for (tier, units) in tiers.iter().zip([2u32, 5, 11, 17, 25]) {
            let bound = f64::from(units) / hz;
            for b in [bound, -bound] {
                assert_eq!(t.judge(b), Some(*tier), "{d:?} {b}");
            }
            let past = f64::from(units) / hz + 1e-9;
            assert_ne!(t.judge(past), Some(*tier), "{d:?} {past}");
            assert_ne!(t.judge(-past), Some(*tier), "{d:?} {past}");
        }
        assert_eq!(t.judge(25.0 / hz + 1e-9), None);
        assert_eq!(t.hold_window_seconds(), 38.0 / hz);
        assert_eq!(t.roll_window_seconds(), 42.0 / hz);
        assert_eq!(t.mine_window_seconds(), 0.075);
        assert_eq!((t.scale, t.add), (1.0, 0.0));
    }
    // In play, to the microsecond: Normal's W1 ends at 16.667 ms.
    let s = song(vec![tap(1.0, 0), tap(2.0, 0)]);
    let (r, _) = run(&s, normal(), edges(&[hit(1.016666, 0), hit(2.016667, 0)]));
    assert_eq!(r.tally.count(Judgement::W1), 1);
    assert_eq!(r.tally.count(Judgement::W2), 1);
    let (r, _) = run(&s, normal(), edges(&[hit(0.791667, 0), hit(1.791666, 0)]));
    assert_eq!(r.tally.count(Judgement::W5), 1, "{:?}", r.tally);
    assert_eq!(r.tally.count(Judgement::Miss), 1, "{:?}", r.tally);
}

#[test]
fn words_say_early_or_late_and_counts_leave_w0_out() {
    let names = ddi::names();
    let words = |delta: f64, j| names.word(j, delta, false);
    assert_eq!(words(-0.01, Judgement::W1), "Awesome");
    assert_eq!(words(0.01, Judgement::W1), "Amazing");
    assert_eq!(names.word(Judgement::W1, 0.0, true), "Absolute");
    assert_eq!(names.word(Judgement::W1, -0.005, true), "Absolute");
    assert_eq!(words(-0.03, Judgement::W2), "Fantastic");
    assert_eq!(words(0.03, Judgement::W2), "Glorious");
    assert_eq!(words(-0.06, Judgement::W3), "Nice");
    assert_eq!(words(0.06, Judgement::W3), "Great");
    assert_eq!(words(-0.1, Judgement::W4), "OK");
    assert_eq!(words(0.1, Judgement::W4), "Good");
    assert_eq!(words(-0.2, Judgement::W5), "Too early");
    assert_eq!(words(0.2, Judgement::W5), "Too late");
    assert_eq!(words(0.2, Judgement::Miss), "Miss");
    assert_eq!(names.sides(Judgement::W3), ("Nice", "Great"));
    assert_eq!(
        (names.held.as_str(), names.let_go.as_str()),
        ("Held", "Dropped")
    );
    // Every late word has a G, no early one does.
    for j in &Judgement::ALL[..4] {
        let (early, late) = names.sides(*j);
        assert!(
            late.contains(['g', 'G']) && !early.contains(['g', 'G']),
            "{j:?}"
        );
    }
    // Early and Late: W1 and worse, never a W0.
    let s = song(steady(5));
    let (r, _) = run(
        &s,
        normal(),
        edges(&[
            hit(1.0, 1),
            hit(1.99, 1),
            hit(3.01, 1),
            hit(3.97, 1),
            hit(5.008, 1),
        ]),
    );
    assert_eq!(r.tally.w0, 2);
    assert_eq!((r.fast, r.slow), (2, 1));
    let sides = r.ddi.unwrap().sides;
    assert_eq!(sides[Judgement::W1.index()], [1, 1]);
    assert_eq!(sides[Judgement::W2.index()], [1, 0]);
}

#[test]
fn combo_runs_to_great_and_breaks_on_mines() {
    let mut notes = steady(6);
    notes.push(mine(4.5, 0));
    notes.push(hold(2.5, 3.5, 2));
    let s = song(notes);
    // W1, W3 (continues), W4 (breaks), W1, a mine, W1, W1.
    let mut e = edges(&[
        hit(1.0, 1),
        hit(2.08, 1),
        hit(3.12, 1),
        hit(4.0, 1),
        hit(4.5, 0),
        hit(5.0, 1),
        hit(6.0, 1),
    ]);
    // The hold's head hit, then let go: dropped, neither added nor broken.
    e.extend([(2.5, 2, true), (2.6, 2, false)]);
    let (r, _) = run(&s, normal(), e);
    assert_eq!(r.tally.count(Judgement::W3), 1);
    assert_eq!(r.tally.count(Judgement::W4), 1);
    assert_eq!((r.tally.mine_hit, r.tally.let_go), (1, 1));
    // 1 + W3 + head = 3 before the W4; 1 after it; 2 after the mine.
    assert_eq!(r.max_combo, 3);
}

/// The Boos of `edges` on `notes` with Boos on.
fn boos(notes: Vec<Note>, e: &[[(f64, u8, bool); 2]]) -> u32 {
    run(&song(notes), with_boos(), edges(e)).0.tally.boo
}

#[test]
fn boos_are_off_by_default_and_count_a_stray_press() {
    let mut e = on_time(4);
    e.push(hit(2.5, 0));
    let (r, _) = run(&song(steady(4)), normal(), edges(&e));
    assert_eq!(r.tally.boo, 0);
    assert_eq!(boos(steady(4), &e), 1);
    // A Boo costs 5 points and 2 life, and breaks no combo.
    let (r, _) = run(&song(steady(4)), with_boos(), edges(&e));
    assert_eq!(r.score.ddi.unwrap().points, 4 * 50 - 5);
    assert_eq!(r.max_combo, 4);
}

/// A Boo shows its word: the play's judgment flash is a Boo's from the
/// press that counted, for as long as a tap's shows, the combo as it
/// was; no explosion on its receptor; the exempt presses show nothing,
/// nor any press with Boos off; and as many are shown as the results
/// count, in the play and in its replay from the log.
#[test]
fn a_boo_shows_its_word_and_an_exempt_press_none() {
    use ddi_engine::frame::SkinEventKind;
    let flash = |p: &Player, t: f64| p.judgement_flash(t);
    let names = with_boos().names;
    assert_eq!(names.boo.as_deref(), Some("Boo"));
    assert_eq!(names.boo.as_deref(), Some(ddi::BOO_WORD));
    // Taps at 1..=6 s in lane 1; stray presses in lane 0.
    let mut p = Player::new(&song(steady(6)), 0, with_boos(), PlayOptions::default());
    p.start(0.0, 0.0);
    // Before the first note: exempt, nothing shows.
    p.input_at(0, true, 0.5);
    p.input_at(0, false, 0.55);
    assert_eq!(flash(&p, 0.6), None);
    // The first tap, on time.
    p.input_at(1, true, 1.0);
    p.input_at(1, false, 1.05);
    let hit = flash(&p, 1.1).unwrap();
    assert!(!hit.boo);
    assert_eq!((hit.judgement, hit.combo_at), (Judgement::W1, 1));
    // A stray press: a Boo, shown from the press on.
    let events = p.input_at(0, true, 1.5);
    assert!(events.iter().any(|e| e.kind == JudgeEventKind::Boo));
    let boo = flash(&p, 1.5).unwrap();
    assert!(boo.boo);
    assert_eq!(boo.age, 0.0);
    // No early or late side, the combo as it was (a Boo breaks none).
    assert_eq!((boo.delta_seconds, boo.w0, boo.combo_at), (0.0, None, 1));
    // Its release shows nothing new, and it goes as a tap's does.
    p.input_at(0, false, 1.55);
    assert!(flash(&p, 1.9).is_some_and(|f| f.boo && (f.age - 0.4).abs() < 1e-6));
    assert_eq!(flash(&p, 2.6), None);
    // No explosion and no hit effect on its receptor: the skin hears of
    // the press hitting nothing and of its release alone.
    let lane0: Vec<_> = p
        .skin_events_since(0)
        .filter(|e| e.lane == 0)
        .map(|e| e.kind)
        .collect();
    assert!(
        lane0.iter().all(|k| matches!(
            k,
            SkinEventKind::Press | SkinEventKind::Lift | SkinEventKind::Step(None)
        )),
        "{lane0:?}"
    );
    // The next tap's word takes its place.
    p.input_at(0, true, 2.5);
    assert!(flash(&p, 2.5).unwrap().boo);
    p.input_at(1, true, 3.0);
    assert!(!flash(&p, 3.0).unwrap().boo);
    p.update_at(1e6);
    // After the last note: exempt.
    p.input_at(0, true, 1e6);
    let r = p.results();
    assert_eq!(r.tally.boo, 2);
    // The replay from the log shows the same words at the same presses.
    let mut again = Player::new(&song(steady(6)), 0, with_boos(), PlayOptions::default());
    again.start(0.0, 0.0);
    let mut shown = 0;
    for i in &p.log().inputs {
        again.input_at(i.lane, i.pressed, i.time);
        if i.pressed
            && again
                .judgement_flash(i.time)
                .is_some_and(|f| f.boo && f.age == 0.0)
        {
            shown += 1;
        }
    }
    again.update_at(1e6);
    assert_eq!(shown, 2);
    assert_eq!(again.results().tally.boo, shown);
    // Boos off: the same presses show and count nothing.
    let mut off = Player::new(&song(steady(6)), 0, normal(), PlayOptions::default());
    off.start(0.0, 0.0);
    off.input_at(1, true, 1.0);
    off.input_at(0, true, 1.5);
    assert!(!flash(&off, 1.5).unwrap().boo);
    off.update_at(1e6);
    assert_eq!(off.results().tally.boo, 0);
    // A long break, a mine nearby: exempt, nothing shown.
    let notes = vec![
        tap(1.0, 1),
        tap(9.0, 1),
        tap(10.0, 1),
        mine(10.5, 2),
        tap(11.0, 1),
    ];
    let mut p = Player::new(&song(notes), 0, with_boos(), PlayOptions::default());
    p.start(0.0, 0.0);
    p.input_at(1, true, 1.0);
    p.input_at(1, false, 1.05);
    p.input_at(0, true, 5.0);
    assert!(!flash(&p, 5.0).is_some_and(|f| f.boo));
    p.input_at(0, false, 5.05);
    p.input_at(1, true, 9.0);
    p.input_at(1, false, 9.05);
    p.input_at(0, true, 10.4);
    assert!(!flash(&p, 10.4).unwrap().boo);
    p.update_at(1e6);
    assert_eq!(p.results().tally.boo, 0);
    // Other rulesets that count stray presses name no word for them:
    // FFR's Boo and Dancing☆Onigiri's Excessive show nothing in play.
    use ddi_engine::rules::presets;
    assert_eq!(presets::ffr().names.boo, None);
    let mut p = Player::new(&song(steady(6)), 0, presets::ffr(), PlayOptions::default());
    p.start(0.0, 0.0);
    p.input_at(1, true, 1.0);
    let events = p.input_at(0, true, 1.5);
    assert!(events.iter().any(|e| e.kind == JudgeEventKind::Boo));
    assert!(!flash(&p, 1.5).unwrap().boo);
}

#[test]
fn boos_spare_warm_up_the_end_and_long_breaks() {
    let mut e = on_time(4);
    // W5's earliest window is 208 ms before the first note.
    e.push(hit(0.7, 0));
    assert_eq!(boos(steady(4), &e), 0);
    e.pop();
    e.push(hit(0.8, 0));
    assert_eq!(boos(steady(4), &e), 1);
    // After the last note's end.
    e.pop();
    e.push(hit(4.1, 0));
    assert_eq!(boos(steady(4), &e), 0);
    // A break of seven seconds: no note within 3 s of 6.5 (the next one's
    // earliest window is 9.79), one within 3 s of 5.9.
    let mut notes = steady(3);
    notes.extend([tap(10.0, 1), tap(11.0, 1)]);
    let mut e = on_time(3);
    e.extend([hit(10.0, 1), hit(11.0, 1)]);
    e.push(hit(6.5, 0));
    assert_eq!(boos(notes.clone(), &e), 0);
    e.pop();
    e.push(hit(5.9, 0));
    assert_eq!(boos(notes, &e), 1);
}

#[test]
fn boos_spare_a_lift_ready_holds_rolls_and_mines() {
    // A lift's ready press, up to a second before it.
    let mut notes = steady(5);
    notes.push(note(3.5, 0, NoteKind::Lift));
    let mut e = on_time(5);
    e.push([(2.6, 0, true), (3.5, 0, false)]);
    let (r, _) = run(&song(notes.clone()), with_boos(), edges(&e));
    assert_eq!((r.tally.boo, r.tally.w0), (0, 6));
    e.pop();
    e.push([(2.4, 0, true), (3.5, 0, false)]);
    assert_eq!(boos(notes, &e), 1);
    // A hold pressed again within its grace (317 ms), and past it.
    let mut notes = steady(4);
    notes.push(hold(1.0, 3.0, 0));
    let mut e = on_time(4);
    e.push([(1.0, 0, true), (1.5, 0, false)]);
    e.push([(1.7, 0, true), (3.1, 0, false)]);
    let (r, _) = run(&song(notes.clone()), with_boos(), edges(&e));
    assert_eq!((r.tally.boo, r.tally.held), (0, 1));
    e.pop();
    e.push([(2.0, 0, true), (3.1, 0, false)]);
    let (r, _) = run(&song(notes), with_boos(), edges(&e));
    assert_eq!((r.tally.boo, r.tally.let_go), (1, 1));
    // A roll's taps.
    let mut notes = steady(4);
    notes.push(roll(1.0, 3.0, 0));
    let mut e = on_time(4);
    for i in 0..9 {
        e.push(hit(1.0 + 0.25 * f64::from(i), 0));
    }
    let (r, _) = run(&song(notes), with_boos(), edges(&e));
    assert_eq!((r.tally.boo, r.tally.held), (0, 1));
    // Within half a second of a mine in any lane (stepped on or not).
    let mut notes = steady(4);
    notes.push(mine(2.5, 2));
    let mut e = on_time(4);
    e.push(hit(2.1, 0));
    assert_eq!(boos(notes.clone(), &e), 0);
    e.pop();
    e.push(hit(1.9, 0));
    assert_eq!(boos(notes.clone(), &e), 1);
    // Releases never are.
    let mut e = on_time(4);
    e.push([(2.4, 0, false), (2.45, 0, false)]);
    assert_eq!(boos(notes, &e), 0);
}

#[test]
fn a_mine_locks_its_lane_for_a_second() {
    // Stepped on by a press: the next press in its lane is ignored (no
    // hit, no Boo), its note missed; one after the lock hits.
    let mut notes = steady(4);
    notes.extend([mine(2.0, 0), tap(2.5, 0), tap(3.5, 0)]);
    let mut e = on_time(4);
    e.extend([hit(2.0, 0), hit(2.5, 0), hit(3.5, 0)]);
    let (r, events) = run(&song(notes), with_boos(), edges(&e));
    assert_eq!(r.tally.mine_hit, 1);
    assert_eq!(r.tally.boo, 0);
    assert_eq!(r.tally.count(Judgement::Miss), 1);
    assert_eq!(r.tally.w0, 5);
    assert!(
        events
            .iter()
            .any(|e| e.lane == 0 && matches!(e.kind, JudgeEventKind::Tap(Judgement::Miss, _)))
    );
    // Held across a mine in a hold: the key counts as released, the hold
    // is dropped past its grace with the unit it earned (at 2 s), a
    // second mine in the lock is not stepped on, and the key still down
    // when the lock ends must be pressed again.
    let mut notes = steady(6);
    notes.extend([
        hold(1.0, 4.0, 0),
        mine(2.0, 0),
        mine(2.5, 0),
        tap(3.5, 0),
        tap(5.5, 0),
    ]);
    let mut e = on_time(6);
    e.push([(1.0, 0, true), (5.0, 0, false)]);
    e.push(hit(5.5, 0));
    let (r, events) = run(&song(notes), normal(), edges(&e));
    assert_eq!(r.tally.mine_hit, 1);
    assert_eq!(r.tally.let_go, 1);
    assert_eq!(r.tally.hold_units, 1);
    assert_eq!(r.tally.count(Judgement::Miss), 1);
    let dropped = events
        .iter()
        .find(|e| e.kind == JudgeEventKind::LetGo)
        .unwrap();
    assert!((dropped.song_time - (2.0 + 38.0 / 120.0)).abs() < 1e-9);
    // A press in the lock held past its end: still up until pressed again.
    let mut notes = steady(4);
    notes.extend([mine(2.0, 0), tap(3.2, 0), tap(3.6, 0)]);
    let mut e = on_time(4);
    e.extend([hit(2.0, 0), [(2.8, 0, true), (3.3, 0, false)], hit(3.6, 0)]);
    let (r, _) = run(&song(notes), with_boos(), edges(&e));
    assert_eq!(r.tally.count(Judgement::Miss), 1);
    assert_eq!((r.tally.boo, r.tally.w0), (0, 5));
}

#[test]
fn holds_earn_a_unit_a_second_in_the_charts_time() {
    // 2.5 s: three units (1 s, 2 s, the tail).
    let mut notes = steady(5);
    notes.push(hold(1.0, 3.5, 0));
    let s = song(notes);
    let mut e = on_time(5);
    e.push([(1.0, 0, true), (3.6, 0, false)]);
    let (r, _) = run(&s, normal(), e.iter().flatten().copied().collect());
    assert_eq!((r.tally.held, r.tally.hold_units), (1, 3));
    let v = r.score.ddi.unwrap();
    assert_eq!((v.points, v.most), (6 * 50 + 3 * 50, 6 * 50 + 3 * 50));
    // Let go at 2.2 s: dropped at 2.52, one unit.
    e.pop();
    e.push([(1.0, 0, true), (2.2, 0, false)]);
    let (r, _) = run(&s, normal(), edges(&e));
    assert_eq!((r.tally.let_go, r.tally.hold_units), (1, 1));
    assert_eq!(r.score.ddi.unwrap().points, 6 * 50 + 50);
    // A missed head is the Miss alone: no unit, not dropped too.
    e.pop();
    let (r, events) = run(&s, normal(), edges(&e));
    assert_eq!(r.tally.count(Judgement::Miss), 1);
    assert_eq!(
        (r.tally.let_go, r.tally.held, r.tally.hold_units),
        (0, 0, 0)
    );
    assert!(!events.iter().any(|e| e.kind == JudgeEventKind::LetGo));
    assert_eq!(r.life.last().unwrap()[1], 0.5);
    // Through a stop: its time counts (1.5 s of beats and a 1 s stop).
    let mut timing = TimingMap::constant(60.0, 0.0);
    timing.stops.push(StopSegment {
        tick: at(2.0),
        seconds: 1.0,
    });
    let stopped = common::song(
        "dance-single",
        timing,
        vec![hold(1.0, 2.5, 0), tap(1.0, 1), tap(4.0, 1)],
    );
    let (r, _) = run(
        &stopped,
        normal(),
        edges(&[[(1.0, 0, true), (3.6, 0, false)], hit(1.0, 1), hit(5.0, 1)]),
    );
    assert_eq!((r.tally.held, r.tally.hold_units), (1, 3));
    // At another music rate: the same units, the windows in real time.
    for rate in [0.5, 1.5, 2.0] {
        let options = PlayOptions {
            clock: ClockOptions {
                rate,
                ..ClockOptions::default()
            },
            ..PlayOptions::default()
        };
        let mut p = Player::new(&s, 0, normal(), options);
        p.start(0.0, 0.0);
        let mut e = edges(&on_time(5));
        e.extend([(1.0, 0, true), (3.6, 0, false)]);
        autoplay::sort_edges(&mut e);
        for (t, lane, pressed) in e {
            p.update_at(t);
            p.input_at(lane, pressed, t);
        }
        p.update_at(100.0);
        let r = p.results();
        assert_eq!((r.tally.held, r.tally.hold_units), (1, 3), "{rate}");
        let key = p.ruleset().key();
        assert_eq!(replay(&s, 0, &key, &options, p.log()), r, "{rate}");
        let t = &p.ruleset().judge;
        assert!((t.window(Judgement::W1).unwrap().late - rate / 60.0).abs() < 1e-12);
        assert!((t.hold_window_seconds() - rate * 38.0 / 120.0).abs() < 1e-12);
        assert!((t.mine_window_seconds() - rate * 0.075).abs() < 1e-12);
        assert_eq!(t.lock, Some(rate));
    }
    // At twice the rate a press 25 ms of song time late is 12.5 ms real:
    // Amazing, not Glorious.
    let options = PlayOptions {
        clock: ClockOptions {
            rate: 2.0,
            ..ClockOptions::default()
        },
        ..PlayOptions::default()
    };
    let mut p = Player::new(&song(vec![tap(1.0, 0)]), 0, normal(), options);
    p.start(0.0, 0.0);
    p.input_at(0, true, 1.025);
    p.update_at(10.0);
    assert_eq!(p.results().tally.count(Judgement::W1), 1);
}

#[test]
fn life_starts_at_half_and_steps_by_difficulty() {
    let s = song(steady(40));
    // 25 hits fill it; ten Misses from full end the play on Normal.
    let mut e = on_time(25);
    e.truncate(25);
    let (r, _) = run(&s, normal(), edges(&e));
    assert!(r.failed);
    assert_eq!(r.grade, "F");
    assert_eq!(r.ddi.unwrap().lamp, Lamp::Failed);
    assert_eq!(r.tally.count(Judgement::Miss), 10);
    assert_eq!(r.life.iter().map(|l| l[1]).fold(0.0, f32::max), 1.0);
    // Practice: failed there, played on with life at 0 to the end.
    let practice = ddi(DdiOptions {
        practice: true,
        ..DdiOptions::default()
    });
    assert_eq!(practice.fail, FailPolicy::ImmediateContinue);
    let mut e2 = e.clone();
    e2.extend((36..=40).map(|s| hit(f64::from(s), 1)));
    let (r, _) = run(&s, practice, edges(&e2));
    assert!(r.failed);
    assert_eq!(r.grade, "F");
    assert_eq!(r.tally.w0, 30);
    assert_eq!(r.tally.count(Judgement::Miss), 10);
    assert_eq!(r.life.last().unwrap()[1], 0.0);
    // Easy loses a fifth, Hard twice: one Miss from 50.
    for (d, after) in [
        (Difficulty::Easy, 0.48),
        (Difficulty::Normal, 0.40),
        (Difficulty::Hard, 0.30),
    ] {
        let rules = ddi(DdiOptions {
            difficulty: d,
            ..DdiOptions::default()
        });
        let (r, _) = run(&song(steady(1)), rules, Vec::new());
        assert_eq!(r.life.last().unwrap()[1], after, "{d:?}");
        assert!(!r.failed);
    }
}

#[test]
fn scores_three_ways_and_below_zero() {
    let s = song(steady(4));
    // W0, W2 early, a Miss, W3 late: 50 + 40 − 10 + 25 = 105 of 200.
    let (r, _) = run(
        &s,
        normal(),
        edges(&[hit(1.0, 1), hit(1.97, 1), hit(4.06, 1)]),
    );
    let v = r.score.ddi.unwrap();
    assert_eq!((v.points, v.most), (105, 200));
    assert_eq!(v.ratio, 525_000);
    assert_eq!(v.percent, 5250);
    assert_eq!(v.absolute, 105 * 100 + 1);
    // The score: the proportional part and the absolute score together.
    assert_eq!(v.score, 525_000 + 10_501);
    assert_eq!(r.score.percent, Some(0.525));
    // Nothing hit but stray presses and a mine: negative.
    let mut notes = steady(4);
    notes.push(mine(2.5, 0));
    let (r, _) = run(
        &song(notes),
        ddi(DdiOptions {
            boos: true,
            practice: true,
            ..DdiOptions::default()
        }),
        edges(&[hit(2.5, 0), hit(1.5, 2)]),
    );
    let v = r.score.ddi.unwrap();
    assert_eq!(v.points, -4 * 10 - 10 - 5);
    assert_eq!(v.ratio, -275_000);
    assert_eq!(v.percent, -2750);
    assert_eq!(v.absolute, -5500);
    assert_eq!(v.score, -280_500);
    // A best: the higher score, which the W0s break ties in.
    let a = ddi::DdiScoreView::of(100, 200, 1);
    let b = ddi::DdiScoreView::of(100, 200, 0);
    assert_eq!(a.best_cmp(&b), std::cmp::Ordering::Greater);
    assert_eq!(
        ddi::DdiScoreView::of(99, 200, 9).best_cmp(&b),
        std::cmp::Ordering::Less
    );
}

/// The score shown is the proportional part plus the absolute score: a
/// perfect 1,000-note chart with every tap a W0 gives 1,000,000 +
/// 5,000,000 + 1,000, and a longer chart gives more.
#[test]
fn a_perfect_thousand_notes_score_the_sum() {
    let (r, _) = run(&song(steady(1000)), normal(), edges(&on_time(1000)));
    assert_eq!(r.tally.w0, 1000);
    let v = r.score.ddi.unwrap();
    assert_eq!((v.ratio, v.absolute), (1_000_000, 5_001_000));
    assert_eq!(v.score, 6_001_000);
    assert_eq!(v.percent, 10_000);
    let (r, _) = run(&song(steady(1200)), normal(), edges(&on_time(1200)));
    assert_eq!(r.score.ddi.unwrap().score, 1_000_000 + 6_000_000 + 1200);
}

#[test]
fn grades_have_floors_on_short_charts() {
    let grade =
        |notes: u32, e: Vec<[(f64, u8, bool); 2]>| run(&song(steady(notes)), normal(), edges(&e)).0;
    // Every tap W0: Absolute Dito.
    assert_eq!(grade(4, on_time(4)).grade, ddi::ABSOLUTE_DITO);
    // One W1: SSS.
    let mut e = on_time(4);
    e[1] = hit(2.01, 1);
    assert_eq!(grade(4, e).grade, "SSS");
    // One W2 of four is 95 %, an S by the percent: SS by the floor.
    let mut e = on_time(4);
    e[1] = hit(2.03, 1);
    let r = grade(4, e);
    assert_eq!(r.score.ddi.unwrap().percent, 9500);
    assert_eq!(r.grade, "SS");
    // One Miss of two is 40 %, a D by the percent: C by the floor.
    let mut e = on_time(2);
    e.remove(1);
    let r = grade(2, e);
    assert_eq!(r.score.ddi.unwrap().percent, 4000);
    assert_eq!(r.grade, "C");
    // Two Misses of three: the floor is D.
    let mut e = on_time(3);
    e.truncate(1);
    assert_eq!(grade(3, e).grade, "D");
    // A Boo counts against perfection: 6 from Absolute Dito, C at least,
    // and 97.5 % is an SS.
    let mut e = on_time(4);
    e.push(hit(2.5, 0));
    let (r, _) = run(&song(steady(4)), with_boos(), edges(&e));
    assert_eq!(r.grade, "SS");
    // On a long chart the percent decides.
    let mut e = on_time(100);
    e[50] = hit(51.03, 1);
    let r = grade(100, e);
    assert_eq!(r.score.ddi.unwrap().percent, 9980);
    assert_eq!(r.grade, "SSS");
}

#[test]
fn an_autoplayed_absolute_dito_is_absolute_bot() {
    let s = song(steady(4));
    let mut p = Player::new(&s, 0, normal(), PlayOptions::default());
    p.set_autoplay(true);
    p.start(0.0, 0.0);
    for (t, lane, pressed) in edges(&on_time(4)) {
        p.update_at(t);
        p.input_at(lane, pressed, t);
    }
    p.update_at(100.0);
    let r = p.results();
    assert_eq!(r.grade, ddi::ABSOLUTE_BOT);
    assert_eq!(r.ddi.unwrap().lamp, Lamp::AbsoluteDito);
    // Judged again from its log (which does not say who played).
    let again = replay(&s, 0, &p.ruleset().key(), &PlayOptions::default(), p.log());
    assert_eq!(again.grade, ddi::ABSOLUTE_DITO);
    assert_eq!(ddi::bot_grade(&again.grade), ddi::ABSOLUTE_BOT);
    assert_eq!(ddi::bot_grade("SSS"), "SSS");
}

#[test]
fn lamps_from_amazing_to_failed() {
    let lamp = |w0: u32, w1: u32, w2: u32, w3: u32, f: fn(&mut Tally)| {
        let mut t = Tally {
            w0,
            ..Tally::default()
        };
        t.taps[0] = w0 + w1;
        t.taps[1] = w2;
        t.taps[2] = w3;
        f(&mut t);
        Lamp::of(&t, false, 0)
    };
    let none = |_: &mut Tally| {};
    assert_eq!(lamp(10, 0, 0, 0, none), Lamp::AbsoluteDito);
    assert_eq!(lamp(9, 1, 0, 0, none), Lamp::AmazingFullCombo);
    assert_eq!(lamp(9, 0, 1, 0, none), Lamp::GloriousFlag);
    assert_eq!(lamp(8, 0, 2, 0, none), Lamp::GloriousFullCombo);
    assert_eq!(lamp(8, 0, 2, 1, none), Lamp::GreatFlag);
    assert_eq!(lamp(8, 0, 0, 2, none), Lamp::FullCombo);
    let ones: [fn(&mut Tally); 6] = [
        |t: &mut Tally| t.taps[3] = 1,
        |t: &mut Tally| t.taps[4] = 1,
        |t: &mut Tally| t.taps[5] = 1,
        |t: &mut Tally| t.mine_hit = 1,
        |t: &mut Tally| t.let_go = 1,
        |t: &mut Tally| t.boo = 1,
    ];
    for one in ones {
        assert_eq!(lamp(10, 0, 0, 0, one), Lamp::BreakFlag);
        assert_eq!(lamp(8, 0, 2, 2, one), Lamp::BreakFlag);
    }
    assert_eq!(
        lamp(10, 0, 0, 0, |t| {
            t.boo = 1;
            t.let_go = 1
        }),
        Lamp::Clear
    );
    let mut t = Tally::default();
    t.taps[0] = 10;
    t.w0 = 10;
    assert_eq!(Lamp::of(&t, true, 0), Lamp::Failed);
    assert_eq!(Lamp::of(&t, false, 1), Lamp::BreakFlag);
    assert!(Lamp::AbsoluteDito > Lamp::AmazingFullCombo);
    assert!(Lamp::GloriousFlag > Lamp::GloriousFullCombo);
    assert!(Lamp::GreatFlag > Lamp::FullCombo);
    assert!(Lamp::FullCombo > Lamp::BreakFlag);
    assert!(Lamp::BreakFlag > Lamp::Clear && Lamp::Clear > Lamp::Failed);
    // In play: one Boo on an otherwise Amazing play is a Break flag.
    let mut e = on_time(4);
    e[0] = hit(1.01, 1);
    e.push(hit(2.5, 0));
    let (r, _) = run(&song(steady(4)), with_boos(), edges(&e));
    assert_eq!(r.ddi.unwrap().lamp, Lamp::BreakFlag);
    assert_eq!(r.max_combo, 4);
}

#[test]
fn keys_build_ddi_on_every_chart_and_bests_drop_practice() {
    let options = DdiOptions {
        difficulty: Difficulty::Hard,
        boos: true,
        practice: true,
    };
    let key = ddi(options).key();
    assert_eq!(
        key,
        RulesetKey {
            id: "ddi".into(),
            holds: HoldStyle::Dance,
            danoni: None,
            stepmania: Default::default(),
            ddi: Some(options),
        }
    );
    assert_eq!(
        key.best_key().ddi,
        Some(DdiOptions {
            practice: false,
            ..options
        })
    );
    let json = serde_json::to_string(&key).unwrap();
    assert_eq!(serde_json::from_str::<RulesetKey>(&json).unwrap(), key);
    // Other keys are written as before.
    let itg = ddi_engine::rules::presets::itg().key();
    assert!(!serde_json::to_string(&itg).unwrap().contains("ddi"));
    // Pump's charts keep DDI's holds, not ticked ones; any layout builds.
    for layout in ["pump-single", "pump-double", "dance-double", "danoni-7"] {
        let s = common::song(
            layout,
            TimingMap::constant(60.0, 0.0),
            vec![hold(1.0, 3.0, 0), tap(2.0, 1)],
        );
        let rules = RulesetKey {
            holds: HoldStyle::Ticked,
            ..key.clone()
        }
        .build(&s.charts[0]);
        assert_eq!(rules.judge.hold, HoldRules::Decay, "{layout}");
        assert_eq!(rules.key(), key, "{layout}");
        let r = common::autoplayed(&s, rules);
        assert_eq!(r.grade, ddi::ABSOLUTE_DITO, "{layout}");
        assert_eq!(r.tally.hold_units, 2, "{layout}");
    }
}

/// Every bundled chart, autoplayed under DDI (each difficulty, Boos on):
/// Absolute Dito, judged again from its log the same.
#[test]
fn bundled_charts_autoplay_to_absolute_dito() {
    let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../assets/songs");
    let mut ids: Vec<_> = std::fs::read_dir(&dir)
        .unwrap()
        .flatten()
        .filter(|e| e.path().is_dir())
        .collect();
    ids.sort_by_key(|e| e.file_name());
    let mut charts = 0;
    for e in ids {
        let Some(path) = std::fs::read_dir(e.path())
            .unwrap()
            .flatten()
            .map(|f| f.path())
            .find(|p| p.extension().is_some_and(|x| x == "ssc" || x == "sm"))
        else {
            continue;
        };
        let ext = path.extension().unwrap().to_string_lossy().into_owned();
        let song = ddi_library::scan::parse_simfile(&std::fs::read_to_string(&path).unwrap(), &ext)
            .unwrap();
        for (i, c) in song.charts.iter().enumerate() {
            if song.layout_of(c).is_none() {
                continue;
            }
            let mut one = song.clone();
            one.charts = vec![c.clone()];
            for d in Difficulty::ALL {
                let rules = ddi(DdiOptions {
                    difficulty: d,
                    boos: true,
                    practice: false,
                });
                let r: Results = common::autoplayed(&one, rules);
                let name = format!("{} [{i}] {d:?}", e.file_name().to_string_lossy());
                assert_eq!(r.grade, ddi::ABSOLUTE_DITO, "{name}: {:?}", r.tally);
                assert_eq!(r.ddi.unwrap().lamp, Lamp::AbsoluteDito, "{name}");
                let v = r.score.ddi.unwrap();
                assert_eq!((v.ratio, v.percent), (1_000_000, 10_000), "{name}");
                assert_eq!(v.absolute, 100 * v.most + i64::from(r.tally.w0), "{name}");
                assert_eq!(v.score, 1_000_000 + v.absolute, "{name}");
            }
            charts += 1;
        }
    }
    eprintln!("{charts} charts");
    assert!(charts >= 8, "{charts}");
}

/// The receptor keeps the mine's hit effect for the whole lock: the
/// skin's `HitMine` again every `LOCK_EFFECT_PERIOD`, at the same times
/// whatever the frame rate, and once only where it is turned off
/// (reduced motion).
#[test]
fn a_locked_lane_repeats_its_mine_effect() {
    use ddi_engine::frame::SkinEventKind;
    use ddi_engine::player::LOCK_EFFECT_PERIOD;
    let effects = |step: f64, repeats: bool| -> Vec<f64> {
        let mut notes = steady(4);
        notes.push(mine(2.0, 0));
        let mut p = Player::new(&song(notes), 0, normal(), PlayOptions::default());
        p.set_lock_effect(repeats);
        p.start(0.0, 0.0);
        let mut pressed = false;
        let mut t = 0.0;
        while t < 4.5 {
            if !pressed && t >= 2.0 {
                p.input_at(0, true, 2.0);
                pressed = true;
            }
            p.update_at(t);
            t += step;
        }
        p.skin_events_since(0)
            .filter(|e| e.lane == 0 && e.kind == SkinEventKind::HitMine)
            .map(|e| e.song_time)
            .collect()
    };
    let at = effects(1.0 / 60.0, true);
    assert_eq!(at.len(), 4, "{at:?}");
    for (i, t) in at.iter().enumerate() {
        assert!((t - (2.0 + i as f64 * LOCK_EFFECT_PERIOD)).abs() < 1e-9);
    }
    assert_eq!(effects(1.0 / 144.0, true), at);
    assert_eq!(effects(1.0 / 60.0, false).len(), 1);
}
