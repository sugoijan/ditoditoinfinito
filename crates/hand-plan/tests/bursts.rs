// SPDX-FileCopyrightText: 2026 すごいジャン <me@sugoijan.dev>
// SPDX-License-Identifier: MIT
use ddi_chart::{Note, NoteKind, Tick};
use ddi_hand_plan::{BurstFallback, Input, Options, PlannerJob, PlanningReport, Status};

fn input(bpm: f64, ticks: [i64; 3], mirror: bool) -> Input {
    let mut song = ddi_chart::formats::sm::parse_sm(&format!(
        "#BPMS:0={bpm};#NOTES:danoni-5::Hard:1:0,0,0,0,0:\n10000;"
    ))
    .unwrap();
    let lane = |l: u8| {
        if mirror {
            [3, 1, 2, 0, 4][l as usize]
        } else {
            l
        }
    };
    song.charts[0].notes = vec![
        Note::new(Tick(ticks[0]), lane(0), NoteKind::Tap),
        Note::new(Tick(ticks[1]), lane(4), NoteKind::Tap),
        Note::new(Tick(ticks[2]), lane(1), NoteKind::Tap),
        Note::new(Tick(400), lane(0), NoteKind::HoldHead { end: Tick(448) }),
        Note::new(Tick(480), lane(0), NoteKind::Lift),
        Note::new(Tick(528), lane(1), NoteKind::Mine),
        Note::new(Tick(576), lane(0), NoteKind::RollHead { end: Tick(624) }),
        Note::new(Tick(720), lane(2), NoteKind::Tap),
    ];
    Input::from_song(&song, 0).unwrap()
}
fn solve(input: &Input) -> (Option<ddi_hand_plan::ContactPlan>, PlanningReport) {
    let mut job = PlannerJob::new(input.clone(), Options::default());
    for _ in 0..100_000 {
        if job.step() {
            return job.finish().unwrap();
        }
    }
    panic!("planner did not finish");
}
#[test]
fn one_weak_tap_can_be_removed_after_failure_without_touching_other_events() {
    for mirror in [false, true] {
        let input = input(120., [192, 193, 194], mirror);
        let original = input.effective_notes.clone();
        let (plan, report) = solve(&input);
        assert!(plan.is_none());
        assert_eq!(report.status, Status::ContactFailure);
        assert_eq!(report.failure_tick, Some(194));
        let policy = BurstFallback::default();
        let proposals = policy.candidates(&input, &report, f64::NEG_INFINITY);
        assert_eq!(proposals.len(), 3);
        assert_eq!((proposals[0].tick, proposals[0].lane), (193, 4));
        let reduced = policy
            .reduced_input(&input, &report, f64::NEG_INFINITY, proposals[0])
            .unwrap();
        assert_eq!(input.effective_notes, original);
        let expected = original
            .iter()
            .filter(|n| !(n.tick.0 == 193 && n.lane == 4))
            .cloned()
            .collect::<Vec<_>>();
        assert_eq!(reduced.effective_notes, expected);
        for n in &reduced.notes {
            let old = input
                .notes
                .iter()
                .find(|old| old.tick == n.tick && old.lane == n.lane)
                .unwrap();
            assert_eq!(old.time, n.time);
            assert_eq!(old.end, n.end);
        }
        let (plan, report) = solve(&reduced);
        let plan = plan.unwrap_or_else(|| panic!("{report:?}"));
        assert!(ddi_hand_plan::replay(&reduced, &plan).unwrap().passed);
    }
}
#[test]
fn a_solvable_tight_burst_has_no_reduction_candidates() {
    for mirror in [false, true] {
        let input = input(130., [192, 195, 198], mirror);
        let (plan, report) = solve(&input);
        assert!(plan.is_some(), "{report:?}");
        assert!(
            BurstFallback::default()
                .candidates(&input, &report, f64::NEG_INFINITY)
                .is_empty()
        );
        assert!(report.failure_tick.is_none());
    }
}
#[test]
fn the_gate_has_a_precise_time_boundary_and_never_offers_committed_taps() {
    let failing = input(120., [192, 193, 194], false);
    let (_, mut report) = solve(&failing);
    // Isolate eligibility from feasibility with a completed failure report.
    report.failure_tick = Some(198);
    let at_boundary = input(125., [192, 195, 198], false);
    let outside = input(124.99, [192, 195, 198], false);
    let policy = BurstFallback::default();
    assert_eq!(
        policy
            .candidates(&at_boundary, &report, f64::NEG_INFINITY)
            .len(),
        3
    );
    assert!(
        policy
            .candidates(&outside, &report, f64::NEG_INFINITY)
            .is_empty()
    );
    assert_eq!(
        BurstFallback::new(75)
            .unwrap()
            .candidates(&outside, &report, f64::NEG_INFINITY)
            .len(),
        3
    );
    let through = at_boundary.notes[2].time;
    assert!(policy.candidates(&at_boundary, &report, through).is_empty());
    let choices = policy.candidates(&at_boundary, &report, at_boundary.notes[1].time);
    assert_eq!(choices.len(), 1);
    assert_eq!(choices[0].tick, 198);
    let mut protected = choices[0];
    protected.tick = 400;
    protected.lane = 0;
    assert!(
        policy
            .reduced_input(&at_boundary, &report, f64::NEG_INFINITY, protected)
            .is_err()
    );
    assert!(BurstFallback::new(0).is_err());
    assert!(BurstFallback::new(91).is_err());
}
#[test]
fn validation_failures_timeouts_and_two_tile_trills_are_not_simplified() {
    let mut input = input(120., [192, 193, 194], false);
    let (_, mut report) = solve(&input);
    for status in [
        Status::ContactJudgeFailure,
        Status::Unsupported,
        Status::ContactTimeout,
    ] {
        report.status = status;
        assert!(
            BurstFallback::default()
                .candidates(&input, &report, f64::NEG_INFINITY)
                .is_empty()
        );
    }
    report.status = Status::ContactFailure;
    input.notes[1].lane = input.notes[0].lane;
    input.effective_notes[1].lane = input.effective_notes[0].lane;
    assert!(
        BurstFallback::default()
            .candidates(&input, &report, f64::NEG_INFINITY)
            .is_empty()
    );
}

#[test]
fn a_later_timeout_clears_an_earlier_rejected_row() {
    let input = input(120., [192, 193, 194], false);
    let mut job = PlannerJob::new(input, Options::default());
    for _ in 0..100_000 {
        job.step();
        if job.report.failure_tick.is_some() {
            break;
        }
    }
    assert!(job.report.failure_tick.is_some());
    job.charge_search(5.);
    let (_, report) = job.finish().unwrap();
    assert_eq!(report.status, Status::ContactTimeout);
    assert!(report.failure_tick.is_none());
}
