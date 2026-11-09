// SPDX-FileCopyrightText: 2026 すごいジャン <me@sugoijan.dev>
// SPDX-License-Identifier: MIT
use ddi_hand_pose::Plan;
use serde_json::{Value, json};
fn study() -> Value {
    json!({
        "version":2,"layout":"dance-double", "mat":{"version":1,"pads":2,"pitch_mm":24.,"seam_gap_mm":0.},
        "duration":2.,"initial":{"L":6,"R":13},"tracks":{"L":[],"R":[]},"events":[],
        "placement":{"tile_pitch_mm":24.,"panel_anchor_mm":15.,"center_offset_mm":9.,"other_offset_mm":5.5}
    })
}
fn load(v: &Value) -> Result<Plan, String> {
    Plan::load(&serde_json::to_vec(v).unwrap())
}
#[test]
fn doubled_ids_require_explicit_versioned_geometry() {
    let v = study();
    let p = load(&v).unwrap();
    assert_eq!(p.tile_center(6), [-28., 0., 0.]);
    assert_eq!(p.tile_center(13), [-4., 0., 0.]);
    assert_ne!(p.tile_center(5), p.tile_center(14));
    for (key, value) in [
        ("version", json!(1)),
        ("version", json!(3)),
        ("mat", Value::Null),
        ("layout", json!("dance-single")),
    ] {
        let mut bad = v.clone();
        bad[key] = value;
        assert!(load(&bad).is_err(), "{bad}");
    }
    let mut bad = v.clone();
    bad["mat"]["pitch_mm"] = json!(20.);
    assert!(load(&bad).is_err());
    let mut bad = v.clone();
    bad["initial"]["R"] = json!(19);
    assert!(load(&bad).is_err());
    let mut legacy = v.clone();
    legacy.as_object_mut().unwrap().remove("mat");
    legacy.as_object_mut().unwrap().remove("version");
    legacy["initial"]["R"] = json!(9);
    assert!(
        load(&legacy).is_err(),
        "a named double layout requires version 2"
    );
    legacy["layout"] = json!("dance-single");
    let p = load(&legacy).unwrap();
    assert_eq!(p.tile_center(9), [8., 24., 0.]);
    legacy["initial"]["R"] = json!(13);
    assert!(load(&legacy).is_err());
}
#[test]
fn centered_contacts_and_reuse_keep_physical_geometry_identity() {
    let mut p = load(&study()).unwrap();
    p.center_contacts(None);
    assert_eq!(p.targets(0.).tips, [[-28., 0., 0.], [-4., 0., 0.]]);
    let mut v = study();
    v["mat"]["seam_gap_mm"] = json!(4.);
    let mut next = load(&v).unwrap();
    assert!(!next.same_geometry(&p));
    next.center_contacts(Some((&p, 1.)));
    assert_eq!(next.targets(0.).tips, [[-30., 0., 0.], [-2., 0., 0.]]);
    let mut v = study();
    v["initial"] = json!({"L":14,"R":14});
    let mut p = load(&v).unwrap();
    p.center_contacts(None);
    assert_eq!(p.targets(0.).tips, [[11., 0., 0.], [29., 0., 0.]]);
}

#[test]
fn runtime_refuses_geometry_changes_before_reusing_a_publication() {
    let m = ddi_hand_pose::Model::load(include_bytes!("../../../assets/hand/model.json")).unwrap();
    let p = load(&study()).unwrap();
    let mut runtime = ddi_hand_pose::DanceRuntime::new(&m, &p, 3.).unwrap();
    let mut v = study();
    v["mat"]["seam_gap_mm"] = json!(4.);
    runtime.request(&load(&v).unwrap(), 0.);
    assert_eq!(
        runtime.report.failure.as_ref().unwrap().reason,
        "geometry_changed"
    );
    assert!(!runtime.step(&m, 0.));
}

#[test]
fn crossing_wide_jump_and_held_support_keep_contacts_and_publications() {
    use ddi_hand_pose::{DanceRuntime, Model};
    let model = Model::load(include_bytes!("../../../assets/hand/model.json")).unwrap();
    let swing = |a: f64, b: f64, from: u8, to: u8| json!({"release":a,"land":b,"from":from,"to":to,"lift_m":0.014});
    for (left, right, initial) in [
        (
            vec![
                swing(0.6, 1.2, 4, 6),
                swing(2., 2.6, 6, 13),
                swing(4.2, 4.8, 13, 17),
            ],
            vec![
                swing(1.3, 1.9, 8, 13),
                swing(3., 3.6, 13, 17),
                swing(5., 5.6, 17, 15),
            ],
            [4, 8],
        ),
        (
            vec![swing(0.7, 1.6, 9, 5), swing(2.5, 3.4, 5, 16)],
            vec![swing(0.7, 1.6, 3, 14), swing(2.5, 3.4, 14, 12)],
            [9, 3],
        ),
        (
            vec![],
            vec![
                swing(0.7, 1.4, 8, 13),
                swing(2., 2.7, 13, 17),
                swing(3.3, 4., 17, 11),
                swing(4.6, 5.3, 11, 8),
            ],
            [6, 8],
        ),
    ] {
        let mut data = study();
        data["duration"] = json!(6.);
        if initial == [9, 3] {
            data["layout"] = json!("pump-double");
        }
        data["initial"] = json!({"L":initial[0],"R":initial[1]});
        data["tracks"] = json!({"L":left,"R":right});
        for (mirror, swap) in [(false, false), (true, false), (false, true), (true, true)] {
            let mut data = data.clone();
            if mirror {
                let mirrored = |id: u64| {
                    let pad = (id - 1) / 9;
                    let cell = (id - 1) % 9;
                    (1 - pad) * 9 + cell / 3 * 3 + (2 - cell % 3) + 1
                };
                for foot in ["L", "R"] {
                    data["initial"][foot] =
                        json!(mirrored(data["initial"][foot].as_u64().unwrap()));
                    for swing in data["tracks"][foot].as_array_mut().unwrap() {
                        for key in ["from", "to"] {
                            swing[key] = json!(mirrored(swing[key].as_u64().unwrap()));
                        }
                    }
                }
            }
            if swap {
                for key in ["initial", "tracks"] {
                    let a = data[key]["L"].take();
                    let b = data[key]["R"].take();
                    data[key]["L"] = b;
                    data[key]["R"] = a;
                }
            }
            let mut events = Vec::new();
            for foot in ["L", "R"] {
                for swing in data["tracks"][foot].as_array().unwrap() {
                    events.push(json!({"foot":foot,"time":swing["land"],"music_time":swing["land"],"tick":0,"panel":swing["to"]}));
                }
            }
            events.sort_by(|a, b| {
                a["time"]
                    .as_f64()
                    .unwrap()
                    .total_cmp(&b["time"].as_f64().unwrap())
            });
            data["events"] = json!(events);
            let mut plan = load(&data).unwrap();
            plan.center_contacts(None);
            let mut runtime = DanceRuntime::new(&model, &plan, 3.).unwrap();
            let mut checked_events = 0;
            let mut checked_until = 0.;
            for frame in 0..=120 {
                let t = frame as f64 / 20.;
                let prefix = runtime.sample((t + 0.2).min(6.));
                runtime.request(&plan, t);
                let mut checkpoints = 0;
                while runtime.step(&model, t) {
                    checkpoints += 1;
                    assert!(checkpoints < 100_000);
                }
                assert!(
                    runtime.report.failure.is_none(),
                    "{:?}",
                    runtime.report.failure
                );
                if let Some(before) = prefix {
                    let after = runtime.sample((t + 0.2).min(6.)).unwrap();
                    for (a, b) in before.velocity.iter().zip(after.velocity) {
                        assert!((a - b).abs() < 1e-6);
                    }
                    for (a, b) in before.acceleration.iter().zip(after.acceleration) {
                        assert!((a - b).abs() < 1e-3);
                    }
                    assert_eq!(
                        before.pose.map(f64::to_bits),
                        runtime
                            .sample((t + 0.2).min(6.))
                            .unwrap()
                            .pose
                            .map(f64::to_bits)
                    );
                }
                if frame % 25 == 0 && checked_until < 6. {
                    let end = runtime.report.covered_until.min(6.);
                    let mut probes = ((checked_until * 1000.) as usize..=(end * 1000.) as usize)
                        .map(|i| i as f64 / 1000.)
                        .collect::<Vec<_>>();
                    for start in [0.53, 1.27, 3.55] {
                        probes.extend(
                            (0..=800)
                                .map(|i| start + i as f64 / 8000.)
                                .filter(|v| *v >= checked_until && *v <= end),
                        );
                    }
                    for edge in &plan.edges {
                        probes.extend(
                            (-16..=16)
                                .map(|i| edge + f64::from(i) / 8000.)
                                .filter(|v| *v >= checked_until && *v <= end),
                        );
                    }
                    checked_until = end;
                    for u in probes {
                        let sample = runtime.sample(u).unwrap();
                        let g = model.geometry(&sample.pose);
                        let target = plan.targets(u);
                        if initial == [4, 8] {
                            // Protect this authored study against moving the
                            // snap elsewhere while smoothing the shared entry.
                            assert!(
                                sample.velocity[13].abs() <= 900.,
                                "turn spike at {u}: {}",
                                sample.velocity[13]
                            );
                        }
                        if initial == [4, 8] && (2. ..=2.6).contains(&u) {
                            assert!(
                                sample.velocity[13].abs() <= 360.,
                                "shared-tile turn spikes at {u}: {}",
                                sample.velocity[13]
                            );
                        }
                        for d in 0..2 {
                            let error = (0..3)
                                .map(|j| (g.tips[d][j] - target.tips[d][j]).powi(2))
                                .sum::<f64>()
                                .sqrt();
                            assert!(error <= 2., "subframe position at {u}: {error}");
                            assert!(g.tips[d][2] >= -0.25, "subframe floor at {u}");
                            if target.contact[d] {
                                assert!(g.tips[d][2].abs() <= 0.25, "subframe contact at {u}");
                                let center = plan.tile_center(plan.panel_at(d, u));
                                let margin = 11.5
                                    - (g.tips[d][0] - center[0])
                                        .abs()
                                        .max((g.tips[d][1] - center[1]).abs());
                                assert!(margin >= 1., "subframe tile margin at {u}: {margin}");
                            }
                        }
                        assert!(g.knuckles.iter().all(|h| *h >= 40.));
                        assert!(
                            sample
                                .pose
                                .iter()
                                .enumerate()
                                .all(|(i, q)| *q >= model.lo[i] && *q <= model.hi[i])
                        );
                        assert!(
                            sample
                                .velocity
                                .iter()
                                .chain(&sample.acceleration)
                                .all(|v| v.is_finite())
                        );
                    }
                }
                for event in plan.events.iter().filter(|e| (e.time - t).abs() < 1e-9) {
                    let d = event.foot.index();
                    assert!(!plan.targets(event.time - 1e-6).contact[d]);
                    assert!(plan.targets(event.time).contact[d]);
                    assert_eq!(plan.panel_at(d, event.time), event.panel);
                    assert_eq!(event.music_time, event.time);
                    let at = runtime.sample(event.time).unwrap();
                    let tip = model.geometry(&at.pose).tips[d];
                    assert!(tip[2].abs() <= 0.25);
                    checked_events += 1;
                }
                let sample = runtime.sample(t).expect("prepared study pose");
                let g = model.geometry(&sample.pose);
                let target = plan.targets(t);
                for (d, actual) in g.tips.iter().enumerate() {
                    let error = (0..3)
                        .map(|j| (actual[j] - target.tips[d][j]).powi(2))
                        .sum::<f64>()
                        .sqrt();
                    assert!(error <= 2., "position at {t}: {error}");
                    if target.contact[d] {
                        assert!(actual[2].abs() <= 0.25, "height at {t}");
                        let center = plan.tile_center(plan.panel_at(d, t));
                        let margin = 11.5
                            - (actual[0] - center[0])
                                .abs()
                                .max((actual[1] - center[1]).abs());
                        assert!(margin >= 1., "margin at {t}: {margin}");
                    }
                }
                assert!(g.knuckles.iter().all(|h| *h >= 40.));
            }
            assert!(runtime.report.covered_until >= 6.);
            assert_eq!(checked_until, 6.);
            assert_eq!(checked_events, plan.events.len());
            let joins = runtime.report.maximum_join_errors;
            assert!(
                joins[0] < 1e-8 && joins[1] < 1e-6 && joins[2] < 1e-3,
                "{joins:?}"
            );
        }
    }
}
