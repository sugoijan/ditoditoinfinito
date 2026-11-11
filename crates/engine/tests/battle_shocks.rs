//! A battle's shocks in a play (`ddi_engine::battle`, M8a): judged apart
//! from the chart, a hit costing the ruleset's miss to the gauge and
//! nothing else (D1), the combo broken only where the ruleset's own mines
//! break it (D3), drawn as mines, kept in the log (version 4) and judged
//! alike again from it; and where charts have room for them
//! (`schedule`, `PLACEMENT_V0`).
//!
//! Chart (60 BPM, so beat `b` is at `b` seconds), dance-single: taps at
//! beats 0 (lane 0), 2 (lane 3), 8 (lane 0) and, in the twin chart, a tap
//! at beat 5 lane 2 in place of the shock there. Shocks at beat 4 lane 1
//! (stepped on by a press), 5 lane 2 (crossed while held) and 6 lane 3
//! (passed).

use ddi_chart::{Chart, Difficulty, Note, NoteKind, Song, TICKS_PER_BEAT, Tick, TimingMap};
use ddi_engine::battle::{
    HAZARD_INDEX_BASE, Hazard, HazardError, INJECTED_MINE_V0, PLACEMENT_V0, schedule,
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

/// Every bundled four-lane chart: the sections the schedule finds keep
/// `PLACEMENT_V0`'s rules (eight shocks, each beat at most two, one a
/// row, clear of every step, hold and tail in their lane), the schedule
/// is the same for the same seed, and each chart's share of sections is
/// printed (`-- --nocapture`).
#[test]
fn bundled_charts_have_safe_sections() {
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
            for id in ["itg-fa+", "ddr-a", "ffr"] {
                let r = rules(id, &song);
                let s = schedule(&song, i, &r.judge, 7);
                assert_eq!(s, schedule(&song, i, &r.judge, 7), "pure");
                let timing = c.timing(&song);
                let window = INJECTED_MINE_V0.window(&r.judge);
                let clear = window + r.judge.max_early().max(r.judge.miss_after());
                for sec in s.iter().flatten() {
                    assert_eq!(sec.cells.len(), PLACEMENT_V0.mines);
                    let first = i64::from(sec.start_beat) * TICKS_PER_BEAT;
                    let mut per_beat = [0; 8];
                    for (k, h) in sec.cells.iter().enumerate() {
                        assert!(h.tick.0 >= first && h.tick.0 < first + 8 * TICKS_PER_BEAT);
                        per_beat[((h.tick.0 - first) / TICKS_PER_BEAT) as usize] += 1;
                        assert!(
                            sec.cells[k + 1..].iter().all(|o| o.tick != h.tick),
                            "one a row"
                        );
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
                    assert!(per_beat.iter().all(|&b| b <= 2), "{per_beat:?}");
                }
                let found = s.iter().flatten().count();
                line += &format!(
                    " {id} {found}/{} ({:.0}%)",
                    s.len(),
                    100.0 * found as f64 / s.len().max(1) as f64
                );
            }
            println!("SAFE {line}");
        }
    }
    assert!(charts > 10, "{charts}");
}

#[test]
fn judgement_order_is_the_engine_s() {
    // The ledger's classes count tiers in this order.
    assert_eq!(Judgement::W1.index(), 0);
    assert_eq!(Judgement::Miss.index(), 5);
}
