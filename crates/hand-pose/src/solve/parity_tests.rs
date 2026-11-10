//! The blocking fit (`solve_impl`) and the resumable one (`PoseFit`) against
//! a frozen copy of the blocking fit, bit for bit, over generated starting
//! poses, priors, targets, caps and stopping policies.
use super::*;
use crate::PoseFit;

/// SplitMix64: the cases are the same on every run and target.
struct Rng(u64);
impl Rng {
    fn next(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9e37_79b9_7f4a_7c15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
        z ^ (z >> 31)
    }
    fn range(&mut self, lo: f64, hi: f64) -> f64 {
        lo + (hi - lo) * ((self.next() >> 11) as f64 / (1_u64 << 53) as f64)
    }
}

/// How often each way through the reference fit was taken.
#[derive(Debug, Default)]
struct Paths {
    cap_zero: usize,
    stop_met: usize,
    analytic: usize,
    forward: usize,
    perturbed: usize,
    locked: usize,
    rejected: usize,
    small_change: usize,
    cap_reached: usize,
}

/// `solve_impl` as it was before it was driven by `PoseFit`, with only the
/// counters added. Do not change its arithmetic.
#[allow(clippy::too_many_arguments)]
fn reference(
    model: &Model,
    q: &mut Parameters,
    prior: &Parameters,
    goal: &Goal,
    cap: usize,
    reuse_geometry: bool,
    feasible_bounds: bool,
    stop: &Stop,
    paths: &mut Paths,
) -> SolveReport {
    model.clamp(q);
    let mut kin = [Kin::default(), Kin::default()];
    let mut scratch: Option<Kin> = None;
    let mut base = 0;
    let mut current: Option<(Eval, [f64; 24])> = None;
    let mut out = SolveReport::default();
    if cap.min(40) == 0 {
        paths.cap_zero += 1;
    }
    let mut finished = false;
    for iteration in 0..cap.min(40) {
        let (g, r) = current.unwrap_or_else(|| {
            let g = model.evaluate_into(q, &mut kin[base]);
            (g, residual(&g, q, prior, goal))
        });
        current = Some((g, r));
        out.evaluations += 1;
        if stop.met(&r) {
            paths.stop_met += 1;
            finished = true;
            break;
        }
        let analytic = if reuse_geometry {
            jacobian(model, q, &kin[base], &g)
        } else {
            None
        };
        let j = if let Some(j) = analytic {
            paths.analytic += 1;
            out.evaluations += 1;
            j
        } else {
            paths.forward += 1;
            if reuse_geometry {
                paths.perturbed += 1;
            }
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
            paths.locked += 1;
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
        if !accepted {
            paths.rejected += 1;
        } else if change < stop.change {
            paths.small_change += 1;
        }
        if !accepted || change < stop.change {
            finished = true;
            break;
        }
    }
    if !finished && cap.min(40) > 0 {
        paths.cap_reached += 1;
    }
    let (g, r) = current.unwrap_or_else(|| {
        let g = model.evaluate_into(q, &mut kin[base]);
        (g, residual(&g, q, prior, goal))
    });
    out.target_error_mm = max_tip_error(&g.tips, &goal.tips);
    out.objective = energy(&r);
    out
}

/// Every field of the fitted pose and its report, as bits.
fn bits(q: &Parameters, r: &SolveReport) -> (Vec<u64>, [u64; 6]) {
    (
        q.iter().map(|v| v.to_bits()).collect(),
        [
            r.iterations as u64,
            r.evaluations as u64,
            r.target_error_mm.to_bits(),
            r.objective.to_bits(),
            r.stream_weight.to_bits(),
            u64::from(r.balance_relaxed) | u64::from(r.stream_missing) << 1,
        ],
    )
}

#[test]
fn the_blocking_and_resumable_fits_match_the_frozen_reference_bit_for_bit() {
    let rigid = Model::load(include_bytes!("../../../../assets/hand/model.json")).unwrap();
    // Without a rigid skin there is no analytic Jacobian: reusing geometry
    // then takes forward differences over perturbed kinematics.
    let mut flexible = Model::load(include_bytes!("../../../../assets/hand/model.json")).unwrap();
    flexible.skin.rigid = false;
    let mut rng = Rng(0x05ee_df17);
    let mut paths = Paths::default();
    let caps = [0, 1, 2, 3, 5, 8, 12, 20, 40, 64];
    let stops = [
        STRICT,
        Stop::scaled(6.),
        // Met by the first residual.
        Stop::scaled(1e9),
        // Never met by a step's change: runs to its cap or a rejected search.
        Stop {
            change: 0.,
            ..STRICT
        },
    ];
    // One resumable fit reused across the cases, as the rolling preparation
    // reuses its own: what a previous fit left in it must not matter.
    let style = &rigid.style[0];
    let start_goal = Goal {
        tips: style.tips,
        com: [style.com[0], style.com[1]],
    };
    let mut resumed = PoseFit::new(&rigid, style.q, style.q, start_goal, 0);
    for case in 0..480 {
        let model = if case % 2 == 0 { &rigid } else { &flexible };
        let mut pose = |margin: f64| -> Parameters {
            std::array::from_fn(|i| {
                let span = model.hi[i] - model.lo[i];
                rng.range(model.lo[i] - margin * span, model.hi[i] + margin * span)
            })
        };
        // Some controls start outside their bounds (clamped onto them).
        let q = pose(if case % 3 == 0 { 0.15 } else { 0. });
        let prior = if case % 4 < 2 { q } else { pose(0.) };
        let mut target = pose(0.);
        model.clamp(&mut target);
        let reached = model.geometry(&target);
        // Mostly reachable targets near a real pose, some far out of reach.
        let reach = if case % 5 == 4 { 150. } else { 3. };
        let goal = Goal {
            tips: reached
                .tips
                .map(|t| t.map(|v| v + rng.range(-reach, reach))),
            com: [
                reached.com[0] + rng.range(-4., 4.),
                reached.com[1] + rng.range(-4., 4.),
            ],
        };
        let cap = caps[(case / 2) % caps.len()];
        let stop = stops[(case / 2 / caps.len()) % stops.len()];
        for reuse in [false, true] {
            for feasible in [false, true] {
                let what = format!("case {case} cap {cap} reuse {reuse} feasible {feasible}");
                let mut expected_q = q;
                let expected = reference(
                    model,
                    &mut expected_q,
                    &prior,
                    &goal,
                    cap,
                    reuse,
                    feasible,
                    &stop,
                    &mut paths,
                );
                let expected = bits(&expected_q, &expected);
                let mut blocking_q = q;
                let blocking = solve_impl(
                    model,
                    &mut blocking_q,
                    &prior,
                    &goal,
                    cap,
                    reuse,
                    feasible,
                    &stop,
                );
                assert_eq!(bits(&blocking_q, &blocking), expected, "blocking, {what}");
                if !feasible {
                    resumed.reset_with(model, q, prior, goal, cap, reuse, stop);
                    while !resumed.advance(model) {}
                    let (fitted, report) = resumed.result().unwrap();
                    assert_eq!(bits(&fitted, &report), expected, "resumable, {what}");
                }
            }
        }
    }
    // Every way through the fit was taken.
    let Paths {
        cap_zero,
        stop_met,
        analytic,
        forward,
        perturbed,
        locked,
        rejected,
        small_change,
        cap_reached,
    } = &paths;
    for (name, n) in [
        ("cap 0", cap_zero),
        ("stop met", stop_met),
        ("analytic Jacobian", analytic),
        ("forward differences", forward),
        ("perturbed forward differences", perturbed),
        ("locked bound", locked),
        ("rejected search", rejected),
        ("small change", small_change),
        ("cap reached", cap_reached),
    ] {
        assert!(*n > 0, "no fit took the {name} path: {paths:?}");
    }
}
