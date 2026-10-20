//! Where the play history is kept (`docs/plans/play-history.md`): one
//! [`PlayEntry`] a play, written by the shell through a [`PlayStore`]
//! (the web: IndexedDB; desktop: files) and read back by
//! `ddi_ui::state::history`, which knows what the entries hold. Here they
//! are opaque: two JSON texts (the summary, small and loaded at start; the
//! raw record) and two byte strings (the input log and the timing as
//! played, each in the engine's own versioned encoding).
//!
//! Stores answer through callbacks ([`Reply`]): IndexedDB answers later,
//! files and memory at once. Nothing here waits.

use std::cell::RefCell;
use std::collections::BTreeMap;

use serde_json::{Map, Value};

/// One play as stored.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct PlayEntry {
    /// `<time>-<random>` ([`play_id`]): sorts by when it was played.
    pub id: String,
    /// The summary, JSON (what is worked out; rewritten when stale).
    /// `None`: the store has the play but no summary for it.
    pub summary: Option<String>,
    /// The raw record, JSON (never rewritten).
    pub record: String,
    /// The input log, encoded.
    pub log: Vec<u8>,
    /// The notes' timing as played, encoded.
    pub timing: Vec<u8>,
}

/// What a store calls back with.
pub type Reply<T> = Box<dyn FnOnce(Result<T, String>)>;

/// Where plays are kept. Every call answers once, through `done`, now or
/// later; a call with nothing to do answers at once.
pub trait PlayStore {
    /// Keeps a new play.
    fn append(&self, entry: PlayEntry, done: Reply<()>);
    /// Every play's `(id, summary)`, in id order: a play kept without a
    /// summary (or with one that is not text) too, as `None`.
    fn summaries(&self, done: Reply<Vec<(String, Option<String>)>>);
    /// The plays with these ids, whole (ids not kept are left out).
    fn records(&self, ids: Vec<String>, done: Reply<Vec<PlayEntry>>);
    /// New summaries for kept plays: `(id, summary)`; ids not kept are
    /// left alone.
    fn replace_summaries(&self, summaries: Vec<(String, String)>, done: Reply<()>);
    /// Forgets these plays.
    fn remove(&self, ids: Vec<String>, done: Reply<()>);
    /// Every play, whole, in id order, about `batch` at a time: `each` is
    /// given them as they are read (so the whole history is never in
    /// memory at once); `done` answers with how many there were.
    fn export(&self, batch: usize, each: Box<dyn FnMut(Vec<PlayEntry>)>, done: Reply<usize>);
}

/// A play's id: the time in milliseconds since the Unix epoch, padded so
/// ids sort as the plays were played, and a random part.
pub fn play_id(time_ms: f64, random: u32) -> String {
    format!("{:015}-{random:08x}", time_ms.max(0.0).round() as u64)
}

/// Plays in memory (tests; a shell with nowhere to write).
#[derive(Default)]
pub struct MemoryPlayStore {
    plays: RefCell<BTreeMap<String, PlayEntry>>,
    /// Writes fail (tests).
    pub failing: std::cell::Cell<bool>,
}

impl MemoryPlayStore {
    pub fn len(&self) -> usize {
        self.plays.borrow().len()
    }

    pub fn is_empty(&self) -> bool {
        self.plays.borrow().is_empty()
    }

    /// Keeps an entry as given (tests: a damaged store).
    pub fn put(&self, entry: PlayEntry) {
        self.plays.borrow_mut().insert(entry.id.clone(), entry);
    }

    fn check(&self) -> Result<(), String> {
        if self.failing.get() {
            Err("the store is full".into())
        } else {
            Ok(())
        }
    }
}

impl PlayStore for MemoryPlayStore {
    fn append(&self, entry: PlayEntry, done: Reply<()>) {
        if let Err(e) = self.check() {
            return done(Err(e));
        }
        self.plays.borrow_mut().insert(entry.id.clone(), entry);
        done(Ok(()))
    }

    fn summaries(&self, done: Reply<Vec<(String, Option<String>)>>) {
        let all = self
            .plays
            .borrow()
            .values()
            .map(|e| (e.id.clone(), e.summary.clone()))
            .collect();
        done(Ok(all))
    }

    fn records(&self, ids: Vec<String>, done: Reply<Vec<PlayEntry>>) {
        let plays = self.plays.borrow();
        let found = ids.iter().filter_map(|id| plays.get(id).cloned()).collect();
        drop(plays);
        done(Ok(found))
    }

    fn replace_summaries(&self, summaries: Vec<(String, String)>, done: Reply<()>) {
        if let Err(e) = self.check() {
            return done(Err(e));
        }
        let mut plays = self.plays.borrow_mut();
        for (id, summary) in summaries {
            if let Some(e) = plays.get_mut(&id) {
                e.summary = Some(summary);
            }
        }
        drop(plays);
        done(Ok(()))
    }

    fn remove(&self, ids: Vec<String>, done: Reply<()>) {
        let mut plays = self.plays.borrow_mut();
        for id in ids {
            plays.remove(&id);
        }
        drop(plays);
        done(Ok(()))
    }

    fn export(&self, batch: usize, mut each: Box<dyn FnMut(Vec<PlayEntry>)>, done: Reply<usize>) {
        let all: Vec<PlayEntry> = self.plays.borrow().values().cloned().collect();
        let n = all.len();
        for chunk in all.chunks(batch.max(1)) {
            each(chunk.to_vec());
        }
        done(Ok(n))
    }
}

/// A stored JSON text as a value: itself when it reads, else the text as a
/// JSON string (kept, never dropped).
fn json_or_text(s: &str) -> Value {
    serde_json::from_str::<Value>(s).unwrap_or_else(|_| Value::String(s.to_string()))
}

/// [`json_or_text`] read back: a string is the text itself.
fn text_of_json(v: &Value) -> String {
    match v {
        Value::String(s) => s.clone(),
        other => other.to_string(),
    }
}

impl PlayEntry {
    /// The entry as one line of JSON, everything in it: the summary and
    /// the record as JSON objects (as strings when they do not read; the
    /// summary `null` when there is none), the log and the timing in
    /// base64. The export writes one a line.
    pub fn to_json(&self) -> String {
        let mut o = Map::new();
        o.insert("id".into(), Value::String(self.id.clone()));
        o.insert(
            "summary".into(),
            self.summary.as_deref().map_or(Value::Null, json_or_text),
        );
        o.insert("record".into(), json_or_text(&self.record));
        o.insert("log".into(), Value::String(base64_encode(&self.log)));
        o.insert("timing".into(), Value::String(base64_encode(&self.timing)));
        Value::Object(o).to_string()
    }

    /// An entry from [`PlayEntry::to_json`]'s text.
    pub fn from_json(text: &str) -> Result<PlayEntry, String> {
        let v: Value = serde_json::from_str(text).map_err(|e| e.to_string())?;
        let field = |k: &str| v.get(k).ok_or_else(|| format!("no `{k}`"));
        let text_of = |k: &str| -> Result<String, String> {
            field(k)?
                .as_str()
                .map(str::to_string)
                .ok_or_else(|| format!("`{k}` is not text"))
        };
        Ok(PlayEntry {
            id: text_of("id")?,
            summary: match v.get("summary") {
                None | Some(Value::Null) => None,
                Some(s) => Some(text_of_json(s)),
            },
            record: text_of_json(field("record")?),
            log: base64_decode(&text_of("log")?)?,
            timing: base64_decode(&text_of("timing")?)?,
        })
    }
}

const BASE64: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";

/// Standard base64 (RFC 4648 §4), padded.
pub fn base64_encode(bytes: &[u8]) -> String {
    let mut out = String::with_capacity(bytes.len().div_ceil(3) * 4);
    for chunk in bytes.chunks(3) {
        let n = chunk
            .iter()
            .enumerate()
            .fold(0u32, |n, (i, b)| n | (u32::from(*b) << (16 - 8 * i)));
        for i in 0..4 {
            if i <= chunk.len() {
                out.push(BASE64[(n >> (18 - 6 * i)) as usize & 63] as char);
            } else {
                out.push('=');
            }
        }
    }
    out
}

/// Reads [`base64_encode`]'s text (padding optional).
pub fn base64_decode(text: &str) -> Result<Vec<u8>, String> {
    let digits: Vec<u8> = text
        .trim_end_matches('=')
        .bytes()
        .map(|c| {
            BASE64
                .iter()
                .position(|&d| d == c)
                .map(|p| p as u8)
                .ok_or_else(|| format!("not base64: {:?}", c as char))
        })
        .collect::<Result<_, _>>()?;
    if digits.len() % 4 == 1 {
        return Err("base64 cut short".into());
    }
    let mut out = Vec::with_capacity(digits.len() * 3 / 4);
    for chunk in digits.chunks(4) {
        let n = chunk
            .iter()
            .enumerate()
            .fold(0u32, |n, (i, d)| n | (u32::from(*d) << (18 - 6 * i)));
        for i in 0..chunk.len() - 1 {
            out.push((n >> (16 - 8 * i)) as u8);
        }
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::rc::Rc;

    #[test]
    fn base64_matches_the_rfc() {
        // RFC 4648 §10.
        for (raw, enc) in [
            ("", ""),
            ("f", "Zg=="),
            ("fo", "Zm8="),
            ("foo", "Zm9v"),
            ("foob", "Zm9vYg=="),
            ("fooba", "Zm9vYmE="),
            ("foobar", "Zm9vYmFy"),
        ] {
            assert_eq!(base64_encode(raw.as_bytes()), enc);
            assert_eq!(base64_decode(enc).unwrap(), raw.as_bytes());
        }
        let all: Vec<u8> = (0..=255).collect();
        assert_eq!(base64_decode(&base64_encode(&all)).unwrap(), all);
        assert!(base64_decode("Zm9v!").is_err());
        assert!(base64_decode("Z").is_err());
    }

    #[test]
    fn ids_sort_as_played() {
        let a = play_id(1_760_000_000_000.0, 0xffff_ffff);
        let b = play_id(1_760_000_000_001.0, 0);
        assert!(a < b, "{a} {b}");
        assert_eq!(play_id(5.0, 0xab), "000000000000005-000000ab");
    }

    #[test]
    fn an_entry_round_trips_as_json() {
        let e = PlayEntry {
            id: play_id(1.0, 2),
            summary: Some(r#"{"a":1}"#.into()),
            record: r#"{"b":[1,2]}"#.into(),
            log: vec![0, 1, 2, 250],
            timing: vec![9; 7],
        };
        let line = e.to_json();
        assert!(!line.contains('\n'));
        assert_eq!(PlayEntry::from_json(&line).unwrap(), e);
        assert!(PlayEntry::from_json("{}").is_err());
        // Texts that do not read are kept as strings; no summary is null.
        let odd = PlayEntry {
            summary: None,
            record: "{broken".into(),
            ..e.clone()
        };
        let line = odd.to_json();
        let v: Value = serde_json::from_str(&line).unwrap();
        assert_eq!(v["summary"], Value::Null);
        assert_eq!(v["record"], Value::String("{broken".into()));
        assert_eq!(PlayEntry::from_json(&line).unwrap(), odd);
    }

    #[test]
    fn the_memory_store_keeps_and_forgets() {
        let store = MemoryPlayStore::default();
        let got = Rc::new(RefCell::new(Vec::new()));
        for i in 0..3 {
            store.append(
                PlayEntry {
                    id: play_id(f64::from(i), 0),
                    summary: Some(format!("{i}")),
                    ..PlayEntry::default()
                },
                Box::new(|r| r.unwrap()),
            );
        }
        store.replace_summaries(
            vec![(play_id(1.0, 0), "x".into()), ("gone".into(), "y".into())],
            Box::new(|r| r.unwrap()),
        );
        store.remove(vec![play_id(0.0, 0)], Box::new(|r| r.unwrap()));
        let g = got.clone();
        store.summaries(Box::new(move |r| *g.borrow_mut() = r.unwrap()));
        assert_eq!(
            *got.borrow(),
            vec![
                (play_id(1.0, 0), Some("x".into())),
                (play_id(2.0, 0), Some("2".into()))
            ]
        );
        let g = got.clone();
        store.records(
            vec![play_id(2.0, 0), "gone".into()],
            Box::new(move |r| {
                *g.borrow_mut() = r.unwrap().into_iter().map(|e| (e.id, e.summary)).collect()
            }),
        );
        assert_eq!(*got.borrow(), vec![(play_id(2.0, 0), Some("2".into()))]);
        assert_eq!(store.len(), 2);
        // The export, a batch at a time.
        let batches = Rc::new(RefCell::new(Vec::new()));
        let b = batches.clone();
        let n = Rc::new(RefCell::new(None));
        let m = n.clone();
        store.export(
            1,
            Box::new(move |e| b.borrow_mut().push(e.len())),
            Box::new(move |r| *m.borrow_mut() = Some(r.unwrap())),
        );
        assert_eq!(*batches.borrow(), vec![1, 1]);
        assert_eq!(*n.borrow(), Some(2));
    }
}
