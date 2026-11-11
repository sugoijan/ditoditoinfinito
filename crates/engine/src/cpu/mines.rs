//! How a CPU opponent responds to battle shocks put into its play
//! (`docs/plans/multiplayer-prototype-progress.md`, M8b; the maintainer's
//! decision D8: by level only). The shocks are judged honestly by its own
//! [`crate::player::Player`] (`Player::add_hazards`); what changes is its
//! script of presses still to come: a shock it steps on is a real press in
//! the shock's window, and its own presses near a shock are disturbed
//! (later or earlier, sometimes left out).
//!
//! One value per CPU says how well it avoids shocks, its *avoidance*,
//! `0..=1` ([`MineResponseTable::avoidance`], from its level), so that it
//! can become an eighth `Skills` axis later (the maintainer's intent: mine
//! avoidance as a skill). The response is read from the avoidance
//! ([`MineResponseTable::response`]).
//!
//! PROVISIONAL and uncalibrated: [`MINE_RESPONSE_V0`]'s numbers are working
//! values, never a measurement.

/// What a CPU does about the shocks of one attack.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct MineResponse {
    /// The chance it steps on each shock (an extra press in its window).
    pub hit: f64,
    /// Its own presses within [`MineResponse::near`] of a shock move by up
    /// to this many seconds, either way.
    pub jitter: f64,
    /// The chance such a press is left out.
    pub slip: f64,
    /// Seconds either side of a shock its own presses are disturbed in.
    pub near: f64,
}

impl MineResponse {
    /// No response at all: every shock passed, nothing disturbed (the
    /// study's control; tests only, never a level's).
    pub const OFF: MineResponse = MineResponse {
        hit: 0.0,
        jitter: 0.0,
        slip: 0.0,
        near: 0.0,
    };

    /// It changes nothing.
    pub fn is_off(&self) -> bool {
        self.hit <= 0.0 && self.slip <= 0.0 && self.jitter <= 0.0
    }
}

/// The responses a CPU's avoidance spans, and the avoidance each level
/// gets.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct MineResponseTable {
    pub id: &'static str,
    /// At avoidance 0: steps on the most.
    pub poor: MineResponse,
    /// At avoidance 0.5.
    pub typical: MineResponse,
    /// At avoidance 1: steps on the fewest.
    pub good: MineResponse,
    /// Each level's avoidance (the engine's levels, [`super::STRENGTHS`]).
    pub by_level: [f64; 5],
}

/// `mine-response-v0` (PROVISIONAL, uncalibrated): levels 1–2 avoid
/// poorly (14% of shocks stepped on, own presses near them ±45 ms, 4%
/// left out), 3–4 typically (6%, ±25 ms, 1.5%), 5 well (2%, ±12 ms,
/// 0.5%); disturbed within 0.3 s of a shock.
pub const MINE_RESPONSE_V0: MineResponseTable = MineResponseTable {
    id: "mine-response-v0",
    poor: MineResponse {
        hit: 0.14,
        jitter: 0.045,
        slip: 0.04,
        near: 0.30,
    },
    typical: MineResponse {
        hit: 0.06,
        jitter: 0.025,
        slip: 0.015,
        near: 0.30,
    },
    good: MineResponse {
        hit: 0.02,
        jitter: 0.012,
        slip: 0.005,
        near: 0.30,
    },
    by_level: [0.0, 0.0, 0.5, 0.5, 1.0],
};

impl MineResponseTable {
    /// The avoidance of a CPU at `level` (an index into
    /// [`super::STRENGTHS`]; past the last, the last's).
    pub fn avoidance(&self, level: usize) -> f64 {
        self.by_level[level.min(self.by_level.len() - 1)]
    }

    /// The response at `avoidance` (`0..=1`): between the table's three,
    /// in a straight line.
    pub fn response(&self, avoidance: f64) -> MineResponse {
        let a = if avoidance.is_finite() {
            avoidance.clamp(0.0, 1.0)
        } else {
            0.5
        };
        let (from, to, f) = if a <= 0.5 {
            (self.poor, self.typical, a / 0.5)
        } else {
            (self.typical, self.good, (a - 0.5) / 0.5)
        };
        let mix = |x: f64, y: f64| if f >= 1.0 { y } else { x + (y - x) * f };
        MineResponse {
            hit: mix(from.hit, to.hit),
            jitter: mix(from.jitter, to.jitter),
            slip: mix(from.slip, to.slip),
            near: mix(from.near, to.near),
        }
    }
}

/// What a draw is for: each purpose its own stream.
#[derive(Clone, Copy)]
pub(super) enum Purpose {
    Hit = 1,
    HitPlace = 2,
    Slip = 3,
    Jitter = 4,
}

/// A uniform draw in `0..1` keyed by the CPU's seed, the shock (its tick
/// and lane), the purpose and `extra` (a press's own key): the same
/// whatever else happened, so a response never depends on the frames.
pub(super) fn draw(seed: u64, tick: i64, lane: u8, purpose: Purpose, extra: u64) -> f64 {
    let mix = |mut x: u64| {
        x = x.wrapping_add(0x9E37_79B9_7F4A_7C15);
        x = (x ^ (x >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        x = (x ^ (x >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        x ^ (x >> 31)
    };
    let z = mix(seed
        ^ mix(tick as u64)
        ^ mix(u64::from(lane) << 40 | purpose as u64)
        ^ mix(extra.rotate_left(17)));
    (z >> 11) as f64 * (1.0 / (1u64 << 53) as f64)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_table_is_named_and_levels_avoid_better_as_they_rise() {
        let t = MINE_RESPONSE_V0;
        assert_eq!(t.id, "mine-response-v0");
        assert_eq!(t.response(t.avoidance(0)), t.poor);
        assert_eq!(t.response(t.avoidance(1)), t.poor);
        assert_eq!(t.response(t.avoidance(2)), t.typical);
        assert_eq!(t.response(t.avoidance(3)), t.typical);
        assert_eq!(t.response(t.avoidance(4)), t.good);
        assert_eq!(t.avoidance(9), 1.0);
        let mid = t.response(0.75);
        assert!(mid.hit < t.typical.hit && mid.hit > t.good.hit);
        assert!(MineResponse::OFF.is_off() && !t.good.is_off());
    }

    #[test]
    fn draws_are_keyed_and_uniform_enough() {
        let a = draw(7, 480, 2, Purpose::Hit, 0);
        assert_eq!(a, draw(7, 480, 2, Purpose::Hit, 0));
        assert_ne!(a, draw(8, 480, 2, Purpose::Hit, 0));
        assert_ne!(a, draw(7, 480, 2, Purpose::Slip, 0));
        let n = 4000;
        let below = (0..n)
            .filter(|&i| draw(1, i, 0, Purpose::Hit, 0) < 0.25)
            .count();
        assert!((800..1200).contains(&below), "{below}");
    }
}
