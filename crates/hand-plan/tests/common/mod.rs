// SPDX-FileCopyrightText: 2026 すごいジャン <me@sugoijan.dev>
// SPDX-License-Identifier: MIT
// Each test binary uses some of these.
#![allow(dead_code)]
use ddi_chart::Tick;
use ddi_hand_plan::{ContactPlan, Input, Options, PlannerJob, PlanningReport};
pub mod double_motion;

/// The whole-chart plan of `input`, which the judges' replay must pass, and
/// its report.
pub fn planned(input: &Input) -> (ContactPlan, PlanningReport) {
    let mut job = PlannerJob::new(input.clone(), Options::default());
    for _ in 0..1_000_000 {
        if job.step() {
            break;
        }
    }
    let (plan, report) = job.finish().expect("bounded planner completed");
    let plan = plan.unwrap_or_else(|| panic!("{report:?}"));
    assert!(ddi_hand_plan::replay(input, &plan).unwrap().passed);
    (plan, report)
}
/// [`planned`]'s plan alone.
pub fn plan(input: &Input) -> ContactPlan {
    planned(input).0
}
/// [`plan`] on two pads: an explicit version 2 mat, a pose plan made of it
/// and every event at its music time.
pub fn double_plan(input: &Input) -> ContactPlan {
    let p = plan(input);
    assert_eq!(p.version, 2);
    assert_eq!(p.mat.unwrap().pads(), 2);
    p.pose_plan().unwrap();
    for e in &p.events {
        assert_eq!(e.music_time, input.timing.seconds_at(Tick(e.tick)));
    }
    p
}
