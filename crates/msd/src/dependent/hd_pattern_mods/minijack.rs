// Translated to Rust from Etterna's difficulty calculator, MinaCalc:
// https://github.com/etternagame/etterna, src/Etterna/MinaCalc/Dependent/HD_PatternMods/Minijack.h
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

//! Minijacks: a quick jack between slower notes on a column.

use crate::calc_window::{CalcMovingWindow, MAX_MOVING_WINDOW_SIZE};
use crate::dependent::hd_basic_sequencing::ColType;
use crate::dependent::interval_hand_info::ItvHandInfo;
use crate::note_data::pmod;
use crate::pattern_mod_helpers::{NEUTRAL, clamp};
use crate::sequencing_helpers::MS_INIT;

#[derive(Clone, Debug)]
pub struct MinijackMod {
    min_mod: f32,
    max_mod: f32,
    base: f32,
    mj_scaler: f32,
    mj_buffer: f32,

    left_ms: CalcMovingWindow<f32>,
    right_ms: CalcMovingWindow<f32>,
    minijacks: i32,
    pmod: f32,
    left_since_last_right: i32,
    right_since_last_left: i32,
    left_notes: CalcMovingWindow<i32>,
    right_notes: CalcMovingWindow<i32>,
    off_since_last_on: i32,
    off_hand_notes: CalcMovingWindow<i32>,
}

const MINIJACK_SPEED_INCREASE_FACTOR: f32 = 1.9;
const MINIJACK_CONFIRMATION_FACTOR: f32 = 1.3;
const SLOW_MINIJACK_CUTOFF_MS: f32 = 149.5;
const WINDOW: usize = 3;

impl Default for MinijackMod {
    fn default() -> Self {
        MinijackMod {
            min_mod: 1.0,
            max_mod: 1.25,
            base: 0.4,
            mj_scaler: 2.6,
            mj_buffer: 0.3,
            left_ms: CalcMovingWindow::default(),
            right_ms: CalcMovingWindow::default(),
            minijacks: 0,
            pmod: 1.0,
            left_since_last_right: 0,
            right_since_last_left: 0,
            left_notes: CalcMovingWindow::default(),
            right_notes: CalcMovingWindow::default(),
            off_since_last_on: 0,
            off_hand_notes: CalcMovingWindow::default(),
        }
    }
}

impl MinijackMod {
    pub const PMOD: usize = pmod::MINIJACK;

    pub fn full_reset(&mut self) {
        self.pmod = NEUTRAL;
        self.minijacks = 0;
        self.left_ms.fill(MS_INIT);
        self.right_ms.fill(MS_INIT);
        self.left_notes.fill(0);
        self.right_notes.fill(0);
        self.off_hand_notes.fill(0);
        self.left_since_last_right = 0;
        self.right_since_last_left = 0;
        self.off_since_last_on = 0;
    }

    fn minijack_check(
        minijacks: &mut i32,
        off_hand_notes: &CalcMovingWindow<i32>,
        mv: &CalcMovingWindow<f32>,
        mw_off_tap_counts: &CalcMovingWindow<i32>,
    ) {
        let max = mv.get_max_for_window(WINDOW);
        if max != MS_INIT {
            let min = mv.get_min_for_window(WINDOW);
            let recent_ms = mv.at(MAX_MOVING_WINDOW_SIZE - 1);
            let last_ms = mv.at(MAX_MOVING_WINDOW_SIZE - 2);
            let last_last_ms = mv.at(MAX_MOVING_WINDOW_SIZE - 3);
            if last_ms > SLOW_MINIJACK_CUTOFF_MS {
                return;
            }
            if last_ms == min
                && recent_ms > last_ms * MINIJACK_CONFIRMATION_FACTOR
                && last_last_ms > last_ms * MINIJACK_SPEED_INCREASE_FACTOR
                && mw_off_tap_counts.at(MAX_MOVING_WINDOW_SIZE - 2) == 0
                && off_hand_notes.at(MAX_MOVING_WINDOW_SIZE - 2) == 0
            {
                *minijacks += 1;
            }
        }
    }

    pub fn advance_sequencing(&mut self, ct: ColType, ms_now: f32) {
        match ct {
            ColType::Left => {
                if self.right_since_last_left > 0 || self.left_since_last_right > 0 {
                    self.right_notes.push(self.right_since_last_left);
                }
                self.left_since_last_right += 1;
                self.right_since_last_left = 0;
                self.left_ms.push(ms_now);
                self.commit_off_hand_taps();
                Self::minijack_check(
                    &mut self.minijacks,
                    &self.off_hand_notes,
                    &self.left_ms,
                    &self.right_notes,
                );
            }
            ColType::Right => {
                if self.left_since_last_right > 0 || self.right_since_last_left > 0 {
                    self.left_notes.push(self.left_since_last_right);
                }
                self.right_since_last_left += 1;
                self.left_since_last_right = 0;
                self.right_ms.push(ms_now);
                self.commit_off_hand_taps();
                Self::minijack_check(
                    &mut self.minijacks,
                    &self.off_hand_notes,
                    &self.right_ms,
                    &self.left_notes,
                );
            }
            ColType::OhJump => {
                self.left_notes.push(self.left_since_last_right);
                self.right_notes.push(self.right_since_last_left);
                self.left_since_last_right = 0;
                self.right_since_last_left = 0;
                self.left_ms.push(ms_now);
                self.right_ms.push(ms_now);
                self.commit_off_hand_taps();
                Self::minijack_check(
                    &mut self.minijacks,
                    &self.off_hand_notes,
                    &self.left_ms,
                    &self.right_notes,
                );
                Self::minijack_check(
                    &mut self.minijacks,
                    &self.off_hand_notes,
                    &self.right_ms,
                    &self.left_notes,
                );
            }
            _ => {}
        }
    }

    pub fn advance_off_hand_sequencing(&mut self) {
        self.off_since_last_on += 1;
    }

    fn commit_off_hand_taps(&mut self) {
        self.off_hand_notes.push(self.off_since_last_on);
        self.off_since_last_on = 0;
    }

    fn set_pmod(&mut self, itvhi: &ItvHandInfo) {
        if self.minijacks == 0 || itvhi.get_taps_nowi() == 0 {
            self.pmod = NEUTRAL;
            return;
        }
        let mj = (self.minijacks as f32 + self.mj_buffer) * self.mj_scaler;
        let taps = itvhi.get_taps_nowf() - self.mj_buffer;
        self.pmod = self.base + mj / taps;
        self.pmod = clamp(self.pmod, self.min_mod, self.max_mod);
    }

    /// `operator()`.
    pub fn get(&mut self, itvhi: &ItvHandInfo) -> f32 {
        self.set_pmod(itvhi);
        self.minijacks = 0;
        self.pmod
    }
}
