// Translated to Rust from Etterna's difficulty calculator, MinaCalc:
// https://github.com/etternagame/etterna, src/Etterna/MinaCalc/SequencingHelpers.h
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

//! Generic sequencing helpers: timing constants and conversions, and the
//! tolerant comparisons of row time deltas.

/// Default for any field tracking seconds.
pub const S_INIT: f32 = -5.0;
/// Default for any field tracking milliseconds.
pub const MS_INIT: f32 = 5000.0;

/// Global multiplier to standardize baselines (`3.632F * 1.06F`).
pub const FINALSCALER: f32 = 3.632 * 1.06;

/// Number of 1 bits in a row's notes.
#[inline]
pub fn column_count(notes: u32) -> i32 {
    notes.count_ones() as i32
}

/// Milliseconds between two timestamps in seconds.
#[inline]
pub fn ms_from(now: f32, last: f32) -> f32 {
    (now - last) * 1000.0
}

#[inline]
pub fn ms_to_bpm(x: f32) -> f32 {
    15000.0 / x
}

#[inline]
pub fn ms_to_nps(x: f32) -> f32 {
    1000.0 / x
}

#[inline]
pub fn ms_to_scaled_nps(ms: f32) -> f32 {
    ms_to_nps(ms) * FINALSCALER
}

/// `max_val`: `*std::max_element`.
pub fn max_val(v: &[f32]) -> f32 {
    v[max_index(v)]
}

/// `max_index`: the first largest (`std::max_element`).
pub fn max_index(v: &[f32]) -> usize {
    let mut largest = 0;
    for i in 1..v.len() {
        if v[largest] < v[i] {
            largest = i;
        }
    }
    largest
}

/// Tolerance for comparisons of row time deltas.
pub const ANY_MS_EPSILON: f32 = 0.1;

/// a > b
#[inline]
pub fn any_ms_is_greater(a: f32, b: f32) -> bool {
    (a - b) > ANY_MS_EPSILON
}

/// a < b
#[inline]
pub fn any_ms_is_lesser(a: f32, b: f32) -> bool {
    (b - a) > ANY_MS_EPSILON
}

/// a == b
#[inline]
pub fn any_ms_is_close(a: f32, b: f32) -> bool {
    (a - b).abs() <= ANY_MS_EPSILON
}

/// a == 0
#[inline]
pub fn any_ms_is_zero(a: f32) -> bool {
    any_ms_is_close(a, 0.0)
}
