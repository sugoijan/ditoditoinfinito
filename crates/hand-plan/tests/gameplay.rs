// SPDX-FileCopyrightText: 2026 すごいジャン <me@sugoijan.dev>
// SPDX-License-Identifier: MIT
use ddi_hand_plan::{Input, Options, PlannerJob};

#[test]
fn taps_holds_and_mirrors_keep_scored_times_and_pass_actual_judges() {
    for (layout, notes) in [
        ("dance-single", "1000\n0100\n0010\n0001"),
        ("dance-single", "0001\n0100\n0010\n1000"),
        ("dance-single", "2000\n0001\n3000\n0010"),
        ("danoni-5", "00001\n10000\n01000\n00010"),
    ] {
        let song = ddi_chart::formats::sm::parse_sm(&format!(
            "#BPMS:0=120;#OFFSET:-1;#NOTES:{layout}::Easy:2:0,0,0,0,0:\n{notes};"
        ))
        .unwrap();
        let input = Input::from_song(&song, 0).unwrap();
        let mut job = PlannerJob::new(input.clone(), Options::default());
        for _ in 0..100000 {
            if job.step() {
                break;
            }
        }
        let (plan, report) = job.finish().unwrap();
        let plan = plan.unwrap_or_else(|| panic!("{}", serde_json::to_string(&report).unwrap()));
        assert_eq!(plan.events.len(), input.notes.len());
        for event in &plan.events {
            assert_eq!(
                event.music_time,
                input.timing.seconds_at(ddi_chart::Tick(event.tick))
            );
        }
        assert!(ddi_hand_plan::replay(&input, &plan).unwrap().passed);
    }
}

#[test]
fn a_double_lift_can_land_one_support_and_take_the_other_directly_to_a_tap() {
    use ddi_chart::{Note, NoteKind, Tick};
    use ddi_hand_plan::{Foot, RestoreKind, RollingPlanner};
    for mirror in [false, true] {
        let lane = |l: u8| if mirror { [3, 1, 2, 0][l as usize] } else { l };
        let mut song = ddi_chart::formats::sm::parse_sm(
            "#BPMS:0=120;#NOTES:dance-single::Hard:1:0,0,0,0,0:\n1000;",
        )
        .unwrap();
        song.charts[0].notes = vec![
            Note::new(Tick(96), lane(2), NoteKind::Tap),
            Note::new(Tick(96), lane(3), NoteKind::Tap),
            Note::new(Tick(192), lane(2), NoteKind::Lift),
            Note::new(Tick(192), lane(3), NoteKind::Lift),
            Note::new(Tick(214), lane(3), NoteKind::Tap),
            Note::new(Tick(226), lane(2), NoteKind::Tap),
            Note::new(Tick(238), lane(3), NoteKind::Tap),
            Note::new(Tick(250), lane(2), NoteKind::Tap),
            Note::new(Tick(960), lane(0), NoteKind::Tap),
        ];
        let input = Input::from_song(&song, 0).unwrap();
        let mut job = PlannerJob::new(input.clone(), Options::default());
        while !job.step() {}
        let (plan, report) = job.finish().unwrap();
        let plan = plan.unwrap_or_else(|| panic!("{report:?}"));
        assert!(ddi_hand_plan::replay(&input, &plan).unwrap().passed);
        let tap = plan.events.iter().find(|e| e.tick == 214).unwrap();
        let tracks = [&plan.tracks.left, &plan.tracks.right];
        let direct = tracks[tap.foot.index()]
            .iter()
            .find(|s| s.land == tap.time)
            .unwrap();
        assert_eq!(
            direct.release + plan.source_music_origin_seconds,
            input.timing.seconds_at(Tick(192))
        );
        assert_eq!(direct.to, tap.panel);
        assert!(
            plan.aux_contacts
                .iter()
                .flatten()
                .any(|a| a.kind == RestoreKind::LiftClear
                    && a.foot == tap.foot
                    && a.time == tap.time)
        );
        let other = if tap.foot == Foot::L { 1 } else { 0 };
        assert!(
            tracks[other]
                .iter()
                .any(|s| s.release == direct.release && s.land < direct.land && s.to == 5)
        );
        for e in &plan.events {
            assert_eq!(e.music_time, input.timing.seconds_at(Tick(e.tick)));
        }
        // Keep the same choice valid across ordinary rolling publications.
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
            while planner.step() {}
            assert!(planner.report.failure.is_none(), "{:?}", planner.report);
            planner.take_publication();
        }
    }
}
