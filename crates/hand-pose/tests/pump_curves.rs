// SPDX-FileCopyrightText: 2026 すごいジャン <me@sugoijan.dev>
// SPDX-License-Identifier: MIT
use ddi_hand_pose::{ContactCurve, Model, Plan, Rolling, correct_contact_pose};

// A short authored corner/support transfer with a 55 ms double-support phase,
// followed by a 70 ms flight. No library chart or sampled dance is embedded.
fn support_transfer() -> Plan {
    Plan::load(br#"{
        "layout":"pump-single", "duration":1.2,
        "initial":{"L":7,"R":3},
        "placement":{"tile_pitch_mm":24,"panel_anchor_mm":15,"center_offset_mm":9,"other_offset_mm":5.5},
        "tracks":{
            "L":[{"release":0.25,"land":0.5,"from":7,"to":4,"lift_m":0.036},
                 {"release":0.555,"land":0.75,"from":4,"to":5,"lift_m":0.034}],
            "R":[{"release":0.36,"land":0.5,"from":3,"to":3,"lift_m":0.035},
                 {"release":0.555,"land":0.625,"from":3,"to":9,"lift_m":0.045}]
        },
        "events":[{"time":0.5,"music_time":0.5,"tick":48,"panel":3,"foot":"R"},
                  {"time":0.625,"music_time":0.625,"tick":60,"panel":9,"foot":"R"},
                  {"time":0.75,"music_time":0.75,"tick":72,"panel":5,"foot":"L"}]
    }"#).unwrap()
}

#[test]
fn corner_support_liftoff_keeps_contacts_and_subframe_clearance() {
    let m = Model::load(include_bytes!("../../../assets/hand/model.json")).unwrap();
    let mut p = support_transfer();
    p.center_contacts(None);
    let mut rolling = Rolling::new(&m, p.clone(), 3.);
    rolling.request(p.clone(), 0.);
    for _ in 0..100_000 {
        if rolling.work_slice(&m, 0., 1) == 0 {
            break;
        }
    }
    let mut times = (0..=144).map(|i| i as f64 / 120.).collect::<Vec<_>>();
    times.extend(&p.edges);
    times.sort_by(f64::total_cmp);
    times.dedup();
    let poses = times
        .iter()
        .map(|&t| {
            let (q, _) = rolling.evaluate(&m, &p, t);
            correct_contact_pose(&m, p.targets(t), q).0
        })
        .collect::<Vec<_>>();
    let curve = ContactCurve::prepare(&m, &p, &times, &poses).unwrap();
    let mut probes = (0..=1200).map(|i| i as f64 / 1000.).collect::<Vec<_>>();
    probes.extend((0..=80).map(|i| 0.55 + i as f64 / 8000.));
    for t in probes {
        let sample = curve.sample(t);
        let g = m.geometry(&sample.pose);
        let target = p.targets(t);
        for d in 0..2 {
            let error = (0..3)
                .map(|j| (g.tips[d][j] - target.tips[d][j]).powi(2))
                .sum::<f64>()
                .sqrt();
            assert!(error <= 2., "error {error} at {t}");
            assert!(g.tips[d][2] >= -0.25, "floor at {t}");
            if target.contact[d] {
                assert!(g.tips[d][2].abs() <= 0.25, "contact at {t}");
            }
        }
        assert!(g.knuckles.iter().all(|h| *h >= 40.), "knuckles at {t}");
        assert!(
            sample
                .velocity
                .iter()
                .chain(&sample.acceleration)
                .all(|v| v.is_finite())
        );
    }
    let join = curve.join_errors();
    assert!(
        join[0] < 1e-8 && join[1] < 1e-6 && join[2] < 1e-3,
        "{join:?}"
    );
}
