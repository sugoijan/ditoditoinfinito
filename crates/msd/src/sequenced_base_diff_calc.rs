// Translated to Rust from Etterna's difficulty calculator, MinaCalc:
// https://github.com/etternagame/etterna, src/Etterna/MinaCalc/SequencedBaseDiffCalc.h
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

//! Base difficulties built row by row: the NPS base and an interval's
//! points (`nps`), the grindscaler, the tech base (`techyo`) and the
//! chord proportions (`propz`). Upstream's MSBase estimate, its chordjack
//! base (`ceejay`) and the jack base are debug output only and left out,
//! as is `oversimplified_jacks` (other key counts).

use crate::agnostic::interval_info::tap_size;
use crate::calc_window::CalcMovingWindow;
use crate::dependent::hd_basic_sequencing::ColType;
use crate::dependent::hd_sequencers::generic_sequencing::SequencerGeneral;
use crate::minacalc::{Calc, MAX_ROWS_FOR_SINGLE_INTERVAL, NUM_HANDS};
use crate::note_data::diff;
use crate::pattern_mod_helpers::{clamp, max, weighted_average};
use crate::sequencing_helpers::{FINALSCALER, ms_to_scaled_nps};
use crate::ulbu_acolytes::itv_idx_to_time;

/// Required share of the average notes for an interval to pass.
pub const MIN_THRESHOLD: f32 = 0.65;

/// `std::log(6.2F)`.
fn downscale_logbase() -> f32 {
    libm::logf(6.2)
}

/// `nps::actual_cancer`: NPSBase and the interval's points for a hand.
pub fn actual_cancer(calc: &mut Calc, hand: usize) {
    for itv in 0..calc.numitv {
        let mut notes = 0i32;
        for row in calc.rows_of(itv) {
            notes += row.hand_counts[hand];
        }
        let nps = notes as f32 * FINALSCALER * 1.6;
        calc.init_base_diff_vals[hand][diff::NPS_BASE][itv] = nps;
        calc.itv_points[hand][itv] = notes * 2;
    }
}

/// `nps::grindscale`: a multiplier from the file's length and density
/// (applied to scores only).
pub fn grindscale(calc: &mut Calc) {
    let mut populated_intervals = 0i32;
    let mut avg_notes = 0.0f32;
    for itv in 0..calc.numitv {
        let mut notes = 0.0f32;
        for hand in 0..NUM_HANDS {
            notes += calc.init_base_diff_vals[hand][diff::NPS_BASE][itv];
        }
        if notes > 0.0 {
            avg_notes += notes;
            populated_intervals += 1;
        }
    }
    if populated_intervals > 0 {
        avg_notes /= populated_intervals as f32;
        let mut failed_intervals = 0i32;
        for itv in 0..calc.numitv {
            let mut notes = 0.0f32;
            for hand in 0..NUM_HANDS {
                notes += calc.init_base_diff_vals[hand][diff::NPS_BASE][itv];
            }
            if notes > 0.0 && notes < avg_notes * MIN_THRESHOLD {
                failed_intervals += 1;
            }
        }
        let file_length = itv_idx_to_time(populated_intervals - failed_intervals);
        let ping = 0.3f32;
        let timescaler = (ping * (libm::logf(file_length + 1.0) / downscale_logbase())) + ping;
        calc.grindscaler = clamp(timescaler, 0.1, 1.0);
    } else {
        calc.grindscaler = 0.1;
    }
}

/// `techyo`: the tech base, from a convoluted chaos of the hand's timings.
#[derive(Clone, Debug)]
pub struct Techyo {
    tc_base_weight: f32,
    nps_base_weight: f32,
    rm_base_weight: f32,
    chaos_comp_window: f32,
    tc_static_base_window: f32,

    row_counter: usize,
    teehee: CalcMovingWindow<f32>,
    rm_itv_max_diff: f32,
}

impl Default for Techyo {
    fn default() -> Self {
        Techyo {
            tc_base_weight: 4.0,
            nps_base_weight: 9.0,
            rm_base_weight: 1.0,
            chaos_comp_window: 4.0,
            tc_static_base_window: 2.0,
            row_counter: 0,
            teehee: CalcMovingWindow::default(),
            rm_itv_max_diff: 0.0,
        }
    }
}

impl Techyo {
    pub fn advance_base(&mut self, seq: &SequencerGeneral, ct: ColType, calc: &mut Calc) {
        if self.row_counter >= MAX_ROWS_FOR_SINGLE_INTERVAL {
            return;
        }
        let chaos_comp = self.calc_chaos_comp(seq, ct);
        self.teehee.push(chaos_comp);
        calc.tc_static[self.row_counter] = self
            .teehee
            .get_mean_of_window(self.tc_static_base_window as i32 as usize);
        self.row_counter += 1;
    }

    pub fn advance_rm_comp(&mut self, rm_diff: f32) {
        self.rm_itv_max_diff = max(self.rm_itv_max_diff, rm_diff);
    }

    /// The interval's TechBase.
    pub fn get_itv_diff(&self, nps_base: f32, calc: &Calc) -> f32 {
        let mut rmbase = self.rm_itv_max_diff;
        let nps_biased_chaos_base = weighted_average(
            self.get_tc_base(calc),
            nps_base,
            self.tc_base_weight,
            self.nps_base_weight,
        );
        if rmbase >= nps_biased_chaos_base {
            rmbase = weighted_average(rmbase, nps_biased_chaos_base, self.rm_base_weight, 1.0);
        }
        max(nps_biased_chaos_base, rmbase)
    }

    pub fn interval_end(&mut self) {
        self.row_counter = 0;
        self.rm_itv_max_diff = 0.0;
    }

    pub fn full_reset(&mut self) {
        self.row_counter = 0;
        self.rm_itv_max_diff = 0.0;
        self.teehee.zero();
    }

    fn get_tc_base(&self, calc: &Calc) -> f32 {
        if self.row_counter == 0 {
            return 0.0;
        }
        let mut ms_total = 0.0f32;
        for i in 0..self.row_counter {
            ms_total += calc.tc_static[i];
        }
        let ms_mean = ms_total / self.row_counter as f32;
        ms_to_scaled_nps(ms_mean)
    }

    /// Upstream also computes a trill ratio here (`pewp`, `obliosis`) that
    /// only reaches its debug output.
    fn calc_chaos_comp(&self, seq: &SequencerGeneral, ct: ColType) -> f32 {
        let a = seq.get_sc_ms_now(ct, true);
        let b = if ct == ColType::OhJump {
            seq.get_sc_ms_now(ct, false)
        } else {
            seq.get_cc_ms_now()
        };
        let c = (a + b) / 2.0;
        let window = self.chaos_comp_window as i32 as usize;
        let mut pineapple = seq.mw_any_ms.get_cv_of_window(window);
        let mut porcupine = seq.mw_sc_ms[ColType::Left.idx()].get_cv_of_window(window);
        let mut sequins = seq.mw_sc_ms[ColType::Right.idx()].get_cv_of_window(window);
        const OIOI: f32 = 0.5;
        const IOIO: f32 = 0.5;
        pineapple = clamp(pineapple + OIOI, OIOI, IOIO + OIOI);
        porcupine = clamp(porcupine + OIOI, OIOI, IOIO + OIOI);
        sequins = clamp(sequins + OIOI, OIOI, IOIO + OIOI);
        let vertebrae = clamp((pineapple + porcupine + sequins) / 3.0, OIOI, IOIO + OIOI);
        c / vertebrae
    }
}

/// `propz`: taps in chords of each size (four keys).
#[derive(Clone, Debug, Default)]
pub struct Propz {
    pub taps_by_size: [i32; 4],
}

impl Propz {
    pub fn advance_sequencing(&mut self, row_count: i32) {
        if (row_count - 1) as usize >= self.taps_by_size.len() {
            return;
        }
        self.taps_by_size[(row_count - 1) as usize] += row_count;
    }

    pub fn get_prop(&self, size: usize) -> f32 {
        if size.wrapping_sub(1) >= self.taps_by_size.len() {
            return 0.0;
        }
        let n = self.taps_by_size[size] as f32;
        let d = self.taps_by_size[tap_size::SINGLE] as f32;
        if d == 0.0 {
            return 9_999_999.0;
        }
        n / d
    }

    pub fn full_reset(&mut self) {
        self.taps_by_size = [0; 4];
    }
}

/// `diffz`: the sequenced base difficulties a hand carries.
#[derive(Clone, Debug, Default)]
pub struct Diffz {
    pub tc: Techyo,
}

impl Diffz {
    pub fn interval_end(&mut self) {
        self.tc.interval_end();
    }

    pub fn full_reset(&mut self) {
        self.interval_end();
        self.tc.full_reset();
    }
}
