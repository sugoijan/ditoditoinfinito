use super::*;
use crate::cpu::{LEVELS, Opponent, Plan};
use crate::rules::ddi::{self, DdiOptions};
use crate::rules::{FailPolicy, Ruleset, Tally};
use ddi_chart::{Note, TimingMap};

const BEAT: i64 = 48;

fn beatucada() -> Song {
    let path =
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../assets/songs/beatucada");
    let path = std::fs::read_dir(path)
        .unwrap()
        .flatten()
        .map(|e| e.path())
        .find(|p| p.extension().is_some_and(|e| e == "ssc"))
        .unwrap();
    ddi_library::scan::parse_simfile(&std::fs::read_to_string(path).unwrap(), "ssc").unwrap()
}

/// Beatucada's chart 1 replaced by `notes` at 120 BPM (a beat half a
/// second).
fn song_with(mut notes: Vec<Note>) -> Song {
    let mut s = beatucada();
    notes.sort_by_key(|n| (n.tick, n.lane));
    s.charts[1].timing = Some(TimingMap::constant(120.0, 0.0));
    s.charts[1].notes = notes;
    s
}

fn rules() -> Ruleset {
    let mut r = ddi::ruleset(&DdiOptions::default());
    r.fail = FailPolicy::Off;
    r
}

/// A CPU at `level` (`cpu-levels-v1`) playing the chart whole.
fn opponent(song: &Song, level: usize, seed: u64, skills: Skills) -> Opponent {
    let plan = Plan::automatic(song, 1).unwrap();
    let strength = crate::cpu::CPU_LEVELS_V1.level(level).unwrap();
    let o = Opponent::planned_as(
        &plan,
        song,
        1,
        rules(),
        (strength, crate::cpu::MINE_RESPONSE_V0.avoidance(level)),
        seed,
        skills,
        CpuPlay::FullV1,
    )
    .unwrap();
    assert_eq!(o.play(), CpuPlay::FullV1);
    o
}

fn played(song: &Song, level: usize, seed: u64) -> Opponent {
    let mut o = opponent(song, level, seed, Skills::default());
    o.start(0.0);
    o.finish();
    o
}

fn tally(song: &Song, level: usize, seeds: std::ops::RangeInclusive<u64>) -> Tally {
    let mut t = Tally::default();
    for seed in seeds {
        let o = played(song, level, seed);
        validate_full_script(o.presses()).unwrap();
        let r = o.player().tally();
        for (a, b) in t.taps.iter_mut().zip(r.taps) {
            *a += b;
        }
        t.held += r.held;
        t.let_go += r.let_go;
        t.mine_hit += r.mine_hit;
        t.boo += r.boo;
    }
    t
}

fn tap(beat: f64, lane: u8) -> Note {
    Note::new(Tick((beat * BEAT as f64).round() as i64), lane, NoteKind::Tap)
}

fn hold(beat: f64, beats: f64, lane: u8) -> Note {
    let at = (beat * BEAT as f64).round() as i64;
    let end = Tick(at + (beats * BEAT as f64).round() as i64);
    Note::new(Tick(at), lane, NoteKind::HoldHead { end })
}

fn roll(beat: f64, beats: f64, lane: u8) -> Note {
    let at = (beat * BEAT as f64).round() as i64;
    let end = Tick(at + (beats * BEAT as f64).round() as i64);
    Note::new(Tick(at), lane, NoteKind::RollHead { end })
}

fn mine(beat: f64, lane: u8) -> Note {
    Note::new(Tick((beat * BEAT as f64).round() as i64), lane, NoteKind::Mine)
}

/// Holds of two beats in lane 0 every four beats, quarter notes in lanes
/// 2 and 3 between (two a second).
fn quiet_holds() -> Song {
    let mut notes = Vec::new();
    for k in 0..16 {
        let b = 4.0 + 4.0 * f64::from(k);
        notes.push(hold(b, 2.0, 0));
        for q in 0..4 {
            notes.push(tap(b + f64::from(q), 2 + (q % 2) as u8));
        }
    }
    song_with(notes)
}

/// Holds of four beats in lane 0 under a stream of `per_beat` notes a
/// beat in lanes 1 to 3 (twice that a second).
fn busy_holds(per_beat: u32) -> Song {
    let mut notes = Vec::new();
    for k in 0..12 {
        let b = 4.0 + 6.0 * f64::from(k);
        notes.push(hold(b, 4.0, 0));
        for s in 1..4 * per_beat {
            notes.push(tap(b + f64::from(s) / f64::from(per_beat), 1 + (s % 3) as u8));
        }
    }
    song_with(notes)
}

#[test]
fn holds_are_kept_down_to_their_ends_where_nothing_is_hard() {
    let song = quiet_holds();
    let mut kept = Vec::new();
    for level in [0, 4, 9] {
        let t = tally(&song, level, 1..=8);
        // Every head hit is held or let go.
        assert!(t.held + t.let_go + t.taps[5] >= 16 * 8, "{level}: {t:?}");
        kept.push(f64::from(t.held) / f64::from(t.held + t.let_go));
    }
    // Nearly all kept where the level keeps up (the weakest is at its
    // limit here: two notes a second and the holds).
    assert!(kept[1] >= 0.95 && kept[2] >= 0.99, "{kept:?}");
    assert!(kept[0] <= kept[1], "{kept:?}");
    // The key is down from the head to just past the end.
    let o = played(&song, 9, 1);
    let first = o.presses().iter().find(|p| p.lane == 0).unwrap();
    assert!(first.release - first.press > 0.9, "{first:?}");
}

#[test]
fn holds_under_a_busy_stream_are_let_go_more_the_weaker_the_level() {
    let song = busy_holds(2);
    let dropped = |level: usize| {
        let t = tally(&song, level, 1..=16);
        f64::from(t.let_go) / f64::from((t.held + t.let_go).max(1))
    };
    let (weak, middle, strong) = (dropped(0), dropped(4), dropped(9));
    assert!(weak > middle && middle > strong, "{weak} {middle} {strong}");
    assert!(strong < 0.05, "{strong}");
    assert!(weak > 0.3, "{weak}");
    // Sixteenths: the strongest lets go of some too.
    let song16 = busy_holds(4);
    let t = tally(&song16, 9, 1..=16);
    assert!(t.let_go > 0, "{t:?}");
    // The tap-only copy has no holds to keep.
    let plan = Plan::automatic(&song, 1).unwrap();
    let level = crate::cpu::CPU_LEVELS_V1.level(4).unwrap();
    let mut o = Opponent::planned_as(
        &plan,
        &song,
        1,
        rules(),
        (level, 0.0),
        1,
        Skills::default(),
        CpuPlay::TapsV0,
    )
    .unwrap();
    o.start(0.0);
    o.finish();
    assert_eq!((o.player().tally().held, o.player().tally().let_go), (0, 0));
}

#[test]
fn a_row_played_while_a_hold_is_down_asks_more() {
    let song = busy_holds(4);
    let plan = FullPlan::new(&song, 1).unwrap();
    let held: Vec<u8> = plan.demands().iter().map(|d| d.held).collect();
    assert!(held.iter().filter(|&&h| h == 1).count() >= 12 * 14, "{held:?}");
    assert_eq!(held[0], 0);
    let a = crate::cpu::CPU_LEVELS_V1.level(4).unwrap();
    let s = Skills::default();
    let mut d = plan.demands()[5];
    d.held = 0;
    let free = loads(&a, &s, &d, &ABSOLUTE_V1).0;
    d.held = 2;
    let busy = loads(&a, &s, &d, &ABSOLUTE_V1).0;
    assert!(busy > free * 1.4, "{free} {busy}");
}

#[test]
fn rolls_are_re_tapped_while_they_last() {
    let mut notes = Vec::new();
    for k in 0..12 {
        let b = 4.0 + 6.0 * f64::from(k);
        notes.push(roll(b, 4.0, 1));
        notes.push(tap(b + 1.0, 3));
        notes.push(tap(b + 3.0, 2));
    }
    let song = song_with(notes);
    let t = tally(&song, 9, 1..=8);
    assert!(t.held >= 12 * 8 - 2, "{t:?}");
    // Two seconds each, a tap every 0.12 s.
    let o = played(&song, 9, 3);
    let taps = o.presses().iter().filter(|p| p.lane == 1).count();
    assert!(taps >= 12 * 15, "{taps}");
    assert_eq!(o.player().tally().boo, 0);
    // A weaker level loses more of them.
    let weak = tally(&song, 0, 1..=8);
    assert!(weak.let_go >= t.let_go, "{weak:?} {t:?}");
}

#[test]
fn lifts_are_released_on_their_time() {
    let notes: Vec<Note> = (0..48)
        .map(|k| {
            Note::new(
                Tick(4 * BEAT + k * BEAT),
                (k % 4) as u8,
                if k % 2 == 0 { NoteKind::Lift } else { NoteKind::Tap },
            )
        })
        .collect();
    let song = song_with(notes);
    let t = tally(&song, 9, 1..=4);
    // Every lift and tap judged, none missed.
    assert_eq!(t.taps.iter().sum::<u32>(), 48 * 4, "{t:?}");
    assert!(t.taps[5] <= 1, "{t:?}");
    assert_eq!(t.boo, 0);
}

#[test]
fn a_mine_with_no_note_of_its_lane_beside_it_costs_nothing() {
    let mut notes = Vec::new();
    for k in 0..48 {
        notes.push(tap(4.0 + f64::from(k) / 2.0, (k % 2) as u8));
        notes.push(mine(4.25 + f64::from(k) / 2.0, 2 + (k % 2) as u8));
    }
    let song = song_with(notes);
    for level in 0..LEVELS {
        assert_eq!(tally(&song, level, 1..=4).mine_hit, 0, "{level}");
    }
}

#[test]
fn mines_beside_notes_are_stepped_on_more_by_weaker_levels() {
    let mut notes = Vec::new();
    for k in 0..48 {
        let b = 4.0 + f64::from(k);
        let lane = (k % 4) as u8;
        notes.push(mine(b - 0.5, lane));
        notes.push(tap(b, lane));
    }
    let song = song_with(notes);
    let hits = |level| tally(&song, level, 1..=16).mine_hit;
    let (weak, strong) = (hits(0), hits(9));
    assert!(weak > strong, "{weak} {strong}");
    // About 30% and 2% of 768.
    assert!((120..400).contains(&weak), "{weak}");
    assert!(strong < 60, "{strong}");
    // Evasion bends it as it bends a shock's.
    let stepped = |e: f64| -> u32 {
        (1..=16)
            .map(|seed| {
                let mut o = opponent(&song, 4, seed, Skills::default().with_evasion(e).unwrap());
                o.start(0.0);
                o.finish();
                o.player().tally().mine_hit
            })
            .sum()
    };
    let (dodger, stomper) = (stepped(2.0), stepped(0.5));
    assert!(dodger < stomper, "{dodger} {stomper}");
}

#[test]
fn a_mine_inside_a_hold_is_let_go_around_or_stepped_on() {
    let mut notes = Vec::new();
    for k in 0..12 {
        let b = 4.0 + 8.0 * f64::from(k);
        notes.push(hold(b, 6.0, 0));
        notes.push(mine(b + 3.0, 0));
        notes.push(tap(b + 1.0, 2));
        notes.push(tap(b + 5.0, 3));
    }
    let song = song_with(notes);
    let strong = tally(&song, 9, 1..=8);
    let weak = tally(&song, 0, 1..=8);
    // The strong level lets go around nearly every one and keeps its holds.
    assert!(strong.mine_hit <= 6, "{strong:?}");
    assert!(strong.held >= 12 * 8 - 8, "{strong:?}");
    assert!(weak.mine_hit > strong.mine_hit, "{weak:?} {strong:?}");
    // Let go around it, its key is up at the mine and down again after.
    let o = played(&song, 9, 2);
    let lane0: Vec<_> = o.presses().iter().filter(|p| p.lane == 0).collect();
    assert!(lane0.len() >= 20, "{lane0:?}");
}

#[test]
fn the_same_presses_at_any_frame_rate_and_from_the_same_seed() {
    let song = busy_holds(4);
    let run = |step: f64| {
        let mut o = opponent(&song, 4, 5, Skills::default());
        o.start(0.0);
        let mut t = -2.0;
        while !o.finished() && t < 400.0 {
            t += step;
            o.advance(t);
        }
        o
    };
    let (a, b) = (run(1.0 / 60.0), run(1.0 / 144.0));
    assert_eq!(a.presses(), b.presses());
    assert_eq!(a.player().tally(), b.player().tally());
    assert_eq!(a.score(), b.score());
    assert_eq!(a.life(), b.life());
    let c = played(&song, 4, 5);
    assert_eq!(c.player().tally(), a.player().tally());
    assert_ne!(played(&song, 4, 6).presses(), a.presses());
}

#[test]
fn extreme_charts_stay_valid() {
    // Rows at one time, a hold of no length, a hold over everything, a
    // roll under a jack, lifts at once, mines everywhere.
    let mut notes = vec![
        hold(4.0, 0.0, 0),
        hold(5.0, 400.0, 3),
        roll(6.0, 8.0, 1),
        Note::new(Tick(7 * BEAT), 2, NoteKind::Lift),
    ];
    for k in 0..2000 {
        let b = 8.0 + f64::from(k) / 12.0;
        notes.push(tap(b, (k % 3) as u8));
        notes.push(mine(b + 1.0 / 24.0, 2));
    }
    let song = song_with(notes);
    for level in [0, 9] {
        for seed in 1..=3 {
            let o = played(&song, level, seed);
            validate_full_script(o.presses()).unwrap();
        }
    }
}
