//! How far behind a view draws the other competitors' fields
//! (`docs/plans/multiplayer-online-play.md`, "The views' delay"): a jitter
//! buffer, one per device, sans-IO.
//!
//! A remote moment reaches a device after the sender's batching, the trip
//! up, the room's tick and the trip down; the device draws it a delay
//! after it happened (song time), so that a field's stream is judged past
//! the view's time when the view gets there and its notes never wait at
//! the receptors for a judgment still on its way (a stall). `live-v1`'s
//! views waited a constant second. Here each device measures how late its
//! frames really are and waits as little as that allows:
//!
//! - **What is measured**: each time a frame brings a competitor's news,
//!   how far that competitor's stream had fallen behind the device's song
//!   time just before (the device's song time less the time its stream
//!   was judged to): the peak of the lag a view must cover, sampled where
//!   it peaks. A field that stalls is sampled at once too.
//! - **The target**: a high quantile of the last few seconds' samples
//!   ([`ViewDelayRules::quantile_permille`] of [`ViewDelayRules::window_ms`])
//!   and a margin, never under the floor nor over the ceiling (the live
//!   rules' `presentation_delay_ms`, a second).
//! - **The delay drawn with** follows the target: up fast (the view runs
//!   slower for a moment, never backwards), down slowly (a little faster),
//!   so a spike raises it at once and it comes down over seconds.
//!
//! It is display alone: nothing the room decides, nothing judged and
//! nothing any other device sees depends on it. Its numbers are
//! [`VIEW_DELAY_V0`] (PROVISIONAL), named and versioned.

use std::collections::VecDeque;

/// A view's delay's numbers. PROVISIONAL.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ViewDelayRules {
    pub id: &'static str,
    /// The least delay a view draws with (ms), whatever it measured.
    pub floor_ms: u32,
    /// Added to the measured quantile (ms): the room for what the window
    /// did not see.
    pub margin_ms: u32,
    /// The samples the target is made from: those of the last this many
    /// milliseconds (whole seconds, the oldest second dropped at once).
    pub window_ms: u32,
    /// The quantile of the window's samples the target covers (per mille).
    pub quantile_permille: u32,
    /// How fast the delay rises toward a higher target: ms of delay a
    /// second (the view runs at `1 − this/1000` of real time meanwhile).
    pub grow_per_s_ms: u32,
    /// How fast it falls toward a lower one (the view runs at
    /// `1 + this/1000`).
    pub shrink_per_s_ms: u32,
    /// The samples' resolution (ms): the histogram's buckets.
    pub bucket_ms: u32,
}

/// `view-delay-v0` (PROVISIONAL, 2026-11-13): a floor of 150 ms, the 99th
/// percentile of the last 8 s and 40 ms, up at 500 ms a second, down at
/// 50 ms a second.
pub const VIEW_DELAY_V0: ViewDelayRules = ViewDelayRules {
    id: "view-delay-v0",
    floor_ms: 150,
    margin_ms: 40,
    window_ms: 8_000,
    quantile_permille: 990,
    grow_per_s_ms: 500,
    shrink_per_s_ms: 50,
    bucket_ms: 10,
};

impl ViewDelayRules {
    /// The delay a view starts with, before any frame came: a batch and a
    /// tick (`live`'s), what the clock's exchanges say a trip each way
    /// takes (`uncertainty_ms`, about half a round trip: three of them, a
    /// round trip and a half), and the margin twice; within the floor and
    /// `ceiling_ms`.
    pub fn initial_ms(&self, live: &super::LiveRules, uncertainty_ms: f64, ceiling_ms: f64) -> f64 {
        let guess = (live.batch_ms + live.tick_ms) as f64
            + 3.0 * uncertainty_ms.max(0.0)
            + 2.0 * f64::from(self.margin_ms);
        guess.clamp(
            f64::from(self.floor_ms),
            ceiling_ms.max(f64::from(self.floor_ms)),
        )
    }
}

/// One device's view delay.
#[derive(Clone, Debug)]
pub struct ViewDelay {
    rules: ViewDelayRules,
    ceiling_ms: f64,
    current_ms: f64,
    target_ms: f64,
    /// Each second's samples (its index: the song second), a histogram of
    /// `bucket_ms` buckets up to the ceiling, the last one everything
    /// beyond.
    slots: VecDeque<(i64, Vec<u32>)>,
    last: Option<f64>,
    samples: u64,
    stalls: u32,
}

impl ViewDelay {
    /// A delay under `rules` within `ceiling_ms`, starting at `initial_ms`.
    pub fn new(rules: ViewDelayRules, ceiling_ms: f64, initial_ms: f64) -> ViewDelay {
        let ceiling_ms = ceiling_ms.max(f64::from(rules.floor_ms));
        let initial = initial_ms.clamp(f64::from(rules.floor_ms), ceiling_ms);
        ViewDelay {
            rules,
            ceiling_ms,
            current_ms: initial,
            target_ms: initial,
            slots: VecDeque::new(),
            last: None,
            samples: 0,
            stalls: 0,
        }
    }

    /// A constant delay (`live-v1`'s way, and a local opponent's none):
    /// floor and ceiling both `ms`.
    pub fn fixed(ms: f64) -> ViewDelay {
        let rules = ViewDelayRules {
            id: "fixed",
            floor_ms: ms.max(0.0).round() as u32,
            ..VIEW_DELAY_V0
        };
        ViewDelay::new(rules, ms, ms)
    }

    pub fn rules(&self) -> ViewDelayRules {
        self.rules
    }

    fn buckets(&self) -> usize {
        (self.ceiling_ms / f64::from(self.rules.bucket_ms.max(1))).ceil() as usize + 1
    }

    /// A competitor's stream was `lag_ms` behind this device's song time
    /// `now_ms` (just before its news came, or now that its field stalls).
    pub fn sample(&mut self, now_ms: f64, lag_ms: f64) {
        if !lag_ms.is_finite() || !now_ms.is_finite() {
            return;
        }
        let n = self.buckets();
        let bucket =
            ((lag_ms.max(0.0) / f64::from(self.rules.bucket_ms.max(1))) as usize).min(n - 1);
        let second = (now_ms / 1_000.0).floor() as i64;
        match self.slots.back_mut() {
            Some((s, h)) if *s >= second => h[bucket] += 1,
            _ => {
                let mut h = vec![0; n];
                h[bucket] = 1;
                self.slots.push_back((second, h));
            }
        }
        self.samples += 1;
    }

    /// A field stalled at `now_ms`, its stream `lag_ms` behind: counted,
    /// and sampled (the target rises at once).
    pub fn stalled(&mut self, now_ms: f64, lag_ms: f64) {
        self.stalls += 1;
        self.sample(now_ms, lag_ms);
    }

    /// Time passes to song ms `now_ms`: the window moves on, the target is
    /// made again and the delay follows it. The delay to draw with now.
    pub fn advance(&mut self, now_ms: f64) -> f64 {
        if !now_ms.is_finite() {
            return self.current_ms;
        }
        // At most a quarter second at a time (a pause, a frame held).
        let dt = self.last.map_or(0.0, |l| (now_ms - l).clamp(0.0, 250.0));
        self.last = Some(self.last.map_or(now_ms, |l| l.max(now_ms)));
        let oldest = ((now_ms - f64::from(self.rules.window_ms)) / 1_000.0).floor() as i64;
        while self.slots.front().is_some_and(|(s, _)| *s < oldest) {
            self.slots.pop_front();
        }
        if let Some(q) = self.quantile_ms() {
            self.target_ms = (q + f64::from(self.rules.margin_ms))
                .clamp(f64::from(self.rules.floor_ms), self.ceiling_ms);
        }
        let step = |per_s: u32| f64::from(per_s) * dt / 1_000.0;
        if self.target_ms > self.current_ms {
            self.current_ms =
                (self.current_ms + step(self.rules.grow_per_s_ms)).min(self.target_ms);
        } else {
            self.current_ms =
                (self.current_ms - step(self.rules.shrink_per_s_ms)).max(self.target_ms);
        }
        self.current_ms
    }

    /// The window's quantile (the upper edge of its bucket), if it has
    /// samples.
    fn quantile_ms(&self) -> Option<f64> {
        let total: u64 = self
            .slots
            .iter()
            .map(|(_, h)| h.iter().map(|&c| u64::from(c)).sum::<u64>())
            .sum();
        if total == 0 {
            return None;
        }
        let rank = (total * u64::from(self.rules.quantile_permille))
            .div_ceil(1_000)
            .max(1);
        let mut seen = 0u64;
        for b in 0..self.buckets() {
            seen += self.slots.iter().map(|(_, h)| u64::from(h[b])).sum::<u64>();
            if seen >= rank {
                return Some((b + 1) as f64 * f64::from(self.rules.bucket_ms));
            }
        }
        Some(self.ceiling_ms)
    }

    /// The delay drawn with now (ms).
    pub fn delay_ms(&self) -> f64 {
        self.current_ms
    }

    /// The delay it is moving toward (ms).
    pub fn target_ms(&self) -> f64 {
        self.target_ms
    }

    pub fn ceiling_ms(&self) -> f64 {
        self.ceiling_ms
    }

    /// Samples taken so far.
    pub fn samples(&self) -> u64 {
        self.samples
    }

    /// Stalls counted so far.
    pub fn stalls(&self) -> u32 {
        self.stalls
    }
}

/// A view of a stage's frames as a device draws it, measured: its delay
/// fed from the frames as they come ([`ViewMeter::frame`]) and each
/// competitor's field stalled whenever its stream is judged short of the
/// view's time ([`ViewMeter::step`]). What the game's play does
/// (`ddi_session::battle::live`), kept here for the tools that measure it
/// without a play: the contract's tests, the server's load driver.
#[derive(Clone, Debug)]
pub struct ViewMeter {
    delay: ViewDelay,
    me: Option<crate::ParticipantId>,
    through: std::collections::BTreeMap<crate::ParticipantId, i64>,
    ended: std::collections::BTreeSet<crate::ParticipantId>,
    stalled: std::collections::BTreeSet<crate::ParticipantId>,
    last: Option<f64>,
    /// Field-time watched and field-time stalled (ms), stalls begun, from
    /// the measuring's start.
    pub field_ms: f64,
    pub stalled_ms: f64,
    pub onsets: u32,
    /// The delay at each step from the measuring's start (ms).
    pub delays: Vec<f64>,
}

impl ViewMeter {
    /// A view by `delay`, of everyone but `me`.
    pub fn new(delay: ViewDelay, me: Option<crate::ParticipantId>) -> ViewMeter {
        ViewMeter {
            delay,
            me,
            through: Default::default(),
            ended: Default::default(),
            stalled: Default::default(),
            last: None,
            field_ms: 0.0,
            stalled_ms: 0.0,
            onsets: 0,
            delays: Vec::new(),
        }
    }

    pub fn delay(&self) -> &ViewDelay {
        &self.delay
    }

    /// A frame read at this device's song ms `now_ms`.
    pub fn frame(&mut self, f: &super::Frame, now_ms: f64) {
        for e in &f.entries {
            if Some(e.participant) == self.me {
                continue;
            }
            if let Some(prev) = self.through.get(&e.participant)
                && !self.ended.contains(&e.participant)
            {
                self.delay.sample(now_ms, now_ms - *prev as f64 / 1_000.0);
            }
            let t = self.through.entry(e.participant).or_insert(e.through_us);
            *t = (*t).max(e.through_us);
            if e.end.is_some() {
                self.ended.insert(e.participant);
            }
        }
    }

    /// The view at song ms `now_ms`: the delay drawn with; measured from
    /// song ms `from` on.
    pub fn step(&mut self, now_ms: f64, from: f64) -> f64 {
        let dt = self.last.map_or(0.0, |l| (now_ms - l).max(0.0));
        self.last = Some(now_ms);
        let d = self.delay.advance(now_ms);
        let view = now_ms - d;
        let measuring = now_ms >= from;
        if measuring {
            self.delays.push(d);
        }
        for (q, through) in &self.through {
            if self.ended.contains(q) || view <= 0.0 {
                self.stalled.remove(q);
                continue;
            }
            if measuring {
                self.field_ms += dt;
            }
            let at = *through as f64 / 1_000.0;
            if at < view {
                if measuring {
                    self.stalled_ms += dt;
                }
                if self.stalled.insert(*q) {
                    if measuring {
                        self.onsets += 1;
                    }
                    self.delay.stalled(now_ms, now_ms - at);
                }
            } else {
                self.stalled.remove(q);
            }
        }
        d
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::live::{LIVE_V1, LIVE_V2};

    #[test]
    fn it_starts_from_a_guess_within_its_bounds() {
        let r = VIEW_DELAY_V0;
        assert_eq!(r.initial_ms(&LIVE_V2, 40.0, 1_000.0), 200.0 + 120.0 + 80.0);
        assert_eq!(r.initial_ms(&LIVE_V1, 400.0, 1_000.0), 1_000.0);
        assert_eq!(r.initial_ms(&LIVE_V2, 0.0, 1_000.0), 280.0);
        let d = ViewDelay::new(r, 1_000.0, 5.0);
        assert_eq!(d.delay_ms(), 150.0, "the floor");
    }

    #[test]
    fn it_follows_the_lag_up_fast_and_down_slowly() {
        let mut d = ViewDelay::new(VIEW_DELAY_V0, 1_000.0, 600.0);
        // Steady lags of 200 to 300 ms, ten a second: the target is their
        // 99th percentile and the margin; the delay comes down to it at
        // 50 ms a second.
        let mut t = 0.0;
        for i in 0..1_000 {
            t = f64::from(i) * 16.0;
            if i % 6 == 0 {
                d.sample(t, 200.0 + f64::from(i % 100));
            }
            d.advance(t);
        }
        assert!((d.target_ms() - 340.0).abs() <= 10.0, "{}", d.target_ms());
        assert!(
            (d.delay_ms() - d.target_ms()).abs() < 1e-9,
            "{}",
            d.delay_ms()
        );
        // A spike: up at once (500 ms a second), never past the ceiling.
        d.stalled(t + 16.0, 900.0);
        d.advance(t + 16.0);
        let before = d.delay_ms();
        d.advance(t + 116.0);
        assert!(
            (d.delay_ms() - before - 50.0).abs() < 1e-6,
            "{}",
            d.delay_ms()
        );
        for k in 2..30 {
            d.advance(t + 16.0 + f64::from(k) * 50.0);
        }
        // Its bucket's upper edge (910) and the margin.
        assert_eq!(d.target_ms(), 950.0);
        assert!((d.delay_ms() - 950.0).abs() < 1e-9);
        assert_eq!(d.stalls(), 1);
        // The spike leaves the window after 8 s; the delay comes down at
        // 50 ms a second.
        let at = t + 16.0;
        let mut u = at;
        while u < at + 9_000.0 {
            u += 16.0;
            if (u as i64) % 96 < 16 {
                d.sample(u, 250.0);
            }
            d.advance(u);
        }
        assert!(d.target_ms() <= 300.0, "{}", d.target_ms());
        assert!(d.delay_ms() > 850.0, "slowly: {}", d.delay_ms());
    }

    #[test]
    fn a_fixed_delay_never_moves() {
        let mut d = ViewDelay::fixed(1_000.0);
        d.sample(0.0, 10.0);
        d.sample(10.0, 5_000.0);
        for i in 0..100 {
            assert_eq!(d.advance(f64::from(i) * 100.0), 1_000.0);
        }
        let mut z = ViewDelay::fixed(0.0);
        z.sample(0.0, 300.0);
        assert_eq!(z.advance(100.0), 0.0);
    }
}
