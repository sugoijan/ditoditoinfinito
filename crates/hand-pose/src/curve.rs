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
pub(crate) fn quintic(a: &Knot, b: &Knot, t: f64) -> Knot {
    let d = b.t - a.t;
    let u = ((t - a.t) / d).clamp(0., 1.);
    let mut out = Knot {
        t,
        p: [0.; 16],
        v: [0.; 16],
        a: [0.; 16],
    };
    for j in 0..16 {
        let c0 = a.p[j];
        let c1 = a.v[j] * d;
        let c2 = a.a[j] * d * d / 2.;
        let dx = b.p[j] - c0 - c1 - c2;
        let dv = b.v[j] * d - c1 - 2. * c2;
        let da = b.a[j] * d * d - 2. * c2;
        let c = [
            c0,
            c1,
            c2,
            10. * dx - 4. * dv + da / 2.,
            -15. * dx + 7. * dv - da,
            6. * dx - 3. * dv + da / 2.,
        ];
        for (k, &v) in c.iter().enumerate() {
            out.p[j] += v * u.powi(k as i32);
            if k > 0 {
                out.v[j] += k as f64 * v * u.powi(k as i32 - 1) / d;
            }
            if k > 1 {
                out.a[j] += (k * (k - 1)) as f64 * v * u.powi(k as i32 - 2) / (d * d);
            }
        }
    }
    out
}
impl Curve {
    pub(crate) fn constant(q: Parameters, c: [f64; 2]) -> Self {
        Self {
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
    pub(crate) fn at(&self, t: f64) -> Knot {
        let k = self
            .knots
            .partition_point(|k| k.t <= t)
            .saturating_sub(1)
            .min(self.knots.len() - 2);
        quintic(&self.knots[k], &self.knots[k + 1], t)
    }
    pub(crate) fn from_guide(g: &Guide) -> Self {
        let n = g.times.len();
        let mut k: Vec<_> = (0..n)
            .map(|i| Knot {
                t: g.times[i],
                p: mix(g.poses[i], g.centers[i]),
                v: [0.; 16],
                a: [0.; 16],
            })
            .collect();
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
        Self { knots: k }
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
                        return new.knots[i].clone();
                    }
                    return new.at(t);
                }
                let a = self.at(t);
                let b = new.at(t);
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
        Self { knots }
    }
}
