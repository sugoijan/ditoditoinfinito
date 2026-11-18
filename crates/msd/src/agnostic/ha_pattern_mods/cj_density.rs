// Translated to Rust from Etterna's difficulty calculator, MinaCalc:
// https://github.com/etternagame/etterna, src/Etterna/MinaCalc/Agnostic/HA_PatternMods/CJDensity.h
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

//! Chord density: chords of each size relative to the interval's notes.

use crate::agnostic::interval_info::tap_size;
use crate::agnostic::meta_interval_info::MetaItvInfo;
use crate::note_data::pmod;
use crate::pattern_mod_helpers::{NEUTRAL, clamp};

#[derive(Clone, Debug)]
pub struct CjDensityMod {
    min_mod: f32,
    max_mod: f32,
    base: f32,
    single_scaler: f32,
    jump_scaler: f32,
    hand_scaler: f32,
    quad_scaler: f32,
    pmod: f32,
}

impl Default for CjDensityMod {
    fn default() -> Self {
        CjDensityMod {
            min_mod: 0.7,
            max_mod: 1.0,
            base: 0.0,
            single_scaler: 1.0,
            jump_scaler: 1.0,
            hand_scaler: 0.95,
            quad_scaler: 0.75,
            pmod: NEUTRAL,
        }
    }
}

impl CjDensityMod {
    pub const PMOD: usize = pmod::CJ_DENSITY;

    /// `operator()`.
    pub fn get(&mut self, mitvi: &MetaItvInfo) -> f32 {
        let itvi = &mitvi.itvi;
        if itvi.total_taps == 0 {
            return NEUTRAL;
        }
        let t_taps = itvi.total_taps as f32;
        let a0 = (itvi.taps_by_size[tap_size::SINGLE] as f32 * self.single_scaler) / t_taps;
        let a1 = (itvi.taps_by_size[tap_size::JUMP] as f32 * self.jump_scaler) / t_taps;
        let a2 = (itvi.taps_by_size[tap_size::HAND] as f32 * self.hand_scaler) / t_taps;
        let a3 = (itvi.taps_by_size[tap_size::QUAD] as f32 * self.quad_scaler) / t_taps;
        let aaa = a0 + a1 + a2 + a3;
        self.pmod = clamp(self.base + aaa, self.min_mod, self.max_mod);
        self.pmod
    }
}
