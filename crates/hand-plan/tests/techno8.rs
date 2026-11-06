// SPDX-FileCopyrightText: 2026 すごいジャン <me@sugoijan.dev>
// SPDX-License-Identifier: MIT
use ddi_chart::{Glyph, Note, NoteKind, Tick};
use ddi_hand_plan::{ContactPlan, Input, Options, PlannerJob, RollingPlanner, layout_panels};

fn song(mirror: bool) -> ddi_chart::Song {
    let mut song = ddi_chart::formats::sm::parse_sm(
        "#BPMS:0=120;#NOTES:techno-single8::Hard:1:0,0,0,0,0:\n10000000;",
    )
    .unwrap();
    let lane = |n: u8| if mirror { 7 - n } else { n };
    let mut notes = (0..8)
        .map(|n| Note::new(Tick(192 + i64::from(n) * 48), lane(n), NoteKind::Tap))
        .collect::<Vec<_>>();
    notes.extend([
        Note::new(Tick(624), lane(0), NoteKind::Tap),
        Note::new(Tick(624), lane(5), NoteKind::Tap),
        Note::new(Tick(696), lane(2), NoteKind::Tap),
        Note::new(Tick(696), lane(7), NoteKind::Tap),
        Note::new(Tick(792), lane(1), NoteKind::HoldHead { end: Tick(936) }),
        Note::new(Tick(840), lane(6), NoteKind::Tap),
        Note::new(Tick(888), lane(4), NoteKind::Tap),
        Note::new(Tick(1032), lane(6), NoteKind::RollHead { end: Tick(1176) }),
        Note::new(Tick(1080), lane(1), NoteKind::Tap),
        Note::new(Tick(1128), lane(3), NoteKind::Tap),
        Note::new(Tick(1272), lane(0), NoteKind::Lift),
        Note::new(Tick(1272), lane(5), NoteKind::Lift),
        Note::new(Tick(1344), lane(5), NoteKind::Tap),
        Note::new(Tick(1368), lane(0), NoteKind::Tap),
        Note::new(Tick(1608), lane(3), NoteKind::Tap),
    ]);
    notes.extend((0..8).map(|n| Note::new(Tick(1512), lane(n), NoteKind::Mine)));
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
fn techno_mapping_uses_every_outer_tile_and_matches_displayed_glyphs() {
    let panels = layout_panels("techno-single8").unwrap();
    assert_eq!(panels, &[1, 4, 7, 2, 8, 9, 6, 3]);
    let layout = ddi_chart::Layout::techno_single8();
    for (lane, &panel) in layout.lanes.iter().zip(panels) {
        let Glyph::Arrow { rotation_deg } = lane.glyph else {
            panic!("expected arrow")
        };
        let expected = match rotation_deg as i32 {
            -45 => 1,
            0 => 4,
            45 => 7,
            -90 => 2,
            90 => 8,
            135 => 9,
            180 => 6,
            225 => 3,
            _ => panic!("unexpected direction"),
        };
        assert_eq!(panel, expected, "{}", lane.name);
    }
    assert!(!panels.contains(&5));
}
#[test]
fn techno_mixed_notes_preserve_scored_times_and_center_escape_is_unscored() {
    for mirror in [false, true] {
        let song = song(mirror);
        let original = song.charts[0].notes.clone();
        let input = Input::from_song(&song, 0).unwrap();
        let plan = solve(&input);
        assert!(plan.aux_contacts.iter().flatten().any(|a| a.panel == 5));
        for e in &plan.events {
            assert_eq!(e.music_time, input.timing.seconds_at(Tick(e.tick)));
            assert!(layout_panels("techno-single8").unwrap().contains(&e.panel));
        }
        assert_eq!(song.charts[0].notes, original);
    }
}
#[test]
fn techno_rolling_publications_stay_valid_with_eight_incoming_lanes() {
    for mirror in [false, true] {
        let input = Input::from_song(&song(mirror), 0).unwrap();
        let mut planner = RollingPlanner::configured(
            3.,
            input.timing.clone(),
            "techno-single8",
            input.rules.clone(),
        )
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
                assert_eq!(p.plan.layout, "techno-single8");
                assert!(p.plan.events.iter().all(|e| e.panel != 5));
            }
        }
    }
}
#[test]
fn all_28_techno_jump_pairs_support_repeated_hits() {
    for a in 0..8 {
        for b in a + 1..8 {
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
fn eighth_lane_rapid_mine_keeps_adaptive_departure_and_exact_touchdowns() {
    let mut song = song(false);
    song.charts[0].notes = vec![
        Note::new(Tick(192), 7, NoteKind::Tap),
        Note::new(Tick(204), 7, NoteKind::Mine),
        Note::new(Tick(288), 3, NoteKind::Tap),
    ];
    solve(&Input::from_song(&song, 0).unwrap());
}
#[test]
fn techno_can_use_air_time_and_regrab_two_holds_without_retiming() {
    let mut song = song(false);
    song.charts[0].notes = vec![
        Note::new(Tick(192), 1, NoteKind::HoldHead { end: Tick(480) }),
        Note::new(Tick(192), 6, NoteKind::HoldHead { end: Tick(480) }),
        Note::new(Tick(288), 1, NoteKind::Mine),
        Note::new(Tick(288), 6, NoteKind::Mine),
        Note::new(Tick(576), 4, NoteKind::Tap),
    ];
    let plan = solve(&Input::from_song(&song, 0).unwrap());
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
fn techno_conversion_preserves_nine_key_source_and_removes_only_oni() {
    let mut song = ddi_chart::formats::sm::parse_sm(
        "#BPMS:0=120;#NOTES:danoni-9B::Hard:1:0,0,0,0,0:\n100000000;",
    )
    .unwrap();
    song.charts[0].notes = (0..9)
        .map(|n| Note::new(Tick(192 + i64::from(n) * 48), n, NoteKind::Tap))
        .collect();
    let original = song.charts[0].notes.clone();
    song.add_chart_versions();
    let i = song
        .charts
        .iter()
        .position(|c| c.layout == "techno-single8")
        .unwrap();
    let input = Input::from_song(&song, i).unwrap();
    let plan = solve(&input);
    assert_eq!(plan.events.len(), 8);
    assert_eq!(song.charts[0].notes, original);
}
#[test]
fn an_unscored_center_cannot_be_substituted_for_a_techno_lane() {
    let mut input = Input::from_song(&song(false), 0).unwrap();
    input.notes[0].panel = 5;
    assert!(ddi_hand_plan::Search::new(&input, Options::default()).is_err());
}
