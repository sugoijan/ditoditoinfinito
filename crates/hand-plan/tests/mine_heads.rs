// SPDX-FileCopyrightText: 2026 すごいジャン <me@sugoijan.dev>
// SPDX-License-Identifier: MIT
use ddi_chart::{Note, NoteKind, Tick};
use ddi_hand_plan::{Input, Options, PlannerJob};
mod common;
fn input(kind: &str, mirror: bool) -> Input {
    let bpm = match kind {
        "arrival" | "departure" => 150,
        "guarded-exit" => 220,
        "staggered" | "staggered-tail" => 130,
        "staggered-hazards" => 132,
        "coincident" => 10000,
        "bracket" => 180,
        "support" => 128,
        "repeat" => 140,
        "jump" | "cluster" => 185,
        _ => 179,
    };
    let single = matches!(
        kind,
        "arrival" | "departure" | "guarded-exit" | "coincident"
    );
    let layout = if matches!(kind, "staggered" | "staggered-tail") {
        "danoni-5"
    } else if single {
        "dance-single"
    } else {
        "dance-double"
    };
    let row = if matches!(kind, "staggered" | "staggered-tail") {
        "10000"
    } else if single {
        "1000"
    } else {
        "10000000"
    };
    let mut song = ddi_chart::formats::sm::parse_sm(&format!(
        "#OFFSET:-4;#BPMS:0={bpm};#NOTES:{layout}::Hard:1:0,0,0,0,0:{row};"
    ))
    .unwrap();
    let mut notes = Vec::new();
    let mut add = |tick, lane, kind| notes.push(Note::new(Tick(tick), lane, kind));
    match kind {
        "staggered" | "staggered-tail" => {
            for (tick, lane) in [
                (192, 0),
                (195, 4),
                (198, 1),
                (228, 1),
                (231, 4),
                (234, 2),
                (264, 2),
                (267, 4),
                (270, 3),
            ] {
                add(tick, lane, NoteKind::Tap);
            }
            if kind == "staggered" {
                add(288, 0, NoteKind::HoldHead { end: Tick(336) });
                add(336, 4, NoteKind::Tap);
                add(384, 2, NoteKind::Tap);
                add(384, 4, NoteKind::Tap);
            }
        }
        "coincident" => {
            add(192, 0, NoteKind::Mine);
            add(197, 2, NoteKind::Mine);
            add(198, 0, NoteKind::Tap);
            add(1200, 1, NoteKind::Tap);
        }
        "staggered-hazards" => {
            for (tick, lane) in [(192, 3), (216, 4), (252, 3), (288, 2), (312, 3), (360, 4)] {
                add(tick, lane, NoteKind::Tap);
            }
            for tick in [240, 276, 300] {
                for lane in 0..4 {
                    add(tick, lane, NoteKind::Mine);
                }
            }
        }
        "arrival" => {
            add(192, 2, NoteKind::Tap);
            add(192, 3, NoteKind::Tap);
            for tick in [216, 228, 240] {
                for lane in 0..4 {
                    add(tick, lane, NoteKind::Mine);
                }
            }
            for (tick, lane) in [(252, 3), (276, 1), (300, 2)] {
                add(tick, lane, NoteKind::Tap);
            }
        }
        "guarded-exit" => {
            // A repeated UP hit leaves less than 95 ms for the following
            // mine. Its connected departure can end on the exact RIGHT hit.
            for (tick, lane) in [(192, 2), (204, 0), (216, 2), (228, 0), (240, 3)] {
                add(tick, lane, NoteKind::Tap);
            }
            add(240, 2, NoteKind::Mine);
        }
        "departure" => {
            add(192, 3, NoteKind::Tap);
            for lane in 0..4 {
                add(204, lane, NoteKind::Mine);
            }
            add(228, 1, NoteKind::Tap);
            add(252, 2, NoteKind::Tap);
        }
        "bracket" => {
            // Isolate two 83 ms brackets with a quiet interval, so this checks
            // exact-mode contact precision. Full-chart runs also check stream.
            for (tick, lane) in [(192, 2), (204, 4), (216, 2), (360, 4), (372, 3), (384, 4)] {
                add(tick, lane, NoteKind::Tap);
            }
            for (tick, lanes) in [(192, 4..8), (216, 4..8), (360, 0..4), (384, 0..4)] {
                for lane in lanes {
                    add(tick, lane, NoteKind::Mine);
                }
            }
        }
        "support" => {
            // Authored alternating support/flight pattern. Neither fixed
            // center is a legal final stance beside the seam-facing tap.
            for i in 0..10 {
                let tick = 192 + i * 24;
                let lane = if i % 2 == 0 { 3 } else { 4 };
                add(tick, lane, NoteKind::Tap);
                if i != 1 {
                    for mine_lane in if lane == 3 { 4..8 } else { 0..4 } {
                        add(tick, mine_lane, NoteKind::Mine);
                    }
                }
            }
        }
        "repeat" => {
            add(192, 3, NoteKind::Tap);
            add(192, 4, NoteKind::Tap);
            add(240, 4, NoteKind::Tap);
            for lane in 4..8 {
                add(264, lane, NoteKind::Mine);
            }
            add(288, 4, NoteKind::HoldHead { end: Tick(336) });
        }
        "jump" => {
            add(192, 3, NoteKind::HoldHead { end: Tick(240) });
            add(192, 4, NoteKind::Tap);
            add(240, 2, NoteKind::HoldHead { end: Tick(264) });
            for lane in 0..8 {
                add(312, lane, NoteKind::Mine);
            }
            add(336, 2, NoteKind::Tap);
        }
        "cluster" | "unknown" => {
            add(192, 3, NoteKind::HoldHead { end: Tick(288) });
            add(192, 5, NoteKind::Tap);
            add(240, 4, NoteKind::HoldHead { end: Tick(288) });
            let step = if kind == "cluster" { 12 } else { 24 };
            for tick in [336, 336 + step, 336 + 2 * step] {
                for lane in 0..8 {
                    add(tick, lane, NoteKind::Mine);
                }
            }
            add(336 + 4 * step, 3, NoteKind::Tap);
            add(336 + 6 * step, 4, NoteKind::Tap);
        }
        _ => {
            add(192, 2, NoteKind::Tap);
            add(216, 1, NoteKind::Tap);
            for (tick, lane) in [(252, 0), (288, 1), (324, 3), (360, 4)] {
                for mine_lane in 0..8 {
                    add(tick - 12, mine_lane, NoteKind::Mine);
                }
                add(tick, lane, NoteKind::Tap);
            }
        }
    }
    if mirror {
        for n in &mut notes {
            n.lane = if matches!(kind, "staggered" | "staggered-tail") {
                [3, 1, 2, 0, 4][n.lane as usize]
            } else if single {
                [3, 1, 2, 0][n.lane as usize]
            } else {
                [7, 5, 6, 4, 3, 1, 2, 0][n.lane as usize]
            };
        }
    }
    notes.sort_by_key(|n| (n.tick, n.lane));
    song.charts[0].notes = notes;
    Input::from_song(&song, 0).unwrap()
}
#[test]
fn mine_clearance_can_end_on_repeats_holds_and_staggered_scored_landings() {
    for kind in [
        "repeat",
        "jump",
        "alternating",
        "cluster",
        "unknown",
        "support",
        "arrival",
        "departure",
        "guarded-exit",
        "staggered",
        "coincident",
        "staggered-hazards",
        "bracket",
    ] {
        for mirror in [false, true] {
            let input = input(kind, mirror);
            let mut job = PlannerJob::new(input.clone(), Options::default());
            for _ in 0..1_000_000 {
                if job.step() {
                    break;
                }
            }
            let (p, r) = job.finish().expect("bounded search");
            let p = p.unwrap_or_else(|| panic!("{kind} {mirror}: {r:?}"));
            if matches!(kind, "staggered" | "staggered-tail") {
                assert_eq!(r.attempts.last().unwrap().staggered_landings, Some(true));
                assert!(
                    r.attempts
                        .iter()
                        .rev()
                        .skip(1)
                        .any(|a| a.adaptive_timing == Some(true) && a.staggered_landings.is_none())
                );
            }
            if kind == "coincident" {
                assert!(r.attempts.is_empty());
            }
            assert!(ddi_hand_plan::replay(&input, &p).unwrap().passed);
            assert!(
                p.events
                    .iter()
                    .all(|e| e.music_time == input.timing.seconds_at(Tick(e.tick)))
            );
            if kind == "repeat" || kind == "alternating" || kind == "support" {
                assert!(p.tracks.left.iter().chain(&p.tracks.right).any(|s| {
                    p.mines
                        .iter()
                        .any(|m| s.release < m.time && s.land > m.time)
                        && p.events.iter().any(|e| e.time == s.land && e.panel == s.to)
                }));
            }
        }
    }
}
#[test]
fn mine_head_landings_keep_rolling_prefixes_contacts_and_skin_clearance() {
    let m = ddi_hand_pose::Model::load(include_bytes!("../../../assets/hand/model.json")).unwrap();
    for kind in [
        "repeat",
        "jump",
        "alternating",
        "cluster",
        "unknown",
        "support",
        "arrival",
        "departure",
        "guarded-exit",
        "staggered",
        "coincident",
        "staggered-hazards",
        "bracket",
    ] {
        for mirror in [false, true] {
            common::double_motion::check(&m, input(kind, mirror), kind, mirror);
        }
    }
}

#[test]
fn burst_then_empty_tail_remains_prepared_through_motion_end() {
    let model =
        ddi_hand_pose::Model::load(include_bytes!("../../../assets/hand/model.json")).unwrap();
    for mirror in [false, true] {
        for hz in [4, 60] {
            common::double_motion::check_at_hz(
                &model,
                input("staggered-tail", mirror),
                "staggered-tail",
                mirror,
                hz,
            );
        }
    }
}
