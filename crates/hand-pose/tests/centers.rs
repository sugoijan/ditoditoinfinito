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
