//! Plays imported from an export on desktop (`docs/plans/phase-10.md`,
//! step 1; `ddi_desktop::history::Rework::import`), headless: plays kept
//! in one plays folder exported, imported into another on the import's
//! thread (their raw data as it was, a play of a song not here kept,
//! lines that are not plays counted), then judged again to the summaries
//! they had; the same file again imports nothing.
//!
//! Exchange with the web, run by hand (ignored):
//! `DDI_WEB_EXPORT=<file.jsonl>` imports the web's export and judges its
//! plays of bundled songs again to the summaries the web kept;
//! `DDI_EXPORT_PLAYS_FROM=<plays dir> DDI_EXPORT_TO=<file.jsonl>` writes
//! a desktop plays folder's export for the web to import.

mod common;
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::time::{Duration, Instant};

use ddi_desktop::history::Rework;
use ddi_desktop::plays::FilePlayStore;
use ddi_engine::player::{PlayOptions, Player};
use ddi_engine::rules::presets;
use ddi_platform::history::{PlayEntry, PlayStore, play_id};
use ddi_ui::state::history::{
    ChartKey, Ending, ImportReport, JUDGE_VERSION, LoggedInput, PlayLog, PlayRecord, PlaySetup,
    Plays, RECORD_VERSION, replay,
};

fn scratch(name: &str) -> PathBuf {
    let dir = PathBuf::from(env!("CARGO_TARGET_TMPDIR"))
        .join(format!("import-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    dir
}

/// The plays in `dir`, read.
fn plays_in(dir: &Path) -> (FilePlayStore, Rc<Plays>) {
    let store = FilePlayStore::new(dir.to_path_buf());
    let plays = Plays::new(Rc::new(store.clone()));
    plays.load(|r| r.unwrap());
    (store, plays)
}

/// `plays`' export, as Options writes it.
fn export(plays: &Plays) -> String {
    let out = Rc::new(std::cell::RefCell::new(String::new()));
    let o = out.clone();
    plays.export(
        50,
        move |l| o.borrow_mut().push_str(&l),
        |r| {
            r.unwrap();
        },
    );
    out.take()
}

/// `path` imported into `plays` (over `rework`'s store), polled to its
/// end, then every play judged again that can be.
fn import(rework: &mut Rework, plays: &Plays, path: PathBuf) -> ImportReport {
    assert!(rework.import(path, plays, None));
    // One at a time.
    assert!(!rework.import(PathBuf::from("/nowhere"), plays, None));
    let started = Instant::now();
    let report = loop {
        assert!(started.elapsed() < Duration::from_secs(60), "never done");
        rework.poll(plays, false, false, true, None);
        if let Some(r) = rework.take_imported() {
            break r;
        }
        std::thread::sleep(Duration::from_millis(2));
    };
    assert_eq!(rework.importing(), None);
    report
}

/// Polls `rework` until no play of a song here is stale.
fn judge_again(rework: &mut Rework, plays: &Plays, left: usize) {
    let started = Instant::now();
    while plays.history().stale_count() > left {
        assert!(
            started.elapsed() < Duration::from_secs(60),
            "never worked out"
        );
        rework.poll(plays, false, false, true, None);
        std::thread::sleep(Duration::from_millis(2));
    }
}

/// Plays of Beatucada's charts kept in `plays`: `(chart, how late,
/// song id)`, pressed by the autoplayer's script (the player's own: they
/// count); a song id other than `beatucada` gets a chart no song has.
fn keep_plays(plays: &Rc<Plays>, of: &[(usize, f64, &str)]) {
    let song = ddi_desktop::play::load::read_chart("beatucada").unwrap();
    let rules = presets::itg().key();
    for (i, &(chart, bias, id)) in of.iter().enumerate() {
        let player = Player::new(
            &song,
            chart,
            rules.build(&song.charts[chart]),
            PlayOptions::default(),
        );
        let mut inputs = Vec::new();
        for p in ddi_engine::autoplay::script(player.judge().notes(), bias) {
            for (time, pressed) in [(p.press, true), (p.release, false)] {
                inputs.push(LoggedInput {
                    time,
                    lane: p.lane,
                    pressed,
                });
            }
        }
        inputs.sort_by(|a, b| a.time.total_cmp(&b.time));
        let log = PlayLog {
            start: 0.0,
            inputs,
            ..Default::default()
        };
        let results = replay(&song, chart, &rules, &PlayOptions::default(), &log);
        let digest = if id == "beatucada" {
            song.chart_digest(chart)
        } else {
            // Another chart's notes: not found under any song here.
            ddi_chart::ChartDigest(0x5eed)
        };
        let record = PlayRecord {
            version: RECORD_VERSION,
            chart: ChartKey {
                song: id.into(),
                digest,
                index: chart as u32,
            },
            ruleset: rules.clone(),
            options: PlayOptions::default(),
            song_offset: 0.0,
            setup: PlaySetup::default(),
            settings: serde_json::json!({ "speed": 2.5, "kept_as_stored": true }),
            ended: Ending::Finished,
            autoplay: None,
            time: 1_760_000_000_000.0 + i as f64,
            app_version: "0.1.0".into(),
            judge_version: JUDGE_VERSION,
        };
        let id = play_id(record.time, 0xab + i as u32);
        plays
            .record(id, &record, &log, &results, |r| r.unwrap())
            .unwrap();
    }
}

#[test]
fn an_export_is_imported_on_its_own_thread_and_judged_again() {
    common::indexes();
    let (from_dir, to_dir) = (scratch("from"), scratch("to"));
    let (store, plays) = plays_in(&from_dir);
    // Plays of its first two charts, early and late, and one of a song
    // this library does not have.
    keep_plays(
        &plays,
        &[
            (0, 0.02, "beatucada"),
            (1, -0.03, "beatucada"),
            (0, 0.0, "a-pack/not-here"),
        ],
    );
    drop(store);
    let file = from_dir.with_extension("jsonl");
    let mut text = export(&plays);
    // A line that is not a play among them.
    text.push_str("{\"id\":\"../escape\"}\n");
    std::fs::write(&file, &text).unwrap();

    let (to_store, to) = plays_in(&to_dir);
    let mut rework = Rework::new(to_store.clone());
    let report = import(&mut rework, &to, file.clone());
    assert_eq!(
        (report.imported, report.duplicates, report.invalid),
        (3, 0, 1),
        "{report:?}"
    );
    assert_eq!(report.problems[0].0, 4);
    assert_eq!(to.history().len(), 3);
    // Every play stale until judged again; then as it was, but the one
    // of a song not here, kept stale.
    assert_eq!(to.history().stale_count(), 3);
    judge_again(&mut rework, &to, 1);
    for (id, s) in plays.history().iter() {
        let got = to.history().get(id).cloned().unwrap();
        if s.chart.song == "a-pack/not-here" {
            assert!(got.is_stale());
            assert_eq!(got.chart, s.chart);
        } else {
            assert_eq!(&got, s, "{id}");
        }
    }
    // The raw play as it was, in the new folder's files.
    let ids: Vec<String> = plays.history().ids();
    let (a, b) = (read(&from_dir, &ids), read(&to_dir, &ids));
    for (x, y) in a.iter().zip(&b) {
        assert_eq!((&x.id, &x.log, &x.timing), (&y.id, &y.log, &y.timing));
        let (rx, ry): (serde_json::Value, serde_json::Value) = (
            serde_json::from_str(&x.record).unwrap(),
            serde_json::from_str(&y.record).unwrap(),
        );
        assert_eq!(rx, ry);
        assert_eq!(ry["settings"]["kept_as_stored"], true);
    }
    // A restart reads them; the same file again imports nothing.
    let (_, again) = plays_in(&to_dir);
    assert_eq!(again.history().len(), 3);
    let report = import(&mut rework, &to, file.clone());
    assert_eq!((report.imported, report.duplicates), (0, 3));
    // A file that is not there says so.
    let report = import(&mut rework, &to, to_dir.join("missing.jsonl"));
    assert!(report.error.is_some(), "{report:?}");
    let _ = std::fs::remove_dir_all(&from_dir);
    let _ = std::fs::remove_dir_all(&to_dir);
    let _ = std::fs::remove_file(&file);
}

fn read(dir: &Path, ids: &[String]) -> Vec<PlayEntry> {
    let store = FilePlayStore::new(dir.to_path_buf());
    let out = Rc::new(std::cell::RefCell::new(Vec::new()));
    let o = out.clone();
    store.records(
        ids.to_vec(),
        Box::new(move |r| *o.borrow_mut() = r.unwrap()),
    );
    out.take()
}

/// The web's export (`DDI_WEB_EXPORT`) imported here: its plays of
/// bundled songs judged again by this build to what the web kept.
#[test]
#[ignore = "needs DDI_WEB_EXPORT, an export from the web build"]
fn a_web_export_imports_and_judges_the_same() {
    common::indexes();
    let Ok(file) = std::env::var("DDI_WEB_EXPORT") else {
        eprintln!("DDI_WEB_EXPORT not set");
        return;
    };
    let text = std::fs::read_to_string(&file).unwrap();
    let web: Vec<PlayEntry> = text
        .lines()
        .map(|l| PlayEntry::from_json(l).unwrap())
        .collect();
    assert!(!web.is_empty());
    let dir = scratch("web");
    let (store, plays) = plays_in(&dir);
    let mut rework = Rework::new(store);
    let report = import(&mut rework, &plays, PathBuf::from(&file));
    assert_eq!(report.imported, web.len(), "{report:?}");
    // Those the web could not judge (their song not there) stay so.
    let stale = web
        .iter()
        .filter(|e| {
            serde_json::from_str::<ddi_ui::state::history::PlaySummary>(
                e.summary.as_deref().unwrap_or("null"),
            )
            .map_or(true, |s| s.is_stale())
        })
        .count();
    judge_again(&mut rework, &plays, stale);
    for e in &web {
        let kept: ddi_ui::state::history::PlaySummary =
            serde_json::from_str(e.summary.as_deref().unwrap()).unwrap();
        let got = plays.history().get(&e.id).cloned().unwrap();
        // The mean and spread to the nanosecond: wasm's and native sums
        // differ in their last bits.
        let near = |w: Option<ddi_ui::state::history::Worked>| {
            w.map(|mut w| {
                w.mean = (w.mean * 1e9).round();
                w.stddev = (w.stddev * 1e9).round();
                w
            })
        };
        assert_eq!(near(got.worked.clone()), near(kept.worked), "{}", e.id);
        eprintln!(
            "{} {} {:?} {}",
            e.id,
            got.chart.song,
            got.worked.as_ref().map(|w| w.score),
            got.worked.as_ref().map_or("", |w| w.grade.as_str())
        );
    }
    let _ = std::fs::remove_dir_all(&dir);
}

/// A desktop plays folder (`DDI_EXPORT_PLAYS_FROM`; without it, plays
/// of Beatucada kept here) exported to `DDI_EXPORT_TO`, for the web to
/// import.
#[test]
#[ignore = "needs DDI_EXPORT_TO"]
fn a_desktop_plays_folder_exports_for_the_web() {
    common::indexes();
    let Ok(to) = std::env::var("DDI_EXPORT_TO") else {
        eprintln!("DDI_EXPORT_TO not set");
        return;
    };
    let from = std::env::var("DDI_EXPORT_PLAYS_FROM").map_or_else(
        |_| {
            let dir = scratch("for-web");
            let (_, plays) = plays_in(&dir);
            keep_plays(
                &plays,
                &[
                    (0, 0.02, "beatucada"),
                    (1, -0.03, "beatucada"),
                    (1, 0.05, "beatucada"),
                    (0, 0.0, "a-pack/not-here"),
                ],
            );
            dir
        },
        PathBuf::from,
    );
    let (_, plays) = plays_in(&from);
    assert!(!plays.history().is_empty());
    std::fs::write(&to, export(&plays)).unwrap();
    eprintln!("{} plays exported to {to}", plays.history().len());
}
