//! Plays imported from an export: round trips, duplicates, and lines that
//! are not plays (`import.rs`).

use std::rc::Rc;

use ddi_platform::history::{MemoryPlayStore, base64_encode};
use serde_json::Value;

use super::tests::{answered, itg, play, record, song};
use super::*;

/// A history over a store of its own, read.
fn plays() -> (Rc<MemoryPlayStore>, Rc<Plays>) {
    let store = Rc::new(MemoryPlayStore::default());
    let plays = Plays::new(store.clone());
    plays.load(|r| r.unwrap());
    (store, plays)
}

/// Plays pressed `bias` seconds late (one each), kept in a fresh history.
fn kept(biases: &[f64]) -> (Rc<MemoryPlayStore>, Rc<Plays>) {
    let song = song();
    let (store, p) = plays();
    for (i, bias) in biases.iter().enumerate() {
        let chart = i % 2;
        let (log, results) = play(&song, chart, &itg(), Some(*bias));
        let time = 1_700_000_000_000.0 + i as f64;
        let rec = record(&song, chart, &itg(), time, i == 2);
        p.record(play_id(time, i as u32), &rec, &log, &results, |r| {
            r.unwrap()
        })
        .unwrap();
    }
    (store, p)
}

/// The history's export as one text.
fn export(p: &Plays) -> String {
    let out = Rc::new(std::cell::RefCell::new(String::new()));
    let o = out.clone();
    p.export(
        2,
        move |lines| o.borrow_mut().push_str(&lines),
        |r| {
            r.unwrap();
        },
    );
    out.take()
}

/// `text` imported into `p`, pushed `chunk` bytes at a time.
fn import(p: &Rc<Plays>, text: &[u8], chunk: usize) -> ImportReport {
    let mut reader = PlayImport::new(p.history().ids());
    for part in text.chunks(chunk.max(1)) {
        reader.push(part);
    }
    reader.finish();
    let (r, done) = answered();
    p.import(reader.take(), done);
    let (new, kept) = r.take().unwrap().unwrap();
    let mut report = reader.report().clone();
    report.imported = new;
    report.duplicates += kept;
    report
}

/// Every stale play of `p` judged again on the test song.
fn rework_all(p: &Rc<Plays>) {
    let song = Rc::new(song());
    for id in p.stale_songs() {
        let (r, done) = answered();
        p.rework_song(&id, song.clone(), usize::MAX, done);
        r.take().unwrap().unwrap();
    }
}

#[test]
fn an_export_imported_elsewhere_comes_back_whole_and_judged_again() {
    let (from, a) = kept(&[0.0, 0.03, 0.0, -0.05]);
    let text = export(&a);
    assert_eq!(text.lines().count(), 4);
    // Byte by byte or in large parts: the same.
    for chunk in [1, 7, 4096, usize::MAX] {
        let (to, b) = plays();
        let report = import(&b, text.as_bytes(), chunk);
        assert_eq!(
            report,
            ImportReport {
                imported: 4,
                ..ImportReport::default()
            },
            "chunk {chunk}"
        );
        assert_eq!(b.history().len(), 4);
        // Nothing worked out is trusted: every play waits to be judged
        // again, its score kept meanwhile (the bests pending).
        assert_eq!(b.history().stale_count(), 4);
        let song = song();
        let best = b
            .history()
            .best(song.chart_digest(0), &itg(), false)
            .unwrap();
        assert!(best.pending);
        // Raw data as it was: the record (the settings with the fields
        // this build does not know), the log, the timing.
        for (id, _) in a.history().iter() {
            let (x, done) = answered();
            from.records(vec![id.clone()], Box::new(done));
            let (y, done) = answered();
            to.records(vec![id.clone()], Box::new(done));
            let (x, y) = (x.take().unwrap().unwrap(), y.take().unwrap().unwrap());
            let (x, y) = (&x[0], &y[0]);
            assert_eq!((&x.log, &x.timing), (&y.log, &y.timing));
            let rx: Value = serde_json::from_str(&x.record).unwrap();
            let ry: Value = serde_json::from_str(&y.record).unwrap();
            assert_eq!(rx, ry);
            assert_eq!(ry["settings"]["a_field_added_later"][1], 2);
        }
        // Judged again: what the plays came to where they were played.
        rework_all(&b);
        assert_eq!(b.history().stale_count(), 0);
        for (id, s) in a.history().iter() {
            assert_eq!(b.history().get(id), Some(s), "{id}");
        }
        assert_eq!(b.history().bests(), a.history().bests());
        // The export of the import is the export, line for line, once
        // judged again (summaries and all).
        let again = export(&b);
        for (x, y) in text.lines().zip(again.lines()) {
            let (x, y): (Value, Value) = (
                serde_json::from_str(x).unwrap(),
                serde_json::from_str(y).unwrap(),
            );
            assert_eq!(x, y);
        }
    }
}

#[test]
fn plays_kept_already_or_twice_in_the_file_are_left() {
    let (_, a) = kept(&[0.0, 0.01, 0.02]);
    let text = export(&a);
    let (store, b) = plays();
    assert_eq!(import(&b, text.as_bytes(), 64).imported, 3);
    // Again: all kept already.
    let report = import(&b, text.as_bytes(), 64);
    assert_eq!((report.imported, report.duplicates), (0, 3));
    assert_eq!(store.len(), 3);
    // A play twice in one file, and one already kept.
    let (_, c) = plays();
    let first = text.lines().next().unwrap();
    let doubled = format!("{first}\n{first}\n{text}");
    let report = import(&c, doubled.as_bytes(), 5);
    assert_eq!((report.imported, report.duplicates), (3, 2));
    // Into the history it came from: nothing new.
    let report = import(&a, text.as_bytes(), 1000);
    assert_eq!((report.imported, report.duplicates), (0, 3));
    assert!(
        report
            .notice()
            .starts_with("0 plays imported; 3 plays already kept.")
    );
}

/// A line of `entry`'s export with `f` done to its JSON.
fn changed(line: &str, f: impl FnOnce(&mut Value)) -> String {
    let mut v: Value = serde_json::from_str(line).unwrap();
    f(&mut v);
    v.to_string()
}

#[test]
fn lines_that_are_not_plays_are_counted_and_named() {
    let (_, a) = kept(&[0.0]);
    let text = export(&a);
    let good = text.trim_end();
    let bad = [
        // Not JSON, not an object, no log.
        "{\"id\":".to_string(),
        "[1,2,3]".into(),
        changed(good, |v| {
            v.as_object_mut().unwrap().remove("log");
        }),
        // Bad base64, and a log that does not read.
        changed(good, |v| v["log"] = "@@@@".into()),
        changed(good, |v| v["log"] = base64_encode(&[1, 0, 0]).into()),
        changed(good, |v| v["timing"] = base64_encode(&[1, 9]).into()),
        // Ids that are not a play's (one would leave the plays folder).
        changed(good, |v| v["id"] = "../../storage".into()),
        changed(good, |v| v["id"] = "000000000000001-0000000G".into()),
        // A record that does not read, one that is text, none.
        changed(good, |v| v["record"]["chart"] = 5.into()),
        changed(good, |v| v["record"] = "{broken".into()),
        changed(good, |v| v["record"] = Value::Null),
        changed(good, |v| v["record"]["version"] = 0.into()),
        changed(good, |v| v["record"]["time"] = (-1.0).into()),
        changed(good, |v| v["record"]["song_offset"] = 1e12.into()),
        changed(good, |v| v["record"]["chart"]["song"] = "".into()),
    ];
    // An input log whose times are past any song.
    let far = PlayLog {
        start: 0.0,
        inputs: vec![LoggedInput {
            time: 1e9,
            lane: 0,
            pressed: true,
        }],
    };
    let far = changed(good, |v| v["log"] = base64_encode(&far.encode()).into());
    let mut file = String::new();
    for l in bad.iter().chain([&far]) {
        file.push_str(l);
        file.push('\n');
    }
    let (_, b) = plays();
    let report = import(&b, file.as_bytes(), 33);
    assert_eq!(report.imported, 0);
    assert_eq!(report.invalid, bad.len() + 1);
    assert_eq!(report.problems.len(), MAX_PROBLEMS);
    assert_eq!(report.problems[0].0, 1);
    assert_eq!(report.problems[3].0, 4);
    assert!(
        report.problems[3].1.contains("base64"),
        "{:?}",
        report.problems
    );
    assert!(b.history().is_empty());
    // Each named on its own.
    let why = |l: &str| match read_line(l) {
        Err(Refused::Invalid(w)) => w,
        other => panic!("{l}: {other:?}"),
    };
    assert!(why(&bad[6]).contains("id"));
    assert!(why(&bad[9]).contains("record"));
    assert!(why(&far).contains("out of range"));
    // The good line still reads among them.
    let report = import(&b, format!("{file}{good}").as_bytes(), 100);
    assert_eq!((report.imported, report.invalid), (1, bad.len() + 1));
}

#[test]
fn plays_from_a_newer_game_are_left_out_as_such() {
    let (_, a) = kept(&[0.0]);
    let text = export(&a);
    let good = text.trim_end();
    let newer = [
        changed(good, |v| {
            v["record"]["version"] = (RECORD_VERSION + 1).into()
        }),
        changed(good, |v| {
            v["record"]["judge_version"] = (JUDGE_VERSION + 1).into()
        }),
        changed(good, |v| v["log"] = base64_encode(&[9, 0]).into()),
    ];
    for l in &newer {
        assert!(matches!(read_line(l), Err(Refused::Newer(_))), "{l}");
    }
    let (_, b) = plays();
    let report = import(&b, newer.join("\n").as_bytes(), 10);
    assert_eq!((report.imported, report.newer, report.invalid), (0, 3, 0));
    assert!(
        report.notice().contains("from a newer version"),
        "{}",
        report.notice()
    );
}

#[test]
fn a_line_too_long_is_skipped_without_being_kept() {
    let (_, a) = kept(&[0.0, 0.01]);
    let text = export(&a);
    let mut lines = text.lines();
    let (first, second) = (lines.next().unwrap(), lines.next().unwrap());
    // A line past the bound between two plays, fed in parts: never held
    // whole, counted once.
    let long = vec![b'x'; MAX_LINE + 10];
    let (_, b) = plays();
    let mut reader = PlayImport::new(b.history().ids());
    reader.push(first.as_bytes());
    reader.push(b"\n");
    for part in long.chunks(1 << 20) {
        reader.push(part);
    }
    reader.push(b"\n");
    reader.push(second.as_bytes());
    reader.finish();
    assert_eq!(reader.ready(), 2);
    assert_eq!(reader.report().invalid, 1);
    assert_eq!(
        reader.report().problems,
        vec![(2, "longer than 4 MB".into())]
    );
    // A file that is one long line, without its end.
    let mut reader = PlayImport::new(Vec::new());
    for part in long.chunks(1 << 20) {
        reader.push(part);
    }
    reader.finish();
    assert_eq!((reader.ready(), reader.report().invalid), (0, 1));
}

#[test]
fn byte_order_marks_line_breaks_and_blank_lines_are_read_through() {
    let (_, a) = kept(&[0.0, 0.01]);
    let text = export(&a).replace('\n', "\r\n\r\n");
    let mut file = "\u{feff}".as_bytes().to_vec();
    file.extend_from_slice(text.as_bytes());
    // Not UTF-8: one line left out.
    file.extend_from_slice(b"\xff\xfe\n");
    let (_, b) = plays();
    let report = import(&b, &file, 3);
    assert_eq!((report.imported, report.invalid), (2, 1));
    assert_eq!(report.problems[0].1, "not UTF-8 text");
    // An empty file imports nothing.
    assert_eq!(import(&b, b"", 1).imported, 0);
}

#[test]
fn a_score_the_line_claims_is_never_taken_as_judged() {
    let song = song();
    let (_, a) = kept(&[0.08]);
    let text = export(&a);
    let truth = a.history().iter().next().unwrap().1.clone();
    // A summary claiming a better play.
    let forged = changed(text.trim_end(), |v| {
        v["summary"]["worked"]["lamp"] = "marvelous-full-combo".into();
        v["summary"]["worked"]["score"]["percent"] = 1.0.into();
    });
    let read = read_line(&forged).unwrap();
    let w = read.summary.worked.as_ref().unwrap();
    assert_eq!(w.judge_version, 0);
    assert_eq!(w.lamp, Lamp::MarvelousFullCombo);
    // A summary of another play is not this one's.
    let other = changed(text.trim_end(), |v| v["summary"]["time"] = 5.0.into());
    assert_eq!(read_line(&other).unwrap().summary.worked, None);
    // None at all, or text: made from the record.
    let none = changed(text.trim_end(), |v| v["summary"] = Value::Null);
    assert_eq!(read_line(&none).unwrap().summary.chart, truth.chart);
    // Judged again: what the play really came to.
    let (_, b) = plays();
    assert_eq!(import(&b, forged.as_bytes(), 100).imported, 1);
    rework_all(&b);
    let (id, s) = b
        .history()
        .iter()
        .next()
        .map(|(i, s)| (i.clone(), s.clone()))
        .unwrap();
    assert_eq!(Some(&s), a.history().get(&id));
    assert!(s.worked.unwrap().lamp < Lamp::MarvelousFullCombo);
    assert!(
        !b.history()
            .best(song.chart_digest(0), &itg(), false)
            .unwrap()
            .pending
    );
}

#[test]
fn plays_of_songs_not_here_are_kept_until_their_song_comes() {
    let (_, a) = kept(&[0.0]);
    let text = export(&a);
    let elsewhere = changed(text.trim_end(), |v| {
        v["record"]["chart"]["song"] = "a-pack/a-song".into();
        v["summary"]["chart"]["song"] = "a-pack/a-song".into();
    });
    let (store, b) = plays();
    assert_eq!(import(&b, elsewhere.as_bytes(), 100).imported, 1);
    assert_eq!(store.len(), 1);
    // Its song not in the library: it waits, stale.
    assert_eq!(b.stale_songs(), vec!["a-pack/a-song".to_string()]);
    // Its chart found (here, the same notes under another id): judged.
    let song = Rc::new(song());
    let (r, done) = answered();
    b.rework_song("a-pack/a-song", song, 8, done);
    assert_eq!(r.take().unwrap(), Ok(1));
    assert_eq!(b.history().stale_count(), 0);
}

#[test]
fn an_import_that_cannot_be_kept_leaves_nothing() {
    let (_, a) = kept(&[0.0, 0.01]);
    let text = export(&a);
    let (store, b) = plays();
    store.failing.set(true);
    let mut reader = PlayImport::new(b.history().ids());
    reader.push(text.as_bytes());
    reader.finish();
    let (r, done) = answered();
    b.import(reader.take(), done);
    assert_eq!(r.take().unwrap(), Err("the store is full".into()));
    assert!(b.history().is_empty());
    assert!(store.is_empty());
    // Before the history is read: refused.
    let unread = Plays::new(Rc::new(MemoryPlayStore::default()));
    let (r, done) = answered();
    unread.import(Vec::new(), done);
    assert!(r.take().unwrap().is_err());
}

#[test]
fn plays_a_shell_kept_itself_are_adopted_once() {
    let (_, a) = kept(&[0.0, 0.01]);
    let text = export(&a);
    let (_, b) = plays();
    let mut reader = PlayImport::new(Vec::new());
    reader.push(text.as_bytes());
    reader.finish();
    let read: Vec<(String, PlaySummary)> = reader
        .take()
        .into_iter()
        .map(|p| (p.entry.id, p.summary))
        .collect();
    let g = b.generation();
    assert_eq!(b.adopt(read.clone()), 2);
    assert!(b.generation() > g);
    assert_eq!(b.adopt(read), 0);
    assert_eq!(b.history().len(), 2);
}

#[test]
fn a_file_with_too_many_plays_is_cut() {
    let mut reader = PlayImport::new(Vec::new());
    // Blank lines are not plays; garbage lines are, as far as counting.
    let mut text = String::new();
    for _ in 0..MAX_PLAYS + 3 {
        text.push_str("x\n\n");
    }
    reader.push(text.as_bytes());
    reader.finish();
    let r = reader.report();
    assert!(r.cut && reader.is_full());
    assert_eq!(r.invalid, MAX_PLAYS);
    assert!(r.notice().contains(&format!("first {MAX_PLAYS}")));
}

#[test]
fn ids_are_play_ids_only() {
    assert!(is_play_id(&play_id(1_760_000_000_000.0, 0xdead_beef)));
    for id in [
        "",
        "a",
        "../000000000000-00000000",
        "000000000000001-0000000A",
    ] {
        assert!(!is_play_id(id), "{id}");
    }
}
