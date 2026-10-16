//! Frame pacing in play (`docs/plans/desktop.md`, "Frame pacing"): when
//! each frame of a play starts, and when it is shown.
//!
//! Two modes. [`Mode::Block`] (the default) is how play has run: frames
//! one after another, each waiting for the display inside the surface's
//! acquire (Fifo; Mailbox where the surface has it). [`Mode::Late`]
//! (`--pacing late`) starts each frame as late before the refresh it is
//! meant for as its cost allows, the shell sleeping in
//! `ControlFlow::WaitUntil` meanwhile, so what a frame shows (the
//! receptors, the judgements, the notes' place) was read closer to when
//! it is seen; it presents Fifo.
//!
//! No present timestamps reach us (wgpu has none), so the refreshes are
//! found the one way the surface tells: under Fifo with one frame in
//! flight, an acquire that waited returned when the frame before was
//! shown (a drawable is released then). The grid of refreshes is fitted
//! from those returns ([`Grid`]); in late pacing a probe frame every half
//! second starts right after the one before, so its acquire waits and the
//! grid stays locked (its frame is still shown at the next refresh: no
//! frame is lost). A frame's cost, from its planned start to its present
//! with the waits left out, is measured; its planned start is its refresh
//! minus that cost (95th percentile of the last 120) and a margin. An
//! acquire that waits when a drawable should have been free means the
//! frame before missed its refresh: the margin grows for a while, and if
//! no frame of the last half minute made its refresh with less time to
//! spare than the one that missed (a deadline, not a hiccup) its floor
//! rises, so that deadline is not missed again; the floor comes down a
//! step a minute without misses, and each play starts from the first one. Every
//! few seconds a burst of frames one after another looks for a faster
//! refresh than the grid's (a display that went from 60 to 120 Hz while
//! the OS's rate was not looked at again).
//!
//! The predicted presentation late pacing gives the play is the refresh
//! the frame is meant for, on the same grid the blocking frames' "acquire
//! returned plus a frame" lands on, so the calibrated visual offset holds
//! in both modes. Judging never reads it: input keeps its own stamps and
//! the audio clock its authority.
//!
//! Pure: times are host seconds (`crate::clock`); tested on a fake clock
//! and against a simulated display and swapchain (`sim`, built for tests
//! and with the `sim` feature only).

use std::collections::VecDeque;

pub mod report;
#[cfg(any(test, feature = "sim"))]
pub mod sim;
#[cfg(test)]
mod tests;

pub use report::Report;

/// An acquire that took this long (s) waited for the display.
pub const BLOCKED: f64 = 0.001;
/// A wake this close (s) is not slept for.
const EARLY: f64 = 0.000_2;
/// How often (s) late pacing probes the grid.
pub const PROBE_EVERY: f64 = 0.5;
/// Observations kept (s; the last 16 whatever their age).
const KEEP: f64 = 3.0;
/// Observations needed to lock the grid.
const LOCK: usize = 6;
/// The grid is lost when nothing has been observed for this long (s).
const STALE: f64 = 2.0;
/// Frames whose costs are kept.
const COSTS: usize = 120;
/// The margin's first floor (s).
pub const MARGIN: f64 = 0.002;
/// What a missed refresh adds to the margin (s).
const MARGIN_STEP: f64 = 0.001_5;
/// What a deadline's miss adds to the margin's floor, and what the
/// floor comes down by a minute without misses (s).
const FLOOR_STEP: f64 = 0.000_5;
/// How long (s) the frames that made their refresh are remembered, with
/// the time they had to spare: a miss with more to spare than one of them
/// is a hiccup.
const MADE: f64 = 30.0;
/// Less than this (s) between two frames' time to spare is noise.
const SPARE_NOISE: f64 = 0.000_1;
/// The floor comes down a step after this long (s) without a miss.
const FLOOR_HOLD: f64 = 60.0;
/// How often (s) late pacing looks for a faster refresh, and with how
/// many frames one after another.
pub const BURST_EVERY: f64 = 4.0;
const BURST: u32 = 4;
/// The margin falls back to its floor with this time constant (s).
const MARGIN_DECAY: f64 = 5.0;
/// An acquire that began this long (s) before the last frame's refresh
/// was expected to wait.
const TOLERANCE: f64 = 0.000_5;

/// How a play's frames are paced.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum Mode {
    /// One after another, each waiting in the surface's acquire.
    #[default]
    Block,
    /// Each started as late before its refresh as its cost allows.
    Late,
}

impl Mode {
    /// `block` or `late`.
    pub fn parse(s: &str) -> Option<Mode> {
        match s {
            "block" => Some(Mode::Block),
            "late" => Some(Mode::Late),
            _ => None,
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            Mode::Block => "block",
            Mode::Late => "late",
        }
    }
}

/// When the next frame starts.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Next {
    /// Now.
    Now,
    /// At this host time (s): the loop sleeps until then.
    At(f64),
}

/// The display's refreshes, fitted from the times blocked acquires
/// returned at.
#[derive(Clone, Debug)]
pub struct Grid {
    /// The period the OS gives (s).
    hint: Option<f64>,
    /// The period in use (s).
    period: f64,
    /// Measured (not the hint's or the default).
    measured: bool,
    /// Recent observations (host s).
    obs: VecDeque<f64>,
    /// A refresh (host s), once locked.
    anchor: Option<f64>,
    /// The residuals' spread at the last fit (s).
    spread: f64,
}

impl Default for Grid {
    fn default() -> Grid {
        Grid {
            hint: None,
            period: 1.0 / 60.0,
            measured: false,
            obs: VecDeque::new(),
            anchor: None,
            spread: 0.0,
        }
    }
}

fn median(v: &mut [f64]) -> f64 {
    v.sort_by(f64::total_cmp);
    let n = v.len();
    if n == 0 {
        0.0
    } else if n % 2 == 1 {
        v[n / 2]
    } else {
        (v[n / 2 - 1] + v[n / 2]) / 2.0
    }
}

impl Grid {
    /// The period in use (s).
    pub fn period(&self) -> f64 {
        self.period
    }

    /// Whether the period was measured.
    pub fn measured(&self) -> bool {
        self.measured
    }

    /// The observations' spread around the grid at the last fit (s).
    pub fn spread(&self) -> f64 {
        self.spread
    }

    /// The refresh rate the OS gives (Hz). A new one unlike the period
    /// in use starts the grid again.
    pub fn set_hint(&mut self, hz: Option<f64>) {
        let p = hz.filter(|h| (20.0..=1000.0).contains(h)).map(|h| 1.0 / h);
        if p == self.hint {
            return;
        }
        self.hint = p;
        if let Some(p) = p
            && (!self.measured || (p - self.period).abs() > 0.05 * p)
        {
            self.period = p;
            self.measured = false;
            self.obs.clear();
            self.anchor = None;
        }
    }

    /// Locked, with an observation in the last [`STALE`] seconds.
    pub fn locked_at(&self, now: f64) -> bool {
        self.anchor.is_some() && self.obs.back().is_some_and(|o| now - o <= STALE)
    }

    /// The first refresh at or after `t` (locked), else `t`.
    pub fn next_after(&self, t: f64) -> f64 {
        let Some(a) = self.anchor else { return t };
        let k = ((t - a) / self.period - 1e-6).ceil();
        a + k * self.period
    }

    /// The refresh nearest `t` (locked), else `t`.
    pub fn nearest(&self, t: f64) -> f64 {
        let Some(a) = self.anchor else { return t };
        a + ((t - a) / self.period).round() * self.period
    }

    /// An acquire that waited returned at `t`.
    pub fn observe(&mut self, t: f64) {
        self.obs.push_back(t);
        while self.obs.len() > 16 && t - self.obs[0] > KEEP {
            self.obs.pop_front();
        }
        while self.obs.len() > 256 {
            self.obs.pop_front();
        }
        self.fit();
    }

    /// Starts again on `period` (s), measured: a burst found refreshes
    /// that much closer together.
    pub fn reseed(&mut self, period: f64, returns: &[f64]) {
        self.period = period;
        self.measured = true;
        self.obs.clear();
        self.obs.extend(returns.iter().copied());
        self.anchor = None;
    }

    /// Forgets the observations (a play starts; the phase is looked for
    /// again, the period kept).
    pub fn forget(&mut self) {
        self.obs.clear();
        self.anchor = None;
    }

    fn fit(&mut self) {
        if self.obs.len() < LOCK {
            return;
        }
        // The period: blocking frames' returns are mostly one refresh
        // apart, so their median says the period when the one in use is
        // far from it (no hint, a wrong one).
        let mut close: Vec<f64> = self
            .obs
            .iter()
            .zip(self.obs.iter().skip(1))
            .map(|(a, b)| b - a)
            .filter(|d| (0.001..0.05).contains(d))
            .collect();
        if close.len() >= LOCK - 1 {
            let m = median(&mut close);
            if (m - self.period).abs() > 0.1 * self.period {
                self.period = m;
                self.anchor = None;
            }
        }
        let p = self.period;
        let last = *self.obs.back().unwrap();
        // The phase around the circle, then each observation's refresh
        // count and residual from it.
        let (s, c) = self.obs.iter().fold((0.0, 0.0), |(s, c), o| {
            let a = std::f64::consts::TAU * (o - last) / p;
            (s + a.sin(), c + a.cos())
        });
        let reference = last + s.atan2(c) / std::f64::consts::TAU * p;
        let counted: Vec<(f64, f64)> = self
            .obs
            .iter()
            .map(|o| {
                let k = ((o - reference) / p).round();
                (k, o - reference - k * p)
            })
            .collect();
        let mut res: Vec<f64> = counted.iter().map(|(_, r)| *r).collect();
        let mid = median(&mut res);
        // Outliers (an acquire held up for another reason) left out.
        let kept: Vec<(f64, f64)> = counted
            .into_iter()
            .filter(|(_, r)| (r - mid).abs() <= 0.002_f64.min(p / 4.0))
            .collect();
        if kept.len() < LOCK {
            self.anchor = None;
            return;
        }
        // The period refined by the slope over the observations' span.
        let (k0, k1) = (kept[0].0, kept[kept.len() - 1].0);
        if k1 - k0 >= 8.0 {
            let n = kept.len() as f64;
            let mk = kept.iter().map(|(k, _)| k).sum::<f64>() / n;
            let mr = kept.iter().map(|(_, r)| r).sum::<f64>() / n;
            let (sxy, sxx) = kept.iter().fold((0.0, 0.0), |(sxy, sxx), (k, r)| {
                (sxy + (k - mk) * (r - mr), sxx + (k - mk) * (k - mk))
            });
            let slope = sxy / sxx;
            if slope.abs() < 0.02 * p {
                self.period = p + slope;
                self.measured = true;
            }
        }
        // The phase: the median of the residuals, now on the new period.
        let p = self.period;
        let mut res: Vec<f64> = self
            .obs
            .iter()
            .map(|o| {
                let k = ((o - reference) / p).round();
                o - reference - k * p
            })
            .collect();
        let mid = median(&mut res.clone());
        let all = res.len();
        res.retain(|r| (r - mid).abs() <= 0.002_f64.min(p / 4.0));
        // Most returns must fit: a grid that a few do by chance is none.
        if res.len() < LOCK || res.len() * 10 < all * 7 {
            self.anchor = None;
            return;
        }
        let phase = median(&mut res);
        let mut dev: Vec<f64> = res.iter().map(|r| (r - phase).abs()).collect();
        self.spread = median(&mut dev);
        // A grid the returns do not fit (a variable refresh rate, a
        // compositor that releases at no fixed time) is not one.
        self.anchor = (self.spread < 0.001).then_some(reference + phase);
    }
}

/// A window of recent values.
#[derive(Clone, Debug, Default)]
struct Recent(VecDeque<f64>);

impl Recent {
    fn push(&mut self, v: f64) {
        self.0.push_back(v);
        if self.0.len() > COSTS {
            self.0.pop_front();
        }
    }

    /// A quantile (0 when empty).
    fn quantile(&self, q: f64) -> f64 {
        if self.0.is_empty() {
            return 0.0;
        }
        let mut v: Vec<f64> = self.0.iter().copied().collect();
        v.sort_by(f64::total_cmp);
        v[((v.len() - 1) as f64 * q).round() as usize]
    }
}

/// The frame being made.
#[derive(Clone, Copy, Debug, Default)]
struct Frame {
    begin: f64,
    /// When it was planned to start (late pacing).
    scheduled: Option<f64>,
    probe: bool,
    /// Part of a burst.
    burst: bool,
    acquire: (f64, f64),
    predicted: f64,
    latch_miss: bool,
}

/// What one frame did (for the report).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Record {
    pub mode: Mode,
    /// A probe (late pacing): started right after the frame before.
    pub probe: bool,
    pub begin: f64,
    /// How late it started after its planned start (late pacing; s).
    pub late: Option<f64>,
    /// Time spent in the acquire (s).
    pub blocked: f64,
    /// Time spent in the present call (s).
    pub present: f64,
    /// When the present call returned (host s).
    pub end: f64,
    /// From the acquire's return to the present's (s).
    pub build: f64,
    /// When the play read its clock and input (host s).
    pub sample: f64,
    /// The refresh it was drawn for (host s).
    pub predicted: f64,
    /// The refresh it is shown at, as the grid has it (host s).
    pub target: f64,
    /// It was presented after the refresh it was drawn for.
    pub overrun: bool,
    /// It was presented in time for a refresh before the one it was
    /// drawn for (built faster than builds usually are).
    pub early: bool,
    /// Its acquire waited when it should not have: the frame before
    /// missed its refresh.
    pub latch_miss: bool,
    /// The grid was locked.
    pub locked: bool,
    pub period: f64,
    /// The period was measured (else the OS's rate or 60 Hz).
    pub measured: bool,
    pub margin: f64,
}

/// Paces a play's frames.
#[derive(Clone, Debug)]
pub struct Pacer {
    mode: Mode,
    grid: Grid,
    /// Frames' costs: from the planned start to the present's return,
    /// waits left out.
    costs: Recent,
    /// From the acquire's return to the present's.
    builds: Recent,
    floor: f64,
    margin: f64,
    last_target: Option<f64>,
    last_end: Option<f64>,
    probe_at: f64,
    /// When the floor last moved (or a refresh was missed).
    floor_since: f64,
    /// Late frames that made their refresh: when, and the time they had
    /// to spare (their refresh minus their present); the last frame's,
    /// until the next acquire says whether it made it.
    made: VecDeque<(f64, f64)>,
    spare: Option<(f64, f64)>,
    /// When the next burst starts, its frames left and their returns.
    burst_at: f64,
    burst_left: u32,
    burst: Vec<f64>,
    frame: Frame,
    /// Frames late pacing saw miss their refresh.
    pub latch_misses: u32,
    /// Faster refreshes found by bursts.
    pub faster_found: u32,
}

impl Default for Pacer {
    fn default() -> Pacer {
        Pacer::new(Mode::Block)
    }
}

impl Pacer {
    pub fn new(mode: Mode) -> Pacer {
        Pacer {
            mode,
            grid: Grid::default(),
            costs: Recent::default(),
            builds: Recent::default(),
            floor: MARGIN,
            margin: MARGIN,
            last_target: None,
            last_end: None,
            probe_at: 0.0,
            floor_since: 0.0,
            made: VecDeque::new(),
            spare: None,
            burst_at: f64::INFINITY,
            burst_left: 0,
            burst: Vec::new(),
            frame: Frame::default(),
            latch_misses: 0,
            faster_found: 0,
        }
    }

    pub fn mode(&self) -> Mode {
        self.mode
    }

    pub fn set_mode(&mut self, mode: Mode) {
        self.mode = mode;
    }

    pub fn grid(&self) -> &Grid {
        &self.grid
    }

    pub fn margin(&self) -> f64 {
        self.margin
    }

    /// The refresh rate the OS gives (Hz).
    pub fn set_refresh_hint(&mut self, hz: Option<f64>) {
        self.grid.set_hint(hz);
    }

    /// The margin's floor (s).
    pub fn floor(&self) -> f64 {
        self.floor
    }

    /// A play starts: the phase is looked for again, the margin starts
    /// from its first floor (the period and the costs are kept).
    pub fn restart(&mut self) {
        self.grid.forget();
        self.last_target = None;
        self.last_end = None;
        self.probe_at = 0.0;
        self.floor = MARGIN;
        self.margin = MARGIN;
        self.made.clear();
        self.spare = None;
        self.burst_at = f64::INFINITY;
        self.burst_left = 0;
    }

    /// The time budgeted for a frame: its cost and the margin (s).
    pub fn budget(&self) -> f64 {
        self.costs.quantile(0.95).max(0.000_5) + self.margin
    }

    /// When a frame should start, after the frame shown at `last`.
    fn wake_after(&self, last: f64) -> f64 {
        (last + self.grid.period - self.budget()).max(last)
    }

    /// The planned start of the next frame, if late pacing plans it.
    fn planned(&self, now: f64) -> Option<f64> {
        if self.mode != Mode::Late
            || !self.grid.locked_at(now)
            || now >= self.probe_at
            || now >= self.burst_at
            || self.burst_left > 0
        {
            return None;
        }
        self.last_target.map(|l| self.wake_after(l))
    }

    /// When the next frame starts, asked at `now`.
    pub fn next(&self, now: f64) -> Next {
        match self.planned(now) {
            Some(at) if now + EARLY < at => Next::At(at),
            _ => Next::Now,
        }
    }

    /// A frame starts at `now`.
    pub fn begin(&mut self, now: f64) {
        let scheduled = self.planned(now);
        let locked = self.grid.locked_at(now);
        if self.mode == Mode::Late && locked && self.burst_left == 0 && now >= self.burst_at {
            self.burst_left = BURST;
            self.burst.clear();
        }
        self.frame = Frame {
            begin: now,
            scheduled,
            probe: self.mode == Mode::Late && scheduled.is_none() && locked,
            burst: self.mode == Mode::Late && self.burst_left > 0,
            ..Frame::default()
        };
    }

    /// The refresh a frame done at `t` is shown at.
    fn target_after(&self, t: f64) -> f64 {
        let p = self.grid.period;
        if !self.grid.locked_at(t) {
            return t + p;
        }
        let after = self.last_target.map_or(t, |l| t.max(l + p / 2.0));
        self.grid.next_after(after)
    }

    /// The frame's acquire ran from `start` to `end`. The refresh it is
    /// drawn for, when late pacing has one (else the play predicts it).
    pub fn acquired(&mut self, start: f64, end: f64) -> Option<f64> {
        let blocked = end - start;
        let expected = self.last_target.is_some_and(|l| start < l - TOLERANCE);
        if blocked > BLOCKED {
            let was_locked = self.grid.locked_at(start);
            self.grid.observe(end);
            if self.grid.locked_at(end) {
                self.probe_at = end + PROBE_EVERY;
                if self.burst_at.is_infinite() {
                    self.burst_at = end + BURST_EVERY;
                }
            }
            if self.frame.burst {
                self.burst.push(end);
            }
            // A wait that ends a refresh after the frame before was due
            // (a probe's, a burst's) says it missed too.
            let late_return = self
                .last_target
                .is_some_and(|l| end > l + self.grid.period / 2.0);
            if self.mode == Mode::Late
                && was_locked
                && (!expected || late_return)
                && self.last_target.is_some()
            {
                // The frame before missed its refresh: it is shown now.
                self.frame.latch_miss = true;
                self.latch_misses += 1;
                let cap = self.grid.period / 2.0;
                // A deadline the margin's floor must cover, unless a frame
                // lately made its refresh with less time to spare (a
                // hiccup: the margin is lifted for a while only).
                let spare = self.spare.take().map(|(_, s)| s);
                let hiccup =
                    spare.is_some_and(|s| self.made.iter().any(|(_, m)| *m < s - SPARE_NOISE));
                if !hiccup {
                    self.floor = (self.floor + FLOOR_STEP).min(cap);
                }
                self.margin = (self.margin.max(self.floor) + MARGIN_STEP).min(cap);
                self.floor_since = end;
                self.last_target = Some(self.grid.nearest(end));
            }
        }
        // The frame before made its refresh.
        if let Some(made) = self.spare.take() {
            self.made.push_back(made);
        }
        while self.made.front().is_some_and(|(t, _)| end - t > MADE) {
            self.made.pop_front();
        }
        if self.frame.burst {
            self.burst_left -= 1;
            if self.burst_left == 0 {
                self.end_burst(end);
            }
        }
        self.frame.acquire = (start, end);
        // A typical build: one that takes longer and misses the refresh
        // is an overrun, known as one.
        self.frame.predicted = self.target_after(end + self.builds.quantile(0.5));
        (self.mode == Mode::Late && self.grid.locked_at(end)).then_some(self.frame.predicted)
    }

    /// A burst's frames are done: returns closer together than the grid's
    /// period are a faster refresh, the grid started again on it.
    fn end_burst(&mut self, end: f64) {
        self.burst_at = end + BURST_EVERY;
        if self.burst.len() < 3 {
            return;
        }
        let mut gaps: Vec<f64> = self.burst.windows(2).map(|w| w[1] - w[0]).collect();
        let gap = median(&mut gaps);
        if gap > 0.001 && gap < 0.75 * self.grid.period {
            let returns = std::mem::take(&mut self.burst);
            self.grid.reseed(gap, &returns);
            self.faster_found += 1;
        }
    }

    /// The play read its clock at `sample`; the present call ran from
    /// `start` to `end`. What the frame did.
    pub fn presented(&mut self, sample: f64, start: f64, end: f64) -> Record {
        let f = self.frame;
        let build = start - f.acquire.1;
        let late = f.scheduled.map(|s| (f.begin - s).max(0.0));
        let cost = (f.acquire.0 - f.begin) + (end - f.acquire.1) + late.unwrap_or(0.0);
        self.costs.push(cost);
        self.builds.push(end - f.acquire.1);
        let locked = self.grid.locked_at(end);
        let target = if locked {
            self.target_after(start)
        } else {
            f.predicted
        };
        let overrun = target > f.predicted + self.grid.period / 2.0;
        let early = target < f.predicted - self.grid.period / 2.0;
        // A late frame's time to spare, known to have been enough once
        // the next acquire does not wait.
        self.spare =
            (self.mode == Mode::Late && locked && !overrun).then_some((end, target - start));
        // The floor comes down a step a minute without misses.
        if self.floor > MARGIN && end - self.floor_since > FLOOR_HOLD {
            self.floor = (self.floor - FLOOR_STEP).max(MARGIN);
            self.floor_since = end;
        }
        if let Some(prev) = self.last_end {
            let k = (-(end - prev).max(0.0) / MARGIN_DECAY).exp();
            self.margin = self.floor + (self.margin - self.floor) * k;
        }
        self.last_end = Some(end);
        self.last_target = Some(target);
        Record {
            mode: self.mode,
            probe: f.probe,
            begin: f.begin,
            late,
            blocked: f.acquire.1 - f.acquire.0,
            present: end - start,
            end,
            build,
            sample,
            predicted: f.predicted,
            target,
            overrun,
            early,
            latch_miss: f.latch_miss,
            locked,
            period: self.grid.period,
            measured: self.grid.measured,
            margin: self.margin,
        }
    }
}
