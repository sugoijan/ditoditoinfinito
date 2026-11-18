// Translated to Rust from Etterna's difficulty calculator, MinaCalc:
// https://github.com/etternagame/etterna, src/Etterna/MinaCalc/Agnostic/HA_Sequencers/FlamSequencing.h
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

//! Flams: two to four rows close enough to be hit as a chord.

use crate::pattern_mod_helpers::{clamp, fastsqrt};

pub const MAX_FLAM_JAMMIES: i32 = 4;

#[derive(Clone, Debug)]
pub struct Flam {
    /// Columns seen.
    pub unsigned_unseen: u32,
    /// Size in rows; 1 means no flam yet.
    pub size: i32,
    pub flammin: bool,
    pub ms: [f32; 3],
}

impl Default for Flam {
    fn default() -> Self {
        Flam {
            unsigned_unseen: 0,
            size: 1,
            flammin: false,
            ms: [0.0; 3],
        }
    }
}

impl Flam {
    /// Is this row exclusively additive with the current flam?
    pub fn comma_comma_coolmeleon(&self, notes: u32) -> bool {
        (self.unsigned_unseen & notes) == 0
    }

    /// Cumulative milliseconds of the whole flam. Upstream asserts on a
    /// size of 1 and falls through to 2 without assertions.
    pub fn get_dur(&self) -> f32 {
        match self.size {
            1 | 2 => self.ms[0],
            3 => self.ms[0] + self.ms[1],
            4 => self.ms[0] + self.ms[1] + self.ms[2],
            _ => 0.0,
        }
    }

    pub fn start(&mut self, ms_now: f32, notes: u32) {
        self.flammin = true;
        self.unsigned_unseen = 0;
        self.grow(ms_now, notes);
    }

    pub fn grow(&mut self, ms_now: f32, notes: u32) {
        if self.size == MAX_FLAM_JAMMIES {
            return;
        }
        self.unsigned_unseen |= notes;
        self.ms[(self.size - 1) as usize] = ms_now;
        self.size += 1;
    }

    pub fn reset(&mut self) {
        self.flammin = false;
        self.size = 1;
    }
}

/// `FJ_Sequencer`: up to four flams an interval.
#[derive(Clone, Debug)]
pub struct FjSequencer {
    pub flim: Flam,
    pub group_tol: f32,
    pub step_tol: f32,
    pub mod_scaler: f32,
    pub flam_counter: i32,
    pub mod_parts: [f32; 4],
    pub the_fifth_flammament: bool,
}

impl Default for FjSequencer {
    fn default() -> Self {
        FjSequencer {
            flim: Flam::default(),
            group_tol: 0.0,
            step_tol: 0.0,
            mod_scaler: 0.0,
            flam_counter: 0,
            mod_parts: [1.0; 4],
            the_fifth_flammament: false,
        }
    }
}

impl FjSequencer {
    pub fn set_params(&mut self, gt: f32, st: f32, ms: f32) {
        self.group_tol = gt;
        self.step_tol = st;
        self.mod_scaler = ms;
    }

    fn complete_seq(&mut self) {
        if self.flam_counter < MAX_FLAM_JAMMIES {
            self.mod_parts[self.flam_counter as usize] = self.construct_mod_part();
            self.flam_counter += 1;
        } else {
            self.the_fifth_flammament = true;
        }
        self.flim.reset();
    }

    fn flammin_col_check(&self, notes: u32) -> bool {
        self.flim.comma_comma_coolmeleon(notes)
    }

    fn flammin_tol_check(&self, ms_now: f32) -> bool {
        if ms_now > self.group_tol {
            return false;
        }
        if self.flim.get_dur() + ms_now > self.group_tol {
            return false;
        }
        true
    }

    /// `operator()`.
    pub fn advance(&mut self, ms_now: f32, notes: u32) {
        if self.the_fifth_flammament {
            return;
        }
        if !self.flim.flammin {
            if ms_now > self.step_tol {
                return;
            }
            self.flim.start(ms_now, notes);
        } else if self.flammin_tol_check(ms_now) {
            if self.flammin_col_check(notes) {
                self.flim.grow(ms_now, notes);
            } else {
                self.complete_seq();
                self.flim.start(ms_now, notes);
            }
        } else {
            self.complete_seq();
        }
    }

    pub fn handle_interval_end(&mut self) {
        self.the_fifth_flammament = false;
        self.flam_counter = 0;
        self.mod_parts = [1.0; 4];
    }

    fn construct_mod_part(&self) -> f32 {
        let dur = self.flim.get_dur();
        let mut dur_prop = dur / self.group_tol;
        dur_prop /= self.flim.size as f32 / self.mod_scaler;
        dur_prop = clamp(dur_prop, 0.0, 1.0);
        fastsqrt(dur_prop)
    }
}
