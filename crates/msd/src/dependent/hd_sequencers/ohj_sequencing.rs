// Translated to Rust from Etterna's difficulty calculator, MinaCalc:
// https://github.com/etternagame/etterna, src/Etterna/MinaCalc/Dependent/HD_Sequencers/OHJSequencing.h
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

//! One hand jump sequences (taps in jumps).

use crate::dependent::hd_basic_sequencing::{BaseType, ColType};

#[derive(Clone, Copy, Debug, Default)]
pub struct OhjSequencer {
    pub cur_seq_taps: i32,
    pub max_seq_taps: i32,
}

impl OhjSequencer {
    pub fn get_largest_seq_taps(&self) -> i32 {
        if self.cur_seq_taps > self.max_seq_taps {
            self.cur_seq_taps
        } else {
            self.max_seq_taps
        }
    }

    fn complete_seq(&mut self) {
        self.max_seq_taps = self.get_largest_seq_taps();
        self.cur_seq_taps = 0;
    }

    pub fn zero(&mut self) {
        self.cur_seq_taps = 0;
        self.max_seq_taps = 0;
    }

    /// `operator()`.
    pub fn advance(&mut self, ct: ColType, bt: BaseType) {
        if self.cur_seq_taps == 0 {
            if ct != ColType::OhJump {
                return;
            }
            self.cur_seq_taps += 2;
        }
        match bt {
            BaseType::JumpJump => self.cur_seq_taps += 2,
            BaseType::JumpSingle => {}
            BaseType::LeftRight | BaseType::RightLeft => {
                if self.cur_seq_taps == 2 {
                    self.cur_seq_taps -= 1;
                } else {
                    self.cur_seq_taps -= 3;
                }
                self.complete_seq();
            }
            BaseType::SingleSingle => self.complete_seq(),
            BaseType::SingleJump => self.complete_seq(),
            BaseType::Init => {}
        }
    }
}
