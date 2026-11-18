// Translated to Rust from Etterna's difficulty calculator, MinaCalc:
// https://github.com/etternagame/etterna, src/Etterna/MinaCalc/Dependent/HD_Sequencers/GenericSequencing.h
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

//! Anchors and jacks a finger, and the hand's timings (any, same column
//! and cross column milliseconds) the pattern mods read.

use crate::calc_window::CalcMovingWindow;
use crate::dependent::hd_basic_sequencing::{
    CT_LOOP_NO_JUMPS, ColType, NUM_COLS_PER_HAND, invert_col,
};
use crate::pattern_mod_helpers::min;
use crate::sequencing_helpers::{MS_INIT, S_INIT, ms_from};

pub const ANCHOR_SPACING_BUFFER_MS: f32 = 10.0;
pub const ANCHOR_SPEED_INCREASE_CUTOFF_FACTOR: f32 = 2.34;
pub const ANCHOR_LEN_CAP: i32 = 50;

pub const JACK_SPACING_BUFFER_MS: f32 = 10.0;
pub const JACK_SPEED_INCREASE_CUTOFF_FACTOR: f32 = 1.9;
pub const JACK_LEN_CAP: i32 = 4;

/// Milliseconds that certainly end an anchor.
pub const GUARANTEED_RESET_BUFFER_MS: f32 = 1000.0;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AnchStatus {
    ResetTooSlow,
    ResetTooFast,
    Anchoring,
    AnchInit,
}

/// Which `Finger_Sequencing` this is.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FingerKind {
    Anchor,
    Jack,
}

/// `Finger_Sequencing`, as `Anchor_Sequencing` or `Jack_Sequencing`.
#[derive(Clone, Copy, Debug)]
pub struct FingerSequencing {
    pub kind: FingerKind,
    pub ct: ColType,
    pub status: AnchStatus,
    pub len: i32,
    /// Same-column ms: between now and the previous tap.
    pub sc_ms: f32,
    pub max_ms: f32,
    pub len_cap_ms: f32,
    /// Row time of the last note on this column.
    pub last: f32,
    pub start: f32,
}

impl FingerSequencing {
    pub fn new(kind: FingerKind, ct: ColType) -> Self {
        FingerSequencing {
            kind,
            ct,
            status: AnchStatus::AnchInit,
            len: 1,
            sc_ms: MS_INIT,
            max_ms: MS_INIT,
            len_cap_ms: MS_INIT,
            last: S_INIT,
            start: S_INIT,
        }
    }

    pub fn full_reset(&mut self) {
        self.sc_ms = MS_INIT;
        self.max_ms = MS_INIT;
        self.last = S_INIT;
        self.start = S_INIT;
        self.len = 1;
        self.status = AnchStatus::AnchInit;
        self.len_cap_ms = MS_INIT;
    }

    fn check_status(&mut self) {
        match self.status {
            AnchStatus::ResetTooSlow | AnchStatus::ResetTooFast => {
                self.start = self.last;
                self.len = 2;
            }
            AnchStatus::Anchoring => self.len += 1,
            AnchStatus::AnchInit => {}
        }
    }

    fn set_status(&mut self) {
        let (buffer, cutoff) = match self.kind {
            FingerKind::Jack => (JACK_SPACING_BUFFER_MS, JACK_SPEED_INCREASE_CUTOFF_FACTOR),
            FingerKind::Anchor => (
                ANCHOR_SPACING_BUFFER_MS,
                ANCHOR_SPEED_INCREASE_CUTOFF_FACTOR,
            ),
        };
        self.status = if self.sc_ms > self.max_ms + buffer {
            AnchStatus::ResetTooSlow
        } else if self.sc_ms * cutoff < self.max_ms {
            AnchStatus::ResetTooFast
        } else {
            AnchStatus::Anchoring
        };
    }

    /// `operator()`.
    pub fn advance(&mut self, ct: ColType, now: f32) {
        self.sc_ms = ms_from(now, self.last);
        if ct == ColType::Init {
            self.last = now;
            return;
        }
        self.set_status();
        self.check_status();
        self.max_ms = self.sc_ms;
        self.last = now;
    }

    /// `Jack_Sequencing::get_ms`: an adjusted ms average, not nps. It
    /// remembers its value at the length cap, as upstream's does.
    pub fn get_jack_ms(&mut self) -> f32 {
        if self.len > JACK_LEN_CAP {
            return self.len_cap_ms;
        }
        const AVG_MS_MULT: f32 = 1.5;
        const ANCHOR_TIME_BUFFER_MS: f32 = 30.0;
        const MIN_MS: f32 = 95.0;
        let total_ms = ms_from(self.last, self.start);
        let len = (self.len - 1) as f32;
        let avg_ms = total_ms / len;
        let adj_total_ms = total_ms + ANCHOR_TIME_BUFFER_MS + avg_ms * AVG_MS_MULT;
        let mut ms = adj_total_ms / len;
        if self.len == 2 {
            ms *= 1.1;
            ms = if ms < 180.0 { 180.0 } else { ms };
        }
        ms = if ms < MIN_MS { MIN_MS } else { ms };
        if ms.is_nan() {
            ms = self.max_ms;
        }
        if self.len == JACK_LEN_CAP {
            self.len_cap_ms = ms;
        }
        ms
    }
}

/// `AnchorSequencer`: an anchor and a jack sequencer a finger.
#[derive(Clone, Debug)]
pub struct AnchorSequencer {
    pub anch: [FingerSequencing; NUM_COLS_PER_HAND],
    pub jack: [FingerSequencing; NUM_COLS_PER_HAND],
    pub max_seen: [i32; NUM_COLS_PER_HAND],
    pub mw_max: [CalcMovingWindow<i32>; NUM_COLS_PER_HAND],
}

impl Default for AnchorSequencer {
    fn default() -> Self {
        let mut s = AnchorSequencer {
            anch: [
                FingerSequencing::new(FingerKind::Anchor, ColType::Left),
                FingerSequencing::new(FingerKind::Anchor, ColType::Right),
            ],
            jack: [
                FingerSequencing::new(FingerKind::Jack, ColType::Left),
                FingerSequencing::new(FingerKind::Jack, ColType::Right),
            ],
            max_seen: [0; NUM_COLS_PER_HAND],
            mw_max: Default::default(),
        };
        s.full_reset();
        s
    }
}

impl AnchorSequencer {
    pub fn full_reset(&mut self) {
        self.max_seen = [0; NUM_COLS_PER_HAND];
        for c in CT_LOOP_NO_JUMPS {
            self.anch[c.idx()].full_reset();
            self.jack[c.idx()].full_reset();
            self.mw_max[c.idx()].zero();
        }
    }

    /// `operator()`.
    pub fn advance(&mut self, ct: ColType, row_time: f32) {
        if ct == ColType::Left || ct == ColType::Right {
            let c = ct.idx();
            let opposite = invert_col(ct).idx();
            self.anch[c].advance(ct, row_time);
            self.jack[c].advance(ct, row_time);
            self.max_seen[c] = if self.anch[c].len > self.max_seen[c] {
                self.anch[c].len
            } else {
                self.max_seen[c]
            };
            if ms_from(row_time, self.anch[opposite].last) > GUARANTEED_RESET_BUFFER_MS {
                self.anch[opposite].full_reset();
                self.jack[opposite].full_reset();
            }
        } else if ct == ColType::OhJump {
            for c in CT_LOOP_NO_JUMPS {
                let i = c.idx();
                self.anch[i].advance(c, row_time);
                self.jack[i].advance(c, row_time);
                self.max_seen[i] = if self.anch[i].len > self.max_seen[i] {
                    self.anch[i].len
                } else {
                    self.max_seen[i]
                };
            }
        }
    }

    pub fn get_max_for_window_and_col(&self, ct: ColType, window: usize) -> i32 {
        self.mw_max[ct.idx()].get_max_for_window(window)
    }

    pub fn interval_end(&mut self) {
        for c in CT_LOOP_NO_JUMPS {
            let i = c.idx();
            self.mw_max[i].push(self.max_seen[i]);
            self.max_seen[i] = 0;
        }
    }

    pub fn get_lowest_jack_ms(&mut self) -> f32 {
        let l = self.jack[ColType::Left.idx()].get_jack_ms();
        let r = self.jack[ColType::Right.idx()].get_jack_ms();
        min(l, r)
    }
}

/// `SequencerGeneral`: the timings of a hand.
#[derive(Clone, Debug, Default)]
pub struct SequencerGeneral {
    /// Row with something on this hand to the last such row.
    pub mw_any_ms: CalcMovingWindow<f32>,
    pub mw_cc_ms: CalcMovingWindow<f32>,
    pub mw_sc_ms: [CalcMovingWindow<f32>; NUM_COLS_PER_HAND],
    pub as_: AnchorSequencer,
}

impl SequencerGeneral {
    fn set_sc_ms(&mut self, ct: ColType) {
        if ct == ColType::Left || ct == ColType::Right {
            self.mw_sc_ms[ct.idx()].push(self.as_.anch[ct.idx()].sc_ms);
        }
        if ct == ColType::OhJump {
            for c in CT_LOOP_NO_JUMPS {
                self.mw_sc_ms[c.idx()].push(self.as_.anch[c.idx()].sc_ms);
            }
        }
    }

    fn set_cc_ms(&mut self, ct: ColType, row_time: f32) {
        if ct == ColType::Left || ct == ColType::Right {
            let last = self.as_.anch[invert_col(ct).idx()].last;
            self.mw_cc_ms.push(ms_from(row_time, last));
        }
        if ct == ColType::OhJump {
            let v = self.get_sc_ms_now(ColType::OhJump, true);
            self.mw_cc_ms.push(v);
        }
    }

    pub fn advance_sequencing(&mut self, ct: ColType, row_time: f32, ms_now: f32) {
        if ct != ColType::OhJump {
            let reset_sequencer =
                ms_from(row_time, self.as_.anch[ct.idx()].last) > GUARANTEED_RESET_BUFFER_MS;
            if reset_sequencer {
                self.as_.anch[ct.idx()].full_reset();
                self.as_.jack[ct.idx()].full_reset();
                self.mw_sc_ms[ct.idx()].fill(MS_INIT);
                self.mw_cc_ms.fill(MS_INIT);
                self.mw_any_ms.fill(MS_INIT);
            }
        }
        self.as_.advance(ct, row_time);
        self.set_sc_ms(ct);
        self.set_cc_ms(ct, row_time);
        self.mw_any_ms.push(ms_now);
    }

    pub fn get_sc_ms_now(&self, ct: ColType, lower: bool) -> f32 {
        if ct == ColType::Init {
            return MS_INIT;
        }
        if ct == ColType::OhJump {
            let l = self.mw_sc_ms[ColType::Left.idx()].get_now();
            let r = self.mw_sc_ms[ColType::Right.idx()].get_now();
            if lower {
                return if l < r { l } else { r };
            }
            return if l > r { l } else { r };
        }
        self.mw_sc_ms[ct.idx()].get_now()
    }

    pub fn get_any_ms_now(&self) -> f32 {
        self.mw_any_ms.get_now()
    }

    pub fn get_cc_ms_now(&self) -> f32 {
        self.mw_cc_ms.get_now()
    }

    pub fn interval_end(&mut self) {
        self.as_.interval_end();
    }

    pub fn full_reset(&mut self) {
        self.mw_any_ms.fill(MS_INIT);
        self.mw_cc_ms.fill(MS_INIT);
        for c in CT_LOOP_NO_JUMPS {
            self.mw_sc_ms[c.idx()].fill(MS_INIT);
        }
        self.as_.full_reset();
    }
}
