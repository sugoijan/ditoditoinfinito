// Translated to Rust from Etterna's difficulty calculator, MinaCalc:
// https://github.com/etternagame/etterna, src/Etterna/MinaCalc/Dependent/HD_Sequencers/RMSequencing.h
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

//! Runningmen: an anchor with off-anchor taps, one sequencer a finger.

use super::generic_sequencing::{AnchStatus, FingerSequencing};
use crate::dependent::hd_basic_sequencing::{BaseType, ColType};
use crate::dependent::hd_meta_sequencing::MetaType;
use crate::sequencing_helpers::{S_INIT, ms_from, ms_to_scaled_nps};

/// 1.06 * the 4k tech base scaler.
pub const RMA_DIFF_SCALER: f32 = 1.06 * 1.06;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RmBehavior {
    OffTapOh,
    OffTapSh,
    Anchor,
    Jack,
    Init,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RmStatus {
    Inactive,
    Running,
}

/// One runningman (its anchor on one finger).
#[derive(Clone, Copy, Debug, Default)]
pub struct RunningMan {
    pub ran_taps: i32,
    pub len: i32,
    pub off_taps: i32,
    pub off_len: i32,
    pub off_taps_sh: i32,
    pub oht_taps: i32,
    pub oht_len: i32,
    pub ot_sh_len: i32,
    pub jack_taps: i32,
    pub jack_len: i32,
    pub anch_len: i32,
}

impl RunningMan {
    pub fn full_reset(&mut self) {
        self.len = 0;
        self.off_taps_sh = 0;
        self.off_taps = 0;
        self.off_len = 0;
        self.oht_taps = 0;
        self.oht_len = 0;
        self.jack_taps = 0;
        self.jack_len = 0;
        self.anch_len = 0;
    }

    fn add_off_tap_sh(&mut self) {
        self.off_taps_sh += 1;
        self.ot_sh_len += 1;
        self.add_off_tap();
    }

    fn add_off_tap(&mut self) {
        self.off_len += 1;
        self.off_taps += 1;
        self.ran_taps += 1;
    }

    fn add_oht_tap(&mut self) {
        self.oht_len += 1;
        self.oht_taps += 1;
    }

    fn add_anchor_tap(&mut self) {
        self.len += 1;
        self.anch_len += 1;
        self.ran_taps += 1;
    }

    fn add_jack_tap(&mut self) {
        self.jack_len += 1;
        self.jack_taps += 1;
        self.ran_taps += 1;
    }

    fn end_jack_and_anch_runs(&mut self) {
        self.end_anch_run();
        self.end_jack_run();
    }

    fn end_anch_run(&mut self) {
        self.anch_len = 0;
    }

    fn end_jack_run(&mut self) {
        self.jack_len = 0;
    }

    fn end_off_tap_run(&mut self) {
        self.off_len = 0;
        self.ot_sh_len = 0;
    }

    fn restart(&mut self) {
        self.off_taps_sh = 0;
        self.off_taps = 0;
        self.off_len = 0;
        self.oht_taps = 0;
        self.oht_len = 0;
        self.jack_taps = 0;
        self.jack_len = 0;
        self.anch_len = 0;
    }

    pub fn get_off_tap_prop(&self) -> f32 {
        if self.off_taps == 0 {
            return 0.0;
        }
        self.len as f32 / self.off_taps as f32
    }

    /// Off hand taps to anchor length.
    pub fn get_offhand_tap_prop(&self) -> f32 {
        if self.off_taps - self.off_taps_sh <= 0 {
            return 0.0;
        }
        (self.off_taps - self.off_taps_sh) as f32 / self.len as f32
    }

    /// Same hand taps to anchor length.
    pub fn get_off_tap_same_prop(&self) -> f32 {
        if self.off_taps_sh == 0 {
            return 0.0;
        }
        self.off_taps_sh as f32 / self.len as f32
    }
}

/// `RM_Sequencer`.
#[derive(Clone, Copy, Debug)]
pub struct RmSequencer {
    max_oht_len: i32,
    max_off_len: i32,
    max_ot_sh_len: i32,
    max_burst_len: i32,
    max_jack_len: i32,
    max_anchor_len: i32,

    pub ct: ColType,
    pub status: RmStatus,
    pub rmb: RmBehavior,
    pub last_rmb: RmBehavior,
    pub rm: RunningMan,
    is_bursting: bool,
    had_burst: bool,
    pub last_anchor_time: f32,
    pub start: f32,
}

impl Default for RmSequencer {
    fn default() -> Self {
        RmSequencer {
            max_oht_len: 0,
            max_off_len: 0,
            max_ot_sh_len: 0,
            max_burst_len: 0,
            max_jack_len: 0,
            max_anchor_len: 0,
            ct: ColType::Init,
            status: RmStatus::Inactive,
            rmb: RmBehavior::Init,
            last_rmb: RmBehavior::Init,
            rm: RunningMan::default(),
            is_bursting: false,
            had_burst: false,
            last_anchor_time: S_INIT,
            start: S_INIT,
        }
    }
}

impl RmSequencer {
    pub fn set_params(
        &mut self,
        moht: f32,
        moff: f32,
        motsh: f32,
        mburst: f32,
        mjack: f32,
        manch: f32,
    ) {
        self.max_oht_len = moht as i32;
        self.max_off_len = moff as i32;
        self.max_ot_sh_len = motsh as i32;
        self.max_burst_len = mburst as i32;
        self.max_jack_len = mjack as i32;
        self.max_anchor_len = manch as i32;
    }

    pub fn full_reset(&mut self) {
        self.status = RmStatus::Inactive;
        self.rmb = RmBehavior::Init;
        self.last_rmb = RmBehavior::Init;
        self.start = S_INIT;
        self.last_anchor_time = S_INIT;
        self.is_bursting = false;
        self.had_burst = false;
        self.rm.full_reset();
    }

    fn restart(&mut self, as_: &FingerSequencing) {
        self.start = as_.last - (as_.sc_ms / 1000.0);
        self.last_anchor_time = as_.last;
        self.rm.len = 2;
        self.rm.ran_taps = 2;
        self.is_bursting = false;
        self.had_burst = false;
        self.rm.restart();
        self.handle_last_rmb();
    }

    fn should_restart(&self) -> bool {
        self.last_rmb == RmBehavior::OffTapSh
    }

    fn end_off_tap_run(&mut self) {
        if self.is_bursting {
            self.is_bursting = false;
            self.had_burst = true;
        }
        self.rm.end_off_tap_run();
    }

    fn handle_last_rmb(&mut self) {
        if self.last_rmb == RmBehavior::OffTapSh {
            self.rm.add_off_tap_sh();
        }
    }

    fn off_len_exceeds_max(&mut self) -> bool {
        if self.rm.off_len <= self.max_off_len {
            return false;
        }
        if self.had_burst || self.rm.off_len > self.max_burst_len {
            return true;
        }
        self.is_bursting = true;
        false
    }

    fn ot_sh_len_exceeds_max(&self) -> bool {
        self.rm.ot_sh_len > self.max_ot_sh_len
    }

    fn jack_len_exceeds_max(&self) -> bool {
        self.rm.jack_len > self.max_jack_len
    }

    fn anch_len_exceeds_max(&self) -> bool {
        self.rm.anch_len > self.max_anchor_len
    }

    fn oht_len_exceeds_max(&self) -> bool {
        self.rm.oht_len > self.max_oht_len
    }

    fn handle_anchor_behavior(&mut self, as_: &FingerSequencing) {
        if self.anch_len_exceeds_max() {
            self.status = RmStatus::Inactive;
            return;
        }
        match as_.status {
            AnchStatus::ResetTooSlow | AnchStatus::ResetTooFast => {
                if self.should_restart() {
                    self.restart(as_);
                } else {
                    self.status = RmStatus::Inactive;
                }
            }
            AnchStatus::Anchoring => {
                self.rm.add_anchor_tap();
                // RunningMan's, not the sequencer's (no burst handling).
                self.rm.end_off_tap_run();
            }
            AnchStatus::AnchInit => {}
        }
    }

    fn handle_off_tap_sh_behavior(&mut self) {
        self.rm.add_off_tap_sh();
        if self.off_len_exceeds_max() || self.ot_sh_len_exceeds_max() {
            self.status = RmStatus::Inactive;
        } else {
            self.rm.end_jack_and_anch_runs();
        }
    }

    fn handle_off_tap_oh_behavior(&mut self) {
        self.rm.add_off_tap();
        if self.off_len_exceeds_max() {
            self.status = RmStatus::Inactive;
        } else {
            self.rm.end_jack_run();
        }
    }

    fn handle_jack_behavior(&mut self) {
        self.rm.add_jack_tap();
        if self.jack_len_exceeds_max() {
            self.status = RmStatus::Inactive;
        } else {
            self.end_off_tap_run();
        }
    }

    fn handle_oht_behavior(&mut self, ct: ColType) {
        if ct != self.ct {
            if self.rm.oht_len == 0 {
                self.rm.add_oht_tap();
            }
            self.rm.add_oht_tap();
            if self.oht_len_exceeds_max() {
                self.status = RmStatus::Inactive;
            }
        }
    }

    fn handle_rmb(&mut self, as_: &FingerSequencing) {
        match self.rmb {
            RmBehavior::OffTapSh => self.handle_off_tap_sh_behavior(),
            RmBehavior::Anchor => self.handle_anchor_behavior(as_),
            RmBehavior::Jack => self.handle_jack_behavior(),
            _ => {}
        }
    }

    /// Off hand information, before the empty row's `continue`.
    pub fn advance_off_hand_sequencing(&mut self) {
        self.handle_off_tap_oh_behavior();
        self.last_rmb = RmBehavior::OffTapOh;
    }

    /// `operator()`.
    pub fn advance(&mut self, ct: ColType, bt: BaseType, mt: MetaType, as_: &FingerSequencing) {
        if mt == MetaType::Cccccc {
            self.handle_oht_behavior(ct);
        }
        self.last_anchor_time = as_.last;
        if self.last_anchor_time == S_INIT {
            self.full_reset();
            return;
        }
        match bt {
            BaseType::LeftRight | BaseType::RightLeft | BaseType::SingleSingle => {
                self.rmb = if self.ct == ct {
                    RmBehavior::Anchor
                } else {
                    RmBehavior::OffTapSh
                };
            }
            BaseType::JumpSingle => {
                if self.last_rmb == RmBehavior::OffTapOh {
                    self.rmb = if self.ct == ct {
                        RmBehavior::Anchor
                    } else {
                        RmBehavior::OffTapSh
                    };
                } else {
                    self.rmb = RmBehavior::Jack;
                }
            }
            BaseType::SingleJump | BaseType::JumpJump => {
                self.rmb = if self.last_rmb == RmBehavior::OffTapOh {
                    RmBehavior::Anchor
                } else {
                    RmBehavior::Jack
                };
            }
            BaseType::Init => return,
        }
        if self.status == RmStatus::Inactive {
            if self.rmb == RmBehavior::Anchor && self.last_rmb == RmBehavior::OffTapSh {
                self.status = RmStatus::Running;
                self.restart(as_);
            }
        } else {
            self.handle_rmb(as_);
        }
        self.last_rmb = self.rmb;
    }

    pub fn get_difficulty(&self) -> f32 {
        if self.status == RmStatus::Inactive || self.rm.len < 3 {
            return 1.0;
        }
        let flool = ms_from(self.last_anchor_time, self.start);
        let len = self.rm.len as f32;
        let len_1 = (self.rm.len - 1) as f32;
        let pule = (flool / len_1) * (len / len_1);
        ms_to_scaled_nps(pule) * RMA_DIFF_SCALER
    }
}
