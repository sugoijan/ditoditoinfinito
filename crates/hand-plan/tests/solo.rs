// SPDX-FileCopyrightText: 2026 すごいジャン <me@sugoijan.dev>
// SPDX-License-Identifier: MIT
use ddi_chart::{Note, NoteKind, Tick};
use ddi_hand_plan::{Input, Options, PlannerJob, RollingPlanner, layout_panels};

fn input(mirror: bool) -> Input {
    let mut song = ddi_chart::formats::sm::parse_sm(
        "#BPMS:0=120;#NOTES:dance-solo::Hard:1:0,0,0,0,0:\n100000;",
    )
    .unwrap();
    let lane = |l: u8| {
        if mirror {
            [5, 4, 2, 3, 1, 0][l as usize]
        } else {
            l
        }
    };
    let mut notes = (0..6)
        .map(|l| Note::new(Tick(192 + i64::from(l) * 48), lane(l), NoteKind::Tap))
        .collect::<Vec<_>>();
    notes.extend([
        Note::new(Tick(480), lane(1), NoteKind::Tap),
        Note::new(Tick(480), lane(4), NoteKind::Tap),
        Note::new(Tick(600), lane(1), NoteKind::HoldHead { end: Tick(744) }),
        Note::new(Tick(648), lane(4), NoteKind::Tap),
        Note::new(Tick(696), lane(5), NoteKind::Tap),
        Note::new(Tick(816), lane(4), NoteKind::RollHead { end: Tick(960) }),
        Note::new(Tick(864), lane(2), NoteKind::Tap),
        Note::new(Tick(912), lane(0), NoteKind::Tap),
        Note::new(Tick(1008), lane(1), NoteKind::Lift),
        Note::new(Tick(1008), lane(4), NoteKind::Lift),
        Note::new(Tick(1030), lane(4), NoteKind::Tap),
        Note::new(Tick(1042), lane(1), NoteKind::Tap),
        Note::new(Tick(1104), lane(1), NoteKind::Mine),
        Note::new(Tick(1104), lane(4), NoteKind::Mine),
        Note::new(Tick(1200), lane(3), NoteKind::Tap),
    ]);
    song.charts[0].notes = notes;
    Input::from_song(&song, 0).unwrap()
}
#[test]
fn solo_maps_all_six_lanes_and_keeps_center_unscored() {
    assert_eq!(layout_panels("dance-solo").unwrap(), &[4, 7, 2, 8, 9, 6]);
    for mirror in [false, true] {
        let input = input(mirror);
        for n in &input.notes {
            assert_eq!(
                n.panel,
                layout_panels("dance-solo").unwrap()[n.lane as usize]
            );
        }
        let mut job = PlannerJob::new(input.clone(), Options::default());
        for _ in 0..100_000 {
            if job.step() {
                break;
            }
        }
        let (plan, report) = job.finish().unwrap();
        let plan = plan.unwrap_or_else(|| panic!("{report:?}"));
        assert_eq!(plan.layout, "dance-solo");
        assert!(ddi_hand_plan::replay(&input, &plan).unwrap().passed);
        assert!(plan.events.iter().any(|e| e.panel == 7));
        assert!(plan.events.iter().any(|e| e.panel == 9));
        for e in &plan.events {
            assert_eq!(e.music_time, input.timing.seconds_at(Tick(e.tick)));
            assert_ne!(e.panel, 5);
        }
        let mut pose = plan.pose_plan().unwrap();
        pose.center_contacts(None);
        assert_eq!(pose.tile_center(7), [-40., 24., 0.]);
        assert_eq!(pose.tile_center(9), [8., 24., 0.]);
        for e in plan.events.iter().filter(|e| !e.lift) {
            assert_eq!(
                pose.targets(e.time).tips[e.foot.index()],
                pose.tile_center(e.panel)
            );
        }
        // Both diagonal mines may use the unscored center; it must never
        // become a synthetic Solo lane press in judge replay.
        assert!(
            plan.aux_contacts
                .iter()
                .flatten()
                .any(|a| a.kind == ddi_hand_plan::RestoreKind::MineAvoid && a.panel == 5)
        );
    }
}
#[test]
fn solo_contact_publications_stay_valid_as_six_lane_notes_arrive() {
    for mirror in [false, true] {
        let input = input(mirror);
        let mut planner =
            RollingPlanner::configured(3., input.timing.clone(), "dance-solo", input.rules.clone())
                .unwrap();
        let mut cursor = 0;
        for frame in 0..64 {
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
                assert_eq!(p.plan.layout, "dance-solo");
                assert!(p.plan.events.iter().all(|e| e.panel != 5));
            }
        }
    }
}
#[test]
fn an_inconsistent_lane_and_mat_tile_is_rejected() {
    let mut input = input(false);
    input.notes[0].panel = 9;
    assert!(ddi_hand_plan::Search::new(&input, Options::default()).is_err());
}

#[test]
fn converted_seven_key_charts_use_the_same_solo_mapping() {
    let mut song = ddi_chart::formats::sm::parse_sm(
        "#BPMS:0=120;#NOTES:danoni-7::Hard:1:0,0,0,0,0:\n1000000;",
    )
    .unwrap();
    song.charts[0].notes = (0..7)
        .map(|lane| Note::new(Tick(192 + i64::from(lane) * 48), lane, NoteKind::Tap))
        .collect();
    let original = song.charts[0].clone();
    song.add_chart_versions();
    let index = song
        .charts
        .iter()
        .position(|c| c.layout == "dance-solo")
        .unwrap();
    let input = Input::from_song(&song, index).unwrap();
    assert_eq!(
        input.notes.iter().map(|n| n.panel).collect::<Vec<_>>(),
        [4, 7, 2, 8, 9, 6]
    );
    let mut job = PlannerJob::new(input.clone(), Options::default());
    while !job.step() {}
    let (plan, report) = job.finish().unwrap();
    let plan = plan.unwrap_or_else(|| panic!("{report:?}"));
    assert!(ddi_hand_plan::replay(&input, &plan).unwrap().passed);
    assert_eq!(song.charts[0], original);
}
