//! Opt-in study policy, shared by numerical regressions and private comparisons.
use crate::{Goal, Model, Parameters, Targets, refine_contacts};

#[derive(Clone, Copy, Debug, Default)]
pub struct ContactCorrection {
    pub iterations: usize,
    pub evaluations: usize,
    pub blend: f64,
}

fn norm(v: impl Iterator<Item = f64>) -> f64 {
    v.map(|x| x * x).sum::<f64>().sqrt()
}

/// Apply the studied bounded contact correction, without changing target times.
/// Quiet poses are returned bit-for-bit. Smooth activation is not a guarantee
/// of temporally smooth output across branches of the preceding pose solver.
pub fn correct_contact_pose(
    m: &Model,
    target: Targets,
    q: Parameters,
) -> (Parameters, ContactCorrection) {
    let g = m.geometry(&q);
    let h = (0..2)
        .map(|d| (g.tips[d][2] - target.tips[d][2]).abs())
        .fold(0., f64::max);
    let u = ((h - 0.005) / 0.020).clamp(0., 1.);
    if u == 0. {
        return (q, ContactCorrection::default());
    }
    let goal = Goal {
        tips: target.tips,
        com: [g.com[0], g.com[1]],
    };
    let mut candidate = q;
    let r = refine_contacts(m, &mut candidate, &q, &goal, 8);
    let joint = (0..8)
        .map(|i| (candidate[i] - q[i]).abs())
        .fold(0., f64::max);
    let root = norm((8..11).map(|i| candidate[i] - q[i]));
    let angle = norm((11..14).map(|i| candidate[i] - q[i]));
    let scale = (2. / joint.max(1e-12))
        .min(0.5 / root.max(1e-12))
        .min(1. / angle.max(1e-12))
        .min(1.);
    let blend = u * u * u * (10. + u * (-15. + 6. * u)) * scale;
    let candidate = std::array::from_fn(|i| q[i] + blend * (candidate[i] - q[i]));
    let before = (0..2)
        .map(|d| norm((0..3).map(|i| g.tips[d][i] - target.tips[d][i])))
        .fold(0., f64::max);
    let cg = m.geometry(&candidate);
    let after = (0..2)
        .map(|d| norm((0..3).map(|i| cg.tips[d][i] - target.tips[d][i])))
        .fold(0., f64::max);
    let after_height = (0..2)
        .map(|d| (cg.tips[d][2] - target.tips[d][2]).abs())
        .fold(0., f64::max);
    let mut report = ContactCorrection {
        iterations: r.iterations,
        evaluations: r.evaluations,
        blend: 0.,
    };
    if after_height < h && after <= before + 1e-9 {
        report.blend = blend;
        (candidate, report)
    } else {
        (q, report)
    }
}
