//! Minimal IndexedDB access for the imported-song library.
//!
//! IndexedDB stores `Blob`s directly, so imported audio and graphics go from
//! the picked `File` (or a slice of a zip) into storage without passing
//! through wasm memory. OPFS would add nothing for this access pattern (write
//! once, read whole files) and is missing where we need it most: Firefox
//! private windows have no OPFS, and Safari only gained main-thread writes in
//! version 26.
//!
//! Every write issues all its requests synchronously inside one transaction
//! and then awaits the transaction's `complete` event, so no transaction is
//! ever left idle across an `await` (older Safari auto-committed those).

use std::cell::RefCell;

use gloo::events::EventListener;
use js_sys::{Array, Function, Promise};
use wasm_bindgen::{JsCast, JsValue};
use wasm_bindgen_futures::JsFuture;
use web_sys::{
    IdbDatabase, IdbKeyRange, IdbOpenDbRequest, IdbRequest, IdbTransaction, IdbTransactionMode,
};

const NAME: &str = "ddi-library";
const VERSION: u32 = 3;

/// Song records (JSON strings) keyed by song id.
pub(crate) const SONGS: &str = "songs";
/// File blobs keyed by `<song id>/<file name>` (a noteskin's by
/// `noteskin/<game>/<name>/<path>`).
pub(crate) const FILES: &str = "files";
/// Imported noteskins (JSON strings) keyed by `<game>/<name>` (version 2).
pub(crate) const NOTESKINS: &str = "noteskins";
/// Plays, whole (version 3): keyed by the play's id, an object with the
/// record (JSON string) and the log and the timing (bytes)
/// (`crate::history`).
pub(crate) const PLAYS: &str = "plays";
/// The plays' summaries (JSON strings) by the same ids, read at start
/// without the logs (version 3).
pub(crate) const PLAY_SUMMARIES: &str = "play_summaries";

pub(crate) struct Db(IdbDatabase);

/// One write in a [`Db::write`] batch.
pub(crate) enum Write {
    Put {
        store: &'static str,
        key: String,
        value: JsValue,
    },
    Delete {
        store: &'static str,
        key: String,
    },
    /// Every key starting with `prefix`.
    DeletePrefix {
        store: &'static str,
        prefix: String,
    },
    /// Everything in the store.
    Clear {
        store: &'static str,
    },
}

fn err(context: &str, e: impl Into<JsValue>) -> String {
    let e: JsValue = e.into();
    let text = e
        .dyn_ref::<js_sys::Error>()
        .map(|e| String::from(e.message()))
        .or_else(|| {
            e.dyn_ref::<web_sys::DomException>()
                .map(|e| format!("{}: {}", e.name(), e.message()))
        })
        .unwrap_or_else(|| format!("{e:?}"));
    format!("{context}: {text}")
}

/// Resolves with the request's result once it fires `success`, rejects on
/// `error`. The listeners are removed when the future is dropped, so a
/// component that goes away mid-request leaves no dangling callbacks.
async fn request(req: &IdbRequest) -> Result<JsValue, JsValue> {
    let mut listeners = Vec::new();
    let promise = Promise::new(&mut |resolve: Function, reject: Function| {
        let r = req.clone();
        listeners.push(EventListener::once(req, "success", move |_| {
            let _ = resolve.call1(&JsValue::NULL, &r.result().unwrap_or(JsValue::UNDEFINED));
        }));
        let r = req.clone();
        listeners.push(EventListener::once(req, "error", move |_| {
            let e = r.error().ok().flatten().map(JsValue::from);
            let _ = reject.call1(&JsValue::NULL, &e.unwrap_or(JsValue::UNDEFINED));
        }));
        // Only open requests fire this: another tab holds an older version
        // open and has not closed it yet. The open waits for it (the game
        // closes its connection when a newer version asks); meanwhile the
        // library says so (`blocked`).
        listeners.push(EventListener::once(req, "blocked", move |_| {
            BLOCKED.with(|b| b.set(true));
            web_sys::console::warn_1(
                &"the song library waits for another tab with the game open to let go of it".into(),
            );
        }));
    });
    let result = JsFuture::from(promise).await;
    drop(listeners);
    result
}

/// Resolves when the transaction commits, rejects when it aborts. A failed
/// request aborts the transaction; only by `abort` is `tx.error` set (a
/// request's `error` event bubbles here first, while it is still null).
async fn complete(tx: &IdbTransaction) -> Result<(), JsValue> {
    let mut listeners = Vec::new();
    let promise = Promise::new(&mut |resolve: Function, reject: Function| {
        listeners.push(EventListener::once(tx, "complete", move |_| {
            let _ = resolve.call0(&JsValue::NULL);
        }));
        let t = tx.clone();
        listeners.push(EventListener::once(tx, "abort", move |_| {
            let e = t.error().map(JsValue::from);
            let _ = reject.call1(
                &JsValue::NULL,
                &e.unwrap_or_else(|| JsValue::from_str("transaction aborted")),
            );
        }));
    });
    let result = JsFuture::from(promise).await.map(|_| ());
    drop(listeners);
    result
}

/// Keys from `prefix` (inclusive) to just past every string starting with it.
fn prefix_range(prefix: &str) -> Result<IdbKeyRange, JsValue> {
    IdbKeyRange::bound(
        &JsValue::from_str(prefix),
        &JsValue::from_str(&format!("{prefix}\u{ffff}")),
    )
}

/// The open connection and the listeners that close it, shared by every
/// caller so the song list does not open one per banner.
struct Connection {
    db: IdbDatabase,
    _listeners: [EventListener; 2],
}

thread_local! {
    static CONNECTION: RefCell<Option<Connection>> = const { RefCell::new(None) };
    /// The open under way, which every caller meanwhile waits for (one
    /// request, one connection).
    static OPENING: RefCell<Option<Promise>> = const { RefCell::new(None) };
    /// An open waited for another tab (see [`blocked`]).
    static BLOCKED: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
}

/// Whether opening the library is waiting for another tab to close its
/// older connection (it goes on once that tab lets go).
pub(crate) fn blocked() -> bool {
    BLOCKED.with(std::cell::Cell::get) && CONNECTION.with(|c| c.borrow().is_none())
}

impl Db {
    /// The shared connection, opened on first use; callers meanwhile wait
    /// for the same open.
    pub(crate) async fn open() -> Result<Db, String> {
        if let Some(db) = CONNECTION.with(|c| c.borrow().as_ref().map(|c| c.db.clone())) {
            return Ok(Db(db));
        }
        let promise = OPENING.with(|o| {
            o.borrow_mut()
                .get_or_insert_with(|| {
                    wasm_bindgen_futures::future_to_promise(async {
                        let r = Self::open_shared().await;
                        OPENING.with(|o| o.borrow_mut().take());
                        r.map(JsValue::from).map_err(|e| JsValue::from_str(&e))
                    })
                })
                .clone()
        });
        JsFuture::from(promise)
            .await
            .map(|db| Db(db.unchecked_into()))
            .map_err(|e| e.as_string().unwrap_or_else(|| format!("{e:?}")))
    }

    /// Opens the connection and keeps it as the shared one.
    async fn open_shared() -> Result<IdbDatabase, String> {
        let db = Self::open_new().await?;
        BLOCKED.with(|b| b.set(false));
        // A newer version opened elsewhere (another tab after an update)
        // waits until every old connection closes: close ours right away.
        // `close` fires when the browser drops the connection itself.
        let d = db.clone();
        let on_version = EventListener::new(&db, "versionchange", move |_| {
            d.close();
            CONNECTION.with(|c| c.borrow_mut().take());
        });
        let on_close = EventListener::new(&db, "close", |_| {
            CONNECTION.with(|c| c.borrow_mut().take());
        });
        CONNECTION.with(|c| {
            *c.borrow_mut() = Some(Connection {
                db: db.clone(),
                _listeners: [on_version, on_close],
            })
        });
        Ok(db)
    }

    async fn open_new() -> Result<IdbDatabase, String> {
        let factory = web_sys::window()
            .ok_or("no window")?
            .indexed_db()
            .map_err(|e| err("indexedDB", e))?
            .ok_or("IndexedDB is not available in this browser")?;
        let open: IdbOpenDbRequest = factory
            .open_with_u32(NAME, VERSION)
            .map_err(|e| err("indexedDB.open", e))?;
        let o = open.clone();
        let _upgrade = EventListener::new(&open, "upgradeneeded", move |_| {
            let Ok(db) = o.result().and_then(|r| r.dyn_into::<IdbDatabase>()) else {
                return;
            };
            let names = db.object_store_names();
            // Only what is missing is created: an upgrade keeps every
            // store there.
            for store in [SONGS, FILES, NOTESKINS, PLAYS, PLAY_SUMMARIES] {
                if !names.contains(store) {
                    let _ = db.create_object_store(store);
                }
            }
        });
        let db = request(&open).await.map_err(|e| {
            let e = err("opening the song library", e);
            // This tab's code is older than the library (the game was
            // updated in another tab).
            if e.contains("VersionError") {
                "the game was updated in another tab; reload this one".to_string()
            } else {
                e
            }
        })?;
        Ok(db.unchecked_into())
    }

    fn transaction(
        &self,
        stores: &[&str],
        mode: IdbTransactionMode,
    ) -> Result<IdbTransaction, String> {
        let names: Array = stores.iter().map(|s| JsValue::from_str(s)).collect();
        self.0
            .transaction_with_str_sequence_and_mode(&names, mode)
            .map_err(|e| err("transaction", e))
    }

    pub(crate) async fn get(&self, store: &str, key: &str) -> Result<Option<JsValue>, String> {
        let tx = self.transaction(&[store], IdbTransactionMode::Readonly)?;
        let req = tx
            .object_store(store)
            .and_then(|s| s.get(&JsValue::from_str(key)))
            .map_err(|e| err("get", e))?;
        let v = request(&req)
            .await
            .map_err(|e| err(&format!("reading {key}"), e))?;
        Ok((!v.is_undefined()).then_some(v))
    }

    /// Several keys in one transaction; `None` for missing keys.
    pub(crate) async fn get_many(
        &self,
        store: &str,
        keys: &[String],
    ) -> Result<Vec<Option<JsValue>>, String> {
        if keys.is_empty() {
            return Ok(Vec::new());
        }
        let tx = self.transaction(&[store], IdbTransactionMode::Readonly)?;
        let object_store = tx.object_store(store).map_err(|e| err("get", e))?;
        let requests = keys
            .iter()
            .map(|k| object_store.get(&JsValue::from_str(k)))
            .collect::<Result<Vec<IdbRequest>, _>>()
            .map_err(|e| err("get", e))?;
        complete(&tx)
            .await
            .map_err(|e| err(&format!("reading {store}"), e))?;
        Ok(requests
            .iter()
            .map(|r| r.result().ok().filter(|v| !v.is_undefined()))
            .collect())
    }

    /// Keys in `store` starting with `prefix`.
    pub(crate) async fn keys_with_prefix(
        &self,
        store: &str,
        prefix: &str,
    ) -> Result<Vec<String>, String> {
        let tx = self.transaction(&[store], IdbTransactionMode::Readonly)?;
        let req = prefix_range(prefix)
            .and_then(|range| tx.object_store(store)?.get_all_keys_with_key(&range.into()))
            .map_err(|e| err("getAllKeys", e))?;
        complete(&tx)
            .await
            .map_err(|e| err(&format!("reading {store}"), e))?;
        Ok(req
            .result()
            .ok()
            .and_then(|v| v.dyn_into::<Array>().ok())
            .map(|a| a.to_vec())
            .unwrap_or_default()
            .into_iter()
            .filter_map(|k| k.as_string())
            .collect())
    }

    /// Up to `limit` (key, value) pairs of a store in key order, after
    /// `after` (from the first with none): a store read a batch at a time.
    pub(crate) async fn entries_after(
        &self,
        store: &str,
        after: Option<&str>,
        limit: u32,
    ) -> Result<Vec<(String, JsValue)>, String> {
        let tx = self.transaction(&[store], IdbTransactionMode::Readonly)?;
        let range: JsValue = match after {
            Some(k) => IdbKeyRange::lower_bound_with_open(&JsValue::from_str(k), true)
                .map_err(|e| err("range", e))?
                .into(),
            None => JsValue::UNDEFINED,
        };
        let (keys, values) = tx
            .object_store(store)
            .and_then(|s| {
                Ok((
                    s.get_all_keys_with_key_and_limit(&range, limit)?,
                    s.get_all_with_key_and_limit(&range, limit)?,
                ))
            })
            .map_err(|e| err("getAll", e))?;
        complete(&tx)
            .await
            .map_err(|e| err(&format!("reading {store}"), e))?;
        let array = |r: &IdbRequest| {
            r.result()
                .ok()
                .and_then(|v| v.dyn_into::<Array>().ok())
                .map(|a| a.to_vec())
                .unwrap_or_default()
        };
        Ok(array(&keys)
            .into_iter()
            .zip(array(&values))
            .filter_map(|(k, v)| Some((k.as_string()?, v)))
            .collect())
    }

    /// Every (key, value) pair of a store, in key order.
    pub(crate) async fn entries(&self, store: &str) -> Result<Vec<(String, JsValue)>, String> {
        let tx = self.transaction(&[store], IdbTransactionMode::Readonly)?;
        let (keys, values) = tx
            .object_store(store)
            .and_then(|s| Ok((s.get_all_keys()?, s.get_all()?)))
            .map_err(|e| err("getAll", e))?;
        complete(&tx)
            .await
            .map_err(|e| err(&format!("reading {store}"), e))?;
        let array = |r: &IdbRequest| {
            r.result()
                .ok()
                .and_then(|v| v.dyn_into::<Array>().ok())
                .map(|a| a.to_vec())
                .unwrap_or_default()
        };
        Ok(array(&keys)
            .into_iter()
            .zip(array(&values))
            .filter_map(|(k, v)| Some((k.as_string()?, v)))
            .collect())
    }

    /// Applies every write in one read-write transaction: all or nothing.
    pub(crate) async fn write(&self, writes: Vec<Write>) -> Result<(), String> {
        let mut stores: Vec<&str> = writes
            .iter()
            .map(|w| match w {
                Write::Put { store, .. }
                | Write::Delete { store, .. }
                | Write::DeletePrefix { store, .. }
                | Write::Clear { store } => *store,
            })
            .collect();
        stores.sort_unstable();
        stores.dedup();
        if stores.is_empty() {
            return Ok(());
        }
        let tx = self.transaction(&stores, IdbTransactionMode::Readwrite)?;
        let issued = writes.into_iter().try_for_each(|w| -> Result<(), JsValue> {
            match w {
                Write::Put { store, key, value } => {
                    tx.object_store(store)?
                        .put_with_key(&value, &JsValue::from_str(&key))?;
                }
                Write::Delete { store, key } => {
                    tx.object_store(store)?.delete(&JsValue::from_str(&key))?;
                }
                Write::Clear { store } => {
                    tx.object_store(store)?.clear()?;
                }
                Write::DeletePrefix { store, prefix } => {
                    tx.object_store(store)?
                        .delete(&prefix_range(&prefix)?.into())?;
                }
            }
            Ok(())
        });
        if let Err(e) = issued {
            let _ = tx.abort();
            return Err(err("writing the song library", e));
        }
        complete(&tx).await.map_err(|e| {
            let e = err("writing the song library", e);
            if e.contains("QuotaExceeded") {
                "the browser ran out of storage for imported songs and noteskins".to_string()
            } else {
                e
            }
        })
    }
}
