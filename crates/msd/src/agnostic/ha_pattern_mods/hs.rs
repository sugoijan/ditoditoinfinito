// Translated to Rust from Etterna's difficulty calculator, MinaCalc:
// https://github.com/etternagame/etterna, src/Etterna/MinaCalc/Agnostic/HA_PatternMods/HS.h
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

//! Handstream: hands, against jacks and jumptrills.

use crate::agnostic::interval_info::tap_size;
use crate::agnostic::meta_interval_info::MetaItvInfo;
use crate::note_data::pmod;
use crate::pattern_mod_helpers::{NEUTRAL, clamp, fastsqrt};

#[derive(Clone, Debug)]
pub struct HsMod {
    min_mod: f32,
    max_mod: f32,
    prop_buffer: f32,
    total_prop_min: f32,
    total_prop_max: f32,
    total_prop_scaler: f32,
    total_prop_base: f32,
    split_hand_pool: f32,
    split_hand_min: f32,
    split_hand_max: f32,
    jack_pool: f32,
    jack_min: f32,
    jack_max: f32,
    decay_factor: f32,

    total_prop: f32,
    jumptrill_prop: f32,
    jack_prop: f32,
    last_mod: f32,
    pmod: f32,
    t_taps: f32,
}

impl Default for HsMod {
    fn default() -> Self {
        HsMod {
            min_mod: 0.6,
            max_mod: 1.1,
            prop_buffer: 1.0,
            total_prop_min: 0.6,
            total_prop_max: 1.1,
            total_prop_scaler: 5.571,
            total_prop_base: 0.4,
            split_hand_pool: 1.6,
            split_hand_min: 0.89,
            split_hand_max: 1.0,
            jack_pool: 1.35,
            jack_min: 0.5,
            jack_max: 1.0,
            decay_factor: 0.05,
            total_prop: 0.0,
            jumptrill_prop: 0.0,
            jack_prop: 0.0,
            last_mod: 0.6,
            pmod: 0.6,
            t_taps: 0.0,
        }
    }
}

impl HsMod {
    pub const PMOD: usize = pmod::HS;
    const TAP_SIZE: usize = tap_size::HAND;

    pub fn full_reset(&mut self) {
        self.last_mod = self.min_mod;
    }

    fn decay_mod(&mut self) {
        self.pmod = clamp(
            self.last_mod - self.decay_factor,
            self.min_mod,
            self.max_mod,
        );
        self.last_mod = self.pmod;
    }

    /// `operator()`.
    pub fn get(&mut self, mitvi: &MetaItvInfo) -> f32 {
        let itvi = &mitvi.itvi;
        if itvi.total_taps == 0 {
            return NEUTRAL;
        }
        if itvi.taps_by_size[Self::TAP_SIZE] == 0 {
            self.decay_mod();
            return self.pmod;
        }
        self.t_taps = itvi.total_taps as f32;
        self.total_prop = self.total_prop_base
            + ((itvi.taps_by_size[Self::TAP_SIZE] + itvi.mixed_hs_density_tap_bonus) as f32
                + self.prop_buffer)
                / (self.t_taps - self.prop_buffer)
                * self.total_prop_scaler;
        self.total_prop = clamp(
            fastsqrt(self.total_prop),
            self.total_prop_min,
            self.total_prop_max,
        );
        self.jumptrill_prop = clamp(
            self.split_hand_pool - (mitvi.not_hs as f32 / self.t_taps),
            self.split_hand_min,
            self.split_hand_max,
        );
        self.jack_prop = clamp(
            self.jack_pool - (mitvi.actual_jacks as f32 / self.t_taps),
            self.jack_min,
            self.jack_max,
        );
        self.pmod = clamp(
            self.total_prop * self.jumptrill_prop * self.jack_prop,
            self.min_mod,
            self.max_mod,
        );
        if mitvi.dunk_it {
            self.pmod *= 0.99;
        }
        self.last_mod = self.pmod;
        self.pmod
    }
}
