//! Analytical timing/load estimate corrected by the network. All per-level
//! constants and rounding here are inputs to the frozen weights.
use super::features::Chart;
use ddi_engine::cpu::{Ability, CPU_LEVELS_V2};
use std::sync::OnceLock;

const MINE_HIT: [f64; 10] = [
    0.30, 0.268, 0.236, 0.204, 0.172, 0.14, 0.14, 0.06, 0.06, 0.02,
];

pub(super) fn erf(x: f64) -> f64 {
    let t = 1. / (1. + 0.3275911 * x);
    let poly = ((((1.061405429 * t - 1.453152027) * t + 1.421413741) * t - 0.284496736) * t
        + 0.254829592)
        * t;
    1. - poly * (-x * x).exp()
}

fn reward(ability: Ability, width: f64) -> (f64, f64) {
    let lean_variance = ability.lean_ms.powi(2) + 0.67 * ability.drift_ms.powi(2);
    let (mut score, mut stumble) = (0., 0.);
    for (factor, mass) in [(1., 0.98), (3., 0.02)] {
        let divisor = std::f64::consts::SQRT_2 * ((width * factor).powi(2) + lean_variance).sqrt();
        for (window, weight) in [(2., 20.), (5., 30.), (11., 30.), (17., 20.)] {
            score += mass * weight * erf(window * (1000. / 120.) / divisor);
        }
        stumble += mass * (1. - erf(90. / divisor));
    }
    (score, stumble)
}

fn lookup(level: usize, width: f64) -> (f64, f64) {
    // Only compute the quantized widths actually used. Shared across charts.
    static TABLE: OnceLock<Vec<OnceLock<(f64, f64)>>> = OnceLock::new();
    let table = TABLE.get_or_init(|| (0..80010).map(|_| OnceLock::new()).collect());
    let index = (width * 10.).round_ties_even().min(8000.) as usize;
    *table[level * 8001 + index]
        .get_or_init(|| reward(CPU_LEVELS_V2.levels[level], index as f64 / 10.))
}

pub(super) fn predict(chart: &Chart) -> [[f32; 9]; 10] {
    let rows = &chart.rows;
    let mut kept = rows.iter().filter(|r| r.heads > 0.);
    let first = kept.next().expect("validated head count").time;
    let duration = (kept.next_back().map_or(first, |r| r.time) - first).max(1.);
    let near = rows.iter().map(|r| r.near).sum::<f64>() / rows.len() as f64;
    let power = (1.5 + near / 12.).clamp(1.5, 2.);
    let mine_count = rows
        .iter()
        .map(|r| r.mine_near)
        .sum::<f64>()
        .max(rows.iter().map(|r| r.mine_inside).sum());
    let mut sums = [[0.; 8]; 10];
    let mut denominator = 0.;
    let mut sustain = 0.;
    let mut head_rows = 0;
    for row in rows.iter().filter(|r| r.heads > 0.) {
        let d = &row.demand;
        denominator += row.heads + row.units;
        sustain += d.sustain;
        head_rows += 1;
        let progress = (row.time - first) / duration;
        for (level, a) in CPU_LEVELS_V2.levels.iter().enumerate() {
            // Balanced first, then weakening Stream through Technical. Evasion
            // has its own mine penalty below.
            for (condition, sum) in sums[level].iter_mut().enumerate() {
                let capacity = a.speed
                    * if condition == usize::from(d.count.min(3)) {
                        0.75
                    } else {
                        1.
                    };
                let stamina = if condition == 4 { 0.75 } else { 1. };
                let load = d.speed * (1. + 0.25 * f64::from(d.held)) / capacity
                    + 0.7 * (d.sustain / (0.75 * a.speed * stamina) - 1.).max(0.);
                let jack = (condition == 5 && d.count == 1) || (condition == 6 && d.count > 1);
                let lane_capacity = 0.65 * a.speed * if jack { 0.75 } else { 1. };
                let excess = (load.max(d.lane / lane_capacity) - 1.).max(0.);
                let omit = (a.floor + 0.067 * excess + 0.1 * excess * excess).min(0.95);
                let rhythm = if condition == 7 { 0.75 } else { 1. };
                let mut width = (a.spread_ms
                    * (1. + (load - 0.65).max(0.))
                    * (1. + 0.35 * d.irregular / rhythm))
                    .min(150.)
                    * 1.055
                    * (1. + 0.25 * progress);
                let stumble = omit + (1. - omit) * lookup(level, width).1;
                width *= 2. - (-1.35 * (row.near + 0.3) * stumble).exp();
                let tap = (1. - omit) * lookup(level, width).0 - 240. * omit;
                let endure = if condition == 4 || condition == 7 {
                    0.75_f64.sqrt()
                } else {
                    1.
                };
                let body = 100.
                    * (1. - omit)
                    * (-0.3 * omit * (1. + row.near) * row.longest / endure).exp();
                *sum += tap * row.heads + body * row.units;
            }
        }
    }
    let stamina = 1. + (sustain / f64::from(head_rows) / 4.).clamp(0., 2.);
    std::array::from_fn(|i| {
        let penalty = mine_count * MINE_HIT[i] * stamina * 100. / denominator;
        std::array::from_fn(|j| match j {
            0 => (sums[i][0] / denominator - penalty) as f32,
            8 => (penalty * (0.75_f64.powf(-power) - 1.)) as f32,
            _ => ((sums[i][0] - sums[i][j]) / denominator) as f32,
        })
    })
}
