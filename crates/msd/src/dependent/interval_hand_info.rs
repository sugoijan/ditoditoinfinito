// Translated to Rust from Etterna's difficulty calculator, MinaCalc:
// https://github.com/etternagame/etterna, src/Etterna/MinaCalc/Dependent/IntervalHandInfo.h
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

//! A hand's tap counts across an interval, and moving windows of them.

use super::hd_basic_sequencing::{CT_LOOP, ColType, NUM_COL_TYPES};
use crate::calc_window::CalcMovingWindow;
use crate::pattern_mod_helpers::{diff_high_by_low, div_high_by_low, div_low_by_high};

#[derive(Clone, Debug, Default)]
pub struct ItvHandInfo {
    col_taps: [i32; NUM_COL_TYPES],
    mw_col_taps: [CalcMovingWindow<i32>; NUM_COL_TYPES],
    mw_hand_taps: CalcMovingWindow<i32>,
}

impl ItvHandInfo {
    pub fn set_col_taps(&mut self, col: ColType) {
        match col {
            ColType::Left | ColType::Right => self.col_taps[col.idx()] += 1,
            ColType::OhJump => {
                self.col_taps[ColType::Left.idx()] += 1;
                self.col_taps[ColType::Right.idx()] += 1;
                self.col_taps[col.idx()] += 2;
            }
            _ => {}
        }
    }

    pub fn interval_end(&mut self) {
        self.mw_hand_taps
            .push(self.col_taps[ColType::Left.idx()] + self.col_taps[ColType::Right.idx()]);
        for ct in CT_LOOP {
            self.mw_col_taps[ct.idx()].push(self.col_taps[ct.idx()]);
        }
        self.col_taps = [0; NUM_COL_TYPES];
    }

    pub fn zero(&mut self) {
        self.col_taps = [0; NUM_COL_TYPES];
        for mw in &mut self.mw_col_taps {
            mw.zero();
        }
        self.mw_hand_taps.zero();
    }

    pub fn get_col_taps_nowi(&self, ct: ColType) -> i32 {
        self.mw_col_taps[ct.idx()].get_now()
    }

    pub fn get_col_taps_nowf(&self, ct: ColType) -> f32 {
        self.mw_col_taps[ct.idx()].get_now() as f32
    }

    pub fn get_col_taps_windowi(&self, ct: ColType, window: usize) -> i32 {
        self.mw_col_taps[ct.idx()].get_total_for_window(window)
    }

    pub fn get_col_taps_windowf(&self, ct: ColType, window: usize) -> f32 {
        self.mw_col_taps[ct.idx()].get_total_for_window(window) as f32
    }

    pub fn cols_equal_now(&self) -> bool {
        self.get_col_taps_nowi(ColType::Left) == self.get_col_taps_nowi(ColType::Right)
    }

    pub fn cols_equal_window(&self, window: usize) -> bool {
        self.get_col_taps_windowi(ColType::Left, window)
            == self.get_col_taps_windowi(ColType::Right, window)
    }

    pub fn get_col_prop_high_by_low(&self) -> f32 {
        div_high_by_low(
            self.get_col_taps_nowf(ColType::Left),
            self.get_col_taps_nowf(ColType::Right),
        )
    }

    pub fn get_col_prop_low_by_high(&self) -> f32 {
        div_low_by_high(
            self.get_col_taps_nowf(ColType::Left),
            self.get_col_taps_nowf(ColType::Right),
        )
    }

    pub fn get_col_prop_low_by_high_window(&self, window: usize) -> f32 {
        div_low_by_high(
            self.get_col_taps_windowf(ColType::Left, window),
            self.get_col_taps_windowf(ColType::Right, window),
        )
    }

    pub fn get_col_diff_high_by_low(&self) -> i32 {
        diff_high_by_low(
            self.get_col_taps_nowi(ColType::Left),
            self.get_col_taps_nowi(ColType::Right),
        )
    }

    pub fn get_taps_nowi(&self) -> i32 {
        self.mw_hand_taps.get_now()
    }

    pub fn get_taps_nowf(&self) -> f32 {
        self.mw_hand_taps.get_now() as f32
    }

    pub fn get_taps_windowi(&self, window: usize) -> i32 {
        self.mw_hand_taps.get_total_for_window(window)
    }

    pub fn get_taps_windowf(&self, window: usize) -> f32 {
        self.mw_hand_taps.get_total_for_window(window) as f32
    }
}
