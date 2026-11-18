// Translated to Rust from Etterna's difficulty calculator, MinaCalc:
// https://github.com/etternagame/etterna, src/Etterna/MinaCalc/Dependent/HD_PatternMods/OldAnchorScaler.h
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

//! The old (0.57.1) anchor downscaler, for chordjacks.

use crate::dependent::hd_basic_sequencing::ColType;
use crate::dependent::interval_hand_info::ItvHandInfo;
use crate::note_data::pmod;
use crate::pattern_mod_helpers::{NEUTRAL, clamp, fastsqrt, max, min};

#[derive(Clone, Debug)]
pub struct OldAnchorScalerMod {
    min_mod: f32,
    max_mod: f32,
    extra_divisor: f32,
    pmod: f32,
}

impl Default for OldAnchorScalerMod {
    fn default() -> Self {
        OldAnchorScalerMod {
            min_mod: 0.8,
            max_mod: 1.05,
            extra_divisor: 4.45,
            pmod: NEUTRAL,
        }
    }
}

impl OldAnchorScalerMod {
    pub const PMOD: usize = pmod::OLD_ANCHOR_SCALER;

    pub fn full_reset(&mut self) {
        self.pmod = NEUTRAL;
    }

    /// `operator()`.
    pub fn get(&mut self, itvhi: &ItvHandInfo) -> f32 {
        if itvhi.get_col_taps_nowi(ColType::Left) == 0
            || itvhi.get_col_taps_nowi(ColType::Right) == 0
        {
            self.pmod = NEUTRAL;
        } else {
            let l = itvhi.get_col_taps_nowf(ColType::Left);
            let r = itvhi.get_col_taps_nowf(ColType::Right);
            let minval = min(l, r);
            let maxval = max(l, r);
            self.pmod = fastsqrt(1.0 - (minval / maxval / self.extra_divisor));
        }
        self.pmod = clamp(self.pmod, self.min_mod, self.max_mod);
        self.pmod
    }
}
