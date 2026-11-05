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
