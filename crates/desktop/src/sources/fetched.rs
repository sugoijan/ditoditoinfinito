//! Songs fetched from a source and played without importing them
//! (`docs/plans/source-plugins.md`, "Playing from memory: what
//! changes"): found by their ids (`src:<16 hex digits>:<n>`) where the
//! library finds a song ([`crate::library::song`]), their files read from
//! the store (`ddi_source_host::store`: in memory, or where they are for
//! This computer's songs, or in the cache folder where memory would not
//! do), their pictures as `mem:<item>!<path>` texture sources, a play
//! holding its item ([`Held`]) from its prompt until its results close.
//!
//! What a play of one keeps (decided here, a question for the
//! maintainer): it is kept in the play history only where the library has
//! a song of the same chart (by its digest), and then as a play of that
//! song ([`keeps_as`]); else it is not kept at all (no history entry, no
//! best, no replay), so that no kept play ever names a song that may not
//! be found again.

use std::path::PathBuf;
use std::rc::Rc;
use std::sync::{Mutex, OnceLock};

use ddi_library::path::RelPath;
use ddi_library::scan::Reader as _;
use ddi_source_host::store::{self, Hold, ItemFiles, ItemKey, SONG_ID_PREFIX, Stored};
use ddi_source_host::SharedStore;

use crate::library::LibSong;

fn slot() -> &'static Mutex<Option<SharedStore>> {
    static STORE: OnceLock<Mutex<Option<SharedStore>>> = OnceLock::new();
    STORE.get_or_init(Default::default)
}

/// The store fetched songs are read from (the game's sources', or a
/// test's).
pub fn set_store(store: Option<SharedStore>) {
    if let Ok(mut s) = slot().lock() {
        *s = store;
    }
}

pub fn store() -> Option<SharedStore> {
    slot().lock().ok()?.clone()
}

/// Whether `id` names a fetched song.
pub fn is_fetched(id: &str) -> bool {
    id.starts_with(SONG_ID_PREFIX)
}

/// The fetched song `id` as the library lists a song: its entry, pack
/// and folder in its item; its files the store's ([`files`]).
pub fn song(id: &str) -> Option<LibSong> {
    let store = store()?;
    let (s, _) = store.lock().ok()?.song(id)?;
    Some(lib_song(s))
}

/// A fetched song as the library lists one.
fn lib_song(s: ddi_source_host::FetchedSong) -> LibSong {
    LibSong {
        root: PathBuf::new(),
        zip: false,
        dir: s.dir,
        pack: s.pack,
        pack_banner: None,
        entry: s.entry,
        warnings: s.warnings,
        notes: Vec::new(),
        work: None,
        fetched: Some(s.key.0),
    }
}

/// The songs in the store an online room may be given (stage 3: My
/// songs' "Fetched this session"): those whose source's rule allows
/// rooms and whose charts name them (`song_key`), the library's own
/// (`library`, by `song_key`) left out, by id.
pub fn for_rooms(library: &std::collections::HashSet<String>) -> Vec<LibSong> {
    let Some(store) = store() else {
        return Vec::new();
    };
    let every = match store.lock() {
        Ok(s) => s.every_song(),
        Err(_) => return Vec::new(),
    };
    every
        .into_iter()
        .filter(|(s, rule)| {
            *rule == ddi_source_host::protocol::RoomRule::Upload
                && s.song_key.as_ref().is_some_and(|k| !library.contains(k))
        })
        .map(|(s, _)| lib_song(s))
        .collect()
}

/// An item's files.
pub fn files(key: u64) -> Option<ItemFiles> {
    store()?.lock().ok()?.files(ItemKey(key))
}

/// A file of item `key` as a texture source.
pub fn source(key: u64, path: &RelPath) -> String {
    format!("mem:{key:016x}!{path}")
}

/// The bytes a `mem:` texture source names (`rest`: after `mem:`).
pub fn read_source(rest: &str) -> Option<Vec<u8>> {
    let (key, path) = rest.split_once('!')?;
    let key = u64::from_str_radix(key, 16).ok()?;
    let files = files(key)?;
    let path = RelPath::new(path);
    if let Some(Stored::Disk(file)) = files.get(path)
        && std::fs::metadata(file).ok()?.len() > crate::images::MAX_FILE
    {
        return None;
    }
    files.read(path).ok()
}

/// A file of an item where the system's decoders need a path (a movie,
/// a preview's music): its own on disk, else `None`.
pub fn disk_path(key: u64, path: &RelPath) -> Option<PathBuf> {
    match files(key)?.get(path)? {
        Stored::Disk(file) => Some(file.clone()),
        Stored::Memory(_) => None,
    }
}

/// An item kept in the store for as long as a play needs it (its prompt,
/// the play, its results).
#[derive(Clone)]
pub struct Held(Rc<Hold>);

impl Held {
    pub fn key(&self) -> u64 {
        self.0.key().0
    }
}

impl std::fmt::Debug for Held {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Held({:016x})", self.key())
    }
}

impl PartialEq for Held {
    fn eq(&self, other: &Held) -> bool {
        self.key() == other.key()
    }
}

/// Holds song `id`'s item, where it is a fetched song in the store.
pub fn hold(id: &str) -> Option<Held> {
    let store = store()?;
    let key = store.lock().ok()?.song(id)?.0.key;
    store::hold(&store, key).map(|h| Held(Rc::new(h)))
}

/// The song a play of `song_id`'s chart `chart` of `song` is kept as in
/// the play history: the song itself, for any song but a fetched one;
/// for a fetched one, the library's song with a chart of the same content
/// (its digest), else none (the play is not kept).
pub fn keeps_as(song_id: &str, song: &ddi_chart::Song, chart: usize) -> Option<String> {
    if !is_fetched(song_id) {
        return Some(song_id.to_string());
    }
    song.charts.get(chart)?;
    crate::library::find_by_digest(song.chart_digest(chart)).filter(|id| !is_fetched(id))
}
