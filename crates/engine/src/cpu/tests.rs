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

/// The solo CPU at a level of the opponents' absolute scale (`level=N`,
/// `--cpu-level N`): 1 to 10, automatic plans alone; its presses are the
/// level's, and without a level the strength's exactly as before.
#[test]
fn a_solo_cpu_plays_at_an_absolute_level_where_asked() {
    let s = Selection::parse("1", "normal", "7")
        .unwrap()
        .with_level("3")
        .unwrap();
    assert_eq!((s.level, s.absolute), (2, Some(2)));
    for bad in ["0", "11", "x", "-1", "2.5"] {
        assert!(absolute_level(bad).is_err(), "{bad}");
        assert!(Selection::parse("1", "normal", "7").unwrap().with_level(bad).is_err());
    }
    assert!(
        Selection::parse("cpu/custom.json", "normal", "7")
            .unwrap()
            .with_level("3")
            .is_err()
    );
    let (song, profile) = fixture();
    let plan = Plan::automatic(&song, 1).unwrap();
    let at = s.script(&plan).unwrap();
    let opponent = Opponent::on_scale(
        &song,
        1,
        presets::itg(),
        LevelScale::AbsoluteV1,
        2,
        7,
        Skills::default(),
    )
    .unwrap();
    assert_eq!(at, opponent.presses());
    let plain = Selection::parse("1", "normal", "7").unwrap();
    assert_eq!(
        plain.script(&plan).unwrap(),
        plan.script_with_skills(2, 7, Skills::default()).unwrap()
    );
    assert!(solo_script(&Plan::Prepared(profile), 2, Some(2), 7, Skills::default()).is_err());
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

fn press_hash(presses: &[Press]) -> u64 {
    let mut h = 0xcbf2_9ce4_8422_2325_u64;
    let mut eat = |v: u64| {
        for b in v.to_le_bytes() {
            h ^= u64::from(b);
            h = h.wrapping_mul(0x0000_0100_0000_01b3);
        }
    };
    for p in presses {
        eat(p.press.to_bits());
        eat(p.release.to_bits());
        eat(u64::from(p.lane));
    }
    h
}

/// Levels 6 to 10 are the five frozen strengths: the presses of chart 0
/// of the bundled song at seed 7, balanced and with two personalities, as
/// they were before there were ten levels (hashes taken from the code of
/// 2026-11-11).
#[test]
fn levels_six_to_ten_press_exactly_as_the_five_strengths_did() {
    const BEFORE: [[u64; 3]; 5] = [
        [0x820ed3d9153bf986, 0xe746af097ec52e9a, 0xf06f06be23a9e82a],
        [0x922274e3c41af647, 0x9801947c055ac1ae, 0xda14ddb80d5acbf1],
        [0xeec836368f0bc6fb, 0x9de23531965e4ec8, 0x8864d97d653cd870],
        [0x0ed9b85af55dbabe, 0x18e41ed5cb805c58, 0xbeb840a18f2c530d],
        [0x61232be8f535e653, 0xeee0f2767c4eadc4, 0x4754b97efb73e386],
    ];
    let (song, _) = fixture();
    let plan = Plan::automatic(&song, 0).unwrap();
    let skills = [
        Skills::default(),
        Skills::parse("stream").unwrap(),
        Skills::parse("chords").unwrap(),
    ];
    for (strength, before) in BEFORE.iter().enumerate() {
        for (skills, hash) in skills.iter().zip(before) {
            // The solo CPU's strength, and the opponent's level it became.
            let solo = plan.script_with_skills(strength, 7, *skills).unwrap();
            assert_eq!(press_hash(&solo), *hash, "strength {strength}");
            let level = CPU_LEVELS_V0.level(5 + strength).unwrap();
            let at = plan.script_at(level, 7, *skills).unwrap();
            assert_eq!(at, solo, "level {}", 6 + strength);
            let opponent = Opponent::on_scale(
                &song,
                0,
                presets::itg_fa_plus(),
                LevelScale::RelativeV0,
                5 + strength,
                7,
                *skills,
            )
            .unwrap();
            assert_eq!(
                press_hash(opponent.presses()),
                *hash,
                "level {}",
                6 + strength
            );
        }
    }
}

/// The weaker five: deterministic, legal, each its own presses, fewer
/// notes pressed the weaker the level (its lapses).
#[test]
fn levels_one_to_five_lapse_more_the_weaker_they_are() {
    let (song, _) = fixture();
    let plan = Plan::automatic(&song, 0).unwrap();
    let mut pressed = Vec::new();
    for index in 0..5 {
        let level = CPU_LEVELS_V0.level(index).unwrap();
        let script = plan.script_at(level, 7, Skills::default()).unwrap();
        validate_script(&script).unwrap();
        assert_eq!(script, plan.script_at(level, 7, Skills::default()).unwrap());
        assert_ne!(script, plan.script_at(level, 8, Skills::default()).unwrap());
        let total: usize = (1..=16)
            .map(|seed| {
                plan.script_at(level, seed, Skills::default())
                    .unwrap()
                    .len()
            })
            .sum();
        pressed.push(total);
    }
    assert!(pressed.windows(2).all(|w| w[0] < w[1]), "{pressed:?}");
    // A level without lapse at a strength of its own presses every note
    // but the omitted few; a prepared profile has no such level.
    let plain = Level {
        strength: 0.6,
        lapse: 0.0,
    };
    assert!(plan.script_at(plain, 7, Skills::default()).unwrap().len() >= 275);
    let (_, profile) = fixture();
    assert!(
        Plan::Prepared(profile)
            .script_at(plain, 7, Skills::default())
            .is_err()
    );
}

/// Every bundled song's simfile, parsed as the library parses it.
fn bundled_songs() -> Vec<Song> {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../assets/songs");
    let mut dirs: Vec<_> = std::fs::read_dir(root)
        .unwrap()
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.is_dir())
        .collect();
    dirs.sort();
    dirs.iter()
        .filter_map(|d| {
            let path = std::fs::read_dir(d)
                .ok()?
                .flatten()
                .map(|e| e.path())
                .find(|p| p.extension().is_some_and(|e| e == "ssc" || e == "sm"))?;
            let ext = path.extension()?.to_string_lossy().to_string();
            ddi_library::scan::parse_simfile(&std::fs::read_to_string(&path).ok()?, &ext).ok()
        })
        .collect()
}

/// The mean percent under DDI Normal of an opponent at `level` of the
/// opponents' ten (`cpu-levels-v1`) over `seeds`, the fail off.
fn absolute_percent(song: &Song, chart: usize, level: usize, seeds: u64) -> f64 {
    let plan = Plan::automatic(song, chart).unwrap();
    let mut rules = crate::rules::ddi::ruleset(&crate::rules::ddi::DdiOptions::default());
    rules.fail = crate::rules::FailPolicy::Off;
    let strength = LevelScale::AbsoluteV1.strength(level).unwrap();
    (1..=seeds)
        .map(|seed| {
            let mut o = Opponent::planned(
                &plan,
                song,
                chart,
                rules.fresh(),
                (strength, 0.0),
                seed,
                Skills::default(),
            )
            .unwrap();
            o.start(0.0);
            o.finish();
            o.score().percent.unwrap_or(0.0) * 100.0
        })
        .sum::<f64>()
        / seeds as f64
}

/// `cpu-levels-v1`: on every bundled single chart a stronger level scores
/// no worse (within half a point: four seeds), and a level scores no
/// better on a harder chart of the same song (within two points: the
/// slots of a song can be close, as Beatucada's Hard and Challenge are).
/// An easy chart is played nearly perfectly by a middling level and the
/// weakest are lost on the hardest.
#[test]
fn absolute_levels_are_monotone_in_level_and_in_the_chart() {
    let mut charts = 0;
    for song in bundled_songs() {
        let mut singles: Vec<usize> = (0..song.charts.len())
            .filter(|&i| {
                let c = &song.charts[i];
                c.layout == "dance-single"
                    && c.derivation.is_none()
                    && c.difficulty != ddi_chart::Difficulty::Edit
                    && Plan::automatic(&song, i).is_ok()
            })
            .collect();
        singles.sort_by_key(|&i| song.charts[i].difficulty);
        let table: Vec<Vec<f64>> = singles
            .iter()
            .map(|&c| (0..LEVELS).map(|l| absolute_percent(&song, c, l, 4)).collect())
            .collect();
        for (row, &c) in table.iter().zip(&singles) {
            charts += 1;
            for l in 1..LEVELS {
                assert!(
                    row[l] >= row[l - 1] - 0.5,
                    "chart {c}: level {} {} below level {} {}",
                    l + 1,
                    row[l],
                    l,
                    row[l - 1]
                );
            }
        }
        for (k, pair) in table.windows(2).enumerate() {
            // Charts of one meter may be close either way, and so may a
            // harder chart with more holds: kept, they are easy points
            // (DDI's hold units), a tap-only copy has none
            // (`cpu-play-v1`: Beatucada's Easy, 34 holds, plays a little
            // above its Beginner, none, at the weakest levels).
            let (a, b) = (&song.charts[singles[k]], &song.charts[singles[k + 1]]);
            let holds = |i: usize| {
                Plan::automatic(&song, i)
                    .ok()
                    .and_then(|p| p.full().map(|f| f.counts().0 + f.counts().1))
                    .unwrap_or(0)
            };
            let tolerance = if a.meter == b.meter || holds(singles[k + 1]) > holds(singles[k]) {
                6.0
            } else {
                2.0
            };
            for l in 0..LEVELS {
                assert!(
                    pair[1][l] <= pair[0][l] + tolerance,
                    "{}: level {}: a harder chart ({:?} {}) {} above {} ({:?} {})",
                    song.title,
                    l + 1,
                    b.difficulty,
                    b.meter,
                    pair[1][l],
                    pair[0][l],
                    a.difficulty,
                    a.meter,
                );
            }
        }
        // The song's easiest and hardest: level 5 at home on the easiest,
        // level 1 far below level 10 on the hardest.
        let (easy, hard) = (&table[0], &table[table.len() - 1]);
        assert!(easy[4] > 95.0, "{}: {easy:?}", song.title);
        assert!(hard[9] - hard[0] > 40.0, "{}: {hard:?}", song.title);
    }
    assert!(charts >= 12, "{charts}");
}

/// The same level differs from chart to chart as the model says: the
/// demand is the chart's own, in real units. A row far over a level's
/// limit is mostly left out, a row well within it is pressed.
#[test]
fn an_absolute_level_is_lost_on_a_chart_above_its_head() {
    let rows = |gap: f64| -> Vec<Row> {
        (0..400)
            .map(|i| Row {
                seconds: 1.0 + i as f64 * gap,
                mask: 1 << (i % 4),
            })
            .collect()
    };
    let weakest = CPU_LEVELS_V1.levels[0];
    for (gap, pressed) in [(1.0, 0.99), (0.0625, 0.5)] {
        let r = rows(gap);
        let d = absolute::row_demands(&r, &ABSOLUTE_V1);
        let presses = absolute::script(&r, &d, weakest, 7, Skills::default()).unwrap();
        let share = presses.len() as f64 / r.len() as f64;
        if pressed > 0.9 {
            assert!(share >= pressed, "{gap}: {share}");
        } else {
            assert!(share < pressed, "{gap}: {share}");
        }
        validate_script(&presses).unwrap();
    }
}

/// Bounds: rows at one time after a warp, a wall of quads every
/// millisecond, the plan's most rows: no panic, a valid script, every
/// level.
#[test]
fn absolute_levels_stay_within_bounds_on_extreme_rows() {
    let mut rows: Vec<Row> = (0..4096)
        .map(|i| Row {
            seconds: 2.0 + f64::from(i) * 0.001,
            mask: 15,
        })
        .collect();
    rows.extend((0..64).map(|_| Row {
        seconds: 10.0,
        mask: 3,
    }));
    let d = absolute::row_demands(&rows, &ABSOLUTE_V1);
    assert!(d.iter().all(|d| d.speed.is_finite() && d.lane.is_finite()));
    for a in CPU_LEVELS_V1.levels {
        let presses = absolute::script(&rows, &d, a, 3, Skills::default()).unwrap();
        validate_script(&presses).unwrap();
    }
}

/// The presses of `rows` within 40 ms of a note of their lane, as a share
/// of the notes, over seeds 1 to 8, at level 5 of `cpu-levels-v1` with
/// `skills`.
fn absolute_hits(rows: &[Row], skills: Skills) -> f64 {
    let d = absolute::row_demands(rows, &ABSOLUTE_V1);
    let notes: usize = rows.iter().map(|r| r.mask.count_ones() as usize).sum();
    let mut hits = 0;
    for seed in 1..=8 {
        let presses = absolute::script(rows, &d, CPU_LEVELS_V1.levels[4], seed, skills).unwrap();
        hits += presses
            .iter()
            .filter(|p| {
                rows.iter().any(|r| {
                    r.mask & (1 << p.lane) != 0 && (p.press - r.seconds).abs() <= 0.040
                })
            })
            .count();
    }
    hits as f64 / (8 * notes) as f64
}

/// `cpu-levels-v1`: each of the seven pattern axes bends the limit its
/// pattern meets, so a CPU strong on an axis (2) does better than a
/// balanced one on that pattern and one weak on it (0.5) worse; the
/// presets and the practice roster's personalities follow from these.
#[test]
fn each_axis_bends_the_absolute_limit_its_pattern_meets() {
    let rows = |gaps: &[f64], masks: &[u8], n: usize| -> Vec<Row> {
        let mut t = 1.0;
        (0..n)
            .map(|i| {
                let r = Row {
                    seconds: t,
                    mask: masks[i % masks.len()],
                };
                t += gaps[i % gaps.len()];
                r
            })
            .collect()
    };
    // Level 5 keeps up with about 3.95 notes a second.
    let patterns: [(usize, Vec<Row>); 7] = [
        (0, rows(&[0.2], &[1, 2, 4, 8], 300)),
        (1, rows(&[0.3], &[3, 12, 6, 9], 300)),
        (2, rows(&[0.45], &[7, 14, 13, 11], 300)),
        (3, rows(&[0.3], &[1, 2, 4, 8], 600)),
        (4, rows(&[1.0 / 3.0], &[1], 300)),
        (5, rows(&[0.4], &[3], 300)),
        (6, rows(&[0.15, 0.3], &[1, 2, 4, 8], 300)),
    ];
    for (axis, pattern) in &patterns {
        let at = |v: f64| {
            let mut values = [1.0; 7];
            values[*axis] = v;
            absolute_hits(pattern, Skills::new(values).unwrap())
        };
        let (weak, plain, strong) = (at(0.5), at(1.0), at(2.0));
        assert!(
            weak < plain && plain <= strong && weak < strong,
            "{}: {weak} {plain} {strong}",
            SKILL_NAMES[*axis]
        );
    }
}

/// Opt-in, the cost of a plan and of a CPU's presses on every bundled
/// single chart: `cpu-levels-v0` against `cpu-levels-v1`, natively or in
/// wasm32 (`cargo test --release -p ddi-engine --lib planning_cost --
/// --ignored --nocapture`, and with `--target wasm32-wasip1` under a WASI
/// runner). The plan is the same for both (its rows, the v0 frequencies
/// and demands and the v1 demands); the presses differ.
#[test]
#[ignore]
fn planning_cost() {
    // Compiled in: a WASI runner reads no files.
    const SONGS: [&str; 4] = [
        include_str!("../../../../assets/songs/beatucada/Beatucada.ssc"),
        include_str!("../../../../assets/songs/into-my-dream/Into My Dream.ssc"),
        include_str!("../../../../assets/songs/some-things-must/Some Things Must (OutFox Edit).ssc"),
        include_str!("../../../../assets/songs/sweeteners/Sweeteners.ssc"),
    ];
    let mut charts = Vec::new();
    for text in SONGS {
        let song = ddi_library::scan::parse_simfile(text, "ssc").unwrap();
        for i in 0..song.charts.len() {
            if song.charts[i].layout == "dance-single" && Plan::automatic(&song, i).is_ok() {
                charts.push((song.clone(), i));
            }
        }
    }
    let rounds = 5;
    let time = |f: &mut dyn FnMut()| {
        let start = std::time::Instant::now();
        for _ in 0..rounds {
            f();
        }
        start.elapsed().as_secs_f64() * 1e6 / f64::from(rounds) / charts.len() as f64
    };
    let plans: Vec<Plan> = charts
        .iter()
        .map(|(s, i)| Plan::automatic(s, *i).unwrap())
        .collect();
    let rows: usize = plans.iter().map(|p| p.demands().len()).sum::<usize>() / plans.len();
    let plan = time(&mut || {
        for (s, i) in &charts {
            std::hint::black_box(Plan::automatic(s, *i).unwrap());
        }
    });
    let script = |strength: Strength| {
        time(&mut || {
            for p in &plans {
                std::hint::black_box(p.script_strength(strength, 7, Skills::default()).unwrap());
            }
        })
    };
    let v0_frozen = script(LevelScale::RelativeV0.strength(7).unwrap());
    let v0_weak = script(LevelScale::RelativeV0.strength(2).unwrap());
    let v1_strong = script(LevelScale::AbsoluteV1.strength(7).unwrap());
    let v1_weak = script(LevelScale::AbsoluteV1.strength(2).unwrap());
    // The chart itself (`cpu-play-v1`): its holds, rolls, lifts and mines.
    let full = |level: usize| {
        let strength = LevelScale::AbsoluteV1.strength(level).unwrap();
        let avoidance = MINE_RESPONSE_V0.avoidance(level);
        time(&mut || {
            for p in &plans {
                std::hint::black_box(
                    p.script_play(strength, 7, Skills::default(), avoidance, CpuPlay::FullV1)
                        .unwrap(),
                );
            }
        })
    };
    let (full_strong, full_weak) = (full(7), full(2));
    println!(
        "{} charts, {rows} rows on average: a plan {plan:.0} us; presses v0 level 8 {v0_frozen:.0} us, v0 level 3 {v0_weak:.0} us, v1 level 8 {v1_strong:.0} us, v1 level 3 {v1_weak:.0} us; the full chart (cpu-play-v1) level 8 {full_strong:.0} us, level 3 {full_weak:.0} us (a chart)",
        charts.len()
    );
}

/// Opt-in, a level watched (`DDI_WATCH_LEVEL`, 3 by default; `cargo test
/// --release -p ddi-engine --lib a_level_watched -- --ignored
/// --nocapture`): Beatucada's charts under DDI Normal, as a stage plays
/// them (out when its life empties), four seeds each: the percent, the
/// judgments (W0 apart, then the tiers to Miss), and when it went out.
#[test]
#[ignore]
fn a_level_watched() {
    let level: usize = std::env::var("DDI_WATCH_LEVEL")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(3);
    let song = bundled_songs()
        .into_iter()
        .find(|s| s.title == "Beatucada")
        .unwrap();
    for (i, chart) in song.charts.iter().enumerate() {
        if chart.layout != "dance-single" || chart.derivation.is_some() {
            continue;
        }
        for seed in 1..=4 {
            let mut rules = crate::rules::ddi::ruleset(&crate::rules::ddi::DdiOptions::default());
            rules.fail = crate::rules::FailPolicy::Immediate;
            let mut o = Opponent::on_scale(
                &song,
                i,
                rules,
                LevelScale::AbsoluteV1,
                level - 1,
                seed,
                Skills::default(),
            )
            .unwrap();
            o.start(0.0);
            o.finish();
            let t = o.player().tally();
            println!(
                "level {level} {:?} (meter {}) seed {seed}: {:.1}% W0 {} tiers {:?} out {:?} of {:.0} s",
                chart.difficulty,
                chart.meter,
                o.score().percent.unwrap_or(0.0) * 100.0,
                t.w0,
                t.taps,
                o.out_at().map(|s| (s * 10.0).round() / 10.0),
                o.player().song_end_time()
            );
        }
    }
}

/// The solo CPU on the absolute levels by default (round D, 2026-11-14):
/// automatic mode with no level plays level 5; a named strength is levels
/// 6 to 10 of the absolute scale; a named level is that level; a prepared
/// profile keeps its strength (chart-relative, as before).
#[test]
fn the_solo_cpu_plays_the_absolute_levels_by_default() {
    assert_eq!(SOLO_LEVEL, 4, "level 5 of 10");
    let names = ["relaxed", "easy", "normal", "strong", "expert"];
    for (i, name) in names.iter().enumerate() {
        assert_eq!(strength_level(strength(name).unwrap()), 5 + i, "{name}");
    }
    let plain = Selection::parse("1", "normal", "7").unwrap().on_absolute_scale(false);
    assert_eq!(plain.absolute, Some(SOLO_LEVEL));
    let named = Selection::parse("1", "expert", "7").unwrap().on_absolute_scale(true);
    assert_eq!(named.absolute, Some(9));
    let level = Selection::parse("1", "normal", "7")
        .unwrap()
        .with_level("3")
        .unwrap()
        .on_absolute_scale(false);
    assert_eq!(level.absolute, Some(2));
    let prepared = Selection::parse("cpu/custom.json", "normal", "7")
        .unwrap()
        .on_absolute_scale(true);
    assert_eq!(prepared.absolute, None);
}
