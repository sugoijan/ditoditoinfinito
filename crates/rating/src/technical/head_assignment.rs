use super::*;
// The geometric outcome of a primary press on a lane without a nearby mine.
// All quadrature worlds reuse these polylines. Adjacent equal outcomes share a mass.
#[derive(Clone, Copy, PartialEq)]
struct Outcome {
    next: usize,
    tier: Option<usize>,
}
fn pure_outcome(hs: &[f64], k: usize, a: usize, b: usize, at: f64, tiers: &[f64; 6]) -> Outcome {
    let win = tiers[5];
    let h = k.max(a + hs[a..b].partition_point(|v| *v < at - win));
    let landed = h < b && (hs[h] - at).abs() <= win;
    if !landed {
        return Outcome {
            next: h,
            tier: None,
        };
    }
    let diff = (hs[h] - at).abs();
    Outcome {
        next: h + 1,
        tier: Some(tiers[1..].partition_point(|&w| diff > w).min(4)),
    }
}
struct Piece {
    hi: usize,
    outcome: Outcome,
}
pub(super) struct PurePlan {
    a: usize,
    b: usize,
    cuts: Vec<f64>,
    paths: Vec<Vec<Piece>>,
}
pub(super) fn pure_plans(g: &Geometry) -> [Vec<Option<PurePlan>>; 4] {
    let tiers = ddi::TIER_UNITS.map(|v| Difficulty::Normal.seconds(v));
    std::array::from_fn(|j| {
        let l = &g.lanes[j];
        let hs = &l.heads;
        hs.iter()
            .map(|&t| {
                let left = t - BOUND;
                let right = t + BOUND;
                let win = tiers[5];
                let a = hs.partition_point(|v| *v < left - win);
                let b = hs.partition_point(|v| *v <= right + win);
                let m = l.mines.partition_point(|v| *v < left - ddi::MINE_WINDOW);
                if b - a > 12
                    || l.mines
                        .get(m)
                        .is_some_and(|v| *v <= right + ddi::MINE_WINDOW)
                {
                    return None;
                }
                let mut cuts = vec![left, right];
                for &h in &hs[a..b] {
                    for w in tiers {
                        for s in [h - w, h + w] {
                            if s > left && s < right {
                                cuts.push(s);
                            }
                        }
                    }
                }
                cuts.sort_by(f64::total_cmp);
                cuts.dedup_by(|a, b| (*a - *b).abs() < 1e-11);
                let paths = (a..=b)
                    .map(|k| {
                        let mut path: Vec<Piece> = Vec::new();
                        for (i, ww) in cuts.windows(2).enumerate() {
                            let o = pure_outcome(hs, k, a, b, (ww[0] + ww[1]) * 0.5, &tiers);
                            if let Some(p) = path.last_mut().filter(|p| p.outcome == o) {
                                p.hi = i + 1;
                            } else {
                                path.push(Piece {
                                    hi: i + 1,
                                    outcome: o,
                                });
                            }
                        }
                        path
                    })
                    .collect();
                Some(PurePlan { a, b, cuts, paths })
            })
            .collect()
    })
}
pub(super) fn apply_pure(
    hs: &[f64],
    ev: &Event,
    p: &PurePlan,
    l: &Law,
    states: &State,
    worth: &[(f64, f64)],
    out: &mut Counts,
) -> State {
    let tiers = ddi::TIER_UNITS.map(|v| Difficulty::Normal.seconds(v));
    let mut probs = [0.; 160]; // At most 12 heads, each with twelve tier boundaries.
    for (y, &x) in probs.iter_mut().zip(&p.cuts) {
        *y = l.raw_edges_cdf(x - ev.at - l.shift);
    }
    let mut next = State::new();
    let atoms = l.atoms();
    for (&(k, _), &v) in states {
        let k = k.max(p.a);
        credit(&mut next, (k, 0), v * l.omit);
        let mut act = |o: Outcome, mass: f64| {
            if mass <= 0. {
                return;
            };
            let q = v * (1. - l.omit) * mass;
            credit(&mut next, (o.next, 0), q);
            if let Some(tier) = o.tier {
                out.heads += q;
                out.timing_loss += q * (50 - ddi::TAP_POINTS[tier]) as f64;
                let h = o.next - 1;
                out.hold_units += q * if h == ev.head { worth[h].0 } else { worth[h].1 };
            }
        };
        if k > p.b {
            act(
                Outcome {
                    next: k,
                    tier: None,
                },
                1.,
            );
            continue;
        }
        let mut lo = 0;
        for piece in &p.paths[k - p.a] {
            act(piece.outcome, probs[piece.hi] - probs[lo]);
            lo = piece.hi;
        }
        for (x, q) in atoms {
            if q > 0. {
                act(pure_outcome(hs, k, p.a, p.b, ev.at + x, &tiers), q);
            }
        }
    }
    normalize(&mut next);
    next
}
fn normalize(states: &mut State) {
    states.retain(|_, p| *p > 1e-11);
    let total = states.values().sum::<f64>();
    for p in states.values_mut() {
        *p /= total;
    }
}
