//! Chart transforms: Turn (lane remapping) and Cut (note removal).
//!
//! Applied to a chart's notes before the judge sees them, so judging, the
//! score's note counts and the results all describe the transformed chart.
//! Ticks never change, so no note's judged time can change.
//!
//! The behaviour follows StepMania 5.1 (`5_1-new` at `825467b`):
//! `NoteDataUtil::TransformNoteData` applies removals first and turns last
//! (`src/NoteDataUtil.cpp` 3031–3038), the lane tables are those of
//! `GetTrackMapping`, and the cuts are `Little`, `RemoveHoldNotes` (after
//! `ChangeRollsToHolds`) and `RemoveSimultaneousNotes(1)`. The eighth-note
//! cut is DDR's TIMING CUT ON2, StepMania's `Little` rule at a finer grid.
//! See `docs/research/rules-ddr-itg-dwi.md` §4.

use ddi_chart::{Layout, Note, NoteKind, TICKS_PER_BEAT};
use serde::{Deserialize, Serialize};

use crate::lenient;

/// DDR "TURN" / StepMania turn mods.
#[derive(Copy, Clone, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Turn {
    #[default]
    Off,
    /// Lanes reversed (left ↔ right, down ↔ up in dance-single).
    Mirror,
    /// The chart turned a quarter counter-clockwise: up arrows become left.
    Left,
    /// The chart turned a quarter clockwise: up arrows become right.
    Right,
    /// A random lane permutation, fixed by the play's seed.
    Shuffle,
}

/// Which notes survive by their position in the beat.
#[derive(Copy, Clone, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum TimingCut {
    #[default]
    Off,
    /// Only notes on a beat (StepMania "Little", DDR TIMING CUT ON1).
    Quarters,
    /// Only notes on a beat or half beat (DDR TIMING CUT ON2).
    Eighths,
}

/// Chart transforms of one play.
#[derive(Copy, Clone, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(default)]
pub struct TransformOptions {
    #[serde(deserialize_with = "lenient")]
    pub turn: Turn,
    #[serde(deserialize_with = "lenient")]
    pub cut: TimingCut,
    /// At most one panel pressed at a time (StepMania "NoJumps").
    pub no_jumps: bool,
    /// Holds and rolls become taps (DDR FREEZE ARROW cut).
    pub no_holds: bool,
}

impl TransformOptions {
    /// Whether notes are removed or simplified, which makes the play an
    /// assist (DDR's "assist clear" lamp). Turns are not assists.
    pub fn is_assist(&self) -> bool {
        self.cut != TimingCut::Off || self.no_jumps || self.no_holds
    }
}

/// Transformed notes and the lane mapping used.
#[derive(Clone, Debug, PartialEq)]
pub struct Transformed {
    /// Sorted by `(tick, lane)`.
    pub notes: Vec<Note>,
    /// `take_from[new_lane] = old_lane`, as StepMania's `iTakeFromTrack`.
    pub take_from: Vec<u8>,
}

/// Applies `opts` to `notes` of a chart played on `layout`. `seed` fixes
/// the Shuffle permutation.
pub fn apply(notes: &[Note], layout: &Layout, opts: &TransformOptions, seed: u64) -> Transformed {
    let mut notes = notes.to_vec();
    match opts.cut {
        TimingCut::Off => {}
        TimingCut::Quarters => keep_grid(&mut notes, TICKS_PER_BEAT),
        TimingCut::Eighths => keep_grid(&mut notes, TICKS_PER_BEAT / 2),
    }
    if opts.no_holds {
        remove_holds(&mut notes);
    }
    if opts.no_jumps {
        remove_jumps(&mut notes);
    }
    let take_from = lane_map(layout, opts.turn, seed);
    let mut dest: Vec<u8> = (0..take_from.len() as u8).collect();
    for (new, &old) in take_from.iter().enumerate() {
        dest[usize::from(old)] = new as u8;
    }
    for n in &mut notes {
        if let Some(&d) = dest.get(usize::from(n.lane)) {
            n.lane = d;
        }
    }
    notes.sort_by_key(|n| (n.tick, n.lane));
    Transformed { notes, take_from }
}

/// Lanes of each shuffle group, in lane order, groups by first lane.
fn shuffle_groups(layout: &Layout) -> Vec<Vec<usize>> {
    let mut groups: Vec<(u8, Vec<usize>)> = Vec::new();
    for (i, lane) in layout.lanes.iter().enumerate() {
        match groups.iter_mut().find(|(g, _)| *g == lane.shuffle_group) {
            Some((_, lanes)) => lanes.push(i),
            None => groups.push((lane.shuffle_group, vec![i])),
        }
    }
    groups.into_iter().map(|(_, lanes)| lanes).collect()
}

/// `take_from[new_lane] = old_lane` for a turn.
///
/// Left and Right use the layout's table (StepMania `GetTrackMapping`) and
/// are the identity without one. Mirror and Shuffle work within each shuffle
/// group, as Dancing☆Onigiri's `applyMirror` / `applyRandom`
/// (`js/lib/dataLoader.js`); with a single group (every StepMania layout)
/// that is StepMania's full reversal and full shuffle.
pub fn lane_map(layout: &Layout, turn: Turn, seed: u64) -> Vec<u8> {
    let lanes = layout.lane_count();
    let identity: Vec<u8> = (0..lanes as u8).collect();
    match turn {
        Turn::Off => identity,
        Turn::Mirror => {
            let mut map = identity;
            for group in shuffle_groups(layout) {
                for (&to, &from) in group.iter().zip(group.iter().rev()) {
                    map[to] = from as u8;
                }
            }
            map
        }
        Turn::Left | Turn::Right => {
            let Some(left) = turn_table(layout) else {
                return identity;
            };
            if turn == Turn::Left {
                left.clone()
            } else {
                let mut right = vec![0u8; lanes];
                for (t, &from) in left.iter().enumerate() {
                    right[usize::from(from)] = t as u8;
                }
                right
            }
        }
        Turn::Shuffle => {
            let groups = shuffle_groups(layout);
            if groups.iter().all(|g| g.len() < 2) {
                return identity;
            }
            // Re-roll with the next seed while the result is the identity,
            // as StepMania does.
            let mut s = seed;
            loop {
                let mut map = identity.clone();
                let mut rng = SplitMix64(s);
                for group in &groups {
                    // Fisher-Yates over the group's lanes; with one group of
                    // every lane this is a plain shuffle of the field.
                    for i in (1..group.len()).rev() {
                        let j = (rng.next() % (i as u64 + 1)) as usize;
                        map.swap(group[i], group[j]);
                    }
                }
                if map != identity {
                    return map;
                }
                s = s.wrapping_add(1);
            }
        }
    }
}

/// The layout's Left table, if it is a permutation of its lanes (a
/// song-defined layout may carry anything).
fn turn_table(layout: &Layout) -> Option<&Vec<u8>> {
    let t = layout.turn_left.as_ref()?;
    let mut sorted = t.clone();
    sorted.sort_unstable();
    sorted
        .iter()
        .copied()
        .eq(0..layout.lane_count() as u8)
        .then_some(t)
}

/// Whether a turn does anything on a layout: Left and Right need the
/// layout's table, Mirror and Shuffle a group of at least two lanes.
pub fn turn_supported(layout: &Layout, turn: Turn) -> bool {
    match turn {
        Turn::Off => true,
        Turn::Left | Turn::Right => turn_table(layout).is_some(),
        Turn::Mirror | Turn::Shuffle => shuffle_groups(layout).iter().any(|g| g.len() > 1),
    }
}

/// SplitMix64 (Steele, Lea and Flood 2014). Owned here rather than taken
/// from a crate so a dependency update can never change a seeded shuffle.
struct SplitMix64(u64);

impl SplitMix64 {
    fn next(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^ (z >> 31)
    }
}

/// StepMania `Little`: every note off the grid goes, whatever its kind.
fn keep_grid(notes: &mut Vec<Note>, grid: i64) {
    notes.retain(|n| n.tick.0.rem_euclid(grid) == 0);
}

/// Holds and rolls become taps (`ChangeRollsToHolds` + `RemoveHoldNotes`).
fn remove_holds(notes: &mut [Note]) {
    for n in notes {
        if matches!(
            n.kind,
            NoteKind::HoldHead { .. } | NoteKind::RollHead { .. }
        ) {
            n.kind = NoteKind::Tap;
        }
    }
}

/// StepMania `RemoveSimultaneousNotes(1)`: on each row, count the taps,
/// lifts and hold heads (`GetNumTracksWithTapOrHoldHead`) plus the lanes
/// still held by an earlier hold (one ending on this row included), and
/// remove taps and heads, never lifts, from the leftmost lane until at most
/// one remains. Mines and fakes are left alone. Rows are processed in
/// order, so a removed hold no longer holds later rows.
fn remove_jumps(notes: &mut Vec<Note>) {
    notes.sort_by_key(|n| (n.tick, n.lane));
    let mut removed = vec![false; notes.len()];
    // Per lane: index of the latest kept note before the current row,
    // ignoring autokeysounds (`IsHoldNoteAtRow` scans upwards like this).
    let mut last: Vec<Option<usize>> = Vec::new();
    let mut i = 0;
    while i < notes.len() {
        let tick = notes[i].tick;
        let mut end = i;
        while end < notes.len() && notes[end].tick == tick {
            end += 1;
        }
        let held = last
            .iter()
            .flatten()
            .filter(|&&k| match notes[k].kind {
                NoteKind::HoldHead { end } | NoteKind::RollHead { end } => end >= tick,
                _ => false,
            })
            .count();
        let pressable = |n: &Note| {
            matches!(
                n.kind,
                NoteKind::Tap | NoteKind::HoldHead { .. } | NoteKind::RollHead { .. }
            )
        };
        let on_row = (i..end)
            .filter(|&k| pressable(&notes[k]) || notes[k].kind == NoteKind::Lift)
            .count();
        let mut excess = (on_row + held).saturating_sub(1);
        // `i..end` is sorted by lane, so this is leftmost first.
        for k in i..end {
            if excess == 0 {
                break;
            }
            if pressable(&notes[k]) {
                removed[k] = true;
                excess -= 1;
            }
        }
        for k in i..end {
            if removed[k] || notes[k].kind == NoteKind::AutoKeysound {
                continue;
            }
            let lane = usize::from(notes[k].lane);
            if last.len() <= lane {
                last.resize(lane + 1, None);
            }
            last[lane] = Some(k);
        }
        i = end;
    }
    let mut k = 0;
    notes.retain(|_| {
        k += 1;
        !removed[k - 1]
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use ddi_chart::Tick;

    fn n(beat_48: i64, lane: u8, kind: NoteKind) -> Note {
        Note::new(Tick(beat_48), lane, kind)
    }

    fn tap(t: i64, lane: u8) -> Note {
        n(t, lane, NoteKind::Tap)
    }

    fn hold(t: i64, lane: u8, end: i64) -> Note {
        n(t, lane, NoteKind::HoldHead { end: Tick(end) })
    }

    fn opts(turn: Turn) -> TransformOptions {
        TransformOptions {
            turn,
            ..Default::default()
        }
    }

    fn lanes_of(notes: &[Note]) -> Vec<(i64, u8)> {
        notes.iter().map(|n| (n.tick.0, n.lane)).collect()
    }

    #[test]
    fn stepmania_tables() {
        // GetTrackMapping, dance-single: mirror reverses; left takes
        // [2,0,3,1]; right is its inverse.
        assert_eq!(
            lane_map(&Layout::dance_single(), Turn::Mirror, 0),
            [3, 2, 1, 0]
        );
        assert_eq!(
            lane_map(&Layout::dance_single(), Turn::Left, 0),
            [2, 0, 3, 1]
        );
        assert_eq!(
            lane_map(&Layout::dance_single(), Turn::Right, 0),
            [1, 3, 0, 2]
        );
        assert_eq!(
            lane_map(&Layout::dance_double(), Turn::Left, 0),
            [2, 0, 3, 1, 6, 4, 7, 5]
        );
        assert_eq!(
            lane_map(&Layout::dance_double(), Turn::Mirror, 0),
            [7, 6, 5, 4, 3, 2, 1, 0]
        );
        assert_eq!(
            lane_map(&Layout::dance_solo(), Turn::Left, 0),
            [5, 4, 0, 3, 1, 2]
        );
        // No table: identity.
        let generic = Layout::generic(5);
        assert_eq!(lane_map(&generic, Turn::Left, 0), [0, 1, 2, 3, 4]);
        assert!(!turn_supported(&generic, Turn::Right));
        assert!(turn_supported(&generic, Turn::Mirror));
        // A table that is not a permutation is ignored.
        let mut bad = Layout::generic(4);
        bad.turn_left = Some(vec![0, 0, 9, 1]);
        assert!(!turn_supported(&bad, Turn::Left));
        assert_eq!(lane_map(&bad, Turn::Right, 0), [0, 1, 2, 3]);
        for layout in Layout::builtins() {
            for turn in [Turn::Left, Turn::Right, Turn::Mirror, Turn::Shuffle] {
                assert!(turn_supported(&layout, turn), "{} {turn:?}", layout.id);
            }
        }
    }

    /// The shuffle as it was before shuffle groups, for regression.
    fn lane_map_v1(lanes: usize, seed: u64) -> Vec<u8> {
        let identity: Vec<u8> = (0..lanes as u8).collect();
        let mut s = seed;
        loop {
            let mut map = identity.clone();
            let mut rng = SplitMix64(s);
            for i in (1..lanes).rev() {
                let j = (rng.next() % (i as u64 + 1)) as usize;
                map.swap(i, j);
            }
            if map != identity {
                return map;
            }
            s = s.wrapping_add(1);
        }
    }

    /// Five lanes: four arrows in group 0, an onigiri alone in group 1 (the
    /// Dancing☆Onigiri 5-key groups, `shuffle5_0`).
    fn grouped() -> Layout {
        let mut l = Layout::generic(5);
        l.lanes[4].shuffle_group = 1;
        l
    }

    #[test]
    fn mirror_and_shuffle_stay_within_shuffle_groups() {
        assert_eq!(lane_map(&grouped(), Turn::Mirror, 0), [3, 2, 1, 0, 4]);
        let mut seen = std::collections::HashSet::new();
        for seed in 0..500 {
            let map = lane_map(&grouped(), Turn::Shuffle, seed);
            assert_eq!(map[4], 4);
            assert_ne!(map, [0, 1, 2, 3, 4]);
            seen.insert(map);
        }
        assert_eq!(seen.len(), 23);
        // Groups need not be contiguous: 7 keys keep the middle onigiri.
        let mut seven = Layout::generic(7);
        seven.lanes[3].shuffle_group = 1;
        assert_eq!(lane_map(&seven, Turn::Mirror, 0), [6, 5, 4, 3, 2, 1, 0]);
        let mut split = Layout::generic(4);
        split.lanes[1].shuffle_group = 1;
        split.lanes[3].shuffle_group = 1;
        assert_eq!(lane_map(&split, Turn::Mirror, 0), [2, 3, 0, 1]);
        // Only one-lane groups: nothing to permute.
        let mut lonely = Layout::generic(2);
        lonely.lanes[1].shuffle_group = 1;
        assert!(!turn_supported(&lonely, Turn::Shuffle));
        assert_eq!(lane_map(&lonely, Turn::Shuffle, 3), [0, 1]);
    }

    #[test]
    fn left_turns_up_arrows_left() {
        // L, D, U, R = 0, 1, 2, 3. Under Left an up arrow becomes a left
        // arrow, left becomes down, down becomes right, right becomes up.
        let t = apply(
            &[tap(0, 2), tap(48, 0), tap(96, 1), tap(144, 3)],
            &Layout::dance_single(),
            &opts(Turn::Left),
            0,
        );
        assert_eq!(lanes_of(&t.notes), [(0, 0), (48, 1), (96, 3), (144, 2)]);
        let r = apply(&t.notes, &Layout::dance_single(), &opts(Turn::Right), 0);
        assert_eq!(lanes_of(&r.notes), [(0, 2), (48, 0), (96, 1), (144, 3)]);
    }

    #[test]
    fn mirror_is_an_involution_and_keeps_kinds_and_ticks() {
        let notes = vec![
            tap(0, 0),
            hold(0, 2, 96),
            n(48, 1, NoteKind::Mine),
            n(72, 3, NoteKind::RollHead { end: Tick(144) }),
        ];
        let once = apply(&notes, &Layout::dance_single(), &opts(Turn::Mirror), 0);
        assert_eq!(
            once.notes,
            vec![
                hold(0, 1, 96),
                tap(0, 3),
                n(48, 2, NoteKind::Mine),
                n(72, 0, NoteKind::RollHead { end: Tick(144) }),
            ]
        );
        let twice = apply(&once.notes, &Layout::dance_single(), &opts(Turn::Mirror), 0);
        assert_eq!(twice.notes, notes);
    }

    #[test]
    fn shuffle_is_seeded_never_identity_and_reaches_every_order() {
        let mut seen = std::collections::HashSet::new();
        for seed in 0..2000u64 {
            let map = lane_map(&Layout::dance_single(), Turn::Shuffle, seed);
            assert_eq!(map, lane_map(&Layout::dance_single(), Turn::Shuffle, seed));
            assert_ne!(map, [0, 1, 2, 3]);
            let mut sorted = map.clone();
            sorted.sort();
            assert_eq!(sorted, [0, 1, 2, 3]);
            seen.insert(map);
        }
        assert_eq!(seen.len(), 23);
        // The first seeds give the permutations they gave before shuffles
        // knew about groups (a saved seed keeps its order).
        assert_eq!(
            lane_map(&Layout::dance_single(), Turn::Shuffle, 0),
            lane_map_v1(4, 0)
        );
        for seed in 0..64 {
            assert_eq!(
                lane_map(&Layout::dance_double(), Turn::Shuffle, seed),
                lane_map_v1(8, seed)
            );
        }
        // A one-lane layout cannot be shuffled.
        assert_eq!(lane_map(&Layout::generic(1), Turn::Shuffle, 7), [0]);
    }

    #[test]
    fn timing_cuts_keep_the_grid_only() {
        let notes = vec![
            tap(0, 0),
            tap(12, 1),
            hold(24, 2, 60),
            n(36, 3, NoteKind::Mine),
            n(48, 0, NoteKind::Lift),
            tap(64, 1),
        ];
        let q = apply(
            &notes,
            &Layout::dance_single(),
            &TransformOptions {
                cut: TimingCut::Quarters,
                ..Default::default()
            },
            0,
        );
        assert_eq!(lanes_of(&q.notes), [(0, 0), (48, 0)]);
        let e = apply(
            &notes,
            &Layout::dance_single(),
            &TransformOptions {
                cut: TimingCut::Eighths,
                ..Default::default()
            },
            0,
        );
        // The hold's tail is off the grid; the hold stays with it.
        assert_eq!(lanes_of(&e.notes), [(0, 0), (24, 2), (48, 0)]);
        assert_eq!(e.notes[1].kind, NoteKind::HoldHead { end: Tick(60) });
    }

    #[test]
    fn no_holds_turns_holds_and_rolls_into_taps() {
        let t = apply(
            &[
                hold(0, 0, 96),
                n(48, 1, NoteKind::RollHead { end: Tick(96) }),
            ],
            &Layout::dance_single(),
            &TransformOptions {
                no_holds: true,
                ..Default::default()
            },
            0,
        );
        assert!(t.notes.iter().all(|n| n.kind == NoteKind::Tap));
        assert!(
            TransformOptions {
                no_holds: true,
                ..Default::default()
            }
            .is_assist()
        );
        assert!(!opts(Turn::Shuffle).is_assist());
    }

    #[test]
    fn no_jumps_follows_remove_simultaneous_notes() {
        let notes = vec![
            // Jump: the leftmost goes.
            tap(0, 0),
            tap(0, 3),
            // Hand: the two leftmost go.
            tap(48, 0),
            tap(48, 1),
            tap(48, 2),
            // A hold, then a tap while it is held: the tap goes.
            hold(96, 1, 192),
            tap(144, 3),
            // A tap on the row the hold ends counts as simultaneous too.
            tap(192, 2),
            // Mines are neither counted nor removed.
            n(240, 0, NoteKind::Mine),
            tap(240, 1),
            // A lift counts towards a jump but is never removed: the tap
            // goes.
            tap(264, 1),
            n(264, 2, NoteKind::Lift),
            // Jump of two hold heads: the left hold goes, so it does not
            // hold the later row.
            hold(288, 0, 384),
            hold(288, 3, 312),
            tap(336, 1),
        ];
        let t = apply(
            &notes,
            &Layout::dance_single(),
            &TransformOptions {
                no_jumps: true,
                ..Default::default()
            },
            0,
        );
        assert_eq!(
            lanes_of(&t.notes),
            [
                (0, 3),
                (48, 2),
                (96, 1),
                (240, 0),
                (240, 1),
                (264, 2),
                (288, 3),
                (336, 1)
            ]
        );
    }

    #[test]
    fn removals_happen_before_the_turn() {
        // NoJumps removes the leftmost of the original chart, then Mirror
        // moves what is left.
        let t = apply(
            &[tap(0, 0), tap(0, 1)],
            &Layout::dance_single(),
            &TransformOptions {
                turn: Turn::Mirror,
                no_jumps: true,
                ..Default::default()
            },
            0,
        );
        assert_eq!(lanes_of(&t.notes), [(0, 2)]);
        assert_eq!(t.take_from, [3, 2, 1, 0]);
    }
}
