// Translated to Rust from Etterna's difficulty calculator, MinaCalc:
// https://github.com/etternagame/etterna, src/Etterna/MinaCalc/Dependent/HD_PatternMods/WideRangeRoll.h
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

//! Rolls over a few intervals, by their timing.

use crate::calc_window::{CalcMovingWindow, MAX_MOVING_WINDOW_SIZE};
use crate::dependent::hd_basic_sequencing::BaseType;
use crate::dependent::hd_meta_sequencing::MetaType;
use crate::dependent::interval_hand_info::ItvHandInfo;
use crate::note_data::pmod;
use crate::pattern_mod_helpers::{NEUTRAL, clamp, cv, fastsqrt};
use crate::sequencing_helpers::any_ms_is_greater;

#[derive(Clone, Debug)]
pub struct WideRangeRollMod {
    window_param: f32,
    min_mod: f32,
    max_mod: f32,
    base: f32,
    cv_reset: f32,
    cv_threshold: f32,
    other_cv_threshold: f32,

    window: usize,
    mw_max: CalcMovingWindow<i32>,
    mw_adj_ms: CalcMovingWindow<f32>,
    last_passed_check: bool,
    nah_this_file_aint_for_real: i32,
    max_thingy: i32,
    hi_im_a_float: f32,
    idk_ms: [f32; 4],
    seq_ms: [f32; 3],
    moving_cv: f32,
    pmod: f32,
}

impl Default for WideRangeRollMod {
    fn default() -> Self {
        WideRangeRollMod {
            window_param: 5.0,
            min_mod: 0.25,
            max_mod: 1.0,
            base: 0.15,
            cv_reset: 1.0,
            cv_threshold: 0.35,
            other_cv_threshold: 0.3,
            window: 0,
            mw_max: CalcMovingWindow::default(),
            mw_adj_ms: CalcMovingWindow::default(),
            last_passed_check: false,
            nah_this_file_aint_for_real: 0,
            max_thingy: 0,
            hi_im_a_float: 0.0,
            idk_ms: [0.0; 4],
            seq_ms: [0.0; 3],
            moving_cv: 1.0,
            pmod: 0.25,
        }
    }
}

impl WideRangeRollMod {
    pub const PMOD: usize = pmod::WIDE_RANGE_ROLL;

    pub fn full_reset(&mut self) {
        self.mw_max.zero();
        self.mw_adj_ms.zero();
        self.last_passed_check = false;
        self.nah_this_file_aint_for_real = 0;
        self.max_thingy = 0;
        self.hi_im_a_float = 0.0;
        self.seq_ms = [0.0; 3];
        self.idk_ms = [0.0; 4];
        self.moving_cv = self.cv_reset;
        self.pmod = NEUTRAL;
    }

    pub fn setup(&mut self) {
        self.window = clamp(self.window_param as i32, 1, MAX_MOVING_WINDOW_SIZE as i32) as usize;
    }

    fn zoop_the_woop(&mut self, pos: usize, div: f32, scaler: f32) {
        self.seq_ms[pos] /= div;
        self.last_passed_check = self.do_timing_thing(scaler);
        self.seq_ms[pos] *= div;
    }

    fn do_timing_thing(&mut self, scaler: f32) -> bool {
        self.mw_adj_ms.push(self.seq_ms[1]);
        if self.mw_adj_ms.get_cv_of_window(self.window) > self.other_cv_threshold {
            return false;
        }
        self.hi_im_a_float = cv(&self.seq_ms);
        if self.hi_im_a_float < 0.12 {
            self.moving_cv = (self.hi_im_a_float + self.moving_cv + self.hi_im_a_float) / 3.0;
            return true;
        }
        self.moving_cv = (self.hi_im_a_float + self.moving_cv) / 2.0;
        self.moving_cv < self.cv_threshold / scaler
    }

    fn do_other_timing_thing(&mut self, scaler: f32) -> bool {
        self.mw_adj_ms.push(self.idk_ms[1]);
        self.mw_adj_ms.push(self.idk_ms[2]);
        if self.mw_adj_ms.get_cv_of_window(self.window) > self.other_cv_threshold {
            return false;
        }
        self.hi_im_a_float = cv(&self.idk_ms);
        if self.hi_im_a_float < 0.12 {
            self.moving_cv = (self.hi_im_a_float + self.moving_cv + self.hi_im_a_float) / 3.0;
            return true;
        }
        self.moving_cv = (self.hi_im_a_float + self.moving_cv) / 2.0;
        self.moving_cv < self.cv_threshold / scaler
    }

    fn handle_ccacc_timing_check(&mut self) {
        self.zoop_the_woop(1, 2.5, 1.25);
    }

    fn handle_roll_timing_check(&mut self) {
        if any_ms_is_greater(self.seq_ms[1], self.seq_ms[0]) {
            self.zoop_the_woop(1, 2.5, 1.0);
        } else {
            self.seq_ms[0] /= 2.5;
            self.seq_ms[2] /= 2.5;
            self.last_passed_check = self.do_timing_thing(1.0);
            self.seq_ms[0] *= 2.5;
            self.seq_ms[2] *= 2.5;
        }
    }

    /// Upstream ignores what `do_other_timing_thing` answers here, so the
    /// last check's result carries over.
    fn handle_ccsjjscc_timing_check(&mut self, now: f32) {
        self.idk_ms[2] = self.seq_ms[0];
        self.idk_ms[1] = self.seq_ms[1];
        self.idk_ms[0] = self.seq_ms[2];
        self.idk_ms[3] = now;

        self.idk_ms[1] /= 2.5;
        self.idk_ms[2] /= 2.5;
        self.do_other_timing_thing(1.25);
        self.idk_ms[1] *= 2.5;
        self.idk_ms[2] *= 2.5;

        if self.last_passed_check {
            return;
        }

        self.idk_ms[1] /= 3.0;
        self.idk_ms[2] /= 3.0;
        self.do_other_timing_thing(1.25);
        self.idk_ms[1] *= 3.0;
        self.idk_ms[2] *= 3.0;
    }

    fn complete_seq(&mut self) {
        if self.nah_this_file_aint_for_real > 0 {
            self.max_thingy = if self.nah_this_file_aint_for_real > self.max_thingy {
                self.nah_this_file_aint_for_real
            } else {
                self.max_thingy
            };
        }
        self.nah_this_file_aint_for_real = 0;
    }

    fn bibblybop(&mut self, last_mt: MetaType) {
        if last_mt == MetaType::Enigma {
            self.moving_cv = (self.moving_cv + self.hi_im_a_float) / 2.0;
        } else if last_mt == MetaType::MetaEnigma {
            self.moving_cv = (self.moving_cv + self.hi_im_a_float + self.hi_im_a_float) / 3.0;
        }
        if !self.last_passed_check {
            self.complete_seq();
            return;
        }
        self.nah_this_file_aint_for_real += 1;
        if last_mt == MetaType::Enigma {
            self.nah_this_file_aint_for_real += 1;
        }
        if last_mt == MetaType::MetaEnigma {
            self.nah_this_file_aint_for_real += 2;
        }
    }

    pub fn advance_sequencing(
        &mut self,
        bt: BaseType,
        mt: MetaType,
        last_mt: MetaType,
        any_ms: f32,
        tc_ms: f32,
    ) {
        self.update_seq_ms(bt, any_ms, tc_ms);
        if bt == BaseType::SingleJump || bt == BaseType::JumpSingle {
            return;
        }
        if bt == BaseType::JumpJump {
            if self.nah_this_file_aint_for_real > 0 {
                self.bibblybop(last_mt);
            }
            return;
        }
        match mt {
            MetaType::Acca => self.complete_seq(),
            MetaType::Cccccc => {
                self.handle_roll_timing_check();
                self.bibblybop(last_mt);
            }
            MetaType::Ccacc => {
                self.handle_ccacc_timing_check();
                self.bibblybop(last_mt);
            }
            MetaType::Ccsjjscc | MetaType::CcsjjsccInverted => {
                self.handle_ccsjjscc_timing_check(any_ms);
                self.bibblybop(last_mt);
            }
            MetaType::Init | MetaType::Enigma => {}
            MetaType::MetaEnigma | MetaType::UnknowableEnigma => self.complete_seq(),
        }
    }

    fn update_seq_ms(&mut self, bt: BaseType, any_ms: f32, tc_ms: f32) {
        self.seq_ms[0] = self.seq_ms[1];
        self.seq_ms[1] = self.seq_ms[2];
        self.seq_ms[2] = if bt == BaseType::SingleSingle {
            tc_ms
        } else {
            any_ms
        };
    }

    fn set_pmod(&mut self, itvhi: &ItvHandInfo) {
        if itvhi.get_taps_nowi() == 0
            || itvhi.get_taps_windowi(self.window) == 0
            || self.mw_max.get_total_for_window(self.window) == 0
        {
            self.pmod = NEUTRAL;
            return;
        }
        let zomg =
            itvhi.get_taps_windowf(self.window) / self.mw_max.get_total_for_windowf(self.window);
        self.pmod *= zomg;
        self.pmod = clamp(self.base + fastsqrt(self.pmod), self.min_mod, self.max_mod);
    }

    /// `operator()`.
    pub fn get(&mut self, itvhi: &ItvHandInfo) -> f32 {
        self.max_thingy = if self.nah_this_file_aint_for_real > self.max_thingy {
            self.nah_this_file_aint_for_real
        } else {
            self.max_thingy
        };
        self.mw_max.push(self.max_thingy);
        self.set_pmod(itvhi);
        self.max_thingy = 0;
        self.pmod
    }
}
