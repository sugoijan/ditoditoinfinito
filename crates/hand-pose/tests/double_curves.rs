// SPDX-FileCopyrightText: 2026 すごいジャン <me@sugoijan.dev>
// SPDX-License-Identifier: MIT
use ddi_hand_pose::{ContactCurve, Goal, Model, Plan, refine_contacts};

// Authored wide stance with a deliberately poor starting branch. No chart or
// captured pose is embedded: generate it from the accepted model and bounds.
fn check_stalled_stance(offset: [f64; 2]) {
    let model = Model::load(include_bytes!("../../../assets/hand/model.json")).unwrap();
    let mut plan = Plan::load(br#"{
        "version":2,"layout":"dance-double",
        "mat":{"version":1,"pads":2,"pitch_mm":24,"seam_gap_mm":0},
        "duration":0.05,"initial":{"L":6,"R":15},
        "tracks":{"L":[],"R":[]},"events":[],
        "placement":{"tile_pitch_mm":24,"panel_anchor_mm":15,"center_offset_mm":9,"other_offset_mm":5.5}
    }"#).unwrap();
    plan.center_contacts(None);
    let mut q = plan.seed(&model, 0., None);
    let com = model.geometry(&q).com;
    q[..8].copy_from_slice(&model.lo[..8]);
    q[3] = model.hi[3];
    q[5] = 45.;
    q[7] = 8.;
    q[13] = -90.;
    let prior = q;
    for _ in 0..6 {
        refine_contacts(
            &model,
            &mut q,
            &prior,
            &Goal {
                tips: plan.targets(0.).tips,
                com: [com[0] + offset[0], com[1] + offset[1]],
            },
            8,
        );
    }
    assert!(model.geometry(&q).knuckles.iter().any(|h| *h < 40.));
    let curve = ContactCurve::prepare(&model, &plan, &[0., 0.015, 0.035, 0.05], &[q; 4])
        .expect("a bounded balance refit escapes the stalled branch");
    assert!(curve.report.refinement_passes <= 12);
    for i in 0..=400 {
        let t = i as f64 / 8000.;
        let s = curve.sample(t);
        let g = model.geometry(&s.pose);
        for d in 0..2 {
            let error = g.tips[d]
                .iter()
                .zip(plan.targets(t).tips[d])
                .map(|(a, b)| (a - b).powi(2))
                .sum::<f64>()
                .sqrt();
            assert!(error <= 2., "target at {t}: {error}");
            assert!(g.tips[d][2].abs() <= 0.25, "contact at {t}");
            let center = plan.tile_center(plan.panel_at(d, t));
            let margin = 11.5
                - (g.tips[d][0] - center[0])
                    .abs()
                    .max((g.tips[d][1] - center[1]).abs());
            assert!(margin >= 1., "tile at {t}");
        }
        assert!(g.knuckles.iter().all(|h| *h >= 40.), "knuckles at {t}");
        assert!(
            s.pose
                .iter()
                .enumerate()
                .all(|(j, v)| *v >= model.lo[j] && *v <= model.hi[j])
        );
    }
    let joins = curve.join_errors();
    assert!(joins[0] < 1e-8 && joins[1] < 1e-6 && joins[2] < 1e-3);
}

#[test]
fn stalled_wide_stance_can_adjust_balance_without_relaxing_contacts() {
    check_stalled_stance([20., 15.]);
}

#[test]
fn repaired_wide_stance_also_checks_the_interpolated_floor() {
    check_stalled_stance([-20., 0.]);
}
