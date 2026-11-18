// Translated to Rust from Etterna's difficulty calculator, MinaCalc:
// https://github.com/etternagame/etterna, src/Etterna/MinaCalc/Dependent/HD_PatternMods/CJOHAnchor.h
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

//! One hand anchors and chains in chordjacks.

use crate::dependent::hd_basic_sequencing::{BaseType, ColType};
use crate::dependent::hd_sequencers::cjoha_sequencing::CjOhAnchorSequencer;
use crate::dependent::meta_interval_hand_info::MetaItvHandInfo;
use crate::note_data::pmod;
use crate::pattern_mod_helpers::{NEUTRAL, clamp, fastsqrt, max, weighted_average};

#[derive(Clone, Debug)]
pub struct CjOhAnchorMod {
    min_mod: f32,
    max_mod: f32,
    base: f32,
    len_scaler: f32,
    anchor_len_weight: f32,
    swap_scaler: f32,
    not_swap_scaler: f32,

    chain: CjOhAnchorSequencer,
    max_chain_swaps: i32,
    max_not_swaps: i32,
    max_total_len: i32,
    max_anchor_len: i32,
    pmod: f32,
}

impl Default for CjOhAnchorMod {
    fn default() -> Self {
        CjOhAnchorMod {
            min_mod: 1.0,
            max_mod: 1.1,
            base: 0.5,
            len_scaler: 0.2,
            anchor_len_weight: 1.0,
            swap_scaler: 0.10775,
            not_swap_scaler: 0.019,
            chain: CjOhAnchorSequencer::default(),
            max_chain_swaps: 0,
            max_not_swaps: 0,
            max_total_len: 0,
            max_anchor_len: 0,
            pmod: NEUTRAL,
        }
    }
}

impl CjOhAnchorMod {
    pub const PMOD: usize = pmod::CJ_OH_ANCHOR;

    pub fn full_reset(&mut self) {
        self.chain.zero();
        self.max_chain_swaps = 0;
        self.max_not_swaps = 0;
        self.max_total_len = 0;
        self.max_anchor_len = 0;
        self.pmod = NEUTRAL;
    }

    pub fn advance_sequencing(&mut self, ct: ColType, bt: BaseType, last_ct: ColType, any_ms: f32) {
        self.chain.advance(ct, bt, last_ct, any_ms);
    }

    fn set_pmod(&mut self, mitvhi: &MetaItvHandInfo) {
        let itvhi = &mitvhi.itvhi;
        let base_types = &mitvhi.base_types;
        self.max_chain_swaps = self.chain.get_max_chain_swaps();
        self.max_not_swaps = self.chain.get_max_not_swaps();
        self.max_total_len = self.chain.get_max_total_len();
        self.max_anchor_len = self.chain.get_max_anchor_len();
        if itvhi.get_taps_nowi() == 0 {
            self.pmod = NEUTRAL;
            return;
        }
        let taps_in_any_sequence = clamp(
            base_types[BaseType::SingleSingle.idx()]
                + base_types[BaseType::SingleJump.idx()]
                + base_types[BaseType::JumpSingle.idx()],
            1,
            max(1, self.max_total_len),
        );
        let taps_f = taps_in_any_sequence as f32;
        let cs_f = self.max_chain_swaps as f32;
        let cl_f = self.max_total_len as f32;
        let ca_f = self.max_anchor_len as f32;
        let anchor_len_worth = weighted_average(cl_f, ca_f, self.anchor_len_weight, 1.0);
        let anchor_worth = fastsqrt(anchor_len_worth / taps_f) * self.len_scaler;
        let swap_worth = fastsqrt(cs_f / taps_f) * self.swap_scaler;
        self.pmod = clamp(
            self.base + anchor_worth + swap_worth + self.not_swap_scaler,
            self.min_mod,
            self.max_mod,
        );
    }

    /// `operator()`.
    pub fn get(&mut self, mitvhi: &MetaItvHandInfo) -> f32 {
        self.set_pmod(mitvhi);
        self.interval_end();
        self.pmod
    }

    fn interval_end(&mut self) {
        self.chain.reset_max_seq();
        self.max_chain_swaps = 0;
        self.max_not_swaps = 0;
        self.max_total_len = 0;
        self.max_anchor_len = 0;
    }
}
