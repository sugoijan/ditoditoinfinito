// SPDX-FileCopyrightText: 2026 すごいジャン <me@sugoijan.dev>
// SPDX-License-Identifier: MIT
use common::double_plan as plan;
use ddi_chart::{Note, NoteKind, Tick};
use ddi_hand_plan::Input;
mod common;
fn input(layout: &str, kind: &str, mirror: bool) -> Input {
    let mut song = ddi_chart::formats::sm::parse_sm(
        "#BPMS:0=120;#NOTES:pump-double::Hard:1:0,0,0,0,0:1000000000;",
    )
    .unwrap();
    song.charts[0].layout = layout.into();
    let panels = ddi_hand_plan::layout_panels(layout).unwrap();
    let mut notes = Vec::new();
    let mut add = |tick, p: u8, kind| {
        let p = if mirror {
            let c = (p - 1) % 9 + 1;
            (1 - (p - 1) / 9) * 9 + c + 2 - 2 * ((c - 1) % 3)
        } else {
            p
        };
        let lane = panels.iter().position(|t| *t == p).expect("scored tile") as u8;
        notes.push(Note::new(Tick(tick), lane, kind));
    };
    match kind {
        "walking" => {
            for (i, &p) in panels.iter().enumerate() {
                add(192 + i as i64 * 72, p, NoteKind::Tap);
            }
        }
        "jumps" => {
            for (i, [a, b]) in [[9, 3], [5, 14], [5, 14], [16, 10], [3, 16], [9, 10]]
                .into_iter()
                .enumerate()
            {
                add(192 + i as i64 * 96, a, NoteKind::Tap);
                add(192 + i as i64 * 96, b, NoteKind::Tap);
            }
        }
        "held" => {
            add(192, 5, NoteKind::HoldHead { end: Tick(672) });
            for (i, p) in [10, 16, 14, 10].into_iter().enumerate() {
                add(288 + i as i64 * 96, p, NoteKind::Tap);
            }
            add(768, 14, NoteKind::Tap);
        }
        "held-lift" => {
            add(96, 3, NoteKind::Tap);
            add(192, 5, NoteKind::HoldHead { end: Tick(672) });
            add(288, 14, NoteKind::Tap);
            add(432, 14, NoteKind::Lift);
            add(576, 14, NoteKind::Tap);
            add(768, 9, NoteKind::Tap);
        }
        "roll" => {
            add(192, 14, NoteKind::RollHead { end: Tick(528) });
            add(288, 5, NoteKind::Tap);
            add(384, 9, NoteKind::Tap);
            add(480, 10, NoteKind::Tap);
            add(624, 14, NoteKind::Tap);
            add(720, 14, NoteKind::Lift);
            add(864, 5, NoteKind::Tap);
        }
        "hazards" => {
            add(192, 5, NoteKind::Tap);
            add(192, 14, NoteKind::Tap);
            for &p in panels {
                add(336, p, NoteKind::Mine);
            }
            add(480, 5, NoteKind::Tap);
            add(480, 14, NoteKind::Tap);
            for &p in panels {
                add(624, p, NoteKind::Shock);
            }
            add(816, 14, NoteKind::Lift);
            add(936, 5, NoteKind::Tap);
        }
        "immediate" | "delayed" => {
            let start = if kind == "immediate" { 48 } else { 300 };
            for (i, p) in [14, 16, 9, 5].into_iter().enumerate() {
                add(start + i as i64 * 96, p, NoteKind::Tap);
            }
        }
        _ => unreachable!(),
    }
    notes.sort_by_key(|n| (n.tick, n.lane));
    song.charts[0].notes = notes;
    Input::from_song(&song, 0).unwrap()
}
#[test]
fn pump_double_layouts_preserve_all_contacts_including_centers_and_hazards() {
    assert_eq!(
        ddi_hand_plan::layout_panels("pump-halfdouble").unwrap(),
        &[5, 9, 3, 10, 16, 14]
    );
    assert_eq!(
        ddi_hand_plan::layout_panels("pump-double").unwrap(),
        &[1, 7, 5, 9, 3, 10, 16, 14, 18, 12]
    );
    for layout in ["pump-halfdouble", "pump-double"] {
        for kind in [
            "walking",
            "jumps",
            "held",
            "held-lift",
            "roll",
            "hazards",
            "immediate",
            "delayed",
        ] {
            for mirror in [false, true] {
                plan(&input(layout, kind, mirror));
            }
        }
    }
}
#[test]
fn pump_double_windows_preserve_frozen_motion_and_exact_hits() {
    let model =
        ddi_hand_pose::Model::load(include_bytes!("../../../assets/hand/model.json")).unwrap();
    for layout in ["pump-halfdouble", "pump-double"] {
        for kind in [
            "walking",
            "jumps",
            "held",
            "held-lift",
            "roll",
            "hazards",
            "immediate",
            "delayed",
        ] {
            for mirror in [false, true] {
                common::double_motion::check(
                    &model,
                    input(layout, kind, mirror),
                    &format!("{layout}:{kind}"),
                    mirror,
                );
            }
        }
    }
}
#[test]
fn all_allowed_pump_double_jump_pairs_hit_and_rehit_without_simplification() {
    for layout in ["pump-halfdouble", "pump-double"] {
        let m = ddi_dance_mat::MatLayout::from_id(layout).unwrap();
        for a in 0..m.lane_count() {
            for b in a + 1..m.lane_count() {
                if !m.pair_allowed(m.targets(a)[0], m.targets(b)[0]) {
                    continue;
                }
                let mut song = ddi_chart::formats::sm::parse_sm(
                    "#BPMS:0=120;#NOTES:pump-double::Hard:1:0,0,0,0,0:1000000000;",
                )
                .unwrap();
                song.charts[0].layout = layout.into();
                song.charts[0].notes = [192, 288]
                    .into_iter()
                    .flat_map(|tick| {
                        [a, b].map(|lane| Note::new(Tick(tick), lane as u8, NoteKind::Tap))
                    })
                    .collect();
                let i = Input::from_song(&song, 0).unwrap();
                assert_eq!(i.notes.len(), 4);
                plan(&i);
            }
        }
    }
}
