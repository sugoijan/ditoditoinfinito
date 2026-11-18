// Translated to Rust from Etterna's difficulty calculator, MinaCalc:
// https://github.com/etternagame/etterna, src/Etterna/MinaCalc/Dependent/HD_PatternMods/Chaos.h
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

//! How far successive gaps are from simple ratios.

use crate::calc_window::{CalcMovingWindow, MAX_MOVING_WINDOW_SIZE};
use crate::note_data::pmod;
use crate::pattern_mod_helpers::{NEUTRAL, clamp, div_high_by_low};
use crate::sequencing_helpers::{any_ms_is_close, any_ms_is_zero};

#[derive(Clone, Debug)]
pub struct ChaosMod {
    min_mod: f32,
    max_mod: f32,
    base: f32,
    u: CalcMovingWindow<f32>,
    wot: CalcMovingWindow<f32>,
    pmod: f32,
}

impl Default for ChaosMod {
    fn default() -> Self {
        ChaosMod {
            min_mod: 0.88,
            max_mod: 1.07,
            base: -0.088,
            u: CalcMovingWindow::default(),
            wot: CalcMovingWindow::default(),
            pmod: NEUTRAL,
        }
    }
}

impl ChaosMod {
    pub const PMOD: usize = pmod::CHAOS;
    const WINDOW: usize = 6;

    pub fn full_reset(&mut self) {
        self.u.zero();
        self.wot.zero();
        self.pmod = NEUTRAL;
    }

    pub fn advance_sequencing(&mut self, ms_any: &CalcMovingWindow<f32>) {
        let a = ms_any.get_now();
        let b = ms_any.get_last();
        if any_ms_is_zero(a) || any_ms_is_zero(b) || any_ms_is_close(a, b) {
            self.u.push(1.0);
            self.wot.push(self.u.get_mean_of_window(Self::WINDOW));
            return;
        }
        let prop = div_high_by_low(a, b);
        let mop = prop as i32;
        let mut flop = prop - mop as f32;
        if flop == 0.0 {
            flop = 1.0;
        } else if flop >= 0.5 {
            // `abs` of a float: the float overload, as libc++ and MSVC
            // resolve upstream's unqualified call (README.md).
            flop = (flop - 1.0).abs() + 1.0;
        } else if flop < 0.5 {
            flop += 1.0;
        }
        self.u.push(flop);
        self.wot.push(self.u.get_mean_of_window(Self::WINDOW));
    }

    /// `operator()`.
    pub fn get(&mut self, total_taps: i32) -> f32 {
        if total_taps == 0 {
            return NEUTRAL;
        }
        self.pmod = self.base + self.wot.get_mean_of_window(MAX_MOVING_WINDOW_SIZE);
        self.pmod = clamp(self.pmod, self.min_mod, self.max_mod);
        self.pmod
    }
}
