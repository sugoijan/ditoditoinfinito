// SPDX-FileCopyrightText: 2026 すごいジャン <me@sugoijan.dev>
// SPDX-License-Identifier: MIT
use ddi_chart::{Note, NoteKind, Tick};
use ddi_hand_plan::{Input, Options, PlannerJob};
mod common;
fn input(kind: &str, mirror: bool, native: bool) -> Input {
    let mut song = ddi_chart::formats::sm::parse_sm(
        "#BPMS:0=120;#NOTES:danoni-9A::Hard:1:0,0,0,0,0:100000000;",
    )
    .unwrap();
    let mut notes = Vec::new();
    let mut add = |t, lane: u8, kind| {
        notes.push(Note::new(
            Tick(t),
            if mirror { 8 - lane } else { lane },
            kind,
        ))
    };
    match kind {
        "walking" => {
            for (i, lane) in [0, 4, 1, 4, 5, 4, 6, 4, 8].into_iter().enumerate() {
                add(192 + i as i64 * 72, lane, NoteKind::Tap);
            }
        }
        "jumps" => {
            for (i, pair) in [[0, 4], [0, 4], [4, 8], [4, 8], [3, 5], [1, 2], [6, 7]]
                .into_iter()
                .enumerate()
            {
                for lane in pair {
                    add(192 + i as i64 * 96, lane, NoteKind::Tap);
                }
            }
        }
        "held" => {
            add(192, 4, NoteKind::HoldHead { end: Tick(672) });
            for (i, lane) in [5, 6, 7, 8].into_iter().enumerate() {
                add(288 + i as i64 * 96, lane, NoteKind::Tap);
            }
            add(768, 4, NoteKind::Tap);
        }
        "held-lift" => {
            add(96, 5, NoteKind::Tap);
            add(192, 5, NoteKind::HoldHead { end: Tick(672) });
            add(288, 4, NoteKind::Tap);
            add(432, 4, NoteKind::Lift);
            add(576, 4, NoteKind::Tap);
            add(768, 6, NoteKind::Tap);
        }
        "roll" => {
            add(192, 4, NoteKind::RollHead { end: Tick(528) });
            for (i, lane) in [5, 7, 8].into_iter().enumerate() {
                add(288 + i as i64 * 96, lane, NoteKind::Tap);
            }
            add(624, 4, NoteKind::Tap);
            add(720, 4, NoteKind::Lift);
            add(864, 3, NoteKind::Tap);
        }
        "hazards" => {
            add(192, 4, NoteKind::Tap);
            add(192, 8, NoteKind::Tap);
            for lane in 0..9 {
                add(336, lane, NoteKind::Mine);
            }
            add(480, 4, NoteKind::Tap);
            add(480, 0, NoteKind::Tap);
            for lane in 0..9 {
                add(624, lane, NoteKind::Shock);
            }
            add(816, 4, NoteKind::Lift);
            add(936, 4, NoteKind::Tap);
        }
        "immediate" | "delayed" => {
            let start = if kind == "immediate" { 48 } else { 300 };
            for (i, lane) in [4, 8, 6, 4].into_iter().enumerate() {
                add(start + i as i64 * 96, lane, NoteKind::Tap);
            }
        }
        _ => unreachable!(),
    }
    notes.sort_by_key(|n| (n.tick, n.lane));
    song.charts[0].notes = notes;
    let mut input = Input::from_song(&song, 0).unwrap();
    if native {
        input.rules = vec![("danoni".into(), ddi_engine::rules::presets::danoni().judge)];
    }
    input
}
fn plan(i: &Input) -> ddi_hand_plan::ContactPlan {
    let mut job = PlannerJob::new(i.clone(), Options::default());
    for _ in 0..1_000_000 {
        if job.step() {
            break;
        }
    }
    let (p, r) = job.finish().unwrap();
    let p = p.unwrap_or_else(|| panic!("{r:?}"));
    assert!(ddi_hand_plan::replay(i, &p).unwrap().passed);
    assert_eq!(p.version, 2);
    assert_eq!(p.mat.unwrap().pads(), 2);
    p.pose_plan().unwrap();
    for e in &p.events {
        assert_eq!(e.music_time, i.timing.seconds_at(Tick(e.tick)));
    }
    p
}
const KINDS: [&str; 8] = [
    "walking",
    "jumps",
    "held",
    "held-lift",
    "roll",
    "hazards",
    "immediate",
    "delayed",
];
#[test]
fn oni_selects_both_centers_and_preserves_one_button_timing() {
    for kind in KINDS {
        for mirror in [false, true] {
            for native in [false, true] {
                let i = input(kind, mirror, native);
                let p = plan(&i);
                if kind == "jumps" {
                    let centers = p
                        .events
                        .iter()
                        .filter(|e| [5, 14].contains(&e.panel))
                        .map(|e| e.panel)
                        .collect::<Vec<_>>();
                    assert!(centers.contains(&5) && centers.contains(&14), "{centers:?}");
                    assert_eq!(i.effective_notes.len(), 14);
                }
                if kind == "held" {
                    assert_eq!(p.holds.len(), 1);
                    assert_eq!(p.holds[0].panel, if mirror { 5 } else { 14 });
                }
                if kind == "hazards" {
                    assert_eq!(i.notes.iter().filter(|n| n.mine).count(), 18);
                    for panel in [5, 14] {
                        assert_eq!(p.mines.iter().filter(|m| m.panel == panel).count(), 2);
                    }
                }
            }
        }
    }
}
#[test]
fn nine_a_windows_preserve_frozen_motion_and_exact_hits() {
    let model =
        ddi_hand_pose::Model::load(include_bytes!("../../../assets/hand/model.json")).unwrap();
    for kind in KINDS {
        for mirror in [false, true] {
            for native in [false, true] {
                common::double_motion::check(
                    &model,
                    input(kind, mirror, native),
                    &format!("9A:{kind}:native={native}"),
                    mirror,
                );
            }
        }
    }
}
#[test]
fn all_allowed_nine_a_jump_pairs_hit_and_rehit_without_simplification() {
    let m = ddi_dance_mat::MatLayout::from_id("danoni-9A").unwrap();
    for a in 0..9 {
        for b in a + 1..9 {
            if !m
                .targets(a)
                .iter()
                .any(|&ta| m.targets(b).iter().any(|&tb| m.pair_allowed(ta, tb)))
            {
                continue;
            }
            let mut song = ddi_chart::formats::sm::parse_sm(
                "#BPMS:0=120;#NOTES:danoni-9A::Hard:1:0,0,0,0,0:100000000;",
            )
            .unwrap();
            song.charts[0].notes = [192, 288]
                .into_iter()
                .flat_map(|t| [a, b].map(|l| Note::new(Tick(t), l as u8, NoteKind::Tap)))
                .collect();
            let i = Input::from_song(&song, 0).unwrap();
            assert_eq!(i.effective_notes.len(), 4);
            plan(&i);
        }
    }
}

#[test]
fn oni_rapid_rehits_and_lift_followups_preserve_aggregate_edges() {
    for mirror in [false, true] {
        for lift in [false, true] {
            let mut song = ddi_chart::formats::sm::parse_sm(
                "#BPMS:0=120;#NOTES:danoni-9A::Hard:1:0,0,0,0,0:000010000;",
            )
            .unwrap();
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
            // A preceding arrow chooses either side without changing Oni's lane.
            song.charts[0].notes.insert(
                0,
                Note::new(Tick(96), if mirror { 0 } else { 8 }, NoteKind::Tap),
            );
            let i = Input::from_song(&song, 0).unwrap();
            plan(&i);
        }
    }
}

#[test]
fn oni_hazard_cannot_be_evaded_using_the_other_center_or_stepmania_hold_grace() {
    let mut song = ddi_chart::formats::sm::parse_sm(
        "#BPMS:0=120;#NOTES:danoni-9A::Hard:1:0,0,0,0,0:000010000;",
    )
    .unwrap();
    song.charts[0].notes = vec![
        Note::new(Tick(192), 4, NoteKind::HoldHead { end: Tick(480) }),
        Note::new(Tick(192), 7, NoteKind::HoldHead { end: Tick(480) }),
        Note::new(Tick(288), 4, NoteKind::Mine),
        Note::new(Tick(288), 7, NoteKind::Mine),
    ];
    let mut i = Input::from_song(&song, 0).unwrap();
    i.rules = vec![("danoni".into(), ddi_engine::rules::presets::danoni().judge)];
    let mut job = PlannerJob::new(i, Options::default());
    for _ in 0..1_000_000 {
        if job.step() {
            break;
        }
    }
    let (p, r) = job.finish().unwrap();
    assert!(p.is_none(), "{r:?}");
}
