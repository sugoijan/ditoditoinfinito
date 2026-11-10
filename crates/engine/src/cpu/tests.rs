use super::*;
fn fixture() -> (Song, Profile) {
    let path =
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../assets/songs/beatucada");
    let path = std::fs::read_dir(path)
        .unwrap()
        .flatten()
        .map(|e| e.path())
        .find(|p| p.extension().is_some_and(|e| e == "ssc"))
        .unwrap();
    let song =
        ddi_library::scan::parse_simfile(&std::fs::read_to_string(path).unwrap(), "ssc").unwrap();
    let projected = project(&song, 1).unwrap();
    let chart = &projected.charts[1];
    let judge = Judge::new(&chart.notes, chart.timing(&projected), presets::itg().judge);
    let rows = rows(judge.notes()).unwrap();
    let mut q = vec![0.; 91];
    q[40] = 0.8;
    q[88] = 0.1;
    q[89] = 0.1;
    let profile = Profile {
        version: 1,
        chart_digest: song.chart_digest(1),
        model: "test".into(),
        edges: (0..89).map(|i| -0.18 + i as f64 * 0.36 / 88.).collect(),
        laws: vec![vec![q; rows.len()]; 5],
        rows,
    };
    (song, profile)
}
#[test]
fn source_projection_and_binding() {
    let (song, p) = fixture();
    let before = song.clone();
    let projected = p.check_song(&song, 1).unwrap();
    assert_eq!(song, before);
    assert!(
        projected.charts[1]
            .notes
            .iter()
            .all(|n| n.kind == NoteKind::Tap)
    );
    assert!(p.check_song(&song, 0).is_err());
    let mut stale = p.clone();
    stale.rows[0].seconds += 0.001;
    assert!(stale.check_song(&song, 1).is_err());
}
#[test]
fn reproducible_legal_whole_song_motor() {
    let (_, p) = fixture();
    let a = p.script(2, 1).unwrap();
    assert_eq!(a, p.script(2, 1).unwrap());
    assert_ne!(a, p.script(2, 2).unwrap());
    assert!(a.len() > 32);
    assert!(a.last().unwrap().press > 60.);
    validate_script(&a).unwrap();
    let mut dense = p.clone();
    for (i, r) in dense.rows.iter_mut().enumerate() {
        r.seconds = i as f64 * 0.001;
        r.mask = 1;
    }
    let a = dense.script(2, 1).unwrap();
    validate_script(&a).unwrap();
    assert!(a.last().unwrap().press > a.len() as f64 * 0.01);
}
#[test]
fn bounded_validation_and_decoder_atoms() {
    let (_, p) = fixture();
    p.validate().unwrap();
    let mut bad = p.clone();
    bad.laws[0][0][0] = f64::NAN;
    assert!(bad.validate().is_err());
    let mut bad = p.clone();
    bad.rows[0].mask = 16;
    assert!(bad.validate().is_err());
    let mut q = vec![0.; 91];
    q[88] = 1.;
    assert_eq!(p.offset(&q, 0.5, 0.4), -0.18);
    q[88] = 0.;
    q[89] = 1.;
    assert_eq!(p.offset(&q, 0.5, 0.4), 0.18);
    assert!(p.script(5, 1).is_err());
    assert!(
        validate_script(&[Press {
            lane: 4,
            press: 0.,
            release: 0.012
        }])
        .is_err()
    );
}
#[test]
fn fixed_random_contract() {
    let mut r = Random(0);
    assert_eq!(r.unit(), 0.8833108082136426);
    assert_eq!(r.unit(), 0.43152799704850997);
}

#[test]
fn automatic_plans_cover_short_long_and_import_defined_four_key_charts() {
    let (mut song, _) = fixture();
    let source = song.clone();
    let plan = Plan::automatic(&song, 1).unwrap();
    assert_eq!(song, source);
    assert_eq!(plan.model(), "analytical-runtime-v1");
    plan.check_song(&song, 1).unwrap();
    assert!(plan.check_song(&song, 0).is_err());
    for level in 0..5 {
        let script = plan.script(level, 7).unwrap();
        validate_script(&script).unwrap();
        assert_eq!(script, plan.script(level, 7).unwrap());
        assert_ne!(script, plan.script(level, 8).unwrap());
    }
    song.charts[1].notes.truncate(1);
    let short = Plan::automatic(&song, 1).unwrap();
    assert_eq!(short.script(2, 7).unwrap().len(), 1);
    let mut layout = song.layout_of(&song.charts[1]).unwrap();
    layout.id = "import-four".into();
    song.charts[1].layout.clone_from(&layout.id);
    song.layouts.push(layout);
    Plan::automatic(&song, 1).unwrap();
    song.layouts[0].lanes.pop();
    assert!(Plan::automatic(&song, 1).is_err());
    song.charts[1].layout = "dance-single".into();
    song.charts[1].notes.clear();
    assert!(Plan::automatic(&song, 1).is_err());
    assert!(Plan::automatic(&song, usize::MAX).is_err());
    song.charts[1].notes = (0..5000)
        .map(|i| ddi_chart::Note::new(Tick(i * 12), (i % 4) as u8, NoteKind::Tap))
        .collect();
    let long = Plan::automatic(&song, 1).unwrap();
    assert!(long.script(2, 7).unwrap().len() > MAX_ROWS);
    song.charts[1].notes = (0..=MAX_AUTOMATIC_ROWS)
        .map(|i| ddi_chart::Note::new(Tick(i as i64), (i % 4) as u8, NoteKind::Tap))
        .collect();
    assert!(Plan::automatic(&song, 1).is_err());
}

#[test]
fn automatic_strength_improves_average_judged_score() {
    let (song, _) = fixture();
    let plan = Plan::automatic(&song, 1).unwrap();
    let projected = plan.check_song(&song, 1).unwrap();
    let mut totals = [0.; 5];
    for (level, total) in totals.iter_mut().enumerate() {
        for seed in 0..32 {
            let script = plan.script(level, seed).unwrap();
            let mut inputs: Vec<_> = script
                .iter()
                .flat_map(|p| {
                    [
                        crate::history::LoggedInput {
                            time: p.press,
                            lane: p.lane,
                            pressed: true,
                        },
                        crate::history::LoggedInput {
                            time: p.release,
                            lane: p.lane,
                            pressed: false,
                        },
                    ]
                })
                .collect();
            inputs.sort_by(|a, b| a.time.total_cmp(&b.time));
            let log = crate::history::PlayLog {
                inputs,
                ..Default::default()
            };
            let result = crate::history::replay_with(
                &projected,
                1,
                presets::itg(),
                &Default::default(),
                &log,
            );
            *total += result.score.percent.unwrap();
        }
    }
    assert!(totals.windows(2).all(|t| t[0] < t[1]), "{totals:?}");
}

#[test]
fn web_selection_is_explicit_and_bounded() {
    let s = Selection::parse("1", "expert", "18446744073709551615").unwrap();
    assert_eq!(s.level, 4);
    assert_eq!(s.seed, u64::MAX);
    assert!(s.path.is_none());
    assert_eq!(
        Selection::parse("cpu/custom.json", "0.9", "7")
            .unwrap()
            .level,
        1
    );
    for path in [
        "",
        "https://example.org/cpu.json",
        "//host/cpu.json",
        "../private.json",
        "/cpu.json",
        "cpu/./a.json",
        "cpu\\a.json",
    ] {
        assert!(Selection::parse(path, "normal", "1").is_err(), "{path}");
    }
    for seed in ["-1", "1.5", "18446744073709551616", "NaN"] {
        assert!(Selection::parse("1", "normal", seed).is_err());
    }
    assert!(Selection::parse("1", "bad", "1").is_err());
}

#[test]
fn skills_preserve_balanced_motor_and_reject_prepared_modification() {
    let (song, p) = fixture();
    let plan = Plan::automatic(&song, 1).unwrap();
    for level in 0..5 {
        assert_eq!(
            plan.script(level, 7).unwrap(),
            plan.script_with_skills(level, 7, Skills::default())
                .unwrap()
        );
        for preset in ["stream", "jacks", "chords", "stamina", "technical"] {
            let skills = Skills::parse(preset).unwrap();
            let script = plan.script_with_skills(level, 7, skills).unwrap();
            validate_script(&script).unwrap();
            assert_eq!(script, plan.script_with_skills(level, 7, skills).unwrap());
        }
    }
    let prepared = Plan::Prepared(p);
    assert!(
        prepared
            .script_with_skills(2, 7, Skills::parse("jacks").unwrap())
            .is_err()
    );
    assert!(
        Selection::parse("cpu/a.json", "normal", "1")
            .unwrap()
            .with_skills("jacks")
            .is_err()
    );
    assert_eq!(
        Selection::parse("1", "normal", "1")
            .unwrap()
            .with_skills("jacks")
            .unwrap()
            .skills,
        Skills::parse("jacks").unwrap()
    );
}

#[test]
fn each_skill_improves_actual_judged_inputs_on_relevant_patterns() {
    let (mut song, _) = fixture();
    // 125 BPM: twelve ticks = 120 ms. Irregular pattern alternates 80/160 ms.
    song.timing.bpms = vec![ddi_chart::BpmSegment {
        tick: Tick(0),
        bpm: 125.,
    }];
    let masks: [&[u8]; 7] = [
        &[1, 4, 2, 8],
        &[3, 12],
        &[7, 14],
        &[1, 4, 2, 8],
        &[1],
        &[3],
        &[1, 4, 2, 8],
    ];
    for (axis, pattern) in masks.iter().enumerate() {
        song.charts[1].timing = None;
        song.charts[1].notes = (0..128)
            .flat_map(|i| {
                let tick = if axis == 6 {
                    i / 2 * 24 + if i % 2 == 1 { 8 } else { 0 }
                } else {
                    i * 12
                };
                (0..4)
                    .filter(move |lane| pattern[i as usize % pattern.len()] & (1 << lane) != 0)
                    .map(move |lane| ddi_chart::Note::new(Tick(tick), lane, NoteKind::Tap))
            })
            .collect();
        let plan = Plan::automatic(&song, 1).unwrap();
        let projected = plan.check_song(&song, 1).unwrap();
        let mut totals = [0.; 3];
        for (k, multiplier) in [0.5, 1., 2.].into_iter().enumerate() {
            let mut values = [1.; 7];
            values[axis] = multiplier;
            for seed in 0..32 {
                let script = plan
                    .script_with_skills(2, seed, Skills::new(values).unwrap())
                    .unwrap();
                let mut inputs: Vec<_> = script
                    .iter()
                    .flat_map(|p| {
                        [
                            crate::history::LoggedInput {
                                time: p.press,
                                lane: p.lane,
                                pressed: true,
                            },
                            crate::history::LoggedInput {
                                time: p.release,
                                lane: p.lane,
                                pressed: false,
                            },
                        ]
                    })
                    .collect();
                inputs.sort_by(|a, b| a.time.total_cmp(&b.time));
                let mut rules = presets::itg();
                rules.fail = crate::FailPolicy::Off;
                let result = crate::history::replay_with(
                    &projected,
                    1,
                    rules,
                    &Default::default(),
                    &crate::history::PlayLog {
                        inputs,
                        ..Default::default()
                    },
                );
                totals[k] += result.score.percent.unwrap();
            }
        }
        assert!(
            totals[0] < totals[1] && totals[1] < totals[2],
            "{}: {totals:?}",
            SKILL_NAMES[axis]
        );
    }
}
