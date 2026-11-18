// Translated to Rust from Etterna's difficulty calculator, MinaCalc:
// https://github.com/etternagame/etterna, src/Etterna/MinaCalc/MinaCalc.h, MinaCalc.cpp
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

//! The calculator's driver: the rows into intervals and base difficulties
//! (through [`Ulbu`]), then for each skillset the player skill at which
//! the expected points reach the goal (`Chisel`), the stamina model, and
//! the overall rating.

use crate::minacalc_helpers::{
    ALL_ZERO_OUTPUT, DEFAULT_SCORE_GOAL, MAX_RATING, MIN_RATING, SSR_GOAL_CAP, aggregate_skill,
    downscale_low_accuracy_scores,
};
use crate::note_data::{
    NUM_SKILLSET, NoteInfo, SKILL_CHORDJACK, SKILL_HANDSTREAM, SKILL_JACKSPEED, SKILL_JUMPSTREAM,
    SKILL_OVERALL, SKILL_STAMINA, SKILL_STREAM, SKILL_TECHNICAL, diff, pmod,
};
use crate::pattern_mod_helpers::{clamp, fastpow, fastsqrt, max, min};
use crate::sequencing_helpers::{max_index, max_val};
use crate::ulbu::{BASESCALERS, PMODS, Ulbu};
use crate::ulbu_acolytes::fast_walk_and_check_for_skip;

/// Hard cap on the intervals of a file.
pub const MAX_INTERVALS: i32 = 100_000;
/// Hard cap on the rows of a half-second interval (a 100 nps burst).
pub const MAX_ROWS_FOR_SINGLE_INTERVAL: usize = 50;

pub const LEFT_HAND: usize = 0;
pub const RIGHT_HAND: usize = 1;
pub const NUM_HANDS: usize = 2;

/// A row as the calculator keeps it (`RowInfo`).
#[derive(Clone, Copy, Debug, Default)]
pub struct RowInfo {
    pub row_notes: u32,
    /// 1 to 4: a tap, jump, hand or quad.
    pub row_count: i32,
    pub hand_counts: [i32; NUM_HANDS],
    /// Rate-scaled time.
    pub row_time: f32,
}

/// The calculator (`Calc`). Its buffers are kept between ratings; every
/// rating starts from the state a new upstream `Calc` has.
#[derive(Clone, Debug, Default)]
pub struct Calc {
    /// Score related output (true) or the listing's MSD (false).
    pub ssr: bool,

    pub(crate) hand_col_masks: [u32; NUM_HANDS],
    pub(crate) col_masks: Vec<u32>,
    /// Every row, interval after interval (upstream's `adj_ni`).
    pub(crate) rows: Vec<RowInfo>,
    pub(crate) itv_start: Vec<usize>,
    pub(crate) itv_size: Vec<usize>,
    pub(crate) itv_points: [Vec<i32>; NUM_HANDS],
    pub(crate) pmod_vals: [Vec<Vec<f32>>; NUM_HANDS],
    pub(crate) init_base_diff_vals: [[Vec<f32>; diff::NUM]; NUM_HANDS],
    pub(crate) base_adj_diff: [[Vec<f32>; NUM_SKILLSET]; NUM_HANDS],
    pub(crate) base_diff_for_stam_mod: [[Vec<f32>; NUM_SKILLSET]; NUM_HANDS],
    pub(crate) stam_adj_diff: Vec<f32>,
    /// (time, difficulty) a row with a note, a hand.
    pub(crate) jack_diff: [Vec<(f32, f32)>; NUM_HANDS],
    pub(crate) tc_static: Vec<f32>,
    pub(crate) numitv: usize,
    pub(crate) max_points: f32,
    pub(crate) grindscaler: f32,
}

impl Calc {
    pub fn new() -> Calc {
        Calc::default()
    }

    pub(crate) fn rows_of(&self, itv: usize) -> &[RowInfo] {
        let start = self.itv_start[itv];
        &self.rows[start..start + self.itv_size[itv]]
    }

    /// Every per-interval vector at `numitv` zeros, as a new `Calc` has.
    fn size_for_intervals(&mut self) {
        let n = self.numitv;
        let zero = |v: &mut Vec<f32>| {
            v.clear();
            v.resize(n, 0.0);
        };
        for hand in 0..NUM_HANDS {
            self.itv_points[hand].clear();
            self.itv_points[hand].resize(n, 0);
            self.pmod_vals[hand].resize_with(pmod::NUM, Vec::new);
            for v in &mut self.pmod_vals[hand] {
                zero(v);
            }
            for v in &mut self.init_base_diff_vals[hand] {
                zero(v);
            }
            for v in &mut self.base_adj_diff[hand] {
                zero(v);
            }
            for v in &mut self.base_diff_for_stam_mod[hand] {
                zero(v);
            }
            self.jack_diff[hand].clear();
        }
        zero(&mut self.stam_adj_diff);
        self.tc_static.clear();
        self.tc_static.resize(MAX_ROWS_FOR_SINGLE_INTERVAL, 0.0);
        self.grindscaler = 1.0;
    }

    /// `CalcMain`: the eight numbers for `rows` at `music_rate` and
    /// `score_goal` (a fraction, 0.93 for 93%).
    pub fn calc_main(
        &mut self,
        rows: &[NoteInfo],
        music_rate: f32,
        score_goal: f32,
    ) -> [f32; NUM_SKILLSET] {
        let Some(ulbu) = self.initialize_hands(rows, music_rate, 0.0) else {
            return ALL_ZERO_OUTPUT;
        };
        self.max_points = self.total_max_points();

        let mut values = [0.0f32; NUM_SKILLSET];
        for (i, v) in values.iter_mut().enumerate() {
            *v = self.chisel(0.1, 10.24, score_goal, i, false);
        }

        let highest_base_skillset = max_index(&values);
        let base = values[highest_base_skillset];

        for i in 0..NUM_SKILLSET {
            if values[i] > base * 0.9 {
                values[i] = self.chisel(values[i] * 0.9, 0.32, score_goal, i, true);
            }
        }

        let highest_stam_adjusted_skillset = max_index(&values);
        let mut highest_stam_adj_ss_value = values[highest_base_skillset];
        if highest_stam_adjusted_skillset == SKILL_JACKSPEED {
            highest_stam_adj_ss_value *= 0.8;
        }

        const STAM_CURVE_SHIFT: f32 = 0.015;
        let mut stam_adj_mult =
            libm::powf((highest_stam_adj_ss_value / base) - STAM_CURVE_SHIFT, 2.5);
        stam_adj_mult = clamp(stam_adj_mult, 0.8, 1.08);
        values[SKILL_STAMINA] =
            highest_stam_adj_ss_value * stam_adj_mult * BASESCALERS[SKILL_STAMINA];

        ulbu.apply_keymode_multipliers(&mut values);

        if self.ssr {
            const SSRCAP: f32 = 40.0;
            for r in &mut values {
                *r = downscale_low_accuracy_scores(*r, score_goal);
                *r = min(*r, SSRCAP);
                if highest_stam_adjusted_skillset == SKILL_JACKSPEED {
                    *r = downscale_low_accuracy_scores(*r, score_goal);
                }
            }
        }

        let agg = aggregate_skill(&values, 0.25, 1.11, 0.0, 10.24);
        let highest = max_val(&values);
        values[SKILL_OVERALL] = if agg > highest { agg } else { highest };

        // The mean of one pass is the pass.
        let mut output = values;
        let mut highest_final_ss = SKILL_OVERALL;
        let mut highest_final_ssv = -1.0f32;
        for (i, &v) in output.iter().enumerate() {
            if i == SKILL_OVERALL {
                continue;
            }
            if v > highest_final_ssv {
                highest_final_ss = i;
                highest_final_ssv = v;
            }
        }
        if self.ssr {
            if highest_final_ss == SKILL_JACKSPEED || highest_final_ss == SKILL_CHORDJACK {
                self.grindscaler = fastsqrt(self.grindscaler);
            }
            for ssv in &mut output {
                *ssv *= self.grindscaler;
            }
        }
        output
    }

    fn total_max_points(&self) -> f32 {
        let mut max_points = 0i32;
        for i in 0..self.numitv {
            max_points += self.itv_points[LEFT_HAND][i] + self.itv_points[RIGHT_HAND][i];
        }
        max_points as f32
    }

    /// `InitializeHands`: `None` to skip the file.
    fn initialize_hands(
        &mut self,
        rows: &[NoteInfo],
        music_rate: f32,
        offset: f32,
    ) -> Option<Ulbu> {
        if fast_walk_and_check_for_skip(rows, music_rate, self, offset) {
            return None;
        }
        self.size_for_intervals();
        let mut ulbu = Ulbu::default();
        ulbu.run(self);
        for hand in 0..NUM_HANDS {
            self.init_adj_diff(&ulbu, hand);
        }
        Some(ulbu)
    }

    /// `InitAdjDiff`: each skillset's difficulty an interval from the NPS
    /// base and the pattern mods.
    fn init_adj_diff(&mut self, ulbu: &Ulbu, hand: usize) {
        for i in 0..self.numitv {
            let mut pmod_product_cur_interval = [1.0f32; NUM_SKILLSET];
            for ss in 0..NUM_SKILLSET {
                if ss == SKILL_OVERALL || ss == SKILL_STAMINA {
                    continue;
                }
                for &pm in PMODS[ss] {
                    pmod_product_cur_interval[ss] *= self.pmod_vals[hand][pm][i];
                }
            }
            for ss in 0..NUM_SKILLSET {
                if ss == SKILL_OVERALL || ss == SKILL_STAMINA {
                    continue;
                }
                let adj_npsbase = self.init_base_diff_vals[hand][diff::NPS_BASE][i]
                    * pmod_product_cur_interval[ss]
                    * BASESCALERS[ss];
                let mut adj_diff = adj_npsbase;
                let mut stam_base = adj_npsbase;
                ulbu.adj_diff_func(
                    i,
                    hand,
                    &mut adj_diff,
                    &mut stam_base,
                    adj_npsbase,
                    ss,
                    &pmod_product_cur_interval,
                    self,
                );
                self.base_adj_diff[hand][ss][i] = adj_diff;
                self.base_diff_for_stam_mod[hand][ss][i] = stam_base;
            }
        }
    }

    /// `Chisel`: the player skill at which `ss`'s expected points reach
    /// `score_goal`, searched from `player_skill` up by `resolution`.
    fn chisel(
        &mut self,
        player_skill: f32,
        resolution: f32,
        score_goal: f32,
        ss: usize,
        stamina: bool,
    ) -> f32 {
        if ss == SKILL_OVERALL || ss == SKILL_STAMINA {
            return MIN_RATING;
        }
        let reqpoints = self.max_points * score_goal;

        let mut curr_player_skill = player_skill;
        let mut curr_resolution = resolution;
        loop {
            if curr_player_skill > MAX_RATING {
                return MIN_RATING;
            }
            curr_player_skill += curr_resolution;
            let gotpoints = self.calc_gotpoints(curr_player_skill, ss, stamina);
            if !(gotpoints < reqpoints) {
                break;
            }
        }
        curr_player_skill -= curr_resolution;
        curr_resolution /= 2.0;

        for _ in 1..=7 {
            if curr_player_skill > MAX_RATING {
                return MIN_RATING;
            }
            curr_player_skill += curr_resolution;
            let gotpoints = self.calc_gotpoints(curr_player_skill, ss, stamina);
            if gotpoints > reqpoints {
                curr_player_skill -= curr_resolution;
            }
            curr_resolution /= 2.0;
        }
        curr_player_skill + 2.0 * curr_resolution
    }

    /// Chisel's `calc_gotpoints`.
    fn calc_gotpoints(&mut self, curr_player_skill: f32, ss: usize, stamina: bool) -> f32 {
        let mut gotpoints = match ss {
            SKILL_TECHNICAL => self.max_points * TECH_PBM,
            SKILL_JACKSPEED => self.max_points * JACK_PBM,
            SKILL_STREAM => self.max_points * STREAM_PBM,
            SKILL_JUMPSTREAM | SKILL_HANDSTREAM | SKILL_CHORDJACK => {
                self.max_points * BAD_NEWBIE_SKILLSETS_PBM
            }
            _ => 0.0,
        };
        let max_slap_dash_jack_cap_hack_tech_hat = self.max_points * 0.1;
        for hand in 0..NUM_HANDS {
            if ss == SKILL_JACKSPEED {
                gotpoints -= self.jackloss(curr_player_skill, hand, stamina);
            } else {
                self.calc_internal(&mut gotpoints, curr_player_skill, ss, stamina, hand);
            }
            if ss == SKILL_TECHNICAL {
                gotpoints -= fastsqrt(min(
                    max_slap_dash_jack_cap_hack_tech_hat,
                    self.jackloss(curr_player_skill * 0.75, hand, stamina) * 0.85,
                ));
            }
        }
        gotpoints
    }

    /// `CalcInternal`: the points lost a hand at player skill `x`.
    fn calc_internal(&mut self, gotpoints: &mut f32, x: f32, ss: usize, stam: bool, hand: usize) {
        if stam {
            self.stam_adjust(x, ss, hand);
        }
        let v = if stam {
            &self.stam_adj_diff
        } else {
            &self.base_adj_diff[hand][ss]
        };
        let pointloss_pow_val = match ss {
            SKILL_CHORDJACK => 1.8,
            SKILL_TECHNICAL => 2.0,
            _ => 1.7f32,
        };
        let points = &self.itv_points[hand];
        for i in 0..self.numitv {
            if x < v[i] {
                let pts = points[i] as f32;
                *gotpoints -=
                    pts - (pts * fastpow(f64::from(x / v[i]), f64::from(pointloss_pow_val)));
            }
        }
    }

    /// `StamAdjust`: the stamina model's multiplier, interval by interval,
    /// into `stam_adj_diff`.
    fn stam_adjust(&mut self, x: f32, ss: usize, hand: usize) {
        const STAM_CEIL: f32 = 1.075_234;
        const STAM_MAG: f32 = 243.0;
        const STAM_FSCALE: f32 = 500.0;
        const STAM_PROP: f32 = 0.69424;
        const SUPER_STAM_CEIL: f32 = 1.09;
        let mut stam_floor = 0.95f32;
        let mut mod_ = 0.95f32;
        let mut avs2 = 0.0f32;
        let base_diff = &self.base_diff_for_stam_mod[hand][ss];
        let diff = &self.base_adj_diff[hand][ss];
        for i in 0..self.numitv {
            let avs1 = avs2;
            avs2 = base_diff[i];
            mod_ += ((((avs1 + avs2) / 2.0) / (STAM_PROP * x)) - 1.0) / STAM_MAG;
            if mod_ > 0.95 {
                stam_floor += (mod_ - 0.95) / STAM_FSCALE;
            }
            let local_ceil = STAM_CEIL * stam_floor;
            mod_ = min(clamp(mod_, stam_floor, local_ceil), SUPER_STAM_CEIL);
            self.stam_adj_diff[i] = diff[i] * mod_;
        }
    }

    /// `jackloss`, with `JackStamAdjust` folded in when `stam`.
    fn jackloss(&self, x: f32, hand: usize, stam: bool) -> f32 {
        const STAM_CEIL: f32 = 1.05234;
        const STAM_MAG: f32 = 23.0;
        const STAM_FSCALE: f32 = 750.0;
        const STAM_PROP: f32 = 0.49424;
        const SUPER_STAM_CEIL: f32 = 1.01;
        let diff = &self.jack_diff[hand];
        let mut total = 0.0f32;
        let mut stam_floor = 0.95f32;
        let mut mod_ = 1.0f32;
        let mut avs2 = 0.0f32;
        for &(_, d) in diff {
            let y = if stam {
                let avs1 = avs2;
                avs2 = d;
                mod_ += ((((avs1 + avs2) / 2.0) / (STAM_PROP * x)) - 1.0) / STAM_MAG;
                if mod_ > 0.95 {
                    stam_floor += (mod_ - 0.95) / STAM_FSCALE;
                }
                let local_ceil = STAM_CEIL * stam_floor;
                mod_ = min(clamp(mod_, stam_floor, local_ceil), SUPER_STAM_CEIL);
                d * mod_
            } else {
                d
            };
            if x < y && y > 0.0 {
                total += jack_pointloser_func(x, y);
            }
        }
        total
    }
}

/// Point buffer multipliers: max points a bit above the actual.
const TECH_PBM: f32 = 1.0;
const JACK_PBM: f32 = 1.0175;
const STREAM_PBM: f32 = 1.01;
const BAD_NEWBIE_SKILLSETS_PBM: f32 = 1.0;

const MAGIC_NUM: f32 = 12.0;

#[inline]
fn jack_pointloser_func(x: f32, y: f32) -> f32 {
    max(MAGIC_NUM * libm::erff(0.04 * (y - x)), 0.0)
}

/// What Etterna lists for a chart (`MinaSDCalc` for caching): ratings at
/// `rate` and the default score goal, without the score-only adjustments.
/// A chart of one row or none is all zero.
pub fn msd(rows: &[NoteInfo], rate: f32) -> [f32; NUM_SKILLSET] {
    if rows.len() <= 1 {
        return ALL_ZERO_OUTPUT;
    }
    let mut calc = Calc::new();
    calc.ssr = false;
    calc.calc_main(rows, rate, DEFAULT_SCORE_GOAL)
}

/// What Etterna gives a score (`MinaSDCalc` for a score): ratings at
/// `rate` and `goal` (capped at 96.5%), with the score adjustments.
pub fn ssr(rows: &[NoteInfo], rate: f32, goal: f32) -> [f32; NUM_SKILLSET] {
    if rows.len() <= 1 {
        return ALL_ZERO_OUTPUT;
    }
    let mut calc = Calc::new();
    calc.ssr = true;
    calc.calc_main(rows, rate, min(goal, SSR_GOAL_CAP))
}
