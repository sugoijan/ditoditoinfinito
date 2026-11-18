// Translated to Rust from Etterna's difficulty calculator, MinaCalc:
// https://github.com/etternagame/etterna, src/Etterna/MinaCalc/Ulbu.h, UlbuBase.h
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

//! The four key driver (`TheGreatBazoinkazoinkInTheSky`, with what it
//! inherits from `Bazoinkazoink`): the hand-agnostic loop over every row,
//! then the hand-dependent loop over each hand's rows, setting every
//! pattern mod and base difficulty an interval. The XML parameter loading
//! (debug only) and the other key counts' drivers are left out.

use crate::agnostic::ha_pattern_mods::{
    cj::CjMod,
    cj_density::CjDensityMod,
    flam_jam::FlamJamMod,
    hand_balance::HandBalanceMod,
    hs::HsMod,
    hs_density::HsDensityMod,
    js::JsMod,
    old_jump_scaler::OldJumpScalerMod,
    stream::StreamMod,
    the_thing_finder::{TheThingLookerFinderThing, TheThingLookerFinderThing2},
};
use crate::agnostic::interval_info::tap_size;
use crate::agnostic::meta_interval_info::MetaItvInfo;
use crate::agnostic::meta_row_info::MetaRowInfo;
use crate::dependent::hd_basic_sequencing::{BaseType, ColType, determine_col_type};
use crate::dependent::hd_pattern_mods::{
    balance::BalanceMod, chaos::ChaosMod, cjoh_anchor::CjOhAnchorMod, cjohj::CjOhJumpMod,
    minijack::MinijackMod, ohj::OhJumpMod, oht::OhTrillMod, old_anchor_scaler::OldAnchorScalerMod,
    old_ohj_scaler::OldOhjScalerMod, old_roll_scaler::OldRollScalerMod, roll::RollMod,
    roll_js::RollJsMod, running_man::RunningManMod, voht::VohTrillMod,
    wide_range_anchor::WideRangeAnchorMod, wide_range_balance::WideRangeBalanceMod,
    wide_range_cj_anchor::WideRangeCjAnchorMod, wide_range_jj::WideRangeJjMod,
    wide_range_jumptrill::WideRangeJumptrillMod, wide_range_roll::WideRangeRollMod,
};
use crate::dependent::hd_sequencers::generic_sequencing::SequencerGeneral;
use crate::dependent::meta_hand_info::MetaHandInfo;
use crate::dependent::meta_interval_hand_info::MetaItvHandInfo;
use crate::minacalc::{Calc, NUM_HANDS};
use crate::note_data::{
    NUM_SKILLSET, SKILL_CHORDJACK, SKILL_HANDSTREAM, SKILL_JACKSPEED, SKILL_JUMPSTREAM,
    SKILL_TECHNICAL, diff, pmod,
};
use crate::pattern_mod_helpers::{clamp, fastpow, fastsqrt, max};
use crate::sequenced_base_diff_calc::{Diffz, Propz, actual_cancer, grindscale};
use crate::sequencing_helpers::{S_INIT, ms_from, ms_to_scaled_nps};
use crate::ulbu_acolytes::{
    bruh_they_the_same, run_agnostic_smoothing_pass, run_dependent_smoothing_pass, set_agnostic,
    set_dependent, smooth,
};

/// The pattern mods each skillset multiplies (`pmods`).
pub const PMODS: [&[usize]; NUM_SKILLSET] = [
    // overall
    &[],
    // stream
    &[
        pmod::STREAM,
        pmod::OH_TRILL,
        pmod::VOH_TRILL,
        pmod::ROLL,
        pmod::WIDE_RANGE_ROLL,
        pmod::WIDE_RANGE_JUMPTRILL,
        pmod::WIDE_RANGE_JJ,
        pmod::FLAM_JAM,
    ],
    // jumpstream
    &[
        pmod::JS,
        pmod::WIDE_RANGE_BALANCE,
        pmod::WIDE_RANGE_JUMPTRILL,
        pmod::WIDE_RANGE_JJ,
        pmod::VOH_TRILL,
        pmod::ROLL_JS,
        pmod::FLAM_JAM,
    ],
    // handstream
    &[
        pmod::HS,
        pmod::OH_JUMP_MOD,
        pmod::THE_THING,
        pmod::WIDE_RANGE_ROLL,
        pmod::WIDE_RANGE_JUMPTRILL,
        pmod::WIDE_RANGE_JJ,
        pmod::OH_TRILL,
        pmod::VOH_TRILL,
        pmod::FLAM_JAM,
        pmod::HS_DENSITY,
    ],
    // stamina
    &[],
    // jackspeed
    &[],
    // chordjack (OldOHJScaler is applied in adj_diff_func)
    &[
        pmod::CJ,
        pmod::OLD_ANCHOR_SCALER,
        pmod::OLD_JUMP_SCALER,
        pmod::OLD_ROLL_SCALER,
        pmod::CJ_DENSITY,
        pmod::WIDE_RANGE_CJ_ANCHOR,
        pmod::WIDE_RANGE_JUMPTRILL,
        pmod::VOH_TRILL,
        pmod::FLAM_JAM,
    ],
    // technical
    &[
        pmod::OH_TRILL,
        pmod::VOH_TRILL,
        pmod::BALANCE,
        pmod::ROLL,
        pmod::CHAOS,
        pmod::WIDE_RANGE_JUMPTRILL,
        pmod::WIDE_RANGE_JJ,
        pmod::WIDE_RANGE_BALANCE,
        pmod::WIDE_RANGE_ROLL,
        pmod::FLAM_JAM,
        pmod::MINIJACK,
        pmod::THE_THING,
        pmod::THE_THING2,
    ],
];

/// Each skillset's base scaler (`basescalers`).
pub const BASESCALERS: [f32; NUM_SKILLSET] = [0.0, 0.91, 0.75, 0.77, 0.93, 1.01, 1.06, 1.06];

/// The four key driver's state, made new for every rating.
#[derive(Clone, Debug, Default)]
pub struct Ulbu {
    hand: usize,

    // Bazoinkazoink's
    mitvi: MetaItvInfo,
    last_mri: MetaRowInfo,
    mri: MetaRowInfo,
    cj: CjMod,
    hb: HandBalanceMod,

    // TheGreatBazoinkazoinkInTheSky's
    mitvhi: MetaItvHandInfo,
    last_mhi: MetaHandInfo,
    mhi: MetaHandInfo,
    seq: SequencerGeneral,
    s: StreamMod,
    js: JsMod,
    hs: HsMod,
    cjd: CjDensityMod,
    hsd: HsDensityMod,
    ohj: OhJumpMod,
    cjohj: CjOhJumpMod,
    roll: RollMod,
    rolljs: RollJsMod,
    bal: BalanceMod,
    oht: OhTrillMod,
    voht: VohTrillMod,
    ch: ChaosMod,
    chain: CjOhAnchorMod,
    rm: RunningManMod,
    mj: MinijackMod,
    wrb: WideRangeBalanceMod,
    wrr: WideRangeRollMod,
    wrjt: WideRangeJumptrillMod,
    wrjj: WideRangeJjMod,
    wra: WideRangeAnchorMod,
    wrcja: WideRangeCjAnchorMod,
    fj: FlamJamMod,
    tt: TheThingLookerFinderThing,
    tt2: TheThingLookerFinderThing2,
    old_anch: OldAnchorScalerMod,
    old_jump: OldJumpScalerMod,
    old_ohj: OldOhjScalerMod,
    old_roll: OldRollScalerMod,
    diffz: Diffz,
    pub propz: Propz,
}

impl Ulbu {
    /// `operator()`: the main driver.
    pub fn run(&mut self, calc: &mut Calc) {
        // reset_base_diffs
        for hand in 0..NUM_HANDS {
            calc.init_base_diff_vals[hand][diff::TECH_BASE].fill(0.0);
            calc.jack_diff[hand].clear();
        }
        self.hand = 0;
        self.full_hand_reset();
        self.full_agnostic_reset();
        self.mitvi.reset();
        self.run_agnostic_pmod_loop(calc);
        self.run_dependent_pmod_loop(calc);
    }

    // hand agnostic pmod loop

    fn full_agnostic_reset(&mut self) {
        self.s.full_reset();
        self.js.full_reset();
        self.hs.full_reset();
        self.cj.full_reset();
        self.old_jump.full_reset();
        self.hb.full_reset();
        self.mri.reset();
        self.last_mri.reset();
        self.propz.full_reset();
    }

    fn setup_agnostic_pmods(&mut self) {
        self.s.setup();
        self.fj.setup();
        self.tt.setup();
        self.tt2.setup();
    }

    fn advance_agnostic_sequencing(&mut self, calc: &Calc) {
        let (ms_now, notes) = (self.mri.ms_now, self.mri.notes);
        self.propz.advance_sequencing(self.mri.count);
        self.s.advance_sequencing(ms_now, notes);
        self.fj.advance_sequencing(ms_now, notes);
        self.tt.advance_sequencing(ms_now, notes);
        self.tt2.advance_sequencing(ms_now, notes);
        self.hb.advance_sequencing(notes, calc.hand_col_masks);
    }

    fn set_agnostic_pmods(&mut self, itv: usize, calc: &mut Calc) {
        let v = self.s.get(&self.mitvi);
        set_agnostic(StreamMod::PMOD, v, itv, calc);
        let v = self.js.get(&self.mitvi);
        set_agnostic(JsMod::PMOD, v, itv, calc);
        let v = self.hs.get(&self.mitvi);
        set_agnostic(HsMod::PMOD, v, itv, calc);
        let v = self.cj.get(&self.mitvi);
        set_agnostic(CjMod::PMOD, v, itv, calc);
        let v = self.cjd.get(&self.mitvi);
        set_agnostic(CjDensityMod::PMOD, v, itv, calc);
        let v = self.hsd.get(&self.mitvi);
        set_agnostic(HsDensityMod::PMOD, v, itv, calc);
        let v = self.fj.get();
        set_agnostic(FlamJamMod::PMOD, v, itv, calc);
        let v = self.tt.get();
        set_agnostic(TheThingLookerFinderThing::PMOD, v, itv, calc);
        let v = self.tt2.get();
        set_agnostic(TheThingLookerFinderThing2::PMOD, v, itv, calc);
        let v = self.old_jump.get(&self.mitvi);
        set_agnostic(OldJumpScalerMod::PMOD, v, itv, calc);
        let v = self.hb.get();
        set_agnostic(HandBalanceMod::PMOD, v, itv, calc);
    }

    fn run_agnostic_pmod_loop(&mut self, calc: &mut Calc) {
        self.setup_agnostic_pmods();
        for itv in 0..calc.numitv {
            for row in 0..calc.itv_size[itv] {
                let ri = calc.rows[calc.itv_start[itv] + row];
                self.mri.advance(
                    &self.last_mri,
                    &mut self.mitvi,
                    ri.row_time,
                    ri.row_count,
                    ri.row_notes,
                    &calc.col_masks,
                );
                self.advance_agnostic_sequencing(calc);
                std::mem::swap(&mut self.mri, &mut self.last_mri);
            }
            self.set_agnostic_pmods(itv, calc);
            self.mitvi.handle_interval_end();
        }
        run_agnostic_smoothing_pass(calc.numitv, calc);
        bruh_they_the_same(calc.numitv, calc);
    }

    // hand dependent pmod loop

    fn handle_row_dependent_pattern_advancement(&mut self, row_time: f32) {
        let mhi = self.mhi;
        self.ohj.advance_sequencing(mhi.ct, mhi.bt);
        self.cjohj.advance_sequencing(mhi.ct, mhi.bt);
        self.chain
            .advance_sequencing(mhi.ct, mhi.bt, mhi.last_ct, self.seq.mw_any_ms.get_now());
        self.oht.0.advance_sequencing(mhi.mt, &self.seq.mw_any_ms);
        self.voht.0.advance_sequencing(mhi.mt, &self.seq.mw_any_ms);
        self.rm
            .advance_sequencing(mhi.ct, mhi.bt, mhi.mt, &self.seq.as_);
        self.wrr.advance_sequencing(
            mhi.bt,
            mhi.mt,
            mhi.last_mt,
            self.seq.mw_any_ms.get_now(),
            self.seq.get_sc_ms_now(mhi.ct, true),
        );
        self.wrjt
            .advance_sequencing(mhi.bt, mhi.mt, mhi.last_mt, &mut self.seq.mw_any_ms);
        self.wrjj.advance_sequencing(mhi.ct, row_time);
        self.ch.advance_sequencing(&self.seq.mw_any_ms);
        self.roll.0.advance_sequencing(mhi.ct, row_time);
        self.rolljs.0.advance_sequencing(mhi.ct, row_time);
        self.mj
            .advance_sequencing(mhi.ct, self.seq.get_sc_ms_now(mhi.ct, true));
        self.old_roll
            .advance_sequencing(mhi.ct, self.seq.mw_any_ms.get_now());
    }

    fn setup_dependent_mods(&mut self) {
        self.oht.0.setup();
        self.voht.0.setup();
        self.rm.setup();
        self.wrr.setup();
        self.wrjt.setup();
        self.wrjj.setup();
        self.wrb.setup();
        self.wra.setup();
        self.wrcja.setup();
    }

    fn set_dependent_pmods(&mut self, itv: usize, calc: &mut Calc) {
        let h = self.hand;
        let itvhi = &self.mitvhi.itvhi;
        let v = self.ohj.get(&self.mitvhi);
        set_dependent(h, OhJumpMod::PMOD, v, itv, calc);
        let v = self.chain.get(&self.mitvhi);
        set_dependent(h, CjOhAnchorMod::PMOD, v, itv, calc);
        let v = self.cjohj.get(&self.mitvhi);
        set_dependent(h, CjOhJumpMod::PMOD, v, itv, calc);
        let v = self.oht.0.get(itvhi);
        set_dependent(h, OhTrillMod::PMOD, v, itv, calc);
        let v = self.voht.0.get(itvhi);
        set_dependent(h, VohTrillMod::PMOD, v, itv, calc);
        let v = self.bal.get(itvhi);
        set_dependent(h, BalanceMod::PMOD, v, itv, calc);
        let v = self.roll.0.get(itvhi);
        set_dependent(h, RollMod::PMOD, v, itv, calc);
        let v = self.rolljs.0.get(itvhi);
        set_dependent(h, RollJsMod::PMOD, v, itv, calc);
        let v = self.ch.get(itvhi.get_taps_nowi());
        set_dependent(h, ChaosMod::PMOD, v, itv, calc);
        let v = self.rm.get(itvhi.get_taps_nowi());
        set_dependent(h, RunningManMod::PMOD, v, itv, calc);
        let v = self.wrb.get(itvhi);
        set_dependent(h, WideRangeBalanceMod::PMOD, v, itv, calc);
        let v = self.wrr.get(itvhi);
        set_dependent(h, WideRangeRollMod::PMOD, v, itv, calc);
        let v = self.wrjt.get(itvhi);
        set_dependent(h, WideRangeJumptrillMod::PMOD, v, itv, calc);
        let v = self.wrjj.get(itvhi);
        set_dependent(h, WideRangeJjMod::PMOD, v, itv, calc);
        let v = self.wra.get(itvhi, &self.seq.as_);
        set_dependent(h, WideRangeAnchorMod::PMOD, v, itv, calc);
        let v = self.wrcja.get(itvhi, &self.seq.as_);
        set_dependent(h, WideRangeCjAnchorMod::PMOD, v, itv, calc);
        let v = self.mj.get(itvhi);
        set_dependent(h, MinijackMod::PMOD, v, itv, calc);
        let v = self.old_anch.get(itvhi);
        set_dependent(h, OldAnchorScalerMod::PMOD, v, itv, calc);
        let v = self.old_ohj.get(itvhi);
        set_dependent(h, OldOhjScalerMod::PMOD, v, itv, calc);
        let v = self.old_roll.get(itvhi);
        set_dependent(h, OldRollScalerMod::PMOD, v, itv, calc);
    }

    fn full_hand_reset(&mut self) {
        self.ohj.full_reset();
        self.chain.full_reset();
        self.cjohj.full_reset();
        self.bal.full_reset();
        self.roll.0.full_reset();
        self.rolljs.0.full_reset();
        self.oht.0.full_reset();
        self.voht.0.full_reset();
        self.ch.full_reset();
        self.rm.full_reset();
        self.wrr.full_reset();
        self.wrjt.full_reset();
        self.wrjj.full_reset();
        self.wrb.full_reset();
        self.wra.full_reset();
        self.wrcja.full_reset();
        self.mj.full_reset();
        self.old_anch.full_reset();
        self.old_ohj.full_reset();
        self.old_roll.full_reset();
        self.seq.full_reset();
        self.mitvhi.zero();
        self.mhi.full_reset();
        self.last_mhi.full_reset();
        self.diffz.full_reset();
    }

    fn handle_dependent_interval_end(&mut self, itv: usize, calc: &mut Calc) {
        // Updates the hand counts, so it must come first.
        self.mitvhi.interval_end();
        self.seq.interval_end();
        self.set_dependent_pmods(itv, calc);
        self.set_sequenced_base_diffs(itv, calc);
        self.diffz.interval_end();
    }

    fn update_sequenced_base_diffs(&mut self, ct: ColType, row_time: f32, calc: &mut Calc) {
        let mut jack =
            ms_to_scaled_nps(self.seq.as_.get_lowest_jack_ms()) * BASESCALERS[SKILL_JACKSPEED];
        if jack.is_nan() {
            jack = 0.0;
        }
        calc.jack_diff[self.hand].push((row_time, jack));
        self.diffz.tc.advance_base(&self.seq, ct, calc);
        self.diffz
            .tc
            .advance_rm_comp(self.rm.get_highest_anchor_difficulty());
    }

    fn set_sequenced_base_diffs(&self, itv: usize, calc: &mut Calc) {
        let nps_base = calc.init_base_diff_vals[self.hand][diff::NPS_BASE][itv];
        calc.init_base_diff_vals[self.hand][diff::TECH_BASE][itv] =
            self.diffz.tc.get_itv_diff(nps_base, calc);
    }

    fn run_dependent_pmod_loop(&mut self, calc: &mut Calc) {
        self.setup_dependent_mods();
        for ids in calc.hand_col_masks {
            let mut last_row_time = S_INIT;
            self.full_hand_reset();
            calc.jack_diff[self.hand].clear();

            actual_cancer(calc, self.hand);
            let numitv = calc.numitv;
            smooth(
                &mut calc.init_base_diff_vals[self.hand][diff::NPS_BASE],
                0.0,
                numitv,
            );

            for itv in 0..calc.numitv {
                for row in 0..calc.itv_size[itv] {
                    let ri = calc.rows[calc.itv_start[itv] + row];
                    let row_time = ri.row_time;
                    let row_notes = ri.row_notes;
                    let row_count = ri.row_count;
                    let any_ms = ms_from(row_time, last_row_time);
                    let ct = determine_col_type(row_notes, ids);
                    if ct == ColType::Empty {
                        self.rm.advance_off_hand_sequencing();
                        self.mj.advance_off_hand_sequencing();
                        if row_count == 2 {
                            self.rm.advance_off_hand_sequencing();
                        }
                        continue;
                    }
                    self.seq.advance_sequencing(ct, row_time, any_ms);
                    let last = self.last_mhi;
                    self.mhi.advance(&last, ct);
                    self.mitvhi.itvhi.set_col_taps(ct);
                    self.handle_row_dependent_pattern_advancement(row_time);
                    self.update_sequenced_base_diffs(ct, row_time, calc);
                    if self.mhi.bt != BaseType::Init {
                        self.mitvhi.base_types[self.mhi.bt.idx()] += 1;
                        self.mitvhi.meta_types[self.mhi.mt.idx()] += 1;
                    }
                    std::mem::swap(&mut self.last_mhi, &mut self.mhi);
                    last_row_time = row_time;
                }
                self.handle_dependent_interval_end(itv, calc);
            }
            run_dependent_smoothing_pass(calc.numitv, calc);
            self.hand += 1;
        }
        grindscale(calc);
    }

    /// `adj_diff_func`: a skillset's difficulty an interval from its
    /// adjusted NPS base.
    #[allow(clippy::too_many_arguments)]
    pub fn adj_diff_func(
        &self,
        itv: usize,
        hand: usize,
        adj_diff: &mut f32,
        stam_base: &mut f32,
        adj_npsbase: f32,
        ss: usize,
        pmod_product_cur_interval: &[f32; NUM_SKILLSET],
        calc: &Calc,
    ) {
        let pmods = &calc.pmod_vals[hand];
        let nps_base = calc.init_base_diff_vals[hand][diff::NPS_BASE][itv];
        match ss {
            SKILL_JUMPSTREAM => {
                *adj_diff /= max(pmods[pmod::HS][itv], 1.0);
                *adj_diff /= fastsqrt(pmods[pmod::OH_JUMP_MOD][itv] * 0.95);
                let a = *adj_diff;
                let b = nps_base * pmod_product_cur_interval[SKILL_HANDSTREAM];
                *stam_base = max(a, b);
            }
            SKILL_HANDSTREAM => {
                let a = adj_npsbase;
                let b = nps_base * pmod_product_cur_interval[SKILL_JUMPSTREAM];
                *stam_base = max(a, b);
            }
            SKILL_CHORDJACK => {
                *adj_diff *= fastsqrt(pmods[pmod::OLD_OHJ_SCALER][itv]);
            }
            SKILL_TECHNICAL => {
                *adj_diff = calc.init_base_diff_vals[hand][diff::TECH_BASE][itv]
                    * pmod_product_cur_interval[ss]
                    * BASESCALERS[ss]
                    / max(fastpow(f64::from(pmods[pmod::CJ][itv] + 0.05), 2.0), 1.0);
                *adj_diff *= fastsqrt(pmods[pmod::OH_JUMP_MOD][itv]);
            }
            _ => {}
        }
    }

    /// Chordjack scaled by how much of the file is chords.
    pub fn apply_keymode_multipliers(&self, values: &mut [f32; NUM_SKILLSET]) {
        let qprop = self.propz.get_prop(tap_size::QUAD);
        let hprop = self.propz.get_prop(tap_size::HAND);
        let jprop = self.propz.get_prop(tap_size::JUMP);
        let magic_number_awesome = 0.2f32;
        let another_magic_number = 1.015f32;
        let definitelycj = clamp(qprop + hprop + jprop + magic_number_awesome, 0.5, 1.0);
        values[SKILL_CHORDJACK] *= definitelycj * another_magic_number;
    }
}
