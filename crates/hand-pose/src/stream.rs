// SPDX-FileCopyrightText: 2026 すごいジャン <me@sugoijan.dev>
// SPDX-License-Identifier: MIT
//! Stream mode for dense passages (`docs/plans/hand-stream-mode.md`).
//!
//! Dense stretches of a chart leave exact rolling preparation for motion fitted
//! once per knot: every note keeps its arrow and its time, but contact is no
//! longer exact. The switching rule reads only notes already known when each
//! row arrives, and a stream knot is fitted only once its targets and balance
//! window can no longer change, so stream motion is the same for every request
//! timing; the cost is a few fits per note, whatever the density.
use crate::curve::{Curve, value};
use crate::solve::{Stop, solve_with};
use crate::{Goal, Guide, Model, Parameters, Plan, refine_contacts};
use std::collections::VecDeque;

/// Seconds of rows, ending at a row, that its rate counts: the window is open
/// at its start, so a steady rate counts as itself.
const RATE_WINDOW: f64 = 1.;
/// Note times come from beats and tempos in floating point: a row a window
/// earlier, to this, is outside it (5 rows per second never read as 6).
const TIME_EPS: f64 = 1e-6;
/// Longest flight (release to landing) that knot finality allows for. The
/// contact planner's flights are at most 0.42 s; longer ones are counted.
const FLIGHT_BOUND: f64 = 0.5;
/// The balance target is the support point (the planted fingertip, or the
/// midpoint when both or neither are planted) smoothed by a Gaussian of this
/// sigma, truncated at `BALANCE_REACH`, on an absolute 120 Hz grid. It reads
/// targets alone, which are final with their swings: the plan's support
/// weight would also need the next contact change, unknown inside a gap.
const BALANCE_SIGMA: f64 = 0.08;
const BALANCE_REACH: f64 = 0.16;
/// Fill knots lie on an absolute grid of this rate, away from swing markers.
const FILL_HZ: f64 = 30.;
/// Stream entry may begin no earlier than this after its row can first be
/// known (the commit margin plus a request's lag).
const ENTRY_MARGIN: f64 = 0.35;
/// Stream exit likewise, leaving room for the exact job after it: its 0.6 s
/// splice ends before the exit blend begins, and it prepares everything
/// visible beyond (up to 2.6 s) by the 0.25 s commit margin. At 1.1 s, 15
/// frames for about 565 units of 36 a frame, it was a frame late after long
/// gaps; 1.5 s gives 39 frames for about 490.
const EXIT_MARGIN: f64 = 1.5;
/// First fits are smoothed like exact preparation's (a Gaussian of this
/// sigma, density-weighted, truncated at `SMOOTH_REACH`) before the refit:
/// with both fingers in the air, fingertip targets leave the root free, and
/// independent fits wander.
const SMOOTH_SIGMA: f64 = 0.04;
const SMOOTH_REACH: f64 = 0.16;
/// Shortest full-stream time before an exit.
const MIN_STREAM: f64 = 0.5;
/// Stream knots kept past an exit, so a dense row arriving later can still
/// merge the sections.
const EXIT_TAIL: f64 = 1.5;

/// Opt-in stream policy (`RollingOptions::stream`).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct StreamOptions {
    /// Rows per second (over the second ending at a row) that start stream mode.
    pub enter: f64,
    /// Rows per second below which stream mode ends.
    pub leave: f64,
    /// Seconds of each blend between exact and stream motion.
    pub blend: f64,
}
impl Default for StreamOptions {
    fn default() -> Self {
        Self {
            enter: 6.,
            leave: 4.,
            blend: 0.25,
        }
    }
}
impl StreamOptions {
    pub fn valid(&self) -> bool {
        self.enter.is_finite()
            && self.leave.is_finite()
            && self.leave > 0.
            && self.leave <= self.enter
            && self.blend.is_finite()
            && self.blend > 0.
            && self.blend <= 1.
    }
}

fn smoother(u: f64) -> f64 {
    if u <= 0. {
        0.
    } else if u >= 1. {
        1.
    } else {
        u * u * u * (10. + u * (-15. + 6. * u))
    }
}

#[derive(Default)]
struct Motion {
    /// First fits, each warm-started from the previous: time, pose, balance
    /// target and fingertip error.
    raw: Vec<(f64, Parameters, [f64; 2], f64)>,
    /// Raw knots already smoothed and refitted.
    refitted: usize,
    /// No raw knot will follow the last one (the chart's end).
    raw_closed: bool,
    /// Refitted knots, the motion sampled.
    times: Vec<f64>,
    poses: Vec<Parameters>,
    centers: Vec<[f64; 2]>,
    curve: Option<Curve>,
    /// No knot will follow the last one (the chart's end).
    closed: bool,
}
impl Motion {
    /// The next raw knot to smooth and refit, once the raw knots its
    /// smoothing reads (and their neighbors) are all fitted.
    fn smoothable(&self) -> Option<usize> {
        let f = self.refitted;
        let t = self.raw.get(f)?.0;
        (self.raw_closed || self.raw.last()?.0 > t + SMOOTH_REACH).then_some(f)
    }

    /// The density-weighted Gaussian average of the raw poses around `f`.
    fn smoothed(&self, f: usize) -> Parameters {
        let n = self.raw.len();
        let t = self.raw[f].0;
        let (mut sum, mut weight) = ([0.; 14], 0.);
        for j in 0..n {
            let dt = self.raw[j].0 - t;
            if dt.abs() > SMOOTH_REACH {
                continue;
            }
            let density = if n == 1 {
                1.
            } else if j == 0 {
                self.raw[1].0 - self.raw[0].0
            } else if j + 1 == n {
                self.raw[n - 1].0 - self.raw[n - 2].0
            } else {
                (self.raw[j + 1].0 - self.raw[j - 1].0) / 2.
            };
            let w = (-0.5 * (dt / SMOOTH_SIGMA).powi(2)).exp() * density;
            weight += w;
            for (s, v) in sum.iter_mut().zip(&self.raw[j].1) {
                *s += w * v;
            }
        }
        sum.map(|v| v / weight)
    }

    /// Through this time, sampled motion can no longer change: a quintic
    /// segment's end derivatives depend on the knot after it.
    fn final_through(&self) -> f64 {
        let n = self.times.len();
        if self.closed && n >= 2 {
            self.times[n - 1]
        } else if n >= 3 {
            self.times[n - 2]
        } else {
            f64::NEG_INFINITY
        }
    }
}

struct Section {
    /// Blend in begins.
    enter: f64,
    /// Blend in ends; stream motion alone from here.
    full: f64,
    /// Blend out begins (ending `blend` later); `None` while open.
    leave: Option<f64>,
    motion: Motion,
}

pub(crate) struct Stream {
    options: StreamOptions,
    horizon: f64,
    stop: Stop,
    /// Known rows within the rate window of the last one.
    rows: VecDeque<f64>,
    last_row: f64,
    sections: Vec<Section>,
    /// Time ranges of sections no longer kept, for checks of published motion.
    past: Vec<[f64; 2]>,
    plan: Option<Plan>,
    /// Knots at or before this time are final.
    final_until: f64,
    complete: bool,
    /// Support points on the 120 Hz grid: (index of the first, points).
    balance: (i64, VecDeque<[f64; 2]>),
    /// Swings already checked against `FLIGHT_BOUND`, per track.
    inspected: [usize; 2],
    pub(crate) knots: usize,
    pub(crate) long_flights: usize,
    pub(crate) sections_started: usize,
}

impl Stream {
    pub(crate) fn new(options: StreamOptions, horizon: f64, guide_tolerance: f64) -> Self {
        Self {
            options,
            horizon,
            stop: Stop::scaled(guide_tolerance),
            rows: VecDeque::new(),
            last_row: f64::NEG_INFINITY,
            sections: Vec::new(),
            past: Vec::new(),
            plan: None,
            final_until: f64::NEG_INFINITY,
            complete: false,
            balance: (0, VecDeque::new()),
            inspected: [0; 2],
            knots: 0,
            long_flights: 0,
            sections_started: 0,
        }
    }

    /// Take a request's visible plan: decide newly known rows and advance the
    /// finality bound. `complete` means the plan's duration is the chart's end
    /// and every note is visible.
    pub(crate) fn observe(&mut self, visible: &Plan, visible_until: f64, complete: bool) {
        let mut times: Vec<f64> = visible
            .events
            .iter()
            .map(|e| e.time)
            .filter(|t| *t > self.last_row)
            .collect();
        times.sort_by(f64::total_cmp);
        times.dedup();
        for t in times {
            self.decide(visible, t);
        }
        for (d, track) in [&visible.tracks.left, &visible.tracks.right]
            .into_iter()
            .enumerate()
        {
            let from = self.inspected[d].min(track.len());
            self.long_flights += track[from..]
                .iter()
                .filter(|s| s.land - s.release > FLIGHT_BOUND)
                .count();
            self.inspected[d] = track.len();
        }
        self.complete = complete;
        // Targets at a time are final once every swing releasing by then is
        // visible: those land within `FLIGHT_BOUND`.
        self.final_until = if complete {
            visible.duration
        } else {
            visible_until - FLIGHT_BOUND - BALANCE_REACH
        }
        .max(self.final_until);
        self.plan = Some(visible.clone());
    }

    fn decide(&mut self, plan: &Plan, t: f64) {
        let previous = self.last_row;
        self.last_row = t;
        self.rows.push_back(t);
        while self
            .rows
            .front()
            .is_some_and(|r| *r <= t - RATE_WINDOW + TIME_EPS)
        {
            self.rows.pop_front();
        }
        let rate = self.rows.len() as f64 / RATE_WINDOW;
        let open = self.sections.last().is_some_and(|s| s.leave.is_none());
        let blend = self.options.blend;
        if !open && rate >= self.options.enter {
            // Stream motion alone by the earliest release landing on this row.
            let release = [&plan.tracks.left, &plan.tracks.right]
                .into_iter()
                .filter_map(|track| {
                    let i = track.partition_point(|s| s.land < t);
                    track.get(i).filter(|s| s.land == t).map(|s| s.release)
                })
                .fold(t, f64::min);
            let mut enter = (release - blend)
                .max(t - self.horizon + ENTRY_MARGIN)
                .max(0.);
            if let Some(last) = self.sections.last_mut() {
                let leave = last.leave.expect("closed section");
                if leave >= t - self.horizon + EXIT_MARGIN {
                    // Its exit has not begun: continue the same section.
                    last.leave = None;
                    return;
                }
                enter = enter.max(leave + blend);
            }
            self.sections.push(Section {
                enter,
                full: enter + blend,
                leave: None,
                motion: Motion::default(),
            });
            self.sections_started += 1;
        } else if open && rate < self.options.leave {
            let horizon = self.horizon;
            let s = self.sections.last_mut().unwrap();
            s.leave = Some(
                previous
                    .max(s.full + MIN_STREAM)
                    .max(t - horizon + EXIT_MARGIN),
            );
        }
    }

    fn section(&self, t: f64) -> Option<&Section> {
        let i = self.sections.partition_point(|s| s.enter < t);
        let s = self.sections.get(i.checked_sub(1)?)?;
        let end = s.leave.map_or(f64::INFINITY, |x| x + self.options.blend);
        (t < end).then_some(s)
    }

    /// Stream weight: 0 for exact motion, 1 for stream motion, a quintic
    /// smoothstep between.
    pub(crate) fn weight(&self, t: f64) -> f64 {
        let Some(s) = self.section(t) else {
            return 0.;
        };
        let up = smoother((t - s.enter) / self.options.blend);
        let down = s
            .leave
            .map_or(1., |x| 1. - smoother((t - x) / self.options.blend));
        up.min(down)
    }

    /// Where stream motion alone holds at `t`, until when (infinite while the
    /// section is open).
    pub(crate) fn full_through(&self, t: f64) -> Option<f64> {
        let s = self.section(t)?;
        let until = s.leave.unwrap_or(f64::INFINITY);
        (t >= s.full && t <= until).then_some(until)
    }

    fn interval(&self, s: &Section) -> [f64; 2] {
        [
            s.enter,
            s.leave.map_or(f64::INFINITY, |x| x + self.options.blend),
        ]
    }

    /// Every time range with stream weight so far.
    pub(crate) fn intervals(&self) -> Vec<[f64; 2]> {
        self.past
            .iter()
            .copied()
            .chain(self.sections.iter().map(|s| self.interval(s)))
            .collect()
    }

    /// Bound a future snapshot by actual finalized stream knots, including
    /// smoothing's lookahead and the extra interpolation neighbor. Before a
    /// section's first prepared knot its blend-in boundary still has weight 0.
    pub(crate) fn cap_end(&self, mut end: f64) -> f64 {
        for s in &self.sections {
            let last = self.interval(s)[1];
            let ready = s.motion.final_through();
            if s.enter < end && ready < last {
                end = end.min(ready.max(s.enter));
            }
        }
        end
    }

    /// Stream pose and balance target at `t`, if final.
    pub(crate) fn sample(&self, t: f64) -> Option<(Parameters, [f64; 2])> {
        let m = &self.section(t)?.motion;
        if t < *m.times.first()? || t > m.final_through() {
            return None;
        }
        Some(value(m.curve.as_ref()?.at(t).p))
    }

    /// Whether a knot is ready to fit.
    #[cfg(feature = "profiling")]
    pub(crate) fn pending(&self) -> bool {
        let Some(plan) = &self.plan else {
            return false;
        };
        self.sections
            .iter()
            .any(|s| s.motion.smoothable().is_some() || self.next(plan, s).is_some())
    }

    fn end(&self, plan: &Plan, s: &Section) -> f64 {
        s.leave
            .map_or(self.final_until, |x| self.final_until.min(x + EXIT_TAIL))
            .min(plan.duration)
    }

    /// The section's next knot time, if final.
    fn next(&self, plan: &Plan, s: &Section) -> Option<f64> {
        if s.motion.raw_closed {
            return None;
        }
        let end = self.end(plan, s);
        let t = match s.motion.raw.last().map(|k| k.0) {
            None => s.enter,
            Some(after) => {
                let t = next_knot(plan, after);
                if t > plan.duration && self.complete && after < plan.duration {
                    plan.duration
                } else {
                    t
                }
            }
        };
        (t <= end).then_some(t)
    }

    /// Fit up to `units` final knots, earliest section first; returns the
    /// number fitted. Motion before `now - 1.5` is dropped.
    pub(crate) fn advance(&mut self, m: &Model, now: f64, units: usize) -> usize {
        let Some(plan) = self.plan.take() else {
            return 0;
        };
        let mut used = 0;
        for i in 0..self.sections.len() {
            let mut added = false;
            while used < units {
                // Refit first: it is what playback samples.
                if let Some(f) = self.sections[i].motion.smoothable() {
                    let motion = &self.sections[i].motion;
                    let (t, raw, com, raw_error) = motion.raw[f];
                    let smoothed = motion.smoothed(f);
                    let goal = Goal {
                        tips: plan.targets(t).tips,
                        com,
                    };
                    let mut q = smoothed;
                    let mut error = solve_with(m, &mut q, &smoothed, &goal, 12, true, &self.stop)
                        .target_error_mm;
                    if error > 0.05 {
                        let mut refined = q;
                        let r = refine_contacts(m, &mut refined, &smoothed, &goal, 8);
                        if r.target_error_mm < error {
                            (q, error) = (refined, r.target_error_mm);
                        }
                    }
                    if error > 2. && raw_error < error {
                        q = raw;
                    }
                    let complete = self.complete && t == plan.duration;
                    let motion = &mut self.sections[i].motion;
                    motion.refitted += 1;
                    motion.times.push(t);
                    motion.poses.push(q);
                    motion.centers.push(com);
                    motion.closed |= complete;
                    self.knots += 1;
                    used += 1;
                    added = true;
                    continue;
                }
                let Some(t) = self.next(&plan, &self.sections[i]) else {
                    break;
                };
                let com = self.balance(&plan, t);
                let goal = Goal {
                    tips: plan.targets(t).tips,
                    com,
                };
                let motion = &self.sections[i].motion;
                // Start from the previous knot (continuity), regularized towards
                // the style pose matched to these targets (posture): with the
                // previous knot as the prior, posture drifts over a long section.
                let (mut q, cap, prior) = match motion.raw.last() {
                    Some(k) => (k.1, 12, plan.seed(m, t, Some(&k.1))),
                    None => {
                        let seed = plan.seed(m, t, None);
                        (seed, 24, seed)
                    }
                };
                let mut error =
                    solve_with(m, &mut q, &prior, &goal, cap, true, &self.stop).target_error_mm;
                if cap == 12 && error > 0.05 {
                    // A step blocked by a joint limit stops the ordinary fit
                    // early; the bounds-aware refinement keeps going from the
                    // same pose, so neighboring knots stay close.
                    let mut refined = q;
                    let r = refine_contacts(m, &mut refined, &prior, &goal, 8);
                    if r.target_error_mm < error {
                        (q, error) = (refined, r.target_error_mm);
                    }
                }
                if cap == 12 && error > 2. {
                    // Only a gross miss restarts from the style poses: a
                    // restart can land on a distant pose, a visible jump.
                    let mut retry = plan.seed(m, t, Some(&q));
                    let prior = retry;
                    let r = solve_with(m, &mut retry, &prior, &goal, 24, true, &self.stop);
                    if r.target_error_mm < error {
                        (q, error) = (retry, r.target_error_mm);
                    }
                }
                let motion = &mut self.sections[i].motion;
                motion.raw.push((t, q, com, error));
                motion.raw_closed |= self.complete && t == plan.duration;
                self.knots += 1;
                used += 1;
            }
            let motion = &mut self.sections[i].motion;
            // Raw knots a pending smoothing still reads stay.
            let first = motion
                .raw
                .get(motion.refitted)
                .map_or(f64::INFINITY, |k| k.0 - SMOOTH_REACH - 0.1);
            let keep = motion
                .raw
                .partition_point(|k| k.0 < (now - 1.5).min(first))
                .min(motion.refitted);
            motion.raw.drain(..keep);
            motion.refitted -= keep;
            if added {
                let keep = motion
                    .times
                    .partition_point(|t| *t < now - 1.5)
                    .min(motion.times.len().saturating_sub(3));
                motion.times.drain(..keep);
                motion.poses.drain(..keep);
                motion.centers.drain(..keep);
                if motion.times.len() >= 2 {
                    motion.curve = Some(Curve::from_guide(&Guide {
                        times: motion.times.clone(),
                        poses: motion.poses.clone(),
                        centers: motion.centers.clone(),
                    }));
                }
            }
        }
        // Finished sections well behind `now` are no longer sampled.
        let blend = self.options.blend;
        let n = self.sections.len();
        let old = self.sections[..n.saturating_sub(1)]
            .partition_point(|s| s.leave.is_some_and(|x| x + blend < now - 2.));
        for s in self.sections.drain(..old).collect::<Vec<_>>() {
            let interval = self.interval(&s);
            self.past.push(interval);
        }
        let first = (((now - 2.) * 120.).floor() as i64).max(self.balance.0);
        let drop = ((first - self.balance.0) as usize).min(self.balance.1.len());
        self.balance.1.drain(..drop);
        self.balance.0 += drop as i64;
        self.plan = Some(plan);
        used
    }

    /// The support point smoothed over ±0.16 s on the absolute 120 Hz grid.
    fn balance(&mut self, plan: &Plan, t: f64) -> [f64; 2] {
        let lo = ((t - BALANCE_REACH) * 120.).ceil() as i64;
        let hi = ((t + BALANCE_REACH) * 120.).floor() as i64;
        let (mut sum, mut weight) = ([0.; 2], 0.);
        for k in lo..=hi {
            let p = self.support(plan, k);
            let w = (-0.5 * ((k as f64 / 120. - t) / BALANCE_SIGMA).powi(2)).exp();
            sum[0] += w * p[0];
            sum[1] += w * p[1];
            weight += w;
        }
        [sum[0] / weight, sum[1] / weight]
    }

    fn support(&mut self, plan: &Plan, k: i64) -> [f64; 2] {
        let point = |k: i64| {
            let target = plan.targets(k as f64 / 120.);
            let u = match target.contact {
                [true, false] => 0.,
                [false, true] => 1.,
                _ => 0.5,
            };
            let tips = target.tips;
            [
                tips[0][0] * (1. - u) + tips[1][0] * u,
                tips[0][1] * (1. - u) + tips[1][1] * u,
            ]
        };
        let (base, cache) = &mut self.balance;
        if cache.is_empty() || k < *base {
            cache.clear();
            *base = k;
        }
        while (*base + cache.len() as i64) <= k {
            let next = *base + cache.len() as i64;
            cache.push_back(point(next));
        }
        cache[(k - *base) as usize]
    }
}

/// The next knot after `after`: a swing's release, landing or midpoint (one
/// within a millisecond after another is dropped), or an absolute 30 Hz fill
/// point with no marker within 1/60 s. A pure function of the plan near
/// `after`, so knots never depend on how the work was divided.
fn next_knot(plan: &Plan, after: f64) -> f64 {
    let mut markers = Vec::new();
    for track in [&plan.tracks.left, &plan.tracks.right] {
        let from = track.partition_point(|s| s.land < after - 0.5);
        for s in track[from..]
            .iter()
            .take_while(|s| s.release <= after + 0.1)
        {
            markers.extend([s.release, s.land, (s.release + s.land) / 2.]);
        }
    }
    markers.retain(|t| *t > after - 0.02 && *t <= after + 0.1);
    markers.sort_by(f64::total_cmp);
    markers.dedup();
    let kept: Vec<f64> = markers
        .iter()
        .enumerate()
        .filter(|(i, t)| *i == 0 || **t - markers[i - 1] >= 1e-3)
        .map(|(_, t)| *t)
        .collect();
    let marker = kept.iter().copied().find(|t| *t > after);
    let first = (after * FILL_HZ).floor() as i64 + 1;
    let fill = (first..first + 2)
        .map(|k| k as f64 / FILL_HZ)
        .filter(|g| *g > after)
        .find(|g| markers.iter().all(|m| (m - g).abs() >= 1. / 60.));
    match (marker, fill) {
        (Some(a), Some(b)) => a.min(b),
        (Some(a), None) => a,
        (None, Some(b)) => b,
        // Unreachable: two fill points cannot both have markers within 1/60 s
        // without one of those markers lying ahead of `after`.
        (None, None) => (after * FILL_HZ).floor() / FILL_HZ + 3. / FILL_HZ,
    }
}
