// Translated to Rust from Etterna's difficulty calculator, MinaCalc:
// https://github.com/etternagame/etterna, src/Etterna/MinaCalc/Agnostic/IntervalInfo.h
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

//! Raw note counts across an interval, reset at its end.

/// `tap_size`: index into `taps_by_size` (a row of n notes is n - 1).
pub mod tap_size {
    pub const SINGLE: usize = 0;
    pub const JUMP: usize = 1;
    pub const HAND: usize = 2;
    pub const QUAD: usize = 3;
    pub const NUM: usize = 16;
}

#[derive(Clone, Debug, Default)]
pub struct ItvInfo {
    /// Total taps.
    pub total_taps: i32,
    /// Non single taps.
    pub chord_taps: i32,
    /// Count of taps for each tap size.
    pub taps_by_size: [i32; tap_size::NUM],
    /// Inflated by dense hs/js mix.
    pub mixed_hs_density_tap_bonus: i32,
}

impl ItvInfo {
    pub fn handle_interval_end(&mut self) {
        self.total_taps = 0;
        self.chord_taps = 0;
        self.mixed_hs_density_tap_bonus = 0;
        self.taps_by_size = [0; tap_size::NUM];
    }

    pub fn update_tap_counts(&mut self, row_count: i32) {
        self.total_taps += row_count;
        if row_count > 1 {
            self.chord_taps += row_count;
        }
        self.taps_by_size[(row_count - 1) as usize] += row_count;
        if self.taps_by_size[tap_size::HAND] > 0 {
            self.mixed_hs_density_tap_bonus += self.taps_by_size[tap_size::JUMP];
        }
    }
}
