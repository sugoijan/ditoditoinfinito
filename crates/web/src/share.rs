//! Songs shared by link (`docs/plans/shared-songs.md`): a share read from
//! the private Worker at [`base`] by its id (`#/s/<id>`), held in memory
//! for the session and never in the library or IndexedDB; its songs are
//! the records the web stores imported songs as, their files reached by
//! URL. One share is current at a time ([`current`]); a play of one of
//! its songs, opened at its own address, loads the share again by the id
//! in the song's id ([`song_id`], `shared:<id>:<n>`).

use std::cell::RefCell;
use std::rc::Rc;

use serde::{Deserialize, Serialize};
use wasm_bindgen::JsCast;

use ddi_ui::screens::data::{ShareState, ShareView};

use crate::songs::ImportedSong;
use crate::web::idb::{self, Db};

/// The server's base URL (the private Worker, `ddi-server`): `api` beside
/// the page (`https://sugoijan.dev/ditoditoinfinito/api`, the Worker's
/// route on the app's own origin, so nothing crosses origins; `just
/// dev` proxies it to a local Worker, `Trunk.toml`), else the build's
/// `DDI_API_URL`; `window.__DDI_API_URL` overrides both for tests.
pub(crate) fn base() -> String {
    let window = web_sys::window();
    let from_page = window.as_ref().and_then(|w| {
        js_sys::Reflect::get(w, &"__DDI_API_URL".into())
            .ok()
            .and_then(|v| v.as_string())
    });
    let beside_the_page = || {
        let l = window.as_ref()?.location();
        let (origin, path) = (l.origin().ok()?, l.pathname().ok()?);
        let dir = path.rsplit_once('/').map_or("", |(d, _)| d);
        Some(format!("{origin}{dir}/api"))
    };
    from_page
        .or_else(|| option_env!("DDI_API_URL").map(str::to_string))
        .or_else(beside_the_page)
        .unwrap_or_else(|| "https://sugoijan.dev/ditoditoinfinito/api".to_string())
        .trim_end_matches('/')
        .to_string()
}

/// A share as the Worker answers `GET <base>/s/<id>`.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub(crate) struct Share {
    #[serde(default)]
    pub(crate) pack: String,
    /// Unix seconds; `None`: never.
    #[serde(default)]
    pub(crate) expires_at: Option<f64>,
    #[serde(default)]
    pub(crate) created_at: f64,
    pub(crate) songs: Vec<SharedSong>,
    #[serde(default)]
    pub(crate) pack_art: Option<String>,
    /// The share's id, as loaded (not in the Worker's answer).
    #[serde(skip)]
    pub(crate) id: String,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub(crate) struct SharedSong {
    pub(crate) record: ImportedSong,
    #[serde(default)]
    pub(crate) files: Vec<SharedFile>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub(crate) struct SharedFile {
    pub(crate) path: String,
    #[serde(default)]
    pub(crate) bytes: f64,
    pub(crate) url: String,
}

/// Why a share could not be read.
#[derive(Clone, Debug, PartialEq)]
pub(crate) enum ShareError {
    /// No share has this id (404).
    Missing,
    /// It had, and expired (410).
    Expired,
    Other(String),
}

impl ShareError {
    /// What the page says: one line for a link that is gone either way.
    pub(crate) fn message(&self) -> String {
        match self {
            ShareError::Missing | ShareError::Expired => {
                "This link doesn't exist or has expired.".into()
            }
            ShareError::Other(e) => format!("The shared songs could not be loaded: {e}"),
        }
    }
}

thread_local! {
    static CURRENT: RefCell<Option<Rc<Share>>> = const { RefCell::new(None) };
}

/// The share loaded last, if any.
pub(crate) fn current() -> Option<Rc<Share>> {
    CURRENT.with_borrow(Clone::clone)
}

pub(crate) use ddi_library::share::{is_shared_id, parse_song_id, song_id, valid_id};

/// The URL of file `path` of the shared song `song_id`, as the share
/// lists it (relative to the API's base, as the Worker lists them, or
/// absolute; a file it does not list is asked for at the Worker's path
/// for it, which also serves what the uploader sent and the list left
/// out); `None` for a song of no share.
pub(crate) fn file_url(song_id: &str, path: &str) -> Option<String> {
    let (share, n) = parse_song_id(song_id)?;
    if let Some(s) = current().filter(|s| s.id == share)
        && let Some(song) = s.songs.get(n)
        && let Some(f) = song.files.iter().find(|f| f.path == path)
    {
        return Some(resolve(&f.url));
    }
    let encoded: Vec<String> = path
        .split('/')
        .map(|p| String::from(js_sys::encode_uri_component(p)))
        .collect();
    Some(format!("{}/s/{share}/f/{n}/{}", base(), encoded.join("/")))
}

/// A listed URL as fetched: an absolute one as it is, a relative one
/// under the API's base.
fn resolve(url: &str) -> String {
    if url.contains("://") {
        url.to_string()
    } else {
        format!("{}/{}", base(), url.trim_start_matches('/'))
    }
}

/// The size the share lists for file `path` of the shared song
/// `song_id`, if it lists it.
pub(crate) fn file_size(song_id: &str, path: &str) -> Option<u64> {
    let (share, n) = parse_song_id(song_id)?;
    let s = current().filter(|s| s.id == share)?;
    let f = s.songs.get(n)?.files.iter().find(|f| f.path == path)?;
    (f.bytes >= 0.0).then_some(f.bytes as u64)
}

/// The share `id`, as loaded now or read from the Worker and kept as
/// the current one: each song's record renamed to its shared id, with
/// nothing of the library it came from (no kept folder, no links, no
/// opening still).
pub(crate) async fn load(id: &str) -> Result<Rc<Share>, ShareError> {
    if !valid_id(id) {
        return Err(ShareError::Missing);
    }
    if let Some(s) = current().filter(|s| s.id == id) {
        return Ok(s);
    }
    publish(id, "loading", None);
    let url = format!("{}/s/{id}", base());
    let resp = gloo::net::http::Request::get(&url)
        .send()
        .await
        .map_err(|e| ShareError::Other(format!("GET {url}: {e}")))?;
    let result = match resp.status() {
        404 => Err(ShareError::Missing),
        410 => Err(ShareError::Expired),
        s if !resp.ok() => Err(ShareError::Other(format!("GET {url}: HTTP {s}"))),
        _ => resp
            .text()
            .await
            .map_err(|e| ShareError::Other(e.to_string()))
            .and_then(|t| from_json(id, &t)),
    };
    match &result {
        Ok(share) => {
            let share = share.clone();
            CURRENT.with_borrow_mut(|c| *c = Some(share.clone()));
            publish(id, "ready", Some(&share));
        }
        Err(e) => publish(
            id,
            match e {
                ShareError::Missing => "missing",
                ShareError::Expired => "expired",
                ShareError::Other(_) => "failed",
            },
            None,
        ),
    }
    result
}

/// A share from the Worker's JSON, its songs made this share's.
pub(crate) fn from_json(id: &str, text: &str) -> Result<Rc<Share>, ShareError> {
    let mut share: Share =
        serde_json::from_str(text).map_err(|e| ShareError::Other(format!("the share: {e}")))?;
    share.id = id.to_string();
    for (n, song) in share.songs.iter_mut().enumerate() {
        let r = &mut song.record;
        r.entry.id = song_id(id, n);
        r.folder = None;
        r.links.clear();
        r.still = None;
        if r.pack.is_empty() {
            r.pack = share.pack.clone();
        }
    }
    Ok(Rc::new(share))
}

impl Share {
    /// How long it has left, in a few words; `None`: forever.
    pub(crate) fn expires_in(&self, now_secs: f64) -> Option<String> {
        ddi_library::share::expires_in(self.expires_at, now_secs)
    }
}

/// `window.__DDI_SHARE`, for tests (its own object: the UI host writes
/// `__DDI_DEBUG` whole each frame): `{id, state, pack, songs,
/// expiresAt, expiresIn}`, `state` among `loading`, `ready`, `missing`,
/// `expired`, `failed`.
fn publish(id: &str, state: &str, share: Option<&Share>) {
    let Some(w) = web_sys::window() else {
        return;
    };
    let o = js_sys::Object::new();
    let set = |k: &str, v: wasm_bindgen::JsValue| {
        let _ = js_sys::Reflect::set(&o, &k.into(), &v);
    };
    set("id", id.into());
    set("state", state.into());
    if let Some(s) = share {
        set("pack", s.pack.as_str().into());
        set("songs", (s.songs.len() as f64).into());
        set(
            "expiresAt",
            s.expires_at
                .map_or(wasm_bindgen::JsValue::NULL, wasm_bindgen::JsValue::from_f64),
        );
        let now = js_sys::Date::now() / 1000.0;
        set(
            "expiresIn",
            s.expires_in(now)
                .map_or(wasm_bindgen::JsValue::NULL, |t| t.into()),
        );
    }
    let _ = js_sys::Reflect::set(&w, &"__DDI_SHARE".into(), &o);
}

/// What `POST <base>/s` is sent: the share to make, its files by path
/// and size (their bytes follow, one `PUT` each).
#[derive(Clone, Debug, PartialEq, Serialize)]
pub(crate) struct NewShare {
    pub(crate) pack: String,
    /// `1h`, `1d`, `1w`, `1m` or `never`.
    pub(crate) expires: String,
    pub(crate) songs: Vec<NewSong>,
    pub(crate) pack_art: bool,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
pub(crate) struct NewSong {
    pub(crate) record: ImportedSong,
    pub(crate) files: Vec<NewFile>,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
pub(crate) struct NewFile {
    pub(crate) path: String,
    pub(crate) bytes: f64,
}

/// The Worker's answer to `POST <base>/s`.
#[derive(Deserialize)]
struct Created {
    id: String,
}

/// Told how the upload goes (each file sent, the end).
pub(crate) type Progress = Rc<dyn Fn(&ShareView)>;

/// Shares the imported songs `ids` as the pack `pack`, expiring after
/// `expires` (`1h`, `1d`, `1w`, `1m`, `never`), with `token`
/// (`docs/plans/shared-songs.md`): their records and files read from
/// the library (a linked song's from its folder), the share made, each
/// file put, the share finished; the link to it. `progress` is told at
/// each step; `window.__DDI_SHARE_UPLOAD` says the same for tests.
pub(crate) async fn upload(
    pack: String,
    ids: Vec<String>,
    expires: String,
    token: String,
    progress: Progress,
) -> Result<String, String> {
    let mut view = ShareView {
        state: ShareState::Uploading,
        songs: ids.len(),
        ..ShareView::default()
    };
    let tell = |v: &ShareView| {
        publish_upload(v);
        progress(v);
    };
    tell(&view);
    let result = upload_inner(&pack, &ids, &expires, &token, &mut view, &tell).await;
    match &result {
        Ok(link) => {
            view.state = ShareState::Done;
            view.link = link.clone();
        }
        Err(e) => {
            view.state = ShareState::Failed;
            view.error = e.clone();
        }
    }
    tell(&view);
    result
}

async fn upload_inner(
    pack: &str,
    ids: &[String],
    expires: &str,
    token: &str,
    view: &mut ShareView,
    tell: &dyn Fn(&ShareView),
) -> Result<String, String> {
    if token.is_empty() {
        return Err("Set the upload token under Options → Sharing first.".into());
    }
    if ids.is_empty() {
        return Err("Nothing to share.".into());
    }
    let db = Db::open().await?;
    // Every song's record and files first: the sizes go in the request
    // that makes the share.
    let mut songs = Vec::new();
    let mut blobs: Vec<Vec<(String, web_sys::Blob)>> = Vec::new();
    for id in ids {
        let record = crate::songs::imported_record(&db, id).await?;
        let prefix = crate::songs::file_key(id, "");
        let mut paths: Vec<String> = db
            .keys_with_prefix(idb::FILES, &prefix)
            .await?
            .into_iter()
            .filter_map(|k| k.strip_prefix(&prefix).map(String::from))
            // The opening still is this library's, not the song's.
            .filter(|p| p != crate::stills::STILL_FILE)
            .collect();
        for linked in record.links.keys() {
            if !paths.contains(linked) {
                paths.push(linked.clone());
            }
        }
        let mut files = Vec::new();
        let mut these = Vec::new();
        for path in paths {
            let blob = crate::songs::song_file(&db, &record, &path)
                .await
                .map_err(|e| format!("{}: {}", record.entry.title, e.message()))?;
            let bytes = blob.size();
            view.bytes += bytes;
            files.push(NewFile {
                path: path.clone(),
                bytes,
            });
            these.push((path, blob));
        }
        songs.push(NewSong { record, files });
        blobs.push(these);
    }
    let art_key = crate::songs::pack_art_key(pack);
    let art = db
        .get(idb::FILES, &art_key)
        .await?
        .and_then(|v| v.dyn_into::<web_sys::Blob>().ok());
    if let Some(a) = &art {
        view.bytes += a.size();
    }
    tell(view);
    let new = NewShare {
        pack: pack.to_string(),
        expires: expires.to_string(),
        songs,
        pack_art: art.is_some(),
    };
    let base = base();
    let created: Created = gloo::net::http::Request::post(&format!("{base}/s"))
        .header("Authorization", &format!("Bearer {token}"))
        .json(&new)
        .map_err(|e| e.to_string())?
        .send()
        .await
        .map_err(|e| format!("making the share: {e}"))
        .and_then(checked)?
        .json()
        .await
        .map_err(|e| format!("the share's id: {e}"))?;
    let id = created.id;
    for (n, files) in blobs.into_iter().enumerate() {
        for (path, blob) in files {
            let encoded: Vec<String> = path
                .split('/')
                .map(|p| String::from(js_sys::encode_uri_component(p)))
                .collect();
            let url = format!("{base}/s/{id}/f/{n}/{}", encoded.join("/"));
            put(&url, token, &blob)
                .await
                .map_err(|e| format!("{path}: {e}"))?;
            view.done_bytes += blob.size();
            tell(view);
        }
        view.done_songs += 1;
        tell(view);
    }
    if let Some(a) = art {
        put(&format!("{base}/s/{id}/art"), token, &a).await?;
        view.done_bytes += a.size();
        tell(view);
    }
    gloo::net::http::Request::post(&format!("{base}/s/{id}/finish"))
        .header("Authorization", &format!("Bearer {token}"))
        .send()
        .await
        .map_err(|e| format!("finishing the share: {e}"))
        .and_then(checked)?;
    Ok(link(&id))
}

/// One file's bytes to the Worker.
async fn put(url: &str, token: &str, blob: &web_sys::Blob) -> Result<(), String> {
    gloo::net::http::Request::put(url)
        .header("Authorization", &format!("Bearer {token}"))
        .body(blob)
        .map_err(|e| e.to_string())?
        .send()
        .await
        .map_err(|e| e.to_string())
        .and_then(checked)
        .map(|_| ())
}

/// A response that is not a success, as an error.
fn checked(r: gloo::net::http::Response) -> Result<gloo::net::http::Response, String> {
    match r.status() {
        401 | 403 => Err("the upload token was refused".into()),
        413 => Err("too large to share (2 GB a share, 512 MB a file, 200 songs)".into()),
        s if !r.ok() => Err(format!("HTTP {s}")),
        _ => Ok(r),
    }
}

/// The shares the token may see (`GET <base>/s`), each with its link.
pub(crate) async fn list(token: &str) -> Result<ddi_ui::screens::data::SharesView, String> {
    use ddi_ui::screens::data::{ShareRow, SharesState, SharesView};
    if token.is_empty() {
        return Err("Set the upload token under Options → Sharing first.".into());
    }
    let resp = gloo::net::http::Request::get(&format!("{}/s", base()))
        .header("Authorization", &format!("Bearer {token}"))
        .send()
        .await
        .map_err(|e| format!("reading the shares: {e}"))
        .and_then(checked)?;
    let listing: ddi_library::share::ShareListing =
        resp.json().await.map_err(|e| format!("the shares: {e}"))?;
    Ok(SharesView {
        state: SharesState::Ready,
        admin: listing.role == "admin",
        shares: listing
            .shares
            .into_iter()
            .map(|summary| ShareRow {
                link: link(&summary.id),
                summary,
            })
            .collect(),
        error: String::new(),
    })
}

/// Deletes share `id` (`DELETE <base>/s/<id>`; an uploader's own only).
pub(crate) async fn delete(token: &str, id: &str) -> Result<(), String> {
    gloo::net::http::Request::delete(&format!("{}/s/{id}", base()))
        .header("Authorization", &format!("Bearer {token}"))
        .send()
        .await
        .map_err(|e| format!("deleting the share: {e}"))
        .and_then(checked)
        .map(|_| ())
}

thread_local! {
    /// The shared files coming down now: `(name, bytes so far, bytes in
    /// all)`, for the prompt's quiet line.
    static DOWNLOADS: RefCell<Vec<(String, f64, f64)>> = const { RefCell::new(Vec::new()) };
}

/// A shared file's download moved (`songs::fetch_bytes_counted`).
pub(crate) fn note_download(name: &str, done: f64, total: f64) {
    DOWNLOADS.with_borrow_mut(|d| match d.iter_mut().find(|(n, _, _)| n == name) {
        Some(entry) => {
            entry.1 = done;
            entry.2 = total;
        }
        None => d.push((name.to_string(), done, total)),
    });
}

pub(crate) fn download_done(name: &str) {
    DOWNLOADS.with_borrow_mut(|d| d.retain(|(n, _, _)| n != name));
}

/// What the prompt says of the shared files coming down, if any:
/// "downloading the song… 2.1 of 4.1 MB".
pub(crate) fn download_line() -> Option<String> {
    DOWNLOADS.with_borrow(|d| {
        if d.is_empty() {
            return None;
        }
        let done: f64 = d.iter().map(|(_, b, _)| b).sum();
        let total: f64 = d.iter().map(|(_, _, t)| t).sum();
        Some(if total > 0.0 {
            format!(
                "downloading the song… {:.1} of {:.1} MB",
                done / 1e6,
                total / 1e6
            )
        } else {
            format!("downloading the song… {:.1} MB", done / 1e6)
        })
    })
}

/// The link to a share: this page's address at `#/s/<id>`.
pub(crate) fn link(id: &str) -> String {
    let page = web_sys::window()
        .and_then(|w| {
            let l = w.location();
            Some(format!("{}{}", l.origin().ok()?, l.pathname().ok()?))
        })
        .unwrap_or_default();
    format!("{page}#/s/{id}")
}

/// `window.__DDI_SHARE_UPLOAD`, for tests: `{state, doneSongs, songs,
/// doneBytes, bytes, link, error}`.
fn publish_upload(v: &ShareView) {
    let Some(w) = web_sys::window() else {
        return;
    };
    let o = js_sys::Object::new();
    let set = |k: &str, v: wasm_bindgen::JsValue| {
        let _ = js_sys::Reflect::set(&o, &k.into(), &v);
    };
    set(
        "state",
        match v.state {
            ShareState::Idle => "idle",
            ShareState::Uploading => "uploading",
            ShareState::Done => "done",
            ShareState::Failed => "failed",
        }
        .into(),
    );
    set("doneSongs", (v.done_songs as f64).into());
    set("songs", (v.songs as f64).into());
    set("doneBytes", v.done_bytes.into());
    set("bytes", v.bytes.into());
    set("link", v.link.as_str().into());
    set("error", v.error.as_str().into());
    let _ = js_sys::Reflect::set(&w, &"__DDI_SHARE_UPLOAD".into(), &o);
}

#[cfg(test)]
mod tests {
    use super::*;
    use wasm_bindgen_test::wasm_bindgen_test;

    #[wasm_bindgen_test]
    fn a_share_reads_from_the_workers_json_and_takes_its_songs() {
        let text = r#"{"pack":"A pack","expires_at":1900000000,"created_at":1700000000,
            "songs":[{"record":{"entry":{"id":"u-1","title":"T","artist":"A","chart":"a.ssc","music":"a.ogg"},
            "pack":"","folder":"folders/x","links":{"a.ogg":{"path":"a.ogg"}},
            "still":{"state":"no_movie"}},
            "files":[{"path":"a.ssc","bytes":10,"url":"https://w/s/abc12345/f/0/a.ssc"}]}]}"#;
        let share = from_json("abc12345", text).expect("a share");
        assert_eq!(share.id, "abc12345");
        assert_eq!(share.pack, "A pack");
        let r = &share.songs[0].record;
        assert_eq!(r.entry.id, "shared:abc12345:0");
        assert_eq!(r.pack, "A pack");
        assert!(r.folder.is_none() && r.links.is_empty() && r.still.is_none());
        assert_eq!(
            share.songs[0].files[0].url,
            "https://w/s/abc12345/f/0/a.ssc"
        );
        assert!(from_json("abc12345", "{").is_err());
    }
}
