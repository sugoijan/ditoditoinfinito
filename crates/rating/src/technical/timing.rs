use super::*;
pub(super) type TimingGrids = HashMap<i32, [f64; 89]>;
pub(super) struct Law {
    pub(super) omit: f64,
    pub(super) shift: f64,
    grid: [f64; 89],
}
impl Law {
    pub(super) fn raw_edges_cdf(&self, x: f64) -> f64 {
        let x = x.clamp(-BOUND, BOUND);
        let g = &self.grid;
        let k = TIMING_EDGES.partition_point(|v| *v <= x);
        if k == 0 {
            return g[0];
        }
        if k >= 89 {
            return g[88];
        }
        let w = (x - TIMING_EDGES[k - 1]) / (TIMING_EDGES[k] - TIMING_EDGES[k - 1]);
        g[k - 1] + w * (g[k] - g[k - 1])
    }
    pub(super) fn atoms(&self) -> [(f64, f64); 4] {
        let a = self.raw_edges_cdf(-BOUND);
        let b = self.raw_edges_cdf(BOUND);
        [
            ((-BOUND + self.shift).clamp(-BOUND, BOUND), a),
            ((BOUND + self.shift).clamp(-BOUND, BOUND), 1. - b),
            (-BOUND, self.raw_edges_cdf(-BOUND - self.shift) - a),
            (BOUND, b - self.raw_edges_cdf(BOUND - self.shift)),
        ]
    }
    fn stumble(&self, threshold: f64) -> f64 {
        let mut near = self.raw_edges_cdf(threshold - self.shift)
            - self.raw_edges_cdf(-threshold - self.shift);
        for (x, p) in self.atoms() {
            if x.abs() <= threshold {
                near += p;
            }
        }
        (self.omit + (1. - self.omit) * (1. - near)).clamp(0., 1. - 1e-10)
    }
}
pub(super) fn laws(
    g: &Geometry,
    level: usize,
    weak: bool,
    z: f64,
    form_z: f64,
    grids: &mut TimingGrids,
) -> Vec<Law> {
    let mut vs = [1.; 7];
    if weak {
        vs[6] = 0.75;
    }
    let a = &CPU_LEVELS_V2.levels[level];
    let m = a.model;
    let skills = Skills::new(vs).unwrap();
    let first = g.rows[0].0;
    let span = (g.rows.last().unwrap().0 - first).max(1.);
    let mut recent = VecDeque::<(f64, f64)>::new();
    g.rows
        .iter()
        .map(|&(t, d)| {
            while recent.front().is_some_and(|e| e.0 <= t - m.burst_seconds) {
                recent.pop_front();
            }
            let burst = {
                // Linear decay in time since the most recent stumble, conditioned on its row.
                let mut remaining = 1.;
                let mut effect = 0.;
                for &(s, log) in recent.iter().rev() {
                    let survived = log.exp();
                    effect += remaining * (1. - survived) * (1. - (t - s) / m.burst_seconds);
                    remaining *= survived;
                }
                effect
            };
            let (w, om, _) = row_response(a, &skills, &d, m);
            let form = 1. + m.form * if form_z > 0. { form_z } else { form_z / 2. };
            let spread =
                w * form * (1. + m.fatigue_end * ((t - first) / span)) * (1. + m.burst * burst);
            let width = (spread * 10.).round().max(1.) / 10.;
            let shift = {
                let phase = if m.drift_period > 0. {
                    ((t - first) / m.drift_period).fract()
                } else {
                    0.
                };
                z * (a.lean_ms.powi(2)
                    + a.drift_ms.powi(2) * ((1. - phase).powi(2) + phase.powi(2)))
                .sqrt()
                    / 1000.
            };
            let grid = *grids
                .entry((width * 10.).round() as i32)
                .or_insert_with(|| {
                    TIMING_EDGES.map(|x| {
                        let f = |s: f64| {
                            0.5 + 0.5
                                * x.signum()
                                * erf(x.abs() * 1000. / s / std::f64::consts::SQRT_2)
                        };
                        (1. - m.tail_share) * f(width) + m.tail_share * f(width * m.tail_width)
                    })
                });
            let law = Law {
                omit: 0.0001 + om * 0.9999,
                shift,
                grid,
            };
            let st = law.stumble(m.stumble_ms / 1000.);
            let l = f64::from(d.count) * (-st).ln_1p();
            recent.push_back((t, l));
            law
        })
        .collect()
}
