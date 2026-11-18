// Translated to Rust from Etterna's difficulty calculator, MinaCalc:
// https://github.com/etternagame/etterna, src/Etterna/MinaCalc/../Models/NoteData/NoteDataStructures.h
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

//! The calculator's input row and the enumerations it indexes by.

/// One row: which columns have a note (bit `i` for column `i`, the
/// leftmost column bit 0), and its time in seconds.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct NoteInfo {
    pub notes: u32,
    pub row_time: f32,
}

/// The eight numbers, in upstream's order.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[repr(usize)]
pub enum Skillset {
    Overall = 0,
    Stream = 1,
    Jumpstream = 2,
    Handstream = 3,
    Stamina = 4,
    JackSpeed = 5,
    Chordjack = 6,
    Technical = 7,
}

pub const NUM_SKILLSET: usize = 8;

impl Skillset {
    pub const ALL: [Skillset; NUM_SKILLSET] = [
        Skillset::Overall,
        Skillset::Stream,
        Skillset::Jumpstream,
        Skillset::Handstream,
        Skillset::Stamina,
        Skillset::JackSpeed,
        Skillset::Chordjack,
        Skillset::Technical,
    ];

    /// The seven skillsets, without Overall.
    pub const SKILLS: [Skillset; 7] = [
        Skillset::Stream,
        Skillset::Jumpstream,
        Skillset::Handstream,
        Skillset::Stamina,
        Skillset::JackSpeed,
        Skillset::Chordjack,
        Skillset::Technical,
    ];
}

pub const SKILL_OVERALL: usize = 0;
pub const SKILL_STREAM: usize = 1;
pub const SKILL_JUMPSTREAM: usize = 2;
pub const SKILL_HANDSTREAM: usize = 3;
pub const SKILL_STAMINA: usize = 4;
pub const SKILL_JACKSPEED: usize = 5;
pub const SKILL_CHORDJACK: usize = 6;
pub const SKILL_TECHNICAL: usize = 7;

/// `CalcPatternMod`: one value a mod an interval a hand.
pub mod pmod {
    pub const STREAM: usize = 0;
    pub const JS: usize = 1;
    pub const HS: usize = 2;
    pub const CJ: usize = 3;
    pub const CJ_DENSITY: usize = 4;
    pub const HS_DENSITY: usize = 5;
    pub const CJ_OH_ANCHOR: usize = 6;
    pub const OH_JUMP_MOD: usize = 7;
    pub const CJ_OH_JUMP: usize = 8;
    pub const BALANCE: usize = 9;
    pub const ROLL: usize = 10;
    pub const ROLL_JS: usize = 11;
    pub const OH_TRILL: usize = 12;
    pub const VOH_TRILL: usize = 13;
    pub const CHAOS: usize = 14;
    pub const FLAM_JAM: usize = 15;
    pub const WIDE_RANGE_ROLL: usize = 16;
    pub const WIDE_RANGE_JUMPTRILL: usize = 17;
    pub const WIDE_RANGE_JJ: usize = 18;
    pub const WIDE_RANGE_BALANCE: usize = 19;
    pub const WIDE_RANGE_ANCHOR: usize = 20;
    pub const WIDE_RANGE_CJ_ANCHOR: usize = 21;
    pub const THE_THING: usize = 22;
    pub const THE_THING2: usize = 23;
    pub const RAN_MAN: usize = 24;
    pub const MINIJACK: usize = 25;
    pub const TOTAL_PATTERN_MOD: usize = 26;
    pub const G_STREAM: usize = 27;
    pub const G_CHORD_STREAM: usize = 28;
    pub const G_BRACKETING: usize = 29;
    pub const HAND_SWITCH: usize = 30;
    pub const OLD_ANCHOR_SCALER: usize = 31;
    pub const OLD_JUMP_SCALER: usize = 32;
    pub const OLD_OHJ_SCALER: usize = 33;
    pub const OLD_ROLL_SCALER: usize = 34;
    pub const HAND_BALANCE: usize = 35;
    pub const NUM: usize = 36;
}

/// `CalcDiffValue`: the base difficulties. Only those that reach the
/// output are kept (NPSBase, TechBase); MSBase, JackBase, CJBase and
/// RMABase are upstream's debug output.
pub mod diff {
    pub const NPS_BASE: usize = 0;
    pub const TECH_BASE: usize = 1;
    pub const NUM: usize = 2;
}
