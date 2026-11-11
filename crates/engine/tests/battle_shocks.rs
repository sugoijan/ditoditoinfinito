//! A battle's shocks in a play (`ddi_engine::battle`, M8a): judged apart
//! from the chart, a hit costing the ruleset's miss to the gauge and
//! nothing else (D1), the combo broken only where the ruleset's own mines
//! break it (D3), drawn as mines, kept in the log (version 4) and judged
//! alike again from it (under DDI a shock is a mine: `ddi_shocks.rs`);
//! and where charts have room for them (`schedule`: `PLACEMENT_V1`'s
//! margins, and `PLACEMENT_V0`'s clearances before them).
//!
//! Chart (60 BPM, so beat `b` is at `b` seconds), dance-single: taps at
//! beats 0 (lane 0), 2 (lane 3), 8 (lane 0) and, in the twin chart, a tap
//! at beat 5 lane 2 in place of the shock there. Shocks at beat 4 lane 1
//! (stepped on by a press), 5 lane 2 (crossed while held) and 6 lane 3
//! (passed).

use ddi_chart::{Chart, Difficulty, Note, NoteKind, Song, TICKS_PER_BEAT, Tick, TimingMap};
use ddi_engine::battle::{
    Clearance, HAZARD_INDEX_BASE, Hazard, HazardError, INJECTED_MINE_V0, PLACEMENT, PLACEMENT_V0,
    PLACEMENT_V1, PLACEMENT_V2, beside_count, schedule, schedule_with,
};
use ddi_engine::frame::SpriteKind;
use ddi_engine::history::{PlayLog, replay_with};
use ddi_engine::player::{BattleMines, PlayOptions, Player, Results};
use ddi_engine::rules::presets;
use ddi_engine::rules::{Judgement, Ruleset};
use ddi_platform::HostTime;

const RULESETS: [&str; 7] = [
    "itg",
    "itg-fa+",
    "sm5",
    "ddr-a",
    "ddr-x",
    "ddr-extreme",
    "ffr",
];

fn song(twin: bool) -> Song {
    let t = Tick::from_beats;
    let mut notes = vec![
        Note::new(t(0), 0, NoteKind::Tap),
        Note::new(t(2), 3, NoteKind::Tap),
        Note::new(t(8), 0, NoteKind::Tap),
    ];
    if twin {
        notes.push(Note::new(t(5), 2, NoteKind::Tap));
    }
    notes.sort_by_key(|n| (n.tick, n.lane));
    Song::generated(
        "Battle",
        TimingMap::constant(60.0, 0.0),
        Chart::plain("dance-single", Difficulty::Challenge, 9, notes),
        "test",
    )
}

fn shocks() -> Vec<Hazard> {
    [(4, 1), (5, 2), (6, 3)]
        .into_iter()
        .map(|(b, lane)| Hazard {
            tick: Tick(b * TICKS_PER_BEAT),
            lane,
        })
        .collect()
}

fn rules(id: &str, song: &Song) -> Ruleset {
    presets::stepmania(id, presets::HoldStyle::ByGame, &song.charts[0])
}

/// The taps on time; lane 1 pressed on the first shock (or `off` past
/// it), lane 2 held across the second.
fn script(off: f64) -> Vec<(f64, u8, bool)> {
    vec![
        (0.0, 0, true),
        (0.05, 0, false),
        (2.0, 3, true),
        (2.05, 3, false),
        (4.0 + off, 1, true),
        (4.05 + off, 1, false),
        (4.5, 2, true),
        (5.5, 2, false),
        (8.0, 0, true),
        (8.05, 0, false),
    ]
}

/// `script` on `song` under `id`, the shocks put in (`with`), judged in
/// `step` second updates: the player, its life at each update.
fn play(song: &Song, id: &str, with: bool, script: &[(f64, u8, bool)], step: f64) -> Player {
    let mut p = Player::new(song, 0, rules(id, song), PlayOptions::default());
    p.start(0.0, 0.0);
    if with {
        p.add_hazards(&shocks()).unwrap();
    }
    let mut next = 0;
    let mut t = -1.0;
    while !p.finished() && t < 20.0 {
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

/// The life just before and just after song second `at`.
fn life_around(r: &Results, at: f32) -> (f32, f32) {
    let before = r
        .life
        .iter()
        .take_while(|l| l[0] < at)
        .last()
        .map_or(r.life[0][1], |l| l[1]);
    let after = r
        .life
        .iter()
        .take_while(|l| l[0] <= at)
        .last()
        .map_or(before, |l| l[1]);
    (before, after)
}

#[test]
fn a_shock_costs_the_ruleset_s_miss_and_nothing_else_under_every_ruleset() {
    for id in RULESETS {
        let plain = song(false);
        let with = play(&plain, id, true, &script(0.0), 0.01).results();
        let without = play(&plain, id, false, &script(0.0), 0.01).results();
        // The held shock costs what the twin chart's tap there costs
        // missed, from the same life: both without the first shock's press.
        let quiet: Vec<_> = script(0.0).into_iter().filter(|e| e.1 != 1).collect();
        let held = play(&plain, id, true, &quiet, 0.01).results();
        assert_eq!(
            held.battle_mines,
            Some(BattleMines { hit: 1, avoided: 2 }),
            "{id}"
        );
        let twin = song(true);
        // The same presses: lane 2 down half a beat early, held past its
        // tap (no hit), as across the shock.
        let missed_play = play(&twin, id, false, &quiet, 0.01).results();
        assert_eq!(missed_play.tally.count(Judgement::Miss), 1, "{id}");
        let (b, a) = life_around(&held, 5.0);
        let miss_at = 5.0 + rules(id, &twin).judge.miss_after() as f32;
        let (mb, ma) = life_around(&missed_play, miss_at);
        assert_eq!(b, mb, "{id}: the same life before");
        assert!(
            (b - a - (mb - ma)).abs() < 1e-6,
            "{id}: a shock {}, a miss {}",
            b - a,
            mb - ma
        );
        assert!(a < b, "{id}: {b} -> {a}");
        assert!(
            with.life.last().unwrap()[1] < without.life.last().unwrap()[1],
            "{id}"
        );
    }
}

/// D3: the combo breaks where the ruleset's own mines break it (DDR A
/// and X), and nowhere else.
#[test]
fn a_shock_breaks_the_combo_only_where_the_ruleset_s_mines_do() {
    for id in RULESETS {
        let plain = song(false);
        let with = play(&plain, id, true, &script(0.0), 0.01);
        let without = play(&plain, id, false, &script(0.0), 0.01);
        let breaks = rules(id, &plain).combo.mine_breaks;
        if breaks {
            assert!(
                with.results().max_combo < 3 || with.combo() < without.combo(),
                "{id}"
            );
            assert_eq!(with.combo(), 1, "{id}: broken, then the last tap");
        } else {
            assert_eq!(with.combo(), without.combo(), "{id}");
            assert_eq!(
                with.results().max_combo,
                without.results().max_combo,
                "{id}"
            );
        }
    }
    assert!(rules("ddr-a", &song(false)).combo.mine_breaks);
    assert!(!rules("itg", &song(false)).combo.mine_breaks);
}

/// A press that steps on a shock is not also the chart's stray press
/// (DDR's and FFR's Boo).
#[test]
fn a_press_on_a_shock_is_no_boo_too() {
    for id in ["ddr-extreme", "ffr", "sm5"] {
        let plain = song(false);
        let with = play(&plain, id, true, &script(0.0), 0.01).results();
        let without = play(&plain, id, false, &script(0.0), 0.01).results();
        // Without shocks the press at 4 s is a stray press where the
        // ruleset counts them; with them it is the shock's alone (the
        // lane-2 press at 4.5 s stays one either way).
        assert!(with.tally.boo <= without.tally.boo, "{id}");
        if without.tally.boo > 0 {
            assert_eq!(with.tally.boo + 1, without.tally.boo, "{id}");
        }
    }
}

#[test]
fn ffr_s_shocks_are_judged_in_70_ms() {
    let plain = song(false);
    let ffr = rules("ffr", &plain);
    assert_eq!(ffr.judge.mine_window_seconds(), 0.0);
    assert_eq!(INJECTED_MINE_V0.window(&ffr.judge), 0.070);
    let hit = play(&plain, "ffr", true, &script(0.065), 0.01).results();
    let past = play(&plain, "ffr", true, &script(0.075), 0.01).results();
    assert_eq!(hit.battle_mines.unwrap().hit, 2);
    assert_eq!(past.battle_mines.unwrap().hit, 1);
}

#[test]
fn the_same_at_any_step_and_again_from_the_log() {
    let plain = song(false);
    for id in RULESETS {
        let a = play(&plain, id, true, &script(0.0), 1.0 / 60.0);
        let b = play(&plain, id, true, &script(0.0), 1.0 / 144.0);
        let c = play(&plain, id, true, &script(0.0), 0.7);
        assert_eq!(a.results(), b.results(), "{id}");
        assert_eq!(a.results(), c.results(), "{id}");
        let log = a.log().clone();
        assert_eq!(log.hazards.len(), 3);
        // Version 4, read back equal; judged again equal.
        let bytes = log.encode();
        assert_eq!(bytes[0], 4);
        let read = PlayLog::decode(&bytes).unwrap();
        assert_eq!(read, log);
        let again = replay_with(&plain, 0, rules(id, &plain), a.options(), &read);
        assert_eq!(again, a.results(), "{id}");
    }
    // A quit play with shocks keeps both.
    let mut q = PlayLog {
        start: 0.0,
        end: Some(3.25),
        ..PlayLog::default()
    };
    q.hazards = vec![ddi_engine::history::LoggedHazard { tick: 96, lane: 1 }];
    assert_eq!(PlayLog::decode(&q.encode()).unwrap(), q);
    // Without shocks the log keeps its old versions.
    let plain_log = play(&plain, "itg", false, &script(0.0), 0.01).log().clone();
    assert!(plain_log.encode()[0] < 4);
}

#[test]
fn shocks_show_as_the_skin_s_mines_until_stepped_on() {
    let plain = song(false);
    let mut p = Player::new(&plain, 0, rules("itg", &plain), PlayOptions::default());
    p.start(0.0, 0.0);
    p.add_hazards(&shocks()).unwrap();
    let shown = |p: &Player, t: f64| -> Vec<usize> {
        p.frame(HostTime(t))
            .notes
            .iter()
            .filter(|n| n.note_index >= HAZARD_INDEX_BASE)
            .inspect(|n| assert_eq!(n.kind, SpriteKind::Mine))
            .map(|n| n.note_index - HAZARD_INDEX_BASE)
            .collect()
    };
    p.update_at(2.5);
    assert_eq!(shown(&p, 2.5), vec![0, 1, 2]);
    p.input_at(1, true, 4.0);
    p.update_at(4.1);
    assert_eq!(shown(&p, 4.1), vec![1, 2], "the one stepped on goes");
    // Its hit is the skin's mine explosion in its lane.
    assert!(
        p.skin_events_since(0)
            .any(|e| e.lane == 1 && e.kind == ddi_engine::frame::SkinEventKind::HitMine)
    );
    // Late: refused, nothing changed.
    assert_eq!(
        p.add_hazards(&[Hazard {
            tick: Tick::from_beats(4),
            lane: 0
        }]),
        Err(HazardError::Late)
    );
    assert_eq!(p.hazards().unwrap().mines().len(), 3);
}

/// `placement-v1`'s margins, checked apart from the placement's own
/// code: every shock of `section` is outside each step's span in its lane
/// (W3 before a step to a quarter of a second after it; a hold or a roll
/// from that long before its head to a quarter of a second past its end;
/// a lift from a second before it).
fn assert_margins(song: &Song, chart: usize, table: &ddi_engine::rules::JudgeTable, cells: &[Hazard]) {
    let c = &song.charts[chart];
    let timing = c.timing(song);
    let Clearance::Margins {
        after_note,
        lift_ready,
    } = PLACEMENT_V1.clear
    else {
        panic!("margins");
    };
    assert_eq!((after_note, lift_ready), (0.25, 1.0));
    let w3 = table.window(Judgement::W3).unwrap().early;
    for h in cells {
        let m = timing.seconds_at(h.tick);
        for n in c.notes.iter().filter(|n| n.lane == h.lane) {
            let at = timing.seconds_at(n.tick);
            let (from, to) = match n.kind {
                NoteKind::Tap => (at - w3, at),
                NoteKind::Lift => (at - lift_ready, at),
                NoteKind::HoldHead { end } | NoteKind::RollHead { end } => {
                    (at - w3, timing.seconds_at(end))
                }
                NoteKind::Mine | NoteKind::Shock => {
                    assert_ne!(n.tick, h.tick, "on a mine");
                    continue;
                }
                _ => continue,
            };
            assert!(
                m < from || m > to + after_note,
                "a shock at {m:.3} s in lane {} within {from:.3}..{:.3} of {:?}",
                h.lane,
                to + after_note,
                n.kind
            );
        }
    }
}

fn ddi_rules(difficulty: ddi_engine::rules::ddi::Difficulty) -> Ruleset {
    ddi_engine::rules::ddi::ruleset(&ddi_engine::rules::ddi::DdiOptions {
        difficulty,
        ..Default::default()
    })
}

/// A chart at 120 BPM (a beat half a second) with a sixteenth stream on
/// lanes 0 and 1, a hold then a roll on lane 2 and lifts on lane 3.
fn busy_song() -> Song {
    let t = |sixteenths: i64| Tick(sixteenths * TICKS_PER_BEAT / 4);
    let mut notes = Vec::new();
    // Lanes 0 and 1: a sixteenth stream over the first 16 beats, each
    // lane stepped every eighth (a quarter of a second).
    for i in 0..64 {
        notes.push(Note::new(t(i), (i % 2) as u8, NoteKind::Tap));
    }
    // Lane 2: a hold over beats 2 to 6, a roll over beats 9 to 11.
    notes.push(Note::new(t(8), 2, NoteKind::HoldHead { end: t(24) }));
    notes.push(Note::new(t(36), 2, NoteKind::RollHead { end: t(44) }));
    // Lane 3: lifts at beats 5 and 13.
    notes.push(Note::new(t(20), 3, NoteKind::Lift));
    notes.push(Note::new(t(52), 3, NoteKind::Lift));
    // A last step far on, so that the chart has sections to its end.
    notes.push(Note::new(t(128), 0, NoteKind::Tap));
    notes.sort_by_key(|n| (n.tick, n.lane));
    Song::generated(
        "Busy",
        TimingMap::constant(120.0, 0.0),
        Chart::plain("dance-single", Difficulty::Challenge, 12, notes),
        "test",
    )
}

/// `placement-v1` on a chart with a stream, a hold, a roll and lifts: no
/// shock in the stream's lanes while it runs (a step every quarter of a
/// second leaves no room between a quarter of a second after one and W3
/// before the next), none during the hold or the roll nor just after
/// them, none from a second before a lift; a chart that is all stream
/// has no section at all (skipped, never filled with unsafe places); and
/// what it places, an exact play avoids.
#[test]
fn placement_keeps_its_margins_about_streams_holds_rolls_and_lifts() {
    use ddi_engine::rules::ddi::Difficulty as D;
    let song = busy_song();
    let c = &song.charts[0];
    let timing = c.timing(&song);
    for difficulty in D::ALL {
        let rules = ddi_rules(difficulty);
        for seed in 0..24 {
            let s = schedule(&song, 0, &rules.judge, seed);
            assert_eq!(s, schedule(&song, 0, &rules.judge, seed), "pure");
            assert!(s.iter().flatten().count() >= 2, "{difficulty:?} {seed}");
            for sec in s.iter().flatten() {
                assert_eq!(sec.cells.len(), PLACEMENT_V1.mines);
                assert_margins(&song, 0, &rules.judge, &sec.cells);
                for h in &sec.cells {
                    let m = timing.seconds_at(h.tick);
                    // The stream runs to 8 s (and its last steps' quarter
                    // of a second): nothing in its lanes from their first
                    // step on (lane 1's is a sixteenth in: a shock on the
                    // chart's first row is W3 clear of it).
                    assert!(
                        h.lane >= 2 || m > 8.0 || (h.lane == 1 && m <= 0.0),
                        "in the stream: {h:?}"
                    );
                    // The hold (1 s to 3 s) and the roll (4.5 s to 5.5 s).
                    assert!(
                        h.lane != 2 || !((0.9..=3.25).contains(&m) || (4.4..=5.75).contains(&m)),
                        "{h:?}"
                    );
                    // The lifts at 2.5 s and 6.5 s, their keys ready.
                    assert!(
                        h.lane != 3 || !((1.5..=2.75).contains(&m) || (5.5..=6.75).contains(&m)),
                        "{h:?}"
                    );
                }
            }
        }
    }
    // The older placement asked for more room both ways of a step.
    let rules = ddi_rules(D::Normal);
    let v0 = schedule_with(&song, 0, &rules.judge, 7, &PLACEMENT_V0);
    let v1 = schedule(&song, 0, &rules.judge, 7);
    assert_eq!(v0.len(), v1.len());
    assert!(v0.iter().flatten().count() <= v1.iter().flatten().count());
}

/// A chart at 140 BPM with room beside its notes: quarter steps turning
/// through the lanes with a jump every other bar, eighth runs, a hold, a
/// roll and lifts, over 48 beats.
fn mixed_song() -> Song {
    let t = |eighths: i64| Tick(eighths * TICKS_PER_BEAT / 2);
    let mut notes = Vec::new();
    for beat in 0..48i64 {
        let bar = beat / 4;
        let lane = ((beat * 3 + bar) % 4) as u8;
        match (bar % 6, beat % 4) {
            // A hold over two beats, then a roll, each in its bar.
            (2, 0) => notes.push(Note::new(t(beat * 2), lane, NoteKind::HoldHead { end: t(beat * 2 + 4) })),
            (2, 1) | (2, 2) => {}
            (4, 0) => notes.push(Note::new(t(beat * 2), lane, NoteKind::RollHead { end: t(beat * 2 + 3) })),
            (4, 1) => {}
            // A lift at the bar's end.
            (5, 3) => notes.push(Note::new(t(beat * 2), lane, NoteKind::Lift)),
            // An eighth run.
            (3, _) => {
                notes.push(Note::new(t(beat * 2), lane, NoteKind::Tap));
                notes.push(Note::new(t(beat * 2 + 1), (lane + 1) % 4, NoteKind::Tap));
            }
            // A jump on the first beat of every other bar.
            (0, 0) => {
                notes.push(Note::new(t(beat * 2), lane, NoteKind::Tap));
                notes.push(Note::new(t(beat * 2), (lane + 2) % 4, NoteKind::Tap));
            }
            _ => notes.push(Note::new(t(beat * 2), lane, NoteKind::Tap)),
        }
    }
    // A last step far on, so that the chart has sections to its end.
    notes.push(Note::new(t(128), 0, NoteKind::Tap));
    notes.sort_by_key(|n| (n.tick, n.lane));
    Song::generated(
        "Mixed",
        TimingMap::constant(140.0, 0.0),
        Chart::plain("dance-single", Difficulty::Hard, 9, notes),
        "test",
    )
}

/// `placement-v2` (the maintainer: "prioritize adding some of them to
/// lanes which already have a note nearby"): v1's margins exactly, on a
/// chart with steps, jumps, eighth runs, a hold, a roll and lifts; most
/// of a section's shocks next to a note of their lane where v1 put few;
/// never two in a row, at most two a beat, never two of a lane within
/// the lock's second where the section has other room; no section v1
/// has lost; the same shocks for the same chart, section and seed.
#[test]
fn placement_v2_aims_most_shocks_next_to_a_note_of_their_lane() {
    use ddi_engine::rules::ddi::Difficulty as D;
    for (name, song) in [("mixed", mixed_song()), ("busy", busy_song())] {
        let c = &song.charts[0];
        let timing = c.timing(&song);
        for difficulty in D::ALL {
            let rules = ddi_rules(difficulty);
            let (mut near_v2, mut near_v1, mut total, mut close) = (0, 0, 0, 0);
            // The sections wholly inside the chart's pattern (its last
            // bars are empty but for a far last step).
            let (mut near_in, mut total_in, mut least_in) = (0, 0, usize::MAX);
            for seed in 0..24 {
                let v2 = schedule_with(&song, 0, &rules.judge, seed, &PLACEMENT_V2);
                assert_eq!(v2, schedule_with(&song, 0, &rules.judge, seed, &PLACEMENT_V2), "pure");
                assert_eq!(v2, schedule(&song, 0, &rules.judge, seed), "what attacks use");
                let v1 = schedule_with(&song, 0, &rules.judge, seed, &PLACEMENT_V1);
                assert_eq!(v1.len(), v2.len());
                for (a, b) in v1.iter().zip(&v2) {
                    assert!(a.is_none() || b.is_some(), "a section v1 has is lost");
                    if let Some(a) = a {
                        near_v1 += beside_count(&song, 0, &rules.judge, &a.cells, &PLACEMENT_V2);
                    }
                    let Some(sec) = b else { continue };
                    assert_eq!(sec.cells.len(), PLACEMENT_V2.mines);
                    assert_margins(&song, 0, &rules.judge, &sec.cells);
                    let first = i64::from(sec.start_beat) * TICKS_PER_BEAT;
                    let mut per_beat = [0; 8];
                    for (k, h) in sec.cells.iter().enumerate() {
                        per_beat[((h.tick.0 - first) / TICKS_PER_BEAT) as usize] += 1;
                        for o in &sec.cells[k + 1..] {
                            assert_ne!(o.tick, h.tick, "one a row");
                            let gap = (timing.seconds_at(o.tick) - timing.seconds_at(h.tick)).abs();
                            if o.lane == h.lane && gap < 1.0 {
                                close += 1;
                            }
                        }
                    }
                    assert!(per_beat.iter().all(|&b| b <= 2), "{per_beat:?}");
                    let near = beside_count(&song, 0, &rules.judge, &sec.cells, &PLACEMENT_V2);
                    near_v2 += near;
                    total += sec.cells.len();
                    if sec.start_beat + 8 <= 48 {
                        near_in += near;
                        total_in += sec.cells.len();
                        least_in = least_in.min(near);
                    }
                }
            }
            println!(
                "{name} {difficulty:?}: next to a note v2 {near_v2} of {total} ({:.0}%), v1 {near_v1}; inside the pattern {near_in} of {total_in} ({:.0}%), the least in a section {least_in}; two of a lane within a second: {close}",
                100.0 * near_v2 as f64 / total.max(1) as f64,
                100.0 * near_in as f64 / total_in.max(1) as f64
            );
            assert!(total > 0, "{name} {difficulty:?}");
            assert!(near_v2 > near_v1, "{name} {difficulty:?}: {near_v2} {near_v1}");
            if name == "mixed" {
                // Room beside its notes throughout its pattern: most of
                // the eight (five of them and more on average), never
                // fewer than half in a section.
                assert!(near_in * 8 >= total_in * 5, "{difficulty:?}: {near_in} of {total_in}");
                assert!(least_in >= 4, "{difficulty:?}: {least_in}");
                assert_eq!(close, 0, "{difficulty:?}: a lane's shocks a second apart");
            }
        }
    }
}

/// What the placement puts into a play, an exact play avoids: every step
/// pressed on time and its key kept down a fifth of a second (a hold's to
/// its end, a roll tapped, a lift's key down half a second ahead), with
/// every other section's shocks in the play, steps on none of them, under
/// every difficulty; and a chart that is all stream has no section.
#[test]
fn what_the_placement_places_an_exact_play_avoids() {
    use ddi_engine::rules::ddi::Difficulty as D;
    for song in [busy_song(), mixed_song()] {
    let c = &song.charts[0];
    let timing = c.timing(&song);
    for difficulty in D::ALL {
        for seed in [1u64, 7, 23] {
            let rules = ddi_rules(difficulty);
            let s = schedule(&song, 0, &rules.judge, seed);
            let mut p = Player::new(&song, 0, rules, PlayOptions::default());
            p.start(0.0, 0.0);
            // Sections that do not overlap: every other slot.
            let mut put = 0;
            for sec in s.iter().step_by(2).flatten() {
                p.add_hazards(&sec.cells).unwrap();
                put += sec.cells.len();
            }
            assert!(put >= 8, "{difficulty:?} {seed}: {put}");
            let mut edges: Vec<(f64, u8, bool)> = Vec::new();
            for n in &c.notes {
                let at = timing.seconds_at(n.tick);
                match n.kind {
                    NoteKind::Tap => edges.extend([(at, n.lane, true), (at + 0.2, n.lane, false)]),
                    NoteKind::Lift => {
                        edges.extend([(at - 0.5, n.lane, true), (at, n.lane, false)])
                    }
                    NoteKind::HoldHead { end } => edges.extend([
                        (at, n.lane, true),
                        (timing.seconds_at(end) + 0.2, n.lane, false),
                    ]),
                    NoteKind::RollHead { end } => {
                        let end = timing.seconds_at(end);
                        let mut t = at;
                        while t < end {
                            edges.extend([(t, n.lane, true), (t + 0.05, n.lane, false)]);
                            t += 0.2;
                        }
                    }
                    _ => {}
                }
            }
            edges.sort_by(|a, b| a.0.total_cmp(&b.0).then(a.2.cmp(&b.2)));
            // The stream's lanes' keys go up before their next press.
            let mut down = [false; 4];
            let mut t = -1.0;
            let mut next = 0;
            while !p.finished() && t < 80.0 {
                t += 1.0 / 60.0;
                while next < edges.len() && edges[next].0 <= t {
                    let (at, lane, pressed) = edges[next];
                    next += 1;
                    // A key already down is let go first (the stream's
                    // steps are a quarter of a second apart).
                    if pressed && down[usize::from(lane)] {
                        p.input_at(lane, false, at - 0.001);
                    }
                    if pressed || down[usize::from(lane)] {
                        p.input_at(lane, pressed, at);
                    }
                    down[usize::from(lane)] = pressed;
                }
                p.update_at(t);
            }
            let r = p.results();
            assert_eq!(
                r.battle_mines.map(|b| b.hit),
                Some(0),
                "{difficulty:?} {seed}: {:?}",
                p.hazards().unwrap().mines()
            );
            assert_eq!(r.tally.mine_hit, 0);
            assert_eq!(r.tally.count(Judgement::Miss), 0, "{:?}", r.tally);
        }
    }
    }
    // A chart that is all stream (every lane stepped every quarter of a
    // second): no section anywhere.
    let mut notes: Vec<Note> = Vec::new();
    for i in 0..128i64 {
        for lane in 0..4u8 {
            if (i + i64::from(lane)) % 2 == 0 {
                notes.push(Note::new(Tick(i * TICKS_PER_BEAT / 2), lane, NoteKind::Tap));
            }
        }
    }
    // Every lane every eighth at 240 BPM would be jumps throughout; here
    // each lane is stepped every other eighth at 120 BPM, half a second
    // apart: room between. Twice as dense leaves none.
    let dense: Vec<Note> = (0..256i64)
        .flat_map(|i| {
            (0..4u8)
                .filter(move |lane| (i + i64::from(*lane)) % 2 == 0)
                .map(move |lane| Note::new(Tick(i * TICKS_PER_BEAT / 4), lane, NoteKind::Tap))
        })
        .collect();
    let chart = |notes: Vec<Note>| {
        Song::generated(
            "Stream",
            TimingMap::constant(120.0, 0.0),
            Chart::plain("dance-single", Difficulty::Challenge, 15, notes),
            "test",
        )
    };
    let rules = ddi_rules(D::Normal);
    let sparse = chart(notes);
    assert!(schedule(&sparse, 0, &rules.judge, 3).iter().flatten().count() > 0);
    let dense = chart(dense);
    let s = schedule(&dense, 0, &rules.judge, 3);
    assert!(!s.is_empty() && s.iter().all(Option::is_none), "skipped");
}

/// Every bundled four-lane chart: the sections the schedule finds keep
/// the placement's rules (eight shocks, each beat at most two, one a
/// row; `placement-v1`'s margins, which `placement-v2` keeps, under DDI's
/// three difficulties, most of v2's shocks next to a note of their lane,
/// `placement-v0`'s clearances under the rulesets rooms played before),
/// the schedule is the same for the same seed, and each chart's share of
/// sections is printed (`-- --nocapture`).
#[test]
fn bundled_charts_have_safe_sections() {
    use ddi_engine::rules::ddi::Difficulty as D;
    let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../assets/songs");
    let mut ids: Vec<_> = std::fs::read_dir(&dir)
        .unwrap()
        .flatten()
        .filter(|e| e.path().is_dir())
        .collect();
    ids.sort_by_key(|e| e.file_name());
    let mut charts = 0;
    let (mut found_v1, mut found_v0, mut slots) = (0, 0, 0);
    // `placement-v2` against v1 under DDI Normal: the sections each
    // finds, their shocks next to a note of their lane, and the pairs of
    // one lane's shocks within the lock's second.
    let (mut found_v2, mut near_v2, mut near_v1, mut close_v2, mut close_v1) = (0, 0, 0, 0, 0);
    let close = |song: &Song, i: usize, cells: &[Hazard]| {
        let timing = song.charts[i].timing(song);
        let mut n = 0;
        for (k, h) in cells.iter().enumerate() {
            for o in &cells[k + 1..] {
                let gap = (timing.seconds_at(o.tick) - timing.seconds_at(h.tick)).abs();
                if o.lane == h.lane && gap < 1.0 {
                    n += 1;
                }
            }
        }
        n
    };
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
        let Ok(song) =
            ddi_library::scan::parse_simfile(&std::fs::read_to_string(&path).unwrap(), &ext)
        else {
            continue;
        };
        for (i, c) in song.charts.iter().enumerate() {
            let Some(layout) = song.layout_of(c) else {
                continue;
            };
            if layout.lane_count() != 4 || layout.players != 1 || c.derivation.is_some() {
                continue;
            }
            charts += 1;
            let mut line = format!(
                "{} [{i}] {:?} {}:",
                e.file_name().to_string_lossy(),
                c.difficulty,
                c.meter
            );
            let shape = |sec: &ddi_engine::battle::Section| {
                assert_eq!(sec.cells.len(), PLACEMENT.mines);
                let first = i64::from(sec.start_beat) * TICKS_PER_BEAT;
                let mut per_beat = [0; 8];
                for (k, h) in sec.cells.iter().enumerate() {
                    assert!(h.tick.0 >= first && h.tick.0 < first + 8 * TICKS_PER_BEAT);
                    per_beat[((h.tick.0 - first) / TICKS_PER_BEAT) as usize] += 1;
                    assert!(
                        sec.cells[k + 1..].iter().all(|o| o.tick != h.tick),
                        "one a row"
                    );
                }
                assert!(per_beat.iter().all(|&b| b <= 2), "{per_beat:?}");
            };
            // DDI, the rooms' ruleset: the margins.
            for (name, difficulty) in [("easy", D::Easy), ("normal", D::Normal), ("hard", D::Hard)]
            {
                let r = ddi_rules(difficulty);
                let s = schedule(&song, i, &r.judge, 7);
                assert_eq!(s, schedule(&song, i, &r.judge, 7), "pure");
                for sec in s.iter().flatten() {
                    shape(sec);
                    assert_margins(&song, i, &r.judge, &sec.cells);
                }
                let found = s.iter().flatten().count();
                if difficulty == D::Normal {
                    found_v2 += found;
                    slots += s.len();
                    found_v0 += schedule_with(&song, i, &r.judge, 7, &PLACEMENT_V0)
                        .iter()
                        .flatten()
                        .count();
                    let v1 = schedule_with(&song, i, &r.judge, 7, &PLACEMENT_V1);
                    found_v1 += v1.iter().flatten().count();
                    for (a, b) in v1.iter().zip(&s) {
                        assert!(a.is_none() || b.is_some(), "a section v1 has is lost");
                        if let Some(a) = a {
                            assert_margins(&song, i, &r.judge, &a.cells);
                            near_v1 += beside_count(&song, i, &r.judge, &a.cells, &PLACEMENT_V2);
                            close_v1 += close(&song, i, &a.cells);
                        }
                        if let Some(b) = b {
                            near_v2 += beside_count(&song, i, &r.judge, &b.cells, &PLACEMENT_V2);
                            close_v2 += close(&song, i, &b.cells);
                        }
                    }
                }
                line += &format!(
                    " ddi-{name} {found}/{} ({:.0}%)",
                    s.len(),
                    100.0 * found as f64 / s.len().max(1) as f64
                );
            }
            // The older placement, under the rulesets rooms played before.
            for id in ["itg-fa+", "ddr-a", "ffr"] {
                let r = rules(id, &song);
                let s = schedule_with(&song, i, &r.judge, 7, &PLACEMENT_V0);
                let timing = c.timing(&song);
                let window = INJECTED_MINE_V0.window(&r.judge);
                let clear = window + r.judge.max_early().max(r.judge.miss_after());
                for sec in s.iter().flatten() {
                    shape(sec);
                    for h in &sec.cells {
                        let m = timing.seconds_at(h.tick);
                        for n in c.notes.iter().filter(|n| n.lane == h.lane) {
                            let at = timing.seconds_at(n.tick);
                            match n.kind {
                                NoteKind::Tap | NoteKind::Lift => assert!((at - m).abs() > clear),
                                NoteKind::HoldHead { end } | NoteKind::RollHead { end } => {
                                    let end = timing.seconds_at(end);
                                    assert!((at - m).abs() > clear);
                                    assert!(end < m - window || at > m + window, "a body across");
                                }
                                _ => {}
                            }
                        }
                    }
                }
            }
            println!("SAFE {line}");
        }
    }
    println!(
        "SAFE all: placement-v2 {found_v2}/{slots} ({:.1}%), placement-v1 {found_v1}/{slots}, placement-v0 under DDI Normal {found_v0}/{slots} ({:.1}%)",
        100.0 * found_v2 as f64 / slots.max(1) as f64,
        100.0 * found_v0 as f64 / slots.max(1) as f64
    );
    println!(
        "NEAR all (DDI Normal): next to a note of their lane, placement-v2 {near_v2} of {} shocks ({:.1}%), placement-v1 {near_v1} of {} ({:.1}%); two of a lane within a second: v2 {close_v2}, v1 {close_v1}",
        found_v2 * 8,
        100.0 * near_v2 as f64 / (found_v2 * 8).max(1) as f64,
        found_v1 * 8,
        100.0 * near_v1 as f64 / (found_v1 * 8).max(1) as f64
    );
    // Most of what v2 places on real charts is next to a note; v1 put
    // few there.
    assert!(near_v2 * 2 > found_v2 * 8, "{near_v2} of {}", found_v2 * 8);
    assert!(near_v2 > 2 * near_v1, "{near_v2} {near_v1}");
    assert!(charts > 10, "{charts}");
}

#[test]
fn judgement_order_is_the_engine_s() {
    // The ledger's classes count tiers in this order.
    assert_eq!(Judgement::W1.index(), 0);
    assert_eq!(Judgement::Miss.index(), 5);
}
