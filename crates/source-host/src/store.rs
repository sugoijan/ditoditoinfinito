//! Songs fetched without importing (`docs/plans/source-plugins.md`,
//! "Memory first"; decided 2026-11-25: "prioritize keeping them in memory
//! only and using known cache dirs when that's not possible/feasible").
//!
//! An item's files live in memory, each as one shared buffer, unless:
//!
//! - the item's files are over [`StoreLimits::item_memory`] (64 MiB): the
//!   whole item goes to the cache folder;
//! - it is a movie: the video decoders open movies by path, so movies are
//!   always in the cache folder;
//! - memory as a whole would pass [`StoreLimits::memory`] (256 MiB):
//!   items nothing holds are dropped first, least recently used first, to
//!   be fetched again on a press; items something holds (a play, a room:
//!   [`Hold`]) move to the cache folder instead;
//! - the plugin wrote them there itself (`cached`), or they are where they
//!   already were on the player's disk (`in_place`: taking neither).
//!
//! The cache folder is the app's cache's `sources/`, at most
//! [`StoreLimits::disk`] (2 GiB) for the run: past it, items nothing holds
//! are dropped, and a fetch that still does not fit fails as too large.
//! It is emptied when the store opens (a run that ended without its exit)
//! and when it is dropped.
//!
//! Stages 2 and 3 read songs from here: [`Store::song`] finds a fetched
//! song by its id (`src:<16 hex digits>:<n>`, never a library, bundled or
//! `shared:` id), [`ItemFiles`] reads its files as the library's scanner
//! and a play's loader read a folder's, and a play or a room keeps its item
//! with [`hold`] for as long as it needs it.

use std::collections::{BTreeMap, HashMap};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, Weak};

use ddi_library::manifest::ManifestEntry;
use ddi_library::path::{RelPath, RelPathBuf};
use ddi_library::report::Finding;
use ddi_library::scan::Reader;
use ddi_source_protocol::RoomRule;

/// The store as the host's threads and the game share it. Its lock is
/// held for bookkeeping only; files are written and read outside it.
pub type SharedStore = Arc<Mutex<Store>>;

/// The store's limits (decided 2026-11-26: "as proposed", to adjust
/// after use).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct StoreLimits {
    /// One item's files in memory, at most.
    pub item_memory: u64,
    /// Every item's files in memory together, at most.
    pub memory: u64,
    /// The cache folder's files together, at most, for the run.
    pub disk: u64,
}

impl Default for StoreLimits {
    fn default() -> StoreLimits {
        StoreLimits {
            item_memory: 64 << 20,
            memory: 256 << 20,
            disk: 2 << 30,
        }
    }
}

/// A fetched item in the store: the FNV-1a 64 of its plugin's id, its
/// item's id and its revision.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ItemKey(pub u64);

impl ItemKey {
    pub fn of(plugin: &str, item: &str, revision: Option<&str>) -> ItemKey {
        let mut h = ddi_library::pack::Fnv1a64::new();
        for part in [plugin, item, revision.unwrap_or("")] {
            h.write(&(part.len() as u64).to_le_bytes());
            h.write(part.as_bytes());
        }
        ItemKey(h.finish())
    }

    /// The id of its `n`th song.
    pub fn song_id(self, n: usize) -> String {
        format!("{SONG_ID_PREFIX}{:016x}:{n}", self.0)
    }
}

/// What every fetched song's id starts with.
pub const SONG_ID_PREFIX: &str = "src:";

/// Where an item came from.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Origin {
    pub plugin: String,
    pub item: String,
    pub revision: Option<String>,
    /// Whether its songs may be added to a room: the manifest's rule,
    /// narrowed by the item's metadata (stage 3 reads it).
    pub rooms: RoomRule,
}

/// One of an item's files.
#[derive(Clone, Debug)]
pub enum Stored {
    Memory(Arc<[u8]>),
    /// A file on disk: in the cache folder, or the player's own.
    Disk(PathBuf),
}

/// A song read from a fetched item.
#[derive(Clone, Debug, PartialEq)]
pub struct FetchedSong {
    /// `src:<key>:<n>`.
    pub id: String,
    pub key: ItemKey,
    /// Its entry (its id the one above), files relative to `dir`.
    pub entry: ManifestEntry,
    pub pack: String,
    /// Its folder inside the item.
    pub dir: RelPathBuf,
    /// The room list's key for it (`ddi_share_protocol::song_key`), where
    /// every original chart has a digest.
    pub song_key: Option<String>,
    /// What the scan found worth saying (`ddi_library::scan::Scanned`).
    pub warnings: Vec<Finding>,
}

/// An item's files, cheap to clone, read as a folder is
/// ([`ddi_library::scan::Reader`]).
#[derive(Clone, Debug, Default)]
pub struct ItemFiles(pub Arc<BTreeMap<RelPathBuf, Stored>>);

impl ItemFiles {
    /// Every file's path, sorted.
    pub fn paths(&self) -> Vec<RelPathBuf> {
        self.0.keys().cloned().collect()
    }

    /// Where a file is.
    pub fn get(&self, path: &RelPath) -> Option<&Stored> {
        self.0.get(path.normalize()?.as_rel_path())
    }
}

impl Reader for ItemFiles {
    fn read(&self, path: &RelPath) -> Result<Vec<u8>, String> {
        match self.get(path).ok_or_else(|| format!("{path}: not in the item"))? {
            Stored::Memory(bytes) => Ok(bytes.to_vec()),
            Stored::Disk(file) => std::fs::read(file).map_err(|e| format!("{path}: {e}")),
        }
    }

    fn read_at(&self, path: &RelPath, at: u64, len: usize) -> Result<Vec<u8>, String> {
        match self.get(path).ok_or_else(|| format!("{path}: not in the item"))? {
            Stored::Memory(bytes) => {
                let start = (at.min(bytes.len() as u64)) as usize;
                Ok(bytes[start..(start + len).min(bytes.len())].to_vec())
            }
            Stored::Disk(file) => {
                use std::io::{Read as _, Seek as _};
                let mut f = std::fs::File::open(file).map_err(|e| format!("{path}: {e}"))?;
                f.seek(std::io::SeekFrom::Start(at))
                    .map_err(|e| e.to_string())?;
                let mut out = Vec::with_capacity(len.min(1 << 20));
                f.take(len as u64)
                    .read_to_end(&mut out)
                    .map_err(|e| e.to_string())?;
                Ok(out)
            }
        }
    }
}

/// A fetched item, its files assembled and its songs read
/// ([`crate::assemble`]), before it enters the store.
#[derive(Debug)]
pub struct Assembled {
    pub files: BTreeMap<RelPathBuf, Stored>,
    /// Memory and cache bytes it takes (reserved in the store while it was
    /// assembled).
    pub memory: u64,
    pub disk: u64,
    /// Cache folders it owns (deleted with it).
    pub owned: Vec<PathBuf>,
}

struct Item {
    origin: Origin,
    files: ItemFiles,
    songs: Vec<FetchedSong>,
    holds: u32,
    used: u64,
    memory: u64,
    disk: u64,
    owned: Vec<PathBuf>,
    /// Changed whenever its files move, so a spill made outside the lock
    /// lands only on what it read.
    generation: u64,
}

/// How much the store holds.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Usage {
    pub items: usize,
    /// Bytes in memory, reserved for fetches under way included.
    pub memory: u64,
    /// Bytes in the cache folder, reserved included.
    pub disk: u64,
}

/// Why room could not be made in memory.
#[derive(Debug, PartialEq, Eq)]
pub enum NoRoom {
    /// Moving these held items to the cache folder would make it
    /// ([`spill`]).
    Spill(Vec<ItemKey>),
    /// Nothing can be moved: the bytes go to the cache folder.
    Full,
}

pub struct Store {
    root: PathBuf,
    limits: StoreLimits,
    items: HashMap<ItemKey, Item>,
    /// Every item's memory and disk bytes, and the reserved.
    memory: u64,
    disk: u64,
    tick: u64,
    folders: u64,
}

impl Store {
    /// The store in `root` (the cache's `sources/`), emptied of what an
    /// earlier run left.
    pub fn open(root: PathBuf, limits: StoreLimits) -> Store {
        let _ = std::fs::remove_dir_all(&root);
        Store {
            root,
            limits,
            items: HashMap::new(),
            memory: 0,
            disk: 0,
            tick: 0,
            folders: 0,
        }
    }

    pub fn shared(self) -> SharedStore {
        Arc::new(Mutex::new(self))
    }

    pub fn limits(&self) -> StoreLimits {
        self.limits
    }

    pub fn usage(&self) -> Usage {
        Usage {
            items: self.items.len(),
            memory: self.memory,
            disk: self.disk,
        }
    }

    /// A new folder of the cache's for a fetch or a spill (not made yet).
    pub fn new_folder(&mut self, what: &str) -> PathBuf {
        self.folders += 1;
        self.root.join(format!("{what}-{}", self.folders))
    }

    fn touch(&mut self) -> u64 {
        self.tick += 1;
        self.tick
    }

    /// Reserves `n` more bytes of memory, making room by dropping items
    /// nothing holds (least recently used first).
    pub fn reserve_memory(&mut self, n: u64) -> Result<(), NoRoom> {
        if self.memory + n <= self.limits.memory {
            self.memory += n;
            return Ok(());
        }
        let mut unheld: Vec<(u64, ItemKey)> = self
            .items
            .iter()
            .filter(|(_, i)| i.holds == 0 && i.memory > 0)
            .map(|(k, i)| (i.used, *k))
            .collect();
        unheld.sort_unstable();
        for (_, key) in unheld {
            if self.memory + n <= self.limits.memory {
                break;
            }
            self.drop_item(key);
        }
        if self.memory + n <= self.limits.memory {
            self.memory += n;
            return Ok(());
        }
        let mut held: Vec<(u64, ItemKey, u64)> = self
            .items
            .iter()
            .filter(|(_, i)| i.memory > 0)
            .map(|(k, i)| (i.used, *k, i.memory))
            .collect();
        held.sort_unstable();
        let mut freed = 0;
        let mut spill = Vec::new();
        for (_, key, bytes) in held {
            if self.memory + n - freed <= self.limits.memory {
                break;
            }
            freed += bytes;
            spill.push(key);
        }
        if spill.is_empty() || self.memory + n - freed > self.limits.memory {
            return Err(NoRoom::Full);
        }
        Err(NoRoom::Spill(spill))
    }

    /// Gives back memory reserved and not used.
    pub fn release_memory(&mut self, n: u64) {
        self.memory = self.memory.saturating_sub(n);
    }

    /// Reserves `n` more bytes of the cache folder, making room by
    /// dropping items nothing holds; `false` where it cannot.
    pub fn reserve_disk(&mut self, n: u64) -> bool {
        if self.disk + n > self.limits.disk {
            let mut unheld: Vec<(u64, ItemKey)> = self
                .items
                .iter()
                .filter(|(_, i)| i.holds == 0 && i.disk > 0)
                .map(|(k, i)| (i.used, *k))
                .collect();
            unheld.sort_unstable();
            for (_, key) in unheld {
                if self.disk + n <= self.limits.disk {
                    break;
                }
                self.drop_item(key);
            }
        }
        if self.disk + n > self.limits.disk {
            return false;
        }
        self.disk += n;
        true
    }

    /// Gives back cache bytes reserved and not used.
    pub fn release_disk(&mut self, n: u64) {
        self.disk = self.disk.saturating_sub(n);
    }

    fn drop_item(&mut self, key: ItemKey) {
        if let Some(item) = self.items.remove(&key) {
            self.memory = self.memory.saturating_sub(item.memory);
            self.disk = self.disk.saturating_sub(item.disk);
            for dir in item.owned {
                let _ = std::fs::remove_dir_all(dir);
            }
        }
    }

    /// Puts an assembled item in (its reservations becoming its own),
    /// replacing one of the same key.
    pub fn insert(
        &mut self,
        key: ItemKey,
        origin: Origin,
        assembled: Assembled,
        songs: Vec<FetchedSong>,
    ) {
        self.drop_item(key);
        let used = self.touch();
        self.items.insert(
            key,
            Item {
                origin,
                files: ItemFiles(Arc::new(assembled.files)),
                songs,
                holds: 0,
                used,
                memory: assembled.memory,
                disk: assembled.disk,
                owned: assembled.owned,
                generation: 0,
            },
        );
    }

    /// Whether `key` is in, its use noted.
    pub fn contains(&mut self, key: ItemKey) -> bool {
        let used = self.touch();
        self.items.get_mut(&key).map(|i| i.used = used).is_some()
    }

    /// An item's songs.
    pub fn songs(&self, key: ItemKey) -> Option<&[FetchedSong]> {
        self.items.get(&key).map(|i| i.songs.as_slice())
    }

    /// An item's files.
    pub fn files(&self, key: ItemKey) -> Option<ItemFiles> {
        self.items.get(&key).map(|i| i.files.clone())
    }

    /// Where an item came from.
    pub fn origin(&self, key: ItemKey) -> Option<&Origin> {
        self.items.get(&key).map(|i| &i.origin)
    }

    /// A fetched song by its id, with its item's files.
    pub fn song(&mut self, id: &str) -> Option<(FetchedSong, ItemFiles)> {
        let hex = id.strip_prefix(SONG_ID_PREFIX)?.split(':').next()?;
        let key = ItemKey(u64::from_str_radix(hex, 16).ok()?);
        let used = self.touch();
        let item = self.items.get_mut(&key)?;
        item.used = used;
        let song = item.songs.iter().find(|s| s.id == id)?.clone();
        Some((song, item.files.clone()))
    }

    /// Whether something holds `key`.
    pub fn held(&self, key: ItemKey) -> bool {
        self.items.get(&key).is_some_and(|i| i.holds > 0)
    }

    /// Whether `key`'s files are all on disk.
    pub fn on_disk(&self, key: ItemKey) -> bool {
        self.items
            .get(&key)
            .is_some_and(|i| i.files.0.values().all(|s| matches!(s, Stored::Disk(_))))
    }
}

impl Drop for Store {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.root);
    }
}

/// Keeps an item in the store (in memory, or moved to the cache folder
/// under pressure, never dropped) until it is dropped.
#[derive(Debug)]
pub struct Hold {
    store: Weak<Mutex<Store>>,
    key: ItemKey,
}

impl Hold {
    pub fn key(&self) -> ItemKey {
        self.key
    }
}

impl Drop for Hold {
    fn drop(&mut self) {
        if let Some(store) = self.store.upgrade()
            && let Ok(mut s) = store.lock()
            && let Some(item) = s.items.get_mut(&self.key)
        {
            item.holds = item.holds.saturating_sub(1);
        }
    }
}

/// Holds `key` (a play, a room); `None` where it is not in the store.
pub fn hold(store: &SharedStore, key: ItemKey) -> Option<Hold> {
    let mut s = store.lock().ok()?;
    let used = s.touch();
    let item = s.items.get_mut(&key)?;
    item.holds += 1;
    item.used = used;
    Some(Hold {
        store: Arc::downgrade(store),
        key,
    })
}

/// Moves `key`'s files in memory to the cache folder, written outside
/// the lock; `false` where the cache folder has no room (it stays in
/// memory) or writing failed.
pub fn spill(store: &SharedStore, key: ItemKey) -> bool {
    let (dir, generation, memory_files) = {
        let Ok(mut s) = store.lock() else {
            return false;
        };
        let Some(item) = s.items.get(&key) else {
            return true;
        };
        let generation = item.generation;
        let files: Vec<(RelPathBuf, Arc<[u8]>)> = item
            .files
            .0
            .iter()
            .filter_map(|(p, s)| match s {
                Stored::Memory(b) => Some((p.clone(), b.clone())),
                Stored::Disk(_) => None,
            })
            .collect();
        let bytes: u64 = files.iter().map(|(_, b)| b.len() as u64).sum();
        if !s.reserve_disk(bytes) {
            return false;
        }
        (s.new_folder("spill"), generation, files)
    };
    let bytes: u64 = memory_files.iter().map(|(_, b)| b.len() as u64).sum();
    let written: Option<Vec<(RelPathBuf, PathBuf)>> = memory_files
        .iter()
        .map(|(path, data)| {
            let file = path.to_path(&dir)?;
            std::fs::create_dir_all(file.parent()?).ok()?;
            std::fs::write(&file, data).ok()?;
            Some((path.clone(), file))
        })
        .collect();
    let Ok(mut s) = store.lock() else {
        return false;
    };
    let lands = written.is_some()
        && s
            .items
            .get(&key)
            .is_some_and(|i| i.generation == generation);
    if !lands {
        s.release_disk(bytes);
        drop(s);
        let _ = std::fs::remove_dir_all(&dir);
        return written.is_some();
    }
    let item = s.items.get_mut(&key).expect("checked above");
    let mut files = (*item.files.0).clone();
    for (path, file) in written.unwrap_or_default() {
        files.insert(path, Stored::Disk(file));
    }
    item.files = ItemFiles(Arc::new(files));
    let freed = item.memory;
    item.memory = 0;
    item.disk += bytes;
    item.generation += 1;
    item.owned.push(dir);
    s.memory = s.memory.saturating_sub(freed);
    true
}

/// Reserves `n` bytes of memory for `store`, moving held items to the
/// cache folder as it must; `false` where memory cannot be had (the bytes
/// then go to the cache folder).
pub fn reserve_memory(store: &SharedStore, n: u64) -> bool {
    for _ in 0..3 {
        let outcome = match store.lock() {
            Ok(mut s) => s.reserve_memory(n),
            Err(_) => return false,
        };
        match outcome {
            Ok(()) => return true,
            Err(NoRoom::Full) => return false,
            Err(NoRoom::Spill(keys)) => {
                for key in keys {
                    if !spill(store, key) {
                        return false;
                    }
                }
            }
        }
    }
    false
}

/// Whether a path is somewhere the store may delete (inside its root).
pub fn inside(root: &Path, path: &Path) -> bool {
    path.starts_with(root) && path != root
}

#[cfg(test)]
mod tests {
    use super::*;

    fn store(name: &str, limits: StoreLimits) -> (SharedStore, PathBuf) {
        let root = std::env::temp_dir().join(format!("ddi-store-{name}-{}", std::process::id()));
        (Store::open(root.clone(), limits).shared(), root)
    }

    fn item(store: &SharedStore, n: u64, bytes: usize) -> ItemKey {
        let key = ItemKey::of("p", &n.to_string(), None);
        assert!(reserve_memory(store, bytes as u64), "room for item {n}");
        let mut files = BTreeMap::new();
        files.insert(RelPathBuf::from("song.ssc"), Stored::Memory(vec![n as u8; bytes].into()));
        store.lock().unwrap().insert(
            key,
            Origin {
                plugin: "p".into(),
                item: n.to_string(),
                revision: None,
                rooms: RoomRule::Upload,
            },
            Assembled {
                files,
                memory: bytes as u64,
                disk: 0,
                owned: vec![],
            },
            vec![],
        );
        key
    }

    const SMALL: StoreLimits = StoreLimits {
        item_memory: 100,
        memory: 250,
        disk: 300,
    };

    #[test]
    fn past_memory_the_least_recently_used_unheld_item_goes() {
        let (store, _) = store("lru", SMALL);
        let a = item(&store, 1, 100);
        let b = item(&store, 2, 100);
        assert!(store.lock().unwrap().contains(a)); // a used after b
        let c = item(&store, 3, 100);
        let s = store.lock().unwrap();
        assert!(s.files(a).is_some());
        assert!(s.files(b).is_none(), "b was the least recently used");
        assert!(s.files(c).is_some());
        assert_eq!(s.usage().memory, 200);
    }

    #[test]
    fn a_held_item_moves_to_the_cache_folder_instead_of_going() {
        let (store, root) = store("hold", SMALL);
        let a = item(&store, 1, 100);
        let b = item(&store, 2, 100);
        let hold_a = hold(&store, a).unwrap();
        let hold_b = hold(&store, b).unwrap();
        let c = item(&store, 3, 100);
        {
            let s = store.lock().unwrap();
            assert!(s.on_disk(a), "a, the least recently used, moved");
            assert!(!s.on_disk(b));
            assert!(s.files(c).is_some());
            assert_eq!(s.usage(), Usage { items: 3, memory: 200, disk: 100 });
            let files = s.files(a).unwrap();
            assert_eq!(files.read(RelPath::new("song.ssc")).unwrap(), vec![1u8; 100]);
            assert_eq!(files.read_at(RelPath::new("song.ssc"), 98, 10).unwrap(), vec![1u8; 2]);
        }
        assert!(root.read_dir().unwrap().count() > 0);
        drop((hold_a, hold_b));
        assert!(!store.lock().unwrap().held(a));
        drop(store);
        assert!(!root.exists(), "the cache folder is emptied with the store");
    }

    #[test]
    fn the_cache_folder_drops_unheld_items_then_refuses() {
        let (store, _) = store("disk", SMALL);
        let mut s = store.lock().unwrap();
        assert!(s.reserve_disk(200));
        assert!(!s.reserve_disk(200), "past the run's cap");
        s.release_disk(200);
        assert!(s.reserve_disk(300));
    }

    #[test]
    fn song_ids_are_their_own_and_found_by_id() {
        let key = ItemKey::of("plugin", "item", Some("r1"));
        assert_ne!(key, ItemKey::of("plugin", "item", Some("r2")));
        assert_ne!(ItemKey::of("ab", "c", None), ItemKey::of("a", "bc", None));
        let id = key.song_id(2);
        assert!(id.starts_with("src:") && id.ends_with(":2") && id.len() == 4 + 16 + 2);
        let (store, _) = store("ids", SMALL);
        assert!(store.lock().unwrap().song(&id).is_none());
        assert!(store.lock().unwrap().song("shared:x:1").is_none());
    }
}
