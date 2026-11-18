// Translated to Rust from Etterna's difficulty calculator, MinaCalc:
// https://github.com/etternagame/etterna, src/Etterna/MinaCalc/Dependent/HD_PatternMods/WideRangeCJAnchor.h
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

//! Long anchors in chordjacks over a few intervals.

use crate::dependent::hd_basic_sequencing::ColType;
use crate::dependent::hd_sequencers::generic_sequencing::AnchorSequencer;
use crate::dependent::interval_hand_info::ItvHandInfo;
use crate::note_data::pmod;
use crate::pattern_mod_helpers::{NEUTRAL, clamp, fastpow, fastsqrt};

#[derive(Clone, Debug)]
pub struct WideRangeCjAnchorMod {
    min_mod: f32,
    max_mod: f32,
    len_3_weight: f32,
    len_long_weight: f32,
    min_required_taps: f32,
    moving_window_intervals: f32,
    long_anchor_pow_base: f32,
    pmod: f32,
}

impl Default for WideRangeCjAnchorMod {
    fn default() -> Self {
        WideRangeCjAnchorMod {
            min_mod: 0.5,
            max_mod: 1.0,
            len_3_weight: 0.01,
            len_long_weight: -0.0034,
            min_required_taps: 7.0,
            moving_window_intervals: 2.0,
            long_anchor_pow_base: 1.025,
            pmod: 0.5,
        }
    }
}

impl WideRangeCjAnchorMod {
    pub const PMOD: usize = pmod::WIDE_RANGE_CJ_ANCHOR;

    pub fn full_reset(&mut self) {
        self.pmod = NEUTRAL;
    }

    pub fn setup(&mut self) {}

    fn set_pmod(&mut self, itvhi: &ItvHandInfo, as_: &AnchorSequencer) {
        if itvhi.get_taps_nowf() < self.min_required_taps {
            self.pmod = fastsqrt(self.pmod);
            return;
        }
        let window = self.moving_window_intervals as i32 as usize;
        let left = as_.get_max_for_window_and_col(ColType::Left, window);
        let right = as_.get_max_for_window_and_col(ColType::Right, window);
        let anchorest_value = if left > right { left } else { right };
        if anchorest_value < 3 {
            self.pmod = fastsqrt(self.pmod);
        } else if anchorest_value == 3 {
            self.pmod = fastsqrt(self.pmod) + self.len_3_weight;
        } else {
            self.pmod += fastpow(
                f64::from(self.long_anchor_pow_base),
                f64::from(anchorest_value),
            ) * self.len_long_weight;
        }
    }

    /// `operator()`.
    pub fn get(&mut self, itvhi: &ItvHandInfo, as_: &AnchorSequencer) -> f32 {
        self.set_pmod(itvhi, as_);
        self.pmod = clamp(self.pmod, self.min_mod, self.max_mod);
        self.pmod
    }
}
