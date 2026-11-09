// SPDX-FileCopyrightText: 2026 すごいジャン <me@sugoijan.dev>
// SPDX-License-Identifier: MIT
use ddi_chart::{Note, NoteKind, Tick};
use ddi_hand_plan::{Input, Options, RollingPlanner, layout_panels};
mod common;
use common::plan;

fn input(mirror: bool) -> Input {
    let mut song = ddi_chart::formats::sm::parse_sm(
        "#BPMS:0=120;#NOTES:pump-single::Hard:1:0,0,0,0,0:\n10000;",
    )
    .unwrap();
    let lane = |l: u8| {
        if mirror {
            [4, 3, 2, 1, 0][l as usize]
        } else {
            l
        }
    };
    let mut notes = (0..5)
        .map(|l| Note::new(Tick(192 + i64::from(l) * 48), lane(l), NoteKind::Tap))
        .collect::<Vec<_>>();
    notes.extend([
        Note::new(Tick(480), lane(0), NoteKind::Tap),
        Note::new(Tick(480), lane(3), NoteKind::Tap),
        Note::new(Tick(552), lane(1), NoteKind::Tap),
        Note::new(Tick(552), lane(4), NoteKind::Tap),
        Note::new(Tick(624), lane(2), NoteKind::HoldHead { end: Tick(768) }),
        Note::new(Tick(672), lane(0), NoteKind::Tap),
        Note::new(Tick(720), lane(1), NoteKind::Tap),
        Note::new(Tick(840), lane(2), NoteKind::RollHead { end: Tick(984) }),
        Note::new(Tick(888), lane(1), NoteKind::Tap),
        Note::new(Tick(936), lane(0), NoteKind::Tap),
        Note::new(Tick(1056), lane(0), NoteKind::Lift),
        Note::new(Tick(1056), lane(3), NoteKind::Lift),
        Note::new(Tick(1128), lane(3), NoteKind::Tap),
        Note::new(Tick(1152), lane(0), NoteKind::Tap),
        Note::new(Tick(1344), lane(2), NoteKind::Tap),
    ]);
    // No safe center: every playable tile is hazardous, but the side tiles are unscored.
    notes.extend((0..5).map(|l| Note::new(Tick(1248), lane(l), NoteKind::Mine)));
    song.charts[0].notes = notes;
    Input::from_song(&song, 0).unwrap()
}
#[test]
fn pump_maps_all_five_lanes_with_exact_hits_and_hazard_clearance() {
    assert_eq!(layout_panels("pump-single").unwrap(), &[1, 7, 5, 9, 3]);
    for mirror in [false, true] {
        let input = input(mirror);
        let plan = plan(&input);
        for e in &plan.events {
            assert_eq!(e.music_time, input.timing.seconds_at(Tick(e.tick)));
            assert!(layout_panels("pump-single").unwrap().contains(&e.panel));
        }
        let mut pose = plan.pose_plan().unwrap();
        pose.center_contacts(None);
        assert_eq!(pose.tile_center(1), [-40., -24., 0.]);
        assert_eq!(pose.tile_center(3), [8., -24., 0.]);
        assert!(
            plan.aux_contacts
                .iter()
                .flatten()
                .any(|a| matches!(a.panel, 4 | 6))
        );
        assert!(
            plan.aux_contacts
                .iter()
                .flatten()
                .all(|a| layout_panels("pump-single").unwrap().contains(&a.panel)
                    || matches!(a.panel, 4 | 6))
        );
    }
}
#[test]
fn pump_rolling_publications_preserve_contacts_as_notes_arrive() {
    for mirror in [false, true] {
        let input = input(mirror);
        let mut planner = RollingPlanner::configured(
            3.,
            input.timing.clone(),
            "pump-single",
            input.rules.clone(),
        )
        .unwrap();
        let mut cursor = 0;
        for frame in 0..72 {
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
                assert_eq!(p.plan.layout, "pump-single");
            }
        }
    }
}
#[test]
fn all_ten_pump_jump_pairs_support_exact_repeated_hits() {
    for a in 0..5 {
        for b in a + 1..5 {
            let mut song = ddi_chart::formats::sm::parse_sm(
                "#BPMS:0=120;#NOTES:pump-single::Hard:1:0,0,0,0,0:\n10000;",
            )
            .unwrap();
            song.charts[0].notes = vec![
                Note::new(Tick(192), a, NoteKind::Tap),
                Note::new(Tick(192), b, NoteKind::Tap),
                Note::new(Tick(288), a, NoteKind::Tap),
                Note::new(Tick(288), b, NoteKind::Tap),
            ];
            let input = Input::from_song(&song, 0).unwrap();
            plan(&input);
        }
    }
}
#[test]
fn mismatched_pump_lane_and_tile_is_rejected() {
    let mut input = input(false);
    input.notes[0].panel = 4;
    assert!(ddi_hand_plan::Search::new(&input, Options::default()).is_err());
}

#[test]
fn five_key_conversion_preserves_source_and_maps_oni_to_center() {
    let mut song = ddi_chart::formats::sm::parse_sm(
        "#BPMS:0=120;#NOTES:danoni-5::Hard:1:0,0,0,0,0:\n10000\n01000\n00100\n00010\n00001;",
    )
    .unwrap();
    let original = song.charts[0].notes.clone();
    song.add_chart_versions();
    let index = song
        .charts
        .iter()
        .position(|c| c.layout == "pump-single")
        .unwrap();
    let input = Input::from_song(&song, index).unwrap();
    let plan = plan(&input);
    assert!(plan.events.iter().any(|e| e.panel == 5));
    assert_eq!(song.charts[0].notes, original);
}

#[test]
fn unscored_side_returns_do_not_consume_repeated_center_notes() {
    let mut song = ddi_chart::formats::sm::parse_sm(
        "#BPMS:0=120;#NOTES:pump-single::Hard:1:0,0,0,0,0:\n00100;",
    )
    .unwrap();
    song.charts[0].notes = [192, 216, 240, 264]
        .into_iter()
        .map(|t| Note::new(Tick(t), 2, NoteKind::Tap))
        .collect();
    song.charts[0].notes.extend([
        Note::new(Tick(336), 2, NoteKind::Lift),
        Note::new(Tick(360), 2, NoteKind::Tap),
    ]);
    let input = Input::from_song(&song, 0).unwrap();
    let plan = plan(&input);
    assert!(
        plan.aux_contacts
            .iter()
            .flatten()
            .filter(|a| a.kind == ddi_hand_plan::RestoreKind::CompanionHop)
            .all(|a| matches!(a.panel, 4 | 6))
    );
    assert_eq!(plan.events.len(), input.effective_notes.len());
}
