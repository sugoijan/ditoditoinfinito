//! Songs fetched from a source and played without importing them
//! (`docs/plans/source-plugins.md`, "Playing from memory: what
//! changes"): found by their ids (`src:<16 hex digits>:<n>`) where the
//! library finds a song ([`crate::library::song`]), their files read from
//! the store (`ddi_source_host::store`: in memory, or where they are for
//! This computer's songs, or in the cache folder where memory would not
//! do), their pictures as `mem:<item>!<path>` texture sources, a play
//! holding its item ([`Held`]) from its prompt until its results close.
//!
//! What a play of one keeps (the maintainer, 2026-11-27: "We should keep
//! the play even if the song isn't kept"): every play is kept, as any
//! play is, at every level of how far its song may go (a record is the
//! player's own presses and score, never the song): as the library's
//! song where the library has a song of the same chart (by its digest),
//! else by its `src:` id, the same the next time its item is fetched from
//! the same source at the same revision ([`keeps_as`]). Either way it
//! carries where the song came from ([`origin_of`],
//! `ddi_ui::state::history::PlayOrigin`), from which Find songs lists it
//! and fetches it again.

use std::path::PathBuf;
use std::rc::Rc;
use std::sync::{Mutex, OnceLock};

use ddi_library::path::RelPath;
use ddi_library::scan::Reader as _;
use ddi_source_host::store::{self, Hold, ItemFiles, ItemKey, SONG_ID_PREFIX, Stored};
use ddi_source_host::SharedStore;
use ddi_ui::state::history::{OriginChart, PlayOrigin};

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

/// How far fetched song `id` (or any song of item `key`) may go, as the
/// store keeps it (its source's level, lowered by its item's): `None`
/// where it is not a fetched song in the store.
pub fn reach(id: &str) -> Option<super::Reach> {
    let store = store()?;
    let mut s = store.lock().ok()?;
    let key = s.song(id)?.0.key;
    s.origin(key).map(|o| o.reach)
}

/// How far item `key`'s songs may go, as the store keeps it.
pub fn item_reach(key: ItemKey) -> Option<super::Reach> {
    store()?.lock().ok()?.origin(key).map(|o| o.reach)
}

/// The songs in the store an online room may be given (stage 3: My
/// songs' "Fetched this session"): those whose level allows rooms and
/// whose charts name them (`song_key`), the library's own (`library`, by
/// `song_key`) left out, by id.
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
        .filter(|(s, reach)| {
            reach.rooms() && s.song_key.as_ref().is_some_and(|k| !library.contains(k))
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

/// How a play of a song is kept in the play history ([`keeps_as`]).
#[derive(Clone, Debug, PartialEq)]
pub struct Kept {
    /// The song id it is kept under.
    pub song: String,
    /// Where its song came from: a found song's source and item.
    pub origin: Option<PlayOrigin>,
}

/// How a play of `song_id`'s chart `chart` of `song` is kept: any song
/// but a fetched one as itself; a fetched one as the library's song with
/// a chart of the same content (its digest) where there is one, else as
/// itself, by its `src:` id, with its origin either way (where the store
/// still has it).
pub fn keeps_as(song_id: &str, song: &ddi_chart::Song, chart: usize) -> Kept {
    if !is_fetched(song_id) {
        return Kept {
            song: song_id.to_string(),
            origin: None,
        };
    }
    let library = song
        .charts
        .get(chart)
        .and_then(|_| crate::library::find_by_digest(song.chart_digest(chart)))
        .filter(|id| !is_fetched(id));
    Kept {
        song: library.unwrap_or_else(|| song_id.to_string()),
        origin: origin_of(song_id, song, chart),
    }
}

/// Where fetched song `song_id` (its chart `chart` of `song`, as stored)
/// came from, as a play keeps it: its source, item and revision as the
/// store has them, which of the item's songs, its title and artist, the
/// chart played. `None` where the store does not have it.
pub fn origin_of(song_id: &str, song: &ddi_chart::Song, chart: usize) -> Option<PlayOrigin> {
    let store = store()?;
    let mut st = store.lock().ok()?;
    let (fetched, _) = st.song(song_id)?;
    let from = st.origin(fetched.key)?.clone();
    let n: u32 = song_id.rsplit(':').next()?.parse().ok()?;
    let c = song.charts.get(chart)?;
    let name = if c.name.is_empty() {
        c.description.clone()
    } else {
        c.name.clone()
    };
    PlayOrigin {
        source: from.plugin,
        item: from.item,
        revision: from.revision,
        song: n,
        title: fetched.entry.title.clone(),
        artist: fetched.entry.artist.clone(),
        chart: OriginChart {
            name,
            style: c.layout.clone(),
            difficulty: c.difficulty,
            meter: c.meter,
        },
    }
    .bounded()
}

/// A fetched song in the store with a chart of content `digest` (the
/// first by id): a play kept under another `src:` id (an earlier
/// revision whose chart this one kept) judged again, or replayed, on it.
pub fn find_by_digest(digest: ddi_chart::ChartDigest) -> Option<String> {
    let every = store()?.lock().ok()?.every_song();
    every
        .into_iter()
        .find(|(s, _)| s.entry.charts.iter().any(|c| c.digest == Some(digest)))
        .map(|(s, _)| s.id)
}

/// A song with a chart of content `digest`: the library's (the bundled
/// songs first), else one fetched in this run.
pub fn song_by_digest(digest: ddi_chart::ChartDigest) -> Option<String> {
    crate::library::find_by_digest(digest).or_else(|| find_by_digest(digest))
}
