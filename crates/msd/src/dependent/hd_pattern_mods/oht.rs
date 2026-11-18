// Translated to Rust from Etterna's difficulty calculator, MinaCalc:
// https://github.com/etternagame/etterna, src/Etterna/MinaCalc/Dependent/HD_PatternMods/OHT.h
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

//! One hand trills.

use crate::calc_window::{CalcMovingWindow, MAX_MOVING_WINDOW_SIZE};
use crate::dependent::hd_meta_sequencing::MetaType;
use crate::dependent::interval_hand_info::ItvHandInfo;
use crate::note_data::pmod;
use crate::pattern_mod_helpers::{NEUTRAL, clamp};

pub const MAX_TRILLS_PER_INTERVAL: usize = 4;

/// The body `OHTrillMod` and `VOHTrillMod` share; `min_len` is VOHT's
/// extra check (none for OHT) and `zero_taps_on_reset` OHT's.
#[derive(Clone, Debug)]
pub struct TrillCore {
    window_param: f32,
    min_mod: f32,
    max_mod: f32,
    base: f32,
    suppression: f32,
    cv_reset: f32,
    cv_threshhold: f32,
    min_len: Option<f32>,
    zero_taps_on_reset: bool,

    window: usize,
    cc_window: usize,
    luca_turilli: bool,
    badjuju: CalcMovingWindow<f32>,
    mw_oht_taps: CalcMovingWindow<i32>,
    foundyatrills: [i32; MAX_TRILLS_PER_INTERVAL],
    found_oht: usize,
    oht_len: i32,
    oht_taps: i32,
    hello_my_name_is_goat: f32,
    moving_cv: f32,
    pmod: f32,
}

impl TrillCore {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        window_param: f32,
        min_mod: f32,
        base: f32,
        suppression: f32,
        cv_threshhold: f32,
        min_len: Option<f32>,
        zero_taps_on_reset: bool,
    ) -> Self {
        TrillCore {
            window_param,
            min_mod,
            max_mod: 1.0,
            base,
            suppression,
            cv_reset: 1.0,
            cv_threshhold,
            min_len,
            zero_taps_on_reset,
            window: 0,
            cc_window: 0,
            luca_turilli: false,
            badjuju: CalcMovingWindow::default(),
            mw_oht_taps: CalcMovingWindow::default(),
            foundyatrills: [0; MAX_TRILLS_PER_INTERVAL],
            found_oht: 0,
            oht_len: 0,
            oht_taps: 0,
            hello_my_name_is_goat: 0.0,
            moving_cv: 1.0,
            pmod: min_mod,
        }
    }

    pub fn full_reset(&mut self) {
        self.badjuju.zero();
        if self.zero_taps_on_reset {
            self.mw_oht_taps.zero();
        }
        self.luca_turilli = false;
        self.found_oht = 0;
        self.oht_len = 0;
        self.foundyatrills = [0; MAX_TRILLS_PER_INTERVAL];
        self.moving_cv = self.cv_reset;
        self.pmod = NEUTRAL;
    }

    pub fn setup(&mut self) {
        self.window = clamp(self.window_param as i32, 1, MAX_MOVING_WINDOW_SIZE as i32) as usize;
        self.cc_window = clamp(self.window_param as i32, 1, MAX_MOVING_WINDOW_SIZE as i32) as usize;
    }

    fn make_thing(&mut self, itv_taps: f32) -> f32 {
        self.hello_my_name_is_goat = 0.0;
        if self.found_oht == 0 {
            return 0.0;
        }
        for v in self.foundyatrills {
            if v == 0 {
                continue;
            }
            self.hello_my_name_is_goat = (v as f32 / itv_taps) - self.suppression;
        }
        clamp(self.hello_my_name_is_goat, 0.1, 1.0)
    }

    fn complete_seq(&mut self) {
        if !self.luca_turilli || self.oht_len == 0 {
            return;
        }
        if self.found_oht < MAX_TRILLS_PER_INTERVAL {
            self.foundyatrills[self.found_oht] = self.oht_len;
        }
        self.luca_turilli = false;
        self.oht_len = 0;
        self.found_oht += 1;
        self.moving_cv = (self.moving_cv + self.cv_reset) / 2.0;
    }

    fn oht_timing_check(&mut self, ms_any: &CalcMovingWindow<f32>) -> bool {
        self.moving_cv = (self.moving_cv + ms_any.get_cv_of_window(self.cc_window)) / 2.0;
        self.moving_cv < self.cv_threshhold
    }

    fn wifflewaffle(&mut self) {
        if self.luca_turilli {
            self.oht_len += 1;
            self.oht_taps += 1;
        } else {
            self.luca_turilli = true;
            self.oht_len += 3;
            self.oht_taps += 3;
        }
    }

    pub fn advance_sequencing(&mut self, mt: MetaType, ms_any: &CalcMovingWindow<f32>) {
        match mt {
            MetaType::Cccccc => {
                if self.oht_timing_check(ms_any) {
                    self.wifflewaffle();
                } else {
                    self.complete_seq();
                }
            }
            MetaType::Ccacc => {}
            _ => self.complete_seq(),
        }
    }

    fn set_pmod(&mut self, itvhi: &ItvHandInfo) {
        let taps_window = itvhi.get_taps_windowi(self.window);
        let oht_window = self.mw_oht_taps.get_total_for_window(self.window);
        if taps_window == 0 || oht_window == 0 {
            self.pmod = NEUTRAL;
            return;
        }
        if let Some(min_len) = self.min_len
            && (oht_window as f32) < min_len
        {
            self.pmod = NEUTRAL;
            return;
        }
        if taps_window == oht_window {
            self.pmod = self.min_mod;
            return;
        }
        let thing = self.make_thing(itvhi.get_taps_nowf());
        self.badjuju.push(thing);
        self.pmod = self.base - self.badjuju.get_mean_of_window(self.window);
        self.pmod = clamp(self.pmod, self.min_mod, self.max_mod);
    }

    /// `operator()`.
    pub fn get(&mut self, itvhi: &ItvHandInfo) -> f32 {
        if self.oht_len > 0 && self.found_oht < MAX_TRILLS_PER_INTERVAL {
            self.foundyatrills[self.found_oht] = self.oht_len;
            self.found_oht += 1;
        }
        self.mw_oht_taps.push(self.oht_taps);
        self.set_pmod(itvhi);
        self.interval_end();
        self.pmod
    }

    fn interval_end(&mut self) {
        self.foundyatrills = [0; MAX_TRILLS_PER_INTERVAL];
        self.found_oht = 0;
        self.oht_len = 0;
        self.oht_taps = 0;
    }
}

#[derive(Clone, Debug)]
pub struct OhTrillMod(pub TrillCore);

impl Default for OhTrillMod {
    fn default() -> Self {
        OhTrillMod(TrillCore::new(3.0, 0.9, 1.35, 0.4, 0.5, None, true))
    }
}

impl OhTrillMod {
    pub const PMOD: usize = pmod::OH_TRILL;
}
