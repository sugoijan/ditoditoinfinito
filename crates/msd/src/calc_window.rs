// Translated to Rust from Etterna's difficulty calculator, MinaCalc:
// https://github.com/etternagame/etterna, src/Etterna/MinaCalc/CalcWindow.h
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

//! A moving window of the last six values, with the statistics the
//! pattern mods ask of it.

use crate::pattern_mod_helpers::fastsqrt;
use crate::sequencing_helpers::any_ms_is_greater;

pub const MAX_MOVING_WINDOW_SIZE: usize = 6;

/// Applies to acca and cccccc as well.
pub const CCACC_TIMING_CHECK_SIZE: usize = 3;

/// What a window holds: `float` or `int`.
pub trait WindowValue: Copy + PartialOrd + std::ops::Add<Output = Self> {
    const ZERO: Self;
    fn as_f32(self) -> f32;
}

impl WindowValue for f32 {
    const ZERO: f32 = 0.0;
    #[inline]
    fn as_f32(self) -> f32 {
        self
    }
}

impl WindowValue for i32 {
    const ZERO: i32 = 0;
    #[inline]
    fn as_f32(self) -> f32 {
        self as f32
    }
}

/// `CalcMovingWindow`: the oldest value first, the newest last.
#[derive(Clone, Copy, Debug)]
pub struct CalcMovingWindow<T> {
    itv_vals: [T; MAX_MOVING_WINDOW_SIZE],
}

impl<T: WindowValue> Default for CalcMovingWindow<T> {
    fn default() -> Self {
        CalcMovingWindow {
            itv_vals: [T::ZERO; MAX_MOVING_WINDOW_SIZE],
        }
    }
}

impl<T: WindowValue> CalcMovingWindow<T> {
    /// `operator()`: push a new value, the oldest out.
    #[inline]
    pub fn push(&mut self, new_val: T) {
        for i in 1..MAX_MOVING_WINDOW_SIZE {
            self.itv_vals[i - 1] = self.itv_vals[i];
        }
        self.itv_vals[MAX_MOVING_WINDOW_SIZE - 1] = new_val;
    }

    /// `operator[]`.
    #[inline]
    pub fn at(&self, pos: usize) -> T {
        self.itv_vals[pos]
    }

    #[inline]
    pub fn get_now(&self) -> T {
        self.itv_vals[MAX_MOVING_WINDOW_SIZE - 1]
    }

    #[inline]
    pub fn get_last(&self) -> T {
        self.itv_vals[MAX_MOVING_WINDOW_SIZE - 2]
    }

    pub fn get_total_for_window(&self, window: usize) -> T {
        let mut o = T::ZERO;
        let mut i = MAX_MOVING_WINDOW_SIZE;
        while i > MAX_MOVING_WINDOW_SIZE - window {
            i -= 1;
            o = o + self.itv_vals[i];
        }
        o
    }

    pub fn get_max_for_window(&self, window: usize) -> T {
        let mut o = T::ZERO;
        let mut i = MAX_MOVING_WINDOW_SIZE;
        while i > MAX_MOVING_WINDOW_SIZE - window {
            i -= 1;
            o = if self.itv_vals[i] > o {
                self.itv_vals[i]
            } else {
                o
            };
        }
        o
    }

    pub fn get_min_for_window(&self, window: usize) -> T {
        let mut o = self.get_now();
        let mut i = MAX_MOVING_WINDOW_SIZE;
        while i > MAX_MOVING_WINDOW_SIZE - window {
            i -= 1;
            o = if self.itv_vals[i] < o {
                self.itv_vals[i]
            } else {
                o
            };
        }
        o
    }

    pub fn get_mean_of_window(&self, window: usize) -> f32 {
        self.get_total_for_window(window).as_f32() / window as f32
    }

    pub fn get_total_for_windowf(&self, window: usize) -> f32 {
        let mut o = 0.0f32;
        let mut i = MAX_MOVING_WINDOW_SIZE;
        while i > MAX_MOVING_WINDOW_SIZE - window {
            i -= 1;
            o += self.itv_vals[i].as_f32();
        }
        o
    }

    pub fn get_cv_of_window(&self, window: usize) -> f32 {
        let mut sd = 0.0f32;
        let avg = self.get_mean_of_window(window);
        let mut i = MAX_MOVING_WINDOW_SIZE;
        while i > MAX_MOVING_WINDOW_SIZE - window {
            i -= 1;
            let v = self.itv_vals[i].as_f32();
            sd += (v - avg) * (v - avg);
        }
        fastsqrt(sd / window as f32) / avg
    }

    pub fn zero(&mut self) {
        self.itv_vals = [T::ZERO; MAX_MOVING_WINDOW_SIZE];
    }

    pub fn fill(&mut self, val: T) {
        self.itv_vals = [val; MAX_MOVING_WINDOW_SIZE];
    }
}

impl CalcMovingWindow<f32> {
    /// The anchor in the center divided by `factor` for a cv check. The
    /// value put back may differ in its last bit, as upstream's does.
    pub fn ccacc_timing_check(&mut self, factor: f32, threshold: f32) -> bool {
        self.itv_vals[4] /= factor;
        let o = self.get_cv_of_window(CCACC_TIMING_CHECK_SIZE);
        self.itv_vals[4] *= factor;
        o < threshold
    }

    /// The cross column in the center multiplied by `factor`.
    pub fn acca_timing_check(&mut self, factor: f32, threshold: f32) -> bool {
        self.itv_vals[4] *= factor;
        let o = self.get_cv_of_window(CCACC_TIMING_CHECK_SIZE);
        self.itv_vals[4] /= factor;
        o < threshold
    }

    pub fn roll_timing_check(&mut self, factor: f32, threshold: f32) -> bool {
        if any_ms_is_greater(self.itv_vals[4], self.itv_vals[5]) {
            self.ccacc_timing_check(factor, threshold)
        } else {
            self.acca_timing_check(factor, threshold)
        }
    }
}
