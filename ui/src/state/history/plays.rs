//! The history over a store: what the shells call. Memory answers at
//! once; the store is written behind it, and its answers come back
//! through callbacks (`ddi_platform::history::Reply`).

use std::cell::{Cell, Ref, RefCell};
use std::collections::HashSet;
use std::rc::Rc;

use ddi_chart::Song;
use ddi_engine::player::Results;

use super::{
    ChartDigest, History, PlayEntry, PlayLog, PlayRecord, PlayStore, PlaySummary, new_entry, rework,
};

/// The plays kept, in memory and in a store.
pub struct Plays {
    history: RefCell<History>,
    store: Rc<dyn PlayStore>,
    loaded: Cell<bool>,
    /// Plays that could not be worked out again this session (their chart
    /// changed or is gone): not tried again until the next start.
    unmatched: RefCell<HashSet<String>>,
    /// Bumped on every change, for views to notice.
    generation: Cell<u64>,
}

impl Plays {
    pub fn new(store: Rc<dyn PlayStore>) -> Rc<Plays> {
        Rc::new(Plays {
            history: RefCell::new(History::default()),
            store,
            loaded: Cell::new(false),
            unmatched: RefCell::new(HashSet::new()),
            generation: Cell::new(0),
        })
    }

    pub fn history(&self) -> Ref<'_, History> {
        self.history.borrow()
    }

    /// The summaries are in (or could not be read).
    pub fn loaded(&self) -> bool {
        self.loaded.get()
    }

    pub fn generation(&self) -> u64 {
        self.generation.get()
    }

    fn changed(&self) {
        self.generation.set(self.generation.get() + 1);
    }

    /// Reads the summaries from the store, adding them to what was
    /// recorded meanwhile; then rebuilds the summaries that are missing or
    /// do not read from their records (stale, to be worked out again).
    pub fn load(self: &Rc<Self>, done: impl FnOnce(Result<(), String>) + 'static) {
        let me = self.clone();
        self.store.summaries(Box::new(move |r| match r {
            Ok(list) => {
                me.loaded.set(true);
                let loaded = History::from_summaries(list);
                let mut h = me.history.borrow_mut();
                for (id, s) in loaded.iter() {
                    h.add(id.clone(), s.clone());
                }
                for id in loaded.unreadable() {
                    h.mark_unreadable(id.clone());
                }
                drop(h);
                me.changed();
                me.recover(done);
            }
            Err(e) => done(Err(e)),
        }));
    }

    /// Summaries from the records of the plays whose summary is missing or
    /// does not read; plays whose record does not read either stay listed
    /// as unreadable.
    fn recover(self: &Rc<Self>, done: impl FnOnce(Result<(), String>) + 'static) {
        let ids = self.history.borrow().unreadable().to_vec();
        if ids.is_empty() {
            return done(Ok(()));
        }
        let me = self.clone();
        self.store.records(
            ids,
            Box::new(move |r| {
                let entries = match r {
                    Ok(e) => e,
                    Err(e) => return done(Err(e)),
                };
                let mut changes = Vec::new();
                for e in entries {
                    if let Ok(s) = PlaySummary::from_record(&e.record)
                        && let Ok(json) = serde_json::to_string(&s)
                    {
                        me.history.borrow_mut().add(e.id.clone(), s);
                        changes.push((e.id, json));
                    }
                }
                me.changed();
                me.store.replace_summaries(changes, Box::new(done));
            }),
        );
    }

    /// Keeps a play: in memory at once, in the store behind. Its summary.
    pub fn record(
        self: &Rc<Self>,
        id: String,
        record: &PlayRecord,
        log: &PlayLog,
        results: &Results,
        done: impl FnOnce(Result<(), String>) + 'static,
    ) -> Result<PlaySummary, String> {
        let (entry, summary) = new_entry(id.clone(), record, log, results)?;
        self.history.borrow_mut().add(id.clone(), summary.clone());
        self.changed();
        let me = self.clone();
        // Not kept: not in memory either.
        self.store.append(
            entry,
            Box::new(move |r| {
                if r.is_err() {
                    me.history.borrow_mut().remove(&[id]);
                    me.changed();
                }
                done(r)
            }),
        );
        Ok(summary)
    }

    /// Works out again up to `max` stale plays of `song_id` on `song` (as
    /// stored): reads their records, judges them again, and writes the new
    /// summaries. How many were worked out.
    pub fn rework_song(
        self: &Rc<Self>,
        song_id: &str,
        song: Rc<Song>,
        max: usize,
        done: impl FnOnce(Result<usize, String>) + 'static,
    ) {
        let ids: Vec<String> = {
            let unmatched = self.unmatched.borrow();
            self.history
                .borrow()
                .stale()
                .remove(song_id)
                .unwrap_or_default()
                .into_iter()
                .filter(|id| !unmatched.contains(id))
                .take(max)
                .collect()
        };
        if ids.is_empty() {
            return done(Ok(0));
        }
        let me = self.clone();
        self.store.records(
            ids.clone(),
            Box::new(move |r| {
                let entries = match r {
                    Ok(e) => e,
                    Err(e) => return done(Err(e)),
                };
                let mut changes = Vec::new();
                for id in &ids {
                    let worked = entries
                        .iter()
                        .find(|e| &e.id == id)
                        .ok_or_else(|| "not stored".to_string())
                        .and_then(|e| rework(e, &song));
                    let summary = match worked {
                        Ok(w) => me.history.borrow_mut().set_worked(id, w).cloned(),
                        Err(_) => None,
                    };
                    match summary.map(|s| serde_json::to_string(&s)) {
                        Some(Ok(json)) => changes.push((id.clone(), json)),
                        _ => {
                            me.unmatched.borrow_mut().insert(id.clone());
                        }
                    }
                }
                let n = changes.len();
                me.changed();
                me.store
                    .replace_summaries(changes, Box::new(move |r| done(r.map(|_| n))));
            }),
        );
    }

    /// The songs with stale plays still to work out this session.
    pub fn stale_songs(&self) -> Vec<String> {
        let unmatched = self.unmatched.borrow();
        self.history
            .borrow()
            .stale()
            .into_iter()
            .filter(|(_, ids)| ids.iter().any(|id| !unmatched.contains(id)))
            .map(|(song, _)| song)
            .collect()
    }

    fn remove(self: &Rc<Self>, ids: Vec<String>, done: impl FnOnce(Result<(), String>) + 'static) {
        self.history.borrow_mut().remove(&ids);
        self.changed();
        self.store.remove(ids, Box::new(done));
    }

    /// Forgets every play.
    pub fn remove_all(self: &Rc<Self>, done: impl FnOnce(Result<(), String>) + 'static) {
        let ids = self.history.borrow().ids();
        self.remove(ids, done)
    }

    /// Forgets a chart's plays (by its content, under any song id).
    pub fn remove_chart(
        self: &Rc<Self>,
        digest: ChartDigest,
        done: impl FnOnce(Result<(), String>) + 'static,
    ) {
        let ids = self.history.borrow().ids_of_chart(digest);
        self.remove(ids, done)
    }

    /// Forgets the plays of these songs (a pack's).
    pub fn remove_songs(
        self: &Rc<Self>,
        songs: &[String],
        done: impl FnOnce(Result<(), String>) + 'static,
    ) {
        let ids = self.history.borrow().ids_of_songs(songs);
        self.remove(ids, done)
    }

    /// Every play kept, as the export's JSON lines (`PlayEntry::to_json`),
    /// a chunk of about `batch` plays at a time to `each`; `done` answers
    /// with how many.
    pub fn export(
        &self,
        batch: usize,
        mut each: impl FnMut(String) + 'static,
        done: impl FnOnce(Result<usize, String>) + 'static,
    ) {
        self.store.export(
            batch,
            Box::new(move |entries: Vec<PlayEntry>| each(export_lines(&entries))),
            Box::new(done),
        );
    }
}

/// One play a line, everything in it (`PlayEntry::to_json`).
pub fn export_lines(entries: &[PlayEntry]) -> String {
    let mut out = String::new();
    for e in entries {
        out.push_str(&e.to_json());
        out.push('\n');
    }
    out
}
