//! Songs shared by link (`docs/plans/shared-songs.md`): a share read from
//! the private Worker at [`base`] by its id (`#/s/<id>`), held in memory
//! for the session and never in the library or IndexedDB; its songs are
//! the records the web stores imported songs as, their files reached by
//! URL. Several shares are kept at once (an online room's stages each
//! have one, M5d); [`current`] is the one loaded last (song select's
//! shared mode); a play of one of its songs, opened at its own address,
//! loads the share again by the id in the song's id ([`song_id`],
//! `shared:<id>:<n>`). Uploads go with the sharing token, or an online
//! room's upload grant for a stage's song ([`upload_room`]).

use std::cell::RefCell;
use std::rc::Rc;

use wasm_bindgen::JsCast;

use ddi_library::share::{Created, NewShare, ShareDoc, ShareFileDoc, ShareSongDoc};
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
        .unwrap_or_else(|| ddi_library::share::DEFAULT_BASE.to_string())
        .trim_end_matches('/')
        .to_string()
}

/// A share as the Worker answers `GET <base>/s/<id>`, its records as
/// the library stores them.
pub(crate) type Share = ShareDoc<ImportedSong>;

pub(crate) use ddi_library::share::ShareError;

thread_local! {
    /// The shares read this session, by id, and the one read last.
    static SHARES: RefCell<std::collections::BTreeMap<String, Rc<Share>>> =
        const { RefCell::new(std::collections::BTreeMap::new()) };
    static LAST: RefCell<Option<String>> = const { RefCell::new(None) };
}

/// The share loaded last, if any.
pub(crate) fn current() -> Option<Rc<Share>> {
    let id = LAST.with_borrow(Clone::clone)?;
    kept(&id)
}

/// Share `id`, if read this session.
fn kept(id: &str) -> Option<Rc<Share>> {
    SHARES.with_borrow(|m| m.get(id).cloned())
}

pub(crate) use ddi_library::share::{is_shared_id, parse_song_id, song_id, valid_id};

/// The URL of file `path` of the shared song `song_id`, as the share
/// lists it (relative to the API's base, as the Worker lists them, or
/// absolute; a file it does not list is asked for at the Worker's path
/// for it, which also serves what the uploader sent and the list left
/// out); `None` for a song of no share.
pub(crate) fn file_url(song_id: &str, path: &str) -> Option<String> {
    let (share, n) = parse_song_id(song_id)?;
    if let Some(s) = kept(share)
        && let Some(song) = s.songs.get(n)
        && let Some(f) = song
            .files
            .iter()
            .find(|f| f.path == path && !f.url.is_empty())
    {
        return Some(ddi_library::share::resolve_url(&base(), &f.url));
    }
    Some(ddi_library::share::file_url(&base(), share, n, path))
}

/// The size the share lists for file `path` of the shared song
/// `song_id`, if it lists it.
pub(crate) fn file_size(song_id: &str, path: &str) -> Option<u64> {
    let (share, n) = parse_song_id(song_id)?;
    let s = kept(share)?;
    let f = s.songs.get(n)?.files.iter().find(|f| f.path == path)?;
    (f.bytes >= 0.0).then_some(f.bytes as u64)
}

/// The share `id`, as loaded now or read from the Worker and kept as
/// the current one: each song's record renamed to its shared id, with
/// nothing of the library it came from (no kept folder, no links, no
/// opening still).
pub(crate) async fn load(id: &str) -> Result<Rc<Share>, ShareError> {
    if !valid_id(id) {
        return Err(ShareError::Gone { expired: false });
    }
    if let Some(s) = kept(id) {
        LAST.with_borrow_mut(|l| *l = Some(id.to_string()));
        return Ok(s);
    }
    publish(id, "loading", None);
    let url = format!("{}/s/{id}", base());
    let resp = gloo::net::http::Request::get(&url)
        .send()
        .await
        .map_err(ShareError::failed("reading the share"))?;
    let status = resp.status();
    let result = if let Some(gone) = ddi_library::share::gone_of(status) {
        Err(gone)
    } else if !resp.ok() {
        Err(ShareError::Status { status, body: None })
    } else {
        resp.text()
            .await
            .map_err(ShareError::failed("reading the share"))
            .and_then(|t| from_json(id, &t))
    };
    match &result {
        Ok(share) => {
            let share = share.clone();
            SHARES.with_borrow_mut(|m| m.insert(id.to_string(), share.clone()));
            LAST.with_borrow_mut(|l| *l = Some(id.to_string()));
            publish(id, "ready", Some(&share));
        }
        Err(e) => publish(
            id,
            match e {
                ShareError::Gone { expired: false } => "missing",
                ShareError::Gone { expired: true } => "expired",
                _ => "failed",
            },
            None,
        ),
    }
    result
}

/// A share from the Worker's JSON, its songs made this share's.
pub(crate) fn from_json(id: &str, text: &str) -> Result<Rc<Share>, ShareError> {
    let mut share: Share =
        serde_json::from_str(text).map_err(|e| ShareError::Answer(e.to_string()))?;
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
            ddi_library::share::expires_in(s.expires_at, now)
                .map_or(wasm_bindgen::JsValue::NULL, |t| t.into()),
        );
    }
    let _ = js_sys::Reflect::set(&w, &"__DDI_SHARE".into(), &o);
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
) -> Result<String, ShareError> {
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
            view.error = Some(e.clone());
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
) -> Result<String, ShareError> {
    if token.is_empty() {
        return Err(ShareError::NoToken);
    }
    if ids.is_empty() {
        return Err(ShareError::Nothing);
    }
    let gathered = gather_imported(pack, ids, view).await?;
    tell(view);
    let auth = format!("Bearer {token}");
    let id = send_share(pack, expires, gathered, &auth, false, view, tell).await?;
    Ok(link(&id))
}

/// A share's songs as they go up: each song's record and files, and the
/// pack's art.
struct Gathered {
    songs: Vec<ShareSongDoc<ImportedSong>>,
    blobs: Vec<Vec<(String, web_sys::Blob)>>,
    art: Option<web_sys::Blob>,
}

/// The imported songs `ids`: their records and files read from the
/// library (a linked song's from its folder), the sizes counted in
/// `view`.
async fn gather_imported(
    pack: &str,
    ids: &[String],
    view: &mut ShareView,
) -> Result<Gathered, ShareError> {
    let reading = "reading the library";
    let db = Db::open().await.map_err(ShareError::failed(reading))?;
    // Every song's record and files first: the sizes go in the request
    // that makes the share.
    let mut songs = Vec::new();
    let mut blobs: Vec<Vec<(String, web_sys::Blob)>> = Vec::new();
    for id in ids {
        let record = crate::songs::imported_record(&db, id)
            .await
            .map_err(|_| ShareError::NoChart(id.clone()))?;
        let prefix = crate::songs::file_key(id, "");
        let mut paths: Vec<String> = db
            .keys_with_prefix(idb::FILES, &prefix)
            .await
            .map_err(ShareError::failed(reading))?
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
                .map_err(|e| {
                    ShareError::of(
                        record.entry.title.clone(),
                        ShareError::of(path.clone(), ShareError::failed(reading)(e.message())),
                    )
                })?;
            let bytes = blob.size();
            view.bytes += bytes;
            files.push(ShareFileDoc {
                path: path.clone(),
                bytes,
                url: String::new(),
            });
            these.push((path, blob));
        }
        songs.push(ShareSongDoc { record, files });
        blobs.push(these);
    }
    let art_key = crate::songs::pack_art_key(pack);
    let art = db
        .get(idb::FILES, &art_key)
        .await
        .map_err(ShareError::failed(reading))?
        .and_then(|v| v.dyn_into::<web_sys::Blob>().ok());
    if let Some(a) = &art {
        view.bytes += a.size();
    }
    Ok(Gathered { songs, blobs, art })
}

/// A bundled song (`songs/<id>/`, the manifest's entry) as an imported
/// song's record and files would be (`ddi_library::share::stored_files`:
/// the paths a share keeps them under, read at the paths the entry names).
async fn gather_bundled(id: &str, view: &mut ShareView) -> Result<Gathered, ShareError> {
    let reading = "reading the bundled song";
    let manifest = crate::songs::load_manifest()
        .await
        .map_err(ShareError::failed(reading))?;
    let entry = manifest
        .songs
        .into_iter()
        .find(|e| e.id == id)
        .ok_or_else(|| ShareError::NoChart(id.to_string()))?;
    let mut files = Vec::new();
    let mut these = Vec::new();
    for (stored, named) in ddi_library::share::stored_files(&entry) {
        let blob = crate::songs::fetch_blob(&format!("songs/{id}/{named}"))
            .await
            .map_err(|e| ShareError::of(named.clone(), ShareError::failed(reading)(e)))?;
        view.bytes += blob.size();
        files.push(ShareFileDoc {
            path: stored.clone(),
            bytes: blob.size(),
            url: String::new(),
        });
        these.push((stored, blob));
    }
    let bytes = files.iter().map(|f| f.bytes).sum();
    let record: ImportedSong = serde_json::from_value(ddi_library::share::record_of(
        &entry,
        "",
        js_sys::Date::now(),
        bytes,
    ))
    .map_err(|e| ShareError::Answer(e.to_string()))?;
    Ok(Gathered {
        songs: vec![ShareSongDoc { record, files }],
        blobs: vec![these],
        art: None,
    })
}

/// `gathered` made a share at the server with `auth` (`Bearer <token>` or
/// `Grant <grant>`; `grant`: refusals read as a grant's), each file put,
/// the share finished; its id.
async fn send_share(
    pack: &str,
    expires: &str,
    gathered: Gathered,
    auth: &str,
    grant: bool,
    view: &mut ShareView,
    tell: &dyn Fn(&ShareView),
) -> Result<String, ShareError> {
    let Gathered { songs, blobs, art } = gathered;
    let new = NewShare {
        pack: pack.to_string(),
        expires: expires.to_string(),
        songs,
        pack_art: art.is_some(),
    };
    let base = base();
    let made = gloo::net::http::Request::post(&format!("{base}/s"))
        .header("Authorization", auth)
        .json(&new)
        .map_err(ShareError::failed("making the share"))?
        .send()
        .await
        .map_err(ShareError::failed("making the share"))?;
    let created: Created = checked(made, grant)
        .await?
        .json()
        .await
        .map_err(|e| ShareError::Answer(e.to_string()))?;
    let id = created.id;
    for (n, files) in blobs.into_iter().enumerate() {
        for (path, blob) in files {
            let url = ddi_library::share::file_url(&base, &id, n, &path);
            put(&url, auth, grant, &blob)
                .await
                .map_err(|e| ShareError::of(path.clone(), e))?;
            view.done_bytes += blob.size();
            tell(view);
        }
        view.done_songs += 1;
        tell(view);
    }
    if let Some(a) = art {
        put(&format!("{base}/s/{id}/art"), auth, grant, &a).await?;
        view.done_bytes += a.size();
        tell(view);
    }
    let finished = gloo::net::http::Request::post(&format!("{base}/s/{id}/finish"))
        .header("Authorization", auth)
        .send()
        .await
        .map_err(ShareError::failed("finishing the share"))?;
    checked(finished, grant).await?;
    Ok(id)
}

/// An online room's stage song (M5d) uploaded as the room's share under
/// the room's `grant`: the library song `song` (imported or bundled), its
/// expiry the room's (the server's to set). `progress` is told the bytes
/// sent of all; the share's id.
pub(crate) async fn upload_room(
    song: &str,
    grant: &str,
    progress: &dyn Fn(f64, f64),
) -> Result<String, ShareError> {
    let mut view = ShareView {
        state: ShareState::Uploading,
        songs: 1,
        ..ShareView::default()
    };
    let gathered = if crate::songs::is_imported_id(song) {
        gather_imported("", &[song.to_string()], &mut view).await?
    } else {
        gather_bundled(song, &mut view).await?
    };
    progress(0.0, view.bytes);
    let tell = |v: &ShareView| {
        publish_upload(v);
        progress(v.done_bytes, v.bytes);
    };
    let auth = format!("Grant {grant}");
    let result = send_share("", "1d", gathered, &auth, true, &mut view, &tell).await;
    view.state = if result.is_ok() {
        ShareState::Done
    } else {
        ShareState::Failed
    };
    view.error = result.as_ref().err().cloned();
    publish_upload(&view);
    result
}

/// One file's bytes to the Worker.
async fn put(url: &str, auth: &str, grant: bool, blob: &web_sys::Blob) -> Result<(), ShareError> {
    let r = gloo::net::http::Request::put(url)
        .header("Authorization", auth)
        .body(blob)
        .map_err(ShareError::failed("sending a file"))?
        .send()
        .await
        .map_err(ShareError::failed("sending a file"))?;
    checked(r, grant).await.map(|_| ())
}

/// A response that is not a success, as an error (a grant's refusals
/// with their code, `ddi_library::share::grant_refusal`).
async fn checked(
    r: gloo::net::http::Response,
    grant: bool,
) -> Result<gloo::net::http::Response, ShareError> {
    if r.ok() {
        return Ok(r);
    }
    let status = r.status();
    if grant {
        let body = r.text().await.ok();
        Err(ddi_library::share::grant_refusal(status, body.as_deref()))
    } else {
        Err(ddi_library::share::refusal(status, None))
    }
}

/// The shares the token may see (`GET <base>/s`), each with its link.
pub(crate) async fn list(token: &str) -> Result<ddi_ui::screens::data::SharesView, ShareError> {
    if token.is_empty() {
        return Err(ShareError::NoToken);
    }
    let resp = gloo::net::http::Request::get(&format!("{}/s", base()))
        .header("Authorization", &format!("Bearer {token}"))
        .send()
        .await
        .map_err(ShareError::failed("reading the shares"))?;
    let resp = checked(resp, false).await?;
    let listing: ddi_library::share::ShareListing = resp
        .json()
        .await
        .map_err(|e| ShareError::Answer(e.to_string()))?;
    Ok(ddi_ui::screens::data::SharesView::from_listing(
        listing, link,
    ))
}

/// Deletes share `id` (`DELETE <base>/s/<id>`; an uploader's own only).
pub(crate) async fn delete(token: &str, id: &str) -> Result<(), ShareError> {
    let resp = gloo::net::http::Request::delete(&format!("{}/s/{id}", base()))
        .header("Authorization", &format!("Bearer {token}"))
        .send()
        .await
        .map_err(ShareError::failed("deleting the share"))?;
    checked(resp, false).await.map(|_| ())
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
        Some(ddi_ui::screens::prompt::download_line(done, total))
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
    set(
        "error",
        v.error
            .as_ref()
            .map_or(wasm_bindgen::JsValue::NULL, |e| e.to_string().into()),
    );
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
