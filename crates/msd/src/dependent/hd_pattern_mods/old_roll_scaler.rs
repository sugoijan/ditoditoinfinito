// Translated to Rust from Etterna's difficulty calculator, MinaCalc:
// https://github.com/etternagame/etterna, src/Etterna/MinaCalc/Dependent/HD_PatternMods/OldRollScaler.h
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

//! The old (0.57.1) roll downscaler, for chordjacks.

use crate::dependent::hd_basic_sequencing::ColType;
use crate::dependent::interval_hand_info::ItvHandInfo;
use crate::note_data::pmod;
use crate::pattern_mod_helpers::{NEUTRAL, clamp, cv, fastpow, fastsqrt, mean};

#[derive(Clone, Debug)]
pub struct OldRollScalerMod {
    min_mod: f32,
    max_mod: f32,
    use_cv_threshold: f32,
    min_cv: f32,
    cv_base: f32,
    power_scale: f32,
    pmod: f32,
    all_ms: Vec<f32>,
    left_empty: bool,
    right_empty: bool,
}

impl Default for OldRollScalerMod {
    fn default() -> Self {
        OldRollScalerMod {
            min_mod: 0.9,
            max_mod: 1.075,
            use_cv_threshold: 0.6,
            min_cv: 0.15,
            cv_base: 0.85,
            power_scale: 6.0,
            pmod: NEUTRAL,
            all_ms: Vec::new(),
            left_empty: true,
            right_empty: true,
        }
    }
}

impl OldRollScalerMod {
    pub const PMOD: usize = pmod::OLD_ROLL_SCALER;

    pub fn full_reset(&mut self) {
        self.pmod = NEUTRAL;
        self.interval_end();
    }

    fn interval_end(&mut self) {
        self.all_ms.clear();
        self.left_empty = true;
        self.right_empty = true;
    }

    pub fn advance_sequencing(&mut self, ct: ColType, any_ms: f32) {
        match ct {
            ColType::Left => {
                self.left_empty = false;
                self.all_ms.push(any_ms);
            }
            ColType::Right => {
                self.right_empty = false;
                self.all_ms.push(any_ms);
            }
            ColType::OhJump => {
                self.all_ms.push(any_ms);
                self.all_ms.push(any_ms);
                self.left_empty = false;
                self.right_empty = false;
            }
            _ => {}
        }
    }

    fn set_pmod(&mut self) {
        if self.left_empty || self.right_empty || self.all_ms.len() == 1 {
            self.pmod = NEUTRAL;
        } else {
            let mmm = mean(&self.all_ms);
            for x in &mut self.all_ms {
                *x = if mmm / *x < self.use_cv_threshold {
                    mmm
                } else {
                    *x
                };
            }
            let dacv = cv(&self.all_ms);
            if dacv >= self.min_cv {
                self.pmod = fastsqrt(fastsqrt(self.cv_base + dacv));
            } else {
                self.pmod = fastpow(f64::from(self.cv_base + dacv), f64::from(self.power_scale));
            }
        }
        self.pmod = clamp(self.pmod, self.min_mod, self.max_mod);
    }

    /// `operator()`.
    pub fn get(&mut self, _itvhi: &ItvHandInfo) -> f32 {
        self.set_pmod();
        self.interval_end();
        self.pmod
    }
}
