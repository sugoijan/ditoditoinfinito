// Translated to Rust from Etterna's difficulty calculator, MinaCalc:
// https://github.com/etternagame/etterna, src/Etterna/MinaCalc/PatternModHelpers.h
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

//! Generic pattern mod helpers, and C++'s `std::min`, `std::max` and
//! `std::clamp` as upstream gets them (their answers with NaN differ from
//! Rust's `f32::min` and friends).

pub const NEUTRAL: f32 = 1.0;

/// `std::max(a, b)`: `(a < b) ? b : a`.
#[inline]
pub fn max<T: PartialOrd>(a: T, b: T) -> T {
    if a < b { b } else { a }
}

/// `std::min(a, b)`: `(b < a) ? b : a`.
#[inline]
pub fn min<T: PartialOrd>(a: T, b: T) -> T {
    if b < a { b } else { a }
}

/// `std::clamp(v, lo, hi)`: `v < lo ? lo : hi < v ? hi : v`.
#[inline]
pub fn clamp<T: PartialOrd>(v: T, lo: T, hi: T) -> T {
    if v < lo {
        lo
    } else if hi < v {
        hi
    } else {
        v
    }
}

/// Upstream's `fastpow`: the double's high word scaled (significantly
/// inaccurate, on purpose).
#[inline]
pub fn fastpow(a: f64, b: f64) -> f32 {
    let hi = (a.to_bits() >> 32) as u32 as i32;
    let scaled = b * f64::from(hi.wrapping_sub(1_072_632_447)) + 1_072_632_447.0;
    // static_cast<int>: truncation toward zero.
    let hi = scaled as i32;
    f64::from_bits(u64::from(hi as u32) << 32) as f32
}

/// Upstream's `fastsqrt`: `x * rsqrt(x)` with the processor's approximate
/// reciprocal square root (`_mm_rsqrt_ss`). Its last bits depend on the
/// processor; this computes it as upstream's build for ARM does, through
/// sse2neon's `_mm_rsqrt_ps` (`FRSQRTE`, then one `FRSQRTS` Newton step),
/// in software, so every target gets the same bits.
#[inline]
pub fn fastsqrt(x: f32) -> f32 {
    if x == 0.0 {
        return 0.0;
    }
    let estimate = frsqrte(x);
    let refined = estimate * frsqrts(x * estimate, estimate);
    // sse2neon puts back an infinite estimate (from a zero input).
    let refined = if estimate.is_infinite() {
        estimate
    } else {
        refined
    };
    x * refined
}

/// `FRSQRTS`: `(3 - a * b) / 2` rounded once. The product of two `f32`s is
/// exact in `f64`, and so is the difference near 1 where it is used.
#[inline]
fn frsqrts(a: f32, b: f32) -> f32 {
    ((3.0 - f64::from(a) * f64::from(b)) / 2.0) as f32
}

/// `FRSQRTE` for single precision, as the Arm Architecture Reference
/// Manual's `FPRSqrtEstimate` defines it (8-bit estimate, FPCR.FZ off).
pub fn frsqrte(x: f32) -> f32 {
    let bits = x.to_bits();
    if x.is_nan() {
        return f32::NAN;
    }
    if x == 0.0 {
        return if bits >> 31 == 1 {
            f32::NEG_INFINITY
        } else {
            f32::INFINITY
        };
    }
    if bits >> 31 == 1 {
        return f32::NAN;
    }
    if x.is_infinite() {
        return 0.0;
    }
    let mut exp = ((bits >> 23) & 0xff) as i32;
    let mut fraction = u64::from(bits & 0x7f_ffff) << 29;
    if exp == 0 {
        while fraction & (1 << 51) == 0 {
            fraction <<= 1;
            exp -= 1;
        }
        fraction = (fraction << 1) & ((1 << 52) - 1);
    }
    let scaled = if exp & 1 == 0 {
        0x100 | ((fraction >> 44) & 0xff)
    } else {
        0x80 | ((fraction >> 45) & 0x7f)
    };
    let result_exp = (380 - exp).div_euclid(2) as u32;
    let estimate = u32::from(RSQRT_ESTIMATES[scaled as usize]);
    f32::from_bits(((result_exp & 0xff) << 23) | ((estimate & 0xff) << 15))
}

/// `RecipSqrtEstimate` for each scaled input 128..=511 (others unused).
static RSQRT_ESTIMATES: [u16; 512] = rsqrt_estimates();

const fn rsqrt_estimates() -> [u16; 512] {
    let mut table = [0u16; 512];
    let mut a = 128u64;
    while a < 512 {
        let mut x = a;
        if x < 256 {
            x = x * 2 + 1;
        } else {
            x = (x >> 1) << 1;
            x = (x + 1) * 2;
        }
        let mut b = 512u64;
        while x * (b + 1) * (b + 1) < (1 << 28) {
            b += 1;
        }
        table[a as usize] = b.div_ceil(2) as u16;
        a += 1;
    }
    table
}

/// `sum`: `std::accumulate` from zero, in order.
#[inline]
pub fn sum(v: &[f32]) -> f32 {
    let mut s = 0.0f32;
    for &x in v {
        s += x;
    }
    s
}

/// `mean`.
#[inline]
pub fn mean(v: &[f32]) -> f32 {
    sum(v) / v.len() as f32
}

/// Coefficient of variation (`cv`).
pub fn cv(input: &[f32]) -> f32 {
    let mut sd = 0.0f32;
    let average = mean(input);
    for &i in input {
        sd += (i - average) * (i - average);
    }
    fastsqrt(sd / input.len() as f32) / average
}

#[inline]
pub fn div_high_by_low(mut a: f32, mut b: f32) -> f32 {
    if b > a {
        std::mem::swap(&mut a, &mut b);
    }
    a / b
}

#[inline]
pub fn div_low_by_high(mut a: f32, mut b: f32) -> f32 {
    if b > a {
        std::mem::swap(&mut a, &mut b);
    }
    b / a
}

#[inline]
pub fn diff_high_by_low(mut a: i32, mut b: i32) -> i32 {
    if b > a {
        std::mem::swap(&mut a, &mut b);
    }
    a - b
}

#[inline]
pub fn weighted_average(a: f32, b: f32, x: f32, y: f32) -> f32 {
    (x * a + ((y - x) * b)) / y
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn std_min_max_clamp_keep_their_nan_answers() {
        assert!(max(f32::NAN, 1.0).is_nan());
        assert_eq!(max(1.0, f32::NAN), 1.0);
        assert!(min(f32::NAN, 1.0).is_nan());
        assert_eq!(min(1.0, f32::NAN), 1.0);
        assert!(clamp(f32::NAN, 0.0, 1.0).is_nan());
    }

    #[test]
    fn fastsqrt_is_close_to_sqrt() {
        for i in 1..2000 {
            let x = i as f32 * 0.37;
            let got = fastsqrt(x);
            let want = x.sqrt();
            assert!((got - want).abs() / want < 1e-4, "{x}: {got} {want}");
        }
        assert_eq!(fastsqrt(0.0), 0.0);
    }

    #[test]
    fn fastpow_is_a_rough_pow() {
        let got = fastpow(0.5, 2.0);
        assert!((got - 0.25).abs() < 0.02, "{got}");
        // 2^3 by the high word: 8.93.
        assert!((fastpow(2.0, 3.0) - 8.92775).abs() < 1e-4);
    }
}
