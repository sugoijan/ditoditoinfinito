// Translated to Rust from Etterna's difficulty calculator, MinaCalc:
// https://github.com/etternagame/etterna, src/Etterna/MinaCalc/Agnostic/HA_PatternMods/TheThingFinder.h
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

//! Rolly jumpstream that plays as a jumptrill or a mashed roll (the two
//! things).

use crate::agnostic::ha_sequencers::thing_sequencing::{TtSequencing, TtSequencing2};
use crate::note_data::pmod;
use crate::pattern_mod_helpers::clamp;

#[derive(Clone, Debug)]
pub struct TheThingLookerFinderThing {
    min_mod: f32,
    max_mod: f32,
    base: f32,
    group_tol: f32,
    step_tol: f32,
    scaler: f32,
    pub tt: TtSequencing,
    pmod: f32,
}

impl Default for TheThingLookerFinderThing {
    fn default() -> Self {
        TheThingLookerFinderThing {
            min_mod: 0.15,
            max_mod: 1.0,
            base: 0.05,
            group_tol: 35.0,
            step_tol: 17.5,
            scaler: 0.2,
            tt: TtSequencing::default(),
            pmod: 0.15,
        }
    }
}

impl TheThingLookerFinderThing {
    pub const PMOD: usize = pmod::THE_THING;

    pub fn setup(&mut self) {
        self.tt
            .set_params(self.group_tol, self.step_tol, self.scaler);
    }

    pub fn advance_sequencing(&mut self, ms_now: f32, notes: u32) {
        self.tt.advance(ms_now, notes);
    }

    /// `operator()`.
    pub fn get(&mut self) -> f32 {
        let m = self.tt.mod_parts;
        self.pmod = m[0] + m[1] + m[2] + m[3];
        self.pmod /= 4.0;
        self.pmod = clamp(self.base + self.pmod, self.min_mod, self.max_mod);
        self.tt.reset();
        self.pmod
    }
}

#[derive(Clone, Debug)]
pub struct TheThingLookerFinderThing2 {
    min_mod: f32,
    max_mod: f32,
    base: f32,
    group_tol: f32,
    step_tol: f32,
    scaler: f32,
    pub tt2: TtSequencing2,
    pmod: f32,
}

impl Default for TheThingLookerFinderThing2 {
    fn default() -> Self {
        TheThingLookerFinderThing2 {
            min_mod: 0.15,
            max_mod: 1.0,
            base: 0.05,
            group_tol: 35.0,
            step_tol: 17.5,
            scaler: 0.2,
            tt2: TtSequencing2::default(),
            pmod: 0.15,
        }
    }
}

impl TheThingLookerFinderThing2 {
    pub const PMOD: usize = pmod::THE_THING2;

    pub fn setup(&mut self) {
        self.tt2
            .set_params(self.group_tol, self.step_tol, self.scaler);
    }

    pub fn advance_sequencing(&mut self, ms_now: f32, notes: u32) {
        self.tt2.advance(ms_now, notes);
    }

    /// `operator()`.
    pub fn get(&mut self) -> f32 {
        let m = self.tt2.mod_parts;
        self.pmod = m[0] + m[1] + m[2] + m[3];
        self.pmod /= 4.0;
        self.pmod = clamp(self.base + self.pmod, self.min_mod, self.max_mod);
        self.tt2.reset();
        self.pmod
    }
}
