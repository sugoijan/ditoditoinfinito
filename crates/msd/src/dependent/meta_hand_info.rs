// Translated to Rust from Etterna's difficulty calculator, MinaCalc:
// https://github.com/etternagame/etterna, src/Etterna/MinaCalc/Dependent/MetaHandInfo.h
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

//! Row by row hand sequencing: the base and meta patterns a hand forms.

use super::hd_basic_sequencing::{BaseType, ColType, determine_base_pattern_type};
use super::hd_meta_sequencing::{MetaType, determine_meta_type};

#[derive(Clone, Copy, Debug)]
pub struct MetaHandInfo {
    pub ct: ColType,
    pub last_ct: ColType,
    pub bt: BaseType,
    pub last_bt: BaseType,
    pub last_last_bt: BaseType,
    pub mt: MetaType,
    pub last_mt: MetaType,
}

impl Default for MetaHandInfo {
    fn default() -> Self {
        MetaHandInfo {
            ct: ColType::Init,
            last_ct: ColType::Init,
            bt: BaseType::Init,
            last_bt: BaseType::Init,
            last_last_bt: BaseType::Init,
            mt: MetaType::Init,
            last_mt: MetaType::Init,
        }
    }
}

impl MetaHandInfo {
    pub fn full_reset(&mut self) {
        *self = MetaHandInfo::default();
    }

    /// `operator()`.
    pub fn advance(&mut self, last: &MetaHandInfo, ct: ColType) {
        self.ct = ct;
        self.last_ct = last.ct;
        self.last_last_bt = last.last_bt;
        self.last_bt = last.bt;
        self.last_mt = last.mt;
        self.bt = determine_base_pattern_type(ct, self.last_ct);
        self.mt = determine_meta_type(
            self.bt,
            self.last_bt,
            self.last_last_bt,
            last.last_last_bt,
            self.last_mt,
        );
    }
}
