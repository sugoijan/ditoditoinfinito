//! The room's clock as a device estimates it: a cheap exchange on the
//! room's socket (a [`Ping`] with the device's own time, answered at once
//! with a [`Pong`] carrying the room's) and a sans-IO estimator of the
//! offset between the two clocks, with how far it may be off.
//!
//! Each exchange gives an offset (the room's time less the midpoint of the
//! device's send and receive) good to within half its round trip, whatever
//! the asymmetry of the two ways. The estimate is the median offset of the
//! three exchanges with the lowest round trips among the recent ones: the
//! lowest round trips bound the error best, and the median leaves one bad
//! sample out (one whose answer was held on one way only looks like a
//! long round trip, but a clock that jumped once does not). Its
//! uncertainty is the largest of those three half round trips plus their
//! disagreement. Samples older than a minute (relative to the newest) are
//! dropped, so a drifting clock is followed.

use serde::{Deserialize, Serialize};
use std::collections::VecDeque;

/// A clock exchange's question: `t` the device's own clock, ms.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Ping {
    pub n: u32,
    pub t: u64,
}

/// Its answer, at once: the question's `n` and `t` back, and `s` the
/// room's clock (ms) when it read the ping.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Pong {
    pub n: u32,
    pub t: u64,
    pub s: u64,
}

impl Pong {
    /// The room's answer to `ping` read at room time `now_ms`.
    pub fn answer(ping: Ping, now_ms: u64) -> Pong {
        Pong {
            n: ping.n,
            t: ping.t,
            s: now_ms,
        }
    }
}

/// The room's clock from a device's: `room = local + offset_us`, off by at
/// most about `uncertainty_us`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ClockEstimate {
    pub offset_us: i64,
    pub uncertainty_us: i64,
    /// Exchanges it rests on.
    pub samples: usize,
}

impl ClockEstimate {
    pub fn room_us(&self, local_us: i64) -> i64 {
        local_us + self.offset_us
    }

    pub fn local_us(&self, room_us: i64) -> i64 {
        room_us - self.offset_us
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Sample {
    /// When it was received, the device's clock (µs).
    at_us: i64,
    offset_us: i64,
    rtt_us: i64,
}

/// The estimator: exchanges in, an estimate out.
#[derive(Clone, Debug)]
pub struct ClockEstimator {
    samples: VecDeque<Sample>,
    max_samples: usize,
    max_age_us: i64,
}

impl Default for ClockEstimator {
    fn default() -> Self {
        Self::new()
    }
}

impl ClockEstimator {
    /// How many of the lowest round trips the estimate takes the median of.
    pub const BEST: usize = 3;

    pub fn new() -> Self {
        Self {
            samples: VecDeque::new(),
            max_samples: 32,
            max_age_us: 60_000_000,
        }
    }

    /// One exchange: sent and received on the device's clock, the room's
    /// time between them, all µs. A negative round trip (a clock that ran
    /// backwards) is refused.
    pub fn sample_us(&mut self, sent_us: i64, received_us: i64, room_us: i64) -> bool {
        let rtt_us = received_us - sent_us;
        if rtt_us < 0 {
            return false;
        }
        let offset_us = room_us - (sent_us + rtt_us / 2);
        self.samples.push_back(Sample {
            at_us: received_us,
            offset_us,
            rtt_us,
        });
        while self.samples.len() > self.max_samples {
            self.samples.pop_front();
        }
        let newest = received_us;
        self.samples.retain(|s| newest - s.at_us <= self.max_age_us);
        true
    }

    /// A pong received at the device's `received_ms`.
    pub fn pong(&mut self, pong: &Pong, received_ms: u64) -> bool {
        self.sample_us(
            pong.t as i64 * 1_000,
            received_ms as i64 * 1_000,
            pong.s as i64 * 1_000,
        )
    }

    pub fn len(&self) -> usize {
        self.samples.len()
    }

    pub fn is_empty(&self) -> bool {
        self.samples.is_empty()
    }

    /// The estimate from the exchanges kept, if any.
    pub fn estimate(&self) -> Option<ClockEstimate> {
        let mut best: Vec<Sample> = self.samples.iter().copied().collect();
        if best.is_empty() {
            return None;
        }
        best.sort_by_key(|s| (s.rtt_us, std::cmp::Reverse(s.at_us)));
        best.truncate(Self::BEST);
        let mut offsets: Vec<i64> = best.iter().map(|s| s.offset_us).collect();
        offsets.sort_unstable();
        let offset_us = if offsets.len() == 2 {
            // Two: the lower round trip's (a mean would let a bad one in).
            best[0].offset_us
        } else {
            offsets[offsets.len() / 2]
        };
        let half = (best.iter().map(|s| s.rtt_us).max().unwrap_or(0) + 1) / 2;
        // Disagreement among those kept beside the one taken, the bad one
        // of three left out (a median's neighbors).
        let spread = if offsets.len() >= 3 {
            let mid = offsets.len() / 2;
            (offsets[mid] - offsets[mid - 1]).min(offsets[mid + 1] - offsets[mid])
        } else {
            offsets.last().unwrap_or(&0) - offsets.first().unwrap_or(&0)
        };
        Some(ClockEstimate {
            offset_us,
            uncertainty_us: half + spread,
            samples: self.samples.len(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A tiny seeded generator (SplitMix64) for the jitter.
    struct Rng(u64);
    impl Rng {
        fn next(&mut self) -> u64 {
            self.0 = self.0.wrapping_add(0x9E37_79B9_7F4A_7C15);
            let mut z = self.0;
            z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
            z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
            z ^ (z >> 31)
        }
        fn below(&mut self, n: u64) -> u64 {
            self.next() % n.max(1)
        }
    }

    /// Exchanges against a room whose clock is `truth` µs ahead of the
    /// device's: `up` and `down` the one-way delays' floors, jittered by
    /// up to `jitter` each way.
    fn run(truth: i64, up: i64, down: i64, jitter: u64, n: usize, seed: u64) -> ClockEstimator {
        let mut rng = Rng(seed);
        let mut est = ClockEstimator::new();
        let mut local = 1_000_000i64;
        for _ in 0..n {
            let sent = local;
            let up_d = up + rng.below(jitter) as i64;
            let down_d = down + rng.below(jitter) as i64;
            let room = sent + up_d + truth;
            let received = sent + up_d + down_d;
            assert!(est.sample_us(sent, received, room));
            local += 500_000;
        }
        est
    }

    #[test]
    fn one_exchange_is_good_to_half_its_round_trip() {
        let mut est = ClockEstimator::new();
        // 40 ms up, 10 ms down: the midpoint is 15 ms off, within 25 ms.
        est.sample_us(0, 50_000, 7_000_000 + 40_000);
        let e = est.estimate().unwrap();
        assert_eq!(e.offset_us, 7_000_000 + 15_000);
        assert!((e.offset_us - 7_000_000).abs() <= e.uncertainty_us);
        assert_eq!(e.uncertainty_us, 25_000);
    }

    #[test]
    fn jitter_and_asymmetry_stay_within_the_uncertainty() {
        for seed in 0..32 {
            // 30 ms up, 5 ms down, up to 80 ms of jitter each way.
            let truth = -123_456_789;
            let est = run(truth, 30_000, 5_000, 80_000, 16, seed);
            let e = est.estimate().unwrap();
            let err = (e.offset_us - truth).abs();
            assert!(err <= e.uncertainty_us, "seed {seed}: {err} > {e:?}");
            // The lowest round trips keep it tight: better than the
            // worst case of a single exchange.
            assert!(e.uncertainty_us < 90_000, "seed {seed}: {e:?}");
        }
    }

    #[test]
    fn one_bad_sample_is_left_out() {
        let truth = 5_000_000;
        let mut est = run(truth, 10_000, 10_000, 4_000, 8, 7);
        let good = est.estimate().unwrap();
        // A clock that jumped once: the lowest round trip of all, with an
        // offset a second off.
        est.sample_us(
            10_000_000,
            10_000_000 + 1_000,
            10_000_000 + truth + 1_000_000,
        );
        let e = est.estimate().unwrap();
        assert!((e.offset_us - truth).abs() <= 12_000, "{e:?}");
        assert!((e.offset_us - good.offset_us).abs() <= 12_000);
        // A held answer: a long round trip, never among the best.
        est.sample_us(11_000_000, 11_900_000, 11_000_000 + truth + 10_000);
        assert_eq!(est.estimate().unwrap().offset_us, e.offset_us);
    }

    #[test]
    fn a_backwards_clock_is_refused_and_old_samples_go() {
        let mut est = ClockEstimator::new();
        assert!(!est.sample_us(10, 5, 0));
        assert!(est.estimate().is_none());
        est.sample_us(0, 10_000, 1_000_000);
        // A minute and more later the old one is gone.
        est.sample_us(70_000_000, 70_020_000, 71_000_000 + 10_000);
        assert_eq!(est.len(), 1);
        assert_eq!(est.estimate().unwrap().offset_us, 1_000_000);
    }

    #[test]
    fn pongs_are_read_in_milliseconds() {
        let mut est = ClockEstimator::new();
        let ping = Ping { n: 1, t: 1_000 };
        let pong = Pong::answer(ping, 51_010);
        assert!(est.pong(&pong, 1_020));
        let e = est.estimate().unwrap();
        assert_eq!(e.offset_us, 50_000_000);
        assert_eq!(e.room_us(1_000_000), 51_000_000);
        assert_eq!(e.local_us(51_000_000), 1_000_000);
    }
}
