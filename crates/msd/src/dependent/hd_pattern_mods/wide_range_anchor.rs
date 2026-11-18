// Translated to Rust from Etterna's difficulty calculator, MinaCalc:
// https://github.com/etternagame/etterna, src/Etterna/MinaCalc/Dependent/HD_PatternMods/WideRangeAnchor.h
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

//! The difference between a hand's longest anchors over a few intervals.

use crate::calc_window::MAX_MOVING_WINDOW_SIZE;
use crate::dependent::hd_basic_sequencing::ColType;
use crate::dependent::hd_sequencers::generic_sequencing::AnchorSequencer;
use crate::dependent::interval_hand_info::ItvHandInfo;
use crate::note_data::pmod;
use crate::pattern_mod_helpers::{NEUTRAL, clamp, diff_high_by_low};

#[derive(Clone, Debug)]
pub struct WideRangeAnchorMod {
    window_param: f32,
    min_mod: f32,
    max_mod: f32,
    base: f32,
    diff_min: f32,
    diff_max: f32,
    scaler: f32,
    window: usize,
    a: i32,
    b: i32,
    diff: i32,
    divisor: f32,
    pmod: f32,
}

impl Default for WideRangeAnchorMod {
    fn default() -> Self {
        WideRangeAnchorMod {
            window_param: 2.0,
            min_mod: 1.0,
            max_mod: 1.1,
            base: 1.0,
            diff_min: 4.0,
            diff_max: 16.0,
            scaler: 0.5,
            window: 0,
            a: 0,
            b: 0,
            diff: 0,
            divisor: 0.0,
            pmod: 1.0,
        }
    }
}

impl WideRangeAnchorMod {
    pub const PMOD: usize = pmod::WIDE_RANGE_ANCHOR;

    pub fn full_reset(&mut self) {
        self.interval_end();
        self.pmod = NEUTRAL;
    }

    pub fn setup(&mut self) {
        self.window = clamp(self.window_param as i32, 1, MAX_MOVING_WINDOW_SIZE as i32) as usize;
        self.divisor = (self.diff_max as i32 - self.diff_min as i32) as f32;
        if self.divisor < 0.1 {
            self.divisor = 0.1;
        }
    }

    fn set_pmod(&mut self, as_: &AnchorSequencer) {
        self.a = as_.get_max_for_window_and_col(ColType::Left, self.window);
        self.b = as_.get_max_for_window_and_col(ColType::Right, self.window);
        self.diff = diff_high_by_low(self.a, self.b);
        if self.a == 0 && self.b == 0 {
            self.pmod = NEUTRAL;
            return;
        }
        if self.a == 0 || self.b == 0 {
            self.pmod = self.max_mod;
            return;
        }
        if self.diff <= self.diff_min as i32 {
            self.pmod = self.min_mod;
            return;
        }
        if self.diff > self.diff_max as i32 {
            self.pmod = self.max_mod;
            return;
        }
        self.pmod = self.base + (self.scaler * ((self.diff as f32 - self.diff_min) / self.divisor));
        self.pmod = clamp(self.pmod, self.min_mod, self.max_mod);
    }

    /// `operator()`.
    pub fn get(&mut self, _itvhi: &ItvHandInfo, as_: &AnchorSequencer) -> f32 {
        self.set_pmod(as_);
        self.interval_end();
        self.pmod
    }

    fn interval_end(&mut self) {
        self.diff = 0;
        self.a = 0;
        self.b = 0;
    }
}
