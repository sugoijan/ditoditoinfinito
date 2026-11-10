//! Match points as exact fractions. Tied places share the average of the
//! awards they occupy (12 and 9 tied for first give 10.5 each; three tied
//! for first under the eight-player table give 28/3), and equal totals must
//! compare equal, so points are never rounded floats.

use serde::{Deserialize, Serialize};
use std::cmp::Ordering;
use std::fmt;
use std::ops::Add;

/// A non-negative number of points, kept as a reduced fraction.
#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
#[serde(try_from = "RawPoints", into = "RawPoints")]
pub struct Points {
    num: u64,
    den: u64,
}

#[derive(Serialize, Deserialize)]
struct RawPoints {
    num: u64,
    den: u64,
}

impl TryFrom<RawPoints> for Points {
    type Error = &'static str;
    fn try_from(raw: RawPoints) -> Result<Self, Self::Error> {
        if raw.den == 0 {
            return Err("points need a non-zero denominator");
        }
        Ok(Self::ratio(raw.num, raw.den))
    }
}

impl From<Points> for RawPoints {
    fn from(p: Points) -> Self {
        Self {
            num: p.num,
            den: p.den,
        }
    }
}

fn gcd(mut a: u128, mut b: u128) -> u128 {
    while b != 0 {
        (a, b) = (b, a % b);
    }
    a
}

impl Points {
    pub const ZERO: Self = Self { num: 0, den: 1 };

    pub fn whole(n: u64) -> Self {
        Self { num: n, den: 1 }
    }

    /// `num / den`, reduced. `den` must not be zero.
    pub fn ratio(num: u64, den: u64) -> Self {
        assert!(den != 0, "points need a non-zero denominator");
        Self::reduced(num.into(), den.into())
    }

    fn reduced(num: u128, den: u128) -> Self {
        let g = gcd(num, den).max(1);
        let (num, den) = (num / g, den / g);
        // Awards are small and shared by at most the field, so a stage's
        // denominators divide the field's lcm and sums stay tiny.
        Self {
            num: u64::try_from(num).expect("match points stay small"),
            den: u64::try_from(den).expect("match points stay small"),
        }
    }

    pub fn numerator(self) -> u64 {
        self.num
    }

    pub fn denominator(self) -> u64 {
        self.den
    }

    /// For display only; comparisons use the exact fraction.
    pub fn as_f64(self) -> f64 {
        self.num as f64 / self.den as f64
    }
}

impl Add for Points {
    type Output = Self;
    fn add(self, other: Self) -> Self {
        let num = u128::from(self.num) * u128::from(other.den)
            + u128::from(other.num) * u128::from(self.den);
        Self::reduced(num, u128::from(self.den) * u128::from(other.den))
    }
}

impl std::iter::Sum for Points {
    fn sum<I: Iterator<Item = Self>>(iter: I) -> Self {
        iter.fold(Self::ZERO, Add::add)
    }
}

impl PartialEq for Points {
    fn eq(&self, other: &Self) -> bool {
        self.cmp(other) == Ordering::Equal
    }
}

impl Eq for Points {}

impl PartialOrd for Points {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for Points {
    fn cmp(&self, other: &Self) -> Ordering {
        (u128::from(self.num) * u128::from(other.den))
            .cmp(&(u128::from(other.num) * u128::from(self.den)))
    }
}

impl fmt::Display for Points {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.den == 1 {
            write!(f, "{}", self.num)
        } else {
            write!(f, "{}/{}", self.num, self.den)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fractions_reduce_and_compare_exactly() {
        let third = Points::ratio(28, 3);
        assert_eq!(third + third + third, Points::whole(28));
        assert_eq!(Points::ratio(21, 2), Points::ratio(42, 4));
        assert!(Points::ratio(21, 2) > Points::whole(10));
        assert_eq!(Points::ratio(21, 2).to_string(), "21/2");
        assert_eq!(
            [Points::whole(3), Points::ratio(1, 2)]
                .into_iter()
                .sum::<Points>()
                .as_f64(),
            3.5
        );
    }

    #[test]
    fn a_zero_denominator_is_refused_on_the_wire() {
        assert!(serde_json::from_str::<Points>(r#"{"num":1,"den":0}"#).is_err());
        let p: Points = serde_json::from_str(r#"{"num":4,"den":8}"#).unwrap();
        assert_eq!((p.numerator(), p.denominator()), (1, 2));
    }
}
