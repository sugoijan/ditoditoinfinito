// SPDX-FileCopyrightText: 2026 すごいジャン <me@sugoijan.dev>
// SPDX-License-Identifier: MIT
use ddi_chart::{Glyph, Note, NoteKind, Tick};
use ddi_hand_plan::{ContactPlan, Input, Options, PlannerJob, RollingPlanner, layout_panels};
fn song(mirror: bool) -> ddi_chart::Song {
    let mut song = ddi_chart::formats::sm::parse_sm(
        "#BPMS:0=120;#NOTES:danoni-9B::Hard:1:0,0,0,0,0:\n100000000;",
    )
    .unwrap();
    let lane = |n: u8| if mirror { 8 - n } else { n };
    let mut notes = (0..9)
        .map(|n| Note::new(Tick(192 + i64::from(n) * 48), lane(n), NoteKind::Tap))
        .collect::<Vec<_>>();
    notes.extend([
        Note::new(Tick(672), lane(0), NoteKind::Tap),
        Note::new(Tick(672), lane(8), NoteKind::Tap),
        Note::new(Tick(744), lane(2), NoteKind::Tap),
        Note::new(Tick(744), lane(6), NoteKind::Tap),
        Note::new(Tick(840), lane(1), NoteKind::HoldHead { end: Tick(984) }),
        Note::new(Tick(888), lane(7), NoteKind::Tap),
        Note::new(Tick(936), lane(5), NoteKind::Tap),
        Note::new(Tick(1080), lane(7), NoteKind::RollHead { end: Tick(1224) }),
        Note::new(Tick(1128), lane(1), NoteKind::Tap),
        Note::new(Tick(1176), lane(3), NoteKind::Tap),
        Note::new(Tick(1320), lane(0), NoteKind::Lift),
        Note::new(Tick(1320), lane(8), NoteKind::Lift),
        Note::new(Tick(1392), lane(8), NoteKind::Tap),
        Note::new(Tick(1416), lane(0), NoteKind::Tap),
        Note::new(Tick(1656), lane(4), NoteKind::Tap),
    ]);
    notes.extend((0..9).map(|n| Note::new(Tick(1560), lane(n), NoteKind::Mine)));
    song.charts[0].notes = notes;
    song
}
fn solve(input: &Input) -> ContactPlan {
    let mut job = PlannerJob::new(input.clone(), Options::default());
    for _ in 0..100_000 {
        if job.step() {
            break;
        }
    }
    let (plan, report) = job.finish().expect("bounded planner completed");
    let plan = plan.unwrap_or_else(|| panic!("{report:?}"));
    assert!(ddi_hand_plan::replay(input, &plan).unwrap().passed);
    plan
}
#[test]
fn nine_key_mapping_matches_displayed_directions_including_oni() {
    let panels = layout_panels("danoni-9B").unwrap();
    assert_eq!(panels, &[7, 4, 1, 2, 5, 8, 9, 6, 3]);
    let layout = ddi_chart::Layout::builtin("danoni-9B").unwrap();
    for (lane, &panel) in layout.lanes.iter().zip(panels) {
        let expected = match lane.glyph {
            Glyph::Onigiri => 5,
            Glyph::Arrow { rotation_deg } => match rotation_deg as i32 {
                45 => 7,
                0 => 4,
                -45 => 1,
                -90 => 2,
                90 => 8,
                135 => 9,
                180 => 6,
                225 => 3,
                _ => panic!("direction"),
            },
            _ => panic!("glyph"),
        };
        assert_eq!(panel, expected, "{}", lane.name);
    }
    for panel in 1..=9 {
        assert!(panels.contains(&panel));
    }
}
#[test]
fn mixed_nine_key_notes_keep_exact_times_and_clear_all_tiles_with_air_time() {
    for mirror in [false, true] {
        let song = song(mirror);
        let original = song.charts[0].notes.clone();
        let input = Input::from_song(&song, 0).unwrap();
        let plan = solve(&input);
        for e in &plan.events {
            assert_eq!(e.music_time, input.timing.seconds_at(Tick(e.tick)));
        }
        let t = input.timing.seconds_at(Tick(1560)) - plan.source_music_origin_seconds;
        for track in [&plan.tracks.left, &plan.tracks.right] {
            assert!(
                track
                    .iter()
                    .any(|s| s.release <= t - 0.09 && s.land >= t + 0.09),
                "{track:?}"
            );
        }
        assert_eq!(song.charts[0].notes, original);
    }
}
#[test]
fn nine_key_rolling_publications_preserve_contacts() {
    for mirror in [false, true] {
        let input = Input::from_song(&song(mirror), 0).unwrap();
        let mut planner =
            RollingPlanner::configured(3., input.timing.clone(), "danoni-9B", input.rules.clone())
                .unwrap();
        let mut cursor = 0;
        for frame in 0..92 {
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
                assert_eq!(p.plan.layout, "danoni-9B");
            }
        }
    }
}
#[test]
fn all_36_nine_key_jump_pairs_support_repeated_hits() {
    for a in 0..9 {
        for b in a + 1..9 {
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
fn ninth_lane_mine_uses_adaptive_air_departure() {
    let mut song = song(false);
    song.charts[0].notes = vec![
        Note::new(Tick(192), 8, NoteKind::Tap),
        Note::new(Tick(206), 8, NoteKind::Mine),
        Note::new(Tick(288), 3, NoteKind::Tap),
    ];
    solve(&Input::from_song(&song, 0).unwrap());
}
#[test]
fn shared_center_repeats_and_rolls_do_not_assume_an_escape_tile() {
    for roll in [false, true] {
        let mut song = song(false);
        song.charts[0].notes = if roll {
            vec![
                Note::new(Tick(192), 4, NoteKind::RollHead { end: Tick(384) }),
                Note::new(Tick(480), 1, NoteKind::Tap),
            ]
        } else {
            [192, 216, 240, 264]
                .into_iter()
                .map(|t| Note::new(Tick(t), 4, NoteKind::Tap))
                .collect()
        };
        solve(&Input::from_song(&song, 0).unwrap());
    }
}
#[test]
fn shared_center_roll_tail_can_delay_companion_landing_past_all_tile_mines() {
    let mut song = song(false);
    song.charts[0].notes = vec![
        Note::new(Tick(192), 4, NoteKind::RollHead { end: Tick(204) }),
        Note::new(Tick(288), 4, NoteKind::Tap),
    ];
    song.charts[0]
        .notes
        .extend((0..9).map(|n| Note::new(Tick(210), n, NoteKind::Mine)));
    let plan = solve(&Input::from_song(&song, 0).unwrap());
    assert!(
        plan.aux_contacts
            .iter()
            .flatten()
            .any(|a| a.kind == ddi_hand_plan::RestoreKind::CompanionHop
                && a.time > plan.events[0].time + 0.06)
    );
}
#[test]
fn nine_key_invalid_mat_tile_is_rejected() {
    let mut input = Input::from_song(&song(false), 0).unwrap();
    input.notes[0].panel = 0;
    assert!(ddi_hand_plan::Search::new(&input, Options::default()).is_err());
}

#[test]
fn rapid_shared_center_taps_and_lift_followup_keep_exact_hits() {
    for lift in [false, true] {
        let mut song = song(false);
        song.charts[0].notes = if lift {
            vec![
                Note::new(Tick(192), 4, NoteKind::Lift),
                Note::new(Tick(216), 4, NoteKind::Tap),
            ]
        } else {
            [192, 204, 216, 228, 240]
                .into_iter()
                .map(|t| Note::new(Tick(t), 4, NoteKind::Tap))
                .collect()
        };
        solve(&Input::from_song(&song, 0).unwrap());
    }
}

#[test]
fn rapid_center_taps_keep_the_companion_airborne_over_outer_tile_hazards() {
    let mut song = song(false);
    song.charts[0].notes = [192, 204, 216, 228]
        .into_iter()
        .map(|t| Note::new(Tick(t), 4, NoteKind::Tap))
        .collect();
    song.charts[0].notes.extend(
        (0..9)
            .filter(|n| *n != 4)
            .map(|n| Note::new(Tick(202), n, NoteKind::Mine)),
    );
    solve(&Input::from_song(&song, 0).unwrap());
}
#[test]
fn native_nine_key_holds_keep_the_actual_release_budget() {
    let mut song = song(false);
    song.charts[0].notes = vec![
        Note::new(Tick(192), 1, NoteKind::HoldHead { end: Tick(480) }),
        Note::new(Tick(192), 7, NoteKind::HoldHead { end: Tick(480) }),
        Note::new(Tick(288), 1, NoteKind::Mine),
        Note::new(Tick(288), 7, NoteKind::Mine),
    ];
    let mut input = Input::from_song(&song, 0).unwrap();
    input.rules = vec![("danoni".into(), ddi_engine::rules::presets::danoni().judge)];
    let mut job = PlannerJob::new(input, Options::default());
    for _ in 0..100_000 {
        if job.step() {
            break;
        }
    }
    let (plan, _) = job.finish().unwrap();
    assert!(plan.is_none());
}
