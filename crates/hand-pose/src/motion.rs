//! Single-threaded chart preparation and bounded pose evaluation. The prepared
//! guide is generated from the contact plan/style calibration, never017 motion.
use crate::{Geometry, Goal, Model, Parameters, Plan, solve};
use serde::{Deserialize, Serialize};
type P2 = [f64; 2];
#[derive(Clone, Serialize, Deserialize)]
pub struct Guide {
    pub times: Vec<f64>,
    pub poses: Vec<Parameters>,
    pub centers: Vec<P2>,
}
#[derive(Clone, Debug, Default)]
pub struct Preparation {
    pub seed_count: usize,
    pub solver_iterations: usize,
    pub maximum_contact_error_mm: f64,
    pub support_iterations: Vec<usize>,
}
fn sample<const N: usize>(times: &[f64], values: &[[f64; N]], t: f64) -> [f64; N] {
    let hi = times.partition_point(|v| *v <= t).min(times.len() - 1);
    let lo = if times[hi] > t {
        hi.saturating_sub(1)
    } else {
        hi
    };
    let u = if lo == hi {
        0.
    } else {
        (t - times[lo]) / (times[hi] - times[lo])
    };
    std::array::from_fn(|i| values[lo][i] + u * (values[hi][i] - values[lo][i]))
}
fn smooth(q: &[Parameters], hz: f64, sigma: f64) -> Vec<Parameters> {
    let r = (sigma * hz * 4.).ceil() as isize;
    let weights: Vec<_> = (-r..=r)
        .map(|j| (-(j as f64 / (sigma * hz)).powi(2) * 0.5).exp())
        .collect();
    let norm = weights.iter().sum::<f64>();
    (0..q.len())
        .map(|i| {
            let mut o = [0.; 14];
            for (j, w) in (-r..=r).zip(&weights) {
                let k = (i as isize + j).clamp(0, q.len() as isize - 1) as usize;
                for c in 0..14 {
                    o[c] += q[k][c] * w / norm;
                }
            }
            o
        })
        .collect()
}
fn grid(p: &Plan, hz: f64, apices: bool) -> Vec<f64> {
    let mut mandatory = p.edges.clone();
    if apices {
        for tr in [&p.tracks.left, &p.tracks.right] {
            mandatory.extend(tr.iter().map(|s| (s.release + s.land) * 0.5));
        }
    }
    let mut t = mandatory.clone();
    for i in 0..=(p.duration * hz).ceil() as usize {
        let v = (i as f64 / hz).min(p.duration);
        if mandatory.iter().all(|s| (s - v).abs() > 0.5 / hz) {
            t.push(v);
        }
    }
    t.sort_by(f64::total_cmp);
    t.dedup();
    t
}
fn cross(a: P2, b: P2, c: P2) -> f64 {
    (b[0] - a[0]) * (c[1] - a[1]) - (b[1] - a[1]) * (c[0] - a[0])
}
fn hull(mut points: Vec<P2>) -> Vec<P2> {
    points.sort_by(|a, b| a[0].total_cmp(&b[0]).then(a[1].total_cmp(&b[1])));
    points.dedup();
    if points.len() < 3 {
        return points;
    }
    let mut v = Vec::new();
    for &p in &points {
        while v.len() > 1 && cross(v[v.len() - 2], v[v.len() - 1], p) <= 0. {
            v.pop();
        }
        v.push(p);
    }
    let n = v.len();
    for &p in points.iter().rev().skip(1) {
        while v.len() > n && cross(v[v.len() - 2], v[v.len() - 1], p) <= 0. {
            v.pop();
        }
        v.push(p);
    }
    v.pop();
    v
}
pub(crate) fn project(poly: &[P2], p: P2) -> P2 {
    if poly.is_empty() {
        return p;
    }
    // Edges as (start, end) without an integer division per edge.
    let edges = || poly.iter().zip(poly.iter().skip(1).chain(&poly[..1]));
    if poly.len() >= 3 && edges().all(|(a, b)| cross(*a, *b, p) >= -1e-10) {
        return p;
    }
    // The original scans every edge and keeps the first strictly closer point.
    // An edge whose bounding box is provably no closer (with a margin for
    // rounding) than the current choice cannot replace it, so the projection
    // below is skipped for it; every other edge is computed exactly as before.
    let scale = poly
        .iter()
        .flatten()
        .chain(&p)
        .fold(1_f64, |m, v| m.max(v.abs()));
    let margin = scale * 1e-9;
    let mut best = p;
    let mut distance = f64::INFINITY;
    for (&a, &b) in edges() {
        let gap = |k: usize| {
            let outside = (a[k].min(b[k]) - p[k]).max(p[k] - a[k].max(b[k]));
            (outside - margin).max(0.)
        };
        let (gx, gy) = (gap(0), gap(1));
        if (gx * gx + gy * gy) * (1. - 1e-9) >= distance {
            continue;
        }
        let v = [b[0] - a[0], b[1] - a[1]];
        let u = (((p[0] - a[0]) * v[0] + (p[1] - a[1]) * v[1])
            / (v[0] * v[0] + v[1] * v[1]).max(1e-12))
        .clamp(0., 1.);
        let q = [a[0] + u * v[0], a[1] + u * v[1]];
        let d = (q[0] - p[0]).powi(2) + (q[1] - p[1]).powi(2);
        if d < distance {
            distance = d;
            best = q;
        }
    }
    best
}

/// The original projection, retained as the reference for `project`.
#[cfg(test)]
fn project_reference(poly: &[P2], p: P2) -> P2 {
    if poly.is_empty() {
        return p;
    }
    if poly.len() >= 3
        && (0..poly.len()).all(|i| cross(poly[i], poly[(i + 1) % poly.len()], p) >= -1e-10)
    {
        return p;
    }
    let mut best = p;
    let mut distance = f64::INFINITY;
    for i in 0..poly.len() {
        let a = poly[i];
        let b = poly[(i + 1) % poly.len()];
        let v = [b[0] - a[0], b[1] - a[1]];
        let u = (((p[0] - a[0]) * v[0] + (p[1] - a[1]) * v[1])
            / (v[0] * v[0] + v[1] * v[1]).max(1e-12))
        .clamp(0., 1.);
        let q = [a[0] + u * v[0], a[1] + u * v[1]];
        let d = (q[0] - p[0]).powi(2) + (q[1] - p[1]).powi(2);
        if d < distance {
            distance = d;
            best = q;
        }
    }
    best
}

pub(crate) fn polygon(plan: &Plan, t: f64, g: &Geometry) -> Vec<P2> {
    let mut contact = plan.targets(t).contact;
    for neighbor in [(t - 1e-7).max(0.), (t + 1e-7).min(plan.duration)] {
        let c = plan.targets(neighbor).contact;
        if c[0] ^ c[1] {
            if !contact[0] && !contact[1] {
                contact = c
            } else {
                for i in 0..2 {
                    contact[i] &= c[i];
                }
            }
        }
    }
    let mut points = Vec::new();
    for d in 0..2 {
        if contact[d] {
            for p in &g.patches[d * 144..(d + 1) * 144] {
                if p[2] <= 1.2 {
                    points.push([p[0], p[1]]);
                }
            }
        }
    }
    let poly = hull(points);
    if poly.len() < 3 {
        return poly;
    } // No support constraint in flight.
    // Use the actual patch hull; the runtime guide does not need a margin expansion.
    poly
}
fn support(model: &Model, plan: &Plan, times: &[f64], poses: &[Parameters]) -> (Vec<P2>, usize) {
    let n = times.len();
    let mut polygons = Vec::with_capacity(n);
    let mut actual = Vec::with_capacity(n);
    for (q, t) in poses.iter().zip(times) {
        let g = model.geometry(q);
        polygons.push(polygon(plan, *t, &g));
        actual.push([g.com[0], g.com[1]]);
    }
    let rho = 30.;
    let lam = (0.05_f64 * 120.).powi(4);
    let mut diag = vec![1. + rho; n];
    let mut off1 = vec![0.; n];
    let mut off2 = vec![0.; n];
    for i in 0..n - 2 {
        let dl = times[i + 1] - times[i];
        let dr = times[i + 2] - times[i + 1];
        let avg = (dl + dr) * 0.5;
        let a = (1. / 120_f64).powi(2) / (dl * avg);
        let c = (1. / 120_f64).powi(2) / (dr * avg);
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
    let mut z: Vec<_> = actual
        .iter()
        .zip(&polygons)
        .map(|(&p, h)| project(h, p))
        .collect();
    let mut u = vec![[0.; 2]; n];
    let mut x = vec![[0.; 2]; n];
    let mut steps = 0;
    for iteration in 0..6000 {
        for i in 0..n {
            for c in 0..2 {
                x[i][c] = (actual[i][c] + rho * (z[i][c] - u[i][c])
                    - if i > 0 { l1[i] * x[i - 1][c] } else { 0. }
                    - if i > 1 { l2[i] * x[i - 2][c] } else { 0. })
                    / l0[i];
            }
        }
        for i in (0..n).rev() {
            for c in 0..2 {
                x[i][c] = (x[i][c]
                    - if i + 1 < n {
                        l1[i + 1] * x[i + 1][c]
                    } else {
                        0.
                    }
                    - if i + 2 < n {
                        l2[i + 2] * x[i + 2][c]
                    } else {
                        0.
                    })
                    / l0[i];
            }
        }
        let mut primal = 0_f64;
        let mut dual = 0_f64;
        for i in 0..n {
            let old = z[i];
            z[i] = project(&polygons[i], [x[i][0] + u[i][0], x[i][1] + u[i][1]]);
            for c in 0..2 {
                u[i][c] += x[i][c] - z[i][c];
            }
            primal = primal.max((x[i][0] - z[i][0]).hypot(x[i][1] - z[i][1]));
            dual = dual.max(rho * (old[0] - z[i][0]).hypot(old[1] - z[i][1]));
        }
        steps = iteration + 1;
        if primal < 0.0002 && dual < 0.0002 {
            break;
        }
    }
    (z, steps)
}
impl Guide {
    /// Offline-at-load native prototype preparation. All work stays on this thread.
    /// The source motion is generated here; no baked017 poses or balance path are used.
    pub fn prepare(model: &Model, plan: &Plan) -> (Self, Preparation) {
        let base_times = grid(plan, 60., true);
        let mut seed = Vec::with_capacity(base_times.len());
        for &t in &base_times {
            seed.push(plan.seed(model, t, seed.last()));
        }
        let uniform: Vec<_> = (0..=(plan.duration * 60.).ceil() as usize)
            .map(|i| (i as f64 / 60.).min(plan.duration))
            .collect();
        let values: Vec<_> = uniform
            .iter()
            .map(|&t| sample(&base_times, &seed, t))
            .collect();
        let smooth_base = smooth(&values, 60., 0.025);
        let mut base: Vec<_> = base_times
            .iter()
            .map(|&t| sample(&uniform, &smooth_base, t))
            .collect();
        let mut report = Preparation {
            seed_count: seed.len(),
            ..Default::default()
        };
        for (q, &t) in base.iter_mut().zip(&base_times) {
            let target = plan.targets(t);
            let u = plan.support_u(t);
            let goal = Goal {
                tips: target.tips,
                com: std::array::from_fn(|i| target.tips[0][i] * (1. - u) + target.tips[1][i] * u),
            };
            let prior = *q;
            let r = solve(model, q, &prior, &goal, 32);
            report.solver_iterations += r.iterations;
        }
        let times = grid(plan, 120., true);
        let uniform: Vec<_> = (0..=(plan.duration * 120.).ceil() as usize)
            .map(|i| (i as f64 / 120.).min(plan.duration))
            .collect();
        let raw: Vec<_> = uniform
            .iter()
            .map(|&t| sample(&base_times, &base, t))
            .collect();
        let filtered = smooth(&raw, 120., 0.04);
        let mut poses: Vec<_> = times
            .iter()
            .map(|&t| sample(&uniform, &filtered, t))
            .collect();
        let mut centers = Vec::new();
        for _ in 0..3 {
            let (c, iterations) = support(model, plan, &times, &poses);
            centers = c;
            report.support_iterations.push(iterations);
            for (i, q) in poses.iter_mut().enumerate() {
                let prior = *q;
                let goal = Goal {
                    tips: plan.targets(times[i]).tips,
                    com: centers[i],
                };
                let r = solve(model, q, &prior, &goal, 24);
                report.solver_iterations += r.iterations;
                report.maximum_contact_error_mm =
                    report.maximum_contact_error_mm.max(r.target_error_mm);
            }
        }
        (
            Self {
                times,
                poses,
                centers,
            },
            report,
        )
    }
    pub fn initial(&self, t: f64) -> Parameters {
        sample(&self.times, &self.poses, t)
    }
    pub fn goal(&self, plan: &Plan, t: f64) -> Goal {
        Goal {
            tips: plan.targets(t).tips,
            com: sample(&self.times, &self.centers, t),
        }
    }
    pub fn evaluate(
        &self,
        model: &Model,
        plan: &Plan,
        t: f64,
        cap: usize,
    ) -> (Parameters, crate::SolveReport) {
        let mut q = self.initial(t);
        let prior = q;
        let r = solve(model, &mut q, &prior, &self.goal(plan, t), cap);
        (q, r)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pruned_projection_matches_the_original_bit_for_bit() {
        let mut random = 11_u64;
        let mut next = || {
            random = random.wrapping_mul(6364136223846793005).wrapping_add(1);
            (random >> 11) as f64 / (1u64 << 53) as f64
        };
        for case in 0..4000 {
            let n = 1 + case % 40;
            let (cx, cy, r) = (next() * 80. - 40., next() * 80. - 40., 0.01 + next() * 20.);
            let mut points: Vec<P2> = (0..n)
                .map(|_| {
                    let angle = next() * std::f64::consts::TAU;
                    let rho = r * next().sqrt();
                    [cx + rho * angle.cos(), cy + rho * angle.sin()]
                })
                .collect();
            if case % 5 == 0 {
                // Duplicate and collinear vertices, and tiny edges.
                let first = points[0];
                points.push(first);
                points.push([first[0] + 1e-12, first[1]]);
                points.push([first[0] + 2e-12, first[1]]);
            }
            let poly = if case % 3 == 0 {
                points.clone()
            } else {
                hull(points.clone())
            };
            for k in 0..24 {
                let p = match k % 4 {
                    0 => [cx + (next() - 0.5) * r * 6., cy + (next() - 0.5) * r * 6.],
                    1 => poly[k % poly.len()],
                    2 => {
                        let a = poly[k % poly.len()];
                        let b = poly[(k + 1) % poly.len()];
                        [(a[0] + b[0]) * 0.5, (a[1] + b[1]) * 0.5]
                    }
                    _ => [cx + (next() - 0.5) * 400., cy + (next() - 0.5) * 400.],
                };
                let a = project(&poly, p);
                let b = project_reference(&poly, p);
                assert_eq!(a.map(f64::to_bits), b.map(f64::to_bits), "case {case}/{k}");
            }
        }
    }
}
