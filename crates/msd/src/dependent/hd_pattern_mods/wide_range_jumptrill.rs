// Translated to Rust from Etterna's difficulty calculator, MinaCalc:
// https://github.com/etternagame/etterna, src/Etterna/MinaCalc/Dependent/HD_PatternMods/WideRangeJumptrill.h
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

//! Jumptrillable patterns over a few intervals. Its timing checks change
//! the hand's any-ms window in place, as upstream's do.

use crate::calc_window::{CalcMovingWindow, MAX_MOVING_WINDOW_SIZE};
use crate::dependent::hd_basic_sequencing::BaseType;
use crate::dependent::hd_meta_sequencing::MetaType;
use crate::dependent::interval_hand_info::ItvHandInfo;
use crate::note_data::pmod;
use crate::pattern_mod_helpers::{NEUTRAL, clamp};

pub const WRJT_CV_FACTOR: f32 = 3.0;

#[derive(Clone, Debug)]
pub struct WideRangeJumptrillMod {
    window_param: f32,
    min_mod: f32,
    max_mod: f32,
    cv_threshhold: f32,
    window: usize,
    mw_jt: CalcMovingWindow<i32>,
    jt_counter: i32,
    bro_is_this_file_for_real: bool,
    last_passed_check: bool,
    pmod: f32,
}

impl Default for WideRangeJumptrillMod {
    fn default() -> Self {
        WideRangeJumptrillMod {
            window_param: 3.0,
            min_mod: 0.25,
            max_mod: 1.0,
            cv_threshhold: 0.05,
            window: 0,
            mw_jt: CalcMovingWindow::default(),
            jt_counter: 0,
            bro_is_this_file_for_real: false,
            last_passed_check: false,
            pmod: NEUTRAL,
        }
    }
}

impl WideRangeJumptrillMod {
    pub const PMOD: usize = pmod::WIDE_RANGE_JUMPTRILL;

    pub fn full_reset(&mut self) {
        self.mw_jt.zero();
        self.jt_counter = 0;
        self.bro_is_this_file_for_real = false;
        self.last_passed_check = false;
        self.pmod = NEUTRAL;
    }

    pub fn setup(&mut self) {
        self.window = clamp(self.window_param as i32, 1, MAX_MOVING_WINDOW_SIZE as i32) as usize;
    }

    fn check_last_mt(&self, mt: MetaType) -> bool {
        matches!(mt, MetaType::Acca | MetaType::Ccacc | MetaType::Cccccc) && self.last_passed_check
    }

    fn bibblybop(&mut self, mt: MetaType) {
        self.jt_counter += 1;
        if self.bro_is_this_file_for_real {
            self.jt_counter += 1;
        }
        if self.check_last_mt(mt) {
            self.jt_counter += 1;
            self.bro_is_this_file_for_real = true;
        }
    }

    pub fn advance_sequencing(
        &mut self,
        bt: BaseType,
        mt: MetaType,
        last_mt: MetaType,
        ms_any: &mut CalcMovingWindow<f32>,
    ) {
        if bt == BaseType::JumpJump || bt == BaseType::SingleJump {
            return;
        }
        let checked = match mt {
            MetaType::Cccccc => Some(ms_any.roll_timing_check(WRJT_CV_FACTOR, self.cv_threshhold)),
            MetaType::Ccacc => Some(ms_any.ccacc_timing_check(WRJT_CV_FACTOR, self.cv_threshhold)),
            MetaType::Acca => Some(ms_any.acca_timing_check(WRJT_CV_FACTOR, self.cv_threshhold)),
            _ => None,
        };
        if let Some(passed) = checked {
            self.last_passed_check = passed;
            if passed {
                self.bibblybop(last_mt);
                return;
            }
        }
        self.bro_is_this_file_for_real = false;
    }

    fn set_pmod(&mut self, itvhi: &ItvHandInfo) {
        if itvhi.get_taps_windowi(self.window) == 0
            || self.mw_jt.get_total_for_window(self.window) == 0
        {
            self.pmod = NEUTRAL;
            return;
        }
        if self.mw_jt.get_total_for_window(self.window) < 20 {
            self.pmod = NEUTRAL;
            return;
        }
        self.pmod = itvhi.get_taps_windowf(self.window)
            / self.mw_jt.get_total_for_windowf(self.window)
            * 0.75;
        self.pmod = clamp(self.pmod, self.min_mod, self.max_mod);
    }

    /// `operator()`.
    pub fn get(&mut self, itvhi: &ItvHandInfo) -> f32 {
        self.mw_jt.push(self.jt_counter);
        self.set_pmod(itvhi);
        self.jt_counter = 0;
        self.pmod
    }
}
