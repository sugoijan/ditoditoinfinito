// Translated to Rust from Etterna's difficulty calculator, MinaCalc:
// https://github.com/etternagame/etterna, src/Etterna/MinaCalc/Agnostic/HA_Sequencing.h
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

//! Bitwise helpers on a row's notes.

/// A single tap (or no note).
#[inline]
pub fn is_single_tap(a: u32) -> bool {
    (a & a.wrapping_sub(1)) == 0
}

#[inline]
pub fn is_jack_at_col(column_id: u32, row_notes: u32, last_row_notes: u32) -> bool {
    (column_id & row_notes) != 0 && (column_id & last_row_notes) != 0
}

/// A tap then a chord, or a chord then a tap (counts, not notes).
#[inline]
pub fn is_alternating_chord_single(a: u32, b: u32) -> bool {
    (a > 1 && b == 1) || (a == 1 && b > 1)
}

/// 1[n]1 or [n]1[n] with no jacks between the first and second and the
/// second and third.
pub fn is_alternating_chord_stream(a: u32, b: u32, c: u32) -> bool {
    if is_single_tap(a) {
        if is_single_tap(b) {
            return false;
        }
        if !is_single_tap(c) {
            return false;
        }
    } else {
        if !is_single_tap(b) {
            return false;
        }
        if is_single_tap(c) {
            return false;
        }
    }
    !((a & b) != 0 && (b & c) != 0)
}
