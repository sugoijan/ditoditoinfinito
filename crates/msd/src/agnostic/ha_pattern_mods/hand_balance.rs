// Translated to Rust from Etterna's difficulty calculator, MinaCalc:
// https://github.com/etternagame/etterna, src/Etterna/MinaCalc/Agnostic/HA_PatternMods/HandBalance.h
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

//! The balance of notes across the hands.

use crate::note_data::pmod;
use crate::pattern_mod_helpers::{NEUTRAL, clamp, fastpow};

#[derive(Clone, Debug)]
pub struct HandBalanceMod {
    min_mod: f32,
    max_mod: f32,
    power_scaling: f32,
    pmod: f32,
    left_hand: f32,
    right_hand: f32,
}

impl Default for HandBalanceMod {
    fn default() -> Self {
        HandBalanceMod {
            min_mod: 0.5,
            max_mod: 1.0,
            power_scaling: 0.5,
            pmod: NEUTRAL,
            left_hand: 0.0,
            right_hand: 0.0,
        }
    }
}

impl HandBalanceMod {
    pub const PMOD: usize = pmod::HAND_BALANCE;

    pub fn full_reset(&mut self) {
        self.pmod = NEUTRAL;
        self.interval_end();
    }

    pub fn advance_sequencing(&mut self, notes: u32, hand_col_masks: [u32; 2]) {
        self.left_hand += (notes & hand_col_masks[0]).count_ones() as f32;
        self.right_hand += (notes & hand_col_masks[1]).count_ones() as f32;
    }

    fn interval_end(&mut self) {
        self.left_hand = 0.0;
        self.right_hand = 0.0;
    }

    /// `operator()`.
    pub fn get(&mut self) -> f32 {
        if self.left_hand == 0.0 && self.right_hand == 0.0 {
            return NEUTRAL;
        }
        let diff =
            (self.left_hand - self.right_hand).abs() / ((self.left_hand + self.right_hand) / 2.0);
        let prop = fastpow(f64::from(1.0 - diff), f64::from(self.power_scaling));
        self.pmod = clamp(prop, self.min_mod, self.max_mod);
        self.interval_end();
        self.pmod
    }
}
