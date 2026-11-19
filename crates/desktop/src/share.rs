//! Songs shared by link on desktop (`docs/plans/shared-songs.md`): a
//! share read from the server (`DDI_API_URL`, else the production one)
//! into the cache folder (`shares/<id>/<n>/song/`, each file at the path
//! its entry names it by, as a song folder: one its pack's folder,
//! `shares/<id>/<n>/`, holds what the entry names above it, a pack's
//! `../bn.png`), listed by song select in its
//! shared mode (`--shared <link or id>`) and played from there: the
//! chart, the music and the pictures come with the list, what the
//! background changes play comes when the play loads
//! ([`ensure_all`]). And the way up: the songs of the library a player
//! chose uploaded as a share ([`upload`], song select's Share… or
//! `--share`), with the token kept in the settings, or an online room's
//! stage song under the room's upload grant ([`upload_room`], M5d).
//! Several shares are kept at once (an online room's stages each have
//! one).
//!
//! HTTP through ureq, synchronous, on the threads that call: no runtime
//! in the game; TLS through the system's own stack and trust store.

use std::path::{Path, PathBuf};
use std::sync::Mutex;

use ddi_library::path::RelPathBuf;
use ddi_library::share::{
    self as contract, NewShare, ShareDoc, ShareError, ShareFileDoc, ShareSongDoc,
};
use ddi_ui::screens::data::{ShareState, ShareView};

use crate::library::LibSong;
use crate::play::load::Files;

/// The server's base URL set for this run ([`set_base`]: the bot tool's
/// `--api`), else `DDI_API_URL`, else the production one (the Worker's
/// route beside the app's page).
pub fn base() -> String {
    if let Some(b) = BASE.get() {
        return b.clone();
    }
    std::env::var("DDI_API_URL")
        .ok()
        .filter(|s| !s.trim().is_empty())
        .unwrap_or_else(|| contract::DEFAULT_BASE.to_string())
        .trim()
        .trim_end_matches('/')
        .to_string()
}

static BASE: std::sync::OnceLock<String> = std::sync::OnceLock::new();

/// The server's base URL for the rest of the run (once; `crate::bots`).
pub fn set_base(url: &str) {
    let _ = BASE.set(url.trim().trim_end_matches('/').to_string());
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
    doc: ShareDoc,
    root: PathBuf,
}

/// The shares loaded this run, by id: their files' places, for a play's
/// loading.
static LOADED: Mutex<Vec<Current>> = Mutex::new(Vec::new());

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

/// Each share's download under way, by its id (review3 desktop-5: two
/// shares read at once, a room's resting song's and a stage's, each its
/// own line).
static DOWNLOAD: Mutex<std::collections::BTreeMap<String, Download>> =
    Mutex::new(std::collections::BTreeMap::new());

/// Share `share`'s download under way, if any.
pub fn download(share: &str) -> Option<Download> {
    DOWNLOAD.lock().ok().and_then(|d| d.get(share).cloned())
}

/// A quiet line on share `share`'s: "downloading the song… 2.1 of 4.1
/// MB", or the songs read so far of a share's.
pub fn download_line(share: &str) -> Option<String> {
    let d = download(share)?;
    Some(if d.songs > 0 && d.bytes == 0 {
        ddi_l10n::msg::prompt::share_reading(d.songs_done, d.songs).into()
    } else {
        ddi_ui::screens::prompt::download_line(d.bytes_done as f64, d.bytes as f64)
    })
}

/// [`download_line`] for shared song `song` (`shared:<id>:<n>`), none for
/// any other song.
pub fn song_download_line(song: &str) -> Option<String> {
    let (share, _) = contract::parse_song_id(song)?;
    download_line(share)
}

fn note_download(share: &str, f: impl FnOnce(&mut Download)) {
    if let Ok(mut d) = DOWNLOAD.lock() {
        f(d.entry(share.to_string()).or_default());
    }
}

fn download_over(share: &str) {
    if let Ok(mut d) = DOWNLOAD.lock() {
        d.remove(share);
    }
}

/// How long an answer may take to begin (its status and headers), and a
/// small answer's body to come whole (review3 desktop-2: a connection
/// opened and gone quiet must fail as unreachable, never hang the thread
/// waiting on it: a room's ticket then asked again, Private room's
/// creation said failed, a share's read failing with Retry).
pub const ANSWER_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(30);

/// A long-polled request's (desktop's sign-in attempt): the server waits
/// 25 s at most (`ApiRequest::long`), this is longer.
pub const LONG_POLL_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(45);

/// The slowest a file's body may come before its download fails: bytes
/// a second, past [`ANSWER_TIMEOUT`] ([`file_timeout`]).
const SLOWEST_BYTES_PER_S: u64 = 32 * 1024;

/// How long a file of `bytes` may take to come whole: a stalled download
/// fails, a slow one does not.
fn file_timeout(bytes: u64) -> std::time::Duration {
    ANSWER_TIMEOUT + std::time::Duration::from_secs(bytes / SLOWEST_BYTES_PER_S)
}

/// The HTTP client the share and the online rooms use (`crate::online`):
/// a connection in 20 s, an answer begun and a small body whole in
/// [`ANSWER_TIMEOUT`] (a file's body: [`file_timeout`]; an upload's
/// body sent: no limit).
pub(crate) fn agent() -> ureq::Agent {
    let tls = ureq::tls::TlsConfig::builder()
        .provider(ureq::tls::TlsProvider::NativeTls)
        .root_certs(ureq::tls::RootCerts::PlatformVerifier)
        .build();
    let config = ureq::Agent::config_builder()
        .tls_config(tls)
        .http_status_as_error(false)
        .timeout_connect(Some(std::time::Duration::from_secs(20)))
        .timeout_recv_response(Some(ANSWER_TIMEOUT))
        .timeout_recv_body(Some(ANSWER_TIMEOUT))
        .user_agent("ddi-desktop")
        .build();
    ureq::Agent::new_with_config(config)
}

/// A shared song whose share is not the one loaded (never one song
/// select lists).
fn not_loaded() -> ShareError {
    ShareError::failed("playing a shared song")("the share of this song is not loaded")
}

/// A shared song's folder inside its own (`shares/<id>/<n>/song/`): a
/// file the entry names above the song's folder (`../bn.png`, a pack's,
/// shared as `__up__/bn.png`, `contract::share_path`) has its place
/// beside it, as in the pack it came from.
const SONG_DIR: &str = "song";

/// Where share `id`'s files go: in an online room's store `store` (stage
/// 2 of the room's song list: `rooms/<store>/<id>/` in the cache folder,
/// deleted when the room is left, [`forget_store`]), else `shares/<id>/`.
fn share_root(store: Option<&str>, id: &str) -> Result<PathBuf, ShareError> {
    let cache = crate::paths::cache_dir()
        .ok_or_else(|| ShareError::failed("finding the cache folder")("no cache folder"))?;
    Ok(match store {
        Some(store) => cache.join(ROOMS_DIR).join(store_name(store)).join(id),
        None => cache.join("shares").join(id),
    })
}

/// The cache folder's folder of the rooms' stores.
const ROOMS_DIR: &str = "rooms";

/// A store's name as a folder's: its letters, digits, `-` and `_` (a
/// room's id, a bot's name after it).
fn store_name(store: &str) -> String {
    store
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() || c == '-' || c == '_' { c } else { '_' })
        .take(96)
        .collect()
}

/// Room store `store`'s folder (`rooms/<store>/` in the cache folder).
pub fn store_dir(store: &str) -> Option<PathBuf> {
    Some(
        crate::paths::cache_dir()?
            .join(ROOMS_DIR)
            .join(store_name(store)),
    )
}

/// Room store `store` emptied (the room left or ended): its folder
/// deleted, its shares forgotten as loaded.
pub fn forget_store(store: &str) {
    let Some(dir) = store_dir(store) else {
        return;
    };
    if let Ok(mut c) = LOADED.lock() {
        c.retain(|c| !c.root.starts_with(&dir));
    }
    match std::fs::remove_dir_all(&dir) {
        Ok(()) => eprintln!("room: the room's songs on this device deleted ({})", dir.display()),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
        Err(e) => eprintln!("room: deleting the room's songs ({}): {e}", dir.display()),
    }
}

/// Share `share` of room store `store` deleted (its song left the room's
/// list).
pub fn forget_in(store: &str, share: &str) {
    let Ok(root) = share_root(Some(store), share) else {
        return;
    };
    if let Ok(mut c) = LOADED.lock() {
        c.retain(|c| c.root != root);
    }
    if let Err(e) = std::fs::remove_dir_all(&root)
        && e.kind() != std::io::ErrorKind::NotFound
    {
        eprintln!("room: deleting a song of the room ({}): {e}", root.display());
    }
}

/// Every room's store left by a run that ended without leaving its room
/// (a crash, a quit in a room): deleted at the start of a run that opens
/// the window (the one app of the data folder, `crate::instance`).
pub fn clear_stores() {
    let Some(dir) = crate::paths::cache_dir().map(|c| c.join(ROOMS_DIR)) else {
        return;
    };
    match std::fs::remove_dir_all(&dir) {
        Ok(()) => eprintln!("room: songs of rooms left before deleted ({})", dir.display()),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
        Err(e) => eprintln!("room: deleting songs of rooms left before ({}): {e}", dir.display()),
    }
}

/// The bytes a folder's files take (the stores' sizes, for the log and
/// the tests).
pub fn folder_bytes(dir: &Path) -> u64 {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return 0;
    };
    entries
        .flatten()
        .map(|e| match e.file_type() {
            Ok(t) if t.is_dir() => folder_bytes(&e.path()),
            Ok(_) => e.metadata().map_or(0, |m| m.len()),
            Err(_) => 0,
        })
        .sum()
}

/// The share `id` from the server: `404` and `410` worded as the web
/// says them.
pub fn fetch(id: &str) -> Result<ShareDoc, ShareError> {
    if !contract::valid_id(id) {
        return Err(ShareError::Gone { expired: false });
    }
    let url = format!("{}/s/{id}", base());
    let mut resp = agent()
        .get(&url)
        .call()
        .map_err(ShareError::failed("reading the share"))?;
    let status = resp.status().as_u16();
    if let Some(gone) = contract::gone_of(status) {
        return Err(gone);
    }
    if status != 200 {
        return Err(ShareError::Status { status, body: None });
    }
    let text = resp
        .body_mut()
        .with_config()
        .limit(64 * 1024 * 1024)
        .read_to_string()
        .map_err(ShareError::failed("reading the share"))?;
    let mut doc: ShareDoc =
        serde_json::from_str(&text).map_err(|e| ShareError::Answer(e.to_string()))?;
    doc.id = id.to_string();
    Ok(doc)
}

/// Reads share `id` and brings its songs' charts, music and pictures
/// into the cache; the songs as the library lists them. `progress` is
/// called as it goes ([`download`] says where it is).
pub fn load(id: &str, progress: &dyn Fn()) -> Result<LoadedShare, ShareError> {
    load_in(None, id, progress)
}

/// [`load`] into room store `store` (where its songs downloaded ahead
/// are: what is there at its listed size is not fetched again).
pub fn load_in(
    store: Option<&str>,
    id: &str,
    progress: &dyn Fn(),
) -> Result<LoadedShare, ShareError> {
    load_with(store, id, progress, &|_, _| {}, &|| false)
}

/// [`load_in`] told the bytes so far of all its files (`bytes`), given up
/// where `stop` says so (between files and their chunks).
pub fn load_with(
    store: Option<&str>,
    id: &str,
    progress: &dyn Fn(),
    bytes: &dyn Fn(u64, u64),
    stop: &dyn Fn() -> bool,
) -> Result<LoadedShare, ShareError> {
    let doc = fetch(id)?;
    let root = share_root(store, id)?;
    let all_bytes: u64 = doc
        .songs
        .iter()
        .flat_map(|s| &s.files)
        .filter(|f| contract::is_small(&f.path))
        .map(|f| f.bytes.max(0.0) as u64)
        .sum();
    let done = std::cell::Cell::new(0u64);
    let mut songs = Vec::new();
    let total = doc.songs.len();
    note_download(id, |d| {
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
                if stop() {
                    download_over(id);
                    return Err(stopped());
                }
                let before = done.get();
                let got = ensure_file(&doc, id, n, &stored, &named, &dir, &|got| {
                    bytes(before + got, all_bytes);
                }, stop);
                if got.is_err() {
                    download_over(id);
                }
                done.set(before + got?);
                bytes(done.get(), all_bytes);
            }
        }
        note_download(id, |d| d.songs_done = n + 1);
        progress();
        songs.push(LibSong {
            root: dir,
            zip: false,
            dir: RelPathBuf::from(SONG_DIR),
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
    download_over(id);
    let loaded = LoadedShare {
        id: id.to_string(),
        pack: doc.pack.clone(),
        expires_at: doc.expires_at,
        songs,
    };
    if let Ok(mut c) = LOADED.lock() {
        c.retain(|c| c.doc.id != loaded.id);
        c.push(Current { doc, root });
    }
    Ok(loaded)
}

/// Everything else of shared song `id` (what its background changes
/// play) into the cache, before its play loads.
pub fn ensure_all(id: &str) -> Result<(), ShareError> {
    let Some((share, n)) = contract::parse_song_id(id) else {
        return Ok(());
    };
    let (doc, root) = {
        let c = LOADED.lock().map_err(|_| not_loaded())?;
        let c = c
            .iter()
            .find(|c| c.doc.id == share)
            .ok_or_else(not_loaded)?;
        (c.doc.clone(), c.root.clone())
    };
    let song = doc.songs.get(n).ok_or_else(not_loaded)?;
    let Some((entry, _)) = contract::record_entry(&song.record) else {
        return Ok(());
    };
    let dir = root.join(n.to_string());
    for (stored, named) in contract::stored_files(&entry) {
        if !contract::is_small(&stored) {
            // A movie or an image missing on the server shows the song's
            // background instead, as a missing file does.
            if let Err(e) = ensure_file(&doc, share, n, &stored, &named, &dir, &|_| {}, &|| false) {
                eprintln!("share: {}: {e}", named);
            }
        }
    }
    download_over(share);
    Ok(())
}

/// A download given up for the plan's stop (stage 2's downloads ahead).
fn stopped() -> ShareError {
    ShareError::failed("downloading ahead")("stopped")
}

/// File `stored` of song `n` at `dir/song/<named>` ([`SONG_DIR`]),
/// fetched (as the share lists it, `contract::share_path`) unless it is
/// there at its listed size; its size. `bytes` is told the bytes so far
/// of it; `stop` gives it up between its chunks.
#[allow(clippy::too_many_arguments)]
fn ensure_file(
    doc: &ShareDoc,
    id: &str,
    n: usize,
    stored: &str,
    named: &str,
    dir: &Path,
    on_bytes: &dyn Fn(u64),
    stop: &dyn Fn() -> bool,
) -> Result<u64, ShareError> {
    let stored = contract::share_path(stored);
    let stored = stored.as_str();
    let rel = RelPathBuf::from(SONG_DIR).join(named);
    let rel = rel.normalize().unwrap_or(rel);
    let target = rel.to_path(dir).ok_or_else(|| {
        ShareError::of(
            named,
            ShareError::Answer("a path that leaves its folder".into()),
        )
    })?;
    let listed = doc
        .songs
        .get(n)
        .and_then(|s| s.files.iter().find(|f| f.path == stored));
    if let (Ok(meta), Some(f)) = (std::fs::metadata(&target), listed)
        && meta.is_file()
        && f.bytes >= 0.0
        && meta.len() == f.bytes as u64
    {
        return Ok(meta.len());
    }
    let url = match listed {
        Some(f) if !f.url.is_empty() => contract::resolve_url(&base(), &f.url),
        _ => contract::file_url(&base(), id, n, stored),
    };
    let bytes = listed.map_or(0, |f| f.bytes.max(0.0) as u64);
    let mut resp = agent()
        .get(&url)
        .config()
        .timeout_recv_body(Some(file_timeout(bytes)))
        .build()
        .call()
        .map_err(|e| ShareError::of(named, ShareError::failed("fetching a file")(e)))?;
    if resp.status().as_u16() != 200 {
        return Err(ShareError::of(
            named,
            contract::refusal(resp.status().as_u16(), None),
        ));
    }
    if let Some(parent) = target.parent() {
        std::fs::create_dir_all(parent).map_err(|e| {
            ShareError::of(
                parent.display().to_string(),
                ShareError::failed("making a folder")(e),
            )
        })?;
    }
    // One of this process's, and of this download's: threads of one run
    // (the bot tool's players) may fetch the same file at once.
    static PARTS: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    let tmp = target.with_extension(format!(
        "{}.{}-{}.tmp",
        target
            .extension()
            .and_then(|e| e.to_str())
            .unwrap_or("part"),
        std::process::id(),
        PARTS.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
    ));
    let total = resp
        .headers()
        .get("content-length")
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.parse::<u64>().ok())
        .unwrap_or(0);
    note_download(id, |d| {
        d.file = named.to_string();
        d.bytes_done = 0;
        d.bytes = total;
    });
    let written = (|| -> std::io::Result<u64> {
        let mut file = std::fs::File::create(&tmp)?;
        let mut reader = resp.body_mut().with_config().limit(u64::MAX).reader();
        let mut buf = vec![0u8; 256 * 1024];
        let mut got = 0u64;
        loop {
            if stop() {
                return Err(std::io::Error::other("stopped"));
            }
            let n = std::io::Read::read(&mut reader, &mut buf)?;
            if n == 0 {
                break;
            }
            std::io::Write::write_all(&mut file, &buf[..n])?;
            got += n as u64;
            note_download(id, |d| d.bytes_done = got);
            on_bytes(got);
        }
        file.sync_all()?;
        std::fs::rename(&tmp, &target)?;
        Ok(got)
    })();
    note_download(id, |d| {
        d.file.clear();
        d.bytes_done = 0;
        d.bytes = 0;
    });
    match written {
        Ok(got) => Ok(got),
        Err(_) if stop() => {
            let _ = std::fs::remove_file(&tmp);
            Err(stopped())
        }
        Err(e) => {
            let _ = std::fs::remove_file(&tmp);
            Err(ShareError::of(
                named,
                ShareError::failed("writing a file")(e),
            ))
        }
    }
}

/// A listed song's share as stage 2's downloads ahead first read it: its
/// first song's entry (its shared id), its pack, its banner fetched into
/// room store `store` (its texture source there), and the bytes its song
/// would take (movies left out).
pub fn listing_in(store: &str, id: &str) -> Result<ddi_ui::state::room::Listing, ShareError> {
    let doc = fetch(id)?;
    let root = share_root(Some(store), id)?;
    let song = doc.songs.first().ok_or_else(not_loaded)?;
    let (mut entry, pack) = contract::record_entry(&song.record).ok_or_else(not_loaded)?;
    entry.id = contract::song_id(id, 0);
    let dir = root.join("0");
    let lib = LibSong {
        root: dir.clone(),
        zip: false,
        dir: RelPathBuf::from(SONG_DIR),
        pack: if pack.is_empty() { doc.pack.clone() } else { pack },
        pack_banner: None,
        entry: entry.clone(),
        warnings: Vec::new(),
        notes: Vec::new(),
        work: None,
    };
    let banner = match entry.banner.as_deref() {
        Some(b) => {
            let named = b.as_str().to_string();
            let stored = contract::stored_files(&entry)
                .into_iter()
                .find(|(_, n)| *n == named)
                .map(|(s, _)| s);
            match stored {
                Some(stored) => {
                    ensure_file(&doc, id, 0, &stored, &named, &dir, &|_| {}, &|| false)?;
                    Some(lib.source(b))
                }
                None => None,
            }
        }
        None => None,
    };
    let bytes = song
        .files
        .iter()
        .filter(|f| contract::is_small(&f.path))
        .map(|f| f.bytes.max(0.0) as u64)
        .sum();
    Ok(ddi_ui::state::room::Listing {
        entry,
        pack: lib.pack,
        banner,
        bytes,
    })
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
) -> Result<String, ShareError> {
    if token.trim().is_empty() {
        return Err(ShareError::NoToken);
    }
    if ids.is_empty() {
        return Err(ShareError::Nothing);
    }
    let mut view = ShareView {
        state: ShareState::Uploading,
        songs: ids.len(),
        ..ShareView::default()
    };
    let (songs, sources) = gather(ids, &mut view)?;
    progress(&view);
    let auth = format!("Bearer {token}");
    let id = send_share(
        pack, expires, songs, sources, &auth, false, &mut view, progress,
    )?;
    Ok(link_of(&id))
}

/// An online room's stage song (M5d), library song `song` (its own or a
/// bundled one), uploaded as the room's share under the room's `grant`
/// (its expiry the room's: the server's to set); `progress` is told the
/// bytes sent of all. The share's id.
pub fn upload_room(
    song: &str,
    grant: &str,
    progress: &mut dyn FnMut(f64, f64),
) -> Result<String, ShareError> {
    let mut view = ShareView {
        state: ShareState::Uploading,
        songs: 1,
        ..ShareView::default()
    };
    let (songs, sources) = gather(&[song.to_string()], &mut view)?;
    progress(0.0, view.bytes);
    let auth = format!("Grant {grant}");
    let mut tell = |v: &ShareView| progress(v.done_bytes, v.bytes);
    send_share("", "1d", songs, sources, &auth, true, &mut view, &mut tell)
}

type Songs = Vec<ShareSongDoc>;
type Sources = Vec<Vec<(String, Source)>>;

/// The library's songs `ids` as a share lists them: each song's record and
/// files by the paths a share keeps them under, their bytes' sources; the
/// sizes counted in `view`.
fn gather(ids: &[String], view: &mut ShareView) -> Result<(Songs, Sources), ShareError> {
    // Every song's files first: their sizes make the share.
    let now_ms = crate::clock::epoch_ms().floor();
    let mut songs = Vec::new();
    let mut sources: Sources = Vec::new();
    for id in ids {
        let (files, entry) = Files::of(id).ok_or_else(|| ShareError::NoChart(id.clone()))?;
        let song_pack = crate::library::song(id).map(|s| s.pack).unwrap_or_default();
        let (listed, these, bytes) = listing(&entry, &files)?;
        view.bytes += bytes;
        if !these
            .iter()
            .any(|(s, _)| *s == contract::share_path(entry.chart.as_str()))
        {
            return Err(ShareError::NoChart(entry.title));
        }
        songs.push(ShareSongDoc {
            record: contract::record_of(&entry, &song_pack, now_ms, bytes),
            files: listed,
        });
        sources.push(these);
    }
    Ok((songs, sources))
}

/// A song's files as a share lists them, their bytes' sources and their
/// bytes ([`listing`]).
type Listing = (Vec<ShareFileDoc>, Vec<(String, Source)>, f64);

/// A song's files as a share lists them, `entry` read from `files`: each
/// under the path the share keeps it at (`contract::share_path`: a file
/// the entry names above the song's folder, a pack's banner or music, at
/// `__up__/…`), its bytes' source, and their bytes; a path no share may
/// keep is refused here, said as what it is (the server would refuse the
/// whole share for it, which reads as "too large").
fn listing(
    entry: &ddi_library::manifest::ManifestEntry,
    files: &Files,
) -> Result<Listing, ShareError> {
    let mut listed = Vec::new();
    let mut these = Vec::new();
    let mut bytes = 0.0;
    for (stored, named) in contract::stored_files(entry) {
        let Some(source) = source_of(files, &named) else {
            continue;
        };
        let path = contract::share_path(&stored);
        contract::check_path(&path).map_err(|e| ShareError::of(entry.title.clone(), e))?;
        bytes += source.len() as f64;
        listed.push(ShareFileDoc {
            path: path.clone(),
            bytes: source.len() as f64,
            url: String::new(),
        });
        these.push((path, source));
    }
    Ok((listed, these, bytes))
}

/// `songs` made a share at the server with `auth` (`Bearer <token>` or
/// `Grant <grant>`: `grant`, refusals read as a grant's), each file put,
/// the share finished (`contract::Upload`); its id.
#[allow(clippy::too_many_arguments)]
fn send_share(
    pack: &str,
    expires: &str,
    songs: Songs,
    sources: Sources,
    auth: &str,
    grant: bool,
    view: &mut ShareView,
    progress: &mut dyn FnMut(&ShareView),
) -> Result<String, ShareError> {
    let new = NewShare {
        pack: pack.to_string(),
        expires: expires.to_string(),
        songs,
        pack_art: false,
    };
    let agent = agent();
    let mut up = contract::Upload::new(&base(), &new, None, auth.to_string(), grant)?;
    let mut sources: Vec<Vec<Option<Source>>> = sources
        .into_iter()
        .map(|files| files.into_iter().map(|(_, s)| Some(s)).collect())
        .collect();
    loop {
        let sent = match up.next() {
            contract::Step::Done(id) => return Ok(id.to_string()),
            contract::Step::Create { url, body } => agent
                .post(url)
                .header("Authorization", up.auth())
                .header("Content-Type", "application/json")
                .send(body.as_bytes()),
            contract::Step::Put {
                url,
                song,
                file,
                path,
            } => {
                let source = sources[song][file].take().expect("each file put once");
                let req = agent
                    .put(&url)
                    .header("Authorization", up.auth())
                    .header("Content-Type", "application/octet-stream")
                    .header("Content-Length", &source.len().to_string());
                match source {
                    Source::File(file_path, _) => {
                        let mut file = std::fs::File::open(&file_path).map_err(|e| {
                            ShareError::of(path, ShareError::failed("reading a file")(e))
                        })?;
                        req.send(ureq::SendBody::from_reader(&mut file))
                    }
                    Source::Bytes(b) => req.send(b.as_slice()),
                }
            }
            contract::Step::Art { url } => agent
                .put(url)
                .header("Authorization", up.auth())
                .send_empty(),
            contract::Step::Finish { url } => agent
                .post(url)
                .header("Authorization", up.auth())
                .send_empty(),
        };
        let mut resp = sent.map_err(|e| up.lost(e))?;
        let status = resp.status().as_u16();
        let body = resp.body_mut().read_to_string().ok();
        up.answered(status, body.as_deref(), &mut |told| {
            match told {
                contract::Told::Bytes(b) => view.done_bytes += b,
                contract::Told::Song => view.done_songs += 1,
            }
            progress(view);
        })?;
    }
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
pub fn list(token: &str) -> Result<ddi_ui::screens::data::SharesView, ShareError> {
    if token.trim().is_empty() {
        return Err(ShareError::NoToken);
    }
    let mut resp = agent()
        .get(format!("{}/s", base()))
        .header("Authorization", &format!("Bearer {token}"))
        .call()
        .map_err(ShareError::failed("reading the shares"))?;
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
        .map_err(ShareError::failed("reading the shares"))?;
    let listing: contract::ShareListing =
        serde_json::from_str(&text).map_err(|e| ShareError::Answer(e.to_string()))?;
    Ok(ddi_ui::screens::data::SharesView::from_listing(
        listing, link_of,
    ))
}

/// Deletes share `id` (`DELETE <base>/s/<id>`; an uploader's own only).
pub fn delete(token: &str, id: &str) -> Result<(), ShareError> {
    let mut resp = agent()
        .delete(format!("{}/s/{id}", base()))
        .header("Authorization", &format!("Bearer {token}"))
        .call()
        .map_err(ShareError::failed("deleting the share"))?;
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

#[cfg(test)]
mod tests {
    use super::*;

    /// review3 desktop-5: two shares read at once each keep their own
    /// download's line: one ending leaves the other's.
    #[test]
    fn two_downloads_at_once_keep_their_own_progress() {
        note_download("AAAAAAAAAA", |d| {
            d.bytes = 4_000_000;
            d.bytes_done = 1_000_000;
        });
        note_download("BBBBBBBBBB", |d| {
            d.bytes = 9_000_000;
            d.bytes_done = 2_000_000;
        });
        assert_eq!(download("AAAAAAAAAA").map(|d| d.bytes), Some(4_000_000));
        download_over("BBBBBBBBBB");
        assert_eq!(download("BBBBBBBBBB"), None);
        assert_eq!(download("AAAAAAAAAA").map(|d| d.bytes_done), Some(1_000_000));
        download_over("AAAAAAAAAA");
        assert_eq!(download("AAAAAAAAAA"), None);
    }

    /// support-2: a song read in place whose banner and music sit in its
    /// pack's folder (`#BANNER:../bn.png`) is listed under paths a share
    /// keeps (before: `../bn.png`, which the server refused as a whole,
    /// read as "too large"), each found where the entry names it; its
    /// reader puts them back beside the song's folder, in the share's own.
    #[test]
    fn a_song_with_files_in_its_pack_folder_is_listed_as_a_share_keeps_it() {
        let root = std::env::temp_dir().join(format!("ddi-share-pack-{}", std::process::id()));
        let song = root.join("Pack").join("Song");
        std::fs::create_dir_all(&song).unwrap();
        std::fs::write(song.join("song.sm"), "#TITLE:A;").unwrap();
        std::fs::write(root.join("Pack").join("bn.png"), [1, 2, 3]).unwrap();
        std::fs::write(root.join("Pack").join("music.ogg"), [4; 10]).unwrap();
        let entry: ddi_library::manifest::ManifestEntry =
            serde_json::from_value(serde_json::json!({
                "id": "a", "title": "A", "artist": "B",
                "chart": "song.sm", "music": "../music.ogg", "banner": "../bn.png",
                "charts": [],
            }))
            .unwrap();
        let files = Files::Dir {
            root: root.clone(),
            dir: RelPathBuf::from("Pack/Song"),
        };
        let (listed, these, bytes) = listing(&entry, &files).unwrap();
        let paths: Vec<&str> = listed.iter().map(|f| f.path.as_str()).collect();
        assert_eq!(paths, ["song.sm", "__up__/music.ogg", "__up__/bn.png"]);
        assert!(paths.iter().all(|p| contract::valid_path(p)));
        assert_eq!((these.len(), bytes), (3, 22.0));
        // Read back: the song's folder is `song/` in the share's, so its
        // pack's files have their place beside it.
        let rel = RelPathBuf::from(SONG_DIR).join("../bn.png");
        let rel = rel.normalize().unwrap();
        assert_eq!(rel.as_str(), "bn.png");
        let _ = std::fs::remove_dir_all(&root);
    }
}
