// SPDX-FileCopyrightText: 2026 すごいジャン <me@sugoijan.dev>
// SPDX-License-Identifier: MIT
use ddi_chart::Tick;
use ddi_hand_plan::{Input, RollingPlanner};
use ddi_hand_pose::{DanceRuntime, Model, Plan};
pub fn check(model: &Model, input: Input, kind: &str, mirror: bool) {
    // Rapid notes exercise the production 60 Hz arrival cadence; the older
    // quarter-second studies retain their deliberately coarse window updates.
    let hz = if kind == "staggered" { 60 } else { 4 };
    check_at_hz(model, input, kind, mirror, hz);
}
pub fn check_at_hz(model: &Model, input: Input, kind: &str, mirror: bool, hz: i32) {
    const DURATION: f64 = 14.;
    let mut planner =
        RollingPlanner::configured(3., input.timing.clone(), &input.layout, input.rules.clone())
            .unwrap();
    let mut cursor = 0;
    let mut runtime: Option<DanceRuntime> = None;
    let mut previous: Option<Plan> = None;
    let mut certified = 0.;
    let mut received = 0.;
    let mut checked = 0.;
    let mut checked_hits = 0;
    for frame in 0..=14 * hz {
        let now = frame as f64 / f64::from(hz);
        let known = (now + 3.).min(DURATION);
        let start = cursor;
        while cursor < input.effective_notes.len()
            && input.timing.seconds_at(input.effective_notes[cursor].tick) <= known
        {
            cursor += 1;
        }
        planner
            .push(
                now,
                &input.effective_notes[start..cursor],
                known,
                cursor == input.effective_notes.len(),
            )
            .unwrap();
        planner.request(now);
        let mut checks = 0;
        while planner.step() {
            checks += 1;
            assert!(checks < 1_000_000);
        }
        assert!(
            planner.report.failure.is_none(),
            "{kind} {mirror}: {:?}",
            planner.report
        );
        if let Some(p) = planner.take_publication() {
            let mut pose = p.plan.pose_plan().unwrap();
            pose.center_contacts(previous.as_ref().map(|old| (old, certified)));
            pose.duration = DURATION;
            pose.rebuild_edges();
            runtime.get_or_insert_with(|| DanceRuntime::new(model, &pose, 3.).unwrap());
            received = p.known_until;
            certified = p.certified_until;
            previous = Some(pose);
        }
        // Motion still advances after the final contact publication. Gameplay
        // also requests its retained plan every frame through the audio tail.
        if let (Some(pose), Some(runtime)) = (&previous, &mut runtime) {
            let prefix = if checked >= now {
                runtime.sample(checked)
            } else {
                None
            };
            // All notes received is not the end of the extended motion. Match
            // gameplay: close stream fitting only once the full tail is visible.
            runtime.request_bounded(pose, now, received, certified, received >= pose.duration);
            let mut steps = 0;
            while runtime.step(model, now) {
                steps += 1;
                assert!(steps < 100_000);
            }
            assert!(
                runtime.report.failure.is_none(),
                "{kind} mirror={mirror} at {now}: {:?}",
                runtime.report.failure
            );
            if let Some(prefix) = prefix {
                let after = runtime.sample(checked).unwrap();
                assert_eq!(prefix.pose.map(f64::to_bits), after.pose.map(f64::to_bits));
                for (a, b) in prefix.velocity.into_iter().zip(after.velocity) {
                    assert!((a - b).abs() < 1e-6);
                }
                for (a, b) in prefix.acceleration.into_iter().zip(after.acceleration) {
                    assert!((a - b).abs() < 1e-3);
                }
            }
            let end = (now + 0.5).min(certified).min(DURATION);
            if end > checked {
                let mut probes = vec![end];
                probes.extend(
                    ((checked * 120.).ceil() as usize..=(end * 120.).floor() as usize)
                        .map(|n| n as f64 / 120.),
                );
                for edge in &pose.edges {
                    probes.extend(
                        (-8..=8)
                            .map(|i| edge + f64::from(i) / 8000.)
                            .filter(|t| *t >= checked && *t <= end),
                    );
                }
                for t in probes {
                    let s = runtime
                        .sample(t)
                        .expect("prepared pose through certified horizon");
                    let g = model.geometry(&s.pose);
                    let targets = pose.targets(t);
                    // The dense burst uses the accepted stream policy between
                    // arrows. Exact scored instants retain all contact limits;
                    // the older slow studies remain strict at every sample.
                    let stream = matches!(kind, "staggered" | "staggered-tail")
                        && runtime.stream_at(t)
                        && !pose.events.iter().any(|e| e.time == t);
                    for d in 0..2 {
                        let error = (0..3)
                            .map(|j| (g.tips[d][j] - targets.tips[d][j]).powi(2))
                            .sum::<f64>()
                            .sqrt();
                        assert!(
                            stream || error <= 2.,
                            "{kind} {mirror} at {t} finger {d}: error {error}"
                        );
                        assert!(g.tips[d][2] >= -0.25, "{kind} {mirror} at {t}: floor");
                        if targets.contact[d] && !stream {
                            assert!(
                                g.tips[d][2].abs() <= 0.25,
                                "{kind} {mirror} at {t}: height {}",
                                g.tips[d][2]
                            );
                            let c = pose.tile_center(pose.panel_at(d, t));
                            let margin =
                                11.5 - (g.tips[d][0] - c[0]).abs().max((g.tips[d][1] - c[1]).abs());
                            assert!(margin >= 1., "{kind} {mirror} at {t}: margin {margin}");
                        }
                    }
                    assert!(g.knuckles.iter().all(|h| *h >= 40.));
                    assert!(
                        s.pose
                            .iter()
                            .enumerate()
                            .all(|(i, q)| *q >= model.lo[i] && *q <= model.hi[i])
                    );
                    assert!(
                        s.velocity
                            .iter()
                            .chain(&s.acceleration)
                            .all(|v| v.is_finite())
                    );
                }
                for e in pose
                    .events
                    .iter()
                    .filter(|e| e.time > checked && e.time <= end)
                {
                    let d = usize::from(e.foot == "R");
                    assert_ne!(
                        pose.targets(e.time - 1e-6).contact[d],
                        pose.targets(e.time).contact[d]
                    );
                    assert_eq!(pose.targets(e.time).contact[d], !e.lift);
                    assert_eq!(e.music_time, input.timing.seconds_at(Tick(e.tick)));
                    checked_hits += 1;
                }
                checked = end;
            }
        }
    }
    assert_eq!(checked, DURATION, "full motion and quiet tail checked");
    let runtime = runtime.unwrap();
    assert_eq!(runtime.report.covered_until, DURATION);
    assert!(runtime.sample(DURATION).is_some());
    assert_eq!(runtime.report.guide_late_jobs, 0);
    assert_eq!(
        checked_hits,
        input
            .notes
            .iter()
            .filter(|n| n.judgeable && !n.mine)
            .count()
    );
}
