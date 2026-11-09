use crate::kinematics::{Eval, Kin};
use crate::{Model, Parameters, Point};
#[derive(Clone, Copy)]
pub struct Goal {
    pub tips: [Point; 2],
    pub com: [f64; 2],
}
#[derive(Clone, Copy, Debug, Default)]
pub struct SolveReport {
    pub iterations: usize,
    pub evaluations: usize,
    pub target_error_mm: f64,
    pub objective: f64,
    /// The rolling evaluator relaxed its balance preference to preserve contact.
    pub balance_relaxed: bool,
    /// Stream weight of a rolling evaluation (0 for exact motion).
    pub stream_weight: f64,
    /// Stream motion was not ready; the exact evaluation was returned instead.
    pub stream_missing: bool,
}
const TIP_WEIGHTS: [f64; 3] = [80., 80., 120.];
const PRIOR_WEIGHTS: Parameters = [
    0.30, 0.35, 0.45, 0.55, 0.30, 0.35, 0.45, 0.55, 0.13, 0.13, 0.22, 0.35, 0.35, 0.45,
];
pub(crate) fn residual(g: &Eval, q: &Parameters, prior: &Parameters, goal: &Goal) -> [f64; 24] {
    let mut r = [0.; 24];
    for d in 0..2 {
        for j in 0..3 {
            r[d * 3 + j] = (g.tips[d][j] - goal.tips[d][j]) * TIP_WEIGHTS[j];
        }
    }
    for i in 0..14 {
        r[6 + i] = (q[i] - prior[i]) * PRIOR_WEIGHTS[i];
    }
    for i in 0..2 {
        r[20 + i] = (42.5 - g.knuckles[i]).max(0.) * 12.;
        r[22 + i] = (g.com[i] - goal.com[i]) * 35.;
    }
    r
}
/// The residual's Jacobian from analytic derivatives at the pose evaluated
/// into `kin` (`g`): what the forward differences approximate, without their
/// fourteen evaluations. A knuckle penalty counts where it is active.
pub(crate) fn jacobian(
    model: &Model,
    q: &Parameters,
    kin: &Kin,
    g: &Eval,
) -> Option<[[f64; 14]; 24]> {
    let d = model.derivatives(q, kin, g)?;
    let mut j = [[0.; 14]; 24];
    for c in 0..14 {
        for t in 0..2 {
            for k in 0..3 {
                j[t * 3 + k][c] = d.tips[t][c][k] * TIP_WEIGHTS[k];
            }
        }
        j[6 + c][c] = PRIOR_WEIGHTS[c];
        for i in 0..2 {
            if 42.5 - g.knuckles[i] > 0. {
                j[20 + i][c] = -12. * d.knuckles[i][c];
            }
            j[22 + i][c] = d.com[c][i] * 35.;
        }
    }
    Some(j)
}
pub(crate) fn energy(r: &[f64; 24]) -> f64 {
    r.iter().map(|v| v * v).sum()
}
pub(crate) fn linear(a: [[f64; 14]; 14], b: [f64; 14]) -> [f64; 14] {
    let mut l = [[0.; 14]; 14];
    for i in 0..14 {
        for j in 0..=i {
            let v = a[i][j] - (0..j).map(|k| l[i][k] * l[j][k]).sum::<f64>();
            l[i][j] = if i == j {
                v.max(1e-14).sqrt()
            } else {
                v / l[j][j]
            };
        }
    }
    let mut x = [0.; 14];
    for i in 0..14 {
        x[i] = (b[i] - (0..i).map(|j| l[i][j] * x[j]).sum::<f64>()) / l[i][i];
    }
    for i in (0..14).rev() {
        x[i] = (x[i] - (i + 1..14).map(|j| l[j][i] * x[j]).sum::<f64>()) / l[i][i];
    }
    x
}
/// When a fit stops: every fingertip residual, knuckle penalty and balance
/// residual below these (weighted residual units), or a step changing no
/// control by `change` or more.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct Stop {
    pub(crate) tips: f64,
    pub(crate) knuckle: f64,
    pub(crate) com: f64,
    pub(crate) change: f64,
}
/// The strict policy every fit has used.
pub(crate) const STRICT: Stop = Stop {
    tips: 0.0008,
    knuckle: 0.001,
    com: 0.0035,
    change: 1e-5,
};
impl Stop {
    /// Thresholds `scale` times looser (1 is `STRICT`).
    pub(crate) fn scaled(scale: f64) -> Self {
        if scale == 1. {
            return STRICT;
        }
        Self {
            tips: STRICT.tips * scale,
            knuckle: STRICT.knuckle * scale,
            com: STRICT.com * scale,
            change: STRICT.change * scale,
        }
    }
    pub(crate) fn met(&self, r: &[f64; 24]) -> bool {
        r[..6].iter().all(|x| x.abs() < self.tips)
            && r[20..22].iter().all(|x| *x < self.knuckle)
            && r[22..].iter().all(|x| x.abs() < self.com)
    }
}
/// Fixed iteration/evaluation caps; no heap allocations or worker dispatch.
pub fn solve(
    model: &Model,
    q: &mut Parameters,
    prior: &Parameters,
    goal: &Goal,
    cap: usize,
) -> SolveReport {
    solve_impl(model, q, prior, goal, cap, false, false, &STRICT)
}
/// Opt-in contact-refinement study: compute a step that respects bounds before
/// line search, including limits reached partway through that step. This retains
/// the ordinary objective and authored bounds. It does not change the default fit.
/// Jacobian columns reuse unchanged geometry, which is bit-for-bit identical.
pub fn refine_contacts(
    model: &Model,
    q: &mut Parameters,
    prior: &Parameters,
    goal: &Goal,
    cap: usize,
) -> SolveReport {
    solve_impl(model, q, prior, goal, cap.min(8), true, true, &STRICT)
}

fn feasible_direction(
    a: [[f64; 14]; 14],
    b: Parameters,
    q: &Parameters,
    lo: &Parameters,
    hi: &Parameters,
) -> Parameters {
    let mut delta = linear(a, b);
    let mut fixed = [None; 14];
    for _ in 0..14 {
        let mut changed = false;
        for c in 0..14 {
            if fixed[c].is_none() {
                let lower = lo[c] + 1e-6 - q[c];
                let upper = hi[c] - 1e-6 - q[c];
                if delta[c] < lower || delta[c] > upper {
                    fixed[c] = Some(delta[c].clamp(lower, upper));
                    changed = true;
                }
            }
        }
        if !changed {
            break;
        }
        let mut aa = a;
        let mut bb = b;
        for k in 0..14 {
            for c in 0..14 {
                if let Some(offset) = fixed[c] {
                    bb[k] -= a[k][c] * offset;
                }
            }
        }
        for c in 0..14 {
            if let Some(offset) = fixed[c] {
                for k in 0..14 {
                    aa[c][k] = 0.;
                    aa[k][c] = 0.;
                }
                aa[c][c] = 1.;
                bb[c] = offset;
            }
        }
        delta = linear(aa, bb);
    }
    delta
}
/// Preparation reuses fingertip geometry unaffected by a Jacobian perturbation.
/// Objectives, convergence precision and iteration limits match `solve` exactly.
pub(crate) fn solve_guide(
    model: &Model,
    q: &mut Parameters,
    prior: &Parameters,
    goal: &Goal,
    cap: usize,
) -> SolveReport {
    solve_impl(model, q, prior, goal, cap, true, false, &STRICT)
}
/// A guide fit under the preparation's stopping policy, with or without
/// geometry reuse (`Efficient60` or `Reference60`).
pub(crate) fn solve_with(
    model: &Model,
    q: &mut Parameters,
    prior: &Parameters,
    goal: &Goal,
    cap: usize,
    reuse_geometry: bool,
    stop: &Stop,
) -> SolveReport {
    solve_impl(model, q, prior, goal, cap, reuse_geometry, false, stop)
}
/// Evaluation counts follow the fitting policy: a pose whose geometry is
/// already known (the accepted line-search candidate, or the final pose) is
/// still counted once where the original loop evaluated it, so reports are
/// unchanged; only the repeated arithmetic is skipped.
#[allow(clippy::too_many_arguments)]
fn solve_impl(
    model: &Model,
    q: &mut Parameters,
    prior: &Parameters,
    goal: &Goal,
    cap: usize,
    reuse_geometry: bool,
    feasible_bounds: bool,
    stop: &Stop,
) -> SolveReport {
    model.clamp(q);
    let mut kin = [Kin::default(), Kin::default()];
    // Forward differences' perturbed kinematics, made only where they run.
    let mut scratch: Option<Kin> = None;
    let mut base = 0;
    // Geometry and residual of the current `q`, with its kinematics in `kin[base]`.
    let mut current: Option<(Eval, [f64; 24])> = None;
    let mut out = SolveReport::default();
    for iteration in 0..cap.min(40) {
        let (g, r) = current.unwrap_or_else(|| {
            let g = model.evaluate_into(q, &mut kin[base]);
            (g, residual(&g, q, prior, goal))
        });
        current = Some((g, r));
        out.evaluations += 1;
        if stop.met(&r) {
            break;
        }
        // Efficient fits take the analytic Jacobian (counted as one
        // evaluation); the reference keeps forward differences.
        let analytic = if reuse_geometry {
            jacobian(model, q, &kin[base], &g)
        } else {
            None
        };
        let j = if let Some(j) = analytic {
            out.evaluations += 1;
            j
        } else {
            let mut j = [[0.; 14]; 24];
            let scratch = scratch.get_or_insert_with(Kin::default);
            for c in 0..14 {
                let mut trial = *q;
                trial[c] += 0.002;
                let trial_geometry = if reuse_geometry {
                    model.evaluate_perturbed(&trial, c, &g, &kin[base], scratch)
                } else {
                    model.evaluate_into(&trial, scratch)
                };
                let rr = residual(&trial_geometry, &trial, prior, goal);
                out.evaluations += 1;
                for row in 0..24 {
                    j[row][c] = (rr[row] - r[row]) / 0.002;
                }
            }
            j
        };
        let mut a = [[0.; 14]; 14];
        let mut b = [0.; 14];
        for c in 0..14 {
            for k in 0..14 {
                a[c][k] = (0..24).map(|row| j[row][c] * j[row][k]).sum::<f64>()
                    + if c == k { 0.001 } else { 0. };
            }
            b[c] = -(0..24).map(|row| j[row][c] * r[row]).sum::<f64>();
        }
        let mut delta = if feasible_bounds {
            feasible_direction(a, b, q, &model.lo, &model.hi)
        } else {
            linear(a, b)
        };
        // Solve again with outward-pointing bound variables fixed. Merely clamping
        // the unconstrained step can leave the remaining controls unable to improve.
        let mut locked = [false; 14];
        for _ in 0..if feasible_bounds { 0 } else { 14 } {
            let mut changed = false;
            for c in 0..14 {
                if !locked[c]
                    && ((q[c] <= model.lo[c] + 1e-4 && delta[c] < 0.)
                        || (q[c] >= model.hi[c] - 1e-4 && delta[c] > 0.))
                {
                    locked[c] = true;
                    changed = true;
                }
            }
            if !changed {
                break;
            }
            let mut aa = a;
            let mut bb = b;
            for c in 0..14 {
                if locked[c] {
                    for k in 0..14 {
                        aa[c][k] = 0.;
                        aa[k][c] = 0.;
                    }
                    aa[c][c] = 1.;
                    bb[c] = 0.;
                }
            }
            delta = linear(aa, bb);
        }
        let maximum = delta.iter().map(|v| v.abs()).fold(0., f64::max);
        let scale = (5. / maximum.max(1e-8)).min(1.);
        for v in &mut delta {
            *v *= scale;
        }
        let before = energy(&r);
        let mut accepted = false;
        let mut change = 0_f64;
        for _ in 0..6 {
            let mut candidate = std::array::from_fn(|i| q[i] + delta[i]);
            model.clamp(&mut candidate);
            let cg = model.evaluate_into(&candidate, &mut kin[1 - base]);
            let cr = residual(&cg, &candidate, prior, goal);
            let after = energy(&cr);
            out.evaluations += 1;
            if after <= before {
                change = (0..14)
                    .map(|i| (candidate[i] - q[i]).abs())
                    .fold(0., f64::max);
                *q = candidate;
                current = Some((cg, cr));
                base = 1 - base;
                accepted = true;
                break;
            }
            for v in &mut delta {
                *v *= 0.5;
            }
        }
        out.iterations = iteration + 1;
        if !accepted || change < stop.change {
            break;
        }
    }
    let (g, r) = current.unwrap_or_else(|| {
        let g = model.evaluate_into(q, &mut kin[base]);
        (g, residual(&g, q, prior, goal))
    });
    out.target_error_mm = max_tip_error(&g.tips, &goal.tips);
    out.objective = energy(&r);
    out
}
/// A fingertip's distance from its target, millimeters.
pub(crate) fn tip_error(tip: &Point, target: &Point) -> f64 {
    (0..3)
        .map(|i| (tip[i] - target[i]).powi(2))
        .sum::<f64>()
        .sqrt()
}
/// The larger of the two fingertips' [`tip_error`]s.
pub(crate) fn max_tip_error(tips: &[Point; 2], targets: &[Point; 2]) -> f64 {
    (0..2)
        .map(|d| tip_error(&tips[d], &targets[d]))
        .fold(0., f64::max)
}

#[cfg(test)]
mod bound_tests {
    use super::*;

    #[test]
    fn near_limit_step_redistributes_the_nonzero_bound_offset() {
        let mut a = [[0.; 14]; 14];
        for (i, row) in a.iter_mut().enumerate() {
            row[i] = 1.;
        }
        a[0][0] = 2.;
        a[1][1] = 2.;
        a[0][1] = 1.;
        a[1][0] = 1.;
        let mut b = [0.; 14];
        b[0] = 3.;
        let q = [0.; 14];
        let lo = [-10.; 14];
        let mut hi = [10.; 14];
        hi[0] = 0.100001;
        let d = feasible_direction(a, b, &q, &lo, &hi);
        assert!((d[0] - 0.1).abs() < 1e-12);
        assert!((d[1] + 0.05).abs() < 1e-12);
        // Clamping the unconstrained direction gives (0.1,-1), which loses
        // the free variable's optimal response to the actual bounded step.
        let energy = |x: Parameters| {
            (0..14)
                .map(|i| 0.5 * x[i] * (0..14).map(|j| a[i][j] * x[j]).sum::<f64>() - b[i] * x[i])
                .sum::<f64>()
        };
        let mut clamped = linear(a, b);
        clamped[0] = 0.1;
        assert!(energy(d) < energy(clamped));
    }

    #[test]
    fn bound_feasible_direction_keeps_every_control_in_its_authored_interval() {
        let mut a = [[0.; 14]; 14];
        let mut b = [0.; 14];
        let lo = [-1.; 14];
        let hi = [1.; 14];
        let q = std::array::from_fn(|i| if i % 2 == 0 { 0.999 } else { -0.999 });
        for i in 0..14 {
            a[i][i] = 2.;
            b[i] = if i % 2 == 0 { 5. } else { -5. };
        }
        let d = feasible_direction(a, b, &q, &lo, &hi);
        for i in 0..14 {
            assert!(q[i] + d[i] >= lo[i] + 1e-6 - 1e-12 && q[i] + d[i] <= hi[i] - 1e-6 + 1e-12);
        }
    }
}
