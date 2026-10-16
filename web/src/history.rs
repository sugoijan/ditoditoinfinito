//! The play history on the web (`docs/plans/play-history.md`, step 5):
//! the plays in IndexedDB ([`IdbPlayStore`]: the stores `plays` and
//! `play_summaries` of `ddi-library`), the model over them
//! (`ddi_ui::state::history::Plays`, one for the page), the summaries
//! read at start, plays recorded as they end, stale summaries worked out
//! again (a song's when its chart loads, the rest in the background from
//! their charts alone, a few at a time and never during a play), the
//! export, and `window.__DDI_DEBUG.history`.

use std::cell::{Cell, RefCell};
use std::collections::HashSet;
use std::rc::Rc;

use ddi_chart::{ChartDigest, Song};
use ddi_platform::history::{PlayEntry, PlayStore, Reply};
use ddi_ui::state::history::{Plays, score_value};
use js_sys::{Array, Object, Reflect, Uint8Array};
use wasm_bindgen::{JsCast, JsValue};
use wasm_bindgen_futures::spawn_local;

use crate::web::idb::{self, Db, Write};

/// Plays worked out again in one step (one replay each: a long chart's
/// takes about a quarter of a millisecond).
const REWORK_BATCH: usize = 8;
/// Time between steps for a song whose chart is at hand, ms (a yield).
const FOCUS_STEP_MS: u32 = 16;
/// Time between the background pass's steps, ms.
const PASS_STEP_MS: u32 = 1500;
/// Plays read and written out at a time by the export.
const EXPORT_BATCH: usize = 100;
/// The first wait before reading the summaries again after a failure,
/// ms (doubled each time, up to a minute).
const RETRY_MS: u32 = 2000;

/// The plays in the song library's database.
pub(crate) struct IdbPlayStore;

fn raw_value(e: &PlayEntry) -> JsValue {
    let o = Object::new();
    let _ = Reflect::set(&o, &"record".into(), &JsValue::from_str(&e.record));
    let _ = Reflect::set(&o, &"log".into(), &Uint8Array::from(&e.log[..]).into());
    let _ = Reflect::set(
        &o,
        &"timing".into(),
        &Uint8Array::from(&e.timing[..]).into(),
    );
    o.into()
}

/// A stored play, whatever shape it is in: what is not as written is
/// kept as text (the record), never dropped.
fn from_raw(id: String, summary: Option<String>, raw: &JsValue) -> PlayEntry {
    let get = |k: &str| Reflect::get(raw, &k.into()).ok();
    let bytes = |k: &str| {
        get(k)
            .and_then(|v| v.dyn_into::<Uint8Array>().ok())
            .map(|a| a.to_vec())
            .unwrap_or_default()
    };
    let record = get("record")
        .and_then(|r| r.as_string())
        .unwrap_or_else(|| {
            js_sys::JSON::stringify(raw)
                .ok()
                .and_then(|s| s.as_string())
                .unwrap_or_default()
        });
    PlayEntry {
        id,
        summary,
        record,
        log: bytes("log"),
        timing: bytes("timing"),
    }
}

async fn append(e: PlayEntry) -> Result<(), String> {
    let db = Db::open().await?;
    let mut writes = vec![Write::Put {
        store: idb::PLAYS,
        key: e.id.clone(),
        value: raw_value(&e),
    }];
    if let Some(summary) = &e.summary {
        writes.push(Write::Put {
            store: idb::PLAY_SUMMARIES,
            key: e.id.clone(),
            value: JsValue::from_str(summary),
        });
    }
    db.write(writes).await
}

/// Every play's summary, and the plays with none (`None`, as a summary
/// that is not text).
async fn summaries() -> Result<Vec<(String, Option<String>)>, String> {
    let db = Db::open().await?;
    let mut all: std::collections::BTreeMap<String, Option<String>> = db
        .entries(idb::PLAY_SUMMARIES)
        .await?
        .into_iter()
        .map(|(id, v)| (id, v.as_string()))
        .collect();
    for id in db.keys_with_prefix(idb::PLAYS, "").await? {
        all.entry(id).or_insert(None);
    }
    Ok(all.into_iter().collect())
}

async fn records(ids: Vec<String>) -> Result<Vec<PlayEntry>, String> {
    let db = Db::open().await?;
    let raws = db.get_many(idb::PLAYS, &ids).await?;
    let sums = db.get_many(idb::PLAY_SUMMARIES, &ids).await?;
    Ok(ids
        .into_iter()
        .zip(raws.into_iter().zip(sums))
        .filter_map(|(id, (raw, sum))| {
            let summary = sum.and_then(|s| s.as_string());
            Some(from_raw(id, summary, &raw?))
        })
        .collect())
}

async fn replace_summaries(changes: Vec<(String, String)>) -> Result<(), String> {
    let db = Db::open().await?;
    let ids: Vec<String> = changes.iter().map(|(id, _)| id.clone()).collect();
    // Only plays still kept (one removed meanwhile stays removed).
    let kept = db.get_many(idb::PLAYS, &ids).await?;
    let writes = changes
        .into_iter()
        .zip(kept)
        .filter(|(_, k)| k.is_some())
        .map(|((id, summary), _)| Write::Put {
            store: idb::PLAY_SUMMARIES,
            key: id,
            value: JsValue::from_str(&summary),
        })
        .collect();
    db.write(writes).await
}

async fn remove(ids: Vec<String>) -> Result<(), String> {
    let db = Db::open().await?;
    db.write(
        ids.into_iter()
            .flat_map(|id| {
                [
                    Write::Delete {
                        store: idb::PLAYS,
                        key: id.clone(),
                    },
                    Write::Delete {
                        store: idb::PLAY_SUMMARIES,
                        key: id,
                    },
                ]
            })
            .collect(),
    )
    .await
}

/// Every play, a batch at a time (each batch its own transaction): the
/// whole history is never in memory at once, and the page gets its turn
/// between batches.
async fn export_batches(
    batch: usize,
    mut each: Box<dyn FnMut(Vec<PlayEntry>)>,
) -> Result<usize, String> {
    let db = Db::open().await?;
    let mut after: Option<String> = None;
    let mut n = 0;
    loop {
        let raws = db
            .entries_after(idb::PLAYS, after.as_deref(), batch as u32)
            .await?;
        let Some((last, _)) = raws.last() else {
            break;
        };
        after = Some(last.clone());
        let ids: Vec<String> = raws.iter().map(|(id, _)| id.clone()).collect();
        let sums = db.get_many(idb::PLAY_SUMMARIES, &ids).await?;
        let entries: Vec<PlayEntry> = raws
            .into_iter()
            .zip(sums)
            .map(|((id, raw), sum)| from_raw(id, sum.and_then(|s| s.as_string()), &raw))
            .collect();
        n += entries.len();
        each(entries);
    }
    Ok(n)
}

fn answer<T: 'static>(
    done: Reply<T>,
    f: impl std::future::Future<Output = Result<T, String>> + 'static,
) {
    spawn_local(async move { done(f.await) });
}

impl PlayStore for IdbPlayStore {
    fn append(&self, entry: PlayEntry, done: Reply<()>) {
        answer(done, append(entry));
    }

    fn summaries(&self, done: Reply<Vec<(String, Option<String>)>>) {
        answer(done, summaries());
    }

    fn records(&self, ids: Vec<String>, done: Reply<Vec<PlayEntry>>) {
        if ids.is_empty() {
            return done(Ok(Vec::new()));
        }
        answer(done, records(ids));
    }

    fn replace_summaries(&self, summaries: Vec<(String, String)>, done: Reply<()>) {
        if summaries.is_empty() {
            return done(Ok(()));
        }
        answer(done, replace_summaries(summaries));
    }

    fn remove(&self, ids: Vec<String>, done: Reply<()>) {
        if ids.is_empty() {
            return done(Ok(()));
        }
        answer(done, remove(ids));
    }

    fn export(&self, batch: usize, each: Box<dyn FnMut(Vec<PlayEntry>)>, done: Reply<usize>) {
        answer(done, export_batches(batch, each));
    }
}

thread_local! {
    static PLAYS: RefCell<Option<Rc<Plays>>> = const { RefCell::new(None) };
    /// Why the summaries could not be read, if they could not (they are
    /// read again a while later).
    static LOAD_ERROR: RefCell<Option<String>> = const { RefCell::new(None) };
    static RETRY: RefCell<Option<gloo::timers::callback::Timeout>> = const { RefCell::new(None) };
    static WATCH: RefCell<Option<gloo::timers::callback::Interval>> = const { RefCell::new(None) };
    /// A song is being played: re-working waits.
    static PLAYING: Cell<bool> = const { Cell::new(false) };
    /// The song the play screen is on, and its charts' digests once its
    /// chart is loaded (the debug state's bests).
    static CURRENT: RefCell<Option<(String, Vec<ChartDigest>)>> = const { RefCell::new(None) };
    /// The song whose chart is at hand: its stale plays come first.
    static FOCUS: RefCell<Option<(String, Rc<Song>)>> = const { RefCell::new(None) };
    /// The background pass's song and chart, read once for its steps.
    static BACKGROUND: RefCell<Option<(String, Rc<Song>)>> = const { RefCell::new(None) };
    /// Songs whose chart could not be read this session (a kept folder
    /// asking for permission, a removed song): the pass leaves them.
    static SKIPPED: RefCell<HashSet<String>> = RefCell::new(HashSet::new());
    static STEP: RefCell<Option<gloo::timers::callback::Timeout>> = const { RefCell::new(None) };
    static BUSY: Cell<bool> = const { Cell::new(false) };
}

/// The page's plays.
pub(crate) fn plays() -> Rc<Plays> {
    PLAYS.with(|p| {
        p.borrow_mut()
            .get_or_insert_with(|| Plays::new(Rc::new(IdbPlayStore)))
            .clone()
    })
}

/// Reads the summaries (at start), then works out stale ones in the
/// background. A failure (the library unavailable) is tried again later.
pub(crate) fn start() {
    load(RETRY_MS);
}

fn load(next_wait: u32) {
    // While the library is not open (another tab holding it), the debug
    // state says so.
    let watch = gloo::timers::callback::Interval::new(1000, publish);
    WATCH.with(|w| *w.borrow_mut() = Some(watch));
    plays().load(move |r| {
        WATCH.with(|w| w.borrow_mut().take());
        match r {
            Ok(()) => {
                LOAD_ERROR.with(|l| l.borrow_mut().take());
                schedule(PASS_STEP_MS);
            }
            Err(e) => {
                web_sys::console::warn_1(&format!("play history: {e}").into());
                LOAD_ERROR.with(|l| *l.borrow_mut() = Some(e));
                let t = gloo::timers::callback::Timeout::new(next_wait, move || {
                    load((next_wait * 2).min(60_000))
                });
                RETRY.with(|r| *r.borrow_mut() = Some(t));
            }
        }
        publish();
    });
}

/// A song's play is on (re-working waits) or over (it goes on).
pub(crate) fn set_playing(on: bool) {
    PLAYING.with(|p| p.set(on));
    if !on {
        schedule(FOCUS_STEP_MS);
    }
}

/// The song the play screen is on, for the debug state's bests.
pub(crate) fn set_current(song: Option<&str>) {
    CURRENT.with(|c| *c.borrow_mut() = song.map(|s| (s.to_string(), Vec::new())));
    if song.is_none() {
        FOCUS.with(|f| f.borrow_mut().take());
    }
    publish();
}

/// A song's chart is at hand: its stale plays are worked out first, a
/// few at a time (until its play starts).
pub(crate) fn chart_loaded(id: &str, song: &Song) {
    let digests = (0..song.charts.len())
        .map(|i| song.chart_digest(i))
        .collect();
    CURRENT.with(|c| *c.borrow_mut() = Some((id.to_string(), digests)));
    if plays().history().stale().contains_key(id) {
        FOCUS.with(|f| *f.borrow_mut() = Some((id.to_string(), Rc::new(song.clone()))));
        schedule(0);
    }
    publish();
}

/// The next re-working step in `ms` (sooner replaces later).
fn schedule(ms: u32) {
    let t = gloo::timers::callback::Timeout::new(ms, step);
    STEP.with(|s| *s.borrow_mut() = Some(t));
}

/// Works out a few stale plays: the focused song's, else a song's in the
/// background (its chart read alone). Never during a play; stops when
/// nothing is stale.
fn step() {
    STEP.with(|s| s.borrow_mut().take());
    if PLAYING.with(Cell::get) || BUSY.with(Cell::get) || !plays().loaded() {
        return;
    }
    let stale = plays().stale_songs();
    let focus = FOCUS.with(|f| f.borrow().clone());
    if let Some((id, song)) = focus {
        if stale.contains(&id) {
            return rework(id, song, FOCUS_STEP_MS);
        }
        FOCUS.with(|f| f.borrow_mut().take());
    }
    let skipped = SKIPPED.with(|s| s.borrow().clone());
    let Some(id) = stale.into_iter().find(|s| !skipped.contains(s)) else {
        BACKGROUND.with(|b| b.borrow_mut().take());
        return;
    };
    if let Some(song) = BACKGROUND.with(|b| {
        b.borrow()
            .as_ref()
            .filter(|(i, _)| *i == id)
            .map(|(_, s)| s.clone())
    }) {
        return rework(id, song, PASS_STEP_MS);
    }
    // Where the song is gone (a pack imported again from another folder),
    // its charts are looked for by content.
    let digests: Vec<ChartDigest> = {
        let p = plays();
        let h = p.history();
        let ids = h.stale().remove(&id).unwrap_or_default();
        ids.iter()
            .filter_map(|p| h.get(p).map(|s| s.chart.digest))
            .collect()
    };
    BUSY.with(|b| b.set(true));
    spawn_local(async move {
        let mut loaded = crate::songs::load_chart(&id).await;
        if loaded.is_err()
            && let Some(other) = crate::songs::find_by_digest(&digests).await
        {
            loaded = crate::songs::load_chart(&other).await;
        }
        BUSY.with(|b| b.set(false));
        match loaded {
            Ok(song) => {
                let song = Rc::new(song);
                BACKGROUND.with(|b| *b.borrow_mut() = Some((id, song)));
                // A play may have started meanwhile: `step` looks again.
                schedule(0);
            }
            Err(_) => {
                SKIPPED.with(|s| s.borrow_mut().insert(id));
                schedule(PASS_STEP_MS);
            }
        }
    });
}

fn rework(id: String, song: Rc<Song>, next: u32) {
    BUSY.with(|b| b.set(true));
    plays().rework_song(&id, song, REWORK_BATCH, move |r| {
        BUSY.with(|b| b.set(false));
        if let Err(e) = r {
            web_sys::console::warn_1(&format!("play history: {e}").into());
        }
        publish();
        schedule(next);
    });
}

/// Keeps a play that ended (finished or failed); `done` hears whether it
/// was kept (a play the library could not keep is not kept in memory
/// either).
pub(crate) fn record(
    record: &ddi_ui::state::history::PlayRecord,
    log: &ddi_ui::state::history::PlayLog,
    results: &ddi_engine::Results,
    done: impl FnOnce(Result<(), String>) + 'static,
) {
    let random = (js_sys::Math::random() * 4_294_967_296.0) as u32;
    let id = ddi_platform::history::play_id(record.time, random);
    let r = plays().record(id, record, log, results, move |r| {
        if let Err(e) = &r {
            web_sys::console::warn_1(&format!("play history: keeping the play: {e}").into());
        }
        publish();
        done(r)
    });
    if let Err(e) = r {
        web_sys::console::warn_1(&format!("play history: {e}").into());
    }
    publish();
}

/// Downloads every play kept, one JSON object a line
/// (`ddi_ui::state::history::export_lines`), built a batch at a time into
/// the file's parts. Answers with how many.
pub(crate) fn export(done: impl FnOnce(Result<usize, String>) + 'static) {
    let parts = Array::new();
    let p = parts.clone();
    plays().export(
        EXPORT_BATCH,
        move |chunk| {
            // Each batch leaves wasm memory at once, as its own blob.
            if let Ok(blob) = web_sys::Blob::new_with_str_sequence(&Array::of1(&chunk.into())) {
                p.push(&blob);
            }
        },
        move |r| {
            let r = r.and_then(|count| {
                let options = web_sys::BlobPropertyBag::new();
                options.set_type("application/x-ndjson");
                let blob = web_sys::Blob::new_with_blob_sequence_and_options(&parts, &options)
                    .map_err(|_| "no file")?;
                let url = crate::web::files::object_url(&blob).ok_or("no object URL")?;
                let date = js_sys::Date::new_0();
                let name = format!(
                    "ditoditoinfinito-plays-{:04}-{:02}-{:02}.jsonl",
                    date.get_full_year(),
                    date.get_month() + 1,
                    date.get_date()
                );
                let document = web_sys::window()
                    .and_then(|w| w.document())
                    .ok_or("no document")?;
                let a: web_sys::HtmlElement = document
                    .create_element("a")
                    .map_err(|_| "no link")?
                    .unchecked_into();
                let _ = a.set_attribute("href", &url);
                let _ = a.set_attribute("download", &name);
                a.click();
                // After the download has started.
                gloo::timers::callback::Timeout::new(10_000, move || {
                    crate::web::files::revoke_object_url(&url)
                })
                .forget();
                Ok(count)
            });
            done(r)
        },
    );
}

/// Forgets every play; a chart's (by content); a pack's songs'. No
/// screen asks yet.
#[allow(dead_code)]
pub(crate) fn remove_all(done: impl FnOnce(Result<(), String>) + 'static) {
    plays().remove_all(move |r| {
        publish();
        done(r)
    });
}

#[allow(dead_code)]
pub(crate) fn remove_chart(digest: ChartDigest, done: impl FnOnce(Result<(), String>) + 'static) {
    plays().remove_chart(digest, move |r| {
        publish();
        done(r)
    });
}

#[allow(dead_code)]
pub(crate) fn remove_songs(songs: &[String], done: impl FnOnce(Result<(), String>) + 'static) {
    plays().remove_songs(songs, move |r| {
        publish();
        done(r)
    });
}

fn set(o: &Object, k: &str, v: JsValue) {
    let _ = Reflect::set(o, &JsValue::from_str(k), &v);
}

fn json(value: &impl serde::Serialize) -> JsValue {
    serde_json::to_string(value)
        .ok()
        .and_then(|s| js_sys::JSON::parse(&s).ok())
        .unwrap_or(JsValue::NULL)
}

/// `__DDI_DEBUG.history`: `count`, `loaded`, `error`, `blocked` (the
/// library waits for another tab), `stale` (plays waiting to be worked
/// out again), `unreadable`, `last` (the latest play's summary with its
/// `id`), `song` and `bests` (the play screen's song's charts, once its
/// chart is loaded: digest, ruleset, assist, score, its value, grade,
/// lamp, plays, lastPlayed, pending).
pub(crate) fn debug_into(obj: &Object) {
    let Some(p) = PLAYS.with(|p| p.borrow().clone()) else {
        return;
    };
    let h = p.history();
    let o = Object::new();
    set(&o, "count", (h.len() as f64).into());
    set(&o, "loaded", p.loaded().into());
    if let Some(e) = LOAD_ERROR.with(|l| l.borrow().clone()) {
        set(&o, "error", e.into());
    }
    set(&o, "blocked", idb::blocked().into());
    set(&o, "stale", (h.stale_count() as f64).into());
    set(&o, "unreadable", (h.unreadable().len() as f64).into());
    if let Some((id, s)) = h.last() {
        let last = json(s);
        if last.is_object() {
            set(&Object::from(last.clone()), "id", id.as_str().into());
        }
        set(&o, "last", last);
    }
    let bests = Array::new();
    if let Some((song, digests)) = CURRENT.with(|c| c.borrow().clone())
        && !digests.is_empty()
    {
        set(&o, "song", song.as_str().into());
        let d = Array::new();
        for x in &digests {
            d.push(&x.to_string().into());
        }
        set(&o, "digests", d.into());
        let mut all: Vec<_> = h.bests_of_charts(&digests).into_iter().collect();
        all.sort_by_key(|(k, _)| (k.digest.to_string(), k.ruleset.id.clone(), k.assist));
        for (k, b) in all {
            let e = Object::new();
            set(&e, "digest", k.digest.to_string().into());
            set(&e, "ruleset", json(&k.ruleset));
            set(&e, "assist", k.assist.into());
            set(&e, "score", json(&b.score));
            set(&e, "value", score_value(&b.score).into());
            set(&e, "grade", b.grade.as_str().into());
            set(&e, "lamp", json(&b.lamp));
            set(&e, "plays", b.plays.into());
            set(&e, "lastPlayed", b.last_played.into());
            set(&e, "pending", b.pending.into());
            bests.push(&e);
        }
    }
    set(&o, "bests", bests.into());
    set(obj, "history", o.into());
}

thread_local! {
    /// The page shown that follows the history (song select's bests).
    static WATCHER: RefCell<Option<Rc<dyn Fn()>>> = const { RefCell::new(None) };
}

/// Follows the history's changes (`None`: no more).
pub(crate) fn watch(watcher: Option<Rc<dyn Fn()>>) {
    WATCHER.with(|w| *w.borrow_mut() = watcher);
}

/// Writes `__DDI_DEBUG.history` now (pages that do not draw keep it too),
/// and tells the page that follows.
pub(crate) fn publish() {
    if let Some(w) = WATCHER.with(|w| w.borrow().clone()) {
        w();
    }
    let Some(window) = web_sys::window() else {
        return;
    };
    let obj = Reflect::get(&window, &"__DDI_DEBUG".into())
        .ok()
        .filter(JsValue::is_object)
        .map(Object::from)
        .unwrap_or_else(|| {
            let o = Object::new();
            let _ = Reflect::set(&window, &"__DDI_DEBUG".into(), &o);
            o
        });
    debug_into(&obj);
}
