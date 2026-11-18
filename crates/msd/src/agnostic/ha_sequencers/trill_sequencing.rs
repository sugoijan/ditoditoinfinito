// Translated to Rust from Etterna's difficulty calculator, MinaCalc:
// https://github.com/etternagame/etterna, src/Etterna/MinaCalc/Agnostic/HA_Sequencers/TrillSequencing.h
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

//! Two hand trills (`THT_Sequencing`), for the stream mod.

use crate::agnostic::interval_info::tap_size;
use crate::agnostic::meta_interval_info::MetaItvInfo;
use crate::calc_window::CalcMovingWindow;
use crate::dependent::hd_basic_sequencing::{ColType, determine_col_type};
use crate::minacalc::{LEFT_HAND, RIGHT_HAND};
use crate::pattern_mod_helpers::{clamp, max, weighted_average};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Trillnario {
    TheLackThereof,
    NeedOpposite,
}

#[derive(Clone, Debug)]
pub struct TwoHandTrill {
    current_hand: usize,
    left_hand_state: ColType,
    right_hand_state: ColType,
    current_scenario: Trillnario,
    /// A trill length 1 is 2 notes long and has "completed".
    pub cur_length: i32,
    pub jump_count: i32,
    trill_ms: CalcMovingWindow<f32>,
    pub trills_in_interval: i32,
    pub total_taps: i32,
    pub cv_threshold: f32,
    last_notes: u32,
    last_last_notes: u32,
}

impl Default for TwoHandTrill {
    fn default() -> Self {
        TwoHandTrill {
            current_hand: LEFT_HAND,
            left_hand_state: ColType::Init,
            right_hand_state: ColType::Init,
            current_scenario: Trillnario::TheLackThereof,
            cur_length: 0,
            jump_count: 0,
            trill_ms: CalcMovingWindow::default(),
            trills_in_interval: 0,
            total_taps: 0,
            cv_threshold: 0.5,
            last_notes: 0,
            last_last_notes: 0,
        }
    }
}

impl TwoHandTrill {
    fn two_hand_jump(notes: u32) -> bool {
        (notes & 0b1100) != 0 && (notes & 0b0011) != 0
    }

    pub fn process(&mut self, ms_now: f32, notes: u32) {
        let notes_hand;
        let notes_coltype;
        let prev_notes = self.last_notes;
        let prev_prev_notes = self.last_last_notes;
        self.last_last_notes = self.last_notes;
        self.last_notes = notes;

        if Self::two_hand_jump(notes) {
            if Self::two_hand_jump(prev_notes) {
                self.dead_trill();
                return;
            }
            if Self::two_hand_jump(prev_prev_notes) {
                self.dead_trill();
                return;
            }
            if (notes & 0b1111) == 0b1111 {
                self.dead_trill();
                return;
            } else if (notes & 0b1100) == 0b1100 {
                notes_hand = LEFT_HAND;
                notes_coltype = ColType::OhJump;
            } else if (notes & 0b0011) == 0b0011 {
                notes_hand = RIGHT_HAND;
                notes_coltype = ColType::OhJump;
            } else if (notes & prev_prev_notes) != 0 && (notes & prev_notes) == 0 {
                let ppn = notes & prev_prev_notes;
                if (ppn & 0b1100) != 0 {
                    notes_hand = LEFT_HAND;
                    notes_coltype = determine_col_type(notes, 0b1100);
                } else if (ppn & 0b0011) != 0 {
                    notes_hand = RIGHT_HAND;
                    notes_coltype = determine_col_type(notes, 0b0011);
                } else {
                    self.dead_trill();
                    return;
                }
            } else {
                self.dead_trill();
                return;
            }
        } else if (notes & 0b1100) != 0 {
            notes_hand = LEFT_HAND;
            notes_coltype = determine_col_type(notes, 0b1100);
        } else if (notes & 0b0011) != 0 {
            notes_hand = RIGHT_HAND;
            notes_coltype = determine_col_type(notes, 0b0011);
        } else {
            self.reset();
            return;
        }

        match self.current_scenario {
            Trillnario::TheLackThereof => {
                self.current_hand = notes_hand;
                self.set_hand_states(notes_coltype);
                self.current_scenario = Trillnario::NeedOpposite;
            }
            Trillnario::NeedOpposite => {
                if self.current_hand == notes_hand {
                    self.dead_trill();
                } else {
                    if notes_coltype == ColType::OhJump {
                        self.jump_count += 1;
                    }
                    if self.current_hand == LEFT_HAND {
                        if self.right_hand_state == ColType::Init
                            || self.right_hand_state == notes_coltype
                        {
                            self.right_hand_state = notes_coltype;
                            self.calc_trill(ms_now);
                            self.cur_length += 1;
                            self.total_taps += 1;
                        } else {
                            self.dead_trill();
                            self.right_hand_state = notes_coltype;
                            self.current_scenario = Trillnario::NeedOpposite;
                        }
                    } else if self.left_hand_state == ColType::Init
                        || self.left_hand_state == notes_coltype
                    {
                        self.left_hand_state = notes_coltype;
                        self.calc_trill(ms_now);
                        self.cur_length += 1;
                        self.total_taps += 1;
                    } else {
                        self.dead_trill();
                        self.left_hand_state = notes_coltype;
                        self.current_scenario = Trillnario::NeedOpposite;
                    }
                }
            }
        }
    }

    fn set_hand_states(&mut self, coltype: ColType) {
        if self.current_hand == LEFT_HAND {
            self.left_hand_state = coltype;
        } else {
            self.right_hand_state = coltype;
        }
    }

    fn calc_trill(&mut self, ms_now: f32) -> bool {
        self.trill_ms.push(ms_now);
        true
    }

    /// Upstream's reset leaves `jump_count` alone.
    pub fn reset(&mut self) {
        self.trills_in_interval = 0;
        self.total_taps = 0;
        self.cur_length = 0;
        self.trill_ms.zero();
        self.current_scenario = Trillnario::TheLackThereof;
        self.left_hand_state = ColType::Init;
        self.right_hand_state = ColType::Init;
        self.last_notes = 0;
        self.last_last_notes = 0;
    }

    fn dead_trill(&mut self) {
        self.cur_length = 0;
        self.trill_ms.zero();
        self.current_scenario = Trillnario::TheLackThereof;
        self.left_hand_state = ColType::Init;
        self.right_hand_state = ColType::Init;
        self.last_notes = 0;
        self.last_last_notes = 0;
    }
}

/// Two Hand Trill Sequencing.
#[derive(Clone, Debug)]
pub struct ThtSequencing {
    pub trill: TwoHandTrill,
    trill_buffer: f32,
    trill_scaler: f32,
    jump_buffer: f32,
    jump_scaler: f32,
    jump_weight: f32,
    min_val: f32,
    max_val: f32,
}

impl Default for ThtSequencing {
    fn default() -> Self {
        ThtSequencing {
            trill: TwoHandTrill::default(),
            trill_buffer: 0.0,
            trill_scaler: 1.0,
            jump_buffer: 0.0,
            jump_scaler: 1.0,
            jump_weight: 0.5,
            min_val: 0.0,
            max_val: 1.5,
        }
    }
}

/// Upstream's `_jump_size`, which indexes `taps_by_size` at hands.
const JUMP_SIZE: usize = 2;
const _: () = assert!(JUMP_SIZE == tap_size::HAND);

impl ThtSequencing {
    #[allow(clippy::too_many_arguments)]
    pub fn set_params(
        &mut self,
        cv: f32,
        tbuffer: f32,
        tscaler: f32,
        jbuffer: f32,
        jscaler: f32,
        jweight: f32,
        min: f32,
        max: f32,
    ) {
        self.trill.cv_threshold = cv;
        self.trill_buffer = tbuffer;
        self.trill_scaler = tscaler;
        self.jump_buffer = jbuffer;
        self.jump_scaler = jscaler;
        self.jump_weight = jweight;
        self.min_val = min;
        self.max_val = max;
    }

    /// `operator()`.
    pub fn advance(&mut self, ms_now: f32, notes: u32) {
        self.trill.process(ms_now, notes);
    }

    pub fn reset(&mut self) {
        self.trill.reset();
    }

    /// Like a proportion: 1 is "all trills".
    pub fn get(&self, mitvi: &MetaItvInfo) -> f32 {
        let trill_taps = self.trill.total_taps as f32;
        let trill_jumps = self.trill.jump_count as f32;
        let itvi = &mitvi.itvi;
        let taps = itvi.total_taps as f32;
        if taps == 0.0 {
            return 0.0;
        }
        let jumps = itvi.taps_by_size[JUMP_SIZE] as f32;
        let trill_proportion = (trill_taps + self.trill_buffer)
            / max(taps - self.trill_buffer, 1.0)
            * self.trill_scaler;
        let jump_proportion = (jumps - trill_jumps + self.jump_buffer)
            / max(taps - self.jump_buffer, 1.0)
            * self.jump_scaler;
        let prop = weighted_average(
            trill_proportion,
            jump_proportion,
            clamp(1.0 - self.jump_weight, 0.0, 1.0),
            1.0,
        );
        clamp(prop, self.min_val, self.max_val)
    }
}
