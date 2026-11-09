// SPDX-FileCopyrightText: 2026 すごいジャン <me@sugoijan.dev>
// SPDX-License-Identifier: MIT
use ddi_chart::{Note, NoteKind, Tick};
use ddi_hand_plan::{ContactPlan, Input, Options, PlannerJob, RollingPlanner};
use std::collections::BTreeMap;
mod common;

fn input(kind: &str, mirror: bool) -> Input {
    let mut song = ddi_chart::formats::sm::parse_sm(
        "#BPMS:0=120;#NOTES:dance-double::Hard:1:0,0,0,0,0:10000000;",
    )
    .unwrap();
    let mut notes = Vec::new();
    let mut add = |tick, lane, kind| notes.push(Note::new(Tick(tick), lane, kind));
    match kind {
        "crossing" => {
            for (i, lane) in [0, 2, 3, 4, 6, 7, 5, 3, 0, 7, 0].into_iter().enumerate() {
                add(192 + i as i64 * 72, lane, NoteKind::Tap);
            }
        }
        "jumps" => {
            for (i, [a, b]) in [[1, 3], [3, 4], [4, 6], [5, 7], [2, 4], [3, 6], [0, 2]]
                .into_iter()
                .enumerate()
            {
                add(192 + i as i64 * 96, a, NoteKind::Tap);
                add(192 + i as i64 * 96, b, NoteKind::Tap);
            }
        }
        "held" => {
            add(192, 3, NoteKind::HoldHead { end: Tick(672) });
            for (i, lane) in [4, 6, 5, 4, 6].into_iter().enumerate() {
                add(264 + i as i64 * 72, lane, NoteKind::Tap);
            }
            add(768, 0, NoteKind::Tap);
            add(840, 7, NoteKind::Tap);
        }
        "handoff" => {
            // While Up on the right pad is held, a rapid left-bank tap permits
            // a support handoff or a crossed assignment, without retiming.
            add(192, 6, NoteKind::HoldHead { end: Tick(576) });
            add(192, 4, NoteKind::Tap);
            add(240, 4, NoteKind::Tap);
            add(252, 3, NoteKind::HoldHead { end: Tick(288) });
        }
        "hazards" => {
            add(192, 7, NoteKind::Tap);
            add(264, 6, NoteKind::Tap);
            for lane in 0..8 {
                add(384, lane, NoteKind::Mine);
            }
            add(480, 7, NoteKind::Tap);
            add(576, 4, NoteKind::Lift);
            add(672, 3, NoteKind::Tap);
            for lane in 0..8 {
                add(792, lane, NoteKind::Shock);
            }
            add(912, 2, NoteKind::Tap);
        }
        "roll" => {
            add(192, 4, NoteKind::RollHead { end: Tick(528) });
            add(288, 3, NoteKind::Tap);
            add(384, 5, NoteKind::Tap);
            add(624, 7, NoteKind::Lift);
            add(720, 0, NoteKind::Tap);
            add(816, 7, NoteKind::Tap);
        }
        _ => unreachable!(),
    }
    if mirror {
        for n in &mut notes {
            n.lane = [7, 5, 6, 4, 3, 1, 2, 0][n.lane as usize];
        }
    }
    notes.sort_by_key(|n| (n.tick, n.lane));
    song.charts[0].notes = notes;
    Input::from_song(&song, 0).unwrap()
}
fn plan(input: &Input) -> ContactPlan {
    let mut job = PlannerJob::new(input.clone(), Options::default());
    for _ in 0..1_000_000 {
        if job.step() {
            break;
        }
    }
    let (plan, report) = job.finish().expect("bounded planner completes");
    let plan = plan.unwrap_or_else(|| panic!("{report:?}"));
    assert!(ddi_hand_plan::replay(input, &plan).unwrap().passed);
    assert_eq!(plan.version, 2);
    assert_eq!(plan.mat.unwrap().pads(), 2);
    plan.pose_plan().unwrap();
    for e in &plan.events {
        assert_eq!(e.music_time, input.timing.seconds_at(Tick(e.tick)));
    }
    plan
}
#[test]
fn double_contacts_cover_crossings_jumps_holds_rolls_lifts_and_hazards() {
    assert_eq!(
        ddi_hand_plan::layout_panels("dance-double").unwrap(),
        &[4, 2, 8, 6, 13, 11, 17, 15]
    );
    for kind in ["crossing", "jumps", "held", "handoff", "hazards", "roll"] {
        for mirror in [false, true] {
            plan(&input(kind, mirror));
        }
    }
}
#[test]
fn every_no_stretch_pair_can_be_hit_and_rehit_with_both_mirrors() {
    let layout = ddi_dance_mat::MatLayout::from_id("dance-double").unwrap();
    for a in 0..8 {
        for b in a + 1..8 {
            if !layout.pair_allowed(layout.targets(a)[0], layout.targets(b)[0]) {
                continue;
            }
            let mut song = ddi_chart::formats::sm::parse_sm(
                "#BPMS:0=120;#NOTES:dance-double::Hard:1:0,0,0,0,0:10000000;",
            )
            .unwrap();
            song.charts[0].notes = [192, 288]
                .into_iter()
                .flat_map(|tick| {
                    [a, b].map(|lane| Note::new(Tick(tick), lane as u8, NoteKind::Tap))
                })
                .collect();
            let i = Input::from_song(&song, 0).unwrap();
            plan(&i);
        }
    }
}
#[test]
fn double_publications_preserve_frozen_hits_through_three_second_windows() {
    for kind in ["crossing", "jumps", "held", "handoff", "hazards", "roll"] {
        for mirror in [false, true] {
            let input = input(kind, mirror);
            let mut planner = RollingPlanner::configured(
                3.,
                input.timing.clone(),
                &input.layout,
                input.rules.clone(),
            )
            .unwrap();
            let mut cursor = 0;
            let mut committed = BTreeMap::new();
            for frame in 0..56 {
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
                let mut checks = 0;
                while planner.step() {
                    checks += 1;
                    assert!(checks < 1_000_000);
                }
                assert!(
                    planner.report.failure.is_none(),
                    "{kind} mirror={mirror}: {:?}",
                    planner.report
                );
                if let Some(p) = planner.take_publication() {
                    assert_eq!(p.plan.version, 2);
                    assert_eq!(p.committed.mat, p.plan.mat);
                    p.plan.pose_plan().unwrap();
                    p.committed.pose_plan().unwrap();
                    for e in &p.plan.events {
                        if let Some(old) = committed.get(&(e.tick, e.panel)) {
                            assert_eq!(old, e, "certified event changed");
                        }
                    }
                    for e in p.committed.events {
                        if let Some(old) = committed.insert((e.tick, e.panel), e) {
                            assert_eq!(old, e);
                        }
                    }
                }
            }
            let expected = input
                .notes
                .iter()
                .filter(|n| n.judgeable && !n.mine)
                .count();
            assert_eq!(committed.len(), expected, "{kind} mirror={mirror}");
            assert_eq!(planner.report.committed_heads, expected);
        }
    }
}
#[test]
fn double_plan_replay_rejects_missing_or_wrong_geometry() {
    let input = input("crossing", false);
    let mut p = plan(&input);
    p.mat = None;
    assert!(ddi_hand_plan::replay(&input, &p).is_err());
    p.mat = Some(ddi_dance_mat::Geometry::single());
    assert!(ddi_hand_plan::replay(&input, &p).is_err());
}

#[test]
fn automatic_double_windows_prepare_bounded_poses_without_changing_contacts() {
    let model =
        ddi_hand_pose::Model::load(include_bytes!("../../../assets/hand/model.json")).unwrap();
    for kind in ["crossing", "jumps", "held", "handoff", "hazards", "roll"] {
        for mirror in [false, true] {
            common::double_motion::check(&model, input(kind, mirror), kind, mirror);
        }
    }
}

#[test]
fn incoming_double_rows_use_the_same_no_stretch_and_at_once_two_copy() {
    let mut song = ddi_chart::formats::sm::parse_sm(
        "#BPMS:0=120;#NOTES:dance-double::Hard:1:0,0,0,0,0:10000000;",
    )
    .unwrap();
    song.charts[0].notes = (0..10)
        .flat_map(|i| [0, 3, 7].map(|lane| Note::new(Tick(192 + i * 96), lane, NoteKind::Tap)))
        .collect();
    let input = Input::from_song(&song, 0).unwrap();
    let raw = &song.charts[0].notes;
    let mut planner =
        RollingPlanner::configured(3., input.timing.clone(), &input.layout, input.rules.clone())
            .unwrap();
    let mut cursor = 0;
    let mut events = BTreeMap::new();
    for frame in 0..60 {
        let now = frame as f64 / 4.;
        let known = now + 3.;
        let start = cursor;
        while cursor < raw.len() && input.timing.seconds_at(raw[cursor].tick) <= known {
            cursor += 1;
        }
        planner
            .push(now, &raw[start..cursor], known, cursor == raw.len())
            .unwrap();
        planner.request(now);
        let mut checks = 0;
        while planner.step() {
            checks += 1;
            assert!(checks < 1_000_000);
        }
        assert!(planner.report.failure.is_none(), "{:?}", planner.report);
        if let Some(p) = planner.take_publication() {
            for e in p.committed.events {
                events.insert((e.tick, e.panel), e);
            }
        }
    }
    let expected = input
        .notes
        .iter()
        .map(|n| (n.tick, n.panel))
        .collect::<Vec<_>>();
    assert_eq!(events.keys().copied().collect::<Vec<_>>(), expected);
    assert_eq!(planner.report.removed, raw.len() - expected.len());
    assert!(planner.report.removed > 0);
}

#[test]
fn opening_rest_uses_received_notes_but_never_reseats_a_published_prefix() {
    for delayed in [false, true] {
        let mut song = ddi_chart::formats::sm::parse_sm(
            "#BPMS:0=120;#NOTES:dance-double::Hard:1:0,0,0,0,0:00000001;",
        )
        .unwrap();
        song.charts[0].notes = vec![Note::new(
            Tick(if delayed { 300 } else { 48 }),
            7,
            NoteKind::Tap,
        )];
        let input = Input::from_song(&song, 0).unwrap();
        let mut planner = RollingPlanner::configured(
            3.,
            input.timing.clone(),
            &input.layout,
            input.rules.clone(),
        )
        .unwrap();
        if delayed {
            planner.push(0., &[], 3., false).unwrap();
            planner.request(0.);
            while planner.step() {}
            let p = planner.take_publication().unwrap();
            assert_eq!([p.plan.initial.left, p.plan.initial.right], [5, 5]);
            assert!(p.certified_until > 0.);
        }
        let now = if delayed { 0.25 } else { 0. };
        planner
            .push(now, &input.effective_notes, now + 3., true)
            .unwrap();
        planner.request(now);
        while planner.step() {}
        assert!(planner.report.failure.is_none(), "{:?}", planner.report);
        let p = planner.take_publication().unwrap();
        assert_eq!(
            [p.plan.initial.left, p.plan.initial.right],
            if delayed { [5, 5] } else { [14, 14] }
        );
        assert_eq!(p.plan.events[0].music_time, input.notes[0].time);
    }
}

#[test]
fn opening_support_is_committed_even_before_the_first_scored_row() {
    for (layout, lane) in [
        ("dance-double", 4),
        ("pump-halfdouble", 0),
        ("pump-double", 7),
        ("danoni-9A", 5),
    ] {
        let mut song = ddi_chart::formats::sm::parse_sm(
            "#OFFSET:-1;#BPMS:0=120;#NOTES:dance-single::Easy:1:0,0,0,0,0:1000;",
        )
        .unwrap();
        song.charts[0].layout = layout.into();
        song.charts[0].notes = vec![
            Note::new(Tick(192), lane, NoteKind::Tap),
            Note::new(Tick(384), lane, NoteKind::Tap),
        ];
        let input = Input::from_song(&song, 0).unwrap();
        let mut planner =
            RollingPlanner::configured(3., input.timing.clone(), layout, input.rules.clone())
                .unwrap();
        let mut cursor = 0;
        let mut first = None;
        for frame in 0..=8 {
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
            assert!(
                planner.report.failure.is_none(),
                "{layout}: {:?}",
                planner.report
            );
            if let Some(p) = planner.take_publication() {
                let opening = [p.plan.initial.left, p.plan.initial.right];
                assert_eq!(
                    opening,
                    [p.committed.initial.left, p.committed.initial.right],
                    "{layout} at {now}: published opening differs from its frozen seed"
                );
                assert_eq!(
                    *first.get_or_insert(opening),
                    opening,
                    "{layout}: opening moved after publication"
                );
            }
        }
    }
}
