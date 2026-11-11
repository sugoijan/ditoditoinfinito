//! The history over a store: what the shells call. Memory answers at
//! once; the store is written behind it, and its answers come back
//! through callbacks (`ddi_platform::history::Reply`).

use std::cell::{Cell, Ref, RefCell};
use std::collections::HashSet;
use std::rc::Rc;

use ddi_chart::Song;
use ddi_engine::player::Results;

use super::{
    Asks, ChartDigest, History, Imported, Judged, PlayEntry, PlayLog, PlayRecord, PlayStore,
    PlaySummary, new_entry, rework,
};
use crate::state::settings::RuleChoice;

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
                me.history.borrow_mut().merge(History::from_summaries(list));
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

    /// Keeps imported plays ([`super::PlayImport::take`]'s) that are not
    /// kept yet: in memory at once, in the store behind (all of them
    /// taken out of both if it fails). Answers with how many were new,
    /// and how many were kept already.
    pub fn import(
        self: &Rc<Self>,
        plays: Vec<Imported>,
        done: impl FnOnce(Result<(usize, usize), String>) + 'static,
    ) {
        if !self.loaded() {
            return done(Err("the play history is not read yet".into()));
        }
        let (entries, kept) = self.add_new(
            plays
                .into_iter()
                .map(|p| (p.entry.id.clone(), p.summary, p.entry)),
        );
        if entries.is_empty() {
            return done(Ok((0, kept)));
        }
        let ids: Vec<String> = entries.iter().map(|e| e.id.clone()).collect();
        let me = self.clone();
        self.store.append_all(
            entries,
            Box::new(move |r| match r {
                Ok(()) => done(Ok((ids.len(), kept))),
                Err(e) => {
                    // None of them, here or in the store.
                    me.remove(ids, move |_| done(Err(e)));
                }
            }),
        );
    }

    /// Plays a shell kept in the store itself (desktop imports on a
    /// thread of its own) into memory; those kept already are left. How
    /// many were new.
    pub fn adopt(&self, plays: Vec<(String, PlaySummary)>) -> usize {
        let (added, _) = self.add_new(plays.into_iter().map(|(id, s)| (id, s, ())));
        added.len()
    }

    /// Plays `(id, summary, what goes with it)` not kept yet, into memory
    /// (said changed when any was): what went with those, and how many
    /// were kept already.
    fn add_new<T>(
        &self,
        plays: impl IntoIterator<Item = (String, PlaySummary, T)>,
    ) -> (Vec<T>, usize) {
        let mut added = Vec::new();
        let mut kept = 0;
        {
            let mut h = self.history.borrow_mut();
            for (id, summary, with) in plays {
                if h.contains(&id) {
                    kept += 1;
                } else {
                    h.add(id, summary);
                    added.push(with);
                }
            }
        }
        if !added.is_empty() {
            self.changed();
        }
        (added, kept)
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
        let ids = self.stale_batch(song_id, max);
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
                let asks = me.asks(&ids);
                let changes = me.set_worked(rework_entries(&ids, &entries, &song, &asks));
                let n = changes.len();
                me.store
                    .replace_summaries(changes, Box::new(move |r| done(r.map(|_| n))));
            }),
        );
    }

    /// The next up to `max` stale plays of `song_id` to work out again
    /// (oldest first; none tried this session and left unmatched): what
    /// [`Plays::rework_song`] takes, for a shell that judges them
    /// elsewhere ([`rework_entries`] on a thread, then
    /// [`Plays::set_worked`]).
    pub fn stale_batch(&self, song_id: &str, max: usize) -> Vec<String> {
        self.history
            .borrow()
            .iter()
            .filter(|(id, s)| s.chart.song == song_id && self.to_work_out(id, s))
            .take(max)
            .map(|(id, _)| id.clone())
            .collect()
    }

    /// What judging plays `ids` again takes from their summaries (the
    /// other rules each waits for or was judged again under), for
    /// [`rework_entries`].
    pub fn asks(&self, ids: &[String]) -> Asks {
        let h = self.history.borrow();
        ids.iter()
            .filter_map(|id| Some((id.clone(), h.get(id)?.ask())))
            .collect()
    }

    /// Every kept play (those played under ruleset `only`, a preset's
    /// id, when given) is to be judged again under `rules`
    /// (`docs/plans/ddi-ruleset.md`, "Re-judging"): marked as waiting, in
    /// memory at once and in the store behind, for the shell's
    /// [`super::Reworker`] to judge as it does stale plays. What each came
    /// to as played stays ([`PlaySummary::worked`]). How many wait.
    pub fn rejudge(
        self: &Rc<Self>,
        rules: &RuleChoice,
        only: Option<&str>,
        done: impl FnOnce(Result<(), String>) + 'static,
    ) -> usize {
        let changes: Vec<(String, String)> = self
            .history
            .borrow_mut()
            .ask_rejudge(rules, only)
            .into_iter()
            .filter_map(|(id, s)| Some((id, serde_json::to_string(s).ok()?)))
            .collect();
        let n = changes.len();
        if n == 0 {
            done(Ok(()));
            return 0;
        }
        // Plays left unmatched this session are tried again.
        self.unmatched.borrow_mut().clear();
        self.changed();
        self.store.replace_summaries(changes, Box::new(done));
        n
    }

    /// What the settings' move from the old default ruleset asked for
    /// (`Settings::rejudge_from`), once the summaries are in: the kept
    /// plays under that ruleset wait to be judged again under the rules
    /// now chosen, and the settings ask no more once that is stored. How
    /// many wait.
    pub fn rejudge_asked(self: &Rc<Self>) -> usize {
        use crate::state::settings::Settings;
        if !self.loaded() {
            return 0;
        }
        let settings = Settings::load();
        let Some(from) = settings.rejudge_from.clone() else {
            return 0;
        };
        // Asked no more once the marks are stored: a session that ends
        // before then asks again in the next.
        self.rejudge(&settings.rules(), Some(&from), |stored| {
            if stored.is_ok() {
                let mut settings = Settings::load();
                if settings.rejudge_from.take().is_some() {
                    settings.save();
                }
            }
        })
    }

    /// Whether a play is stale and has not been tried this session.
    fn to_work_out(&self, id: &str, s: &PlaySummary) -> bool {
        s.is_stale() && !self.unmatched.borrow().contains(id)
    }

    /// Whether `song_id` has a play still to work out this session.
    pub fn has_stale_of(&self, song_id: &str) -> bool {
        self.history
            .borrow()
            .iter()
            .any(|(id, s)| s.chart.song == song_id && self.to_work_out(id, s))
    }

    /// The first song, by id, with a play still to work out this session,
    /// the songs `skip` leaves out aside.
    pub fn next_stale_song(&self, skip: impl Fn(&str) -> bool) -> Option<String> {
        self.history
            .borrow()
            .iter()
            .filter(|(id, s)| self.to_work_out(id, s) && !skip(&s.chart.song))
            .map(|(_, s)| s.chart.song.as_str())
            .min()
            .map(str::to_owned)
    }

    /// Plays worked out again ([`rework_entries`]'s answer) into memory;
    /// those that could not be (their chart changed or is gone, or the
    /// play went meanwhile) are left unmatched for the session. The new
    /// summaries, as text, for the store's `replace_summaries`.
    pub fn set_worked(&self, worked: Reworked) -> Vec<(String, String)> {
        let mut changes = Vec::new();
        for (id, worked) in worked {
            let summary = match worked {
                Ok(w) => self.history.borrow_mut().set_worked(&id, w).cloned(),
                Err(_) => None,
            };
            match summary.map(|s| serde_json::to_string(&s)) {
                Some(Ok(json)) => changes.push((id, json)),
                _ => {
                    self.unmatched.borrow_mut().insert(id);
                }
            }
        }
        self.changed();
        changes
    }

    /// The songs with stale plays still to work out this session.
    #[cfg(test)]
    pub fn stale_songs(&self) -> Vec<String> {
        let unmatched = self.unmatched.borrow();
        let songs: std::collections::BTreeSet<String> = self
            .history
            .borrow()
            .iter()
            .filter(|(id, s)| s.is_stale() && !unmatched.contains(*id))
            .map(|(_, s)| s.chart.song.clone())
            .collect();
        songs.into_iter().collect()
    }

    fn remove(self: &Rc<Self>, ids: Vec<String>, done: impl FnOnce(Result<(), String>) + 'static) {
        self.history.borrow_mut().remove(&ids);
        self.changed();
        self.store.remove(ids, Box::new(done));
    }

    /// A play as stored (its record, log and timing), to replay it; or
    /// why it cannot be had.
    pub fn entry(&self, id: &str, done: impl FnOnce(Result<PlayEntry, super::Refusal>) + 'static) {
        let id = id.to_string();
        self.store.records(
            vec![id.clone()],
            Box::new(move |r| {
                done(r.map_err(super::Refusal::History).and_then(|entries| {
                    entries
                        .into_iter()
                        .find(|e| e.id == id)
                        .ok_or(super::Refusal::Gone)
                }))
            }),
        );
    }

    /// Forgets one play.
    pub fn remove_play(self: &Rc<Self>, id: &str, done: impl FnOnce(Result<(), String>) + 'static) {
        self.remove(vec![id.to_string()], done)
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

/// Plays `ids` judged again on `song` from their stored `entries` (as a
/// store's `records` answers: one missing is "not stored"), in `ids`'
/// order, each also under the other rules `asks` names for it
/// ([`Plays::asks`]). Pure: a shell may run it off its UI thread.
pub fn rework_entries(ids: &[String], entries: &[PlayEntry], song: &Song, asks: &Asks) -> Reworked {
    let none = super::Ask::default();
    ids.iter()
        .map(|id| {
            let worked = entries
                .iter()
                .find(|e| &e.id == id)
                .ok_or_else(|| "not stored".to_string())
                .and_then(|e| rework(e, song, asks.get(id).unwrap_or(&none)));
            (id.clone(), worked)
        })
        .collect()
}

/// Plays judged again: each id with what it comes to, or why it could
/// not be.
pub type Reworked = Vec<(String, Result<Judged, String>)>;

/// One play a line, everything in it (`PlayEntry::to_json`).
pub fn export_lines(entries: &[PlayEntry]) -> String {
    let mut out = String::new();
    for e in entries {
        out.push_str(&e.to_json());
        out.push('\n');
    }
    out
}
