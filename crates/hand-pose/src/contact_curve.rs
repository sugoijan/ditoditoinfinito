//! Prepared continuous pose output. No iterative pose solve occurs at sample time.
use crate::curve::{Curve, value};
use crate::kinematics::{Eval, Kin};
use crate::{Goal, Guide, Model, Parameters, Plan, refine_contacts};

const MAX_KNOTS: usize = 32768;
const MAX_PASSES: usize = 6;
// Difficult double-mat fits may need a second, bounded pass over the curve
// after the anchor branch is repaired. Each fit still yields after eight
// iterations; no solve occurs at presentation time.
const DOUBLE_MAX_PASSES: usize = 12;
const MIN_INTERVAL: f64 = 0.001;
// Preparation leaves 50 micrometers inside the published 0.25 mm floor limit.
// Its finite probes must not use the entire allowance before interpolation is
// checked at arbitrary presentation times and by the denser audit.
const PREPARED_FLOOR: f64 = -0.20;

#[derive(Clone, Debug, serde::Serialize)]
pub struct ContactCurveReport {
    pub knots: usize,
    pub added_knots: usize,
    pub refinement_passes: usize,
    pub checked_samples: usize,
    pub maximum_error_mm: f64,
    pub maximum_planted_height_mm: f64,
    pub minimum_tile_margin_mm: f64,
    pub contact_goal_met: bool,
}

#[derive(Clone, Debug, serde::Serialize)]
pub struct ContactCurveError {
    pub reason: &'static str,
    pub time: f64,
}

#[derive(Clone, Copy, Debug)]
pub struct ContactCurveSample {
    pub pose: Parameters,
    pub velocity: Parameters,
    pub acceleration: Parameters,
}

#[derive(Clone)]
pub struct ContactCurve {
    curve: Curve,
    pub report: ContactCurveReport,
}

fn relaxed_at(relaxed: &[[f64; 2]], t: f64) -> bool {
    relaxed.iter().any(|[a, b]| t >= *a && t <= *b)
}

/// Bounded exact-pose geometry reuse. Contact classification/targets and report
/// updates are always checked again; a hit reuses only immutable kinematics.
#[derive(Default)]
pub(crate) struct InspectionCache {
    model: usize,
    entries: std::collections::BTreeMap<[u64; 14], Eval>,
    order: std::collections::VecDeque<[u64; 14]>,
    pub(crate) hits: usize,
    pub(crate) misses: usize,
    pub(crate) peak: usize,
}
impl InspectionCache {
    fn evaluate(&mut self, m: &Model, q: &Parameters, kin: &mut Kin) -> Eval {
        let model = m as *const Model as usize;
        if model != self.model {
            self.entries.clear();
            self.order.clear();
            self.model = model;
        }
        let key = q.map(f64::to_bits);
        if let Some(g) = self.entries.get(&key) {
            self.hits += 1;
            return *g;
        }
        self.misses += 1;
        let g = m.evaluate_into(q, kin);
        if self.entries.len() >= 8192
            && let Some(old) = self.order.pop_front()
        {
            self.entries.remove(&old);
        }
        self.entries.insert(key, g);
        self.order.push_back(key);
        self.peak = self.peak.max(self.entries.len());
        g
    }
}

/// In `relaxed` (stream) time only bounds, the floor guard and knuckle
/// clearance apply; contact accuracy is not checked or refined there. Mine
/// clearance always applies, including geometry reused from an inspection cache.
#[allow(clippy::too_many_arguments)]
fn inspect(
    m: &Model,
    p: &Plan,
    t: f64,
    q: &Parameters,
    report: &mut ContactCurveReport,
    kin: &mut Kin,
    cache: &mut Option<InspectionCache>,
    relaxed: bool,
) -> Option<&'static str> {
    report.checked_samples += 1;
    if q.iter()
        .enumerate()
        .any(|(i, v)| !v.is_finite() || *v < m.lo[i] - 1e-9 || *v > m.hi[i] + 1e-9)
    {
        return Some("pose_bounds");
    }
    let g = match cache {
        Some(cache) => cache.evaluate(m, q, kin),
        None => m.evaluate_into(q, kin),
    };
    if p.mine_contact(t, &g.tips) {
        return Some("mine_contact");
    }
    if p.roll_violation(t, &g.tips) {
        return Some("roll_contact");
    }
    if p.lift_violation(t, &g.tips) {
        return Some("lift_release");
    }
    let target = p.targets(t);
    // Reserve inside the unchanged output limits for arbitrary subframe times.
    // The denser double-mat audit found small peaks between ordinary probes.
    let mut failed = None;
    for d in (0..2).filter(|_| !relaxed) {
        let error = (0..3)
            .map(|i| (g.tips[d][i] - target.tips[d][i]).powi(2))
            .sum::<f64>()
            .sqrt();
        report.maximum_error_mm = report.maximum_error_mm.max(error);
        if !error.is_finite() || error > if p.double_mat() { 1.90 } else { 2. } {
            failed = Some("position");
        }
        if target.contact[d] {
            let height = g.tips[d][2].abs();
            report.maximum_planted_height_mm = report.maximum_planted_height_mm.max(height);
            if height > if p.double_mat() { 0.225 } else { 0.25 } {
                failed = Some("planted_height");
            } else if height > 0.02 && failed.is_none() {
                failed = Some("contact_goal");
            }
            let [x, y] = p
                .physical_position(p.panel_at(d, t))
                .expect("validated panel");
            let margin =
                p.tile_half_mm() - (g.tips[d][0] + 16. - x).abs().max((g.tips[d][1] - y).abs());
            report.minimum_tile_margin_mm = report.minimum_tile_margin_mm.min(margin);
            if margin < 1. {
                failed = Some("tile_margin");
            }
        }
    }
    // Each fingertip's height is its digit's lowest patch point (NaN skipped),
    // so this is exactly "any patch below the floor guard".
    if g.tips.iter().any(|v| v[2] < PREPARED_FLOOR) {
        failed = Some("floor_penetration");
    }
    if g.knuckles
        .iter()
        .any(|v| *v < if p.double_mat() { 40.01 } else { 40. })
    {
        failed = Some("knuckle_height");
    }
    failed
}

// A small vertical error need not choose a new IK branch. Root height is a
// rigid translation: find one offset that is safe at the failed sample and its
// replacement anchor, then certify both with the ordinary contact guards.
fn repair_height(m: &Model, p: &Plan, samples: [(f64, Parameters); 2]) -> Option<f64> {
    let mut lo = -0.5_f64;
    let mut hi = 0.5_f64;
    let mut min = f64::INFINITY;
    let mut max = f64::NEG_INFINITY;
    for (t, q) in &samples {
        let g = m.evaluate_into(q, &mut Kin::default());
        lo = lo.max(m.lo[10] - q[10]);
        hi = hi.min(m.hi[10] - q[10]);
        for d in 0..2 {
            // Keep reserve inside the existing 0.25 mm contact/floor limits.
            lo = lo.max(PREPARED_FLOOR + 0.05 - g.tips[d][2]);
            lo = lo.max(40.05 - g.knuckles[d]);
            if p.targets(*t).contact[d] {
                min = min.min(g.tips[d][2]);
                max = max.max(g.tips[d][2]);
                lo = lo.max(-0.20 - g.tips[d][2]);
                hi = hi.min(0.20 - g.tips[d][2]);
            }
        }
    }
    if lo > hi {
        return None;
    }
    let offset = (if min.is_finite() {
        -(min + max) * 0.5
    } else {
        0.
    })
    .clamp(lo, hi);
    if offset.abs() < 1e-9 {
        return None;
    }
    for (t, mut q) in samples {
        q[10] += offset;
        if !matches!(
            inspect(
                m,
                p,
                t,
                &q,
                &mut fresh_report(0, 0, 0),
                &mut Kin::default(),
                &mut None,
                false
            ),
            None | Some("contact_goal")
        ) {
            return None;
        }
    }
    Some(offset)
}

impl ContactCurve {
    /// Build from a fixed prepared snapshot: knots, exact phases and eight
    /// interior samples per interval are checked. Contact failures trigger bounded
    /// preparation-time refinement, never a query-time fallback to a jumping fit.
    pub fn prepare(
        m: &Model,
        p: &Plan,
        times: &[f64],
        poses: &[Parameters],
    ) -> Result<Self, ContactCurveError> {
        Self::prepare_relaxed(m, p, times, poses, &[])
    }

    /// `prepare`, with contact checks and refinement left out in `relaxed`
    /// (stream) time ranges.
    pub fn prepare_relaxed(
        m: &Model,
        p: &Plan,
        times: &[f64],
        poses: &[Parameters],
        relaxed: &[[f64; 2]],
    ) -> Result<Self, ContactCurveError> {
        let mut job = ContactCurvePreparation::new(m, p, times, poses, relaxed)?;
        while !job.step(m, p) {}
        job.finish().expect("completed curve preparation")
    }

    pub fn time_range(&self) -> [f64; 2] {
        [self.curve.knots[0].t, self.curve.knots.last().unwrap().t]
    }

    pub fn knot_times(&self) -> impl Iterator<Item = f64> + '_ {
        self.curve.knots.iter().map(|k| k.t)
    }

    /// Numerical left/right endpoint agreement of the analytic quintics: pose,
    /// velocity and acceleration. This measures joins, not a speed limit.
    pub fn join_errors(&self) -> [f64; 3] {
        let mut error = [0_f64; 3];
        for w in self.curve.knots.windows(3) {
            let a = crate::curve::quintic(&w[0], &w[1], w[1].t);
            let b = crate::curve::quintic(&w[1], &w[2], w[1].t);
            for (j, (a, b)) in [(a.p, b.p), (a.v, b.v), (a.a, b.a)].into_iter().enumerate() {
                for i in 0..14 {
                    error[j] = error[j].max((a[i] - b[i]).abs());
                }
            }
        }
        error
    }

    /// Preserve the committed prefix and match position/velocity/acceleration
    /// through publication. An invalid blended contact is rejected explicitly.
    pub fn splice_checked(
        &self,
        m: &Model,
        p: &Plan,
        new: &Self,
        start: f64,
        end: f64,
        now: f64,
    ) -> Result<Self, ContactCurveError> {
        self.splice_checked_relaxed(m, p, new, start, end, now, &[])
    }

    /// `splice_checked`, with contact checks left out in `relaxed` time.
    #[allow(clippy::too_many_arguments)]
    pub fn splice_checked_relaxed(
        &self,
        m: &Model,
        p: &Plan,
        new: &Self,
        start: f64,
        end: f64,
        now: f64,
        relaxed: &[[f64; 2]],
    ) -> Result<Self, ContactCurveError> {
        let mut job = ContactCurvePublication::new(self, p, new, start, end, now, relaxed)?;
        while !job.step(m, p) {}
        job.finish().expect("completed curve publication")
    }

    /// Allocation-free C2 quintic evaluation with analytic time derivatives.
    /// Explicit full-turn geometry normalizes the world-yaw position to the
    /// model's ±180° coordinates; its derivatives follow continuous angular
    /// phase. The actual rotation remains C2 across that coordinate wrap.
    pub fn sample(&self, t: f64) -> ContactCurveSample {
        let range = self.time_range();
        let t = if t.is_finite() {
            t.clamp(range[0], range[1])
        } else {
            range[0]
        };
        let k = self.curve.at(t);
        ContactCurveSample {
            pose: self.curve.parameters(k.p).0,
            velocity: value(k.v).0,
            acceleration: value(k.a).0,
        }
    }
}

fn fresh_report(knots: usize, added: usize, pass: usize) -> ContactCurveReport {
    ContactCurveReport {
        knots,
        added_knots: added,
        refinement_passes: pass,
        checked_samples: 0,
        maximum_error_mm: 0.,
        maximum_planted_height_mm: 0.,
        minimum_tile_margin_mm: f64::INFINITY,
        contact_goal_met: false,
    }
}

type Failure = (f64, Parameters, &'static str);

/// Cooperative snapshot preparation. One step checks one observation, refines
/// one interval (at most eight contact iterations), or builds a bounded curve.
/// Partial curves are never exposed. The blocking API drives this same job.
pub struct ContactCurvePreparation {
    times: Vec<f64>,
    poses: Vec<Parameters>,
    original: usize,
    pass: usize,
    relaxed: Vec<[f64; 2]>,
    curve: Option<Curve>,
    report: ContactCurveReport,
    queries: Vec<f64>,
    at: usize,
    selected: Vec<Option<Failure>>,
    hard: Option<(f64, &'static str)>,
    failures: Vec<Failure>,
    refining: bool,
    flat: std::collections::BTreeSet<u64>,
    kin: Kin,
    cache: Option<InspectionCache>,
    result: Option<Result<ContactCurve, ContactCurveError>>,
}
impl ContactCurvePreparation {
    pub(crate) fn with_cache(mut self, cache: Option<InspectionCache>) -> Self {
        self.cache = cache;
        self
    }
    pub(crate) fn take_cache(&mut self) -> Option<InspectionCache> {
        self.cache.take()
    }

    pub fn new(
        m: &Model,
        p: &Plan,
        times: &[f64],
        poses: &[Parameters],
        relaxed: &[[f64; 2]],
    ) -> Result<Self, ContactCurveError> {
        if times.len() != poses.len()
            || times.len() < 2
            || times.len() > MAX_KNOTS
            || times.iter().any(|t| !t.is_finite())
            || times.windows(2).any(|w| w[1] < w[0])
            || times.first().is_some_and(|t| *t < 0.)
            || times.last().is_some_and(|t| *t > p.duration)
            || poses.iter().any(|q| {
                q.iter()
                    .enumerate()
                    .any(|(i, v)| !v.is_finite() || *v < m.lo[i] || *v > m.hi[i])
            })
        {
            return Err(ContactCurveError {
                reason: "invalid_samples",
                time: 0.,
            });
        }
        // Numerical curve knots may share a near-coincident boundary. The Plan
        // retains every original phase/hit timestamp and validates each below.
        let mut clean_times: Vec<f64> = Vec::new();
        let mut clean_poses = Vec::new();
        for (&time, &pose) in times.iter().zip(poses) {
            if clean_times
                .last()
                .is_some_and(|last| time - last < MIN_INTERVAL)
            {
                let mandatory = |t: f64| {
                    p.edges.binary_search_by(|v| v.total_cmp(&t)).is_ok()
                        || p.events.iter().any(|e| e.time == t)
                };
                if *clean_times.last().unwrap() != times[0]
                    && (mandatory(time)
                        || time == *times.last().unwrap()
                        || !mandatory(*clean_times.last().unwrap()))
                {
                    *clean_times.last_mut().unwrap() = time;
                    *clean_poses.last_mut().unwrap() = pose;
                }
            } else {
                clean_times.push(time);
                clean_poses.push(pose);
            }
        }
        if clean_times.len() < 2 {
            return Err(ContactCurveError {
                reason: "invalid_samples",
                time: 0.,
            });
        }

        let original = clean_times.len();
        Ok(Self {
            times: clean_times,
            poses: clean_poses,
            original,
            pass: 0,
            relaxed: relaxed.to_vec(),
            curve: None,
            report: fresh_report(original, 0, 0),
            queries: Vec::new(),
            at: 0,
            selected: Vec::new(),
            hard: None,
            failures: Vec::new(),
            refining: false,
            flat: Default::default(),
            kin: Kin::default(),
            cache: None,
            result: None,
        })
    }
    pub fn step(&mut self, m: &Model, p: &Plan) -> bool {
        if self.result.is_some() {
            return true;
        }
        if self.curve.is_none() {
            self.curve = Some(Curve::from_guide(
                &Guide {
                    times: self.times.clone(),
                    poses: self.poses.clone(),
                    centers: vec![[0.; 2]; self.times.len()],
                },
                p.periodic_yaw(m),
            ));
            // Near a phase edge there is no room for another knot. Prevent
            // a fast lift's tangents from leaking into planted time. Adjacent
            // quintics share these derivatives, preserving C2 continuity.
            for knot in &mut self.curve.as_mut().unwrap().knots {
                if self.flat.contains(&knot.t.to_bits()) {
                    knot.v = [0.; 16];
                    knot.a = [0.; 16];
                }
            }
            self.report = fresh_report(
                self.times.len(),
                self.times.len() - self.original,
                self.pass,
            );
            self.queries = self.times.clone();
            for w in self.times.windows(2) {
                for j in 1..=8 {
                    self.queries.push(w[0] + (w[1] - w[0]) * j as f64 / 9.);
                }
            }
            self.queries.extend(
                p.edges
                    .iter()
                    .copied()
                    .chain(p.events.iter().map(|e| e.time))
                    .filter(|t| *t >= self.times[0] && *t <= *self.times.last().unwrap()),
            );
            self.queries
                .extend(p.mine_probes(self.times[0], *self.times.last().unwrap()));
            self.queries
                .extend(p.release_probes(self.times[0], *self.times.last().unwrap()));
            self.queries
                .extend(p.roll_probes(self.times[0], *self.times.last().unwrap()));
            self.queries.sort_by(f64::total_cmp);
            self.queries.dedup();
            self.selected = vec![None; self.times.len() - 1];
            self.hard = None;
            self.at = 0;
            self.refining = false;
            return false;
        }
        if !self.refining && self.at < self.queries.len() {
            let t = self.queries[self.at];
            self.at += 1;
            let q = self
                .curve
                .as_ref()
                .unwrap()
                .parameters(self.curve.as_ref().unwrap().at(t).p)
                .0;
            if let Some(reason) = inspect(
                m,
                p,
                t,
                &q,
                &mut self.report,
                &mut self.kin,
                &mut self.cache,
                relaxed_at(&self.relaxed, t),
            ) {
                if reason != "contact_goal" && self.hard.is_none() {
                    self.hard = Some((t, reason));
                }
                let i = self
                    .times
                    .partition_point(|v| *v <= t)
                    .saturating_sub(1)
                    .min(self.times.len() - 2);
                let replace = self.selected[i].as_ref().is_none_or(|old| {
                    if (reason != "contact_goal") != (old.2 != "contact_goal") {
                        return reason != "contact_goal";
                    }
                    let anchor = |v: f64| {
                        (v - self.times[i]).abs() < 1e-6 || (v - self.times[i + 1]).abs() < 1e-6
                    };
                    if anchor(t) != anchor(old.0) {
                        return anchor(t);
                    }
                    let mid = (self.times[i] + self.times[i + 1]) * 0.5;
                    (t - mid).abs() < (old.0 - mid).abs()
                });
                if replace {
                    self.selected[i] = Some((t, q, reason));
                }
            }
            return false;
        }
        if !self.refining {
            self.failures = self.selected.drain(..).flatten().collect();
            self.failures.sort_by_key(|f| f.2 == "contact_goal");
            self.failures.truncate(256);
            if self.failures.is_empty() {
                self.report.contact_goal_met = true;
                self.complete();
                return true;
            }
            if (self.pass >= 1 && self.hard.is_none())
                || self.pass
                    == if p.double_mat() {
                        DOUBLE_MAX_PASSES
                    } else {
                        MAX_PASSES
                    }
                || self.times.len() + self.failures.len() > MAX_KNOTS
            {
                if let Some((time, reason)) = self.hard {
                    self.result = Some(Err(ContactCurveError { reason, time }));
                } else {
                    self.complete();
                }
                return true;
            }
            self.refining = true;
            self.at = 0;
            return false;
        }
        if self.at < self.failures.len() {
            let (mut t, mut q, reason) = self.failures[self.at];
            let failed = (t, q);
            let failed_clearance = if matches!(reason, "floor_penetration" | "knuckle_height") {
                let g = m.evaluate_into(&q, &mut self.kin);
                [
                    g.tips[0][2].min(g.tips[1][2]),
                    g.knuckles[0].min(g.knuckles[1]),
                ]
            } else {
                [f64::INFINITY; 2]
            };
            self.at += 1;
            let at = self.times.partition_point(|v| *v < t);
            let min_interval = if p.double_mat() && self.pass >= MAX_PASSES {
                MIN_INTERVAL * 0.25
            } else {
                MIN_INTERVAL
            };
            let nearby = [at.checked_sub(1), (at < self.times.len()).then_some(at)]
                .into_iter()
                .flatten()
                .filter(|i| (self.times[*i] - t).abs() < min_interval)
                .min_by(|a, b| {
                    (self.times[*a] - t)
                        .abs()
                        .total_cmp(&(self.times[*b] - t).abs())
                });
            if nearby.is_some()
                && (reason == "planted_height" || (p.double_mat() && reason != "contact_goal"))
            {
                let interval = self
                    .times
                    .partition_point(|v| *v <= t)
                    .saturating_sub(1)
                    .min(self.times.len() - 2);
                self.flat.insert(self.times[interval].to_bits());
                self.flat.insert(self.times[interval + 1].to_bits());
            }
            if let Some(i) = nearby {
                let protected = i == 0
                    || i + 1 == self.times.len()
                    || p.edges
                        .iter()
                        .chain(p.events.iter().map(|e| &e.time))
                        .any(|v| (*v - self.times[i]).abs() < 1e-8);
                if p.double_mat()
                    && reason != "contact_goal"
                    && !protected
                    && t - self.times[i - 1] >= min_interval
                    && self.times[i + 1] - t >= min_interval
                {
                    if self.flat.remove(&self.times[i].to_bits()) {
                        self.flat.insert(t.to_bits());
                    }
                    // Relocate only a numerical knot. Keep endpoints, every
                    // scored/phase timestamp, spacing and the knot cap intact.
                    self.times[i] = t;
                } else {
                    t = self.times[i];
                    // A failing double-mat interpolant is a better warm start than
                    // an unchanged, already-fitted anchor. Retarget it at the
                    // protected timestamp rather than moving a contact edge.
                    if !p.double_mat() || reason == "contact_goal" {
                        q = self.poses[i];
                    }
                }
            }
            let g = m.evaluate_into(&q, &mut self.kin);
            let prior = q;
            let height = if (reason == "floor_penetration"
                || (p.double_mat() && matches!(reason, "planted_height" | "knuckle_height")))
                && (!p.double_mat() || self.pass == 0)
                && !relaxed_at(&self.relaxed, failed.0)
                && !relaxed_at(&self.relaxed, t)
            {
                repair_height(m, p, [failed, (t, q)])
            } else {
                None
            };
            if let Some(offset) = height {
                q[10] += offset;
            } else if matches!(reason, "floor_penetration" | "knuckle_height")
                && relaxed_at(&self.relaxed, t)
            {
                // Stream time deliberately relaxes precise targets. A rigid
                // lift fixes floor/knuckle clearance while keeping the streamed digit
                // controls; forcing an exact two-tip refit can fail needlessly.
                q[10] += (0.02 - failed_clearance[0].min(g.tips[0][2]).min(g.tips[1][2]))
                    .max(40.05 - failed_clearance[1].min(g.knuckles[0]).min(g.knuckles[1]))
                    .max(0.);
                m.clamp(&mut q);
            } else {
                let target = p.targets(t);
                let mut center = [g.com[0], g.com[1]];
                if p.double_mat() && reason != "contact_goal" && self.pass > 0 {
                    // Balance is a preference, not a contact constraint. Nudge
                    // stalled fits toward support; low knuckles can use more.
                    let low_knuckle = (41. - g.knuckles[0].min(g.knuckles[1])).clamp(0., 1.);
                    let weight =
                        0.025 + ((self.pass as f64 * 0.025).min(0.1) - 0.025) * low_knuckle;
                    for j in 0..2 {
                        center[j] +=
                            weight * ((target.tips[0][j] + target.tips[1][j]) * 0.5 - g.com[j]);
                    }
                }
                refine_contacts(
                    m,
                    &mut q,
                    &prior,
                    &Goal {
                        tips: target.tips,
                        com: center,
                    },
                    8,
                );
                if relaxed_at(&self.relaxed, t) {
                    // Repairing another constraint (for example a knuckle or
                    // a mine) must not undo floor clearance at this anchor.
                    let g = m.evaluate_into(&q, &mut self.kin);
                    q[10] += (0.02 - g.tips[0][2].min(g.tips[1][2])).max(0.);
                    m.clamp(&mut q);
                }
            }
            if let Some(i) = nearby {
                self.poses[i] = q;
            } else {
                self.times.insert(at, t);
                self.poses.insert(at, q);
            }
            return false;
        }
        self.pass += 1;
        self.curve = None;
        false
    }
    fn complete(&mut self) {
        self.result = Some(Ok(ContactCurve {
            curve: self.curve.take().unwrap(),
            report: self.report.clone(),
        }));
    }
    pub fn finish(self) -> Option<Result<ContactCurve, ContactCurveError>> {
        self.result
    }
}

/// Checked C2 publication, yielding between geometry checks. A pending result
/// never replaces the committed prefix until every check has succeeded.
pub struct ContactCurvePublication {
    curve: Curve,
    report: ContactCurveReport,
    queries: Vec<f64>,
    at: usize,
    relaxed: Vec<[f64; 2]>,
    kin: Kin,
    cache: Option<InspectionCache>,
    failure: Option<ContactCurveError>,
}
impl ContactCurvePublication {
    pub(crate) fn with_cache(mut self, cache: Option<InspectionCache>) -> Self {
        self.cache = cache;
        self
    }
    pub(crate) fn take_cache(&mut self) -> Option<InspectionCache> {
        self.cache.take()
    }

    #[allow(clippy::too_many_arguments)]
    pub fn new(
        old: &ContactCurve,
        p: &Plan,
        new: &ContactCurve,
        start: f64,
        end: f64,
        now: f64,
        relaxed: &[[f64; 2]],
    ) -> Result<Self, ContactCurveError> {
        Self::with_grid(old, p, new, start, end, now, relaxed, true)
    }
    // Runtime snapshots already have a dense common 120 Hz grid. Adding a
    // second 60 Hz grid can create sub-millisecond intervals beside its knots.
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn with_grid(
        old: &ContactCurve,
        p: &Plan,
        new: &ContactCurve,
        start: f64,
        end: f64,
        now: f64,
        relaxed: &[[f64; 2]],
        extra_grid: bool,
    ) -> Result<Self, ContactCurveError> {
        if old.curve.periodic_yaw != new.curve.periodic_yaw {
            return Err(ContactCurveError {
                reason: "geometry_changed",
                time: now,
            });
        }
        if ![start, end, now].iter().all(|t| t.is_finite())
            || start < now + 0.25
            || end <= start
            || start < old.time_range()[0]
            || end > new.time_range()[1]
            || start < new.time_range()[0]
        {
            return Err(ContactCurveError {
                reason: "invalid_splice",
                time: start,
            });
        }
        let curve = old
            .curve
            .splice_grid(&new.curve, start, end, now, extra_grid);
        if curve.knots.len() > MAX_KNOTS {
            return Err(ContactCurveError {
                reason: "knot_budget",
                time: start,
            });
        }
        if curve.knots.windows(2).any(|w| {
            w[1].t - w[0].t
                < if p.double_mat() {
                    MIN_INTERVAL * 0.25 - 1e-9
                } else {
                    MIN_INTERVAL - 1e-9
                }
        }) {
            return Err(ContactCurveError {
                reason: "splice_spacing",
                time: start,
            });
        }

        let mut queries = curve.knots.iter().map(|k| k.t).collect::<Vec<_>>();
        for w in curve.knots.windows(2) {
            for i in 1..=8 {
                queries.push(w[0].t + (w[1].t - w[0].t) * i as f64 / 9.);
            }
        }
        let min = curve.knots[0].t;
        let max = curve.knots.last().unwrap().t;
        queries.extend(
            p.edges
                .iter()
                .copied()
                .chain(p.events.iter().map(|e| e.time))
                .filter(|t| *t >= min && *t <= max),
        );
        queries.extend(p.mine_probes(min, max));
        queries.extend(p.release_probes(min, max));
        queries.extend(p.roll_probes(min, max));
        if !extra_grid {
            // Shared knots outside the blend are cloned verbatim from the
            // already-checked curves. Only the new blend needs new geometry
            // checks; rescanning the entire prefix/tail can miss a deadline.
            queries.retain(|t| *t >= start && *t <= end);
        }
        let mut report = fresh_report(curve.knots.len(), 0, 0);
        report.contact_goal_met = if extra_grid {
            true
        } else {
            old.report.contact_goal_met && new.report.contact_goal_met
        };
        Ok(Self {
            curve,
            report,
            queries,
            at: 0,
            relaxed: relaxed.to_vec(),
            kin: Kin::default(),
            cache: None,
            failure: None,
        })
    }
    pub fn step(&mut self, m: &Model, p: &Plan) -> bool {
        if self.failure.is_some() || self.at == self.queries.len() {
            return true;
        }
        let t = self.queries[self.at];
        self.at += 1;
        let q = self.curve.parameters(self.curve.at(t).p).0;
        if let Some(reason) = inspect(
            m,
            p,
            t,
            &q,
            &mut self.report,
            &mut self.kin,
            &mut self.cache,
            relaxed_at(&self.relaxed, t),
        ) {
            if reason == "contact_goal" {
                self.report.contact_goal_met = false;
            } else {
                self.failure = Some(ContactCurveError { reason, time: t });
                return true;
            }
        }
        self.at == self.queries.len()
    }
    pub fn finish(self) -> Option<Result<ContactCurve, ContactCurveError>> {
        if let Some(e) = self.failure {
            Some(Err(e))
        } else if self.at == self.queries.len() {
            Some(Ok(ContactCurve {
                curve: self.curve,
                report: self.report,
            }))
        } else {
            None
        }
    }
}

#[cfg(test)]
mod periodic_tests {
    use super::*;
    #[test]
    fn samples_wrap_positions_but_keep_the_true_angular_derivatives() {
        let mut a = [0.; 14];
        a[13] = 170.;
        let mut b = a;
        b[13] = -170.;
        let curve = ContactCurve {
            curve: Curve::from_guide(
                &Guide {
                    times: vec![0., 0.02],
                    poses: vec![a, b],
                    centers: vec![[0.; 2]; 2],
                },
                true,
            ),
            report: fresh_report(2, 0, 0),
        };
        let s = curve.sample(0.01);
        assert!((-180. ..=180.).contains(&s.pose[13]));
        assert!(s.pose[13].abs() > 179.);
        assert!(s.velocity[13] > 180.);
        assert!((s.velocity[13] - 1875.).abs() < 1e-6);
        assert!(s.acceleration[13].abs() < 1e-6);
    }
}
