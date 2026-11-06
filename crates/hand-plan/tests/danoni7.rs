// SPDX-FileCopyrightText: 2026 すごいジャン <me@sugoijan.dev>
// SPDX-License-Identifier: MIT
use ddi_chart::{Glyph, Note, NoteKind, Tick};
use ddi_hand_plan::{ContactPlan, Input, Options, PlannerJob, RollingPlanner, layout_panels};

fn song(mirror: bool) -> ddi_chart::Song {
    let mut song = ddi_chart::formats::sm::parse_sm(
        "#BPMS:0=120;#NOTES:danoni-7::Hard:1:0,0,0,0,0:\n1000000;",
    )
    .unwrap();
    let lane = |n: u8| if mirror { 6 - n } else { n };
    let mut notes = (0..7)
        .map(|n| Note::new(Tick(192 + i64::from(n) * 48), lane(n), NoteKind::Tap))
        .collect::<Vec<_>>();
    notes.extend([
        Note::new(Tick(576), lane(1), NoteKind::Tap),
        Note::new(Tick(576), lane(5), NoteKind::Tap),
        Note::new(Tick(648), lane(3), NoteKind::Tap),
        Note::new(Tick(720), lane(2), NoteKind::Tap),
        Note::new(Tick(720), lane(4), NoteKind::Tap),
        Note::new(Tick(816), lane(3), NoteKind::HoldHead { end: Tick(960) }),
        Note::new(Tick(864), lane(0), NoteKind::Tap),
        Note::new(Tick(912), lane(6), NoteKind::Tap),
        Note::new(Tick(1056), lane(3), NoteKind::RollHead { end: Tick(1200) }),
        Note::new(Tick(1104), lane(0), NoteKind::Tap),
        Note::new(Tick(1152), lane(2), NoteKind::Tap),
        Note::new(Tick(1296), lane(1), NoteKind::Lift),
        Note::new(Tick(1296), lane(5), NoteKind::Lift),
        Note::new(Tick(1368), lane(5), NoteKind::Tap),
        Note::new(Tick(1392), lane(1), NoteKind::Tap),
        Note::new(Tick(1632), lane(3), NoteKind::Tap),
    ]);
    notes.extend((0..7).map(|n| Note::new(Tick(1536), lane(n), NoteKind::Mine)));
    song.charts[0].notes = notes;
    song
}
fn solve(input: &Input) -> (ContactPlan, ddi_hand_plan::PlanningReport) {
    let mut job = PlannerJob::new(input.clone(), Options::default());
    for _ in 0..100_000 {
        if job.step() {
            break;
        }
    }
    let (plan, report) = job.finish().expect("bounded planner completed");
    let plan = plan.unwrap_or_else(|| panic!("{report:?}"));
    assert!(ddi_hand_plan::replay(input, &plan).unwrap().passed);
    (plan, report)
}
#[test]
fn seven_key_mat_mapping_follows_displayed_glyphs() {
    let panels = layout_panels("danoni-7").unwrap();
    assert_eq!(panels, &[4, 1, 2, 5, 8, 9, 6]);
    let layout = ddi_chart::Layout::builtin("danoni-7").unwrap();
    for (lane, &panel) in layout.lanes.iter().zip(panels) {
        let expected = match lane.glyph {
            Glyph::Onigiri => 5,
            Glyph::Arrow { rotation_deg } => match rotation_deg as i32 {
                0 => 4,
                -45 => 1,
                -90 => 2,
                90 => 8,
                135 => 9,
                180 => 6,
                _ => panic!("unexpected direction"),
            },
            _ => panic!("unexpected glyph"),
        };
        assert_eq!(panel, expected, "{}", lane.name);
    }
    assert!(!panels.contains(&7) && !panels.contains(&3));
}
#[test]
fn seven_key_mixed_notes_keep_exact_times_and_unscored_corner_clearance() {
    for mirror in [false, true] {
        let song = song(mirror);
        let original = song.charts[0].notes.clone();
        let input = Input::from_song(&song, 0).unwrap();
        let (plan, _) = solve(&input);
        for event in &plan.events {
            assert_eq!(event.music_time, input.timing.seconds_at(Tick(event.tick)));
            assert!(layout_panels("danoni-7").unwrap().contains(&event.panel));
        }
        assert!(
            plan.aux_contacts
                .iter()
                .flatten()
                .any(|a| matches!(a.panel, 7 | 3))
        );
        assert_eq!(song.charts[0].notes, original);
    }
}
#[test]
fn seven_key_rolling_publications_stay_valid_as_notes_arrive() {
    for mirror in [false, true] {
        let input = Input::from_song(&song(mirror), 0).unwrap();
        let mut planner =
            RollingPlanner::configured(3., input.timing.clone(), "danoni-7", input.rules.clone())
                .unwrap();
        let mut cursor = 0;
        for frame in 0..88 {
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
            if let Some(p) = planner.take_publication() {
                assert_eq!(p.plan.layout, "danoni-7");
            }
        }
    }
}
#[test]
fn every_seven_key_jump_pair_supports_repeated_hits() {
    for a in 0..7 {
        for b in a + 1..7 {
            let mut song = song(false);
            song.charts[0].notes = vec![
                Note::new(Tick(192), a, NoteKind::Tap),
                Note::new(Tick(192), b, NoteKind::Tap),
                Note::new(Tick(288), a, NoteKind::Tap),
                Note::new(Tick(288), b, NoteKind::Tap),
            ];
            solve(&Input::from_song(&song, 0).unwrap());
        }
    }
}
#[test]
fn seventh_lane_rapid_mine_uses_the_existing_adaptive_departure_policy() {
    let mut song = song(false);
    song.charts[0].notes = vec![
        Note::new(Tick(192), 6, NoteKind::Tap),
        Note::new(Tick(204), 6, NoteKind::Mine),
        Note::new(Tick(288), 3, NoteKind::Tap),
    ];
    let (_, report) = solve(&Input::from_song(&song, 0).unwrap());
    assert!(report.adaptive_gate.is_some(), "{report:?}");
}
#[test]
fn an_unused_corner_cannot_be_substituted_for_a_seven_key_lane() {
    let mut input = Input::from_song(&song(false), 0).unwrap();
    input.notes[0].panel = 7;
    assert!(ddi_hand_plan::Search::new(&input, Options::default()).is_err());
}

#[test]
fn held_seven_key_hazards_can_use_air_time_and_regrab_both_holds() {
    let mut song = song(false);
    song.charts[0].notes = vec![
        Note::new(Tick(192), 0, NoteKind::HoldHead { end: Tick(480) }),
        Note::new(Tick(192), 6, NoteKind::HoldHead { end: Tick(480) }),
        Note::new(Tick(288), 0, NoteKind::Mine),
        Note::new(Tick(288), 6, NoteKind::Mine),
        Note::new(Tick(576), 3, NoteKind::Tap),
    ];
    let input = Input::from_song(&song, 0).unwrap();
    let (plan, _) = solve(&input);
    for track in [&plan.tracks.left, &plan.tracks.right] {
        assert!(
            track
                .iter()
                .any(|s| s.from == s.to && matches!(s.from, 4 | 6)),
            "{track:?}"
        );
    }
    assert_eq!(plan.holds.len(), 2);
    assert_eq!(plan.events.len(), 3);
}

#[test]
fn native_seven_key_hold_budget_is_not_relaxed_for_mine_avoidance() {
    let mut song = song(false);
    song.charts[0].notes = vec![
        Note::new(Tick(192), 0, NoteKind::HoldHead { end: Tick(480) }),
        Note::new(Tick(192), 6, NoteKind::HoldHead { end: Tick(480) }),
        Note::new(Tick(288), 0, NoteKind::Mine),
        Note::new(Tick(288), 6, NoteKind::Mine),
    ];
    let mut input = Input::from_song(&song, 0).unwrap();
    input.rules = vec![("danoni".into(), ddi_engine::rules::presets::danoni().judge)];
    let mut job = PlannerJob::new(input, Options::default());
    for _ in 0..100_000 {
        if job.step() {
            break;
        }
    }
    let (plan, report) = job.finish().unwrap();
    assert!(
        plan.is_none(),
        "must not accept a hold release beyond native DanOni's budget"
    );
    assert!(matches!(
        report.status,
        ddi_hand_plan::Status::ContactFailure | ddi_hand_plan::Status::ContactJudgeFailure
    ));
}

#[test]
fn a_native_shared_center_hold_keeps_continuous_support_across_sides() {
    let mut song = song(false);
    song.charts[0].notes = vec![
        Note::new(Tick(192), 3, NoteKind::HoldHead { end: Tick(480) }),
        Note::new(Tick(240), 0, NoteKind::Tap),
        Note::new(Tick(288), 6, NoteKind::Tap),
        Note::new(Tick(576), 4, NoteKind::Tap),
    ];
    let mut input = Input::from_song(&song, 0).unwrap();
    input.rules = vec![("danoni".into(), ddi_engine::rules::presets::danoni().judge)];
    let (plan, _) = solve(&input);
    assert_eq!(plan.holds.len(), 1);
    assert_eq!(plan.events.len(), 4);
}
