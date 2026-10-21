//! Phase 10's rulesets on desktop (`docs/plans/rulesets.md`), with no
//! device (`Audio::headless` on a fake clock): a bundled chart autoplayed
//! under DDR EXTREME and under DDR A's FLARE and LIFE4 gauges as the
//! settings choose them, the results' FLARE rank, points and lamp, and
//! the play kept with its gauge.

mod common;
use std::cell::RefCell;
use std::rc::Rc;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

use ddi_desktop::audio::Audio;
use ddi_desktop::play::{Ended, Feed, Play, Request, load};
use ddi_engine::Judgement;
use ddi_engine::rules::StepManiaOptions;
use ddi_engine::rules::ddr::DdrGauge;
use ddi_platform::HostTime;
use ddi_platform::history::MemoryPlayStore;
use ddi_ui::state::history::{Lamp, Plays};
use ddi_ui::state::settings::Settings;

/// Autoplays chart `chart` of `song` to its end under `settings`; the
/// results and the history it was kept in.
fn play(
    song: &str,
    chart: usize,
    settings: &Settings,
) -> (
    ddi_engine::player::Results,
    Rc<Plays>,
    ddi_engine::rules::Ruleset,
) {
    let loaded = Rc::new(load::load(song, 48_000).unwrap());
    let clock = Arc::new(AtomicU64::new(1_000_000_000));
    let c = clock.clone();
    let audio = Audio::headless(1.0, Some(Arc::new(move || c.load(Ordering::SeqCst))));
    let store = Rc::new(MemoryPlayStore::default());
    let plays = Plays::new(store.clone());
    let mut p = Play::start(
        Request::song(song, chart, true),
        loaded,
        settings,
        audio,
        Rc::new(RefCell::new(Feed::default())),
        None,
        60.0,
    )
    .unwrap();
    let step = 1_000_000_000 / 60;
    let results = loop {
        let now = clock.fetch_add(step, Ordering::SeqCst) + step;
        match p.tick(HostTime(now as f64 / 1e9), Some(&plays)) {
            Some(Ended::Finished { results, .. }) => break *results,
            Some(_) => panic!("the play did not finish"),
            None => {}
        }
        assert!(now < 1_000_000_000 * 1000, "never ends");
    };
    let rules = p.session.rules.fresh();
    (results, plays, rules)
}

fn ddr_a(gauge: DdrGauge) -> Settings {
    Settings {
        ruleset: "ddr-a".into(),
        stepmania: StepManiaOptions {
            gauge,
            ..StepManiaOptions::default()
        },
        ..Settings::default()
    }
}

#[test]
fn ddr_rulesets_from_the_settings_to_the_results() {
    common::indexes();
    // Beatucada's Beginner (a 1: 145 points before the FLARE rank).
    let (r, plays, rules) = play("beatucada", 1, &ddr_a(DdrGauge::Flare(10)));
    assert!(!r.failed);
    assert_eq!(r.tally.count(Judgement::W1), r.tally.total_taps());
    let ddr = r.ddr.expect("DDR A keeps its gauge");
    assert_eq!(
        (ddr.gauge, ddr.flare, ddr.points),
        (DdrGauge::Flare(10), Some(10), Some(232))
    );
    let data = ddi_ui::screens::play::results_data(&ddi_ui::screens::play::PlayInfo {
        results: &r,
        ruleset: &rules,
        layout: &ddi_chart::Layout::builtin("dance-single").unwrap(),
        song: Default::default(),
        chart: String::new(),
        mods: Vec::new(),
        device_changed: false,
        video_notes: Vec::new(),
        offset: None,
        panels: String::new(),
        autoplay: false,
    });
    assert!(
        data.stats
            .contains(&("Flare skill".into(), "FLARE EX · 232 points".into())),
        "{:?}",
        data.stats
    );
    assert_eq!(data.mods, "FLARE EX");
    // Kept with its gauge (an autoplay: kept, never a best).
    let history = plays.history();
    let (_, summary) = history.iter().next().expect("kept");
    assert_eq!(summary.ruleset.stepmania.gauge, DdrGauge::Flare(10));
    assert_eq!(
        summary.worked.as_ref().unwrap().lamp,
        Lamp::MarvelousFullCombo
    );
    drop(history);

    // LIFE4: kept as the play's gauge, the rank 0.
    let (r, _, _) = play("beatucada", 1, &ddr_a(DdrGauge::Life4));
    assert_eq!(r.ddr.unwrap().flare, Some(0));
    assert!(r.ddr.unwrap().life4());

    // DDR EXTREME: all Perfect, MAX2's score for a 1-foot song, AAA.
    let extreme = Settings {
        ruleset: "ddr-extreme".into(),
        ..Settings::default()
    };
    let (r, _, _) = play("beatucada", 1, &extreme);
    assert_eq!(r.tally.count(Judgement::W2), r.tally.total_taps());
    assert_eq!(r.score.money, Some(10_000_000));
    assert_eq!(r.grade, "AAA");
    assert_eq!(r.ddr, None);
}
