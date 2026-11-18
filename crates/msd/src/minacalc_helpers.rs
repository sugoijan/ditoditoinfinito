// Translated to Rust from Etterna's difficulty calculator, MinaCalc:
// https://github.com/etternagame/etterna, src/Etterna/MinaCalc/MinaCalcHelpers.h
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

//! Definitions only the calculator's driver needs.

use crate::pattern_mod_helpers::{max, min};

pub const MAX_RATING: f32 = 100.0;
pub const MIN_RATING: f32 = 0.0;
pub const DEFAULT_SCORE_GOAL: f32 = 0.93;
pub const LOW_ACC_CUTOFF: f32 = 0.9;
pub const SSR_GOAL_CAP: f32 = 0.965;

/// `dimples_the_all_zero_output`.
pub const ALL_ZERO_OUTPUT: [f32; 8] = [MIN_RATING; 8];

pub fn downscale_low_accuracy_scores(f: f32, sg: f32) -> f32 {
    if sg >= LOW_ACC_CUTOFF {
        f
    } else {
        min(
            max(
                f / libm::powf(1.0 + (LOW_ACC_CUTOFF - sg), 3.25),
                MIN_RATING,
            ),
            MAX_RATING,
        )
    }
}

/// The overall rating from the skillsets: roughly a binary search, eleven
/// rounds, in upstream's mix of `float` and `double`.
pub fn aggregate_skill(
    v: &[f32],
    delta_multiplier: f64,
    result_multiplier: f32,
    mut rating: f32,
    mut resolution: f32,
) -> f32 {
    for _ in 0..11 {
        loop {
            rating += resolution;
            let mut sum = 0.0f64;
            for &vv in v {
                sum += max(
                    0.0f64,
                    2.0 / libm::erfc(delta_multiplier * f64::from(vv - rating)) - 2.0,
                );
            }
            if !(libm::pow(2.0, f64::from(rating) * 0.1) < sum) {
                break;
            }
        }
        rating -= resolution;
        resolution /= 2.0;
    }
    rating += resolution * 2.0;
    rating * result_multiplier
}
