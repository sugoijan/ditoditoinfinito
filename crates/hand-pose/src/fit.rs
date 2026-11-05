//! Resumable version of the strict fit. Yielding never changes arithmetic order.
use crate::kinematics::{Eval, Kin};
use crate::solve::{STRICT, Stop, energy, jacobian, linear, residual};
use crate::{Goal, Model, Parameters, SolveReport};

#[derive(Clone, Copy)]
enum Phase {
    Residual,
    Jacobian,
    Column(usize),
    Row(usize),
    Direction,
    Bounds(usize),
    Search(usize),
    Finish,
    Done,
}

/// A single-threaded fit with checkpoints inside each iteration. No clock, heap
/// allocation or partial result publication. Reuse it with [`Self::reset`].
pub struct PoseFit {
    q: Parameters,
    prior: Parameters,
    goal: Goal,
    cap: usize,
    iteration: usize,
    phase: Phase,
    report: SolveReport,
    reuse: bool,
    stop: Stop,
    /// Geometry and residual of `q` (kinematics in `kin[base]`) when `known`.
    geometry: Eval,
    r: [f64; 24],
    known: bool,
    kin: [Kin; 2],
    base: usize,
    scratch: Kin,
    j: [[f64; 14]; 24],
    a: [[f64; 14]; 14],
    b: Parameters,
    delta: Parameters,
    locked: [bool; 14],
    before: f64,
}

impl PoseFit {
    pub fn new(m: &Model, q: Parameters, prior: Parameters, goal: Goal, cap: usize) -> Self {
        let mut fit = Self {
            q,
            prior,
            goal,
            cap: 0,
            iteration: 0,
            phase: Phase::Done,
            report: SolveReport::default(),
            reuse: false,
            stop: STRICT,
            geometry: Eval {
                tips: [[0.; 3]; 2],
                com: [0.; 3],
                knuckles: [0.; 2],
            },
            r: [0.; 24],
            known: false,
            kin: [Kin::default(), Kin::default()],
            base: 0,
            scratch: Kin::default(),
            j: [[0.; 14]; 24],
            a: [[0.; 14]; 14],
            b: [0.; 14],
            delta: [0.; 14],
            locked: [false; 14],
            before: 0.,
        };
        fit.reset(m, q, prior, goal, cap);
        fit
    }

    pub fn reset(&mut self, m: &Model, q: Parameters, prior: Parameters, goal: Goal, cap: usize) {
        self.reset_with_reuse(m, q, prior, goal, cap, true);
    }

    pub(crate) fn reset_with_reuse(
        &mut self,
        m: &Model,
        q: Parameters,
        prior: Parameters,
        goal: Goal,
        cap: usize,
        reuse: bool,
    ) {
        self.reset_with(m, q, prior, goal, cap, reuse, STRICT);
    }

    #[allow(clippy::too_many_arguments)]
    pub(crate) fn reset_with(
        &mut self,
        m: &Model,
        mut q: Parameters,
        prior: Parameters,
        goal: Goal,
        cap: usize,
        reuse: bool,
        stop: Stop,
    ) {
        self.stop = stop;
        m.clamp(&mut q);
        self.q = q;
        self.prior = prior;
        self.goal = goal;
        self.cap = cap.min(40);
        self.iteration = 0;
        self.report = SolveReport::default();
        self.reuse = reuse;
        self.known = false;
        self.phase = if self.cap == 0 {
            Phase::Finish
        } else {
            Phase::Residual
        };
    }

    /// Perform one bounded checkpoint: at most one geometry evaluation, one
    /// normal-matrix row or one 14-variable linear solve. Returns true when done.
    pub fn advance(&mut self, m: &Model) -> bool {
        match self.phase {
            Phase::Residual => {
                // An accepted candidate's evaluation is this pose's; it still
                // counts as the policy's evaluation (see `solve`).
                if !self.known {
                    self.geometry = m.evaluate_into(&self.q, &mut self.kin[self.base]);
                    self.r = residual(&self.geometry, &self.q, &self.prior, &self.goal);
                    self.known = true;
                }
                self.report.evaluations += 1;
                self.phase = if self.stop.met(&self.r) {
                    Phase::Finish
                } else if self.reuse {
                    Phase::Jacobian
                } else {
                    Phase::Column(0)
                };
            }
            Phase::Jacobian => {
                // As `solve`: analytic where geometry is reused (a skin that
                // allows it), else forward differences one column at a time.
                self.phase =
                    if let Some(j) = jacobian(m, &self.q, &self.kin[self.base], &self.geometry) {
                        self.j = j;
                        self.report.evaluations += 1;
                        Phase::Row(0)
                    } else {
                        Phase::Column(0)
                    };
            }
            Phase::Column(c) => {
                let mut trial = self.q;
                trial[c] += 0.002;
                let g = if self.reuse {
                    m.evaluate_perturbed(
                        &trial,
                        c,
                        &self.geometry,
                        &self.kin[self.base],
                        &mut self.scratch,
                    )
                } else {
                    m.evaluate_into(&trial, &mut self.scratch)
                };
                let rr = residual(&g, &trial, &self.prior, &self.goal);
                self.report.evaluations += 1;
                for row in 0..24 {
                    self.j[row][c] = (rr[row] - self.r[row]) / 0.002;
                }
                self.phase = if c == 13 {
                    Phase::Row(0)
                } else {
                    Phase::Column(c + 1)
                };
            }
            Phase::Row(c) => {
                for k in 0..14 {
                    self.a[c][k] = (0..24)
                        .map(|row| self.j[row][c] * self.j[row][k])
                        .sum::<f64>()
                        + if c == k { 0.001 } else { 0. };
                }
                self.b[c] = -(0..24).map(|row| self.j[row][c] * self.r[row]).sum::<f64>();
                self.phase = if c == 13 {
                    Phase::Direction
                } else {
                    Phase::Row(c + 1)
                };
            }
            Phase::Direction => {
                self.delta = linear(self.a, self.b);
                self.locked.fill(false);
                self.phase = Phase::Bounds(0);
            }
            Phase::Bounds(pass) => {
                let mut changed = false;
                for c in 0..14 {
                    if !self.locked[c]
                        && ((self.q[c] <= m.lo[c] + 1e-4 && self.delta[c] < 0.)
                            || (self.q[c] >= m.hi[c] - 1e-4 && self.delta[c] > 0.))
                    {
                        self.locked[c] = true;
                        changed = true;
                    }
                }
                if changed {
                    let mut a = self.a;
                    let mut b = self.b;
                    for c in 0..14 {
                        if self.locked[c] {
                            for k in 0..14 {
                                a[c][k] = 0.;
                                a[k][c] = 0.;
                            }
                            a[c][c] = 1.;
                            b[c] = 0.;
                        }
                    }
                    self.delta = linear(a, b);
                }
                if changed && pass < 13 {
                    self.phase = Phase::Bounds(pass + 1);
                } else {
                    let maximum = self.delta.iter().map(|v| v.abs()).fold(0., f64::max);
                    let scale = (5. / maximum.max(1e-8)).min(1.);
                    for v in &mut self.delta {
                        *v *= scale;
                    }
                    self.before = energy(&self.r);
                    self.phase = Phase::Search(0);
                }
            }
            Phase::Search(attempt) => {
                let mut candidate = std::array::from_fn(|i| self.q[i] + self.delta[i]);
                m.clamp(&mut candidate);
                let next = 1 - self.base;
                let cg = m.evaluate_into(&candidate, &mut self.kin[next]);
                let cr = residual(&cg, &candidate, &self.prior, &self.goal);
                let after = energy(&cr);
                self.report.evaluations += 1;
                if after <= self.before {
                    let change = (0..14)
                        .map(|i| (candidate[i] - self.q[i]).abs())
                        .fold(0., f64::max);
                    self.q = candidate;
                    self.geometry = cg;
                    self.r = cr;
                    self.base = next;
                    self.report.iterations = self.iteration + 1;
                    self.iteration += 1;
                    self.phase = if change < self.stop.change || self.iteration == self.cap {
                        Phase::Finish
                    } else {
                        Phase::Residual
                    };
                } else {
                    for v in &mut self.delta {
                        *v *= 0.5;
                    }
                    if attempt == 5 {
                        self.report.iterations = self.iteration + 1;
                        self.phase = Phase::Finish;
                    } else {
                        self.phase = Phase::Search(attempt + 1);
                    }
                }
            }
            Phase::Finish => {
                if !self.known {
                    self.geometry = m.evaluate_into(&self.q, &mut self.kin[self.base]);
                    self.r = residual(&self.geometry, &self.q, &self.prior, &self.goal);
                    self.known = true;
                }
                let g = &self.geometry;
                self.report.target_error_mm = (0..2)
                    .map(|d| {
                        (0..3)
                            .map(|i| (g.tips[d][i] - self.goal.tips[d][i]).powi(2))
                            .sum::<f64>()
                            .sqrt()
                    })
                    .fold(0., f64::max);
                self.report.objective = energy(&self.r);
                self.phase = Phase::Done;
            }
            Phase::Done => {}
        }
        matches!(self.phase, Phase::Done)
    }

    /// A result is available only after the complete original fitting policy.
    pub fn result(&self) -> Option<(Parameters, SolveReport)> {
        matches!(self.phase, Phase::Done).then_some((self.q, self.report))
    }
}
