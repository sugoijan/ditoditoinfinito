// Translated to Rust from Etterna's difficulty calculator, MinaCalc:
// https://github.com/etternagame/etterna, src/Etterna/MinaCalc/UlbuAcolytes.h
// at commit 2d706147cc4bcf95a8ceb24dc4f4129be207f803 (calculator version
// 527). Changes: see crates/msd/README.md.
//
// Copyright (c) 2016-2023 Etterna <etternadev@gmail.com>.
//
// Permission is hereby granted, free of charge, to any person obtaining a
// copy of this software and associated documentation files (the
// "Software"), to deal in the Software without restriction, including
// without limitation the rights to use, copy, modify, merge, publish,
// distribute, sublicense, and/or sell copies of the Software, and to permit
// persons to whom the Software is furnished to do so, subject to the
// following conditions:
//
// The above copyright notice and this permission notice shall be included
// in all copies or substantial portions of the Software.
//
// THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS
// OR IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF
// MERCHANTABILITY, FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN
// NO EVENT SHALL THE AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM,
// DAMAGES OR OTHER LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR
// OTHERWISE, ARISING FROM, OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE
// USE OR OTHER DEALINGS IN THE SOFTWARE.

//! Smoothing, the pattern mods' bookkeeping, and the walk that turns the
//! rows into half-second intervals (`fast_walk_and_check_for_skip`).

use crate::minacalc::{
    Calc, LEFT_HAND, MAX_INTERVALS, MAX_ROWS_FOR_SINGLE_INTERVAL, RIGHT_HAND, RowInfo,
};
use crate::note_data::{NoteInfo, pmod};
use crate::pattern_mod_helpers::NEUTRAL;
use crate::sequencing_helpers::column_count;

pub const INTERVAL_SPAN: f32 = 0.5;

/// Average of three continuous elements, the start biased to `neutral`.
pub fn smooth(input: &mut [f32], neutral: f32, end_interval: usize) {
    let mut f2 = neutral;
    let mut f3 = neutral;
    for v in input.iter_mut().take(end_interval) {
        let f1 = f2;
        f2 = f3;
        f3 = *v;
        *v = (f1 + f2 + f3) / 3.0;
    }
}

pub const AGNOSTIC_MODS: [usize; 12] = [
    pmod::STREAM,
    pmod::JS,
    pmod::HS,
    pmod::CJ,
    pmod::CJ_DENSITY,
    pmod::HS_DENSITY,
    pmod::FLAM_JAM,
    pmod::THE_THING,
    pmod::THE_THING2,
    pmod::G_CHORD_STREAM,
    pmod::OLD_JUMP_SCALER,
    pmod::HAND_BALANCE,
];

pub const DEPENDENT_MODS: [usize; 21] = [
    pmod::OH_JUMP_MOD,
    pmod::BALANCE,
    pmod::ROLL,
    pmod::ROLL_JS,
    pmod::OH_TRILL,
    pmod::VOH_TRILL,
    pmod::CHAOS,
    pmod::WIDE_RANGE_BALANCE,
    pmod::WIDE_RANGE_ROLL,
    pmod::WIDE_RANGE_JUMPTRILL,
    pmod::WIDE_RANGE_JJ,
    pmod::WIDE_RANGE_ANCHOR,
    pmod::RAN_MAN,
    pmod::MINIJACK,
    pmod::CJ_OH_JUMP,
    pmod::G_STREAM,
    pmod::G_BRACKETING,
    pmod::HAND_SWITCH,
    pmod::OLD_ANCHOR_SCALER,
    pmod::OLD_OHJ_SCALER,
    pmod::OLD_ROLL_SCALER,
];

/// `PatternMods`.
pub fn set_agnostic(pmod: usize, val: f32, pos: usize, calc: &mut Calc) {
    calc.pmod_vals[LEFT_HAND][pmod][pos] = val;
}

pub fn set_dependent(hand: usize, pmod: usize, val: f32, pos: usize, calc: &mut Calc) {
    calc.pmod_vals[hand][pmod][pos] = val;
}

pub fn run_agnostic_smoothing_pass(end_itv: usize, calc: &mut Calc) {
    for pmod in AGNOSTIC_MODS {
        smooth(&mut calc.pmod_vals[LEFT_HAND][pmod], NEUTRAL, end_itv);
    }
}

/// Both hands' values, each time a hand's loop ends (so the left hand's
/// are smoothed twice, as upstream's are).
pub fn run_dependent_smoothing_pass(end_itv: usize, calc: &mut Calc) {
    for pmod in DEPENDENT_MODS {
        for h in &mut calc.pmod_vals {
            smooth(&mut h[pmod], NEUTRAL, end_itv);
        }
    }
}

/// Copy the hand-agnostic mods from the left hand to the right.
pub fn bruh_they_the_same(end_itv: usize, calc: &mut Calc) {
    for pmod in AGNOSTIC_MODS {
        for i in 0..end_itv {
            calc.pmod_vals[RIGHT_HAND][pmod][i] = calc.pmod_vals[LEFT_HAND][pmod][i];
        }
    }
}

/// Time to interval index, half a millisecond late to break ties at an
/// interval's boundary.
#[inline]
pub fn time_to_itv_idx(time: f32) -> i32 {
    ((time + 0.0005) / INTERVAL_SPAN) as i32
}

#[inline]
pub fn itv_idx_to_time(idx: i32) -> f32 {
    idx as f32 * INTERVAL_SPAN
}

/// Checks the rows fit (otherwise it is a joke file, rated all zero) and
/// lays them into intervals with their counts and rate-scaled times.
/// Returns whether to skip.
pub fn fast_walk_and_check_for_skip(
    ni: &[NoteInfo],
    rate: f32,
    calc: &mut Calc,
    offset: f32,
) -> bool {
    let last = ni[ni.len() - 1].row_time;
    if last.is_infinite() || last.is_nan() {
        return true;
    }
    // Upstream grows its vectors past its default of 1000 intervals and
    // refuses a guess of `max_intervals` or more.
    let guess = time_to_itv_idx(last / rate) + 1;
    if guess >= MAX_INTERVALS {
        return true;
    }
    for i in 1..ni.len() {
        if ni[i - 1].row_time >= ni[i].row_time {
            return true;
        }
    }

    // Four keys: no middle column.
    let max_keycount_notes = 0b1111u32;
    let left_hand_mask = 0b0011u32;
    let right_hand_mask = 0b1100u32;
    calc.hand_col_masks = [left_hand_mask, right_hand_mask];
    calc.col_masks = vec![1, 2, 4, 8];

    calc.rows.clear();
    calc.itv_start.clear();
    calc.itv_size.clear();
    calc.itv_start.push(0);
    calc.itv_size.push(0);

    let mut itv = 0i32;
    let mut last_itv = 0i32;
    let mut row_counter = 0usize;
    for ri in ni {
        if row_counter >= MAX_ROWS_FOR_SINGLE_INTERVAL {
            return true;
        }
        if ri.notes > max_keycount_notes {
            return true;
        }
        // Upstream throws on these two (a row with no notes indexes its
        // counts at -1; a time before the first interval indexes it at -1).
        if ri.notes == 0 {
            return true;
        }
        let scaled_time = (ri.row_time + offset) / rate;
        itv = time_to_itv_idx(scaled_time);
        if itv < 0 {
            return true;
        }
        if itv > last_itv {
            calc.itv_size[last_itv as usize] = row_counter;
            for _ in (last_itv + 1)..=itv {
                calc.itv_start.push(calc.rows.len());
                calc.itv_size.push(0);
            }
            last_itv = itv;
            row_counter = 0;
        }
        calc.rows.push(RowInfo {
            row_notes: ri.notes,
            row_count: column_count(ri.notes),
            hand_counts: [
                (ri.notes & left_hand_mask).count_ones() as i32,
                (ri.notes & right_hand_mask).count_ones() as i32,
            ],
            row_time: scaled_time,
        });
        row_counter += 1;
    }
    calc.itv_size[itv as usize] = row_counter;
    calc.numitv = (itv + 1) as usize;
    false
}
