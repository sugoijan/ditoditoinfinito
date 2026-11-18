// Translated to Rust from Etterna's difficulty calculator, MinaCalc:
// https://github.com/etternagame/etterna, src/Etterna/MinaCalc/Agnostic/HA_PatternMods/FlamJam.h
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

//! Continuous flams: taps close enough to be hit as a chord.

use crate::agnostic::ha_sequencers::flam_sequencing::FjSequencer;
use crate::note_data::pmod;
use crate::pattern_mod_helpers::{NEUTRAL, clamp};

#[derive(Clone, Debug)]
pub struct FlamJamMod {
    min_mod: f32,
    max_mod: f32,
    scaler: f32,
    base: f32,
    group_tol: f32,
    step_tol: f32,
    pub fj: FjSequencer,
    pmod: f32,
}

impl Default for FlamJamMod {
    fn default() -> Self {
        FlamJamMod {
            min_mod: 0.3,
            max_mod: 1.0,
            scaler: 0.001,
            base: 0.5,
            group_tol: 35.0,
            step_tol: 17.5,
            fj: FjSequencer::default(),
            pmod: NEUTRAL,
        }
    }
}

impl FlamJamMod {
    pub const PMOD: usize = pmod::FLAM_JAM;

    pub fn setup(&mut self) {
        self.fj
            .set_params(self.group_tol, self.step_tol, self.scaler);
    }

    pub fn advance_sequencing(&mut self, ms_now: f32, notes: u32) {
        self.fj.advance(ms_now, notes);
    }

    /// `operator()`. Upstream resets the sequencer's interval only when it
    /// found a flam.
    pub fn get(&mut self) -> f32 {
        if self.fj.mod_parts[0] == 1.0 {
            return NEUTRAL;
        }
        self.pmod = 1.0;
        for mp in self.fj.mod_parts {
            self.pmod += mp;
        }
        self.pmod /= 5.0;
        self.pmod = clamp(self.base + self.pmod, self.min_mod, self.max_mod);
        self.fj.handle_interval_end();
        self.pmod
    }
}
