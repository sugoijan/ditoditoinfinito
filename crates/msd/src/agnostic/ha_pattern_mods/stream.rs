// Translated to Rust from Etterna's difficulty calculator, MinaCalc:
// https://github.com/etternagame/etterna, src/Etterna/MinaCalc/Agnostic/HA_PatternMods/Stream.h
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

//! Stream: single taps out of all taps in the interval, dampened by jacks.

use crate::agnostic::ha_sequencers::trill_sequencing::ThtSequencing;
use crate::agnostic::interval_info::tap_size;
use crate::agnostic::meta_interval_info::MetaItvInfo;
use crate::note_data::pmod;
use crate::pattern_mod_helpers::{NEUTRAL, clamp, fastsqrt};

#[derive(Clone, Debug)]
pub struct StreamMod {
    base: f32,
    min_mod: f32,
    max_mod: f32,
    prop_buffer: f32,
    prop_scaler: f32,
    jack_pool: f32,
    jack_comp_min: f32,
    jack_comp_max: f32,
    vibro_flag: f32,
    tht_scaler: f32,
    tht_cv_threshold: f32,
    tht_trill_buffer: f32,
    tht_trill_scaler: f32,
    tht_jump_buffer: f32,
    tht_jump_scaler: f32,
    tht_jump_weight: f32,
    tht_min_prop: f32,
    tht_max_prop: f32,

    prop_component: f32,
    jack_component: f32,
    pmod: f32,
    trillsequencer: ThtSequencing,
}

impl Default for StreamMod {
    fn default() -> Self {
        StreamMod {
            base: 0.0,
            min_mod: 0.6,
            max_mod: 1.0,
            prop_buffer: 1.0,
            prop_scaler: 1.41,
            jack_pool: 4.0,
            jack_comp_min: 0.5,
            jack_comp_max: 1.0,
            vibro_flag: 1.0,
            tht_scaler: 0.0,
            tht_cv_threshold: 0.5,
            tht_trill_buffer: 1.4,
            tht_trill_scaler: 1.0,
            tht_jump_buffer: 1.0,
            tht_jump_scaler: 0.5,
            tht_jump_weight: 0.0,
            tht_min_prop: 0.0,
            tht_max_prop: 1.0,
            prop_component: 0.0,
            jack_component: 0.0,
            pmod: 0.6,
            trillsequencer: ThtSequencing::default(),
        }
    }
}

impl StreamMod {
    pub const PMOD: usize = pmod::STREAM;
    const TAP_SIZE: usize = tap_size::SINGLE;

    pub fn setup(&mut self) {
        self.trillsequencer.set_params(
            self.tht_cv_threshold,
            self.tht_trill_buffer,
            self.tht_trill_scaler,
            self.tht_jump_buffer,
            self.tht_jump_scaler,
            self.tht_jump_weight,
            self.tht_min_prop,
            self.tht_max_prop,
        );
    }

    pub fn advance_sequencing(&mut self, ms_now: f32, notes: u32) {
        self.trillsequencer.advance(ms_now, notes);
    }

    pub fn full_reset(&mut self) {
        self.trillsequencer.reset();
    }

    /// `operator()`.
    pub fn get(&mut self, mitvi: &MetaItvInfo) -> f32 {
        let itvi = &mitvi.itvi;
        if itvi.total_taps < 2 {
            return NEUTRAL;
        }
        if itvi.taps_by_size[Self::TAP_SIZE] == 0 {
            return self.min_mod;
        }
        self.prop_component = (itvi.taps_by_size[Self::TAP_SIZE] as f32 + self.prop_buffer)
            / (itvi.total_taps as f32 - self.prop_buffer)
            * self.prop_scaler;
        self.jack_component = clamp(
            self.jack_pool - mitvi.actual_jacks as f32,
            self.jack_comp_min,
            self.jack_comp_max,
        );
        self.pmod = fastsqrt(self.prop_component * self.jack_component);

        let tht_prop = self.trillsequencer.get(mitvi);
        self.pmod *= 1.0 - (tht_prop * self.tht_scaler);
        self.trillsequencer.reset();

        self.pmod = clamp(self.base + self.pmod, self.min_mod, self.max_mod);

        if mitvi.basically_vibro {
            if mitvi.num_var == 1 {
                self.pmod *= 0.5 * self.vibro_flag;
            } else if mitvi.num_var == 2 {
                self.pmod *= 0.9 * self.vibro_flag;
            } else if mitvi.num_var == 3 {
                self.pmod *= 0.95 * self.vibro_flag;
            }
        }
        self.pmod
    }
}
