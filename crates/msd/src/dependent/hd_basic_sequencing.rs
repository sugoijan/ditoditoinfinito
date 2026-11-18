// Translated to Rust from Etterna's difficulty calculator, MinaCalc:
// https://github.com/etternagame/etterna, src/Etterna/MinaCalc/Dependent/HD_BasicSequencing.h
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

//! Basic sequencing for hand-dependent patterns (four keys only): what a
//! hand's notes in a row are, and what two in a row form.

/// `col_type`: a hand's notes in a row.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
pub enum ColType {
    Left = 0,
    Right = 1,
    OhJump = 2,
    Empty = 4,
    Init = 5,
}

pub const NUM_COL_TYPES: usize = 3;
pub const NUM_COLS_PER_HAND: usize = 2;
pub const CT_LOOP: [ColType; NUM_COL_TYPES] = [ColType::Left, ColType::Right, ColType::OhJump];
pub const CT_LOOP_NO_JUMPS: [ColType; NUM_COLS_PER_HAND] = [ColType::Left, ColType::Right];

impl ColType {
    #[inline]
    pub fn idx(self) -> usize {
        self as usize
    }
}

/// A hand's notes in a row (`hand_id` 3 or 12).
pub fn determine_col_type(notes: u32, hand_id: u32) -> ColType {
    let shirt = notes & hand_id;
    if shirt == 0 {
        return ColType::Empty;
    }
    if hand_id == 3 {
        match shirt {
            3 => return ColType::OhJump,
            1 => return ColType::Left,
            2 => return ColType::Right,
            _ => {}
        }
    } else if hand_id == 12 {
        match shirt {
            12 => return ColType::OhJump,
            8 => return ColType::Right,
            4 => return ColType::Left,
            _ => {}
        }
    }
    ColType::Init
}

/// Left or right only.
#[inline]
pub fn invert_col(col: ColType) -> ColType {
    if col == ColType::Left {
        ColType::Right
    } else {
        ColType::Left
    }
}

/// `base_type`: the pattern two successive non-empty col types form.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
pub enum BaseType {
    LeftRight = 0,
    RightLeft = 1,
    JumpSingle = 2,
    SingleSingle = 3,
    SingleJump = 4,
    JumpJump = 5,
    Init = 7,
}

pub const NUM_BASE_TYPES: usize = 6;

impl BaseType {
    #[inline]
    pub fn idx(self) -> usize {
        self as usize
    }
}

pub fn determine_base_pattern_type(now: ColType, last: ColType) -> BaseType {
    if last == ColType::Init {
        return BaseType::Init;
    }
    let single_tap = now == ColType::Left || now == ColType::Right;
    if last == ColType::OhJump {
        if single_tap {
            return BaseType::JumpSingle;
        }
        return BaseType::JumpJump;
    } else if !single_tap {
        return BaseType::SingleJump;
    } else if now == ColType::Left && last == ColType::Right {
        return BaseType::RightLeft;
    } else if now == ColType::Right && last == ColType::Left {
        return BaseType::LeftRight;
    } else if now == last {
        return BaseType::SingleSingle;
    }
    BaseType::Init
}

/// Cross column: successive single notes on the two columns.
#[inline]
pub fn is_cc_tap(bt: BaseType) -> bool {
    bt == BaseType::LeftRight || bt == BaseType::RightLeft
}
