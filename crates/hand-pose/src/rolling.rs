//! Cooperative, single-threaded rolling preparation. Only changed future motion
//! is rebuilt. Immutable committed motion is retained through a C2 splice.
use crate::curve::{Curve, value};
use crate::motion::{polygon, project};
use crate::stream::{Stream, StreamOptions};
use crate::{Goal, Guide, Model, Parameters, Plan, PoseFit, SolveReport, solve};
/// Preparation evaluation policy; both retain the original 60 Hz guide grid.
#[derive(Clone, Copy, Debug, Default)]
pub enum PreparationMode {
    Reference60,
    #[default]
    Efficient60,
}
/// Rolling preparation policy. `Default` is the accepted policy, approved
/// after side-by-side review on 2026-11-02 (`docs/plans/hand-timeout-next.md`):
/// one support/refit pass, guide-only stopping thirty times looser than the
/// strict fit, and no job for a request that changes nothing; and, approved on
/// 2026-11-03 (`docs/plans/hand-stream-mode.md`), stream mode in dense
/// passages. `PREVIOUS` keeps the policy before 2026-11-02 for comparisons and
/// `exact()` any policy without stream mode. Other values are experiments that
/// change the generated motion; each needs the motion gates and a review.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct RollingOptions {
    /// Seconds re-prepared before the first changed target.
    pub lead: f64,
    /// Support/refit passes after the seed fits (at least one).
    pub passes: usize,
    /// Guide fits' stopping thresholds, this many times the strict ones. The
    /// sampled contact fit and curve refinement keep the strict policy.
    pub guide_tolerance: f64,
    /// Start no job for a request that changes no target and no duration.
    pub skip_unchanged: bool,
    /// Stream mode for dense passages (`docs/plans/hand-stream-mode.md`);
    /// `None` prepares exact motion throughout.
    pub stream: Option<StreamOptions>,
}
impl RollingOptions {
    /// The same policy without stream mode: exact preparation throughout.
    pub const fn exact(self) -> Self {
        Self {
            stream: None,
            ..self
        }
    }
    /// The policy until 2026-11-02: two passes, strict guide fits, a job for
    /// every request.
    pub const PREVIOUS: Self = Self {
        lead: 0.65,
        passes: 2,
        guide_tolerance: 1.,
        skip_unchanged: false,
        stream: None,
    };
}
impl Default for RollingOptions {
    fn default() -> Self {
        Self {
            lead: 0.65,
            passes: 1,
            guide_tolerance: 30.,
            skip_unchanged: true,
            stream: Some(StreamOptions::default()),
        }
    }
}
struct Support {
    polygons: Vec<Vec<[f64; 2]>>,
    actual: Vec<[f64; 2]>,
    softness: Vec<f64>,
    z: Vec<[f64; 2]>,
    u: Vec<[f64; 2]>,
    x: Vec<[f64; 2]>,
    l0: Vec<f64>,
    l1: Vec<f64>,
    l2: Vec<f64>,
    iterations: usize,
    done: bool,
}
const SUPPORT_RHO: f64 = 30.;
const BALANCE_WEIGHT: f64 = 30.;
const BODY_SMOOTH_SECONDS: f64 = 0.16;
impl Support {
    fn new(times: &[f64], polygons: Vec<Vec<[f64; 2]>>, actual: Vec<[f64; 2]>) -> Self {
        let n = times.len();
        let lam = BODY_SMOOTH_SECONDS.powi(4);
        let mut diag = vec![1. + SUPPORT_RHO; n];
        let mut off1 = vec![0.; n];
        let mut off2 = vec![0.; n];
        for i in 0..n - 2 {
            let dl = times[i + 1] - times[i];
            let dr = times[i + 2] - times[i + 1];
            let avg = (dl + dr) * 0.5;
            let a = 1. / (dl * avg);
            let c = 1. / (dr * avg);
            let b = -a - c;
            diag[i] += lam * a * a;
            diag[i + 1] += lam * b * b;
            diag[i + 2] += lam * c * c;
            off1[i + 1] += lam * a * b;
            off1[i + 2] += lam * b * c;
            off2[i + 2] += lam * a * c;
        }
        let mut l0 = vec![0_f64; n];
        let mut l1 = vec![0_f64; n];
        let mut l2 = vec![0_f64; n];
        for i in 0..n {
            if i > 1 {
                l2[i] = off2[i] / l0[i - 2];
            }
            if i > 0 {
                l1[i] = (off1[i] - l2[i] * if i > 1 { l1[i - 1] } else { 0. }) / l0[i - 1];
            }
            l0[i] = (diag[i] - l1[i].powi(2) - l2[i].powi(2)).sqrt();
        }
        let z: Vec<_> = actual
            .iter()
            .zip(&polygons)
            .map(|(&p, h)| project(h, p))
            .collect();
        // Only open a relaxed balance window where the instantaneous support
        // target would demand an abrupt body transfer. Ordinary support remains
        // a hard preference. The envelope ends 360 ms after the last trigger.
        let mut changes = Vec::new();
        for i in 1..n {
            let speed =
                (z[i][0] - z[i - 1][0]).hypot(z[i][1] - z[i - 1][1]) / (times[i] - times[i - 1]);
            let u = ((speed - 400.) / 400.).clamp(0., 1.);
            if u > 0. {
                changes.push(((times[i] + times[i - 1]) * 0.5, u * u * (3. - 2. * u)));
            }
        }
        let softness = times
            .iter()
            .map(|&t| {
                changes
                    .iter()
                    .map(|&(at, weight)| {
                        let dt = (t - at).abs();
                        if dt >= 0.36 {
                            0.
                        } else {
                            // Compact C2 window, with no state that can remain stuck after
                            // the problematic passage has left the rolling horizon.
                            let u = dt / 0.36;
                            weight * (1. - u * u * u * (10. + u * (-15. + 6. * u)))
                        }
                    })
                    .fold(0., f64::max)
            })
            .collect();
        Self {
            polygons,
            actual,
            softness,
            z,
            u: vec![[0.; 2]; n],
            x: vec![[0.; 2]; n],
            l0,
            l1,
            l2,
            iterations: 0,
            done: false,
        }
    }
    fn step(&mut self, iterations: usize) {
        let n = self.z.len();
        for _ in 0..iterations {
            for i in 0..n {
                for c in 0..2 {
                    self.x[i][c] = (self.actual[i][c]
                        + SUPPORT_RHO * (self.z[i][c] - self.u[i][c])
                        - if i > 0 {
                            self.l1[i] * self.x[i - 1][c]
                        } else {
                            0.
                        }
                        - if i > 1 {
                            self.l2[i] * self.x[i - 2][c]
                        } else {
                            0.
                        })
                        / self.l0[i];
                }
            }
            for i in (0..n).rev() {
                for c in 0..2 {
                    self.x[i][c] = (self.x[i][c]
                        - if i + 1 < n {
                            self.l1[i + 1] * self.x[i + 1][c]
                        } else {
                            0.
                        }
                        - if i + 2 < n {
                            self.l2[i + 2] * self.x[i + 2][c]
                        } else {
                            0.
                        })
                        / self.l0[i];
                }
            }
            // Converged when every knot's primal and dual residual is below
            // 0.001 (`hypot`, NaN ignored as `f64::max` ignores it), decided
            // from squared norms unless one lies within rounding of a limit.
            let mut primal = true;
            let mut dual = true;
            for i in 0..n {
                let old = self.z[i];
                let desired = [self.x[i][0] + self.u[i][0], self.x[i][1] + self.u[i][1]];
                let supported = project(&self.polygons[i], desired);
                // A short single-support phase cannot require an instantaneous
                // body transfer. Balance is a soft preference; the smoothness
                // term can carry the body between supports during rapid steps.
                let soft_weight = BALANCE_WEIGHT / (BALANCE_WEIGHT + SUPPORT_RHO);
                let weight = 1. - self.softness[i] * (1. - soft_weight);
                self.z[i] =
                    std::array::from_fn(|c| desired[c] + weight * (supported[c] - desired[c]));
                for c in 0..2 {
                    self.u[i][c] += self.x[i][c] - self.z[i][c];
                }
                primal =
                    primal && below(self.x[i][0] - self.z[i][0], self.x[i][1] - self.z[i][1], 1.);
                dual = dual && below(old[0] - self.z[i][0], old[1] - self.z[i][1], SUPPORT_RHO);
            }
            self.iterations += 1;
            if (primal && dual) || self.iterations >= 3000 {
                self.done = true;
                break;
            }
        }
    }
}
/// `scale · hypot(a, b) < 0.001`, or NaN, bit for bit: from `a² + b²` where
/// it is clear by far more than its rounding.
#[inline(always)]
fn below(a: f64, b: f64, scale: f64) -> bool {
    let limit = (0.001 / scale) * (0.001 / scale);
    let square = a * a + b * b;
    if square < limit * (1. - 1e-9) {
        true
    } else if square > limit * (1. + 1e-9) {
        false
    } else {
        let norm = scale * a.hypot(b);
        norm.is_nan() || norm < 0.001
    }
}
#[derive(Clone, Copy)]
enum FitKind {
    Seed { com: [f64; 2], retry: bool },
    Refit,
}
#[derive(Clone, Copy, PartialEq)]
enum Tick {
    Pending,
    Unit,
    Finished,
}
struct Job {
    plan: Plan,
    old: Curve,
    times: Vec<f64>,
    q: Vec<Parameters>,
    centers: Vec<[f64; 2]>,
    filtered: Vec<Parameters>,
    polygons: Vec<Vec<[f64; 2]>>,
    actual: Vec<[f64; 2]>,
    support: Option<Support>,
    stage: u8,
    index: usize,
    pass: usize,
    start: f64,
    balance_relaxation_mm: f64,
    preparation: PreparationMode,
    options: RollingOptions,
    stop: crate::solve::Stop,
    fit: Option<Box<PoseFit>>,
    fitting: Option<FitKind>,
    support_ticks: usize,
}
impl Job {
    fn with_options(
        plan: Plan,
        old: Curve,
        start: f64,
        end: f64,
        preparation: PreparationMode,
        options: RollingOptions,
    ) -> Self {
        let mut mandatory = vec![start, end];
        for tr in [&plan.tracks.left, &plan.tracks.right] {
            for s in tr {
                for t in [s.release, s.land, (s.release + s.land) / 2.] {
                    if t > start && t < end {
                        mandatory.push(t);
                    }
                }
            }
        }
        mandatory.sort_by(f64::total_cmp);
        // This is a numerical guide, not the contact timetable. Near-coincident
        // phase markers share a guide knot; Plan retains every exact hit time.
        mandatory.dedup_by(|a, b| (*a - *b).abs() < 0.001);
        if let Some(last) = mandatory.last_mut() {
            *last = end;
        }
        let mut times = mandatory.clone();
        let n = ((end - start) * 60.).ceil() as usize;
        for i in 1..n {
            let t = start + (end - start) * i as f64 / n as f64;
            if mandatory.iter().all(|v| (v - t).abs() > 1. / 120.) {
                times.push(t);
            }
        }
        times.sort_by(f64::total_cmp);
        times.dedup();
        let len = times.len();
        Self {
            plan,
            old,
            times,
            q: vec![[0.; 14]; len],
            centers: vec![[0.; 2]; len],
            filtered: vec![[0.; 14]; len],
            polygons: Vec::with_capacity(len),
            actual: Vec::with_capacity(len),
            support: None,
            stage: 0,
            index: 0,
            pass: 0,
            start,
            balance_relaxation_mm: 0.,
            preparation,
            options,
            stop: crate::solve::Stop::scaled(options.guide_tolerance),
            fit: None,
            fitting: None,
            support_ticks: 0,
        }
    }
    fn start_fit(&mut self, m: &Model, q: Parameters, goal: Goal, cap: usize) {
        let fit = self
            .fit
            .get_or_insert_with(|| Box::new(PoseFit::new(m, q, q, goal, cap)));
        fit.reset_with(
            m,
            q,
            q,
            goal,
            cap,
            matches!(self.preparation, PreparationMode::Efficient60),
            self.stop,
        );
    }
    fn unit(&mut self, m: &Model) -> bool {
        // Preserve the accepted blocking route as an independent numerical
        // reference and for offline consumers that do not need to yield.
        if self.fitting.is_none() && matches!(self.stage, 0 | 4) {
            let reuse = matches!(self.preparation, PreparationMode::Efficient60);
            let stop = self.stop;
            let fit = |m: &Model, q: &mut Parameters, prior: &Parameters, goal: &Goal, cap| {
                crate::solve::solve_with(m, q, prior, goal, cap, reuse, &stop)
            };
            let (mut q, goal) = if self.stage == 0 {
                self.seed_goal(m)
            } else {
                (
                    self.q[self.index],
                    Goal {
                        tips: self.plan.targets(self.times[self.index]).tips,
                        com: self.centers[self.index],
                    },
                )
            };
            let prior = q;
            let report = fit(m, &mut q, &prior, &goal, 20);
            if self.stage == 0 && report.target_error_mm > 0.02 {
                let previous = self.index.checked_sub(1).map(|j| self.q[j]);
                q = self.plan.seed(m, self.times[self.index], previous.as_ref());
                let prior = q;
                fit(m, &mut q, &prior, &goal, 24);
            }
            return self.finish_fit(q) == Tick::Finished;
        }
        loop {
            match self.tick(m) {
                Tick::Pending => {}
                Tick::Unit => return false,
                Tick::Finished => return true,
            }
        }
    }
    fn seed_goal(&self, m: &Model) -> (Parameters, Goal) {
        let i = self.index;
        let previous = i.checked_sub(1).map(|j| self.q[j]);
        let t = self.times[i];
        let target = self.plan.targets(t);
        let (mut q, old_com) = value(self.old.at(t).p);
        let err = m
            .geometry(&q)
            .tips
            .iter()
            .zip(target.tips)
            .map(|(a, b)| (0..3).map(|k| (a[k] - b[k]).powi(2)).sum::<f64>())
            .fold(0., f64::max);
        if err > 36. {
            q = self.plan.seed(m, t, previous.as_ref());
        }
        let u = self.plan.support_u(t);
        let com = if err < 0.01 {
            old_com
        } else {
            std::array::from_fn(|j| target.tips[0][j] * (1. - u) + target.tips[1][j] * u)
        };
        (
            q,
            Goal {
                tips: target.tips,
                com: self.plan.centered_balance(target.tips, com),
            },
        )
    }
    fn finish_fit(&mut self, q: Parameters) -> Tick {
        self.q[self.index] = q;
        self.fitting = None;
        self.index += 1;
        if self.index == self.times.len() {
            self.index = 0;
            if self.stage == 0 {
                self.stage = 1;
            } else {
                self.pass += 1;
                if self.pass >= self.options.passes {
                    return Tick::Finished;
                }
                self.stage = 2;
            }
        }
        Tick::Unit
    }
    fn tick(&mut self, m: &Model) -> Tick {
        let n = self.times.len();
        let i = self.index;
        match self.stage {
            0 => {
                if let Some(FitKind::Seed { com, retry }) = self.fitting {
                    let fit = self.fit.as_mut().unwrap();
                    if !fit.advance(m) {
                        return Tick::Pending;
                    }
                    let (q, report) = fit.result().unwrap();
                    if !retry && report.target_error_mm > 0.02 {
                        let previous = i.checked_sub(1).map(|j| self.q[j]);
                        let q = self.plan.seed(m, self.times[i], previous.as_ref());
                        let goal = Goal {
                            tips: self.plan.targets(self.times[i]).tips,
                            com,
                        };
                        self.start_fit(m, q, goal, 24);
                        self.fitting = Some(FitKind::Seed { com, retry: true });
                        return Tick::Pending;
                    }
                    return self.finish_fit(q);
                }
                let (q, goal) = self.seed_goal(m);
                let com = goal.com;
                self.start_fit(m, q, goal, 20);
                self.fitting = Some(FitKind::Seed { com, retry: false });
                return Tick::Pending;
            }
            1 => {
                let t = self.times[i];
                let mut sum = 0.;
                let mut q = [0.; 14];
                for j in 0..n {
                    let dt = self.times[j] - t;
                    if dt.abs() > 0.16 {
                        continue;
                    }
                    let density = if j == 0 {
                        self.times[1] - self.times[0]
                    } else if j + 1 == n {
                        self.times[n - 1] - self.times[n - 2]
                    } else {
                        (self.times[j + 1] - self.times[j - 1]) / 2.
                    };
                    let w = (-0.5 * (dt / 0.04).powi(2)).exp() * density;
                    sum += w;
                    for c in 0..14 {
                        q[c] += w * self.q[j][c];
                    }
                }
                self.filtered[i] = q.map(|v| v / sum);
                self.index += 1;
                if self.index == n {
                    self.q.copy_from_slice(&self.filtered);
                    self.stage = 2;
                    self.index = 0;
                }
            }
            2 => {
                let g = m.geometry(&self.q[i]);
                self.polygons.push(polygon(&self.plan, self.times[i], &g));
                self.actual.push([g.com[0], g.com[1]]);
                self.index += 1;
                if self.index == n {
                    self.support = Some(Support::new(
                        &self.times,
                        std::mem::take(&mut self.polygons),
                        std::mem::take(&mut self.actual),
                    ));
                    self.stage = 3;
                    self.index = 0;
                }
            }
            3 => {
                let s = self.support.as_mut().unwrap();
                s.step(1);
                self.support_ticks += 1;
                if !s.done && self.support_ticks < 16 {
                    return Tick::Pending;
                }
                self.support_ticks = 0;
                if s.done {
                    for (center, polygon) in s.z.iter().zip(&s.polygons) {
                        let supported = project(polygon, *center);
                        self.balance_relaxation_mm = self
                            .balance_relaxation_mm
                            .max((center[0] - supported[0]).hypot(center[1] - supported[1]));
                    }
                    for (i, center) in s.z.iter().enumerate() {
                        self.centers[i] = self
                            .plan
                            .centered_balance(self.plan.targets(self.times[i]).tips, *center);
                    }
                    self.support = None;
                    self.stage = 4;
                    self.index = 0;
                }
            }
            4 => {
                if self.fitting.is_none() {
                    let goal = Goal {
                        tips: self.plan.targets(self.times[i]).tips,
                        com: self.centers[i],
                    };
                    self.start_fit(m, self.q[i], goal, 20);
                    self.fitting = Some(FitKind::Refit);
                    return Tick::Pending;
                }
                let fit = self.fit.as_mut().unwrap();
                if !fit.advance(m) {
                    return Tick::Pending;
                }
                let q = fit.result().unwrap().0;
                return self.finish_fit(q);
            }
            _ => unreachable!(),
        }
        Tick::Unit
    }
    fn finish(self) -> Curve {
        Curve::from_guide(&Guide {
            times: self.times,
            poses: self.q,
            centers: self.centers,
        })
    }
}
#[derive(Clone, Debug, Default)]
pub struct WindowReport {
    pub published: usize,
    pub units: usize,
    /// Small checkpoints executed through `work_slice`.
    pub checkpoints: u64,
    pub guide_knots: usize,
    pub peak_knots: usize,
    pub busy: bool,
    pub late_jobs: usize,
    pub minimum_planning_lead: f64,
    pub covered_until: f64,
    pub last_splice: [f64; 2],
    pub maximum_commit_pose_delta: f64,
    pub maximum_commit_velocity_delta: f64,
    pub maximum_commit_acceleration_delta: f64,
    /// Desired balance target outside the instantaneous support hull. This is
    /// intentional during rapid transfers, not a dynamic-stability certificate.
    pub maximum_balance_relaxation_mm: f64,
    /// Stream mode: knots fitted, sections started, jobs not started because
    /// only stream time changed, and swings longer than its flight bound.
    pub stream_knots: usize,
    pub stream_sections: usize,
    pub stream_skipped_jobs: usize,
    pub stream_long_flights: usize,
}
pub struct Rolling {
    curve: Curve,
    published_plan: Plan,
    pending: Option<(Plan, f64)>,
    job: Option<Job>,
    pub report: WindowReport,
    pub horizon: f64,
    // Realtime callers leave preparation time before the protected prefix
    // reaches a new guide. Offline/reference scheduling keeps zero reserve.
    pub(crate) preparation_reserve: f64,
    preparation: PreparationMode,
    options: RollingOptions,
    stream: Option<Stream>,
}
impl Rolling {
    pub fn new(m: &Model, empty: Plan, horizon: f64) -> Self {
        Self::with_preparation(m, empty, horizon, PreparationMode::default())
    }
    pub fn with_preparation(
        m: &Model,
        empty: Plan,
        horizon: f64,
        preparation: PreparationMode,
    ) -> Self {
        Self::with_options(m, empty, horizon, preparation, RollingOptions::default())
    }
    pub fn with_options(
        m: &Model,
        empty: Plan,
        horizon: f64,
        preparation: PreparationMode,
        options: RollingOptions,
    ) -> Self {
        let options = RollingOptions {
            passes: options.passes.max(1),
            ..options
        };
        let mut q = empty.seed(m, 0., None);
        let target = empty.targets(0.);
        let goal = Goal {
            tips: target.tips,
            com: std::array::from_fn(|j| (target.tips[0][j] + target.tips[1][j]) / 2.),
        };
        let prior = q;
        solve(m, &mut q, &prior, &goal, 32);
        Self {
            curve: Curve::constant(q, goal.com),
            published_plan: empty,
            pending: None,
            job: None,
            report: WindowReport::default(),
            horizon,
            preparation_reserve: 0.,
            preparation,
            options,
            stream: options
                .stream
                .map(|s| Stream::new(s, horizon, options.guide_tolerance)),
        }
    }
    /// A plan's duration is the chart's end: once it is visible, every note
    /// is known.
    pub fn request(&mut self, plan: Plan, now: f64) {
        let known = now + self.horizon;
        let finished = plan.duration <= known;
        self.request_known(plan, now, known, finished);
    }
    /// Received contact context can end before the nominal lookahead. A growing
    /// window is not an end-of-chart signal to the stream motion fitter.
    pub fn request_known(&mut self, plan: Plan, now: f64, known_until: f64, finished: bool) {
        // Keep a short settling tail, but never let its extra duration reveal notes.
        let visible_until = (now + self.horizon).min(known_until);
        let tail_end = plan.duration.min(visible_until + 0.5);
        let mut visible = plan.visible(visible_until.min(plan.duration));
        visible.duration = tail_end;
        visible.rebuild_edges();
        if let Some(stream) = self.stream.as_mut() {
            stream.observe(&visible, visible_until, finished);
            self.report.stream_sections = stream.sections_started;
            self.report.stream_long_flights = stream.long_flights;
        }
        self.pending = Some((visible, now));
    }
    fn begin(&mut self, now: f64) {
        if self.job.is_some() {
            return;
        }
        let Some((plan, _)) = self.pending.take() else {
            return;
        };
        let end = plan.duration;
        if end <= now + 0.85 {
            return;
        }
        let mut change = end;
        for t in std::iter::once(now + 0.25)
            .chain(plan.edges.iter().copied())
            .chain(self.published_plan.edges.iter().copied())
        {
            if t < now + 0.25 || t > end {
                continue;
            }
            let a = plan.targets(t + 1e-6);
            let b = self.published_plan.targets(t + 1e-6);
            if a.contact != b.contact
                || a.tips
                    .iter()
                    .flatten()
                    .zip(b.tips.iter().flatten())
                    .any(|(a, b)| (a - b).abs() > 1e-7)
            {
                change = change.min(t);
            }
        }
        if self.options.skip_unchanged && change == end && end == self.published_plan.duration {
            // Nothing visible changed and the published motion already reaches
            // the end: re-preparing its tail would only start a late job.
            return;
        }
        let mut start = (change - self.options.lead)
            .max(now + 0.25 + self.preparation_reserve)
            .min(end - 0.25);
        // Targets are compared 1 µs after each candidate, so the change lies
        // there: one found 1 µs before full stream time (a section's blend in
        // ends at a swing's release) is in it.
        if let Some(until) = self
            .stream
            .as_ref()
            .and_then(|s| s.full_through(change + 1e-6))
        {
            // Exact motion is not shown in stream time. Prepare it only where
            // it is, from 0.6 s (the splice) before the blend out begins;
            // `published_plan` keeps what the exact motion was prepared for.
            if until >= end {
                self.report.stream_skipped_jobs += 1;
                return;
            }
            start = start.max(until - 0.6).min(end - 0.25);
        }
        self.job = Some(Job::with_options(
            plan,
            self.curve.clone(),
            start,
            end,
            self.preparation,
            self.options,
        ));
    }
    /// Instrumentation belongs to the caller; the kernel never reads a clock.
    #[cfg(feature = "profiling")]
    pub fn preparation_stage(&self) -> &'static str {
        if self.stream.as_ref().is_some_and(|s| s.pending()) {
            return "stream";
        }
        match self.job.as_ref().map(|j| j.stage) {
            None if self.pending.is_some() => "begin",
            None => "idle",
            Some(0) => "seed_fit",
            Some(1) => "smooth",
            Some(2) => "support_geometry",
            Some(3) => "balance",
            Some(4) => "contact_refit",
            _ => unreachable!(),
        }
    }
    pub fn work(&mut self, m: &Model, now: f64, units: usize) {
        self.begin(now);
        // Stream knots first: each is one fit, final once made.
        let stream = self.stream.as_mut().map_or(0, |s| s.advance(m, now, units));
        self.report.stream_knots += stream;
        for _ in stream..units {
            let Some(job) = self.job.as_mut() else { break };
            self.report.units += 1;
            let stage = job.stage;
            let batch = if stage == 1 {
                32
            } else if stage == 2 {
                8
            } else {
                1
            };
            let mut finished = false;
            for _ in 0..batch {
                if job.unit(m) {
                    finished = true;
                    break;
                }
                if job.stage != stage {
                    break;
                }
            }
            if finished {
                self.publish(now);
                self.begin(now);
            }
        }
        self.report.busy = self.job.is_some() || self.pending.is_some();
        self.report.guide_knots = self.curve.knots.len();
        self.report.peak_knots = self.report.peak_knots.max(self.report.guide_knots);
    }
    fn publish(&mut self, now: f64) {
        let job = self.job.take().unwrap();
        self.report.maximum_balance_relaxation_mm = self
            .report
            .maximum_balance_relaxation_mm
            .max(job.balance_relaxation_mm);
        if job.start < now + 0.25 {
            self.report.late_jobs += 1;
        }
        let lead = job.start - now;
        if self.report.published == 0 {
            self.report.minimum_planning_lead = lead;
        } else {
            self.report.minimum_planning_lead = self.report.minimum_planning_lead.min(lead);
        }
        let plan = job.plan.clone();
        self.report.covered_until = plan.duration;
        let start = job.start.max(now + 0.25);
        let end = (start + 0.6).min(plan.duration);
        let new = job.finish();
        let before = self.curve.at(start);
        let next = self.curve.splice(&new, start, end, now);
        let after = next.at(start);
        for i in 0..16 {
            self.report.maximum_commit_pose_delta = self
                .report
                .maximum_commit_pose_delta
                .max((before.p[i] - after.p[i]).abs());
            self.report.maximum_commit_velocity_delta = self
                .report
                .maximum_commit_velocity_delta
                .max((before.v[i] - after.v[i]).abs());
            self.report.maximum_commit_acceleration_delta = self
                .report
                .maximum_commit_acceleration_delta
                .max((before.a[i] - after.a[i]).abs());
        }
        self.curve = next;
        self.published_plan = plan;
        self.report.published += 1;
        self.report.last_splice = [start, end];
    }
    /// Run at most `steps` small checkpoints, publishing only complete guides.
    /// A host can call with one checkpoint at a time and stop on its own clock;
    /// deterministic callers can use a fixed quota. The kernel reads no clock.
    pub fn work_slice(&mut self, m: &Model, now: f64, steps: usize) -> usize {
        let mut used = 0;
        for _ in 0..steps {
            self.begin(now);
            if let Some(stream) = self.stream.as_mut()
                && stream.advance(m, now, 1) == 1
            {
                self.report.stream_knots += 1;
                used += 1;
                self.report.checkpoints += 1;
                continue;
            }
            let Some(job) = self.job.as_mut() else {
                break;
            };
            let tick = job.tick(m);
            used += 1;
            self.report.checkpoints += 1;
            if tick == Tick::Finished {
                self.publish(now);
            }
        }
        self.report.busy = self.job.is_some() || self.pending.is_some();
        self.report.guide_knots = self.curve.knots.len();
        self.report.peak_knots = self.report.peak_knots.max(self.report.guide_knots);
        used
    }
    /// The output pose at `t`: exact motion, stream motion, or their blend
    /// (`(1 - w)·exact + w·stream`). Without stream time it is the exact
    /// evaluation, bit for bit.
    pub fn evaluate(&self, m: &Model, plan: &Plan, t: f64) -> (Parameters, SolveReport) {
        let Some(stream) = self.stream.as_ref() else {
            return self.evaluate_exact(m, plan, t);
        };
        let w = stream.weight(t);
        if w == 0. {
            return self.evaluate_exact(m, plan, t);
        }
        let Some((s, stream_com)) = stream.sample(t) else {
            // Stream knots should always lead playback by about two seconds;
            // the caller reports a starved sample as a failure.
            let (q, mut report) = self.evaluate_exact(m, plan, t);
            report.stream_missing = true;
            return (q, report);
        };
        let (mut q, mut report) = if w == 1. {
            (s, SolveReport::default())
        } else {
            let (e, report) = self.evaluate_exact(m, plan, t);
            let exact_com = value(self.curve.at(t).p).1;
            let blend: Parameters = std::array::from_fn(|i| e[i] + w * (s[i] - e[i]));
            // Blended controls of two different poses can move a planted
            // fingertip: fit the blend back to the targets, fading the fit out
            // towards stream motion alone so the output stays continuous.
            let mut fitted = blend;
            crate::solve::solve_guide(
                m,
                &mut fitted,
                &blend,
                &Goal {
                    tips: plan.targets(t).tips,
                    com: std::array::from_fn(|i| exact_com[i] + w * (stream_com[i] - exact_com[i])),
                },
                8,
            );
            let keep = 1. - w.powi(4);
            (
                std::array::from_fn(|i| blend[i] + keep * (fitted[i] - blend[i])),
                report,
            )
        };
        let mut g = m.geometry(&q);
        // Stream knots touch the floor exactly; between them a planted
        // fingertip can dip below it. Lift the hand rigidly (the root's
        // height) by the depth instead of fitting again.
        let low = g.tips[0][2].min(g.tips[1][2]);
        if low < 0. {
            q[10] -= low;
            m.clamp(&mut q);
            g = m.geometry(&q);
        }
        let target = plan.targets(t);
        report.target_error_mm = (0..2)
            .map(|d| {
                (0..3)
                    .map(|i| (g.tips[d][i] - target.tips[d][i]).powi(2))
                    .sum::<f64>()
                    .sqrt()
            })
            .fold(0., f64::max);
        report.stream_weight = w;
        (q, report)
    }
    /// Complete numerical inputs to evaluate + contact correction. Readiness is
    /// checked first: a cached pose must never conceal missing stream knots.
    pub(crate) fn source_key(&self, plan: &Plan, t: f64) -> Option<[u64; 39]> {
        let mut key = [0; 39];
        let weight = self.stream.as_ref().map_or(0., |s| s.weight(t));
        if weight != 1. {
            for (dst, v) in key[..16].iter_mut().zip(self.curve.at(t).p) {
                *dst = v.to_bits();
            }
        }
        for (dst, v) in key[16..22]
            .iter_mut()
            .zip(plan.targets(t).tips.into_iter().flatten())
        {
            *dst = v.to_bits();
        }
        if weight != 0. {
            let (pose, center) = self.stream.as_ref()?.sample(t)?;
            for (dst, v) in key[22..38].iter_mut().zip(pose.into_iter().chain(center)) {
                *dst = v.to_bits();
            }
        }
        key[38] = weight.to_bits();
        Some(key)
    }

    pub(crate) fn cap_stream_end(&self, end: f64) -> f64 {
        self.stream.as_ref().map_or(end, |s| s.cap_end(end))
    }

    /// Time ranges with stream weight (stream and blend time).
    pub fn stream_intervals(&self) -> Vec<[f64; 2]> {
        self.stream.as_ref().map_or(Vec::new(), |s| s.intervals())
    }
    fn evaluate_exact(&self, m: &Model, plan: &Plan, t: f64) -> (Parameters, SolveReport) {
        let (mut q, com) = value(self.curve.at(t).p);
        let prior = q;
        // The same strict fit as `solve`; perturbed columns reuse unchanged
        // geometry, which is bit-for-bit identical.
        let mut report = crate::solve::solve_guide(
            m,
            &mut q,
            &prior,
            &Goal {
                tips: plan.targets(t).tips,
                com,
            },
            8,
        );
        if report.target_error_mm > 0.02 {
            // Contact outranks the preferred balance point. Ease this preference
            // away when the local pose cannot satisfy both, rather than forcing
            // a joint/root excursion. The source contact times never change.
            let u = ((report.target_error_mm - 0.02) / 0.18).clamp(0., 1.);
            let weight = u * u * u * (10. + u * (-15. + 6. * u));
            let actual = m.geometry(&q).com;
            let goal = Goal {
                tips: plan.targets(t).tips,
                com: std::array::from_fn(|i| com[i] + weight * (actual[i] - com[i])),
            };
            let mut candidate = q;
            let mut retry = crate::solve::solve_guide(m, &mut candidate, &prior, &goal, 8);
            retry.iterations += report.iterations;
            retry.evaluations += report.evaluations;
            if retry.target_error_mm <= report.target_error_mm {
                retry.balance_relaxed = true;
                q = candidate;
                report = retry;
            } else {
                report.iterations = retry.iterations;
                report.evaluations = retry.evaluations;
            }
        }
        (q, report)
    }
    pub fn guide(&self) -> Guide {
        Guide {
            times: self.curve.knots.iter().map(|k| k.t).collect(),
            poses: self.curve.knots.iter().map(|k| value(k.p).0).collect(),
            centers: self.curve.knots.iter().map(|k| value(k.p).1).collect(),
        }
    }
}
