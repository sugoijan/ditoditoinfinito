// Translated to Rust from Etterna's difficulty calculator, MinaCalc:
// https://github.com/etternagame/etterna, src/Etterna/MinaCalc/Dependent/HD_PatternMods/CJOHJ.h
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

//! One hand jumps, for chordjacks.

use crate::dependent::hd_basic_sequencing::{BaseType, ColType};
use crate::dependent::hd_sequencers::ohj_sequencing::OhjSequencer;
use crate::dependent::meta_interval_hand_info::MetaItvHandInfo;
use crate::note_data::pmod;
use crate::pattern_mod_helpers::{NEUTRAL, clamp};

#[derive(Clone, Debug)]
pub struct CjOhJumpMod {
    min_mod: f32,
    max_mod: f32,
    prop_pool: f32,
    prop_scaler: f32,

    ohj: OhjSequencer,
    max_ohjump_seq_taps: i32,
    base_seq_prop: f32,
    base_jump_prop: f32,
    prop_component: f32,
    pmod: f32,
}

impl Default for CjOhJumpMod {
    fn default() -> Self {
        CjOhJumpMod {
            min_mod: 0.57,
            max_mod: 1.0,
            prop_pool: 1.0,
            prop_scaler: 0.63,
            ohj: OhjSequencer::default(),
            max_ohjump_seq_taps: 0,
            base_seq_prop: 0.0,
            base_jump_prop: 0.0,
            prop_component: NEUTRAL,
            pmod: NEUTRAL,
        }
    }
}

impl CjOhJumpMod {
    pub const PMOD: usize = pmod::CJ_OH_JUMP;

    pub fn full_reset(&mut self) {
        self.ohj.zero();
        self.max_ohjump_seq_taps = 0;
        self.base_seq_prop = 0.0;
        self.base_jump_prop = 0.0;
        self.prop_component = NEUTRAL;
        self.pmod = NEUTRAL;
    }

    pub fn advance_sequencing(&mut self, ct: ColType, bt: BaseType) {
        self.ohj.advance(ct, bt);
    }

    fn set_prop_comp(&mut self) {
        self.prop_component = self.prop_pool - (self.base_jump_prop * self.prop_scaler);
        self.prop_component = if self.prop_component < 0.1 {
            0.1
        } else {
            self.prop_component
        };
    }

    fn set_pmod(&mut self, mitvhi: &MetaItvHandInfo) {
        let itvhi = &mitvhi.itvhi;
        self.max_ohjump_seq_taps = self.ohj.get_largest_seq_taps();
        if itvhi.get_taps_nowi() == 0 || itvhi.get_col_taps_nowi(ColType::OhJump) == 0 {
            self.pmod = NEUTRAL;
            return;
        }
        if self.max_ohjump_seq_taps >= itvhi.get_taps_nowi() {
            self.pmod = self.min_mod;
            return;
        }
        let ohjcount = itvhi.get_col_taps_nowf(ColType::OhJump) / 2.0;
        let tapcount = (itvhi.get_col_taps_nowf(ColType::Left) - ohjcount)
            + (itvhi.get_col_taps_nowf(ColType::Right) - ohjcount);
        let rows = ohjcount + tapcount;
        self.base_jump_prop = ohjcount / rows;
        self.set_prop_comp();
        self.prop_component = clamp(self.prop_component, 0.1, self.max_mod);
        self.pmod = self.prop_component;
        self.pmod = clamp(self.pmod, self.min_mod, self.max_mod);
    }

    /// `operator()`.
    pub fn get(&mut self, mitvhi: &MetaItvHandInfo) -> f32 {
        self.set_pmod(mitvhi);
        self.interval_end();
        self.pmod
    }

    fn interval_end(&mut self) {
        self.ohj.max_seq_taps = 0;
        self.max_ohjump_seq_taps = 0;
    }
}
