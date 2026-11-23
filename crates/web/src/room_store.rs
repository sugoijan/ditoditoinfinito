//! An online room's songs downloaded ahead on the web
//! (`docs/plans/multiplayer-room-songs.md`, stage 2; the room's client's
//! plan, `ddi_ui::state::room`'s `preload`): kept in the library's
//! IndexedDB (`files`, keys `room/<store>/<share>/<path>`, a room-scoped
//! area, never a song of the library nor of the history), since a list of
//! a hundred songs of four to eight megabytes would not fit in memory.
//! Every read of a room's shared song looks here first ([`blob`]), so a
//! pick, its stage's load and song select's preview come from this device;
//! what is not here comes down the link as before.
//!
//! - The area is deleted when the room is left ([`forget_store`]), and at
//!   the next start for a room no tab has kept alive for
//!   [`STALE_MS`] ([`sweep`]: a crash, a closed tab): each tab touches its
//!   room's area every minute ([`open`]).
//! - A listing (the share's list and its banner) is read first; then a
//!   song's chart, music and pictures, never a movie (`is_small`).
//! - Out of storage (the browser's quota, or `__DDI_ROOM_QUOTA_BYTES`
//!   for tests): [`PreloadProblem::Quota`], the plan falls back to Likely
//!   next.
//! - `window.__DDI_ROOM_STORE` (tests): `{store, files, bytes}`.

use std::cell::RefCell;
use std::collections::BTreeMap;

use ddi_ui::state::room::{Listing, PreloadProblem, Stop};
use wasm_bindgen::JsCast;
use web_sys::Blob;

use crate::web::idb::{self, Db, Write};

/// A room's area unused this long is deleted at the next start (ms).
pub(crate) const STALE_MS: f64 = 30.0 * 60_000.0;

/// How often a kept room's area is touched (ms).
const TOUCH_MS: u32 = 60_000;

const PREFIX: &str = "room/";
const ALIVE: &str = "~alive";
const SHARE_JSON: &str = "~share.json";

/// The room's area this tab keeps: its store's name, the files in it
/// (`<share>/<path>` and their sizes) and its toucher.
struct Kept {
    store: String,
    files: BTreeMap<String, u64>,
    _touch: Option<gloo::timers::callback::Interval>,
}

thread_local! {
    static KEPT: RefCell<Option<Kept>> = const { RefCell::new(None) };
    /// Bytes written to the area this session (the testing quota's).
    static WRITTEN: std::cell::Cell<u64> = const { std::cell::Cell::new(0) };
}

fn area(store: &str) -> String {
    format!("{PREFIX}{store}/")
}

fn key(store: &str, share: &str, path: &str) -> String {
    format!("{}{share}/{path}", area(store))
}

/// Room store `store` kept by this tab: its files known (an area left by a
/// reload of the same room is used again), touched every minute.
pub(crate) fn open(store: &str) {
    let same = KEPT.with_borrow(|k| k.as_ref().is_some_and(|k| k.store == store));
    if same {
        return;
    }
    let name = store.to_string();
    let toucher = {
        let name = name.clone();
        gloo::timers::callback::Interval::new(TOUCH_MS, move || {
            let name = name.clone();
            wasm_bindgen_futures::spawn_local(async move { touch(&name).await });
        })
    };
    KEPT.with_borrow_mut(|k| {
        *k = Some(Kept {
            store: name.clone(),
            files: BTreeMap::new(),
            _touch: Some(toucher),
        })
    });
    wasm_bindgen_futures::spawn_local(async move {
        touch(&name).await;
        let Ok(db) = Db::open().await else {
            return;
        };
        let prefix = area(&name);
        let Ok(keys) = db.keys_with_prefix(idb::FILES, &prefix).await else {
            return;
        };
        let found: Vec<String> = keys
            .iter()
            .filter_map(|k| k.strip_prefix(&prefix))
            .filter(|k| !k.starts_with('~'))
            .map(str::to_string)
            .collect();
        let sizes = sizes_of(&db, &keys).await;
        KEPT.with_borrow_mut(|k| {
            if let Some(k) = k.as_mut().filter(|k| k.store == name) {
                for f in found {
                    let size = sizes.get(&format!("{prefix}{f}")).copied().unwrap_or(0);
                    k.files.insert(f, size);
                }
            }
        });
        publish();
    });
}

/// The files' sizes of `keys` (blobs').
async fn sizes_of(db: &Db, keys: &[String]) -> BTreeMap<String, u64> {
    let values = db.get_many(idb::FILES, keys).await.unwrap_or_default();
    keys.iter()
        .zip(values)
        .filter_map(|(k, v)| Some((k.clone(), v?.dyn_into::<Blob>().ok()?.size() as u64)))
        .collect()
}

/// The area marked alive now.
async fn touch(store: &str) {
    if let Ok(db) = Db::open().await {
        let _ = db
            .write(vec![Write::Put {
                store: idb::FILES,
                key: format!("{}{ALIVE}", area(store)),
                value: js_sys::Date::now().into(),
            }])
            .await;
    }
}

/// Room store `store` deleted (the room left or ended), and forgotten
/// by this tab.
pub(crate) fn forget_store(store: &str) {
    KEPT.with_borrow_mut(|k| {
        if k.as_ref().is_some_and(|k| k.store == store) {
            *k = None;
        }
    });
    publish();
    let prefix = area(store);
    wasm_bindgen_futures::spawn_local(async move {
        if let Ok(db) = Db::open().await
            && let Err(e) = db
                .write(vec![Write::DeletePrefix {
                    store: idb::FILES,
                    prefix,
                }])
                .await
        {
            web_sys::console::warn_1(&format!("room: deleting the room's songs: {e}").into());
        }
        publish_stored();
    });
}

/// Share `share`'s files deleted from room store `store` (its song left
/// the list).
pub(crate) async fn forget(store: &str, share: &str) {
    KEPT.with_borrow_mut(|k| {
        if let Some(k) = k.as_mut().filter(|k| k.store == store) {
            let prefix = format!("{share}/");
            k.files.retain(|f, _| !f.starts_with(&prefix));
        }
    });
    publish();
    if let Ok(db) = Db::open().await {
        let _ = db
            .write(vec![Write::DeletePrefix {
                store: idb::FILES,
                prefix: format!("{}{share}/", area(store)),
            }])
            .await;
    }
    publish_stored();
}

/// At the start: every room's area no tab touched for [`STALE_MS`]
/// deleted, but `keep`'s (the room the page opens on: a reload of it).
pub(crate) fn sweep(keep: Option<String>) {
    wasm_bindgen_futures::spawn_local(async move {
        let Ok(db) = Db::open().await else {
            return;
        };
        let Ok(keys) = db.keys_with_prefix(idb::FILES, PREFIX).await else {
            return;
        };
        let mut stores: Vec<String> = keys
            .iter()
            .filter_map(|k| k.strip_prefix(PREFIX)?.split('/').next())
            .map(str::to_string)
            .collect();
        stores.dedup();
        let now = js_sys::Date::now();
        let mut gone = Vec::new();
        for store in stores {
            if keep.as_deref() == Some(store.as_str()) {
                continue;
            }
            let alive = db
                .get(idb::FILES, &format!("{}{ALIVE}", area(&store)))
                .await
                .ok()
                .flatten()
                .and_then(|v| v.as_f64())
                .unwrap_or(0.0);
            if now - alive >= STALE_MS {
                gone.push(Write::DeletePrefix {
                    store: idb::FILES,
                    prefix: area(&store),
                });
            }
        }
        if !gone.is_empty() {
            let n = gone.len();
            match db.write(gone).await {
                Ok(()) => web_sys::console::log_1(
                    &format!("room: songs of {n} room(s) left before deleted").into(),
                ),
                Err(e) => web_sys::console::warn_1(
                    &format!("room: deleting songs of rooms left before: {e}").into(),
                ),
            }
        }
    });
}

/// File `file` of shared song `song_id` from the room's area, where it is
/// there (none: read it down the link).
pub(crate) async fn blob(song_id: &str, file: &str) -> Option<Blob> {
    let (share, _) = crate::share::parse_song_id(song_id)?;
    let path = ddi_library::share::share_path(file);
    let name = format!("{share}/{path}");
    let store = KEPT.with_borrow(|k| {
        k.as_ref()
            .filter(|k| k.files.contains_key(&name))
            .map(|k| k.store.clone())
    })?;
    let db = Db::open().await.ok()?;
    db.get(idb::FILES, &key(&store, share, &path))
        .await
        .ok()
        .flatten()?
        .dyn_into::<Blob>()
        .ok()
}

/// [`blob`]'s bytes.
pub(crate) async fn bytes(song_id: &str, file: &str) -> Option<Vec<u8>> {
    crate::web::files::blob_bytes(&blob(song_id, file).await?).await.ok()
}

/// The texture source of file `path` of share `share` in the room's area
/// (`files:` its key), where it is there.
pub(crate) fn source(share: &str, path: &str) -> Option<String> {
    let path = ddi_library::share::share_path(path);
    KEPT.with_borrow(|k| {
        let k = k.as_ref()?;
        k.files
            .contains_key(&format!("{share}/{path}"))
            .then(|| format!("files:{}", key(&k.store, share, &path)))
    })
}

/// Share `share` as the room's area keeps its list (a page loaded again
/// in the room reads it from here before the link).
pub(crate) async fn share_json(share: &str) -> Option<String> {
    let store = KEPT.with_borrow(|k| {
        k.as_ref()
            .filter(|k| k.files.contains_key(&format!("{share}/{SHARE_JSON}")))
            .map(|k| k.store.clone())
    })?;
    let db = Db::open().await.ok()?;
    let v = db
        .get(idb::FILES, &key(&store, share, SHARE_JSON))
        .await
        .ok()
        .flatten()?;
    v.as_string()
}

/// The testing quota (`window.__DDI_ROOM_QUOTA_BYTES`): writes past it
/// fail as the browser's quota would.
fn over_quota(more: u64) -> bool {
    crate::web::testing_number("__DDI_ROOM_QUOTA_BYTES")
        .is_some_and(|q| (WRITTEN.get() + more) as f64 > q)
}

/// `value` put at the area's `share`/`path`, its size `size`.
async fn put(
    db: &Db,
    store: &str,
    share: &str,
    path: &str,
    value: wasm_bindgen::JsValue,
    size: u64,
) -> Result<(), PreloadProblem> {
    if over_quota(size) {
        return Err(PreloadProblem::Quota);
    }
    db.write(vec![Write::Put {
        store: idb::FILES,
        key: key(store, share, path),
        value,
    }])
    .await
    .map_err(|e| {
        if e.contains("ran out of storage") || e.contains("Quota") {
            PreloadProblem::Quota
        } else {
            web_sys::console::warn_1(&format!("room: keeping a room's song: {e}").into());
            PreloadProblem::Download
        }
    })?;
    WRITTEN.set(WRITTEN.get() + size);
    KEPT.with_borrow_mut(|k| {
        if let Some(k) = k.as_mut().filter(|k| k.store == store) {
            k.files.insert(format!("{share}/{path}"), size);
        }
    });
    publish_stored();
    Ok(())
}

/// Whether the area has `share`/`path`.
fn has(store: &str, share: &str, path: &str) -> bool {
    KEPT.with_borrow(|k| {
        k.as_ref()
            .is_some_and(|k| k.store == store && k.files.contains_key(&format!("{share}/{path}")))
    })
}

/// Share `share` read (from the link, kept in the area too) for room store
/// `store`.
async fn share_in(
    store: &str,
    share: &str,
) -> Result<std::rc::Rc<crate::share::Share>, PreloadProblem> {
    if has(store, share, SHARE_JSON) {
        return crate::share::load(share).await.map_err(|e| {
            web_sys::console::warn_1(&format!("room: reading share {share}: {e}").into());
            PreloadProblem::Download
        });
    }
    let (loaded, text) = crate::share::load_text(share).await.map_err(|e| {
        web_sys::console::warn_1(&format!("room: reading share {share}: {e}").into());
        PreloadProblem::Download
    })?;
    let db = Db::open().await.map_err(|_| PreloadProblem::Download)?;
    let size = text.len() as u64;
    put(&db, store, share, SHARE_JSON, text.into(), size).await?;
    Ok(loaded)
}

/// The listing of room song `share` (its list and its banner, into the
/// area) for the plan.
pub(crate) async fn listing(store: &str, share: &str) -> Result<Listing, PreloadProblem> {
    let loaded = share_in(store, share).await?;
    let song = loaded.songs.first().ok_or(PreloadProblem::Download)?;
    let entry = song.record.entry.clone();
    let mut banner = None;
    if let Some(b) = entry.banner.as_ref() {
        let path = ddi_library::share::share_path(b.as_str());
        if let Some(f) = song.files.iter().find(|f| f.path == path) {
            fetch_into(store, share, f, &|_| {}, &Stop::default()).await?;
            banner = source(share, &path);
        }
    }
    let bytes = song
        .files
        .iter()
        .filter(|f| ddi_library::share::is_small(&f.path))
        .map(|f| f.bytes.max(0.0) as u64)
        .sum();
    Ok(Listing {
        entry,
        pack: song.record.pack.clone(),
        banner,
        bytes,
    })
}

/// Room song `share` downloaded into the area: its chart, music and
/// pictures (never a movie), `progress` told the bytes so far of all,
/// given up where `stop` says so.
pub(crate) async fn song(
    store: &str,
    share: &str,
    progress: &dyn Fn(f64, f64),
    stop: &Stop,
) -> Result<(), PreloadProblem> {
    let loaded = share_in(store, share).await?;
    let song = loaded.songs.first().ok_or(PreloadProblem::Download)?;
    let files: Vec<&ddi_library::share::ShareFileDoc> = song
        .files
        .iter()
        .filter(|f| ddi_library::share::is_small(&f.path))
        .collect();
    let total: f64 = files.iter().map(|f| f.bytes.max(0.0)).sum();
    let mut done = 0.0;
    for f in files {
        if stop.stopped() {
            return Err(PreloadProblem::Stopped);
        }
        let before = done;
        fetch_into(store, share, f, &|got| progress(before + got, total), stop).await?;
        done += f.bytes.max(0.0);
        progress(done, total);
    }
    Ok(())
}

/// File `f` of share `share` into the area, unless it is there; told its
/// bytes so far.
async fn fetch_into(
    store: &str,
    share: &str,
    f: &ddi_library::share::ShareFileDoc,
    progress: &dyn Fn(f64),
    stop: &Stop,
) -> Result<(), PreloadProblem> {
    if has(store, share, &f.path) {
        return Ok(());
    }
    let base = crate::share::base();
    let url = if f.url.is_empty() {
        ddi_library::share::file_url(&base, share, 0, &f.path)
    } else {
        ddi_library::share::resolve_url(&base, &f.url)
    };
    let blob = fetch_blob_stopping(&url, progress, stop).await?;
    let size = blob.size() as u64;
    let db = Db::open().await.map_err(|_| PreloadProblem::Download)?;
    put(&db, store, share, &f.path, blob.into(), size).await
}

/// `url`'s body as a `Blob`, read chunk by chunk (told its bytes so far,
/// given up where `stop` says so: its request aborted).
async fn fetch_blob_stopping(
    url: &str,
    progress: &dyn Fn(f64),
    stop: &Stop,
) -> Result<Blob, PreloadProblem> {
    let fail = |e: String| {
        web_sys::console::warn_1(&format!("room: downloading {url}: {e}").into());
        PreloadProblem::Download
    };
    let window = web_sys::window().ok_or(PreloadProblem::Download)?;
    let abort = web_sys::AbortController::new().map_err(|e| fail(format!("{e:?}")))?;
    let init = web_sys::RequestInit::new();
    init.set_signal(Some(&abort.signal()));
    // What may wait for what matters more (a stage's own load).
    let _ = js_sys::Reflect::set(&init, &"priority".into(), &"low".into());
    let resp: web_sys::Response =
        wasm_bindgen_futures::JsFuture::from(window.fetch_with_str_and_init(url, &init))
            .await
            .map_err(|e| fail(format!("{e:?}")))?
            .dyn_into()
            .map_err(|_| fail("not a response".into()))?;
    if !resp.ok() {
        return Err(fail(format!("HTTP {}", resp.status())));
    }
    let Some(body) = resp.body() else {
        return crate::web::files::bytes_blob(&[], "").map_err(fail);
    };
    let reader = crate::web::files::body_reader(&body).map_err(fail)?;
    let parts = js_sys::Array::new();
    let mut so_far = 0.0;
    // Stopped before each read: the request aborted.
    let read = if stop.stopped() {
        std::ops::ControlFlow::Break(())
    } else {
        crate::web::files::each_chunk(&reader, |bytes| {
            so_far += f64::from(bytes.length());
            parts.push(&bytes);
            progress(so_far);
            if stop.stopped() {
                std::ops::ControlFlow::Break(())
            } else {
                std::ops::ControlFlow::Continue(())
            }
        })
        .await
        .map_err(fail)?
    };
    if read.is_break() {
        abort.abort();
        return Err(PreloadProblem::Stopped);
    }
    Blob::new_with_u8_array_sequence(&parts).map_err(|e| fail(format!("{e:?}")))
}

/// `window.__DDI_ROOM_STORE` (tests): the area this tab keeps, its files
/// and their bytes.
fn publish() {
    let Some(w) = web_sys::window() else {
        return;
    };
    let o = js_sys::Object::new();
    let set = |k: &str, v: wasm_bindgen::JsValue| {
        let _ = js_sys::Reflect::set(&o, &k.into(), &v);
    };
    KEPT.with_borrow(|k| match k {
        Some(k) => {
            set("store", k.store.as_str().into());
            set("files", (k.files.len() as f64).into());
            set("bytes", (k.files.values().sum::<u64>() as f64).into());
        }
        None => {
            set("store", wasm_bindgen::JsValue::NULL);
            set("files", 0.0.into());
            set("bytes", 0.0.into());
        }
    });
    let _ = js_sys::Reflect::set(&w, &"__DDI_ROOM_STORE".into(), &o);
}

/// [`publish`], and what every room's area holds in the library read
/// again (tests: `window.__DDI_ROOM_STORED`), once the writes asked for
/// are done.
fn publish_stored() {
    publish();
    if crate::web::automated() {
        wasm_bindgen_futures::spawn_local(async {
            let (keys, bytes) = stored_bytes().await;
            if let Some(w) = web_sys::window() {
                let o = js_sys::Object::new();
                let _ = js_sys::Reflect::set(&o, &"keys".into(), &(keys as f64).into());
                let _ = js_sys::Reflect::set(&o, &"bytes".into(), &(bytes as f64).into());
                let _ = js_sys::Reflect::set(&w, &"__DDI_ROOM_STORED".into(), &o);
            }
        });
    }
}

/// The bytes every room's area holds in the library now (tests: a store's
/// size before and after leaving, `window.__DDI_ROOM_STORED`), read from
/// IndexedDB.
async fn stored_bytes() -> (usize, u64) {
    let Ok(db) = Db::open().await else {
        return (0, 0);
    };
    let keys = db
        .keys_with_prefix(idb::FILES, PREFIX)
        .await
        .unwrap_or_default();
    let sizes = sizes_of(&db, &keys).await;
    (keys.len(), sizes.values().sum())
}
