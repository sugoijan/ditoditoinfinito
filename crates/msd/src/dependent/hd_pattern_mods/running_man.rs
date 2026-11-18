// Translated to Rust from Etterna's difficulty calculator, MinaCalc:
// https://github.com/etternagame/etterna, src/Etterna/MinaCalc/Dependent/HD_PatternMods/RunningMan.h
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

//! Runningmen: the pattern mod, and the anchor difficulty tech reads.

use crate::dependent::hd_basic_sequencing::{
    BaseType, CT_LOOP_NO_JUMPS, ColType, NUM_COLS_PER_HAND,
};
use crate::dependent::hd_meta_sequencing::MetaType;
use crate::dependent::hd_sequencers::generic_sequencing::AnchorSequencer;
use crate::dependent::hd_sequencers::rm_sequencing::{RmSequencer, RmStatus};
use crate::note_data::pmod;
use crate::pattern_mod_helpers::{NEUTRAL, clamp, fastsqrt};

#[derive(Clone, Debug)]
pub struct RunningManMod {
    min_mod: f32,
    max_mod: f32,
    base: f32,
    min_anchor_len: f32,
    min_taps_in_rm: f32,
    min_off_taps_same: f32,
    offhand_tap_prop_scaler: f32,
    offhand_tap_prop_min: f32,
    offhand_tap_prop_max: f32,
    offhand_tap_prop_base: f32,
    offhand_tap_prop_anch_diff_base: f32,
    offhand_tap_prop_anch_diff_scaler: f32,
    offhand_tap_prop_anch_diff_min: f32,
    offhand_tap_prop_anch_diff_max: f32,
    off_tap_same_prop_scaler: f32,
    off_tap_same_prop_min: f32,
    off_tap_same_prop_max: f32,
    off_tap_same_prop_base: f32,
    anchor_len_divisor: f32,
    anchor_len_comp_min: f32,
    anchor_len_comp_max: f32,
    min_jack_taps_for_bonus: f32,
    jack_bonus_base: f32,
    min_oht_taps_for_bonus: f32,
    oht_bonus_base: f32,
    max_oht_len: f32,
    max_off_len: f32,
    max_ot_sh_len: f32,
    max_burst_len: f32,
    max_jack_len: f32,
    max_anch_len: f32,

    rms: [RmSequencer; NUM_COLS_PER_HAND],
    highest_rm: RmSequencer,
    offhand_tap_prop: f32,
    off_tap_same_prop: f32,
    anchor_len_comp: f32,
    jack_bonus: f32,
    oht_bonus: f32,
    pmod: f32,
}

impl Default for RunningManMod {
    fn default() -> Self {
        RunningManMod {
            min_mod: 1.0,
            max_mod: 1.1,
            base: 0.5,
            min_anchor_len: 5.0,
            min_taps_in_rm: 1.0,
            min_off_taps_same: 1.0,
            offhand_tap_prop_scaler: 1.0,
            offhand_tap_prop_min: 0.0,
            offhand_tap_prop_max: 1.0,
            offhand_tap_prop_base: 1.7,
            offhand_tap_prop_anch_diff_base: 1.7,
            offhand_tap_prop_anch_diff_scaler: 1.1,
            offhand_tap_prop_anch_diff_min: 0.75,
            offhand_tap_prop_anch_diff_max: 1.0,
            off_tap_same_prop_scaler: 1.0,
            off_tap_same_prop_min: 0.0,
            off_tap_same_prop_max: 1.25,
            off_tap_same_prop_base: 0.8,
            anchor_len_divisor: 5.0,
            anchor_len_comp_min: 0.0,
            anchor_len_comp_max: 1.25,
            min_jack_taps_for_bonus: 1.0,
            jack_bonus_base: 0.1,
            min_oht_taps_for_bonus: 1.0,
            oht_bonus_base: 0.1,
            max_oht_len: 2.0,
            max_off_len: 3.0,
            max_ot_sh_len: 2.0,
            max_burst_len: 6.0,
            max_jack_len: 3.0,
            max_anch_len: 5.0,
            rms: [RmSequencer::default(); NUM_COLS_PER_HAND],
            highest_rm: RmSequencer::default(),
            offhand_tap_prop: 0.0,
            off_tap_same_prop: 0.0,
            anchor_len_comp: 0.0,
            jack_bonus: 0.0,
            oht_bonus: 0.0,
            pmod: NEUTRAL,
        }
    }
}

impl RunningManMod {
    pub const PMOD: usize = pmod::RAN_MAN;

    pub fn full_reset(&mut self) {
        for rm in &mut self.rms {
            rm.full_reset();
        }
        self.offhand_tap_prop = 0.0;
        self.off_tap_same_prop = 0.0;
        self.anchor_len_comp = 0.0;
        self.jack_bonus = 0.0;
        self.oht_bonus = 0.0;
        self.pmod = NEUTRAL;
    }

    pub fn setup(&mut self) {
        for c in CT_LOOP_NO_JUMPS {
            let rm = &mut self.rms[c.idx()];
            rm.ct = c;
            rm.set_params(
                self.max_oht_len,
                self.max_off_len,
                self.max_ot_sh_len,
                self.max_burst_len,
                self.max_jack_len,
                self.max_anch_len,
            );
        }
    }

    pub fn advance_off_hand_sequencing(&mut self) {
        for c in CT_LOOP_NO_JUMPS {
            self.rms[c.idx()].advance_off_hand_sequencing();
        }
    }

    pub fn advance_sequencing(
        &mut self,
        ct: ColType,
        bt: BaseType,
        mt: MetaType,
        as_: &AnchorSequencer,
    ) {
        for c in CT_LOOP_NO_JUMPS {
            self.rms[c.idx()].advance(ct, bt, mt, &as_.anch[c.idx()]);
        }
        self.highest_rm = self.get_active_rm_with_higher_difficulty();
    }

    pub fn get_highest_anchor_difficulty(&self) -> f32 {
        let mut oht_p = self.offhand_tap_prop_anch_diff_base
            - (self.highest_rm.rm.get_offhand_tap_prop() * self.offhand_tap_prop_anch_diff_scaler);
        oht_p = clamp(
            oht_p,
            self.offhand_tap_prop_anch_diff_min,
            self.offhand_tap_prop_anch_diff_max,
        );
        self.highest_rm.get_difficulty() * oht_p
    }

    fn get_active_rm_with_higher_difficulty(&self) -> RmSequencer {
        let l = &self.rms[ColType::Left.idx()];
        let r = &self.rms[ColType::Right.idx()];
        if l.status == RmStatus::Running && r.status == RmStatus::Running {
            return if l.get_difficulty() > r.get_difficulty() {
                *l
            } else {
                *r
            };
        }
        if l.status == RmStatus::Running {
            *l
        } else {
            *r
        }
    }

    fn set_pmod(&mut self, total_taps: i32) {
        if total_taps == 0 {
            self.pmod = NEUTRAL;
            return;
        }
        let rm = self.highest_rm.rm;
        if (rm.len as f32) < self.min_anchor_len
            || (rm.ran_taps as f32) < self.min_taps_in_rm
            || (rm.off_taps_sh as f32) < self.min_off_taps_same
        {
            self.pmod = self.min_mod;
            return;
        }
        self.offhand_tap_prop =
            self.offhand_tap_prop_base - (rm.get_offhand_tap_prop() * self.offhand_tap_prop_scaler);
        self.offhand_tap_prop = clamp(
            self.offhand_tap_prop,
            self.offhand_tap_prop_min,
            self.offhand_tap_prop_max,
        );
        self.off_tap_same_prop = self.off_tap_same_prop_base
            + (rm.get_off_tap_same_prop() * self.off_tap_same_prop_scaler);
        self.off_tap_same_prop = clamp(
            self.off_tap_same_prop,
            self.off_tap_same_prop_min,
            self.off_tap_same_prop_max,
        );
        self.anchor_len_comp = rm.len as f32 / self.anchor_len_divisor;
        self.anchor_len_comp = clamp(
            self.anchor_len_comp,
            self.anchor_len_comp_min,
            self.anchor_len_comp_max,
        );
        self.jack_bonus = if rm.jack_taps as f32 >= self.min_jack_taps_for_bonus {
            self.jack_bonus_base
        } else {
            0.0
        };
        self.oht_bonus = if rm.oht_taps as f32 >= self.min_oht_taps_for_bonus {
            self.oht_bonus_base
        } else {
            0.0
        };
        self.pmod = self.base + self.anchor_len_comp + self.jack_bonus + self.oht_bonus;
        self.pmod = clamp(
            fastsqrt(self.pmod * self.off_tap_same_prop * self.offhand_tap_prop),
            self.min_mod,
            self.max_mod,
        );
    }

    /// `operator()`.
    pub fn get(&mut self, total_taps: i32) -> f32 {
        self.set_pmod(total_taps);
        self.highest_rm.full_reset();
        self.pmod
    }
}
