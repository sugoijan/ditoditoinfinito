//! Complete-song analytical CPU integration, with the desktop loader and fake audio.
//! DDI_CPU_PROFILE switches to a locally prepared frozen artifact for parity;
//! no third-party charts or weights are copied into the repository.
use ddi_desktop::{
    audio::Audio,
    play::{Ended, Feed, Play, cpu, load},
};
use ddi_platform::{HostTime, history::MemoryPlayStore};
use ddi_ui::state::{history::Plays, settings::Settings};
use std::{
    cell::RefCell,
    rc::Rc,
    sync::{
        Arc,
        atomic::{AtomicU64, Ordering},
    },
    time::Instant,
};

#[test]
fn complete_cpu_songs_preserve_inputs_and_do_not_record() {
    let request = if let Ok(path) = std::env::var("DDI_CPU_PROFILE") {
        cpu::read(&path, "normal", 7).unwrap()
    } else {
        cpu::arguments(
            &[
                "--play",
                "beatucada",
                "--chart",
                "3",
                "--cpu=1",
                "--cpu-seed",
                "7",
            ]
            .map(String::from),
        )
        .unwrap()
        .unwrap()
    };
    let source = request
        .cpu
        .as_ref()
        .unwrap()
        .source_file
        .clone()
        .unwrap_or_else(|| {
            std::fs::read_dir(
                std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
                    .join("../../assets/songs/beatucada"),
            )
            .unwrap()
            .flatten()
            .map(|e| e.path())
            .find(|p| p.extension().is_some_and(|e| e == "ssc"))
            .unwrap()
        });
    let loaded = Rc::new(load::load_cpu(&source, 48_000).unwrap());
    for (level, label) in ddi_engine::cpu::LABELS.into_iter().enumerate() {
        let mut request = request.clone();
        let model = request.cpu.as_mut().unwrap();
        model.level = level;
        if model.profile.is_none() {
            model.skills = ddi_engine::cpu::Skills::parse("jacks").unwrap();
            // Automatic mode plays the absolute levels (2026-11-14): a
            // named strength is levels 6 to 10.
            model.absolute = Some(ddi_engine::cpu::strength_level(level));
        }
        let automatic;
        let profile = if let Some(p) = &model.profile {
            p.as_ref()
        } else {
            automatic = ddi_engine::cpu::Plan::automatic(&loaded.song, request.chart).unwrap();
            &automatic
        };
        let script =
            ddi_engine::cpu::solo_script(profile, level, model.absolute, model.seed, model.skills)
                .unwrap();
        let chart = request.chart;
        let clock = Arc::new(AtomicU64::new(1_000_000_000));
        let c = clock.clone();
        let audio = Audio::headless(1., Some(Arc::new(move || c.load(Ordering::SeqCst))));
        let store = Rc::new(MemoryPlayStore::default());
        let plays = Plays::new(store.clone());
        let settings = Settings::default();
        let start = Instant::now();
        let mut play = Play::start(
            request,
            loaded.clone(),
            &settings,
            audio,
            Rc::new(RefCell::new(Feed::default())),
            None,
            60.,
        )
        .unwrap();
        let setup_ms = start.elapsed().as_secs_f64() * 1000.;
        let mut tick_us = vec![];
        let result = loop {
            let now = clock.fetch_add(16_666_667, Ordering::SeqCst) + 16_666_667;
            let start = Instant::now();
            let end = play.tick(HostTime(now as f64 / 1e9), Some(&plays));
            tick_us.push(start.elapsed().as_secs_f64() * 1e6);
            match end {
                Some(Ended::Finished { results, .. }) => break results,
                Some(_) => panic!("CPU did not finish"),
                None => {}
            }
            assert!(now < 1_000_000_000u64 * 86_500, "CPU never ended");
        };
        let log = play.session.player.log();
        assert_eq!(log.inputs.len(), 2 * script.len());
        let expected = script
            .iter()
            .map(|p| (p.lane, (p.press * 1e6).round() / 1e6))
            .collect::<Vec<_>>();
        let actual = log
            .inputs
            .iter()
            .filter(|e| e.pressed)
            .map(|e| (e.lane, e.time))
            .collect::<Vec<_>>();
        assert_eq!(actual, expected);
        assert_eq!(store.len(), 0);
        assert_eq!(result.late_edges, 0);
        assert!(!result.failed);
        let mut rules = settings.ruleset_for(&play.song().charts[chart]);
        rules.fail = ddi_engine::FailPolicy::Off;
        let direct = ddi_engine::history::replay_with(
            play.song(),
            chart,
            rules,
            play.session.player.options(),
            log,
        );
        assert_eq!(result.tally, direct.tally);
        assert_eq!(result.score, direct.score);
        assert_eq!(result.timeline, direct.timeline);
        tick_us.sort_by(f64::total_cmp);
        eprintln!(
            "CPU {label}: {} inputs, setup {setup_ms:.3} ms, session tick p95 {:.3} us, score {:?}",
            log.inputs.len(),
            tick_us[tick_us.len() * 95 / 100],
            result.score.percent
        );
    }
}

#[test]
fn cpu_flags_reject_ambiguous_and_missing_options() {
    for args in [
        vec!["--cpu"],
        vec!["--cpu-skills", "jacks"],
        vec!["--cpu=1", "--play", "song", "--cpu-skills", "1,2"],
        vec!["--cpu", "none", "--cpu-skills", "jacks"],
        vec!["--cpu-strength", "easy"],
        vec!["--cpu", "none", "--auto"],
        vec!["--cpu", "none", "--cpu-seed", "-1"],
        vec!["--cpu", "none", "--cpu-strength", "mystery"],
        vec!["--cpu=1"],
        vec!["--cpu=1", "--play", "song", "--auto"],
        vec!["--cpu=1", "--play", "song", "--chart", "-1"],
        vec!["--cpu=1", "--play", "song", "--cpu-level", "11"],
        vec!["--cpu=1", "--play", "song", "--cpu-level", "0"],
        vec!["--cpu=1", "--play", "song", "--cpu-level", "3", "--cpu-strength", "easy"],
        vec!["--cpu", "none", "--cpu-level", "3"],
        vec!["--cpu-level", "3"],
    ] {
        assert!(cpu::arguments(&args.into_iter().map(str::to_string).collect::<Vec<_>>()).is_err());
    }
    assert!(cpu::arguments(&[]).unwrap().is_none());
    let args = [
        "--play",
        "beatucada",
        "--chart",
        "3",
        "--cpu=1",
        "--cpu-seed",
        "7",
    ];
    let request = cpu::arguments(&args.map(String::from)).unwrap().unwrap();
    assert_eq!(request.song, "beatucada");
    assert_eq!(request.chart, 3);
    let cpu = request.cpu.unwrap();
    assert_eq!(cpu.seed, 7);
    // The absolute levels by default: level 5 where none is named.
    assert_eq!(cpu.absolute, Some(ddi_engine::cpu::SOLO_LEVEL));
    assert!(cpu.source_file.is_none() && cpu.profile.is_none());
    // A level of the opponents' absolute scale instead of a strength.
    let request = cpu::arguments(
        &["--play", "beatucada", "--cpu=1", "--cpu-level", "10"].map(String::from),
    )
    .unwrap()
    .unwrap();
    assert_eq!(request.cpu.unwrap().absolute, Some(9));
    // A named strength: levels 6 to 10 of the absolute scale.
    let request = cpu::arguments(
        &["--play", "beatucada", "--cpu=1", "--cpu-strength", "relaxed"].map(String::from),
    )
    .unwrap()
    .unwrap();
    assert_eq!(request.cpu.unwrap().absolute, Some(5));
}

#[test]
fn cpu_personality_flags_round_trip() {
    for input in ["jacks", "1.1,0.9,1,1.25,0.8,1,1"] {
        let request = cpu::arguments(
            &["--play", "beatucada", "--cpu=1", "--cpu-skills", input].map(String::from),
        )
        .unwrap()
        .unwrap();
        assert_eq!(
            request.cpu.unwrap().skills,
            ddi_engine::cpu::Skills::parse(input).unwrap()
        );
    }
}
