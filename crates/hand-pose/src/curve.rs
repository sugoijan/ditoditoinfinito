// SPDX-FileCopyrightText: 2026 すごいジャン <me@sugoijan.dev>
// SPDX-License-Identifier: MIT
//! Shared bounded quintic interpolation and C2 publication.
use crate::{Guide, Parameters};
type V = [f64; 16];
#[derive(Clone)]
pub(crate) struct Knot {
    pub(crate) t: f64,
    pub(crate) p: V,
    pub(crate) v: V,
    pub(crate) a: V,
}
#[derive(Clone)]
pub(crate) struct Curve {
    pub(crate) knots: Vec<Knot>,
    pub(crate) periodic_yaw: bool,
}
/// Guide poses are smoothed by a Gaussian of this sigma, density-weighted and
/// truncated at `SMOOTH_REACH`, before their refit: with both fingers in the
/// air, fingertip targets leave the root free, and independent fits wander.
pub(crate) const SMOOTH_SIGMA: f64 = 0.04;
pub(crate) const SMOOTH_REACH: f64 = 0.16;
/// The quintic smoothstep, rising from 0 to 1 as `u` does, with zero first
/// and second derivatives at both ends.
pub fn smoother(u: f64) -> f64 {
    u * u * u * (10. + u * (-15. + 6. * u))
}
/// The smoothed pose of `n` around the `f`th (`time(j)` and `pose(j)` give
/// each). World yaw is averaged near the `f`th's, never through zero.
pub(crate) fn smoothed_pose<'a>(
    n: usize,
    f: usize,
    time: impl Fn(usize) -> f64,
    pose: impl Fn(usize) -> &'a Parameters,
    periodic: bool,
) -> Parameters {
    let t = time(f);
    let (mut sum, mut weight) = ([0.; 14], 0.);
    for j in 0..n {
        let dt = time(j) - t;
        if dt.abs() > SMOOTH_REACH {
            continue;
        }
        let density = if n == 1 {
            1.
        } else if j == 0 {
            time(1) - time(0)
        } else if j + 1 == n {
            time(n - 1) - time(n - 2)
        } else {
            (time(j + 1) - time(j - 1)) / 2.
        };
        let w = (-0.5 * (dt / SMOOTH_SIGMA).powi(2)).exp() * density;
        weight += w;
        for (c, (s, v)) in sum.iter_mut().zip(pose(j)).enumerate() {
            let v = if c == 13 && periodic {
                near_yaw(*v, pose(f)[13])
            } else {
                *v
            };
            *s += w * v;
        }
    }
    let mut out = sum.map(|v| v / weight);
    if periodic {
        out[13] = wrap_yaw(out[13]);
    }
    out
}
pub(crate) fn mix(q: Parameters, c: [f64; 2]) -> V {
    let mut p = [0.; 16];
    p[..14].copy_from_slice(&q);
    p[14..].copy_from_slice(&c);
    p
}
pub(crate) fn value(p: V) -> (Parameters, [f64; 2]) {
    (p[..14].try_into().unwrap(), [p[14], p[15]])
}
/// Channel `j`'s polynomial coefficients on a segment of length `d`.
fn coefficients(a: &Knot, b: &Knot, d: f64, j: usize) -> [f64; 6] {
    let c0 = a.p[j];
    let c1 = a.v[j] * d;
    let c2 = a.a[j] * d * d / 2.;
    let dx = b.p[j] - c0 - c1 - c2;
    let dv = b.v[j] * d - c1 - 2. * c2;
    let da = b.a[j] * d * d - 2. * c2;
    [
        c0,
        c1,
        c2,
        10. * dx - 4. * dv + da / 2.,
        -15. * dx + 7. * dv - da,
        6. * dx - 3. * dv + da / 2.,
    ]
}
/// `u`'s powers 0 to 5, each the product `powi` gives: the binary
/// decomposition LLVM expands a constant exponent into and `__powidf2`
/// computes for any other (`u⁵ = u·(u²·u²)`), so values stay bit for bit.
fn powers(u: f64) -> [f64; 6] {
    let u2 = u * u;
    let u4 = u2 * u2;
    [1., u, u2, u * u2, u4, u * u4]
}
pub(crate) fn quintic(a: &Knot, b: &Knot, t: f64) -> Knot {
    let d = b.t - a.t;
    let u = ((t - a.t) / d).clamp(0., 1.);
    let power = powers(u);
    let mut out = Knot {
        t,
        p: [0.; 16],
        v: [0.; 16],
        a: [0.; 16],
    };
    for j in 0..16 {
        for (k, &v) in coefficients(a, b, d, j).iter().enumerate() {
            out.p[j] += v * power[k];
            if k > 0 {
                out.v[j] += k as f64 * v * power[k - 1] / d;
            }
            if k > 1 {
                out.a[j] += (k * (k - 1)) as f64 * v * power[k - 2] / (d * d);
            }
        }
    }
    out
}
/// `quintic`'s position alone, by the same sums.
fn quintic_position(a: &Knot, b: &Knot, t: f64) -> V {
    let d = b.t - a.t;
    let power = powers(((t - a.t) / d).clamp(0., 1.));
    std::array::from_fn(|j| {
        coefficients(a, b, d, j)
            .iter()
            .zip(power)
            .fold(0., |sum, (&v, p)| sum + v * p)
    })
}
/// Equivalent world-yaw phase nearest a reference. This never changes a
/// finger joint or widens a model bound; only internal curve phase is unwrapped.
pub(crate) fn near_yaw(yaw: f64, reference: f64) -> f64 {
    let turns = ((reference - yaw) / 360.).round();
    if turns == 0. { yaw } else { yaw + turns * 360. }
}
pub(crate) fn wrap_yaw(yaw: f64) -> f64 {
    if (-180. ..=180.).contains(&yaw) {
        yaw
    } else {
        (yaw + 180.).rem_euclid(360.) - 180.
    }
}
impl Curve {
    pub(crate) fn constant(q: Parameters, c: [f64; 2], periodic_yaw: bool) -> Self {
        Self {
            periodic_yaw,
            knots: vec![
                Knot {
                    t: 0.,
                    p: mix(q, c),
                    v: [0.; 16],
                    a: [0.; 16],
                },
                Knot {
                    t: 1e9,
                    p: mix(q, c),
                    v: [0.; 16],
                    a: [0.; 16],
                },
            ],
        }
    }
    /// The segment holding `t`.
    fn segment(&self, t: f64) -> usize {
        self.knots
            .partition_point(|k| k.t <= t)
            .saturating_sub(1)
            .min(self.knots.len() - 2)
    }
    pub(crate) fn at(&self, t: f64) -> Knot {
        let k = self.segment(t);
        quintic(&self.knots[k], &self.knots[k + 1], t)
    }
    /// `at`'s position alone, for the many readers of nothing else.
    pub(crate) fn position(&self, t: f64) -> V {
        let k = self.segment(t);
        quintic_position(&self.knots[k], &self.knots[k + 1], t)
    }
    /// Normalize only world-yaw position at the model boundary. Velocity and
    /// acceleration remain derivatives of the continuous internal phase.
    pub(crate) fn parameters(&self, p: V) -> (Parameters, [f64; 2]) {
        let (mut q, center) = value(p);
        if self.periodic_yaw {
            q[13] = wrap_yaw(q[13]);
        }
        (q, center)
    }
    pub(crate) fn from_guide(g: &Guide, periodic_yaw: bool) -> Self {
        let n = g.times.len();
        let mut k: Vec<_> = (0..n)
            .map(|i| Knot {
                t: g.times[i],
                p: mix(g.poses[i], g.centers[i]),
                v: [0.; 16],
                a: [0.; 16],
            })
            .collect();
        if periodic_yaw {
            for i in 1..n {
                k[i].p[13] = near_yaw(k[i].p[13], k[i - 1].p[13]);
            }
        }
        for i in 1..n - 1 {
            let l = k[i].t - k[i - 1].t;
            let r = k[i + 1].t - k[i].t;
            for j in 0..16 {
                let a = (k[i].p[j] - k[i - 1].p[j]) / l;
                let b = (k[i + 1].p[j] - k[i].p[j]) / r;
                if a * b > 0. {
                    k[i].v[j] = (a * r + b * l) / (l + r);
                    let bound = 2.5 * a.abs().min(b.abs());
                    k[i].v[j] = k[i].v[j].clamp(-bound, bound);
                }
                k[i].a[j] = 2. * (b - a) / (l + r);
            }
        }
        // Bound the quintic's Bezier controls to each segment's endpoint range.
        // A short interval beside a longer one must not amplify an acceleration
        // estimate into a body excursion. Shared derivatives retain C2 joins.
        for i in 0..n {
            for j in 0..16 {
                let mut lo = f64::NEG_INFINITY;
                let mut hi = f64::INFINITY;
                for neighbor in [i.checked_sub(1), (i + 1 < n).then_some(i + 1)]
                    .into_iter()
                    .flatten()
                {
                    let d = k[neighbor].t - k[i].t;
                    let a = k[i].p[j].min(k[neighbor].p[j]);
                    let b = k[i].p[j].max(k[neighbor].p[j]);
                    let c = k[i].p[j] + 2. * k[i].v[j] * d / 5.;
                    lo = lo.max((a - c) * 20. / (d * d));
                    hi = hi.min((b - c) * 20. / (d * d));
                }
                k[i].a[j] = k[i].a[j].clamp(lo.min(hi), hi);
            }
        }
        Self {
            knots: k,
            periodic_yaw,
        }
    }
    pub(crate) fn splice(&self, new: &Curve, start: f64, end: f64, now: f64) -> Self {
        self.splice_grid(new, start, end, now, true)
    }
    pub(crate) fn splice_grid(
        &self,
        new: &Curve,
        start: f64,
        end: f64,
        now: f64,
        extra_grid: bool,
    ) -> Self {
        let mut times: Vec<_> = self
            .knots
            .iter()
            .filter(|k| k.t >= now - 1. && k.t < start)
            .map(|k| k.t)
            .collect();
        if extra_grid {
            times.push((now - 1.).max(0.));
        }
        times.extend([start, end]);
        times.extend(new.knots.iter().filter(|k| k.t > start).map(|k| k.t));
        let n = ((end - start) * 60.).ceil() as usize;
        for i in 1..if extra_grid { n } else { 1 } {
            times.push(start + (end - start) * i as f64 / n as f64);
        }
        // Preserve the actual splice boundaries, rather than a neighboring
        // floating-point representation of the same instant. Sub-microsecond
        // polynomial segments magnify roundoff in endpoint accelerations.
        times.retain(|t| (*t - start).abs() > 1e-6 && (*t - end).abs() > 1e-6);
        times.extend([start, end]);
        times.sort_by(f64::total_cmp);
        times.dedup_by(|a, b| (*a - *b).abs() < 1e-6);
        debug_assert_eq!(self.periodic_yaw, new.periodic_yaw);
        // Equivalent phases must share one winding before blending. Position,
        // velocity and acceleration at the committed boundary stay untouched.
        let phase_shift = if self.periodic_yaw {
            let middle = (start + end) * 0.5;
            let a = self.position(middle)[13];
            let b = new.position(middle)[13];
            near_yaw(b, a) - b
        } else {
            0.
        };
        let align = |mut k: Knot| {
            if phase_shift != 0. {
                k.p[13] += phase_shift;
            }
            k
        };
        let knots = times
            .into_iter()
            .map(|t| {
                if t <= start {
                    if !extra_grid && let Ok(i) = self.knots.binary_search_by(|k| k.t.total_cmp(&t))
                    {
                        return self.knots[i].clone();
                    }
                    return self.at(t);
                }
                if t >= end {
                    if !extra_grid && let Ok(i) = new.knots.binary_search_by(|k| k.t.total_cmp(&t))
                    {
                        return align(new.knots[i].clone());
                    }
                    return align(new.at(t));
                }
                let a = self.at(t);
                let b = align(new.at(t));
                let d = end - start;
                let u = (t - start) / d;
                let w = u.powi(3) * (10. - 15. * u + 6. * u * u);
                let v = 30. * u * u * (1. - u).powi(2) / d;
                let acc = 60. * u * (1. - u) * (1. - 2. * u) / (d * d);
                Knot {
                    t,
                    p: std::array::from_fn(|j| a.p[j] + w * (b.p[j] - a.p[j])),
                    v: std::array::from_fn(|j| {
                        a.v[j] + w * (b.v[j] - a.v[j]) + v * (b.p[j] - a.p[j])
                    }),
                    a: std::array::from_fn(|j| {
                        a.a[j]
                            + w * (b.a[j] - a.a[j])
                            + 2. * v * (b.v[j] - a.v[j])
                            + acc * (b.p[j] - a.p[j])
                    }),
                }
            })
            .collect();
        Self {
            knots,
            periodic_yaw: self.periodic_yaw,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn guide(times: &[f64], yaws: &[f64]) -> Guide {
        Guide {
            times: times.to_vec(),
            poses: yaws
                .iter()
                .map(|yaw| {
                    let mut q = [0.; 14];
                    q[13] = *yaw;
                    q
                })
                .collect(),
            centers: vec![[0.; 2]; times.len()],
        }
    }
    #[test]
    fn periodic_yaw_keeps_short_arcs_bounded_poses_and_physical_derivatives() {
        let g = guide(&[0., 1., 2., 3.], &[170., 179., -179., -170.]);
        let curve = Curve::from_guide(&g, true);
        let legacy = Curve::from_guide(&g, false);
        assert_eq!(
            curve.knots.iter().map(|k| k.p[13]).collect::<Vec<_>>(),
            [170., 179., 181., 190.]
        );
        assert_eq!(legacy.knots[2].p[13], -179.);
        for i in 0..=3000 {
            let t = i as f64 / 1000.;
            let k = curve.at(t);
            let q = curve.parameters(k.p).0;
            assert!((-180. ..=180.).contains(&q[13]));
            assert!((q[13].to_radians().cos() - k.p[13].to_radians().cos()).abs() < 1e-12);
        }
        // Numerical derivatives of the actual rotation agree with the analytic
        // unwrapped angular derivatives even across the public ±180° boundary.
        for t in [1.49, 1.5, 1.51] {
            let h = 1e-4;
            let k = curve.at(t);
            let q = curve.parameters(k.p).0;
            let angle = q[13].to_radians();
            let velocity = k.v[13].to_radians();
            let acceleration = k.a[13].to_radians();
            let f = |u: f64| curve.parameters(curve.at(u).p).0[13].to_radians().sin();
            assert!(((f(t + h) - f(t - h)) / (2. * h) - angle.cos() * velocity).abs() < 1e-7);
            assert!(
                ((f(t + h) - 2. * f(t) + f(t - h)) / (h * h)
                    - (angle.cos() * acceleration - angle.sin() * velocity * velocity))
                    .abs()
                    < 1e-6
            );
        }
    }
    #[test]
    fn periodic_publication_aligns_equivalent_turns_and_keeps_the_prefix() {
        let old = Curve::from_guide(
            &guide(&[0., 1., 2., 3., 4.], &[175., 179., -179., -175., -170.]),
            true,
        );
        let new = Curve::from_guide(
            &guide(
                &[1.5, 2., 2.5, 3., 4.],
                &[-180., -179., -177., -175., -170.],
            ),
            true,
        );
        let c = old.splice_grid(&new, 2., 2.5, 1.5, false);
        for i in 1000..=2000 {
            let t = i as f64 / 1000.;
            let a = old.at(t);
            let b = c.at(t);
            assert_eq!(a.p.map(f64::to_bits), b.p.map(f64::to_bits));
            assert_eq!(a.v.map(f64::to_bits), b.v.map(f64::to_bits));
            assert_eq!(a.a.map(f64::to_bits), b.a.map(f64::to_bits));
        }
        for i in 2000..=3000 {
            let k = c.at(i as f64 / 1000.);
            assert!((180. ..=186.).contains(&k.p[13]));
        }
        for w in c.knots.windows(3) {
            let a = quintic(&w[0], &w[1], w[1].t);
            let b = quintic(&w[1], &w[2], w[1].t);
            for (a, b, bound) in [(a.p, b.p, 1e-8), (a.v, b.v, 1e-6), (a.a, b.a, 1e-3)] {
                for (a, b) in a.into_iter().zip(b) {
                    assert!((a - b).abs() < bound);
                }
            }
        }
    }
}
