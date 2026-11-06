// SPDX-FileCopyrightText: 2026 すごいジャン <me@sugoijan.dev>
// SPDX-License-Identifier: MIT
use ddi_hand_pose::{ContactCurve, Model, Plan, Rolling};

#[test]
fn streamed_curve_refines_floor_dips_without_forcing_exact_contacts() {
    let model = Model::load(include_bytes!("../../../assets/hand/model.json")).unwrap();
    let mut plan = Plan::empty(1.);
    plan.initial.left = 4;
    plan.initial.right = 6;
    plan.center_contacts(None);
    let rolling = Rolling::new(&model, plan.clone(), 3.);
    let (mut base, _) = rolling.evaluate(&model, &plan, 0.);
    let low = model
        .geometry(&base)
        .tips
        .iter()
        .map(|p| p[2])
        .fold(f64::INFINITY, f64::min);
    base[10] -= low;
    let mut dipped = base;
    // Low enough to violate both skin-floor and knuckle clearance. A stream
    // refinement must not solve a new IK branch just to raise those points.
    dipped[10] -= 7.;
    let mut lifted = base;
    lifted[10] += 5.;
    let curve = ContactCurve::prepare_relaxed(
        &model,
        &plan,
        &[0., 0.5, 1.],
        &[base, dipped, lifted],
        &[[0., 1.]],
    )
    .unwrap();
    assert!(curve.report.refinement_passes > 0);
    for frame in 0..=4000 {
        let t = frame as f64 / 4000.;
        let q = curve.sample(t).pose;
        for i in 0..14 {
            if i != 10 {
                assert!((q[i] - base[i]).abs() < 1e-8);
            }
        }
        let g = model.geometry(&q);
        assert!(g.patches.iter().all(|p| p[2] >= -0.25), "floor at {t}");
        assert!(g.knuckles.iter().all(|h| *h >= 40.), "knuckle at {t}");
    }
    let joins = curve.join_errors();
    assert!(
        joins[0] < 1e-8 && joins[1] < 1e-6 && joins[2] < 1e-4,
        "{joins:?}"
    );
    // Refining clearance must retain the intended lifted endpoint.
    assert!((curve.sample(1.).pose[10] - lifted[10]).abs() < 1e-8);
}

#[test]
fn exact_vertical_repair_keeps_the_digit_pose() {
    let model = Model::load(include_bytes!("../../../assets/hand/model.json")).unwrap();
    let mut plan = Plan::empty(1.);
    plan.initial.left = 4;
    plan.initial.right = 6;
    plan.center_contacts(None);
    let rolling = Rolling::new(&model, plan.clone(), 3.);
    let (base, _) = rolling.evaluate(&model, &plan, 0.);
    let mut dipped = base;
    dipped[10] -= 0.35;
    let curve = ContactCurve::prepare(&model, &plan, &[0., 1.], &[dipped, dipped]).unwrap();
    assert!(curve.report.refinement_passes > 0);
    for t in [0., 0.25, 0.5, 0.75, 1.] {
        let q = curve.sample(t).pose;
        for i in 0..14 {
            if i != 10 {
                assert!((q[i] - base[i]).abs() < 1e-9, "digit/root angle {i} at {t}");
            }
        }
        let g = model.geometry(&q);
        assert!(
            g.tips
                .iter()
                .all(|tip| tip[2].abs() <= 0.25 && tip[2] >= -0.20)
        );
        assert!(g.knuckles.iter().all(|z| *z >= 40.));
    }
}
