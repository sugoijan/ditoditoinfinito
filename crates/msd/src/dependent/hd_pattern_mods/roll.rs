// Translated to Rust from Etterna's difficulty calculator, MinaCalc:
// https://github.com/etternagame/etterna, src/Etterna/MinaCalc/Dependent/HD_PatternMods/Roll.h
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

//! Rolls: a hand's two columns hit close together.

use crate::dependent::hd_basic_sequencing::ColType;
use crate::dependent::interval_hand_info::ItvHandInfo;
use crate::minacalc::MAX_ROWS_FOR_SINGLE_INTERVAL;
use crate::note_data::pmod;
use crate::pattern_mod_helpers::{NEUTRAL, clamp, max};
use crate::sequencing_helpers::S_INIT;

/// The body `RollMod` and `RollJSMod` share (upstream has two copies,
/// differing in `jj_scaler`).
#[derive(Clone, Debug)]
pub struct RollCore {
    min_mod: f32,
    max_mod: f32,
    base: f32,
    jj_scaler: f32,
    ms_threshold: f32,
    diff_falloff_power: f32,
    required_notes_before_nerf: f32,

    lc: usize,
    rc: usize,
    current_problems: f32,
    pmod: f32,
    left_times: [f32; MAX_ROWS_FOR_SINGLE_INTERVAL],
    right_times: [f32; MAX_ROWS_FOR_SINGLE_INTERVAL],
}

impl RollCore {
    pub fn with_jj_scaler(jj_scaler: f32) -> Self {
        RollCore {
            min_mod: 0.85,
            max_mod: 1.0,
            base: 0.1,
            jj_scaler,
            ms_threshold: 0.0701,
            diff_falloff_power: 1.0,
            required_notes_before_nerf: 6.0,
            lc: 0,
            rc: 0,
            current_problems: 0.0,
            pmod: NEUTRAL,
            left_times: [0.0; MAX_ROWS_FOR_SINGLE_INTERVAL],
            right_times: [0.0; MAX_ROWS_FOR_SINGLE_INTERVAL],
        }
    }

    pub fn full_reset(&mut self) {
        self.left_times = [S_INIT; MAX_ROWS_FOR_SINGLE_INTERVAL];
        self.right_times = [S_INIT; MAX_ROWS_FOR_SINGLE_INTERVAL];
        self.current_problems = 0.0;
        self.lc = 0;
        self.rc = 0;
        self.pmod = NEUTRAL;
    }

    fn check(&mut self) {
        let mut lindex = 0;
        let mut rindex = 0;
        while lindex < self.lc && rindex < self.rc {
            let l = self.left_times[lindex];
            let r = self.right_times[rindex];
            let diff = (l - r).abs();
            if diff <= self.ms_threshold {
                lindex += 1;
                rindex += 1;
                let x = libm::powf(
                    diff / max(self.ms_threshold, 0.00001),
                    self.diff_falloff_power,
                );
                let v = 1.0 + (x / (x - 2.0));
                self.current_problems += v;
            } else if l > r {
                rindex += 1;
            } else if r > l {
                lindex += 1;
            } else {
                lindex += 1;
                rindex += 1;
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
        if itvhi.get_taps_nowi() == 0 || self.current_problems == 0.0 {
            self.pmod = NEUTRAL;
            return;
        }
        if itvhi.get_taps_nowf() < self.required_notes_before_nerf {
            self.pmod = NEUTRAL;
            return;
        }
        self.pmod = itvhi.get_taps_nowf() / ((self.current_problems * 2.0) * self.jj_scaler);
        self.pmod = clamp(self.base + self.pmod, self.min_mod, self.max_mod);
    }

    /// `operator()`.
    pub fn get(&mut self, itvhi: &ItvHandInfo) -> f32 {
        self.check();
        self.set_pmod(itvhi);
        self.interval_end();
        self.pmod
    }

    fn interval_end(&mut self) {
        self.current_problems = 0.0;
        self.left_times = [S_INIT; MAX_ROWS_FOR_SINGLE_INTERVAL];
        self.right_times = [S_INIT; MAX_ROWS_FOR_SINGLE_INTERVAL];
        self.lc = 0;
        self.rc = 0;
    }
}

#[derive(Clone, Debug)]
pub struct RollMod(pub RollCore);

impl Default for RollMod {
    fn default() -> Self {
        RollMod(RollCore::with_jj_scaler(2.5))
    }
}

impl RollMod {
    pub const PMOD: usize = pmod::ROLL;
}
