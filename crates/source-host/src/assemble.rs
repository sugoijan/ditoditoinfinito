//! A fetched item put together from what its plugin delivered
//! (`docs/plans/source-plugins.md`, "What a fetched item is"), then read
//! as songs by the library's own scanner. Everything here is song data
//! from outside: paths are checked before they name a file, sizes are
//! counted as bytes arrive (never trusted as announced), a zip is read by
//! the library's sans-IO reader with every entry's output bounded by its
//! declared size, and the store's limits hold throughout.

use std::collections::BTreeMap;
use std::io::{Read, Seek as _, SeekFrom, Write as _};
use std::path::{Path, PathBuf};

use ddi_library::path::{RelPath, RelPathBuf};
use ddi_library::report::Finding;
use ddi_library::zip::{self, Locate};
use ddi_source_protocol::check::path_problem;
use ddi_source_protocol::limits::{MAX_FILES, MAX_PATH_DEPTH, MAX_SONGS};
use ddi_source_protocol::{Deliver, Delivery};

use crate::store::{
    Assembled, FetchedSong, ItemFiles, ItemKey, SharedStore, Stored, reserve_memory,
};

/// The song folders of a fetched item that did not read, with why.
pub type Skipped = Vec<(RelPathBuf, Finding)>;

/// Why a delivery was not taken.
#[derive(Debug, PartialEq, Eq)]
pub enum Reject {
    /// The plugin broke the protocol (why, for a log): it is stopped.
    Violation(String),
    /// Past the store's limits.
    TooLarge,
    /// Nothing the game can read (why, data).
    Unreadable(String),
}

enum Place {
    Memory(Vec<u8>),
    Disk(PathBuf),
}

struct Part {
    size: u64,
    received: u64,
    done: bool,
    place: Place,
}

/// A fetch's files as they stream in (`File` frames).
pub struct Incoming {
    store: SharedStore,
    item_memory: u64,
    disk_limit: u64,
    parts: BTreeMap<String, Part>,
    /// Bytes reserved in the store: in memory, in the cache folder.
    memory: u64,
    disk: u64,
    folder: Option<PathBuf>,
    /// Past the item's memory: every file goes to the cache folder.
    on_disk: bool,
}

impl Incoming {
    pub fn new(store: SharedStore) -> Incoming {
        let limits = store.lock().map(|s| s.limits()).unwrap_or_default();
        Incoming {
            store,
            item_memory: limits.item_memory,
            disk_limit: limits.disk,
            parts: BTreeMap::new(),
            memory: 0,
            disk: 0,
            folder: None,
            on_disk: false,
        }
    }

    /// The bytes received so far.
    pub fn bytes(&self) -> u64 {
        self.parts.values().map(|p| p.received).sum()
    }

    fn folder(&mut self) -> Result<PathBuf, Reject> {
        if let Some(f) = &self.folder {
            return Ok(f.clone());
        }
        let folder = self
            .store
            .lock()
            .map_err(|_| Reject::TooLarge)?
            .new_folder("fetch");
        std::fs::create_dir_all(&folder).map_err(|e| Reject::Unreadable(e.to_string()))?;
        self.folder = Some(folder.clone());
        Ok(folder)
    }

    fn reserve_disk(&mut self, n: u64) -> Result<(), Reject> {
        let ok = self.store.lock().is_ok_and(|mut s| s.reserve_disk(n));
        if !ok {
            return Err(Reject::TooLarge);
        }
        self.disk += n;
        Ok(())
    }

    fn file_of(&mut self, path: &str) -> Result<PathBuf, Reject> {
        let folder = self.folder()?;
        let file = RelPath::new(path)
            .to_path(&folder)
            .ok_or_else(|| Reject::Violation("a path that leaves its item".into()))?;
        if let Some(dir) = file.parent() {
            std::fs::create_dir_all(dir).map_err(|e| Reject::Unreadable(e.to_string()))?;
        }
        Ok(file)
    }

    /// Every file held in memory moved to the cache folder.
    fn move_to_disk(&mut self) -> Result<(), Reject> {
        self.on_disk = true;
        let in_memory: Vec<String> = self
            .parts
            .iter()
            .filter(|(_, p)| matches!(p.place, Place::Memory(_)))
            .map(|(k, _)| k.clone())
            .collect();
        let bytes: u64 = self.memory;
        self.reserve_disk(bytes)?;
        for path in in_memory {
            let file = self.file_of(&path)?;
            let part = self.parts.get_mut(&path).expect("listed above");
            if let Place::Memory(data) = &part.place {
                std::fs::write(&file, data).map_err(|e| Reject::Unreadable(e.to_string()))?;
            }
            part.place = Place::Disk(file);
        }
        if let Ok(mut s) = self.store.lock() {
            s.release_memory(self.memory);
        }
        self.memory = 0;
        Ok(())
    }

    /// A `File` frame's chunk.
    pub fn chunk(
        &mut self,
        path: &str,
        size: u64,
        offset: u64,
        last: bool,
        bytes: &[u8],
    ) -> Result<(), Reject> {
        if let Some(p) = path_problem(path) {
            return Err(Reject::Violation(format!("file path: {p}")));
        }
        let len = bytes.len() as u64;
        let end = offset
            .checked_add(len)
            .ok_or_else(|| Reject::Violation("a chunk past any size".into()))?;
        if end > size || (last && end != size) {
            return Err(Reject::Violation(
                "a chunk past its file's size, or a last one short of it".into(),
            ));
        }
        if size > self.disk_limit {
            return Err(Reject::TooLarge);
        }
        let new = !self.parts.contains_key(path);
        if new {
            if self.parts.len() >= MAX_FILES {
                return Err(Reject::Violation(format!("more than {MAX_FILES} files")));
            }
            if offset != 0 {
                return Err(Reject::Violation("a file's first chunk not at its start".into()));
            }
            let place = if self.on_disk || ddi_library::video::is_movie(path) {
                Place::Disk(self.file_of(path)?)
            } else {
                Place::Memory(Vec::new())
            };
            self.parts.insert(
                path.to_string(),
                Part {
                    size,
                    received: 0,
                    done: false,
                    place,
                },
            );
        }
        let part = &self.parts[path];
        if part.done || part.size != size || part.received != offset {
            return Err(Reject::Violation(
                "a chunk out of order, after its file's last, or of another size".into(),
            ));
        }
        if matches!(part.place, Place::Memory(_)) {
            let fits = self.memory + len <= self.item_memory
                && reserve_memory(&self.store, len);
            if fits {
                self.memory += len;
            } else {
                self.move_to_disk()?;
            }
        }
        if matches!(self.parts[path].place, Place::Disk(_)) {
            self.reserve_disk(len)?;
        }
        let part = self.parts.get_mut(path).expect("inserted above");
        match &mut part.place {
            Place::Memory(data) => data.extend_from_slice(bytes),
            Place::Disk(file) => {
                let mut f = std::fs::OpenOptions::new()
                    .create(true)
                    .append(true)
                    .open(&*file)
                    .map_err(|e| Reject::Unreadable(e.to_string()))?;
                f.write_all(bytes)
                    .map_err(|e| Reject::Unreadable(e.to_string()))?;
            }
        }
        part.received = end;
        part.done = last;
        Ok(())
    }

    /// The fetch's end: the item's files as `delivery` says they were
    /// delivered (in answer to `deliver`).
    pub fn finish(
        mut self,
        delivery: &Delivery,
        deliver: &Deliver,
        legacy: ddi_library::legacy::Decoder,
    ) -> Result<Assembled, Reject> {
        let lied = |what: &str| Reject::Violation(format!("the fetch's {what} are not what was sent"));
        match (delivery, deliver) {
            (Delivery::Files { files, bytes }, _) => {
                let sent: u64 = self.parts.values().map(|p| p.size).sum();
                if self.parts.is_empty()
                    || self.parts.len() != *files as usize
                    || sent != *bytes
                    || self.parts.values().any(|p| !p.done)
                {
                    return Err(lied("files or bytes"));
                }
                Ok(self.take())
            }
            (Delivery::Archive { path, bytes }, _) => {
                let ok = self.parts.len() == 1
                    && self
                        .parts
                        .get(path)
                        .is_some_and(|p| p.done && p.size == *bytes);
                if !ok {
                    return Err(lied("archive and its bytes"));
                }
                if zip::archive_stem(path).is_none() {
                    return Err(Reject::Unreadable("not a zip".into()));
                }
                let mut whole = self.take();
                let (_, stored) = whole.files.pop_first().expect("one part");
                let result = match &stored {
                    Stored::Memory(data) => {
                        expand(&Mem(data), &self.store, legacy)
                    }
                    Stored::Disk(file) => {
                        let f = std::fs::File::open(file)
                            .map_err(|e| Reject::Unreadable(e.to_string()))?;
                        expand(&Disk::new(f)?, &self.store, legacy)
                    }
                };
                // The archive itself is let go.
                release(&self.store, whole.memory, whole.disk);
                for dir in whole.owned {
                    let _ = std::fs::remove_dir_all(dir);
                }
                result
            }
            (Delivery::Cached { files, bytes }, Deliver::CacheDir { path }) => {
                if !self.parts.is_empty() {
                    return Err(lied("cached files"));
                }
                let found = walk(path)?;
                let sum: u64 = found.iter().map(|(_, _, n)| n).sum();
                if found.is_empty() || found.len() != *files as usize || sum != *bytes {
                    let _ = std::fs::remove_dir_all(path);
                    return Err(lied("cached files or bytes"));
                }
                if !self.store.lock().is_ok_and(|mut s| s.reserve_disk(sum)) {
                    let _ = std::fs::remove_dir_all(path);
                    return Err(Reject::TooLarge);
                }
                Ok(Assembled {
                    files: found
                        .into_iter()
                        .map(|(rel, file, _)| (rel, Stored::Disk(file)))
                        .collect(),
                    memory: 0,
                    disk: sum,
                    owned: vec![path.clone()],
                })
            }
            (Delivery::InPlace { path }, Deliver::Memory { .. }) => {
                if !self.parts.is_empty() {
                    return Err(lied("files"));
                }
                if path.is_dir() {
                    let found = walk(path)?;
                    if found.is_empty() {
                        return Err(Reject::Unreadable("an empty folder".into()));
                    }
                    return Ok(Assembled {
                        files: found
                            .into_iter()
                            .map(|(rel, file, _)| (rel, Stored::Disk(file)))
                            .collect(),
                        memory: 0,
                        disk: 0,
                        owned: vec![],
                    });
                }
                let is_zip = path
                    .file_name()
                    .and_then(|n| n.to_str())
                    .is_some_and(|n| zip::archive_stem(n).is_some());
                if !is_zip || !path.is_file() {
                    return Err(Reject::Unreadable("neither a folder nor a zip".into()));
                }
                let f = std::fs::File::open(path).map_err(|e| Reject::Unreadable(e.to_string()))?;
                expand(&Disk::new(f)?, &self.store, legacy)
            }
            _ => Err(Reject::Violation(
                "a delivery the app did not ask for".into(),
            )),
        }
    }

    /// The parts as an item's files, their reservations handed over.
    fn take(&mut self) -> Assembled {
        let mut files = BTreeMap::new();
        for (path, part) in std::mem::take(&mut self.parts) {
            let stored = match part.place {
                Place::Memory(data) => Stored::Memory(data.into()),
                Place::Disk(file) => Stored::Disk(file),
            };
            files.insert(RelPathBuf::from(path), stored);
        }
        let assembled = Assembled {
            files,
            memory: self.memory,
            disk: self.disk,
            owned: self.folder.take().into_iter().collect(),
        };
        self.memory = 0;
        self.disk = 0;
        assembled
    }
}

fn release(store: &SharedStore, memory: u64, disk: u64) {
    if let Ok(mut s) = store.lock() {
        s.release_memory(memory);
        s.release_disk(disk);
    }
}

impl Drop for Incoming {
    /// A fetch that never finished gives back what it reserved.
    fn drop(&mut self) {
        release(&self.store, self.memory, self.disk);
        if let Some(folder) = self.folder.take() {
            let _ = std::fs::remove_dir_all(folder);
        }
    }
}

/// Bytes read at offsets: a zip in memory or in a file.
trait Source {
    fn len(&self) -> u64;
    fn read_at(&self, at: u64, len: u64) -> Result<Vec<u8>, Reject>;
    fn reader(&self, at: u64, len: u64) -> Result<Box<dyn Read + '_>, Reject>;
}

struct Mem<'a>(&'a [u8]);

impl Source for Mem<'_> {
    fn len(&self) -> u64 {
        self.0.len() as u64
    }
    fn read_at(&self, at: u64, len: u64) -> Result<Vec<u8>, Reject> {
        let end = at.checked_add(len).filter(|&e| e <= self.len());
        let end = end.ok_or_else(|| Reject::Unreadable("a zip's part past its end".into()))?;
        Ok(self.0[at as usize..end as usize].to_vec())
    }
    fn reader(&self, at: u64, len: u64) -> Result<Box<dyn Read + '_>, Reject> {
        let end = at.checked_add(len).filter(|&e| e <= self.len());
        let end = end.ok_or_else(|| Reject::Unreadable("a zip's entry past its end".into()))?;
        Ok(Box::new(&self.0[at as usize..end as usize]))
    }
}

struct Disk {
    file: std::sync::Mutex<std::fs::File>,
    len: u64,
}

impl Disk {
    fn new(file: std::fs::File) -> Result<Disk, Reject> {
        let len = file
            .metadata()
            .map_err(|e| Reject::Unreadable(e.to_string()))?
            .len();
        Ok(Disk {
            file: std::sync::Mutex::new(file),
            len,
        })
    }
}

impl Source for Disk {
    fn len(&self) -> u64 {
        self.len
    }
    fn read_at(&self, at: u64, len: u64) -> Result<Vec<u8>, Reject> {
        if at.checked_add(len).is_none_or(|e| e > self.len) {
            return Err(Reject::Unreadable("a zip's part past its end".into()));
        }
        let mut f = self.file.lock().map_err(|_| Reject::TooLarge)?;
        f.seek(SeekFrom::Start(at))
            .map_err(|e| Reject::Unreadable(e.to_string()))?;
        let mut out = Vec::with_capacity(len.min(1 << 20) as usize);
        (&mut *f)
            .take(len)
            .read_to_end(&mut out)
            .map_err(|e| Reject::Unreadable(e.to_string()))?;
        Ok(out)
    }
    fn reader(&self, at: u64, len: u64) -> Result<Box<dyn Read + '_>, Reject> {
        if at.checked_add(len).is_none_or(|e| e > self.len) {
            return Err(Reject::Unreadable("a zip's entry past its end".into()));
        }
        let mut f = self
            .file
            .lock()
            .map_err(|_| Reject::TooLarge)?
            .try_clone()
            .map_err(|e| Reject::Unreadable(e.to_string()))?;
        f.seek(SeekFrom::Start(at))
            .map_err(|e| Reject::Unreadable(e.to_string()))?;
        Ok(Box::new(std::io::BufReader::new(f.take(len))))
    }
}

/// A zip's entries as an item's files: in memory when their declared
/// sizes fit the item's budget (movies apart), else in the cache folder.
fn expand(
    source: &dyn Source,
    store: &SharedStore,
    legacy: ddi_library::legacy::Decoder,
) -> Result<Assembled, Reject> {
    let bad = |e: zip::ZipError| Reject::Unreadable(e.to_string());
    let len = source.len();
    let tail_len = zip::tail_len(len);
    let tail = source.read_at(len - tail_len, tail_len)?;
    let cd = match zip::locate_central_directory(&tail, len).map_err(bad)? {
        Locate::Found(cd) => cd,
        Locate::NeedZip64Record { offset, len } => {
            zip::parse_zip64_end_record(&source.read_at(offset, len)?).map_err(bad)?
        }
    };
    // A directory past `zip::MAX_DIRECTORY` was refused by the reader.
    let directory = source.read_at(cd.offset, cd.size)?;
    let mut entries = zip::parse_central_directory(&directory, &cd).map_err(bad)?;
    ddi_library::pack::apply_legacy_names(&mut entries, legacy);
    let mut named: BTreeMap<RelPathBuf, zip::ZipEntry> = BTreeMap::new();
    for e in entries {
        if e.is_dir || e.encrypted || ddi_library::pack::is_ignored(&e.name) {
            continue;
        }
        let Some(path) = RelPath::new(&e.name).normalize() else {
            continue;
        };
        if path_problem(path.as_str()).is_some() {
            continue;
        }
        named.insert(path, e);
    }
    if named.is_empty() {
        return Err(Reject::Unreadable("an empty zip".into()));
    }
    if named.len() > MAX_FILES {
        return Err(Reject::TooLarge);
    }
    let limits = store.lock().map_err(|_| Reject::TooLarge)?.limits();
    let small: u64 = named
        .iter()
        .filter(|(p, _)| !ddi_library::video::is_movie(p.as_str()))
        .map(|(_, e)| e.uncompressed_size)
        .fold(0u64, u64::saturating_add);
    let all: u64 = named
        .values()
        .map(|e| e.uncompressed_size)
        .fold(0u64, u64::saturating_add);
    let in_memory = small <= limits.item_memory && reserve_memory(store, small);
    let disk_bytes = if in_memory { all - small } else { all };
    if !store.lock().is_ok_and(|mut s| s.reserve_disk(disk_bytes)) {
        if in_memory {
            release(store, small, 0);
        }
        return Err(Reject::TooLarge);
    }
    let mut assembled = Assembled {
        files: BTreeMap::new(),
        memory: if in_memory { small } else { 0 },
        disk: disk_bytes,
        owned: Vec::new(),
    };
    let mut folder: Option<PathBuf> = None;
    let result = (|| {
        for (path, entry) in &named {
            let header = source.read_at(entry.local_header_offset, zip::LOCAL_HEADER_LEN)?;
            let at = zip::local_data_offset(&header, entry).map_err(bad)?;
            let mut input = source.reader(at, entry.compressed_size)?;
            if in_memory && !ddi_library::video::is_movie(path.as_str()) {
                let mut out = Vec::with_capacity(entry.uncompressed_size.min(16 << 20) as usize);
                zip::extract_to(entry, &mut input, &mut out).map_err(bad)?;
                assembled.files.insert(path.clone(), Stored::Memory(out.into()));
            } else {
                let dir = match &folder {
                    Some(f) => f.clone(),
                    None => {
                        let f = store.lock().map_err(|_| Reject::TooLarge)?.new_folder("zip");
                        assembled.owned.push(f.clone());
                        folder = Some(f.clone());
                        f
                    }
                };
                let file = path
                    .to_path(&dir)
                    .ok_or_else(|| Reject::Unreadable("a path that leaves its zip".into()))?;
                if let Some(parent) = file.parent() {
                    std::fs::create_dir_all(parent)
                        .map_err(|e| Reject::Unreadable(e.to_string()))?;
                }
                let mut out = std::io::BufWriter::new(
                    std::fs::File::create(&file).map_err(|e| Reject::Unreadable(e.to_string()))?,
                );
                zip::extract_to(entry, &mut input, &mut out).map_err(bad)?;
                out.flush().map_err(|e| Reject::Unreadable(e.to_string()))?;
                assembled.files.insert(path.clone(), Stored::Disk(file));
            }
        }
        Ok(())
    })();
    if let Err(e) = result {
        release(store, assembled.memory, assembled.disk);
        for dir in assembled.owned {
            let _ = std::fs::remove_dir_all(dir);
        }
        return Err(e);
    }
    Ok(assembled)
}

/// A folder's files, as paths relative to it, their files and sizes: at
/// most [`MAX_FILES`], [`MAX_PATH_DEPTH`] deep, links not followed, names
/// that are not plain UTF-8 file names passed over.
pub fn walk(root: &Path) -> Result<Vec<(RelPathBuf, PathBuf, u64)>, Reject> {
    let mut out = Vec::new();
    let mut stack = vec![(root.to_path_buf(), String::new(), 0usize)];
    while let Some((dir, prefix, depth)) = stack.pop() {
        let entries = std::fs::read_dir(&dir).map_err(|e| Reject::Unreadable(e.to_string()))?;
        for entry in entries.flatten() {
            let Ok(kind) = entry.file_type() else {
                continue;
            };
            let Some(name) = entry.file_name().to_str().map(str::to_string) else {
                continue;
            };
            let rel = if prefix.is_empty() {
                name
            } else {
                format!("{prefix}/{name}")
            };
            if path_problem(&rel).is_some() || ddi_library::pack::is_ignored(&rel) {
                continue;
            }
            if kind.is_dir() {
                if depth + 1 < MAX_PATH_DEPTH {
                    stack.push((entry.path(), rel, depth + 1));
                }
            } else if kind.is_file() {
                if out.len() >= MAX_FILES {
                    return Err(Reject::TooLarge);
                }
                let size = entry.metadata().map(|m| m.len()).unwrap_or(0);
                out.push((RelPathBuf::from(rel), entry.path(), size));
            }
        }
    }
    out.sort_by(|a, b| a.0.cmp(&b.0));
    Ok(out)
}

/// The songs in an item's files, read by the library's scanner as a
/// folder's are; their ids the item's own (`src:<key>:<n>`), at most
/// [`MAX_SONGS`]. `Err` where none could be read, with why for each
/// folder that failed.
pub fn read_songs(
    files: &ItemFiles,
    key: ItemKey,
    fallback_pack: &str,
    legacy: ddi_library::legacy::Decoder,
) -> Result<(Vec<FetchedSong>, Skipped), Skipped> {
    let paths = files.paths();
    let scan = ddi_library::scan::scan_root(files, &paths, fallback_pack, &[], legacy);
    if scan.songs.is_empty() {
        return Err(scan.failed);
    }
    let songs = scan
        .songs
        .into_iter()
        .take(MAX_SONGS)
        .enumerate()
        .map(|(n, s)| {
            let id = key.song_id(n);
            let digests: Option<Vec<String>> = s
                .entry
                .charts
                .iter()
                .filter(|c| c.derivation.is_none())
                .map(|c| c.digest.map(|d| d.to_string()))
                .collect();
            let song_key = digests
                .filter(|d| !d.is_empty())
                .map(|d| ddi_library::share::song_key(d.iter().map(String::as_str)));
            let mut entry = s.entry;
            entry.id = id.clone();
            FetchedSong {
                id,
                key,
                entry,
                pack: s.pack,
                dir: s.dir,
                song_key,
                warnings: s.warnings,
            }
        })
        .collect();
    Ok((songs, scan.failed))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::store::{Store, StoreLimits};
    use ddi_library::scan::Reader as _;

    const SSC: &str = "#TITLE:Test;#ARTIST:Someone;#MUSIC:song.ogg;#BPMS:0=120;\n#NOTEDATA:;#STEPSTYPE:dance-single;#DIFFICULTY:Hard;#METER:5;#NOTES:\n1000\n0100\n0010\n0001\n;\n";

    fn store(name: &str, limits: StoreLimits) -> SharedStore {
        let root = std::env::temp_dir().join(format!("ddi-assemble-{name}-{}", std::process::id()));
        Store::open(root, limits).shared()
    }

    fn send(incoming: &mut Incoming, path: &str, data: &[u8], chunk: usize) {
        let size = data.len() as u64;
        let mut at = 0;
        loop {
            let end = (at + chunk).min(data.len());
            let last = end == data.len();
            incoming
                .chunk(path, size, at as u64, last, &data[at..end])
                .unwrap();
            if last {
                break;
            }
            at = end;
        }
    }

    #[test]
    fn streamed_files_become_songs_with_their_own_ids_and_key() {
        let store = store("files", StoreLimits::default());
        let mut incoming = Incoming::new(store.clone());
        send(&mut incoming, "Pack/Song/song.ssc", SSC.as_bytes(), 7);
        send(&mut incoming, "Pack/Song/song.ogg", b"OggS....", 3);
        let assembled = incoming
            .finish(
                &Delivery::Files {
                    files: 2,
                    bytes: SSC.len() as u64 + 8,
                },
                &Deliver::Memory { budget: 1 << 20 },
                &ddi_library::legacy::none,
            )
            .unwrap();
        assert_eq!(assembled.disk, 0);
        assert_eq!(assembled.memory, SSC.len() as u64 + 8);
        let key = ItemKey::of("p", "i", None);
        let files = ItemFiles(std::sync::Arc::new(assembled.files));
        let (songs, failed) = read_songs(&files, key, "Fetched", &ddi_library::legacy::none).unwrap();
        assert!(failed.is_empty());
        assert_eq!(songs.len(), 1);
        assert_eq!(songs[0].id, key.song_id(0));
        assert_eq!(songs[0].entry.id, songs[0].id);
        assert_eq!(songs[0].entry.title, "Test");
        assert_eq!(songs[0].pack, "Pack");
        assert!(songs[0].song_key.as_deref().is_some_and(ddi_source_protocol::check::is_song_key));
    }

    #[test]
    fn lies_about_sizes_and_order_are_refused() {
        let store = store("lies", StoreLimits::default());
        let mut incoming = Incoming::new(store.clone());
        // Past its size; a last chunk short of it; out of order.
        assert!(matches!(incoming.chunk("a", 3, 0, false, b"abcd"), Err(Reject::Violation(_))));
        assert!(matches!(incoming.chunk("a", 9, 0, true, b"abcd"), Err(Reject::Violation(_))));
        incoming.chunk("b", 8, 0, false, b"abcd").unwrap();
        assert!(matches!(incoming.chunk("b", 8, 2, false, b"cd"), Err(Reject::Violation(_))));
        assert!(matches!(incoming.chunk("b", 9, 4, true, b"efgh"), Err(Reject::Violation(_))));
        assert!(matches!(incoming.chunk("../x", 1, 0, true, b"x"), Err(Reject::Violation(_))));
        incoming.chunk("b", 8, 4, true, b"efgh").unwrap();
        assert!(matches!(incoming.chunk("b", 8, 8, true, b""), Err(Reject::Violation(_))));
        // Counts that are not what was sent.
        let r = incoming.finish(
            &Delivery::Files { files: 1, bytes: 9 },
            &Deliver::Memory { budget: 1 << 20 },
            &ddi_library::legacy::none,
        );
        assert!(matches!(r, Err(Reject::Violation(_))));
        assert_eq!(store.lock().unwrap().usage().memory, 0, "reservations given back");
    }

    #[test]
    fn past_the_items_budget_the_files_go_to_the_cache_folder_and_movies_always() {
        let limits = StoreLimits {
            item_memory: 16,
            memory: 1 << 20,
            disk: 1 << 20,
        };
        let store = store("spill", limits);
        let mut incoming = Incoming::new(store.clone());
        send(&mut incoming, "bg.mp4", &[1; 4], 4);
        send(&mut incoming, "a.ssc", &[2; 10], 10);
        assert_eq!(store.lock().unwrap().usage().memory, 10);
        send(&mut incoming, "b.ogg", &[3; 10], 5);
        let usage = store.lock().unwrap().usage();
        assert_eq!((usage.memory, usage.disk), (0, 24));
        let a = incoming
            .finish(
                &Delivery::Files { files: 3, bytes: 24 },
                &Deliver::Memory { budget: 16 },
                &ddi_library::legacy::none,
            )
            .unwrap();
        assert!(a.files.values().all(|s| matches!(s, Stored::Disk(_))));
        let files = ItemFiles(std::sync::Arc::new(a.files));
        assert_eq!(files.read(RelPath::new("b.ogg")).unwrap(), vec![3; 10]);
    }

    #[test]
    fn past_the_cache_folders_cap_a_fetch_is_too_large() {
        let limits = StoreLimits {
            item_memory: 4,
            memory: 1 << 20,
            disk: 10,
        };
        let store = store("cap", limits);
        let mut incoming = Incoming::new(store.clone());
        assert_eq!(incoming.chunk("a.ogg", 11, 0, false, b"abc"), Err(Reject::TooLarge));
        assert_eq!(
            incoming.chunk("b.ogg", 10, 0, false, b"abcdef"),
            Ok(()),
            "past the item's memory: on disk"
        );
        assert_eq!(incoming.chunk("c.ogg", 10, 0, false, b"abcdef"), Err(Reject::TooLarge));
        drop(incoming);
        assert_eq!(store.lock().unwrap().usage().disk, 0);
    }

    #[test]
    fn a_zip_is_read_with_hostile_entries_passed_over_or_refused() {
        let store = store("zip", StoreLimits::default());
        let files: Vec<(&str, &[u8])> = vec![
            ("Song/song.ssc", SSC.as_bytes()),
            ("Song/song.ogg", b"OggS"),
            ("../../escape.txt", b"no"),
            ("/abs/x.txt", b"no"),
        ];
        let bytes = zip::write_stored(&files).unwrap();
        let mut incoming = Incoming::new(store.clone());
        send(&mut incoming, "pack.zip", &bytes, 100);
        let a = incoming
            .finish(
                &Delivery::Archive {
                    path: "pack.zip".into(),
                    bytes: bytes.len() as u64,
                },
                &Deliver::Memory { budget: 1 << 20 },
                &ddi_library::legacy::none,
            )
            .unwrap();
        let names: Vec<&str> = a.files.keys().map(|p| p.as_str()).collect();
        // `../../escape.txt` climbs out and is passed over; `/abs/x.txt`
        // reads as `abs/x.txt`, inside the item.
        assert_eq!(names, ["Song/song.ogg", "Song/song.ssc", "abs/x.txt"]);
        assert_eq!(store.lock().unwrap().usage().memory, a.memory);
        // A zip whose entry claims fewer bytes than it holds.
        let mut lying = zip::write_stored(&[("a.ssc", b"0123456789".as_slice())]).unwrap();
        // The central directory's uncompressed size (offset 24 from its
        // header) made 4.
        let cd = lying
            .windows(4)
            .position(|w| w == [0x50, 0x4b, 0x01, 0x02])
            .unwrap();
        lying[cd + 24..cd + 28].copy_from_slice(&4u32.to_le_bytes());
        let mut incoming = Incoming::new(store.clone());
        send(&mut incoming, "x.zip", &lying, 1000);
        let r = incoming.finish(
            &Delivery::Archive {
                path: "x.zip".into(),
                bytes: lying.len() as u64,
            },
            &Deliver::Memory { budget: 1 << 20 },
            &ddi_library::legacy::none,
        );
        assert!(matches!(r, Err(Reject::Unreadable(_))), "{r:?}");
        drop(a);
    }
}
