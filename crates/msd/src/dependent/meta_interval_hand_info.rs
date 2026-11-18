// Translated to Rust from Etterna's difficulty calculator, MinaCalc:
// https://github.com/etternagame/etterna, src/Etterna/MinaCalc/Dependent/MetaIntervalHandInfo.h
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

//! A hand's interval info with counts of base and meta patterns.

use super::hd_basic_sequencing::NUM_BASE_TYPES;
use super::hd_meta_sequencing::NUM_META_TYPES;
use super::interval_hand_info::ItvHandInfo;

#[derive(Clone, Debug, Default)]
pub struct MetaItvHandInfo {
    pub itvhi: ItvHandInfo,
    pub base_types: [i32; NUM_BASE_TYPES],
    pub meta_types: [i32; NUM_META_TYPES],
}

impl MetaItvHandInfo {
    /// Upstream clears the pattern counts here, before the pattern mods of
    /// the interval read them.
    pub fn interval_end(&mut self) {
        self.base_types = [0; NUM_BASE_TYPES];
        self.meta_types = [0; NUM_META_TYPES];
        self.itvhi.interval_end();
    }

    pub fn zero(&mut self) {
        self.base_types = [0; NUM_BASE_TYPES];
        self.meta_types = [0; NUM_META_TYPES];
        self.itvhi.zero();
    }
}
