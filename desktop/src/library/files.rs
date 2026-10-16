//! The library's files where they are: a folder walked into the listing
//! `ddi_library::pack` takes ('/'-separated paths relative to it, with
//! each file's size and time), and a zip read in place with the library's
//! sans-IO zip reader (a file and seeks, never loaded whole). Both are
//! `ddi_library::scan::Reader`s.

use std::collections::HashMap;
use std::fs::File;
use std::io::{Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};
use std::time::UNIX_EPOCH;

use ddi_library::pack;
use ddi_library::scan::Reader;
use ddi_library::zip::{self, Locate, ZipEntry};

/// How deep a walk goes below its root: a StepMania songs folder is
/// pack/song/files, and songs keep a few folders of their own
/// (`bg/`, `movies/`).
const MAX_DEPTH: usize = 6;

/// A file of a listing.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Listed {
    /// Relative to the root, '/'-separated.
    pub path: String,
    pub size: u64,
    /// Modification time, nanoseconds since the epoch (0: unknown).
    pub modified: u64,
}

/// Nanoseconds since the epoch of `meta`'s modification time.
pub fn modified(meta: &std::fs::Metadata) -> u64 {
    meta.modified()
        .ok()
        .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
        .map_or(0, |d| d.as_nanos() as u64)
}

/// A walk's files, and what it left out.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Walked {
    /// Sorted by path.
    pub listed: Vec<Listed>,
    /// `(path, why)`: names that are not Unicode, links leading out of
    /// the root.
    pub unread: Vec<(String, String)>,
}

/// Every file under `root`, as `pack` lists them: what it ignores left
/// out, not past [`MAX_DEPTH`]. Symbolic links are followed only within
/// the root, and a folder reached both as itself and through a link is
/// walked as itself (the real folder's path wins).
pub fn walk(root: &Path) -> Walked {
    let mut w = Walk {
        inside: root.canonicalize().ok(),
        seen: std::collections::HashSet::new(),
        links: Vec::new(),
        out: Walked::default(),
    };
    w.dir(root, "", 0);
    // Folders behind links, once every real folder is walked.
    while !w.links.is_empty() {
        for (path, prefix, depth) in std::mem::take(&mut w.links) {
            w.dir(&path, &prefix, depth);
        }
    }
    w.out.listed.sort_by(|a, b| a.path.cmp(&b.path));
    w.out.unread.sort();
    w.out
}

struct Walk {
    /// The root, resolved (links must stay under it).
    inside: Option<PathBuf>,
    /// Folders walked, resolved.
    seen: std::collections::HashSet<PathBuf>,
    /// Linked folders to walk after the real ones.
    links: Vec<(PathBuf, String, usize)>,
    out: Walked,
}

impl Walk {
    fn dir(&mut self, dir: &Path, prefix: &str, depth: usize) {
        if depth > MAX_DEPTH {
            return;
        }
        if let Ok(real) = dir.canonicalize()
            && !self.seen.insert(real)
        {
            return;
        }
        let Ok(entries) = std::fs::read_dir(dir) else {
            return;
        };
        for e in entries.flatten() {
            let raw = e.file_name();
            let Some(name) = raw.to_str().map(str::to_string) else {
                let shown = raw.to_string_lossy();
                self.out.unread.push((
                    join(prefix, &shown),
                    "its name is not Unicode, so no simfile can name it".into(),
                ));
                continue;
            };
            let path = join(prefix, &name);
            if pack::is_ignored(&path) {
                continue;
            }
            let link = e.file_type().is_ok_and(|t| t.is_symlink());
            if link {
                // Only links that stay inside the root.
                let target = e.path().canonicalize().ok();
                let inside = match (&target, &self.inside) {
                    (Some(t), Some(root)) => t.starts_with(root),
                    _ => false,
                };
                if !inside {
                    if target.is_some() {
                        self.out
                            .unread
                            .push((path, "a link to outside the folder".into()));
                    }
                    continue;
                }
            }
            let Ok(meta) = std::fs::metadata(e.path()) else {
                continue;
            };
            if meta.is_dir() {
                if link {
                    self.links.push((e.path(), path, depth + 1));
                } else {
                    self.dir(&e.path(), &path, depth + 1);
                }
            } else if meta.is_file() {
                self.out.listed.push(Listed {
                    path,
                    size: meta.len(),
                    modified: modified(&meta),
                });
            }
        }
    }
}

fn join(prefix: &str, name: &str) -> String {
    if prefix.is_empty() {
        name.to_string()
    } else {
        format!("{prefix}/{name}")
    }
}

/// Another reader's files under a prefix (the scan's names for a root's
/// files, `crate::library`'s `Root::prefix`).
pub struct Prefixed<'a> {
    pub inner: &'a dyn Reader,
    /// With its trailing `/`.
    pub prefix: String,
}

impl Prefixed<'_> {
    fn strip<'p>(&self, path: &'p str) -> Result<&'p str, String> {
        path.strip_prefix(self.prefix.as_str())
            .ok_or_else(|| format!("{path}: not in the folder"))
    }
}

impl Reader for Prefixed<'_> {
    fn read(&self, path: &str) -> Result<Vec<u8>, String> {
        self.inner.read(self.strip(&normalize(path))?)
    }

    fn read_at(&self, path: &str, at: u64, len: usize) -> Result<Vec<u8>, String> {
        self.inner.read_at(self.strip(&normalize(path))?, at, len)
    }
}

/// A folder's files.
pub struct DirReader {
    pub root: PathBuf,
}

impl DirReader {
    fn path(&self, path: &str) -> Result<PathBuf, String> {
        // Only the root's own files: `..` within it (a simfile reaching
        // its pack's other folders), never out of it.
        if Path::new(path).is_absolute() {
            return Err(format!("{path}: not in the folder"));
        }
        Ok(self.root.join(normalize(path)))
    }
}

impl Reader for DirReader {
    fn read(&self, path: &str) -> Result<Vec<u8>, String> {
        std::fs::read(self.path(path)?).map_err(|e| format!("{path}: {e}"))
    }

    fn read_at(&self, path: &str, at: u64, len: usize) -> Result<Vec<u8>, String> {
        let mut f = File::open(self.path(path)?).map_err(|e| format!("{path}: {e}"))?;
        f.seek(SeekFrom::Start(at)).map_err(|e| e.to_string())?;
        let mut out = Vec::with_capacity(len.min(1 << 20));
        f.take(len as u64)
            .read_to_end(&mut out)
            .map_err(|e| e.to_string())?;
        Ok(out)
    }
}

/// A zip's entries, read in place.
pub struct ZipReader {
    pub path: PathBuf,
    entries: HashMap<String, ZipEntry>,
    /// The entries' names, sorted (as the web sorts an import's paths).
    pub names: Vec<String>,
    /// `(name, why)`: entries that cannot be read (encrypted).
    pub unread: Vec<(String, String)>,
}

fn read_range(f: &mut File, at: u64, len: u64) -> Result<Vec<u8>, String> {
    f.seek(SeekFrom::Start(at)).map_err(|e| e.to_string())?;
    let mut out = Vec::with_capacity(len.min(64 << 20) as usize);
    f.take(len)
        .read_to_end(&mut out)
        .map_err(|e| e.to_string())?;
    Ok(out)
}

impl ZipReader {
    /// The zip at `path`: its central directory read, its names decoded
    /// as the web decodes them (UTF-8, else Shift-JIS when it reads as
    /// Japanese, else CP437).
    pub fn open(
        path: &Path,
        shift_jis: &dyn Fn(&[u8]) -> Option<String>,
    ) -> Result<ZipReader, String> {
        let mut f = File::open(path).map_err(|e| e.to_string())?;
        let len = f.metadata().map_err(|e| e.to_string())?.len();
        let tail_len = zip::tail_len(len);
        let tail = read_range(&mut f, len - tail_len, tail_len)?;
        let cd = match zip::locate_central_directory(&tail, len).map_err(|e| e.to_string())? {
            Locate::Found(cd) => cd,
            Locate::NeedZip64Record { offset, len } => {
                let record = read_range(&mut f, offset, len)?;
                zip::parse_zip64_end_record(&record).map_err(|e| e.to_string())?
            }
        };
        let dir = read_range(&mut f, cd.offset, cd.size)?;
        let mut list = zip::parse_central_directory(&dir, &cd).map_err(|e| e.to_string())?;
        // Legacy names decided for the whole archive.
        let legacy: Vec<usize> = (0..list.len())
            .filter(|&i| list[i].legacy_name.is_some())
            .collect();
        if !legacy.is_empty() {
            let raws: Vec<&[u8]> = legacy
                .iter()
                .filter_map(|&i| list[i].legacy_name.as_deref())
                .collect();
            let names = pack::decode_legacy_names(&raws, shift_jis);
            for (&i, name) in legacy.iter().zip(names) {
                list[i].name = name;
            }
        }
        let mut entries = HashMap::new();
        let mut unread = Vec::new();
        for e in list {
            if e.is_dir || pack::is_ignored(&e.name) {
                continue;
            }
            if e.encrypted {
                unread.push((e.name.clone(), "encrypted; not read".into()));
                continue;
            }
            // Two entries of one name: the last, as the web keeps it.
            entries.insert(e.name.clone(), e);
        }
        let mut names: Vec<String> = entries.keys().cloned().collect();
        names.sort();
        Ok(ZipReader {
            path: path.to_path_buf(),
            entries,
            names,
            unread,
        })
    }

    /// An entry's uncompressed size.
    pub fn size(&self, name: &str) -> Option<u64> {
        self.entries
            .get(&normalize(name))
            .map(|e| e.uncompressed_size)
    }
}

impl ZipReader {
    fn entry(&self, path: &str) -> Result<&ZipEntry, String> {
        self.entries
            .get(&normalize(path))
            .ok_or_else(|| format!("{path}: not in the zip"))
    }

    /// A stored entry's bytes as a range of the zip, `(offset, length)`
    /// (`None`: it is compressed).
    pub fn stored_range(&self, path: &str) -> Result<Option<(u64, u64)>, String> {
        let e = self.entry(path)?;
        if e.method != zip::Method::Stored {
            return Ok(None);
        }
        let (_, at) = self.data_at(e)?;
        Ok(Some((at, e.uncompressed_size)))
    }

    /// What tells an entry's content apart: its CRC-32 and size.
    pub fn content_id(&self, path: &str) -> Option<(u32, u64)> {
        let e = self.entry(path).ok()?;
        Some((e.crc32, e.uncompressed_size))
    }

    /// An entry decompressed into `out` in pieces (a movie: not held).
    pub fn extract_to(&self, path: &str, out: &mut impl std::io::Write) -> Result<(), String> {
        let e = self.entry(path)?;
        let (mut f, at) = self.data_at(e)?;
        f.seek(SeekFrom::Start(at)).map_err(|e| e.to_string())?;
        let mut input = std::io::BufReader::new(f.take(e.compressed_size));
        zip::extract_to(e, &mut input, out).map_err(|e| e.to_string())
    }

    /// The file and where the entry's data starts.
    fn data_at(&self, e: &ZipEntry) -> Result<(File, u64), String> {
        let mut f = File::open(&self.path).map_err(|e| e.to_string())?;
        let header = read_range(&mut f, e.local_header_offset, zip::LOCAL_HEADER_LEN)?;
        let at = zip::local_data_offset(&header, e).map_err(|e| e.to_string())?;
        Ok((f, at))
    }
}

impl Reader for ZipReader {
    fn read(&self, path: &str) -> Result<Vec<u8>, String> {
        let e = self.entry(path)?;
        let (mut f, at) = self.data_at(e)?;
        let data = read_range(&mut f, at, e.compressed_size)?;
        zip::extract(e, &data).map_err(|e| e.to_string())
    }

    /// A stored entry's range by a seek; a deflated one's by inflating
    /// only as far as the range (a movie's header, not the movie).
    fn read_at(&self, path: &str, at: u64, len: usize) -> Result<Vec<u8>, String> {
        let e = self.entry(path)?;
        let end = (at + len as u64).min(e.uncompressed_size);
        if at >= end {
            return Ok(Vec::new());
        }
        let (mut f, data) = self.data_at(e)?;
        match e.method {
            zip::Method::Stored => read_range(&mut f, data + at, end - at),
            zip::Method::Deflated => {
                // Compressed input in growing chunks until it gives the
                // range (the stream's ratio is not known ahead).
                let mut take = (end * 2).max(64 << 10);
                loop {
                    let whole = take >= e.compressed_size;
                    let input = read_range(&mut f, data, take.min(e.compressed_size))?;
                    let out = zip::inflate_prefix(&input, end as usize);
                    if out.len() as u64 >= end || whole {
                        let from = (at as usize).min(out.len());
                        return Ok(out[from..].to_vec());
                    }
                    take *= 4;
                }
            }
            zip::Method::Other(m) => Err(format!("{path}: compression method {m}")),
        }
    }
}

/// `a/b/../c` as `a/c`: zip entries have no folders to walk through.
pub fn normalize(path: &str) -> String {
    let mut parts: Vec<&str> = Vec::new();
    for c in path.split('/') {
        match c {
            "" | "." => {}
            ".." => {
                parts.pop();
            }
            c => parts.push(c),
        }
    }
    parts.join("/")
}

#[cfg(test)]
mod tests {
    use super::*;

    pub(crate) fn scratch(name: &str) -> PathBuf {
        let dir = std::env::temp_dir()
            .join("ddi-library-tests")
            .join(format!("{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn a_walk_lists_files_as_packs_name_them() {
        let root = scratch("walk");
        for p in [
            "Pack/Song/s.sm",
            "Pack/Song/m.ogg",
            "Pack/.DS_Store",
            "Pack/Song/s.sm.old",
        ] {
            let path = root.join(p);
            std::fs::create_dir_all(path.parent().unwrap()).unwrap();
            std::fs::write(&path, b"x").unwrap();
        }
        let got: Vec<String> = walk(&root).listed.into_iter().map(|l| l.path).collect();
        assert_eq!(got, vec!["Pack/Song/m.ogg", "Pack/Song/s.sm"]);
        let r = DirReader { root: root.clone() };
        assert_eq!(r.read("Pack/Song/s.sm").unwrap(), b"x");
        assert_eq!(r.read("Pack/Song/../Song/m.ogg").unwrap(), b"x");
        assert!(r.read("/etc/hosts").is_err());
        assert_eq!(normalize("a/b/../c/./d"), "a/c/d");
    }
}
