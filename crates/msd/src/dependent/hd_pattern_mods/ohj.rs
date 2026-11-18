// Translated to Rust from Etterna's difficulty calculator, MinaCalc:
// https://github.com/etternagame/etterna, src/Etterna/MinaCalc/Dependent/HD_PatternMods/OHJ.h
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

//! One hand jumps.

use crate::dependent::hd_basic_sequencing::{BaseType, ColType};
use crate::dependent::hd_sequencers::ohj_sequencing::OhjSequencer;
use crate::dependent::meta_interval_hand_info::MetaItvHandInfo;
use crate::note_data::pmod;
use crate::pattern_mod_helpers::{NEUTRAL, clamp, fastsqrt, weighted_average};

/// `OHJumpModGuyThing`.
#[derive(Clone, Debug)]
pub struct OhJumpMod {
    min_mod: f32,
    max_mod: f32,
    max_seq_weight: f32,
    max_seq_pool: f32,
    max_seq_scaler: f32,
    prop_pool: f32,
    prop_scaler: f32,

    ohj: OhjSequencer,
    max_ohjump_seq_taps: i32,
    cc_taps: i32,
    floatymcfloatface: f32,
    base_seq_prop: f32,
    base_jump_prop: f32,
    max_seq_component: f32,
    prop_component: f32,
    pmod: f32,
}

impl Default for OhJumpMod {
    fn default() -> Self {
        OhJumpMod {
            min_mod: 0.75,
            max_mod: 1.0,
            max_seq_weight: 0.65,
            max_seq_pool: 1.2,
            max_seq_scaler: 2.0,
            prop_pool: 1.5,
            prop_scaler: 1.0,
            ohj: OhjSequencer::default(),
            max_ohjump_seq_taps: 0,
            cc_taps: 0,
            floatymcfloatface: 0.0,
            base_seq_prop: 0.0,
            base_jump_prop: 0.0,
            max_seq_component: NEUTRAL,
            prop_component: NEUTRAL,
            pmod: NEUTRAL,
        }
    }
}

impl OhJumpMod {
    pub const PMOD: usize = pmod::OH_JUMP_MOD;

    pub fn full_reset(&mut self) {
        self.ohj.zero();
        self.max_ohjump_seq_taps = 0;
        self.cc_taps = 0;
        self.floatymcfloatface = 0.0;
        self.base_seq_prop = 0.0;
        self.base_jump_prop = 0.0;
        self.max_seq_component = NEUTRAL;
        self.prop_component = NEUTRAL;
        self.pmod = NEUTRAL;
    }

    pub fn advance_sequencing(&mut self, ct: ColType, bt: BaseType) {
        self.ohj.advance(ct, bt);
    }

    fn set_max_seq_comp(&mut self) {
        self.max_seq_component = self.max_seq_pool - (self.base_seq_prop * self.max_seq_scaler);
        self.max_seq_component = if self.max_seq_component < 0.1 {
            0.1
        } else {
            self.max_seq_component
        };
        self.max_seq_component = fastsqrt(self.max_seq_component);
    }

    fn set_prop_comp(&mut self) {
        self.prop_component = self.prop_pool - (self.base_jump_prop * self.prop_scaler);
        self.prop_component = if self.prop_component < 0.1 {
            0.1
        } else {
            self.prop_component
        };
        self.prop_component = fastsqrt(self.prop_component);
    }

    fn set_pmod(&mut self, mitvhi: &MetaItvHandInfo) {
        let itvhi = &mitvhi.itvhi;
        self.cc_taps = mitvhi.base_types[BaseType::LeftRight.idx()]
            + mitvhi.base_types[BaseType::RightLeft.idx()];
        self.max_ohjump_seq_taps = self.ohj.get_largest_seq_taps();

        if itvhi.get_taps_nowi() == 0 || itvhi.get_col_taps_nowi(ColType::OhJump) == 0 {
            self.pmod = NEUTRAL;
            return;
        }
        if self.max_ohjump_seq_taps >= itvhi.get_taps_nowi() {
            self.pmod = self.min_mod;
            return;
        }
        if self.max_ohjump_seq_taps < 3 {
            self.base_jump_prop = itvhi.get_col_taps_nowf(ColType::OhJump) / itvhi.get_taps_nowf();
            self.set_prop_comp();
            self.pmod = clamp(self.prop_component, self.min_mod, self.max_mod);
            return;
        }
        if self.cc_taps == 0 {
            self.floatymcfloatface = self.max_ohjump_seq_taps as f32;
            self.base_seq_prop = self.floatymcfloatface / itvhi.get_taps_nowf();
            self.set_max_seq_comp();
            self.pmod = clamp(self.max_seq_component, self.min_mod, self.max_mod);
            return;
        }

        self.floatymcfloatface = self.max_ohjump_seq_taps as f32;
        self.base_seq_prop = self.floatymcfloatface / itvhi.get_taps_nowf();
        self.set_max_seq_comp();
        self.max_seq_component = clamp(self.max_seq_component, 0.1, self.max_mod);

        self.base_jump_prop = itvhi.get_col_taps_nowf(ColType::OhJump) / itvhi.get_taps_nowf();
        self.set_prop_comp();
        self.prop_component = clamp(self.prop_component, 0.1, self.max_mod);

        self.pmod = weighted_average(
            self.max_seq_component,
            self.prop_component,
            self.max_seq_weight,
            1.0,
        );
        self.pmod = clamp(self.pmod, self.min_mod, self.max_mod);
    }

    /// `operator()`.
    pub fn get(&mut self, mitvhi: &MetaItvHandInfo) -> f32 {
        self.set_pmod(mitvhi);
        self.interval_end();
        self.pmod
    }

    fn interval_end(&mut self) {
        self.cc_taps = 0;
        self.ohj.max_seq_taps = 0;
        self.max_ohjump_seq_taps = 0;
    }
}
