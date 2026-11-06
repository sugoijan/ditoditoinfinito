// SPDX-FileCopyrightText: 2026 すごいジャン <me@sugoijan.dev>
// SPDX-License-Identifier: MIT
use ddi_hand_pose::{Model, Plan, Rolling, correct_contact_pose};

#[test]
fn centered_extreme_stances_remain_within_existing_model_bounds() {
    let m = Model::load(include_bytes!("../../../assets/hand/model.json")).unwrap();
    for (left, right) in [
        (4, 6),
        (2, 8),
        (8, 2),
        (4, 8),
        (2, 6),
        (5, 8),
        (4, 5),
        (5, 5),
    ] {
        let mut p = Plan::empty(4.);
        p.initial.left = left;
        p.initial.right = right;
        p.center_contacts(None);
        let r = Rolling::new(&m, p.clone(), 3.);
        let (q, _) = r.evaluate(&m, &p, 0.);
        let (q, _) = correct_contact_pose(&m, p.targets(0.), q);
        for (tip, target) in m.geometry(&q).tips.iter().zip(p.targets(0.).tips) {
            let error = (0..3)
                .map(|j| (tip[j] - target[j]).powi(2))
                .sum::<f64>()
                .sqrt();
            assert!(
                error <= 2. && tip[2].abs() <= 0.25,
                "{left}/{right}: {tip:?} vs {target:?}"
            );
        }
        assert!(
            q.iter()
                .enumerate()
                .all(|(i, v)| *v >= m.lo[i] && *v <= m.hi[i])
        );
    }
}

#[test]
fn solo_corner_stances_have_checked_continuous_contact_curves() {
    let model = Model::load(include_bytes!("../../../assets/hand/model.json")).unwrap();
    for (left, right) in [(7, 9), (7, 2), (2, 9), (7, 5), (5, 9)] {
        let mut plan = Plan::empty(1.);
        plan.initial.left = left;
        plan.initial.right = right;
        plan.center_contacts(None);
        let rolling = Rolling::new(&model, plan.clone(), 3.);
        let (q, _) = rolling.evaluate(&model, &plan, 0.);
        let (q, _) = correct_contact_pose(&model, plan.targets(0.), q);
        let curve = ddi_hand_pose::ContactCurve::prepare(&model, &plan, &[0., 1.], &[q, q])
            .unwrap_or_else(|e| panic!("{left}/{right}: {e:?}"));
        for t in [0., 0.25, 0.5, 0.75, 1.] {
            let sample = curve.sample(t);
            let tips = model.geometry(&sample.pose).tips;
            for (actual, target) in tips.iter().zip(plan.targets(t).tips) {
                let error = (0..3)
                    .map(|i| (actual[i] - target[i]).powi(2))
                    .sum::<f64>()
                    .sqrt();
                assert!(
                    error <= 2. && actual[2].abs() <= 0.25,
                    "{left}/{right} at {t}: {actual:?}"
                );
            }
        }
    }
}

#[test]
fn pump_corner_stances_preserve_contact_and_model_bounds() {
    let model = Model::load(include_bytes!("../../../assets/hand/model.json")).unwrap();
    for (left, right) in [
        (1, 3),
        (7, 9),
        (1, 7),
        (9, 3),
        (1, 9),
        (7, 3),
        (1, 5),
        (5, 3),
        (5, 5),
        (4, 6),
        (6, 4),
        (9, 1),
        (3, 7),
    ] {
        let mut plan = Plan::empty(1.);
        plan.initial.left = left;
        plan.initial.right = right;
        plan.center_contacts(None);
        let rolling = Rolling::new(&model, plan.clone(), 3.);
        let (q, _) = rolling.evaluate(&model, &plan, 0.);
        let (q, _) = correct_contact_pose(&model, plan.targets(0.), q);
        let g = model.geometry(&q);
        assert!(
            g.knuckles.iter().all(|z| *z >= 40.),
            "{left}/{right}: low knuckles {:?}",
            g.knuckles
        );
        let tips = g.tips;
        for (tip, target) in tips.iter().zip(plan.targets(0.).tips) {
            let error = (0..3)
                .map(|j| (tip[j] - target[j]).powi(2))
                .sum::<f64>()
                .sqrt();
            assert!(
                error <= 2. && tip[2].abs() <= 0.25,
                "{left}/{right}: {tip:?} vs {target:?}, error {error}"
            );
        }
        assert!(
            q.iter()
                .enumerate()
                .all(|(i, v)| *v >= model.lo[i] && *v <= model.hi[i])
        );
    }
}
