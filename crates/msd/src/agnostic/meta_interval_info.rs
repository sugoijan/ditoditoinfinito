// Translated to Rust from Etterna's difficulty calculator, MinaCalc:
// https://github.com/etternagame/etterna, src/Etterna/MinaCalc/Agnostic/MetaIntervalInfo.h
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

//! Meta info for the interval: patterning across consecutive rows.

use super::interval_info::ItvInfo;

#[derive(Clone, Debug, Default)]
pub struct MetaItvInfo {
    pub itvi: ItvInfo,

    pub idx: i32,
    pub seriously_not_js: i32,
    pub definitely_not_jacks: i32,
    pub actual_jacks: i32,
    pub actual_jacks_cj: i32,
    pub not_js: i32,
    pub not_hs: i32,
    pub zwop: i32,
    pub shared_chord_jacks: i32,
    pub dunk_it: bool,

    /// Up to three unique row note configurations of the interval.
    pub row_variations: [u32; 3],
    pub num_var: i32,
    /// Fewer than three unique rows in the interval.
    pub basically_vibro: bool,
}

impl MetaItvInfo {
    pub fn reset(&mut self) {
        self.itvi.handle_interval_end();
        self.idx = 0;
        self.seriously_not_js = 0;
        self.definitely_not_jacks = 0;
        self.actual_jacks = 0;
        self.actual_jacks_cj = 0;
        self.not_js = 0;
        self.not_hs = 0;
        self.zwop = 0;
        self.shared_chord_jacks = 0;
        self.dunk_it = false;
        self.row_variations = [0; 3];
        self.num_var = 0;
        // Upstream's reset sets this false (`= 0`); the interval's end sets
        // it true.
        self.basically_vibro = false;
    }

    pub fn handle_interval_end(&mut self) {
        // seriously_not_js is not reset: it tracks longer runs of singles.
        self.definitely_not_jacks = 0;
        self.actual_jacks = 0;
        self.actual_jacks_cj = 0;
        self.not_js = 0;
        self.not_hs = 0;
        self.zwop = 0;
        self.shared_chord_jacks = 0;
        self.row_variations = [0; 3];
        self.num_var = 0;
        self.basically_vibro = true;
        self.dunk_it = false;
        self.itvi.handle_interval_end();
    }
}
