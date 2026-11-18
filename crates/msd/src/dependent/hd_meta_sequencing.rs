// Translated to Rust from Etterna's difficulty calculator, MinaCalc:
// https://github.com/etternagame/etterna, src/Etterna/MinaCalc/Dependent/HD_MetaSequencing.h
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

//! Meta patterns: chains of base patterns.

use super::hd_basic_sequencing::{BaseType, is_cc_tap};

/// `meta_type`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
pub enum MetaType {
    Cccccc = 0,
    Ccacc = 1,
    Acca = 2,
    Ccsjjscc = 3,
    CcsjjsccInverted = 4,
    Enigma = 5,
    MetaEnigma = 6,
    UnknowableEnigma = 7,
    Init = 9,
}

pub const NUM_META_TYPES: usize = 8;

impl MetaType {
    #[inline]
    pub fn idx(self) -> usize {
        self as usize
    }
}

#[inline]
fn detecc_cccccc(now: BaseType, last_last: BaseType) -> bool {
    now == last_last
}

#[inline]
fn detecc_acca(a: BaseType, b: BaseType, c: BaseType) -> bool {
    a == BaseType::SingleSingle && is_cc_tap(b) && c == BaseType::SingleSingle
}

#[inline]
fn detecc_sjjscc(last: BaseType, last_last: BaseType, last_last_last: BaseType) -> bool {
    if !is_cc_tap(last_last_last) {
        return false;
    }
    last == BaseType::JumpSingle && last_last == BaseType::SingleJump
}

pub fn determine_meta_type(
    now: BaseType,
    last: BaseType,
    last_last: BaseType,
    last_last_last: BaseType,
    last_mt: MetaType,
) -> MetaType {
    if is_cc_tap(now) && is_cc_tap(last_last) {
        if detecc_cccccc(now, last_last) {
            return MetaType::Cccccc;
        }
        return MetaType::Ccacc;
    }
    if detecc_acca(now, last, last_last) {
        return MetaType::Acca;
    }
    if is_cc_tap(now) && detecc_sjjscc(last, last_last, last_last_last) {
        if now == last_last_last {
            return MetaType::Ccsjjscc;
        }
        return MetaType::CcsjjsccInverted;
    }
    if last_mt == MetaType::Enigma {
        return MetaType::MetaEnigma;
    }
    if last_mt == MetaType::MetaEnigma {
        return MetaType::UnknowableEnigma;
    }
    MetaType::Enigma
}
