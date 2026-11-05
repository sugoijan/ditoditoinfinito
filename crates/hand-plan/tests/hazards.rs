// SPDX-FileCopyrightText: 2026 すごいジャン <me@sugoijan.dev>
// SPDX-License-Identifier: MIT
use ddi_chart::{Note, NoteKind, Tick};
use ddi_hand_plan::{ContactPlan, Input, Options, PlannerJob, RestoreKind, RollingPlanner};

fn input(layout: &str, mirror: bool, hazards: &[i64]) -> Input {
    let mut song = ddi_chart::formats::sm::parse_sm(&format!(
        "#BPMS:0=120;#NOTES:{layout}::Hard:1:0,0,0,0,0:\n1000{};",
        if layout == "danoni-5" { "0" } else { "" }
    ))
    .unwrap();
    let lane = if mirror { 3 } else { 0 };
    song.charts[0].notes = vec![Note::new(Tick(96), lane, NoteKind::Tap)];
    song.charts[0].notes.extend(
        hazards
            .iter()
            .map(|&tick| Note::new(Tick(tick), lane, NoteKind::Mine)),
    );
    song.charts[0]
        .notes
        .push(Note::new(Tick(384), lane, NoteKind::Tap));
    Input::from_song(&song, 0).unwrap()
}

fn check_clearance(input: &Input, plan: &ContactPlan) {
    let mut pose = plan.pose_plan().unwrap();
    pose.center_contacts(None);
    for mine in input.notes.iter().filter(|n| n.mine && n.judgeable) {
        let time = mine.time - plan.source_music_origin_seconds;
        let (x, y) =
            [(-40., 0.), (-16., -24.), (-16., 24.), (8., 0.), (-16., 0.)][mine.lane as usize];
        for t in (-43..=43)
            .map(|k| time + f64::from(k) / 480.)
            .chain([time - 0.09, time + 0.09])
        {
            assert!(
                pose.targets(t)
                    .tips
                    .iter()
                    .all(|p| p[2] > 0.25 || (p[0] - x).abs() > 11.5 || (p[1] - y).abs() > 11.5),
                "low airborne target at {t}"
            );
        }
    }
}

#[test]
fn repeated_mines_keep_a_safe_escape_without_forcing_repeated_landings() {
    // Three overlapping hazard flights, with an exact scored return later.
    for mirror in [false, true] {
        let input = input("dance-single", mirror, &[192, 210, 228]);
        let mut job = PlannerJob::new(input.clone(), Options::default());
        for _ in 0..100_000 {
            if job.step() {
                break;
            }
        }
        let (plan, report) = job.finish().unwrap();
        let plan = plan.unwrap_or_else(|| panic!("{report:?}"));
        assert!(ddi_hand_plan::replay(&input, &plan).unwrap().passed);
        check_clearance(&input, &plan);
        assert!(
            plan.aux_contacts
                .iter()
                .flatten()
                .any(|a| a.kind == RestoreKind::MineAvoid && a.panel == 5)
        );
        assert_eq!(plan.events.len(), 2);
        for event in &plan.events {
            assert_eq!(event.music_time, input.timing.seconds_at(Tick(event.tick)));
        }
        let mut planner = RollingPlanner::new(3., input.timing.clone()).unwrap();
        let mut cursor = 0;
        for frame in 0..32 {
            let now = frame as f64 / 4.;
            let known = now + 3.;
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
            for _ in 0..100_000 {
                if !planner.step() {
                    break;
                }
            }
            assert!(planner.report.failure.is_none(), "{:?}", planner.report);
            planner.take_publication();
        }
    }
}

#[test]
fn five_key_hazards_still_use_air_time_without_a_safe_center() {
    for mirror in [false, true] {
        let input = input("danoni-5", mirror, &[192]);
        let mut job = PlannerJob::new(input.clone(), Options::default());
        for _ in 0..100_000 {
            if job.step() {
                break;
            }
        }
        let (plan, report) = job.finish().unwrap();
        let plan = plan.unwrap_or_else(|| panic!("{report:?}"));
        assert!(ddi_hand_plan::replay(&input, &plan).unwrap().passed);
        check_clearance(&input, &plan);
        assert!(
            plan.aux_contacts
                .iter()
                .flatten()
                .filter(|a| a.kind == RestoreKind::MineAvoid)
                .all(|a| a.panel != 5)
        );
    }
}

#[test]
fn roll_tail_transfers_include_the_refresh_contacts_in_rapid_recovery() {
    use ddi_chart::Song;
    let mut song: Song = ddi_chart::formats::sm::parse_sm(
        "#BPMS:0=120;#NOTES:dance-single::Hard:1:0,0,0,0,0:\n1000;",
    )
    .unwrap();
    for mirror in [false, true] {
        let lane = |l: u8| if mirror { [3, 1, 2, 0][l as usize] } else { l };
        song.charts[0].notes = vec![
            Note::new(Tick(96), lane(1), NoteKind::RollHead { end: Tick(141) }),
            Note::new(Tick(96), lane(3), NoteKind::Tap),
            Note::new(Tick(141), lane(0), NoteKind::RollHead { end: Tick(153) }),
            Note::new(Tick(165), lane(1), NoteKind::Tap),
            Note::new(Tick(189), lane(2), NoteKind::Tap),
        ];
        let input = Input::from_song(&song, 0).unwrap();
        let mut job = PlannerJob::new(input.clone(), Options::default());
        for _ in 0..100_000 {
            if job.step() {
                break;
            }
        }
        let (plan, report) = job.finish().unwrap();
        let plan = plan.unwrap_or_else(|| panic!("{report:?}"));
        assert!(ddi_hand_plan::replay(&input, &plan).unwrap().passed);
        check_clearance(&input, &plan);
    }
}

#[test]
fn a_companion_can_start_mine_clearance_before_the_hazard_row() {
    for mirror in [false, true] {
        let lane = |l: u8| if mirror { [3, 1, 2, 0][l as usize] } else { l };
        let mut song = ddi_chart::formats::sm::parse_sm(
            "#BPMS:0=120;#NOTES:dance-single::Hard:1:0,0,0,0,0:\n1000;",
        )
        .unwrap();
        song.charts[0].notes = vec![
            Note::new(Tick(192), lane(0), NoteKind::Tap),
            Note::new(Tick(208), lane(3), NoteKind::Tap),
            Note::new(Tick(224), lane(1), NoteKind::Tap),
            Note::new(Tick(232), lane(0), NoteKind::Mine),
            Note::new(Tick(232), lane(3), NoteKind::Mine),
            Note::new(Tick(248), lane(2), NoteKind::Tap),
        ];
        let input = Input::from_song(&song, 0).unwrap();
        let mut job = PlannerJob::new(input.clone(), Options::default());
        for _ in 0..100_000 {
            if job.step() {
                break;
            }
        }
        let (plan, report) = job.finish().unwrap();
        let plan = plan.unwrap_or_else(|| panic!("{report:?}"));
        assert!(ddi_hand_plan::replay(&input, &plan).unwrap().passed);
        check_clearance(&input, &plan);
    }
}

#[test]
fn an_unneeded_short_roll_refresh_does_not_block_the_next_scored_transfer() {
    for mirror in [false, true] {
        let lane = |l: u8| if mirror { [3, 1, 2, 0][l as usize] } else { l };
        let mut song = ddi_chart::formats::sm::parse_sm(
            "#BPMS:0=120;#NOTES:dance-single::Hard:1:0,0,0,0,0:\n1000;",
        )
        .unwrap();
        song.charts[0].notes = vec![
            Note::new(Tick(192), lane(0), NoteKind::RollHead { end: Tick(212) }),
            Note::new(Tick(192), lane(3), NoteKind::HoldHead { end: Tick(288) }),
            Note::new(Tick(212), lane(2), NoteKind::Tap),
        ];
        let input = Input::from_song(&song, 0).unwrap();
        let mut job = PlannerJob::new(input.clone(), Options::default());
        for _ in 0..100_000 {
            if job.step() {
                break;
            }
        }
        let (plan, report) = job.finish().unwrap();
        let plan = plan.unwrap_or_else(|| panic!("{report:?}"));
        assert!(ddi_hand_plan::replay(&input, &plan).unwrap().passed);
        let mut search = ddi_hand_plan::Search::new(
            &input,
            Options {
                adaptive_timing: true,
                hold_tail_grace: 0.22,
                ..Options::default()
            },
        )
        .unwrap();
        for _ in 0..100_000 {
            if search.step() {
                break;
            }
        }
        let (recovered, report) = search.finish().unwrap().unwrap();
        assert!(report.skipped_roll_tail_refreshes > 0);
        assert!(ddi_hand_plan::replay(&input, &recovered).unwrap().passed);
        check_clearance(&input, &plan);
    }
}

#[test]
fn five_key_mine_clusters_share_one_bounded_flight() {
    for mirror in [false, true] {
        let input = input("danoni-5", mirror, &[192, 204, 216]);
        let mut job = PlannerJob::new(input.clone(), Options::default());
        for _ in 0..100_000 {
            if job.step() {
                break;
            }
        }
        let (plan, report) = job.finish().unwrap();
        let plan = plan.unwrap_or_else(|| panic!("{report:?}"));
        assert!(ddi_hand_plan::replay(&input, &plan).unwrap().passed);
        check_clearance(&input, &plan);
        let pose = plan.pose_plan().unwrap();
        for frame in 0..=(pose.duration * 480.).ceil() as usize {
            let t = frame as f64 / 480.;
            let target = pose.targets(t);
            for n in input
                .notes
                .iter()
                .filter(|n| n.mine && (n.time - plan.source_music_origin_seconds - t).abs() <= 0.09)
            {
                let x = if n.lane == 0 { -40. } else { 8. };
                assert!(
                    target
                        .tips
                        .iter()
                        .all(|p| p[2] > 0.25 || (p[0] - x).abs() > 11.5)
                );
            }
        }
    }
}

#[test]
fn mine_clearance_can_land_directly_on_the_next_scored_arrow() {
    for mirror in [false, true] {
        let lane = |l: u8| if mirror { [3, 1, 2, 0][l as usize] } else { l };
        let mut song = ddi_chart::formats::sm::parse_sm(
            "#BPMS:0=120;#NOTES:dance-single::Hard:1:0,0,0,0,0:\n1000;",
        )
        .unwrap();
        song.charts[0].notes = vec![
            Note::new(Tick(192), lane(0), NoteKind::Tap),
            Note::new(Tick(224), lane(0), NoteKind::Tap),
            Note::new(Tick(224), lane(2), NoteKind::Tap),
            Note::new(Tick(240), lane(0), NoteKind::Tap),
            Note::new(Tick(248), lane(1), NoteKind::Mine),
            Note::new(Tick(248), lane(2), NoteKind::Mine),
            Note::new(Tick(248), lane(3), NoteKind::Tap),
            Note::new(Tick(260), lane(0), NoteKind::Tap),
            Note::new(Tick(768), lane(2), NoteKind::Tap),
        ];
        let input = Input::from_song(&song, 0).unwrap();
        let mut job = PlannerJob::new(input.clone(), Options::default());
        for _ in 0..100_000 {
            if job.step() {
                break;
            }
        }
        let (plan, report) = job.finish().unwrap();
        let plan = plan.unwrap_or_else(|| panic!("{report:?}"));
        assert!(ddi_hand_plan::replay(&input, &plan).unwrap().passed);
        check_clearance(&input, &plan);
        assert_eq!(plan.events.len(), 7);
        let mut planner = RollingPlanner::new(3., input.timing.clone()).unwrap();
        let mut cursor = 0;
        for frame in 0..48 {
            let now = frame as f64 / 4.;
            let known = now + 3.;
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
            for _ in 0..100_000 {
                if !planner.step() {
                    break;
                }
            }
            assert!(planner.report.failure.is_none(), "{:?}", planner.report);
            planner.take_publication();
        }
        assert_eq!(planner.report.committed_heads, 7);
    }
}

#[test]
fn a_scored_head_after_the_roll_tail_does_not_remove_a_needed_refresh() {
    for (mirror, next) in [(false, 331), (true, 331), (false, 347), (true, 347)] {
        let lane = if mirror { 3 } else { 0 };
        let mut song = ddi_chart::formats::sm::parse_sm(
            "#BPMS:0=128;#NOTES:dance-single::Hard:1:0,0,0,0,0:\n1000;",
        )
        .unwrap();
        // A post-tail head used to erase the last regular refresh, then its
        // replacement was too near the tail and removed as well. DDR-A's roll
        // budget expired a few milliseconds before the original tail.
        song.charts[0].notes = vec![
            Note::new(Tick(192), lane, NoteKind::RollHead { end: Tick(329) }),
            Note::new(Tick(next), lane, NoteKind::Tap),
        ];
        let input = Input::from_song(&song, 0).unwrap();
        let mut job = PlannerJob::new(input.clone(), Options::default());
        for _ in 0..100_000 {
            if job.step() {
                break;
            }
        }
        let (plan, report) = job.finish().unwrap();
        let plan = plan.unwrap_or_else(|| panic!("{report:?}"));
        assert!(ddi_hand_plan::replay(&input, &plan).unwrap().passed);
        assert_eq!(plan.events.len(), 2);
    }
}

#[test]
fn mine_windows_count_as_rapid_departure_deadlines() {
    for (mirror, jump) in [(false, false), (true, false), (false, true), (true, true)] {
        let lane = |l: u8| if mirror { [3, 1, 2, 0][l as usize] } else { l };
        let mut song = ddi_chart::formats::sm::parse_sm(
            "#BPMS:0=120;#NOTES:dance-single::Hard:1:0,0,0,0,0:\n1000;",
        )
        .unwrap();
        song.charts[0].notes = if jump {
            vec![
                Note::new(Tick(192), lane(2), NoteKind::Tap),
                Note::new(Tick(192), lane(3), NoteKind::Tap),
                Note::new(Tick(206), lane(0), NoteKind::Tap),
                Note::new(Tick(206), lane(1), NoteKind::Tap),
                Note::new(Tick(206), lane(2), NoteKind::Mine),
                Note::new(Tick(206), lane(3), NoteKind::Mine),
            ]
        } else {
            vec![
                Note::new(Tick(192), lane(3), NoteKind::Tap),
                Note::new(Tick(206), lane(3), NoteKind::Mine),
                Note::new(Tick(240), lane(0), NoteKind::Tap),
            ]
        };
        let input = Input::from_song(&song, 0).unwrap();
        let mut job = PlannerJob::new(input.clone(), Options::default());
        for _ in 0..100_000 {
            if job.step() {
                break;
            }
        }
        let (plan, report) = job.finish().unwrap();
        let plan = plan.unwrap_or_else(|| panic!("{report:?}"));
        assert!(ddi_hand_plan::replay(&input, &plan).unwrap().passed);
        check_clearance(&input, &plan);
    }
}

#[test]
fn malformed_lanes_are_rejected_before_recovery_deadline_indexing() {
    let mut input = input("dance-single", false, &[206]);
    input.notes[0].lane = 9;
    input.notes[1].lane = 9;
    let mut job = PlannerJob::new(input, Options::default());
    for _ in 0..100_000 {
        if job.step() {
            break;
        }
    }
    assert!(job.finish().unwrap().0.is_none());
}
