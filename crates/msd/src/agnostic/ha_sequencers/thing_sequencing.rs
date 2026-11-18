// Translated to Rust from Etterna's difficulty calculator, MinaCalc:
// https://github.com/etternagame/etterna, src/Etterna/MinaCalc/Agnostic/HA_Sequencers/ThingSequencing.h
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

//! Patterns that, chained, are extremely jumptrillable: [xx]a[yy]b[zz]
//! (`the_slip`, thing 1) and [12]3[24]1[34]2[13]4[12] (`the_slip2`,
//! thing 2).

// to_slide_or_not_to_slide
const NEEDS_SINGLE: i32 = 1;
const NEEDS_23_JUMP: i32 = 2;
const NEEDS_OPPOSING_SINGLE: i32 = 3;
const NEEDS_OPPOSING_OHJUMP: i32 = 4;
// the_slip2's names for 2 and 3
const NEEDS_DOOR: i32 = 2;
const NEEDS_BLAAP: i32 = 3;

#[derive(Clone, Debug, Default)]
pub struct TheSlip {
    pub slip: u32,
    pub slippin_till_ya_slips_come_true: bool,
    pub slide: i32,
}

impl TheSlip {
    pub fn the_slip_is_the_boot(&self, notes: u32) -> bool {
        let left = self.slip == 3 || self.slip == 7;
        match self.slide {
            NEEDS_SINGLE => {
                if left {
                    notes == 8
                } else {
                    notes == 1
                }
            }
            NEEDS_23_JUMP => notes == 6,
            NEEDS_OPPOSING_SINGLE => {
                if left {
                    notes == 1
                } else {
                    notes == 8
                }
            }
            NEEDS_OPPOSING_OHJUMP => {
                if left {
                    notes == 12 || notes == 14
                } else {
                    notes == 3 || notes == 7
                }
            }
            _ => false,
        }
    }

    pub fn start(&mut self, notes: u32) {
        self.slip = notes;
        self.slide = 0;
        self.slippin_till_ya_slips_come_true = true;
        self.grow();
    }

    pub fn grow(&mut self) {
        self.slide += 1;
    }

    pub fn reset(&mut self) {
        self.slippin_till_ya_slips_come_true = false;
    }
}

#[derive(Clone, Debug, Default)]
pub struct TheSlip2 {
    pub slip: u32,
    pub slippin_till_ya_slips_come_true: bool,
    pub slide: i32,
}

impl TheSlip2 {
    pub fn the_slip_is_the_boot(&self, notes: u32) -> bool {
        let left = self.slip == 3;
        match self.slide {
            NEEDS_SINGLE => {
                if left {
                    notes == 4
                } else {
                    notes == 2
                }
            }
            NEEDS_DOOR => {
                if left {
                    notes == 10
                } else {
                    notes == 5
                }
            }
            NEEDS_BLAAP => {
                if left {
                    notes == 1
                } else {
                    notes == 8
                }
            }
            NEEDS_OPPOSING_OHJUMP => {
                if left {
                    notes == 12
                } else {
                    notes == 3
                }
            }
            _ => false,
        }
    }

    pub fn start(&mut self, notes: u32) {
        self.slip = notes;
        self.slide = 0;
        self.slippin_till_ya_slips_come_true = true;
        self.grow();
    }

    pub fn grow(&mut self) {
        self.slide += 1;
    }

    pub fn reset(&mut self) {
        self.slippin_till_ya_slips_come_true = false;
    }
}

const MAX_SLIPS: i32 = 4;

/// `TT_Sequencing`, used by thing 1.
#[derive(Clone, Debug)]
pub struct TtSequencing {
    pub fizz: TheSlip,
    pub slip_counter: i32,
    pub mod_parts: [f32; 4],
    pub scaler: f32,
}

impl Default for TtSequencing {
    fn default() -> Self {
        TtSequencing {
            fizz: TheSlip::default(),
            slip_counter: 0,
            mod_parts: [1.0; 4],
            scaler: 0.0,
        }
    }
}

impl TtSequencing {
    pub fn set_params(&mut self, _gt: f32, _st: f32, ms: f32) {
        self.scaler = ms;
    }

    fn complete_slip(&mut self, notes: u32) {
        if self.slip_counter < MAX_SLIPS {
            self.mod_parts[self.slip_counter as usize] = self.scaler;
        }
        self.slip_counter += 1;
        self.fizz.start(notes);
    }

    fn start_test(notes: u32) -> bool {
        notes == 3 || notes == 7 || notes == 12 || notes == 14
    }

    /// `operator()`.
    pub fn advance(&mut self, _ms_now: f32, notes: u32) {
        if notes == 15 {
            if self.fizz.slippin_till_ya_slips_come_true {
                self.fizz.reset();
            }
            return;
        }
        if !self.fizz.slippin_till_ya_slips_come_true {
            if Self::start_test(notes) {
                self.fizz.start(notes);
            }
            return;
        }
        if self.fizz.the_slip_is_the_boot(notes) {
            self.fizz.grow();
            if self.fizz.slide == 5 {
                self.complete_slip(notes);
            }
        } else {
            self.fizz.reset();
        }
    }

    pub fn reset(&mut self) {
        self.slip_counter = 0;
        self.mod_parts = [1.0; 4];
    }
}

/// `TT_Sequencing2`, used by thing 2.
#[derive(Clone, Debug)]
pub struct TtSequencing2 {
    pub fizz: TheSlip2,
    pub slip_counter: i32,
    pub mod_parts: [f32; 4],
    pub scaler: f32,
}

impl Default for TtSequencing2 {
    fn default() -> Self {
        TtSequencing2 {
            fizz: TheSlip2::default(),
            slip_counter: 0,
            mod_parts: [1.0; 4],
            scaler: 0.0,
        }
    }
}

impl TtSequencing2 {
    pub fn set_params(&mut self, _gt: f32, _st: f32, ms: f32) {
        self.scaler = ms;
    }

    fn complete_slip(&mut self, notes: u32) {
        if self.slip_counter < MAX_SLIPS {
            self.mod_parts[self.slip_counter as usize] = self.scaler;
        }
        self.slip_counter += 1;
        self.fizz.start(notes);
    }

    fn start_test(notes: u32) -> bool {
        notes == 3 || notes == 12
    }

    /// `operator()`.
    pub fn advance(&mut self, _ms_now: f32, notes: u32) {
        if notes == 15 {
            if self.fizz.slippin_till_ya_slips_come_true {
                self.fizz.reset();
            }
            return;
        }
        if !self.fizz.slippin_till_ya_slips_come_true {
            if Self::start_test(notes) {
                self.fizz.start(notes);
            }
            return;
        }
        if self.fizz.the_slip_is_the_boot(notes) {
            self.fizz.grow();
            if self.fizz.slide == 5 {
                self.complete_slip(notes);
            }
        } else {
            self.fizz.reset();
        }
    }

    pub fn reset(&mut self) {
        self.slip_counter = 0;
        self.mod_parts = [1.0; 4];
    }
}
