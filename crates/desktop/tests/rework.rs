//! Plays judged by an older judge worked out again on desktop
//! (`ddi_desktop::history::Rework`; `docs/plans/play-history.md`,
//! Versions and re-working), headless: kept plays of a bundled song in a
//! plays folder with stale summaries, one under a song id that is gone
//! (its chart found by content); nothing during a play; then each judged
//! again on the history's thread to today's summary, written back to its
//! file, its best no longer pending.

mod common;
use std::rc::Rc;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::{Duration, Instant};

use ddi_desktop::history::Rework;
use ddi_desktop::plays::FilePlayStore;
use ddi_engine::player::{PlayOptions, Player};
use ddi_engine::rules::presets;
use ddi_platform::history::{PlayStore, play_id};
use ddi_ui::state::history::{
    ChartKey, Ending, JUDGE_VERSION, LoggedInput, PlayLog, PlayRecord, PlaySetup, PlaySummary,
    Plays, RECORD_VERSION, new_entry, replay,
};

#[test]
fn stale_plays_are_judged_again_off_the_event_loop() {
    common::indexes();
    let dir = std::path::PathBuf::from(env!("CARGO_TARGET_TMPDIR"))
        .join(format!("rework-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    let song = ddi_desktop::play::load::read_chart("beatucada").unwrap();
    let rules = presets::itg().key();
    let digest = song.chart_digest(0);

    // Two plays of its first chart, 20 ms late, as kept by an older
    // judge: one of the bundled song, one under an id no longer listed.
    let player = Player::new(
        &song,
        0,
        rules.build(&song.charts[0]),
        PlayOptions::default(),
    );
    let mut inputs = Vec::new();
    for p in ddi_engine::autoplay::script(player.judge().notes(), 0.02) {
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
    let results = replay(&song, 0, &rules, &PlayOptions::default(), &log);
    let store = FilePlayStore::new(dir.clone());
    let mut fresh = None;
    for (i, id) in ["beatucada", "a-pack-read-from-elsewhere"]
        .iter()
        .enumerate()
    {
        let record = PlayRecord {
            version: RECORD_VERSION,
            chart: ChartKey {
                song: id.to_string(),
                digest,
                index: 0,
            },
            ruleset: rules.clone(),
            options: PlayOptions::default(),
            song_offset: 0.0,
            setup: PlaySetup::default(),
            settings: serde_json::Value::Null,
            ended: Ending::Finished,
            autoplay: None,
            time: 1_000.0 + i as f64,
            app_version: "0.1.0".into(),
            judge_version: JUDGE_VERSION,
        };
        let (mut entry, mut summary) =
            new_entry(play_id(record.time, 7), &record, &log, &results).unwrap();
        fresh = summary.worked.clone();
        let w = summary.worked.as_mut().unwrap();
        w.judge_version = JUDGE_VERSION - 1;
        w.score.percent = Some(0.01);
        entry.summary = Some(serde_json::to_string(&summary).unwrap());
        store.append(entry, Box::new(|r| r.unwrap()));
    }

    let plays = Plays::new(Rc::new(store.clone()));
    plays.load(|r| r.unwrap());
    assert_eq!(plays.history().stale_count(), 2);
    let best = plays.history().best(digest, &rules, false).unwrap();
    assert!(best.pending);
    assert_eq!(best.score.percent, Some(0.01));
    let woken = Arc::new(AtomicUsize::new(0));
    let w = woken.clone();
    let wake: Arc<dyn Fn() + Send + Sync> = Arc::new(move || {
        w.fetch_add(1, Ordering::SeqCst);
    });
    let mut rework = Rework::new(store);

    // During a play: nothing, and nothing to wake for.
    rework.poll(&plays, true, true, true, Some(&wake));
    assert_eq!(rework.due(), None);
    std::thread::sleep(Duration::from_millis(50));
    rework.poll(&plays, true, true, true, Some(&wake));
    assert_eq!(plays.history().stale_count(), 2);

    // The play over: worked out, the loop woken for each answer.
    let started = Instant::now();
    while plays.history().stale_count() > 0 {
        assert!(
            started.elapsed() < Duration::from_secs(30),
            "never worked out"
        );
        rework.poll(&plays, false, false, true, Some(&wake));
        let wait = rework
            .due()
            .map_or(Duration::from_millis(5), |t| {
                t.saturating_duration_since(Instant::now())
            })
            .min(Duration::from_millis(5));
        std::thread::sleep(wait);
    }
    assert!(woken.load(Ordering::SeqCst) >= 2);
    let best = plays.history().best(digest, &rules, false).unwrap();
    assert!(!best.pending);
    assert_eq!(best.plays, 2);
    // The summaries written (the step after is scheduled once they
    // are): a new start reads today's judge's.
    let deadline = Instant::now() + Duration::from_secs(10);
    while rework.due().is_none() {
        assert!(Instant::now() < deadline, "never written");
        rework.poll(&plays, false, false, true, Some(&wake));
        std::thread::sleep(Duration::from_millis(5));
    }
    let again = Plays::new(Rc::new(FilePlayStore::new(dir.clone())));
    again.load(|r| r.unwrap());
    assert_eq!(again.history().stale_count(), 0);
    for (_, s) in again.history().iter() {
        assert_eq!(s.worked, fresh);
    }
    let file = dir.join(format!("{}.summary.json", play_id(1_000.0, 7)));
    let s: PlaySummary = serde_json::from_str(&std::fs::read_to_string(file).unwrap()).unwrap();
    assert_eq!(s.worked.unwrap().judge_version, JUDGE_VERSION);
    let _ = std::fs::remove_dir_all(&dir);
}
