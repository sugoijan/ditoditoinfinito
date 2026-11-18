// Translated to Rust from Etterna's difficulty calculator, MinaCalc:
// https://github.com/etternagame/etterna, src/Etterna/MinaCalc/Dependent/HD_PatternMods/WideRangeJJ.h
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

//! Jumpjacks over a few intervals.

use crate::calc_window::{CalcMovingWindow, MAX_MOVING_WINDOW_SIZE};
use crate::dependent::hd_basic_sequencing::ColType;
use crate::dependent::interval_hand_info::ItvHandInfo;
use crate::minacalc::MAX_ROWS_FOR_SINGLE_INTERVAL;
use crate::note_data::pmod;
use crate::pattern_mod_helpers::{NEUTRAL, clamp, fastsqrt, max};
use crate::sequencing_helpers::S_INIT;

#[derive(Clone, Debug)]
pub struct WideRangeJjMod {
    window_param: f32,
    jj_required: f32,
    min_mod: f32,
    max_mod: f32,
    total_scaler: f32,
    cur_interval_tap_scaler: f32,
    ms_threshold: f32,
    calming_comp: f32,
    diff_falloff_power: f32,

    window: usize,
    lc: usize,
    rc: usize,
    mw_max_problems: CalcMovingWindow<f32>,
    current_problems: f32,
    max_interval_problems: f32,
    pmod: f32,
    left_times: [f32; MAX_ROWS_FOR_SINGLE_INTERVAL],
    right_times: [f32; MAX_ROWS_FOR_SINGLE_INTERVAL],
}

impl Default for WideRangeJjMod {
    fn default() -> Self {
        WideRangeJjMod {
            window_param: 3.0,
            jj_required: 30.0,
            min_mod: 0.25,
            max_mod: 1.0,
            total_scaler: 2.5,
            cur_interval_tap_scaler: 1.2,
            ms_threshold: 0.065,
            calming_comp: 0.05,
            diff_falloff_power: 6.0,
            window: 0,
            lc: 0,
            rc: 0,
            mw_max_problems: CalcMovingWindow::default(),
            current_problems: 0.0,
            max_interval_problems: 0.0,
            pmod: NEUTRAL,
            left_times: [0.0; MAX_ROWS_FOR_SINGLE_INTERVAL],
            right_times: [0.0; MAX_ROWS_FOR_SINGLE_INTERVAL],
        }
    }
}

impl WideRangeJjMod {
    pub const PMOD: usize = pmod::WIDE_RANGE_JJ;

    pub fn full_reset(&mut self) {
        self.mw_max_problems.zero();
        self.left_times = [S_INIT; MAX_ROWS_FOR_SINGLE_INTERVAL];
        self.right_times = [S_INIT; MAX_ROWS_FOR_SINGLE_INTERVAL];
        self.current_problems = 0.0;
        self.max_interval_problems = 0.0;
        self.lc = 0;
        self.rc = 0;
        self.pmod = NEUTRAL;
    }

    pub fn setup(&mut self) {
        self.window = clamp(self.window_param as i32, 1, MAX_MOVING_WINDOW_SIZE as i32) as usize;
    }

    fn check(&mut self) {
        let mut lindex = 0;
        let mut rindex = 0;
        let mut jump_jacking = false;
        let mut failed_left = false;
        let mut failed_right = false;
        while lindex < self.lc && rindex < self.rc {
            let l = self.left_times[lindex];
            let r = self.right_times[rindex];
            let diff = (l - r).abs();
            if diff < self.ms_threshold {
                lindex += 1;
                rindex += 1;
                if !jump_jacking {
                    self.current_problems = 0.0;
                }
                let x = libm::powf(
                    diff / max(self.ms_threshold, 0.00001),
                    self.diff_falloff_power,
                );
                let v = 1.0 + (x / (x - 2.0));
                self.current_problems += v;
                if self.current_problems > self.max_interval_problems {
                    self.max_interval_problems = self.current_problems;
                }
                jump_jacking = true;
            } else if l > r {
                rindex += 1;
                if failed_right {
                    jump_jacking = false;
                }
                failed_right = true;
            } else if r > l {
                lindex += 1;
                if failed_left {
                    jump_jacking = false;
                }
                failed_left = true;
            } else {
                lindex += 1;
                rindex += 1;
                if failed_left || failed_right {
                    jump_jacking = false;
                }
                failed_left = true;
                failed_right = true;
            }
        }
    }

    pub fn advance_sequencing(&mut self, ct: ColType, time_s: f32) {
        if self.lc >= MAX_ROWS_FOR_SINGLE_INTERVAL || self.rc >= MAX_ROWS_FOR_SINGLE_INTERVAL {
            return;
        }
        match ct {
            ColType::Left => {
                self.left_times[self.lc] = time_s;
                self.lc += 1;
            }
            ColType::Right => {
                self.right_times[self.rc] = time_s;
                self.rc += 1;
            }
            ColType::OhJump => {
                self.left_times[self.lc] = time_s;
                self.lc += 1;
                self.right_times[self.rc] = time_s;
                self.rc += 1;
            }
            _ => {}
        }
    }

    fn set_pmod(&mut self, itvhi: &ItvHandInfo) {
        let taps_in_window = itvhi.get_taps_windowf(self.window) * self.cur_interval_tap_scaler;
        let problems_in_window =
            self.mw_max_problems.get_total_for_windowf(self.window) * self.total_scaler;
        if taps_in_window == 0.0 || problems_in_window < self.jj_required {
            self.pmod = fastsqrt(self.pmod + clamp(self.calming_comp, 0.0, 1.0));
        } else {
            self.pmod = taps_in_window / problems_in_window * 0.75;
        }
        self.pmod = clamp(self.pmod, self.min_mod, self.max_mod);
    }

    /// `operator()`.
    pub fn get(&mut self, itvhi: &ItvHandInfo) -> f32 {
        self.check();
        self.mw_max_problems.push(self.max_interval_problems);
        self.set_pmod(itvhi);
        self.interval_end();
        self.pmod
    }

    fn interval_end(&mut self) {
        self.current_problems = 0.0;
        self.max_interval_problems = 0.0;
        self.left_times = [S_INIT; MAX_ROWS_FOR_SINGLE_INTERVAL];
        self.right_times = [S_INIT; MAX_ROWS_FOR_SINGLE_INTERVAL];
        self.lc = 0;
        self.rc = 0;
    }
}
