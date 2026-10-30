//! Songs shared by link on desktop (`docs/plans/shared-songs.md`): a
//! share read from the server (`DDI_API_URL`, else the production one)
//! into the cache folder (`shares/<id>/<n>/`, each file at the path its
//! entry names it by, as a song folder), listed by song select in its
//! shared mode (`--shared <link or id>`) and played from there: the
//! chart, the music and the pictures come with the list, what the
//! background changes play comes when the play loads
//! ([`ensure_all`]). And the way up: the songs of the library a player
//! chose uploaded as a share ([`upload`], song select's Share… or
//! `--share`), with the token kept in the settings.
//!
//! HTTP through ureq, synchronous, on the threads that call: no runtime
//! in the game; TLS through the system's own stack and trust store.

use std::path::{Path, PathBuf};
use std::sync::Mutex;

use ddi_library::path::RelPathBuf;
use ddi_library::share::{self as contract, ShareDoc, ShareFileDoc, ShareSongDoc};
use ddi_ui::screens::data::{ShareState, ShareView};

use crate::library::LibSong;
use crate::play::load::Files;

/// The server's base URL: `DDI_API_URL`, else the production one (the
/// Worker's route beside the app's page).
pub fn base() -> String {
    std::env::var("DDI_API_URL")
        .ok()
        .filter(|s| !s.trim().is_empty())
        .unwrap_or_else(|| "https://sugoijan.dev/ditoditoinfinito/api".to_string())
        .trim()
        .trim_end_matches('/')
        .to_string()
}

/// The link to share `id`, as the web opens it.
pub fn link_of(id: &str) -> String {
    contract::link_of(&base(), id)
}

/// A share read, as the library lists it.
pub struct LoadedShare {
    pub id: String,
    pub pack: String,
    pub expires_at: Option<f64>,
    pub songs: Vec<LibSong>,
}

struct Current {
    id: String,
    doc: ShareDoc,
    root: PathBuf,
}

/// The share loaded last: its files' places, for a play's loading.
static CURRENT: Mutex<Option<Current>> = Mutex::new(None);

/// What is coming down now: songs done of all (the list's load), and
/// the file being fetched with its bytes so far of all.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Download {
    pub songs_done: usize,
    pub songs: usize,
    pub file: String,
    pub bytes_done: u64,
    pub bytes: u64,
}

static DOWNLOAD: Mutex<Option<Download>> = Mutex::new(None);

/// The download under way, if any.
pub fn download() -> Option<Download> {
    DOWNLOAD.lock().ok().and_then(|d| d.clone())
}

/// A quiet line on it: "downloading the song… 2.1 of 4.1 MB", or the
/// songs read so far of a share's.
pub fn download_line() -> Option<String> {
    let d = download()?;
    Some(if d.songs > 0 && d.bytes == 0 {
        format!("reading the share… {} of {} songs", d.songs_done, d.songs)
    } else {
        contract::download_line(d.bytes_done as f64, d.bytes as f64)
    })
}

fn note_download(f: impl FnOnce(&mut Download)) {
    if let Ok(mut d) = DOWNLOAD.lock() {
        f(d.get_or_insert_with(Default::default));
    }
}

fn download_over() {
    if let Ok(mut d) = DOWNLOAD.lock() {
        *d = None;
    }
}

fn agent() -> ureq::Agent {
    let tls = ureq::tls::TlsConfig::builder()
        .provider(ureq::tls::TlsProvider::NativeTls)
        .root_certs(ureq::tls::RootCerts::PlatformVerifier)
        .build();
    let config = ureq::Agent::config_builder()
        .tls_config(tls)
        .http_status_as_error(false)
        .timeout_connect(Some(std::time::Duration::from_secs(20)))
        .user_agent("ddi-desktop")
        .build();
    ureq::Agent::new_with_config(config)
}

fn http_err(what: &str, e: ureq::Error) -> String {
    format!("{what}: {e}")
}

/// Where share `id`'s files go.
fn share_root(id: &str) -> Result<PathBuf, String> {
    Ok(crate::paths::cache_dir()
        .ok_or("no cache folder")?
        .join("shares")
        .join(id))
}

/// The share `id` from the server: `404` and `410` worded as the web
/// says them.
pub fn fetch(id: &str) -> Result<ShareDoc, String> {
    if !contract::valid_id(id) {
        return Err(contract::GONE.into());
    }
    let url = format!("{}/s/{id}", base());
    let mut resp = agent()
        .get(&url)
        .call()
        .map_err(|e| http_err("reading the share", e))?;
    match resp.status().as_u16() {
        404 | 410 => return Err(contract::GONE.into()),
        200 => {}
        s => return Err(format!("reading the share: HTTP {s}")),
    }
    let text = resp
        .body_mut()
        .with_config()
        .limit(64 * 1024 * 1024)
        .read_to_string()
        .map_err(|e| format!("reading the share: {e}"))?;
    serde_json::from_str(&text).map_err(|e| format!("the share: {e}"))
}

/// Reads share `id` and brings its songs' charts, music and pictures
/// into the cache; the songs as the library lists them. `progress` is
/// called as it goes ([`download`] says where it is).
pub fn load(id: &str, progress: &dyn Fn()) -> Result<LoadedShare, String> {
    let doc = fetch(id)?;
    let root = share_root(id)?;
    let mut songs = Vec::new();
    let total = doc.songs.len();
    note_download(|d| {
        d.songs = total;
        d.songs_done = 0;
    });
    for (n, song) in doc.songs.iter().enumerate() {
        let Some((mut entry, pack)) = contract::record_entry(&song.record) else {
            continue;
        };
        entry.id = contract::song_id(id, n);
        let dir = root.join(n.to_string());
        for (stored, named) in contract::stored_files(&entry) {
            if contract::is_small(&stored) {
                let got = ensure_file(&doc, id, n, &stored, &named, &dir);
                if got.is_err() {
                    download_over();
                }
                got?;
            }
        }
        note_download(|d| d.songs_done = n + 1);
        progress();
        songs.push(LibSong {
            root: dir,
            zip: false,
            dir: RelPathBuf::new(),
            pack: if pack.is_empty() {
                doc.pack.clone()
            } else {
                pack
            },
            pack_banner: None,
            entry,
            warnings: Vec::new(),
            notes: Vec::new(),
            work: None,
        });
    }
    download_over();
    let loaded = LoadedShare {
        id: id.to_string(),
        pack: doc.pack.clone(),
        expires_at: doc.expires_at,
        songs,
    };
    if let Ok(mut c) = CURRENT.lock() {
        *c = Some(Current {
            id: id.to_string(),
            doc,
            root,
        });
    }
    Ok(loaded)
}

/// Everything else of shared song `id` (what its background changes
/// play) into the cache, before its play loads.
pub fn ensure_all(id: &str) -> Result<(), String> {
    let Some((share, n)) = contract::parse_song_id(id) else {
        return Ok(());
    };
    let (doc, root) = {
        let c = CURRENT.lock().map_err(|_| "the share's lock")?;
        let c = c
            .as_ref()
            .filter(|c| c.id == share)
            .ok_or("the share of this song is not loaded")?;
        (c.doc.clone(), c.root.clone())
    };
    let song = doc.songs.get(n).ok_or("no such song in the share")?;
    let Some((entry, _)) = contract::record_entry(&song.record) else {
        return Ok(());
    };
    let dir = root.join(n.to_string());
    for (stored, named) in contract::stored_files(&entry) {
        if !contract::is_small(&stored) {
            // A movie or an image missing on the server shows the song's
            // background instead, as a missing file does.
            if let Err(e) = ensure_file(&doc, share, n, &stored, &named, &dir) {
                eprintln!("share: {}: {e}", named);
            }
        }
    }
    download_over();
    Ok(())
}

/// File `stored` of song `n` at `dir/<named>`, fetched unless it is
/// there at its listed size.
fn ensure_file(
    doc: &ShareDoc,
    id: &str,
    n: usize,
    stored: &str,
    named: &str,
    dir: &Path,
) -> Result<(), String> {
    let rel = RelPathBuf::from(named);
    let rel = rel.normalize().unwrap_or(rel);
    let target = rel
        .to_path(dir)
        .ok_or_else(|| format!("{named}: a path that leaves its folder"))?;
    let listed = doc
        .songs
        .get(n)
        .and_then(|s| s.files.iter().find(|f| f.path == stored));
    if let (Ok(meta), Some(f)) = (std::fs::metadata(&target), listed)
        && meta.is_file()
        && f.bytes >= 0.0
        && meta.len() == f.bytes as u64
    {
        return Ok(());
    }
    let url = match listed {
        Some(f) if !f.url.is_empty() => contract::resolve_url(&base(), &f.url),
        _ => contract::file_url(&base(), id, n, stored),
    };
    let mut resp = agent()
        .get(&url)
        .call()
        .map_err(|e| http_err(&format!("fetching {named}"), e))?;
    if resp.status().as_u16() != 200 {
        return Err(format!("fetching {named}: HTTP {}", resp.status().as_u16()));
    }
    if let Some(parent) = target.parent() {
        std::fs::create_dir_all(parent).map_err(|e| format!("{}: {e}", parent.display()))?;
    }
    let tmp = target.with_extension(format!(
        "{}.{}.tmp",
        target
            .extension()
            .and_then(|e| e.to_str())
            .unwrap_or("part"),
        std::process::id()
    ));
    let total = resp
        .headers()
        .get("content-length")
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.parse::<u64>().ok())
        .unwrap_or(0);
    note_download(|d| {
        d.file = named.to_string();
        d.bytes_done = 0;
        d.bytes = total;
    });
    let written = (|| -> std::io::Result<()> {
        let mut file = std::fs::File::create(&tmp)?;
        let mut reader = resp.body_mut().with_config().limit(u64::MAX).reader();
        let mut buf = vec![0u8; 256 * 1024];
        let mut got = 0u64;
        loop {
            let n = std::io::Read::read(&mut reader, &mut buf)?;
            if n == 0 {
                break;
            }
            std::io::Write::write_all(&mut file, &buf[..n])?;
            got += n as u64;
            note_download(|d| d.bytes_done = got);
        }
        file.sync_all()?;
        std::fs::rename(&tmp, &target)
    })();
    note_download(|d| {
        d.file.clear();
        d.bytes_done = 0;
        d.bytes = 0;
    });
    if let Err(e) = written {
        let _ = std::fs::remove_file(&tmp);
        return Err(format!("writing {named}: {e}"));
    }
    Ok(())
}

/// One of a song's files for an upload: where its bytes come from.
enum Source {
    File(PathBuf, u64),
    Bytes(Vec<u8>),
}

impl Source {
    fn len(&self) -> u64 {
        match self {
            Source::File(_, n) => *n,
            Source::Bytes(b) => b.len() as u64,
        }
    }
}

/// Shares the library's songs `ids` as the pack `pack`, expiring after
/// `expires` (`1h`, `1d`, `1w`, `1m`, `never`), with `token`; the link
/// to the share. `progress` is told after each file.
pub fn upload(
    pack: &str,
    ids: &[String],
    expires: &str,
    token: &str,
    progress: &mut dyn FnMut(&ShareView),
) -> Result<String, String> {
    if token.trim().is_empty() {
        return Err(contract::NO_TOKEN.into());
    }
    if ids.is_empty() {
        return Err("Nothing to share.".into());
    }
    let mut view = ShareView {
        state: ShareState::Uploading,
        songs: ids.len(),
        ..ShareView::default()
    };
    // Every song's files first: their sizes make the share.
    let now_ms = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0.0, |d| d.as_millis() as f64);
    let mut songs = Vec::new();
    let mut sources: Vec<Vec<(String, Source)>> = Vec::new();
    for id in ids {
        let (files, entry) = Files::of(id).ok_or_else(|| format!("no song {id}"))?;
        let song_pack = crate::library::song(id).map(|s| s.pack).unwrap_or_default();
        let mut listed = Vec::new();
        let mut these = Vec::new();
        let mut bytes = 0.0;
        for (stored, named) in contract::stored_files(&entry) {
            let Some(source) = source_of(&files, &named) else {
                continue;
            };
            bytes += source.len() as f64;
            view.bytes += source.len() as f64;
            listed.push(ShareFileDoc {
                path: stored.clone(),
                bytes: source.len() as f64,
                url: String::new(),
            });
            these.push((stored, source));
        }
        if !these.iter().any(|(s, _)| *s == entry.chart.as_str()) {
            return Err(format!("{}: its chart could not be read", entry.title));
        }
        songs.push(ShareSongDoc {
            record: contract::record_of(&entry, &song_pack, now_ms, bytes),
            files: listed,
        });
        sources.push(these);
    }
    progress(&view);
    let doc = ShareDoc {
        pack: pack.to_string(),
        expires_at: None,
        created_at: 0.0,
        songs,
        pack_art: serde_json::Value::Bool(false),
    };
    let mut make = serde_json::to_value(&doc).map_err(|e| e.to_string())?;
    if let Some(o) = make.as_object_mut() {
        o.insert("expires".into(), expires.into());
    }
    let agent = agent();
    let base = base();
    let auth = format!("Bearer {token}");
    let body = serde_json::to_vec(&make).map_err(|e| e.to_string())?;
    let mut resp = agent
        .post(format!("{base}/s"))
        .header("Authorization", &auth)
        .header("Content-Type", "application/json")
        .send(body.as_slice())
        .map_err(|e| http_err("making the share", e))?;
    let status = resp.status().as_u16();
    if status != 201 {
        return Err(contract::refusal(
            status,
            resp.body_mut().read_to_string().ok().as_deref(),
        ));
    }
    let created: serde_json::Value = serde_json::from_str(
        &resp
            .body_mut()
            .read_to_string()
            .map_err(|e| format!("the share's id: {e}"))?,
    )
    .map_err(|e| format!("the share's id: {e}"))?;
    let id = created
        .get("id")
        .and_then(|v| v.as_str())
        .ok_or("the share's id")?
        .to_string();
    for (n, files) in sources.into_iter().enumerate() {
        for (stored, source) in files {
            let url = contract::file_url(&base, &id, n, &stored);
            let len = source.len();
            let req = agent
                .put(&url)
                .header("Authorization", &auth)
                .header("Content-Type", "application/octet-stream")
                .header("Content-Length", &len.to_string());
            let sent = match source {
                Source::File(path, _) => {
                    let mut file =
                        std::fs::File::open(&path).map_err(|e| format!("{stored}: {e}"))?;
                    req.send(ureq::SendBody::from_reader(&mut file))
                }
                Source::Bytes(b) => req.send(b.as_slice()),
            };
            let mut resp = sent.map_err(|e| http_err(&stored, e))?;
            let status = resp.status().as_u16();
            if status != 200 {
                return Err(format!(
                    "{stored}: {}",
                    contract::refusal(status, resp.body_mut().read_to_string().ok().as_deref())
                ));
            }
            view.done_bytes += len as f64;
            progress(&view);
        }
        view.done_songs += 1;
        progress(&view);
    }
    let mut resp = agent
        .post(format!("{base}/s/{id}/finish"))
        .header("Authorization", &auth)
        .send_empty()
        .map_err(|e| http_err("finishing the share", e))?;
    let status = resp.status().as_u16();
    if status != 200 {
        return Err(contract::refusal(
            status,
            resp.body_mut().read_to_string().ok().as_deref(),
        ));
    }
    Ok(link_of(&id))
}

/// The bytes of a song's file `named`: a folder's file by its path, a
/// zip's entry read; `None` when it is not there.
fn source_of(files: &Files, named: &str) -> Option<Source> {
    match files {
        Files::Dir { root, dir } => {
            let rel = dir.join(named);
            let rel = rel.normalize().unwrap_or(rel);
            let path = rel.to_path(root)?;
            let meta = std::fs::metadata(&path).ok()?;
            meta.is_file().then_some(Source::File(path, meta.len()))
        }
        Files::Zip { .. } => files.read(named).ok().map(Source::Bytes),
    }
}

/// The shares the token may see (`GET <base>/s`), each with its link.
pub fn list(token: &str) -> Result<ddi_ui::screens::data::SharesView, String> {
    if token.trim().is_empty() {
        return Err(contract::NO_TOKEN.into());
    }
    let mut resp = agent()
        .get(format!("{}/s", base()))
        .header("Authorization", &format!("Bearer {token}"))
        .call()
        .map_err(|e| http_err("reading the shares", e))?;
    let status = resp.status().as_u16();
    if status != 200 {
        return Err(contract::refusal(
            status,
            resp.body_mut().read_to_string().ok().as_deref(),
        ));
    }
    let text = resp
        .body_mut()
        .with_config()
        .limit(64 * 1024 * 1024)
        .read_to_string()
        .map_err(|e| format!("reading the shares: {e}"))?;
    let listing: contract::ShareListing =
        serde_json::from_str(&text).map_err(|e| format!("the shares: {e}"))?;
    Ok(ddi_ui::screens::data::SharesView::from_listing(
        listing, link_of,
    ))
}

/// Deletes share `id` (`DELETE <base>/s/<id>`; an uploader's own only).
pub fn delete(token: &str, id: &str) -> Result<(), String> {
    let mut resp = agent()
        .delete(format!("{}/s/{id}", base()))
        .header("Authorization", &format!("Bearer {token}"))
        .call()
        .map_err(|e| http_err("deleting the share", e))?;
    let status = resp.status().as_u16();
    if status != 200 {
        return Err(contract::refusal(
            status,
            resp.body_mut().read_to_string().ok().as_deref(),
        ));
    }
    Ok(())
}

/// The entry of a shared song, for the headless report.
pub fn describe(s: &LoadedShare, now: f64) -> String {
    format!(
        "share: {} “{}”, {} songs, {}",
        s.id,
        s.pack,
        s.songs.len(),
        contract::expires_in(s.expires_at, now).unwrap_or_else(|| "never expires".into())
    )
}
