// Translated to Rust from Etterna's difficulty calculator, MinaCalc:
// https://github.com/etternagame/etterna, src/Etterna/MinaCalc/Dependent/HD_Sequencers/CJOHASequencing.h
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
use crate::sequencing_helpers::MS_INIT;

/// A gap bigger than the last one times this starts a new chain.
pub const CHAIN_SLOWDOWN_SCALE_THRESHOLD: f32 = 1.99;

#[derive(Clone, Copy, Debug)]
pub struct CjOhAnchorSequencer {
    pub chain_swaps: i32,
    pub not_swaps: i32,
    pub cur_len: i32,
    pub cur_anchor_len: i32,
    pub chain_swapping: bool,
    pub max_chain_swaps: i32,
    pub max_not_swaps: i32,
    pub max_total_len: i32,
    pub max_anchor_len: i32,
    pub anchor_col: ColType,
    pub last_ms: f32,
}

impl Default for CjOhAnchorSequencer {
    fn default() -> Self {
        CjOhAnchorSequencer {
            chain_swaps: 0,
            not_swaps: 0,
            cur_len: 0,
            cur_anchor_len: 0,
            chain_swapping: false,
            max_chain_swaps: 0,
            max_not_swaps: 0,
            max_total_len: 0,
            max_anchor_len: 0,
            anchor_col: ColType::Init,
            last_ms: MS_INIT,
        }
    }
}

impl CjOhAnchorSequencer {
    pub fn zero(&mut self) {
        self.reset_max_seq();
        self.reset_seq();
        self.last_ms = MS_INIT;
    }

    fn reset_seq(&mut self) {
        self.chain_swaps = 0;
        self.not_swaps = 0;
        self.cur_len = 0;
        self.cur_anchor_len = 0;
        self.chain_swapping = false;
        self.anchor_col = ColType::Init;
    }

    pub fn reset_max_seq(&mut self) {
        self.max_chain_swaps = 0;
        self.max_not_swaps = 0;
        self.max_total_len = 0;
        self.max_anchor_len = 0;
    }

    fn update_max_seq(&mut self) {
        self.max_chain_swaps = self.get_max_chain_swaps();
        self.max_not_swaps = self.get_max_not_swaps();
        self.max_total_len = self.get_max_total_len();
        self.max_anchor_len = self.get_max_anchor_len();
    }

    fn complete_seq(&mut self) {
        if self.chain_swapping {
            self.swap_failed();
        }
        if self.chain_swaps != 0 || self.not_swaps != 0 {
            self.update_max_seq();
        }
        self.reset_seq();
    }

    fn chain_swap(&mut self) {
        self.chain_swapping = false;
        self.max_anchor_len = self.get_max_anchor_len();
        self.cur_anchor_len = 0;
        self.chain_swaps += 1;
    }

    fn swap_failed(&mut self) {
        self.chain_swapping = false;
        self.not_swaps += 1;
    }

    /// `operator()`.
    pub fn advance(&mut self, ct: ColType, bt: BaseType, last_ct: ColType, any_ms: f32) {
        if self.last_ms * CHAIN_SLOWDOWN_SCALE_THRESHOLD < any_ms {
            self.complete_seq();
        }
        self.last_ms = any_ms;

        match bt {
            BaseType::LeftRight | BaseType::RightLeft => self.complete_seq(),
            BaseType::JumpJump => {
                self.cur_len += 1;
                self.cur_anchor_len += 1;
                if self.anchor_col != ColType::Init {
                    self.chain_swapping = true;
                }
            }
            BaseType::SingleSingle => {
                self.anchor_col = ct;
                self.cur_len += 1;
                self.cur_anchor_len += 1;
            }
            BaseType::SingleJump => {
                self.anchor_col = last_ct;
                self.cur_len += 1;
                self.chain_swapping = true;
            }
            BaseType::JumpSingle => {
                self.cur_len += 1;
                self.cur_anchor_len += 1;
                if (self.anchor_col == ColType::Left && ct == ColType::Right)
                    || (self.anchor_col == ColType::Right && ct == ColType::Left)
                {
                    if self.chain_swapping {
                        self.chain_swap();
                    }
                } else if self.chain_swapping {
                    self.swap_failed();
                }
                self.anchor_col = ct;
            }
            BaseType::Init => {}
        }
    }

    pub fn get_max_total_len(&self) -> i32 {
        if self.cur_len > self.max_total_len {
            self.cur_len
        } else {
            self.max_total_len
        }
    }

    pub fn get_max_anchor_len(&self) -> i32 {
        if self.cur_anchor_len > self.max_anchor_len {
            self.cur_anchor_len
        } else {
            self.max_anchor_len
        }
    }

    pub fn get_max_chain_swaps(&self) -> i32 {
        if self.chain_swaps > self.max_chain_swaps {
            self.chain_swaps
        } else {
            self.max_chain_swaps
        }
    }

    pub fn get_max_not_swaps(&self) -> i32 {
        if self.not_swaps > self.max_not_swaps {
            self.not_swaps
        } else {
            self.max_not_swaps
        }
    }
}
