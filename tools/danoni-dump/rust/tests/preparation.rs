use ddi_chart::{NoteKind, formats::sm};
use ddi_danoni_export::{compile_chart, model::*, policy, timing};
use serde_json::{Value, json};

fn chart(mode: &str) -> Chart {
    let count = match mode {
        "5" => 5,
        "7" | "7i" => 7,
        _ => 9,
    };
    serde_json::from_value(json!({"id":"synthetic","mode":mode,"lanes":
        (0..count).map(|_| json!({"tapsSeconds":[],"holdsSeconds":[]})).collect::<Vec<_>>() }))
    .unwrap()
}
fn grid(bpm: f64, phase: f64, sub: u8) -> MusicalGrid {
    serde_json::from_value(json!({"bpm":bpm,"beatZeroSeconds":phase,"subdivisionsPerBeat":sub}))
        .unwrap()
}
fn parsed(result: &PreparedChart) -> ddi_chart::Song {
    sm::parse_ssc(&format!(
        "#TITLE:Synthetic;\n#MUSIC:original.wav;\n{}",
        result.ssc
    ))
    .unwrap()
}

#[test]
fn rating_facts_and_configurable_tiers() {
    let mut c = chart("5");
    let mut metadata = Metadata::new();
    metadata.insert("difficulty".into(), json!("Hard 6"));
    assert_eq!(policy::source_stars(&c, &metadata).unwrap(), None);
    metadata.insert("difficulty".into(), json!("★★★★"));
    assert_eq!(policy::source_stars(&c, &metadata).unwrap(), Some(4));
    metadata.insert("difficultyStars".into(), json!(7));
    c.source.insert("difficultyStars".into(), json!(5));
    assert_eq!(policy::source_stars(&c, &metadata).unwrap(), Some(5));
    c.source.insert("difficultyStars".into(), Value::Null);
    // Explicit chart null overrides metadata's number, then falls back to stars.
    assert_eq!(policy::source_stars(&c, &metadata).unwrap(), Some(4));
    for value in [json!(true), json!(-1), json!(1000), json!(1.5), json!("5")] {
        c.source.insert("difficultyStars".into(), value);
        assert!(policy::source_stars(&c, &metadata).is_err());
    }
    assert_eq!(
        [0, 1, 3, 4, 5, 6, 7, 8, 999]
            .map(|n| policy::tier_for_stars(Some(n), [1, 3, 5, 7]).unwrap()),
        [
            Tier::Beginner,
            Tier::Beginner,
            Tier::Easy,
            Tier::Medium,
            Tier::Medium,
            Tier::Hard,
            Tier::Hard,
            Tier::Challenge,
            Tier::Challenge
        ]
    );
    assert_eq!(
        policy::tier_for_stars(None, [1, 3, 5, 7]).unwrap(),
        Tier::Edit
    );
    assert_eq!(
        policy::tier_for_stars(Some(3), [2, 5, 9, 13]).unwrap(),
        Tier::Easy
    );
    for bounds in [[1, 1, 5, 7], [3, 1, 5, 7]] {
        assert!(policy::tier_for_stars(None, bounds).is_err());
    }
}

#[test]
fn all_supported_modes_reach_the_game_parser() {
    for mode in ["5", "7", "7i", "9A", "9B"] {
        let mut c = chart(mode);
        c.lanes[0].taps_seconds = vec![-0.5, 1.0];
        c.lanes.last_mut().unwrap().holds_seconds = vec![[2.0, 3.0]];
        let before = serde_json::to_value(&c).unwrap();
        let r = compile_chart(&c, &Metadata::new(), &PreparationOptions::default()).unwrap();
        let song = parsed(&r);
        let got = &song.charts[0];
        let t = got.timing(&song);
        assert_eq!(got.layout, format!("danoni-{mode}"));
        assert_eq!(got.notes.len(), 3);
        assert_eq!(got.meter, 0);
        assert_eq!(got.difficulty, ddi_chart::Difficulty::Edit);
        assert!((t.seconds_at(got.notes[0].tick) + 0.5).abs() < 1e-12);
        let hold = got
            .notes
            .iter()
            .find(|n| matches!(n.kind, NoteKind::HoldHead { .. }))
            .unwrap();
        let NoteKind::HoldHead { end } = hold.kind else {
            unreachable!()
        };
        assert!((t.seconds_at(hold.tick) - 2.0).abs() < 1e-12);
        assert!((t.seconds_at(end) - 3.0).abs() < 1e-12);
        assert_eq!(serde_json::to_value(&c).unwrap(), before);
    }
    for mode in ["9i", "9t", "11", "custom"] {
        let mut value = serde_json::to_value(chart("9A")).unwrap();
        value["mode"] = json!(mode);
        assert!(serde_json::from_value::<Chart>(value).is_err());
    }
}

#[test]
fn source_frame_rounding_and_negative_pre_roll() {
    let g = grid(120.0, 0.0, 2);
    for (seconds, expected) in [
        (0.25 / 48.0, 0),
        (0.75 / 48.0, 2),
        (-0.25 / 48.0, 0),
        (-0.75 / 48.0, -2),
    ] {
        assert_eq!(timing::position(seconds, &g, false).unwrap().tick, expected);
    }
    for n in -100..100 {
        assert_eq!(timing::source_frame(n as f64 / 60.0).unwrap(), n);
    }
    assert!(timing::source_frame(0.123456).is_err());
    let mut c = chart("5");
    c.lanes[0].taps_seconds = vec![-0.25, 0.25];
    let o = PreparationOptions {
        grid: Some(g),
        ..Default::default()
    };
    let r = compile_chart(&c, &Metadata::new(), &o).unwrap();
    assert_eq!(r.padding_ticks, 48);
    assert_eq!(r.offset_seconds, 0.5);
    let song = parsed(&r);
    assert_eq!(
        song.charts[0]
            .notes
            .iter()
            .map(|n| n.tick.0)
            .collect::<Vec<_>>(),
        [24, 72]
    );
}

#[test]
fn musical_grid_snaps_frame_jitter_but_retains_outliers() {
    let g = grid(173.4, 0.137, 2);
    for n in 0..100 {
        let intended = 0.137 + n as f64 * 60.0 / 173.4 / 2.0;
        let source = (intended * 60.0).round_ties_even() / 60.0;
        let p = timing::position(source, &g, true).unwrap();
        assert!(p.snapped);
        assert_eq!(p.tick, n * 24);
        assert!(p.error_seconds.abs() <= 0.009 + 1e-12);
        assert!((source + p.error_seconds - intended).abs() < 1e-12);
    }
    let mut g = grid(110.0, 0.1, 2);
    let p = timing::position(0.23, &g, true).unwrap();
    assert!(!p.snapped);
    g.maximum_error_ms = 0.01;
    assert!(timing::position(0.23, &g, true).is_err());
}

#[test]
fn short_holds_fall_back_without_collapsing() {
    let mut c = chart("5");
    c.lanes[0].holds_seconds = vec![[0.0, 1.0 / 60.0]];
    let g = grid(180.0, 1.0 / 120.0, 2);
    let r = compile_chart(
        &c,
        &Metadata::new(),
        &PreparationOptions {
            grid: Some(g),
            ..Default::default()
        },
    )
    .unwrap();
    assert!(r.lanes[0].collision_safeguard);
    assert!(
        r.endpoints
            .iter()
            .all(|e| e.method == QuantizationMethod::LaneCollisionFineFallback)
    );
    assert!(r.lanes[0].holds[0][0] < r.lanes[0].holds[0][1]);
    c.lanes[0].holds_seconds = vec![[0.0, 0.0001]];
    assert!(
        compile_chart(
            &c,
            &Metadata::new(),
            &PreparationOptions {
                grid: Some(grid(180.0, 0.0, 2)),
                ..Default::default()
            }
        )
        .is_err()
    );
}

#[test]
fn malformed_relationships_and_nonfinite_times_reject() {
    for (taps, holds) in [
        (vec![1.0, 1.0], vec![]),
        (vec![], vec![[2.0, 1.0]]),
        (vec![], vec![[1.0, 2.0], [2.0, 3.0]]),
        (vec![2.0], vec![[1.0, 3.0]]),
        (vec![f64::NAN], vec![]),
        (vec![f64::INFINITY], vec![]),
        (vec![7201.0], vec![]),
    ] {
        let mut c = chart("5");
        c.lanes[0].taps_seconds = taps;
        c.lanes[0].holds_seconds = holds;
        assert!(compile_chart(&c, &Metadata::new(), &PreparationOptions::default()).is_err());
    }
    assert!(
        compile_chart(
            &chart("5"),
            &Metadata::new(),
            &PreparationOptions::default()
        )
        .is_err()
    );
    let mut g = grid(120.0, 0.0, 2);
    for v in [0.0, f64::NAN, f64::INFINITY, 361.0] {
        g.bpm = v;
        assert!(timing::validate_grid(&g).is_err());
    }
}

#[test]
fn native_scroll_compensates_bpm_beneath_user_multiplier() {
    for bpm in [75.0, 120.0, 200.0] {
        let mut c = chart("5");
        c.lanes[0].taps_seconds = vec![1.0, 2.0];
        let motion = NativeMotion {
            base_arrow_heights_per_second: 2.8,
            evidence: Metadata::new(),
        };
        let o = PreparationOptions {
            grid: Some(grid(bpm, 0.1, 2)),
            native_motion: Some(motion),
            ..Default::default()
        };
        let r = compile_chart(&c, &Metadata::new(), &o).unwrap();
        assert_eq!(r.padding_ticks % 192, 0);
        assert!(r.offset_seconds >= 0.0);
        let song = parsed(&r);
        let got = &song.charts[0];
        let t = got.timing(&song);
        for x in [1.0, 2.0, 3.0] {
            let opt = ddi_engine::scroll::ScrollOptions {
                speed: ddi_engine::scroll::SpeedMod::XMod(x),
                ..Default::default()
            };
            let v = (ddi_engine::scroll::note_y(opt, t, got.notes[0].tick, 0.49)
                - ddi_engine::scroll::note_y(opt, t, got.notes[0].tick, 0.5))
                as f64
                / 0.01;
            assert!((v - 2.8 * x).abs() < 0.001, "{bpm} BPM x{x}: {v}");
        }
    }
    assert!(policy::base_ratio(None, 75.0, 1.2).is_err());
    assert!(policy::base_ratio(None, f64::NAN, 1.0).is_err());
    let motion = NativeMotion {
        base_arrow_heights_per_second: f64::NAN,
        evidence: Metadata::new(),
    };
    assert!(policy::base_ratio(Some(&motion), 75.0, 1.0).is_err());
}

#[test]
fn labels_ratings_and_effect_omissions_stay_distinct() {
    let mut c = chart("7i");
    c.lanes[0].taps_seconds = vec![1.0];
    c.metadata
        .insert("difficultyLabel".into(), json!("Original: #1 / test"));
    c.source
        .insert("colors".into(), json!([{"seconds":1,"color":"white"}]));
    let o = PreparationOptions {
        rating: RatingPolicy::Reviewed { stars: Some(6) },
        ..Default::default()
    };
    let r = compile_chart(&c, &Metadata::new(), &o).unwrap();
    assert!(!r.ssc.contains("#CHARTNAME:"));
    assert_eq!(r.difficulty_classification, Tier::Hard);
    assert_eq!(r.unrepresented_effects["colors"], 1);
    let o = PreparationOptions {
        preserve_label: true,
        ..o
    };
    let song = parsed(&compile_chart(&c, &Metadata::new(), &o).unwrap());
    assert_eq!(song.charts[0].name, "Original: #1 / test");
    c.metadata
        .insert("difficultyLabel".into(), json!("bad;header"));
    assert!(compile_chart(&c, &Metadata::new(), &o).is_err());
    c.metadata.remove("difficultyLabel");
    c.source.insert("colors".into(), json!(true));
    assert!(compile_chart(&c, &Metadata::new(), &o).is_err());
}

#[test]
fn request_options_reject_typos_and_unpaired_holds() {
    let mut request = json!({"schema":"ddi-danoni-compile-request/1","chart":chart("5"),"options":{"scrollFactorTypo":1}});
    assert!(serde_json::from_value::<CompileRequest>(request.clone()).is_err());
    request["options"] = json!({});
    request["chart"]["lanes"][0]["holdsSeconds"] = json!([[1, 2, 3]]);
    assert!(serde_json::from_value::<CompileRequest>(request).is_err());
}

#[test]
fn invalid_grid_and_scroll_inputs_cannot_escape_validation() {
    for value in [
        serde_json::json!({"bpm":120,"beatZeroSeconds":601,"subdivisionsPerBeat":2}),
        serde_json::json!({"bpm":120,"beatZeroSeconds":0,"subdivisionsPerBeat":5}),
        serde_json::json!({"bpm":120,"beatZeroSeconds":0,"subdivisionsPerBeat":2,"maximumErrorMs":0}),
        serde_json::json!({"bpm":120,"beatZeroSeconds":0,"subdivisionsPerBeat":2,"maximumErrorMs":10}),
    ] {
        let g: MusicalGrid = serde_json::from_value(value).unwrap();
        assert!(timing::validate_grid(&g).is_err());
    }
    let mut c = chart("5");
    c.lanes[0].taps_seconds = vec![1.0];
    c.speed = vec![SpeedEvent {
        seconds: 0.123456,
        multiplier: 1.0,
        source: Metadata::new(),
    }];
    let o = PreparationOptions {
        grid: Some(grid(120.0, 0.0, 2)),
        ..Default::default()
    };
    assert!(compile_chart(&c, &Metadata::new(), &o).is_err());
    c.speed[0].seconds = 1.0;
    c.speed[0].multiplier = f64::INFINITY;
    assert!(compile_chart(&c, &Metadata::new(), &o).is_err());
    assert!(policy::base_ratio(None, 75.0, f64::NAN).is_err());
    assert!(policy::base_ratio(None, 75.0, 0.0).is_err());
    c.speed.clear();
    let edit = PreparationOptions {
        rating: RatingPolicy::Reviewed { stars: Some(6) },
        tier_policy: TierPolicy::Edit,
        ..Default::default()
    };
    let result = compile_chart(&c, &Metadata::new(), &edit).unwrap();
    assert_eq!(result.difficulty_stars, Some(6));
    assert_eq!(result.difficulty_classification, Tier::Edit);
}
