// Translated to Rust from Etterna's difficulty calculator, MinaCalc:
// https://github.com/etternagame/etterna, src/Etterna/MinaCalc/Dependent/HD_PatternMods/Balance.h
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

//! The balance between a hand's two columns.

use crate::dependent::hd_basic_sequencing::ColType;
use crate::dependent::interval_hand_info::ItvHandInfo;
use crate::note_data::pmod;
use crate::pattern_mod_helpers::{NEUTRAL, clamp};

#[derive(Clone, Debug)]
pub struct BalanceMod {
    min_mod: f32,
    max_mod: f32,
    mod_base: f32,
    buffer: f32,
    scaler: f32,
    other_scaler: f32,
    pmod: f32,
}

impl Default for BalanceMod {
    fn default() -> Self {
        BalanceMod {
            min_mod: 0.95,
            max_mod: 1.05,
            mod_base: 0.325,
            buffer: 1.0,
            scaler: 1.0,
            other_scaler: 4.0,
            pmod: NEUTRAL,
        }
    }
}

impl BalanceMod {
    pub const PMOD: usize = pmod::BALANCE;

    pub fn full_reset(&mut self) {
        self.pmod = NEUTRAL;
    }

    /// `operator()`.
    pub fn get(&mut self, itvhi: &ItvHandInfo) -> f32 {
        if itvhi.get_taps_nowi() == 0 {
            return NEUTRAL;
        }
        if itvhi.cols_equal_now() {
            return self.min_mod;
        }
        if itvhi.get_col_taps_nowi(ColType::Left) == 0
            || itvhi.get_col_taps_nowi(ColType::Right) == 0
        {
            return self.max_mod;
        }
        self.pmod = itvhi.get_col_prop_low_by_high();
        self.pmod = self.mod_base + (self.buffer + (self.scaler / self.pmod)) / self.other_scaler;
        self.pmod = clamp(self.pmod, self.min_mod, self.max_mod);
        self.pmod
    }
}
