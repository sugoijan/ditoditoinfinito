//! Plays of found songs (`origin.rs`): the origin kept on a play, read
//! back, bounded, through an export and an import untouched; the songs
//! played from a source listed from the origins alone; a play kept under
//! a found song's id counted once beside a library copy of its chart.

use std::rc::Rc;

use ddi_platform::history::MemoryPlayStore;

use super::tests::{answered, itg, play, record, song};
use super::*;

fn origin(item: &str, revision: &str, title: &str) -> PlayOrigin {
    PlayOrigin {
        source: "example".into(),
        item: item.into(),
        revision: Some(revision.into()),
        song: 0,
        title: title.into(),
        artist: "Test Artist".into(),
        chart: OriginChart {
            name: String::new(),
            style: "dance-single".into(),
            difficulty: ddi_chart::Difficulty::Hard,
            meter: 5,
        },
    }
}

/// A history over a store of its own, read.
fn plays() -> (Rc<MemoryPlayStore>, Rc<Plays>) {
    let store = Rc::new(MemoryPlayStore::default());
    let plays = Plays::new(store.clone());
    plays.load(|r| r.unwrap());
    (store, plays)
}

/// Play `id` as `store` keeps it.
fn stored(store: &MemoryPlayStore, id: &str) -> PlayEntry {
    let (got, done) = answered();
    store.records(vec![id.to_string()], Box::new(done));
    let entries: Vec<PlayEntry> = got.borrow_mut().take().unwrap().unwrap();
    entries.into_iter().next().unwrap()
}

/// A play of chart `chart` kept as song `song_id` at `time`, from
/// `origin`.
fn keep(p: &Rc<Plays>, song_id: &str, chart: usize, time: f64, origin: Option<PlayOrigin>) -> String {
    let s = song();
    let (log, results) = play(&s, chart, &itg(), Some(0.0));
    let mut rec = record(&s, chart, &itg(), time, false);
    rec.chart.song = song_id.into();
    rec.origin = origin;
    let id = play_id(time, 1);
    p.record(id.clone(), &rec, &log, &results, |r| r.unwrap())
        .unwrap();
    id
}

#[test]
fn an_origin_is_kept_on_the_play_and_its_summary_and_read_back() {
    let (store, p) = plays();
    let o = origin("song-1", "r1", "Generated Song 1");
    let id = keep(&p, "src:00000000000000aa:0", 0, 1_000.0, Some(o.clone()));
    assert_eq!(p.history().get(&id).unwrap().origin.as_ref(), Some(&o));
    let entry = stored(&store, &id);
    let rec: PlayRecord = serde_json::from_str(&entry.record).unwrap();
    assert_eq!(rec.origin.as_ref(), Some(&o));
    // Read again at a start: the summary's.
    let again = Plays::new(store.clone());
    again.load(|r| r.unwrap());
    assert_eq!(again.history().get(&id).unwrap().origin.as_ref(), Some(&o));
    // A summary lost: made again from its record, the origin with it.
    let rebuilt = PlaySummary::from_record(&entry.record).unwrap();
    assert_eq!(rebuilt.origin.as_ref(), Some(&o));
    // A play of any other song says none, and writes none (records kept
    // before there was one read the same).
    let plain = keep(&p, "s1", 0, 2_000.0, None);
    let entry = stored(&store, &plain);
    assert!(!entry.record.contains("origin"), "{}", entry.record);
    assert!(!entry.summary.as_deref().unwrap().contains("origin"));
    assert!(p.history().get(&plain).unwrap().origin.is_none());
}

#[test]
fn an_origin_out_of_bounds_reads_as_none_and_its_texts_are_cut() {
    let long_id = "i".repeat(MAX_ORIGIN_ID + 1);
    for bad in [
        PlayOrigin {
            item: long_id.clone(),
            ..origin("x", "r", "T")
        },
        PlayOrigin {
            source: String::new(),
            ..origin("x", "r", "T")
        },
        PlayOrigin {
            revision: Some(long_id.clone()),
            ..origin("x", "r", "T")
        },
        PlayOrigin {
            song: MAX_ORIGIN_SONGS,
            ..origin("x", "r", "T")
        },
    ] {
        assert!(bad.clone().bounded().is_none(), "{bad:?}");
        // In a record: the play reads, without an origin.
        let mut rec = record(&song(), 0, &itg(), 1.0, false);
        rec.origin = Some(bad);
        let text = serde_json::to_string(&rec).unwrap();
        let read: PlayRecord = serde_json::from_str(&text).unwrap();
        assert!(read.origin.is_none());
        assert!(PlaySummary::from_record(&text).unwrap().origin.is_none());
    }
    // A title of many characters of several bytes: cut at an edge.
    let long = "曲".repeat(200);
    let o = PlayOrigin {
        title: long.clone(),
        artist: long.clone(),
        ..origin("x", "r", "T")
    }
    .bounded()
    .unwrap();
    assert!(o.title.len() <= MAX_ORIGIN_TEXT && o.title.chars().all(|c| c == '曲'));
    assert!(o.artist.len() <= MAX_ORIGIN_TEXT && !o.artist.is_empty());
    // A wrong shape is none, never a record that does not read.
    let mut v = serde_json::to_value(record(&song(), 0, &itg(), 1.0, false)).unwrap();
    v["origin"] = serde_json::json!({ "source": 3 });
    let read: PlayRecord = serde_json::from_value(v).unwrap();
    assert!(read.origin.is_none());
}

#[test]
fn an_export_and_an_import_keep_the_origin_untouched() {
    let (_, p) = plays();
    let o = origin("song-7", "r1", "Generated Song 7");
    keep(&p, "src:00000000000000bb:0", 0, 1_700_000_000_000.0, Some(o.clone()));
    let out = Rc::new(std::cell::RefCell::new(String::new()));
    let (o2, done) = (out.clone(), answered());
    p.export(10, move |chunk| o2.borrow_mut().push_str(&chunk), done.1);
    assert_eq!(done.0.borrow_mut().take(), Some(Ok(1)));
    let text = out.borrow().clone();
    let line = text.lines().next().unwrap();
    // Read back by another history (the web's, say): the record's text as
    // it came, the summary made from it with the origin.
    let imported = read_line(line).unwrap();
    let original = PlayEntry::from_json(line).unwrap();
    assert_eq!(imported.entry.record, original.record);
    assert_eq!(imported.summary.origin.as_ref(), Some(&o));
    let (_, other) = plays();
    let (n, done) = answered();
    other.import(vec![imported], done);
    assert_eq!(n.borrow_mut().take(), Some(Ok((1, 0))));
    let h = other.history();
    let (rows, total) = h.played_before("example", 10);
    assert_eq!((rows.len(), total), (1, 1));
    assert_eq!(rows[0].origin, o);
}

#[test]
fn the_songs_played_from_a_source_are_listed_from_the_origins_newest_first() {
    let (_, p) = plays();
    // Song 1 played twice (its second play under a new revision of its
    // item), song 2 once, between them; one from another source; one with
    // no origin.
    keep(&p, "src:01:0", 0, 1_000.0, Some(origin("song-1", "r1", "One")));
    keep(&p, "src:02:0", 1, 2_000.0, Some(origin("song-2", "r1", "Two")));
    let mut renamed = origin("song-1", "r2", "One (new)");
    renamed.chart.meter = 6;
    keep(&p, "src:03:0", 0, 3_000.0, Some(renamed.clone()));
    keep(
        &p,
        "src:04:0",
        0,
        4_000.0,
        Some(PlayOrigin {
            source: "other".into(),
            ..origin("song-9", "r1", "Nine")
        }),
    );
    keep(&p, "s1", 0, 5_000.0, None);
    let h = p.history();
    assert_eq!(h.origin_sources(), ["example", "other"]);
    let (rows, total) = h.played_before("example", 10);
    assert_eq!(total, 2);
    let titles: Vec<&str> = rows.iter().map(|r| r.origin.title.as_str()).collect();
    assert_eq!(titles, ["One (new)", "Two"], "the latest play's, newest first");
    assert_eq!(rows[0].plays, 2);
    assert_eq!(rows[0].origin.revision.as_deref(), Some("r2"));
    assert_eq!(rows[0].last_played, 3_000.0);
    assert_eq!(rows[0].digest, song().chart_digest(0));
    // The chart's best, of every play of its content (the library's play
    // of it, `s1`, too): four plays of chart 0 in all.
    let best = rows[0].best.as_ref().unwrap();
    assert!(best.lamp >= Lamp::FullCombo, "{best:?}");
    assert_eq!(
        h.best(song().chart_digest(0), &itg(), false).unwrap().plays,
        4,
        "each play counted once"
    );
    // Bounded: the newest alone, the count of all.
    let (rows, total) = h.played_before("example", 1);
    assert_eq!((rows.len(), total), (1, 2));
    assert!(h.played_before("nobody", 10).0.is_empty());
}

#[test]
fn a_found_songs_play_counts_once_for_a_library_copy_of_its_chart() {
    // Kept under the found song's id; the library then has the same chart
    // under its own id (imported since): the plays list, the bests and
    // "played" find it by content, once.
    let (_, p) = plays();
    let found = keep(&p, "src:0000000000000abc:0", 0, 1_000.0, Some(origin("song-1", "r1", "One")));
    let library = keep(&p, "u-0123", 0, 2_000.0, None);
    let h = p.history();
    let digest = song().chart_digest(0);
    let rows = play_rows(&h, digest, None, false, &|_| 0);
    let ids: Vec<&str> = rows.iter().map(|r| r.id.as_str()).collect();
    assert_eq!(ids, [library.as_str(), found.as_str()]);
    let bests = h.bests_of_charts(&[digest]);
    assert_eq!(bests.len(), 1);
    assert_eq!(bests.values().next().unwrap().plays, 2);
    assert!(h.played().song("u-0123", [Some(digest)].into_iter()));
}
