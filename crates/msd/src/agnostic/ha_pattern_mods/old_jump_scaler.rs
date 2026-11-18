// Translated to Rust from Etterna's difficulty calculator, MinaCalc:
// https://github.com/etternagame/etterna, src/Etterna/MinaCalc/Agnostic/HA_PatternMods/OldJumpScaler.h
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

//! Downscaler by how many jumps are in the interval.

use crate::agnostic::interval_info::tap_size;
use crate::agnostic::meta_interval_info::MetaItvInfo;
use crate::note_data::pmod;
use crate::pattern_mod_helpers::{NEUTRAL, clamp, fastsqrt};

#[derive(Clone, Debug)]
pub struct OldJumpScalerMod {
    min_mod: f32,
    max_mod: f32,
    extra_divisor: f32,
    pmod: f32,
}

impl Default for OldJumpScalerMod {
    fn default() -> Self {
        OldJumpScalerMod {
            min_mod: 0.6,
            max_mod: 1.1,
            extra_divisor: 6.0,
            pmod: NEUTRAL,
        }
    }
}

impl OldJumpScalerMod {
    pub const PMOD: usize = pmod::OLD_JUMP_SCALER;

    pub fn full_reset(&mut self) {
        self.pmod = NEUTRAL;
    }

    fn set_pmod(&mut self, mitvi: &MetaItvInfo) {
        let itvhi = &mitvi.itvi;
        if itvhi.total_taps == 0 {
            self.pmod = NEUTRAL;
        } else {
            let taps = itvhi.total_taps as f32;
            let jumps = itvhi.taps_by_size[tap_size::JUMP];
            self.pmod = fastsqrt(fastsqrt(1.0 - (jumps as f32 / taps / self.extra_divisor)));
        }
        self.pmod = clamp(self.pmod, self.min_mod, self.max_mod);
    }

    /// `operator()`.
    pub fn get(&mut self, mitvi: &MetaItvInfo) -> f32 {
        self.set_pmod(mitvi);
        self.pmod
    }
}
