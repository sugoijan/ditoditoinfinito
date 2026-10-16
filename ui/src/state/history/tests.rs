use std::cell::RefCell;
use std::rc::Rc;

use ddi_chart::Song;
use ddi_chart::formats::sm::parse_simfile;
use ddi_engine::player::{PlayOptions, Player, Results};
use ddi_engine::rules::presets;
use ddi_platform::history::MemoryPlayStore;

use super::*;

const SM: &str = "#TITLE:History;#OFFSET:0;#BPMS:0=120;
#NOTES:
     dance-single:
     :
     Hard:
     5:
     0,0,0,0,0:
1000
0100
0010
0001
,
1000
0100
0010
0001
;
#NOTES:
     dance-single:
     :
     Easy:
     2:
     0,0,0,0,0:
1000
0000
0001
0000
,
0100
0000
0010
0000
;
";

fn song() -> Song {
    parse_simfile(SM, "sm").unwrap()
}

/// A play of chart `index` pressed `bias` seconds late (none: no presses):
/// its log and what it comes to.
fn play(song: &Song, index: usize, rules: &RulesetKey, bias: Option<f64>) -> (PlayLog, Results) {
    let ruleset = rules.build(&song.charts[index]);
    let player = Player::new(song, index, ruleset, PlayOptions::default());
    let mut inputs = Vec::new();
    if let Some(bias) = bias {
        for p in ddi_engine::autoplay::script(player.judge().notes(), bias) {
            inputs.push(LoggedInput {
                time: p.press,
                lane: p.lane,
                pressed: true,
            });
            inputs.push(LoggedInput {
                time: p.release,
                lane: p.lane,
                pressed: false,
            });
        }
    }
    inputs.sort_by(|a, b| a.time.total_cmp(&b.time));
    let log = PlayLog { start: 0.0, inputs };
    let results = replay(song, index, rules, &PlayOptions::default(), &log);
    (log, results)
}

fn record(song: &Song, index: usize, rules: &RulesetKey, time: f64, auto: bool) -> PlayRecord {
    PlayRecord {
        version: RECORD_VERSION,
        chart: ChartKey {
            song: "s1".into(),
            digest: song.chart_digest(index),
            index: index as u32,
        },
        ruleset: rules.clone(),
        options: PlayOptions::default(),
        song_offset: 0.0,
        setup: PlaySetup {
            inputs: vec![InputDevice::Keyboard],
            ..PlaySetup::default()
        },
        settings: serde_json::json!({ "speed": 2.0, "a_field_added_later": [1, 2] }),
        ended: Ending::Finished,
        autoplay: auto.then_some(Autoplay {
            pad: false,
            bias: 0.0,
        }),
        time,
        app_version: "0.1.0".into(),
    }
}

fn itg() -> RulesetKey {
    presets::itg().key()
}

fn answered<T: 'static>() -> (Rc<RefCell<Option<T>>>, impl FnOnce(T) + 'static) {
    let cell = Rc::new(RefCell::new(None));
    let c = cell.clone();
    (cell, move |v| *c.borrow_mut() = Some(v))
}

#[test]
fn a_play_is_kept_whole_and_read_back() {
    let song = song();
    let (log, results) = play(&song, 0, &itg(), Some(0.0));
    assert_eq!(results.tally.taps[0], 8, "{:?}", results.tally);
    let rec = record(&song, 0, &itg(), 1_000.0, false);
    let (entry, summary) = new_entry(play_id(1_000.0, 7), &rec, &log, &results).unwrap();
    let worked = summary.worked.clone().unwrap();
    assert_eq!(worked.judge_version, JUDGE_VERSION);
    assert!(worked.lamp >= Lamp::FullCombo);
    assert_eq!(worked.late_edges, 0);
    assert!(!summary.autoplay);
    // Everything comes back: the record (with the settings' unknown
    // fields), the log, the timing as played.
    let back: PlayRecord = serde_json::from_str(&entry.record).unwrap();
    assert_eq!(back, rec);
    assert_eq!(PlayLog::decode(&entry.log).unwrap(), log);
    let timing = PlayTiming::decode(&entry.timing).unwrap();
    assert_eq!(timing, PlayTiming::from_results(&results));
    assert_eq!(timing.timeline.len(), results.timeline.len());
    // The export line carries it all.
    let line = export_lines(std::slice::from_ref(&entry));
    assert_eq!(line.lines().count(), 1);
    let parsed = PlayEntry::from_json(line.trim_end()).unwrap();
    assert_eq!(parsed.log, entry.log);
    assert_eq!(parsed.timing, entry.timing);
    let v: serde_json::Value = serde_json::from_str(line.trim_end()).unwrap();
    assert_eq!(v["record"]["settings"]["a_field_added_later"][1], 2);
    assert_eq!(v["summary"]["worked"]["tally"]["taps"][0], 8);
}

#[test]
fn a_play_judged_again_comes_to_the_same() {
    let song = song();
    for bias in [0.0, 0.03, -0.06] {
        let (log, results) = play(&song, 0, &itg(), Some(bias));
        let rec = record(&song, 0, &itg(), 1.0, false);
        let (entry, summary) = new_entry("p".into(), &rec, &log, &results).unwrap();
        assert_eq!(
            Some(rework(&entry, &song).unwrap()),
            summary.worked,
            "bias {bias}"
        );
    }
}

#[test]
fn a_chart_is_found_by_its_content() {
    let song = song();
    let key = |i: usize, index: u32| ChartKey {
        song: "s1".into(),
        digest: song.chart_digest(i),
        index,
    };
    assert_ne!(song.chart_digest(0), song.chart_digest(1));
    assert_eq!(find_chart(&song, &key(1, 1)), Some(1));
    // The chart moved in its song: found where it is now.
    assert_eq!(find_chart(&song, &key(1, 0)), Some(1));
    assert_eq!(find_chart(&song, &key(0, 9)), Some(0));
    // A chart whose notes changed is another chart.
    let mut changed = song.clone();
    changed.charts[1].notes.pop();
    assert_eq!(find_chart(&changed, &key(1, 1)), None);
    let (log, results) = play(&song, 1, &itg(), Some(0.0));
    let (entry, _) = new_entry(
        "p".into(),
        &record(&song, 1, &itg(), 1.0, false),
        &log,
        &results,
    )
    .unwrap();
    assert!(rework(&entry, &changed).is_err());
}

fn summary(rules: &RulesetKey, time: f64, score: f64, lamp: Lamp) -> PlaySummary {
    let song = song();
    let (_, results) = play(&song, 0, rules, Some(0.0));
    let mut worked = Worked::from_results(&results);
    worked.score.percent = Some(score);
    worked.score.money = None;
    worked.lamp = lamp;
    worked.failed = lamp == Lamp::Failed;
    PlaySummary {
        version: RECORD_VERSION,
        chart: ChartKey {
            song: "s1".into(),
            digest: song.chart_digest(0),
            index: 0,
        },
        ruleset: rules.clone(),
        time,
        autoplay: false,
        ended: if worked.failed {
            Ending::Failed
        } else {
            Ending::Finished
        },
        worked: Some(worked),
    }
}

#[test]
fn bests_take_the_best_score_and_lamp_and_leave_autoplay_out() {
    let mut h = History::default();
    h.add("1".into(), summary(&itg(), 1.0, 0.90, Lamp::FullCombo));
    h.add("2".into(), summary(&itg(), 2.0, 0.95, Lamp::Clear));
    h.add("3".into(), summary(&itg(), 3.0, 0.40, Lamp::Failed));
    let mut auto = summary(&itg(), 4.0, 1.0, Lamp::MarvelousFullCombo);
    auto.autoplay = true;
    h.add("4".into(), auto);
    let mut assist = summary(&itg(), 5.0, 0.99, Lamp::PerfectFullCombo);
    assist.worked.as_mut().unwrap().assist = true;
    h.add("5".into(), assist);
    // Another ruleset is another best.
    let sm5 = presets::sm5().key();
    h.add("6".into(), summary(&sm5, 6.0, 0.5, Lamp::Clear));

    let digest = h.get("1").unwrap().chart.digest;
    let b = h.best(digest, &itg(), false).unwrap();
    assert_eq!(b.score.percent, Some(0.95));
    assert_eq!(b.score_play, "2");
    assert_eq!(b.lamp, Lamp::FullCombo);
    assert_eq!(b.lamp_play, "1");
    // The failed play counts (StepMania keeps it); autoplay never does.
    assert_eq!(b.plays, 3);
    assert_eq!(b.last_played, 3.0);
    assert!(!b.pending);
    let a = h.best(digest, &itg(), true).unwrap();
    assert_eq!((a.plays, a.lamp), (1, Lamp::PerfectFullCombo));
    assert_eq!(h.best(digest, &sm5, false).unwrap().plays, 1);
    assert_eq!(h.bests_of_charts(&[digest]).len(), 3);
    assert!(h.bests_of_charts(&[song().chart_digest(1)]).is_empty());
    assert_eq!(h.last().unwrap().0, "6");
    // The same chart played under another song id (a pack imported
    // again from another folder): the same best.
    let mut moved = summary(&itg(), 7.0, 0.97, Lamp::Clear);
    moved.chart.song = "u-elsewhere".into();
    h.add("7".into(), moved);
    let b = h.best(digest, &itg(), false).unwrap();
    assert_eq!((b.plays, b.score_play.as_str()), (4, "7"));
}

#[test]
fn stale_plays_are_marked_and_worked_out_again() {
    let song = Rc::new(song());
    let store = Rc::new(MemoryPlayStore::default());
    let plays = Plays::new(store.clone());
    let (loaded, done) = answered();
    plays.load(done);
    assert!(matches!(loaded.borrow().as_ref(), Some(Ok(()))));
    for (i, bias) in [0.0, 0.05].into_iter().enumerate() {
        let (log, results) = play(&song, 0, &itg(), Some(bias));
        let rec = record(&song, 0, &itg(), i as f64, false);
        plays
            .record(play_id(i as f64, 0), &rec, &log, &results, |r| r.unwrap())
            .unwrap();
    }
    let fresh = plays
        .history()
        .get(&play_id(0.0, 0))
        .unwrap()
        .worked
        .clone();
    // An older judge's summaries, as a store would have them.
    let old = |id: &str| {
        let mut s = plays.history().get(id).unwrap().clone();
        let w = s.worked.as_mut().unwrap();
        w.judge_version = 0;
        w.score.percent = Some(0.01);
        s
    };
    let olds: Vec<(String, String)> = [play_id(0.0, 0), play_id(1.0, 0)]
        .iter()
        .map(|id| (id.clone(), serde_json::to_string(&old(id)).unwrap()))
        .collect();
    store.replace_summaries(olds, Box::new(|r| r.unwrap()));
    let plays = Plays::new(store.clone());
    plays.load(|r| r.unwrap());
    assert_eq!(plays.history().len(), 2);
    assert_eq!(plays.stale_songs(), vec!["s1".to_string()]);
    let digest = plays.history().get(&play_id(0.0, 0)).unwrap().chart.digest;
    let b = plays.history().best(digest, &itg(), false).unwrap();
    assert!(b.pending);
    assert_eq!(b.score.percent, Some(0.01));
    // A few at a time.
    let (n, done) = answered();
    plays.rework_song("s1", song.clone(), 1, done);
    assert_eq!(n.borrow_mut().take().unwrap().unwrap(), 1);
    let (n, done) = answered();
    plays.rework_song("s1", song.clone(), 5, done);
    assert_eq!(n.borrow_mut().take().unwrap().unwrap(), 1);
    assert!(plays.stale_songs().is_empty());
    assert_eq!(plays.history().get(&play_id(0.0, 0)).unwrap().worked, fresh);
    assert!(!plays.history().best(digest, &itg(), false).unwrap().pending);
    // Written to the store: a new start reads the new summaries.
    let again = Plays::new(store.clone());
    again.load(|r| r.unwrap());
    assert_eq!(again.history().get(&play_id(0.0, 0)).unwrap().worked, fresh);
    // The raw record was not touched.
    let (entries, done) = answered();
    store.records(vec![play_id(0.0, 0)], Box::new(done));
    let e = entries.borrow_mut().take().unwrap().unwrap().remove(0);
    assert_eq!(
        serde_json::from_str::<PlayRecord>(&e.record).unwrap().time,
        0.0
    );
}

#[test]
fn plays_of_a_changed_chart_stay_unmatched() {
    let song = song();
    let store = Rc::new(MemoryPlayStore::default());
    let plays = Plays::new(store.clone());
    let (log, results) = play(&song, 0, &itg(), Some(0.0));
    plays
        .record(
            "a".into(),
            &record(&song, 0, &itg(), 1.0, false),
            &log,
            &results,
            |_| {},
        )
        .unwrap();
    let mut s = plays.history().get("a").unwrap().clone();
    s.worked.as_mut().unwrap().judge_version = 0;
    store.replace_summaries(
        vec![("a".into(), serde_json::to_string(&s).unwrap())],
        Box::new(|_| {}),
    );
    let plays = Plays::new(store);
    plays.load(|r| r.unwrap());
    let mut changed = song.clone();
    changed.charts[0].notes.pop();
    let (n, done) = answered();
    plays.rework_song("s1", Rc::new(changed), 5, done);
    assert_eq!(n.borrow_mut().take().unwrap().unwrap(), 0);
    // Kept, still stale, not tried again this session.
    assert!(plays.history().get("a").unwrap().is_stale());
    assert!(plays.stale_songs().is_empty());
}

#[test]
fn unreadable_summaries_are_kept_apart() {
    let good = serde_json::to_string(&summary(&itg(), 1.0, 0.5, Lamp::Clear)).unwrap();
    let h = History::from_summaries(vec![
        ("a".into(), Some(good)),
        ("b".into(), Some("{\"version\":99}".into())),
        ("c".into(), None),
    ]);
    assert_eq!(h.len(), 1);
    assert_eq!(h.unreadable(), ["b".to_string(), "c".to_string()]);
    assert_eq!(
        h.ids(),
        vec!["a".to_string(), "b".to_string(), "c".to_string()]
    );
}

#[test]
fn plays_are_removed_by_chart_by_songs_or_all() {
    let song = song();
    let store = Rc::new(MemoryPlayStore::default());
    let plays = Plays::new(store.clone());
    let (log, results) = play(&song, 0, &itg(), Some(0.0));
    let (log1, results1) = play(&song, 1, &itg(), Some(0.0));
    for (id, songid, index) in [("a", "s1", 0), ("b", "s1", 1), ("c", "s2", 0)] {
        let mut rec = record(&song, index, &itg(), 1.0, false);
        rec.chart.song = songid.into();
        let (l, r) = if index == 0 {
            (&log, &results)
        } else {
            (&log1, &results1)
        };
        plays.record(id.into(), &rec, l, r, |r| r.unwrap()).unwrap();
    }
    plays.remove_chart(song.chart_digest(1), |r| r.unwrap());
    assert_eq!(plays.history().ids(), vec!["a".to_string(), "c".into()]);
    plays.remove_songs(&["s2".into()], |r| r.unwrap());
    assert_eq!(plays.history().ids(), vec!["a".to_string()]);
    assert_eq!(store.len(), 1);
    let text = Rc::new(RefCell::new(String::new()));
    let t = text.clone();
    let (n, done) = answered();
    plays.export(8, move |chunk| t.borrow_mut().push_str(&chunk), done);
    assert_eq!(n.borrow_mut().take().unwrap().unwrap(), 1);
    assert_eq!(text.borrow().lines().count(), 1);
    plays.remove_all(|r| r.unwrap());
    assert!(plays.history().is_empty());
    assert!(store.is_empty());
}

#[test]
fn a_play_recorded_before_the_summaries_load_is_kept() {
    let song = song();
    let store = Rc::new(MemoryPlayStore::default());
    let first = Plays::new(store.clone());
    let (log, results) = play(&song, 0, &itg(), Some(0.0));
    first
        .record(
            "a".into(),
            &record(&song, 0, &itg(), 1.0, false),
            &log,
            &results,
            |_| {},
        )
        .unwrap();
    let plays = Plays::new(store);
    plays
        .record(
            "b".into(),
            &record(&song, 0, &itg(), 2.0, true),
            &log,
            &results,
            |_| {},
        )
        .unwrap();
    plays.load(|r| r.unwrap());
    assert_eq!(plays.history().ids(), vec!["a".to_string(), "b".into()]);
    // b was autoplay: kept, not counted.
    let digest = plays.history().get("a").unwrap().chart.digest;
    assert_eq!(
        plays.history().best(digest, &itg(), false).unwrap().plays,
        1
    );
}

/// Version 1 of the formats as first written (chart 1 of [`SM`], 10 ms
/// late): the reader keeps reading it.
#[test]
fn version_1_entries_keep_reading() {
    const SUMMARY: &str = r#"{"version":1,"chart":{"song":"s1","digest":"e48a15db1323fdff0ee79b944c3da2d3","index":1},"ruleset":{"id":"itg","holds":"Dance","danoni":null},"time":1.0,"autoplay":false,"ended":"finished","worked":{"judge_version":1,"score":{"money":null,"percent":1.0,"grade_percent":null,"ex":12,"max_ex":12},"grade":"★★★★","lamp":"marvelous-full-combo","full_combo":"MarvelousFC","assist":false,"failed":false,"tally":{"taps":[4,0,0,0,0,0],"held":0,"let_go":0,"mine_hit":0,"boo":0,"checkpoint_hit":0,"checkpoint_miss":0},"max_combo":4,"fast":0,"slow":4,"hits":4,"mean":0.009999999999999896,"stddev":0.0}}"#;
    const RECORD: &str = r#"{"version":1,"chart":{"song":"s1","digest":"e48a15db1323fdff0ee79b944c3da2d3","index":1},"ruleset":{"id":"itg","holds":"Dance","danoni":null},"options":{"scroll":{"speed":{"XMod":1.0},"reverse":false,"scroll_action":"Normal"},"transform":{"turn":"Off","cut":"Off","no_jumps":false,"no_holds":false},"appearance":"Visible","seed":0,"clock":{"audio_offset":0.0,"visual_offset":0.0,"rate":1.0},"visible_range":12.0,"receptor_snap":true},"song_offset":0.0,"setup":{"clock":{"audio_offset":0.0,"audio_source":"default","visual_offset":0.0,"visual_source":"default","audio":null,"display":null,"output_latency":0.0,"sample_rate":0.0,"refresh_hz":0.0,"latency_min":0.0,"latency_max":0.0},"drawn":{"noteskin":null,"note_colors":"","chart_colors":false,"field_filter":0.0,"bg_brightness":0.0,"background_frame":"","video":"","show_deltas":false,"graphics":""},"inputs":[{"kind":"keyboard"}]},"settings":{"a_field_added_later":[1,2],"speed":2.0},"ended":"finished","autoplay":null,"time":1.0,"app_version":"0.1.0"}"#;
    const LOG: &str = "AQAAAAAAAAAACJCIJ4Cw6gGT8Nccg7DqAZHw1xyBsOoBkvDXHIKw6gE=";
    const TIMING: &str = "AQSU3J7CBwAAoJwByMLrNQMAoJwB0vD6BwEAoJwBgICABAIAoJwBBQCAgIDwB5TcnsIHlLEQyMLrNZSxENLw+geUsRCAgIAElLEQ";
    use ddi_platform::history::base64_decode;
    let summary: PlaySummary = serde_json::from_str(SUMMARY).unwrap();
    assert_eq!(summary.worked.unwrap().lamp, Lamp::MarvelousFullCombo);
    let record: PlayRecord = serde_json::from_str(RECORD).unwrap();
    assert_eq!(record.setup.inputs, [InputDevice::Keyboard]);
    let entry = PlayEntry {
        id: "x".into(),
        summary: Some(SUMMARY.into()),
        record: RECORD.into(),
        log: base64_decode(LOG).unwrap(),
        timing: base64_decode(TIMING).unwrap(),
    };
    assert_eq!(PlayLog::decode(&entry.log).unwrap().inputs.len(), 8);
    assert_eq!(PlayTiming::decode(&entry.timing).unwrap().timeline.len(), 4);
    // The chart is found by its digest, and judged again.
    let song = song();
    assert_eq!(find_chart(&song, &record.chart), Some(1));
    let again = rework(&entry, &song).unwrap();
    assert_eq!(again.tally.taps.iter().sum::<u32>(), 4);
}

/// Summaries that do not read are rebuilt: a `worked` of a later format
/// (or damaged) is worked out again; a summary lost or broken is rebuilt
/// from the record; a play whose record does not read either stays listed
/// and is removed with the rest.
#[test]
fn summaries_that_do_not_read_are_rebuilt() {
    let song = Rc::new(song());
    let store = Rc::new(MemoryPlayStore::default());
    let plays = Plays::new(store.clone());
    let (log, results) = play(&song, 0, &itg(), Some(0.0));
    for (i, id) in ["a", "b", "c", "d"].into_iter().enumerate() {
        plays
            .record(
                id.into(),
                &record(&song, 0, &itg(), i as f64, false),
                &log,
                &results,
                |r| r.unwrap(),
            )
            .unwrap();
    }
    let fresh = plays.history().get("a").unwrap().worked.clone();
    let (entries, done) = answered();
    store.records(
        vec!["a".into(), "b".into(), "c".into(), "d".into()],
        Box::new(done),
    );
    let mut entries = entries.borrow_mut().take().unwrap().unwrap();
    // a: `worked` of a later format; b: no summary; c: a broken summary;
    // d: neither summary nor record read.
    let mut v: serde_json::Value =
        serde_json::from_str(entries[0].summary.as_ref().unwrap()).unwrap();
    v["worked"]["lamp"] = "a-lamp-from-the-future".into();
    entries[0].summary = Some(v.to_string());
    entries[1].summary = None;
    entries[2].summary = Some("{broken".into());
    entries[3].summary = Some("42".into());
    entries[3].record = "{broken".into();
    for e in entries {
        store.put(e);
    }
    let plays = Plays::new(store.clone());
    let (loaded, done) = answered();
    plays.load(done);
    assert!(matches!(loaded.borrow().as_ref(), Some(Ok(()))));
    {
        let h = plays.history();
        assert_eq!(h.len(), 3);
        assert_eq!(h.unreadable(), ["d".to_string()]);
        assert_eq!(h.stale_count(), 3);
        assert!(h.get("a").unwrap().worked.is_none());
        assert_eq!(h.get("b").unwrap().time, 1.0);
        // The bests wait for them.
        assert!(h.best(song.chart_digest(0), &itg(), false).is_none());
    }
    let (n, done) = answered();
    plays.rework_song("s1", song.clone(), 10, done);
    assert_eq!(n.borrow_mut().take().unwrap().unwrap(), 3);
    for id in ["a", "b", "c"] {
        assert_eq!(plays.history().get(id).unwrap().worked, fresh, "{id}");
    }
    assert_eq!(
        plays
            .history()
            .best(song.chart_digest(0), &itg(), false)
            .unwrap()
            .plays,
        3
    );
    // Kept in the store: the next start reads them as they are now.
    let again = Plays::new(store.clone());
    again.load(|r| r.unwrap());
    assert_eq!(again.history().stale_count(), 0);
    // The export keeps what does not read, as text.
    let text = Rc::new(RefCell::new(String::new()));
    let t = text.clone();
    again.export(
        2,
        move |c| t.borrow_mut().push_str(&c),
        |r| assert_eq!(r.unwrap(), 4),
    );
    let d: serde_json::Value = serde_json::from_str(text.borrow().lines().nth(3).unwrap()).unwrap();
    assert_eq!(d["record"], "{broken");
    again.remove_all(|r| r.unwrap());
    assert!(again.history().is_empty());
    assert!(store.is_empty());
}

/// A play the store could not keep is not kept in memory either, and the
/// shell hears why.
#[test]
fn a_play_that_could_not_be_written_is_rolled_back() {
    let song = song();
    let store = Rc::new(MemoryPlayStore::default());
    store.failing.set(true);
    let plays = Plays::new(store.clone());
    let (log, results) = play(&song, 0, &itg(), Some(0.0));
    let (r, done) = answered();
    plays
        .record(
            "a".into(),
            &record(&song, 0, &itg(), 1.0, false),
            &log,
            &results,
            done,
        )
        .unwrap();
    assert_eq!(
        r.borrow_mut().take().unwrap(),
        Err("the store is full".into())
    );
    assert!(plays.history().is_empty());
}
