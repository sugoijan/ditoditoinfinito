//! The library on disk (`docs/plans/desktop.md`, section 4, D7): the
//! player's song folders read in place, nothing copied.
//!
//! - **Folders** ([`folders`], desktop's own storage key, so the web's
//!   settings are untouched): a StepMania root (it has `Songs/`; its
//!   `RandomMovies/` and `SongMovies/` are the shared folders, its
//!   `NoteSkins/` its skins), a songs folder (its children are packs), one
//!   pack, or a zip. Zips lying in a songs folder are packs of their own,
//!   read in place.
//! - **Songs** are found and built as the web's import finds and builds
//!   them (`ddi_library::pack::find_songs`, `ddi_library::scan`), with the
//!   same ids (`pack::song_id`), so offsets and bests match the web's for
//!   the same pack. Two folders holding the same song: the first in the
//!   list wins; the rest are reported.
//! - **The scan** runs on its own thread. A cache in the cache folder
//!   (`library-v1.json`) lists the songs at once at the start; the scan
//!   then reads each folder again, parsing only songs whose folder
//!   listing (names, sizes, times) changed and zips whose size or time
//!   did, and the list follows. Deleting the cache is always safe.
//! - **Files** are read where they are: banners and backgrounds by path
//!   (`zip:<zip>!<entry>` for a zip's, `crate::images`), previews from
//!   the music's path (a zip's extracted once into the cache folder).

pub mod files;
#[cfg(danoni_importer)]
pub mod managed;
pub mod noteskins;
pub mod offer;

use ddi_library::path::{RelPath, RelPathBuf};

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, OnceLock, RwLock, mpsc};
use std::time::Instant;

use ddi_library::manifest::{Manifest, ManifestEntry};
use ddi_library::pack;
use ddi_library::scan::{self, Reader};
use ddi_platform::storage::{self, Area};
use serde::{Deserialize, Serialize};

use files::{DirReader, Listed, ZipReader};

/// The song folders, in order (desktop's own key).
pub const FOLDERS_KEY: &str = "ddi.desktop.folders.v1";
/// Packs and songs the player removed from the list (files kept).
pub const HIDDEN_KEY: &str = "ddi.desktop.hidden.v1";
/// The cache's file (in the cache folder) and version (12: pump single
/// and 5 keys converted to each other; 11: generated
/// center-free pump charts; 10: FFR levels
/// timed as R^3 times them, new digests; 9: the preview video; 8: FFR
/// levels;
/// 7: jackets, CD titles, lyrics, preview and charts' music, background
/// files named with `,` or `=`; 6: pack covers;
/// version 5 added generated onigiri-free charts).
pub const CACHE_FILE: &str = "library-v1.json";
const CACHE_VERSION: u32 = 12;

/// The song folders the settings list.
pub fn folders() -> Vec<PathBuf> {
    storage::load::<Vec<String>>(Area::Local, FOLDERS_KEY)
        .unwrap_or_default()
        .into_iter()
        .map(PathBuf::from)
        .collect()
}

/// This run's own folders (`--songs`), beside the settings'.
static EXTRA: std::sync::Mutex<Vec<PathBuf>> = std::sync::Mutex::new(Vec::new());

/// Folders read in this run besides the settings' (`--songs`).
pub fn set_extra_folders(extra: &[PathBuf]) {
    if let Ok(mut e) = EXTRA.lock() {
        *e = extra.to_vec();
    }
}

/// Every folder read: the settings', then this run's own.
pub fn all_folders() -> Vec<PathBuf> {
    let mut f = folders();
    if let Ok(extra) = EXTRA.lock() {
        for e in extra.iter() {
            if !f.contains(e) {
                f.push(e.clone());
            }
        }
    }
    f
}

pub fn set_folders(list: &[PathBuf]) {
    let v: Vec<String> = list.iter().map(|p| p.display().to_string()).collect();
    let _ = storage::save(Area::Local, FOLDERS_KEY, &v);
}

/// Whether `a` and `b` name the same pack: equal ignoring case, as song
/// select groups packs (`select_view::select_data`, by `str::to_lowercase`),
/// so "Ünïcode" and "üNÏCODE" are one pack here too.
pub fn same_pack(a: &str, b: &str) -> bool {
    a == b || a.to_lowercase() == b.to_lowercase()
}

/// What the player removed from the list: packs by name as song select
/// showed them (told apart as it tells them, [`same_pack`]), songs by id.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Hidden {
    #[serde(default)]
    pub packs: Vec<String>,
    #[serde(default)]
    pub songs: Vec<String>,
}

impl Hidden {
    pub fn load() -> Hidden {
        storage::load(Area::Local, HIDDEN_KEY).unwrap_or_default()
    }

    pub fn save(&self) {
        let _ = storage::save(Area::Local, HIDDEN_KEY, self);
    }

    /// Takes pack `name` off the list (once).
    pub fn hide_pack(&mut self, name: &str) {
        if !self.packs.iter().any(|p| same_pack(p, name)) {
            self.packs.push(name.to_string());
        }
    }

    /// Takes song `id` off the list (once).
    pub fn hide_song(&mut self, id: &str) {
        if !self.songs.iter().any(|s| s == id) {
            self.songs.push(id.to_string());
        }
    }

    /// Puts pack `name` and its songs `ids` back on the list; whether it
    /// changed.
    pub fn show(&mut self, name: &str, ids: &[&str]) -> bool {
        let before = (self.packs.len(), self.songs.len());
        self.packs.retain(|p| !same_pack(p, name));
        self.songs.retain(|s| !ids.contains(&s.as_str()));
        before != (self.packs.len(), self.songs.len())
    }

    /// Puts pack `name` ([`same_pack`]) and its songs in
    /// `songs` back on the list; whether it changed.
    pub fn show_pack(&mut self, songs: &[LibSong], name: &str) -> bool {
        let ids: Vec<&str> = songs
            .iter()
            .filter(|s| same_pack(&s.pack, name))
            .map(|s| s.entry.id.as_str())
            .collect();
        self.show(name, &ids)
    }

    /// Restore selected imported ids without revealing other rows of a hidden
    /// managed pack. A hidden pack becomes individual hidden ids for its remainder.
    pub fn show_only(&mut self, name: &str, all: &[&str], ids: &[&str]) -> bool {
        let before = self.clone();
        if self.packs.iter().any(|p| same_pack(p, name)) {
            self.packs.retain(|p| !same_pack(p, name));
            let mut hidden: HashSet<String> = self.songs.iter().cloned().collect();
            for id in all.iter().filter(|id| !ids.contains(id)) {
                if hidden.insert((*id).to_string()) {
                    self.songs.push((*id).to_string());
                }
            }
        }
        self.songs.retain(|id| !ids.contains(&id.as_str()));
        *self != before
    }

    /// Restore imported ids under their current pack names while keeping the
    /// other rows hidden, including folder songs sharing the same pack label.
    pub fn show_selected(&mut self, songs: &[LibSong], ids: &[&str]) -> bool {
        let packs = songs
            .iter()
            .filter(|s| ids.contains(&s.entry.id.as_str()))
            .map(|s| s.pack.as_str())
            .collect::<std::collections::BTreeSet<_>>();
        let mut changed = false;
        for pack in packs {
            let all = songs
                .iter()
                .filter(|s| same_pack(&s.pack, pack))
                .map(|s| s.entry.id.as_str())
                .collect::<Vec<_>>();
            changed |= self.show_only(pack, &all, ids);
        }
        changed
    }

    pub fn hides(&self, s: &LibSong) -> bool {
        self.songs.contains(&s.entry.id) || self.packs.iter().any(|p| same_pack(p, &s.pack))
    }
}

/// Text in a legacy code page, strictly (`pack::decode_text_with`'s
/// fallbacks; `ddi_library::legacy`).
pub fn legacy_text(code_page: ddi_library::legacy::Legacy, b: &[u8]) -> Option<String> {
    encoding_rs::Encoding::for_label(code_page.label().as_bytes())?
        .decode_without_bom_handling_and_without_replacement(b)
        .map(|c| c.into_owned())
}

/// The cache folder: the platform's cache directory's
/// `DitoDitoInfinito` (`ditoditoinfinito` on Linux).
pub fn cache_dir() -> Option<PathBuf> {
    crate::paths::cache_dir()
}

/// A song of the library: where it is and its entry.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct LibSong {
    /// The folder or the zip its paths are relative to.
    pub root: PathBuf,
    pub zip: bool,
    /// Its folder, relative to the root.
    pub dir: RelPathBuf,
    pub pack: String,
    /// Pack cover relative to the root (folder or zip).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pack_banner: Option<RelPathBuf>,
    /// Its files relative to its folder (`ddi_library::scan`).
    pub entry: ManifestEntry,
    #[serde(default)]
    pub warnings: Vec<String>,
    #[serde(default)]
    pub notes: Vec<String>,
    /// A Dancing☆Onigiri work's song: where the work is (its entry's
    /// chart is the page or the dump's manifest).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub work: Option<WorkRef>,
}

/// Where a work's song is, every path relative to the root: read again
/// for a play (`play::load`), as the web reads its stored work.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct WorkRef {
    /// The page, or the dump's manifest.
    pub source: RelPathBuf,
    pub dump: bool,
    /// A page's external dos and its charset; a dump's effective dos.
    pub dos: Option<(RelPathBuf, Option<String>)>,
    /// Its music number in the work.
    pub music_no: usize,
    pub music: RelPathBuf,
    /// The music is a script holding it as base64.
    pub encoded: bool,
    /// Its back and mask images: `(name as the data writes it, path)`.
    pub layers: Vec<(String, RelPathBuf)>,
}

impl LibSong {
    /// One of its files (relative to its folder) relative to the root.
    pub fn rooted(&self, file: impl AsRef<str>) -> RelPathBuf {
        let path = self.dir.join(file);
        path.normalize().unwrap_or(path)
    }

    /// One of its files as a texture source: its path, or a zip's entry.
    pub fn source(&self, file: impl AsRef<str>) -> String {
        self.root_source(&self.rooted(file))
    }

    /// A file already relative to the library root, such as a pack cover.
    pub fn root_source(&self, file: &RelPath) -> String {
        if self.zip {
            format!("zip:{}!{}", self.root.display(), file)
        } else {
            file.to_path(&self.root)
                .map(|p| p.display().to_string())
                .unwrap_or_default()
        }
    }
}

/// What a scan found beyond the songs.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Report {
    /// Song folders whose simfile did not read, `(where, why)`.
    pub failed: Vec<(String, String)>,
    /// Packs left out because an earlier folder holds them.
    pub duplicates: Vec<String>,
    /// Folders that could not be read.
    pub missing: Vec<String>,
    /// Songs parsed, and reused from the cache.
    pub parsed: usize,
    pub reused: usize,
    /// How long it took, milliseconds.
    pub ms: f64,
    /// How long the cache's list took to show (from the scan's start).
    pub cached_ms: Option<f64>,
}

/// A scan root: a songs folder or a zip, read as the web reads the same
/// folder or zip picked for an import: its files' paths start with
/// `prefix` (the folder's name, a zip's without `.zip`, a zip in a songs
/// folder under the folder's), and songs at or directly under it fall in
/// `fallback`'s pack, so packs, ids and titles come out as the web's.
#[derive(Clone, Debug, PartialEq)]
struct Root {
    path: PathBuf,
    zip: bool,
    prefix: String,
    fallback: String,
    /// The shared folders' files (`RandomMovies/…`), as the songs name
    /// them.
    shared: Vec<RelPathBuf>,
}

impl Root {
    /// A path of the root's as the scan names it.
    fn virtual_path(&self, path: impl AsRef<str>) -> RelPathBuf {
        RelPath::new(&self.prefix).join(path)
    }

    /// A song folder as the scan names it, relative to the root.
    fn real_dir<'a>(&self, dir: &'a str) -> &'a str {
        if dir == self.prefix {
            ""
        } else {
            dir.strip_prefix(&self.prefix)
                .and_then(|d| d.strip_prefix('/'))
                .unwrap_or(dir)
        }
    }
}

fn is_zip(path: &Path) -> bool {
    path.extension()
        .and_then(|e| e.to_str())
        .is_some_and(|e| e.eq_ignore_ascii_case("zip"))
}

/// A child folder named `name` in any case.
fn child(dir: &Path, name: &str) -> Option<PathBuf> {
    std::fs::read_dir(dir)
        .ok()?
        .flatten()
        .find(|e| {
            e.file_name()
                .to_str()
                .is_some_and(|n| n.eq_ignore_ascii_case(name))
                && e.path().is_dir()
        })
        .map(|e| e.path())
}

/// Where a song folder's songs are: its `Songs` for a StepMania root,
/// else itself.
pub fn songs_dir(folder: &Path) -> PathBuf {
    child(folder, "Songs").unwrap_or_else(|| folder.to_path_buf())
}

/// A folder's name; a zip's without `.zip` (the web's `stem`).
fn root_name(path: &Path, zip: bool) -> String {
    let name = path
        .file_name()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_default();
    if zip {
        name.rsplit_once('.')
            .map_or(name.clone(), |(s, _)| s.to_string())
    } else {
        name
    }
}

/// The roots of a song folder.
fn roots_of(folder: &Path) -> Vec<Root> {
    if is_zip(folder) {
        // A zip picked alone names the pack of the songs at its root.
        let name = root_name(folder, true);
        return vec![Root {
            path: folder.to_path_buf(),
            zip: true,
            prefix: name.clone(),
            fallback: name,
            shared: Vec::new(),
        }];
    }
    // A StepMania root: its songs, and its shared movie folders.
    let (songs, shared) = match child(folder, "Songs") {
        Some(s) => {
            let mut shared = Vec::new();
            for name in ["RandomMovies", "SongMovies"] {
                if let Some(d) = child(folder, name) {
                    shared.extend(
                        files::walk(&d)
                            .listed
                            .into_iter()
                            .map(|l| RelPath::new(name).join(&l.path)),
                    );
                }
            }
            (s, shared)
        }
        None => (folder.to_path_buf(), Vec::new()),
    };
    let prefix = root_name(&songs, false);
    let mut out = vec![Root {
        path: songs.clone(),
        zip: false,
        prefix: prefix.clone(),
        fallback: pack::LOOSE_PACK.to_string(),
        shared: shared.clone(),
    }];
    // Zips in it (a pack's zip) are packs of their own, as a folder of
    // the same name would be.
    if let Ok(entries) = std::fs::read_dir(&songs) {
        let mut zips: Vec<PathBuf> = entries
            .flatten()
            .map(|e| e.path())
            .filter(|p| p.is_file() && is_zip(p))
            .collect();
        zips.sort();
        out.extend(zips.into_iter().map(|z| Root {
            prefix: format!("{prefix}/{}", root_name(&z, true)),
            fallback: pack::LOOSE_PACK.to_string(),
            path: z,
            zip: true,
            shared: shared.clone(),
        }));
    }
    out
}

/// A song folder's listing as the cache compares it: its files' names,
/// sizes and times, out of the root's listing sorted by path.
fn listing_hash(listing: &[Listed], dir: &str) -> u64 {
    let prefix = format!("{dir}/");
    let start = listing.partition_point(|l| l.path.as_str() < prefix.as_str());
    let len = listing[start..].partition_point(|l| l.path.as_str().starts_with(&prefix));
    hash_listed(&listing[start..start + len])
}

/// Files' names, sizes and times as the cache compares them.
fn hash_listed<'a>(files: impl IntoIterator<Item = &'a Listed>) -> u64 {
    let mut h = pack::Fnv1a64::new();
    for l in files {
        h.write(l.path.as_str().as_bytes());
        h.write(&l.size.to_le_bytes());
        h.write(&l.modified.to_le_bytes());
    }
    h.finish()
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
struct Cache {
    version: u32,
    roots: Vec<CachedRoot>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
struct CachedRoot {
    path: PathBuf,
    zip: bool,
    /// How its paths were named ([`Root::prefix`]).
    #[serde(default)]
    prefix: String,
    /// A zip's size and time.
    #[serde(default)]
    stamp: String,
    songs: Vec<CachedSong>,
    /// Song folders whose simfile did not read, `(dir, why, listing)`:
    /// not read again until their listing changes.
    #[serde(default)]
    failed: Vec<(String, String, u64)>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
struct CachedSong {
    listing: u64,
    song: LibSong,
}

fn load_cache(path: &Path) -> Option<Cache> {
    let text = std::fs::read_to_string(path).ok()?;
    serde_json::from_str::<Cache>(&text)
        .ok()
        .filter(|c| c.version == CACHE_VERSION)
}

fn save_cache(path: &Path, cache: &Cache) {
    if let Ok(json) = serde_json::to_string(cache) {
        let _ = crate::paths::write_whole(path, json.as_bytes());
    }
}

fn zip_stamp(path: &Path) -> Option<String> {
    let meta = std::fs::metadata(path).ok()?;
    Some(format!("{}:{}", meta.len(), files::modified(&meta)))
}

/// What a scan reads whole again ("read again"): roots under these
/// folders, and these packs (by name) wherever they are.
#[derive(Clone, Debug, Default)]
pub struct Force {
    pub folders: Vec<PathBuf>,
    pub packs: Vec<String>,
}

impl Force {
    fn root(&self, root: &Root) -> bool {
        self.folders.iter().any(|f| root.path.starts_with(f))
    }

    fn pack(&self, pack: &str) -> bool {
        self.packs.iter().any(|p| same_pack(p, pack))
    }
}

/// Why a scan stops: a newer one superseded it.
struct Superseded;

/// Scans one root, reusing from `cached` what did not change; `None`
/// when it cannot be read (reported).
fn scan_root(
    root: &Root,
    cached_before: Option<&CachedRoot>,
    force: &Force,
    stale: &dyn Fn() -> bool,
    report: &mut Report,
) -> Result<Option<CachedRoot>, Superseded> {
    let cached = cached_before.filter(|c| !force.root(root) && c.prefix == root.prefix);
    if root.zip {
        let Some(stamp) = zip_stamp(&root.path) else {
            report.missing.push(root.path.display().to_string());
            // Kept for when it is back (an unplugged drive).
            return Ok(cached_before.cloned());
        };
        if let Some(c) =
            cached.filter(|c| c.stamp == stamp && !c.songs.iter().any(|s| force.pack(&s.song.pack)))
        {
            report.reused += c.songs.len();
            report.failed.extend(located(
                root,
                &c.failed
                    .iter()
                    .map(|(d, e, _)| (d.clone(), e.clone()))
                    .collect::<Vec<_>>(),
            ));
            return Ok(Some(c.clone()));
        }
        let reader = match ZipReader::open(&root.path, &legacy_text) {
            Ok(r) => r,
            Err(e) => {
                report.missing.push(format!("{}: {e}", root.path.display()));
                return Ok(None);
            }
        };
        let paths: Vec<RelPathBuf> = reader.names.iter().map(|n| root.virtual_path(n)).collect();
        let files = files::Prefixed {
            inner: &reader,
            prefix: format!("{}/", root.prefix),
        };
        let mut out = CachedRoot {
            path: root.path.clone(),
            zip: true,
            prefix: root.prefix.clone(),
            stamp,
            songs: Vec::new(),
            failed: reader
                .unread
                .iter()
                .map(|(name, why)| (root.virtual_path(name).into_string(), why.clone(), 0))
                .collect(),
        };
        scan_songs(root, &files, &paths, None, None, stale, &mut out, report)?;
        return Ok(Some(out));
    }
    if !root.path.is_dir() {
        report.missing.push(root.path.display().to_string());
        // An unplugged drive: its songs are not read again when it is
        // back.
        return Ok(cached_before.cloned());
    }
    let walked = files::walk(&root.path);
    let listing: Vec<Listed> = walked
        .listed
        .into_iter()
        .map(|l| Listed {
            path: root.virtual_path(&l.path),
            ..l
        })
        .collect();
    let paths: Vec<RelPathBuf> = listing.iter().map(|l| l.path.clone()).collect();
    let reader = DirReader {
        root: root.path.clone(),
    };
    let files = files::Prefixed {
        inner: &reader,
        prefix: format!("{}/", root.prefix),
    };
    let mut out = CachedRoot {
        path: root.path.clone(),
        zip: false,
        prefix: root.prefix.clone(),
        stamp: String::new(),
        songs: Vec::new(),
        failed: walked
            .unread
            .into_iter()
            .map(|(p, why)| (root.virtual_path(&p).into_string(), why, 0))
            .collect(),
    };
    scan_songs(
        root,
        &files,
        &paths,
        Some(&listing),
        cached.map(|c| (c, force)),
        stale,
        &mut out,
        report,
    )?;
    Ok(Some(out))
}

/// The songs among a root's `paths` (sorted, as the scan names them)
/// into `out`: each reused from `cached` while its folder's `listing`
/// is the same (a zip's are all read: its stamp changed), else read.
#[allow(clippy::too_many_arguments)]
fn scan_songs(
    root: &Root,
    files: &dyn Reader,
    paths: &[RelPathBuf],
    listing: Option<&[Listed]>,
    cached: Option<(&CachedRoot, &Force)>,
    stale: &dyn Fn() -> bool,
    out: &mut CachedRoot,
    report: &mut Report,
) -> Result<(), Superseded> {
    let reuse: HashMap<&str, &CachedSong> = cached
        .map(|(c, _)| {
            c.songs
                .iter()
                .filter(|s| {
                    s.song.work.is_none() && !ddi_library::ffr::is_level_chart(&s.song.entry.chart)
                })
                .map(|s| (s.song.dir.as_str(), s))
                .collect()
        })
        .unwrap_or_default();
    let failed_before: HashMap<&str, (&String, u64)> = cached
        .map(|(c, _)| {
            c.failed
                .iter()
                .map(|(d, e, h)| (d.as_str(), (e, *h)))
                .collect()
        })
        .unwrap_or_default();
    let mut seen = scan::Duplicates::default();
    let songs = pack::find_songs(paths, &root.fallback);
    for c in &songs {
        if let Some(why) = seen.check(c) {
            out.failed.push((c.dir.to_string(), why, 0));
            continue;
        }
        let hash = listing.map_or(0, |l| listing_hash(l, c.dir.as_str()));
        let forced = cached.is_some_and(|(_, f)| f.pack(&c.pack));
        let real = root.real_dir(c.dir.as_str());
        if listing.is_some() && !forced {
            if let Some(s) = reuse.get(real).filter(|s| s.listing == hash) {
                report.reused += 1;
                out.songs.push((*s).clone());
                continue;
            }
            if let Some((e, _)) = failed_before
                .get(c.dir.as_str())
                .filter(|(_, h)| *h == hash)
            {
                out.failed.push((c.dir.to_string(), (*e).clone(), hash));
                continue;
            }
        }
        if stale() {
            return Err(Superseded);
        }
        report.parsed += 1;
        match scan::scan_song(
            files,
            scan::pack_paths(paths, c),
            c,
            &root.shared,
            &legacy_text,
        ) {
            Ok(s) => out.songs.push(CachedSong {
                listing: hash,
                song: lib_song(root, s),
            }),
            Err(e) => out.failed.push((c.dir.to_string(), e, hash)),
        }
    }
    // Dancing☆Onigiri works, reused from the cache while their folder's
    // listing is the same.
    if stale() {
        return Err(Superseded);
    }
    scan_works(root, files, paths, listing, cached, out, report);
    if stale() {
        return Err(Superseded);
    }
    scan_levels(root, files, paths, &songs, listing, cached, out, report);
    // Refresh even reused songs: a cover can change without a song changing.
    let mut covers = HashMap::new();
    for cached in &mut out.songs {
        let song = &mut cached.song;
        let dir = root.virtual_path(&song.dir);
        // An FFR level's folder is its pack's.
        let pack_dir = if ddi_library::ffr::is_level_chart(&song.entry.chart) {
            dir.as_rel_path()
        } else {
            dir.parent()
        };
        song.pack_banner = covers
            .entry(pack_dir.to_owned())
            .or_insert_with(|| {
                pack::pack_banner(pack_dir, paths)
                    .map(|p| RelPathBuf::from(root.real_dir(p.as_str())))
            })
            .clone();
    }
    let failed: Vec<(String, String)> = out
        .failed
        .iter()
        .map(|(d, e, _)| (d.clone(), e.clone()))
        .collect();
    report.failed.extend(located(root, &failed));
    Ok(())
}

/// The works among a root's `paths` (as the scan names them) into `out`,
/// found as the web's import finds them (`ddi_library::works`): the
/// same songs, ids and entries, their files named in place.
fn scan_works(
    root: &Root,
    files: &dyn Reader,
    paths: &[RelPathBuf],
    listing: Option<&[Listed]>,
    cached: Option<(&CachedRoot, &Force)>,
    out: &mut CachedRoot,
    report: &mut Report,
) {
    let mut seen: HashMap<String, String> = HashMap::new();
    for ws in ddi_library::danoni::work_sources(paths) {
        let label = match &ws {
            ddi_library::danoni::WorkSource::Page(p) | ddi_library::danoni::WorkSource::Dump(p) => {
                p.clone()
            }
        };
        // The work's folder unchanged (names, sizes, times): what the
        // cache has of it (its songs, or why it was not read).
        let hash = listing.map_or(0, |l| listing_hash(l, parent_of(&label)));
        if let (Some(_), Some((c, force))) = (listing, cached) {
            let real = root.real_dir(&label);
            let songs: Vec<&CachedSong> = c
                .songs
                .iter()
                .filter(|s| {
                    s.listing == hash && s.song.work.as_ref().is_some_and(|w| w.source == real)
                })
                .collect();
            let failed: Vec<&(String, String, u64)> = c
                .failed
                .iter()
                .filter(|(at, _, h)| *h == hash && at.starts_with(label.as_str()))
                .collect();
            let forced = songs.iter().any(|s| force.pack(&s.song.pack));
            if !forced && (!songs.is_empty() || !failed.is_empty()) {
                report.reused += songs.len();
                for s in songs {
                    seen.insert(s.song.entry.id.clone(), label.clone());
                    out.songs.push(s.clone());
                }
                out.failed.extend(failed.into_iter().cloned());
                continue;
            }
        }
        let read = match &ws {
            ddi_library::danoni::WorkSource::Page(p) => read_page(files, paths, p),
            ddi_library::danoni::WorkSource::Dump(m) => read_dump(files, paths, m),
        };
        let (w, dos) = match read {
            Ok(Some(w)) => w,
            Ok(None) => continue,
            Err(e) => {
                out.failed.push((label, e, hash));
                continue;
            }
        };
        report.parsed += 1;
        let plan = ddi_library::works::plan(&ws, &label, &w, paths, &root.fallback, &mut seen);
        out.failed
            .extend(plan.skipped.into_iter().map(|(l, e)| (l, e, hash)));
        let real = |p: &str| root.real_dir(p).to_string();
        let dir = real(parent_of(&label));
        for s in plan.songs {
            let song_label = format!("{} ({})", s.label, s.song.title);
            let warnings = plan
                .warnings
                .iter()
                .filter(|(l, _)| *l == s.label || *l == song_label)
                .map(|(_, w)| w.clone())
                .collect();
            let from = |p: &str| RelPath::new(&real(p)).relative_to(RelPath::new(&dir));
            let entry =
                ddi_library::works::entry(&s, from(&label).as_str(), from(&s.music).as_str());
            out.songs.push(CachedSong {
                listing: hash,
                song: LibSong {
                    root: root.path.clone(),
                    zip: root.zip,
                    dir: dir.clone().into(),
                    pack: s.pack.clone(),
                    pack_banner: None,
                    entry,
                    warnings,
                    notes: Vec::new(),
                    work: Some(WorkRef {
                        source: real(&label).into(),
                        dump: matches!(ws, ddi_library::danoni::WorkSource::Dump(_)),
                        dos: dos.as_ref().map(|(p, c)| (real(p).into(), c.clone())),
                        music_no: s.music_no,
                        music: real(&s.music).into(),
                        encoded: s.encoded,
                        layers: s
                            .layers
                            .iter()
                            .map(|(name, path)| (name.clone(), real(path).into()))
                            .collect(),
                    }),
                },
            });
        }
    }
}

/// The FFR levels among a root's `paths` (`ddi_library::ffr`) into
/// `out`, a song each, found as the web's import finds them (the same ids
/// and packs), read in place: a SWF's music is taken out of it when it
/// plays. Each reused from the cache while its file (and its music's)
/// is the same.
#[allow(clippy::too_many_arguments)]
fn scan_levels(
    root: &Root,
    files: &dyn Reader,
    paths: &[RelPathBuf],
    songs: &[pack::SongCandidate],
    listing: Option<&[Listed]>,
    cached: Option<(&CachedRoot, &Force)>,
    out: &mut CachedRoot,
    report: &mut Report,
) {
    use ddi_library::ffr;
    // The cache's songs by folder and chart, its failures by file: each
    // level's first.
    let mut reuse: HashMap<(&str, &str), Vec<&CachedSong>> = HashMap::new();
    let mut failed_before: HashMap<&str, Vec<&(String, String, u64)>> = HashMap::new();
    if let Some((cache, _)) = cached {
        for s in cache.songs.iter().filter(|s| s.song.work.is_none()) {
            reuse
                .entry((s.song.dir.as_str(), s.song.entry.chart.as_str()))
                .or_default()
                .push(s);
        }
        for f in &cache.failed {
            failed_before.entry(f.0.as_str()).or_default().push(f);
        }
    }
    for c in ffr::find_levels(paths, songs, &root.fallback) {
        let label = c.path.to_string();
        // The level's file and its music's, unchanged.
        let hash = listing.map_or(0, |l| {
            let find = |p: &RelPathBuf| l.binary_search_by(|f| f.path.cmp(p)).ok().map(|i| &l[i]);
            let mut found: Vec<&Listed> = std::iter::once(&c.path)
                .chain(c.music.as_ref().filter(|m| **m != c.path))
                .filter_map(find)
                .collect();
            found.sort_by(|a, b| a.path.cmp(&b.path));
            hash_listed(found)
        });
        let dir = root.real_dir(c.dir.as_str()).to_string();
        let (chart, music) = ffr::names_in_place(&c);
        if let (Some(_), Some((_, force))) = (listing, cached)
            && !force.pack(&c.pack)
        {
            if let Some(s) = reuse
                .get(&(dir.as_str(), chart.as_str()))
                .and_then(|v| v.iter().find(|s| s.listing == hash))
            {
                report.reused += 1;
                out.songs.push((*s).clone());
                continue;
            }
            if let Some(f) = failed_before
                .get(label.as_str())
                .and_then(|v| v.iter().find(|(_, _, h)| *h == hash))
            {
                out.failed.push((*f).clone());
                continue;
            }
        }
        // Too large to be a level: not one.
        let bytes = match files.read_at(&c.path, 0, ffr::MAX_LEVEL as usize + 1) {
            Ok(b) if b.len() as u64 > ffr::MAX_LEVEL => continue,
            Ok(b) => b,
            Err(e) => {
                out.failed.push((label, e, hash));
                continue;
            }
        };
        let read = match ffr::read_level(&c, &bytes) {
            Ok(Some(r)) => r,
            Ok(None) => continue,
            Err(e) => {
                out.failed.push((label, e, hash));
                continue;
            }
        };
        report.parsed += 1;
        out.songs.push(CachedSong {
            listing: hash,
            song: LibSong {
                root: root.path.clone(),
                zip: root.zip,
                dir: dir.into(),
                pack: c.pack.clone(),
                pack_banner: None,
                entry: ffr::entry(&c, &read.song, &chart, &music),
                warnings: Vec::new(),
                notes: Vec::new(),
                work: None,
            },
        });
    }
}

/// The largest page read as a possible work, bytes.
const MAX_PAGE: usize = 8 << 20;

/// A page's work, and its external dos with its charset.
type ReadWork = (ddi_library::works::Work, Option<(String, Option<String>)>);

/// A page read as a work: `None` for a page with no dos.
pub fn read_page(
    files: &dyn Reader,
    paths: &[RelPathBuf],
    page: &str,
) -> Result<Option<ReadWork>, String> {
    // A page larger than this holds no work (its dos would be a few
    // hundred kilobytes at most): not read whole.
    let bytes = files.read_at(RelPath::new(page), 0, MAX_PAGE + 1)?;
    if bytes.len() > MAX_PAGE {
        return Ok(None);
    }
    let charset = ddi_library::danoni::declared_charset(&bytes);
    let html = decode_with(&bytes, charset.as_deref());
    let Some(read) = ddi_library::works::read_page(page, &html, charset.as_deref(), paths)? else {
        return Ok(None);
    };
    let external = read.external.clone();
    let text = match &external {
        Some((path, charset)) => Some(decode_with(
            &files.read(RelPath::new(path))?,
            charset.as_deref(),
        )),
        None => None,
    };
    Ok(Some((read.finish(text.as_deref())?, external)))
}

/// A dump read as a work: `None` for another `manifest.json`.
pub fn read_dump(
    files: &dyn Reader,
    paths: &[RelPathBuf],
    manifest: &str,
) -> Result<Option<ReadWork>, String> {
    let bytes = files.read_at(RelPath::new(manifest), 0, MAX_PAGE + 1)?;
    if bytes.len() > MAX_PAGE {
        return Ok(None);
    }
    let text = pack::decode_text(&bytes);
    let Some(plan) = ddi_library::danoni::dump_plan(manifest, &text, paths)? else {
        return Ok(None);
    };
    let effective = plan.effective.clone();
    let eff = pack::decode_text(&files.read(RelPath::new(&effective))?);
    Ok(Some((
        ddi_library::works::dump_work(plan, &eff)?,
        Some((effective, None)),
    )))
}

/// Text in the charset a page declares (the browser's way: the label's
/// encoding, a bad byte as U+FFFD). Without one: UTF-8 when the bytes
/// are, else Shift_JIS. A label no decoder knows (`UTF-8N`) or one that
/// decodes nothing (the "replacement" labels): as the web's import falls
/// back, by what the bytes look like (`pack::decode_text`).
pub fn decode_with(bytes: &[u8], label: Option<&str>) -> String {
    let bytes = bytes.strip_prefix(b"\xEF\xBB\xBF").unwrap_or(bytes);
    let encoding = match label {
        Some(l) => match encoding_rs::Encoding::for_label(l.trim().as_bytes()) {
            Some(e) if e != encoding_rs::REPLACEMENT => e,
            _ => return pack::decode_text(bytes),
        },
        None => match std::str::from_utf8(bytes) {
            Ok(s) => return s.to_string(),
            Err(_) => encoding_rs::SHIFT_JIS,
        },
    };
    encoding.decode_without_bom_handling(bytes).0.into_owned()
}

fn parent_of(path: &str) -> &str {
    path.rsplit_once('/').map_or("", |(d, _)| d)
}

/// Failures named by where they are: the root's path and the folder
/// within it.
fn located(root: &Root, failed: &[(String, String)]) -> Vec<(String, String)> {
    failed
        .iter()
        .map(|(dir, e)| {
            let real = root.real_dir(dir);
            let at = if real.is_empty() {
                root.path.display().to_string()
            } else {
                format!("{}/{real}", root.path.display())
            };
            (at, e.clone())
        })
        .collect()
}

fn lib_song(root: &Root, s: scan::Scanned) -> LibSong {
    LibSong {
        root: root.path.clone(),
        zip: root.zip,
        dir: root.real_dir(s.dir.as_str()).into(),
        pack: s.pack,
        pack_banner: None,
        entry: s.entry,
        warnings: s.warnings,
        notes: s.notes,
        work: None,
    }
}

/// The songs of `roots` in order, the first of each id kept; the packs
/// of the rest reported.
fn first_wins(roots: &[CachedRoot], report: &mut Report) -> Vec<LibSong> {
    let mut seen: HashMap<String, PathBuf> = HashMap::new();
    let mut reported = HashSet::new();
    let mut out = Vec::new();
    for r in roots {
        for s in &r.songs {
            match seen.get(&s.song.entry.id) {
                Some(first) => {
                    let line = format!(
                        "{} in {} is also in {}: the first is listed",
                        s.song.pack,
                        r.path.display(),
                        first.display()
                    );
                    if reported.insert(line.clone()) {
                        report.duplicates.push(line);
                    }
                }
                None => {
                    seen.insert(s.song.entry.id.clone(), r.path.clone());
                    out.push(s.song.clone());
                }
            }
        }
    }
    out
}

/// Whether `root` (a cached root's path) belongs to one of `folders`.
fn under(folders: &[PathBuf], root: &Path) -> bool {
    folders.iter().any(|f| root.starts_with(f))
}

/// Scans `folders` (each a root or several), with `cache` as it was;
/// `force` says what to read whole again; `stale` stops it once a newer
/// scan started. The songs, the report and the cache to keep (with the
/// cached roots of folders that are not there now, an unplugged drive,
/// kept for when they are back).
fn scan_all(
    folders: &[PathBuf],
    cache: Option<&Cache>,
    force: &Force,
    stale: &dyn Fn() -> bool,
) -> Result<(Vec<LibSong>, Report, Cache), Superseded> {
    let start = Instant::now();
    let mut report = Report::default();
    let mut listed = Vec::new();
    let mut kept = Vec::new();
    for folder in folders {
        for root in roots_of(folder) {
            let cached = cache.and_then(|c| {
                c.roots
                    .iter()
                    .find(|r| r.path == root.path && r.zip == root.zip)
            });
            let present = if root.zip {
                root.path.is_file()
            } else {
                root.path.is_dir()
            };
            match scan_root(&root, cached, force, stale, &mut report)? {
                Some(r) if present => {
                    listed.push(r.clone());
                    kept.push(r);
                }
                // Missing: kept in the cache, not listed.
                Some(r) => kept.push(r),
                None => {}
            }
        }
    }
    let songs = first_wins(&listed, &mut report);
    report.ms = start.elapsed().as_secs_f64() * 1000.0;
    Ok((
        songs,
        report,
        Cache {
            version: CACHE_VERSION,
            roots: kept,
        },
    ))
}

/// Scans `folders` once, with no cache (tests, checks): the songs and
/// the report.
pub fn scan_folders(folders: &[PathBuf]) -> (Vec<LibSong>, Report) {
    match scan_all(folders, None, &Force::default(), &|| false) {
        Ok((songs, report, _)) => (songs, report),
        Err(Superseded) => (Vec::new(), Report::default()),
    }
}

/// What the scan thread says.
pub enum News {
    /// The list as the cache had it (at the start).
    Cached(Vec<LibSong>, f64),
    /// The list as the folders hold it now.
    Scanned(Vec<LibSong>, Report),
}

/// The library: its songs now, and the scan that keeps them current.
pub struct Library {
    pub songs: Vec<LibSong>,
    pub report: Report,
    folder_songs: Vec<LibSong>,
    managed_songs: Vec<LibSong>,
    /// A scan is running.
    pub scanning: bool,
    /// The list came from the cache (no scan finished yet).
    pub from_cache: bool,
    rx: Option<mpsc::Receiver<(u64, News)>>,
    cached_ms: Option<f64>,
    /// The newest scan's number: an older one stops, and never saves.
    generation: Arc<AtomicU64>,
    wake: Arc<dyn Fn() + Send + Sync>,
    cache: Option<PathBuf>,
}

/// One scan at a time saves the cache.
static SAVING: Mutex<()> = Mutex::new(());

impl Library {
    /// The library of `folders`: listed from the cache at once (on its
    /// thread), then scanned; `wake` wakes the event loop with news.
    pub fn start(
        folders: Vec<PathBuf>,
        cache: Option<PathBuf>,
        wake: Arc<dyn Fn() + Send + Sync>,
    ) -> Library {
        let mut l = Library {
            songs: Vec::new(),
            folder_songs: Vec::new(),
            managed_songs: Vec::new(),
            report: Report::default(),
            scanning: false,
            from_cache: false,
            rx: None,
            cached_ms: None,
            generation: Arc::default(),
            wake,
            cache,
        };
        l.run(folders, Force::default(), true);
        l
    }

    /// Only folder-backed packs can be read again; managed publications refresh
    /// through the importer, regardless of their display label.
    pub fn rereadable_packs(&self) -> HashSet<String> {
        self.folder_songs
            .iter()
            .map(|s| s.pack.to_lowercase())
            .collect()
    }

    /// Publishes verified managed rows beside folder songs. Folder songs win
    /// an id collision; managed duplicates keep their first row. A later folder
    /// scan replaces only folder rows, so pending scan news cannot erase imports.
    pub fn set_managed(&mut self, songs: Vec<LibSong>) -> bool {
        self.managed_songs = songs;
        self.merge_songs()
    }

    /// Songs read outside a scan (a new song's folder) in the list at
    /// once, in place of those with their ids; `remove`: ids taken out.
    /// A scan started after their folders changed lists them the same.
    pub fn put_songs(&mut self, songs: Vec<LibSong>, remove: &[String]) -> bool {
        self.folder_songs.retain(|s| {
            !remove.contains(&s.entry.id) && !songs.iter().any(|n| n.entry.id == s.entry.id)
        });
        self.folder_songs.extend(songs);
        self.merge_songs()
    }

    fn merge_songs(&mut self) -> bool {
        let mut seen = HashSet::new();
        let songs: Vec<_> = self
            .folder_songs
            .iter()
            .chain(&self.managed_songs)
            .filter(|s| seen.insert(s.entry.id.as_str()))
            .cloned()
            .collect();
        if self.songs == songs {
            register(&songs);
            return false;
        }
        // Cached previews carry immutable file paths; replacing an import must
        // prepare its new music instead of reusing the previous revision.
        let by_id: HashMap<_, _> = songs.iter().map(|s| (s.entry.id.as_str(), s)).collect();
        let changed: HashSet<_> = self
            .songs
            .iter()
            .filter(|old| by_id.get(old.entry.id.as_str()).copied() != Some(*old))
            .map(|s| s.entry.id.as_str())
            .collect();
        register(&songs);
        if let Ok(mut list) = preps().lock() {
            list.retain(|(id, _)| !changed.contains(id.as_str()));
        }
        if let Ok(mut invalidated) = invalidated_previews().lock() {
            invalidated.extend(changed.into_iter().map(str::to_string));
        }
        self.songs = songs;
        true
    }

    /// Scans `folders` again (`force`: the roots under these read whole,
    /// as "read again" asks).
    pub fn scan(&mut self, folders: Vec<PathBuf>, force: Vec<PathBuf>, listed_first: bool) {
        self.run(
            folders,
            Force {
                folders: force,
                packs: Vec::new(),
            },
            listed_first,
        );
    }

    /// Scans `folders` again, pack `pack` read whole.
    pub fn reread_pack(&mut self, folders: Vec<PathBuf>, pack: &str) {
        self.run(
            folders,
            Force {
                folders: Vec::new(),
                packs: vec![pack.to_string()],
            },
            false,
        );
    }

    fn run(&mut self, folders: Vec<PathBuf>, force: Force, listed_first: bool) {
        let generation = self.generation.fetch_add(1, Ordering::SeqCst) + 1;
        let current = self.generation.clone();
        let (tx, rx) = mpsc::channel();
        self.rx = Some(rx);
        self.scanning = true;
        let wake = self.wake.clone();
        let cache_path = self.cache.clone();
        let _ = std::thread::Builder::new()
            .name("ddi-library".into())
            .spawn(move || {
                let stale = || current.load(Ordering::SeqCst) != generation;
                let start = Instant::now();
                let cache = cache_path.as_deref().and_then(load_cache);
                if listed_first && let Some(c) = &cache {
                    // Only the folders listed now.
                    let roots: Vec<CachedRoot> = c
                        .roots
                        .iter()
                        .filter(|r| under(&folders, &r.path))
                        .cloned()
                        .collect();
                    let mut r = Report::default();
                    let songs = first_wins(&roots, &mut r);
                    let ms = start.elapsed().as_secs_f64() * 1000.0;
                    if tx.send((generation, News::Cached(songs, ms))).is_err() {
                        return;
                    }
                    wake();
                }
                let Ok((songs, mut report, kept)) =
                    scan_all(&folders, cache.as_ref(), &force, &stale)
                else {
                    return;
                };
                report.ms = start.elapsed().as_secs_f64() * 1000.0;
                if let Some(p) = &cache_path {
                    let _saving = SAVING.lock();
                    if stale() {
                        return;
                    }
                    save_cache(p, &kept);
                }
                let _ = tx.send((generation, News::Scanned(songs, report)));
                wake();
            });
    }

    /// Takes the scan's news; whether the list changed.
    pub fn take(&mut self) -> bool {
        let Some(rx) = &self.rx else {
            return false;
        };
        let now = self.generation.load(Ordering::SeqCst);
        let mut changed = false;
        while let Ok((generation, news)) = rx.try_recv() {
            if generation != now {
                continue;
            }
            match news {
                News::Cached(songs, ms) => {
                    // A scan done first keeps its newer list.
                    if self.scanning {
                        self.folder_songs = songs;
                        self.from_cache = true;
                        self.cached_ms = Some(ms);
                        changed = true;
                    }
                }
                News::Scanned(songs, mut report) => {
                    self.folder_songs = songs;
                    report.cached_ms = self.cached_ms;
                    self.report = report;
                    self.scanning = false;
                    self.from_cache = false;
                    changed = true;
                }
            }
        }
        if changed {
            self.merge_songs();
        }
        changed
    }

    /// Waits for the scan to end (tests, a timed run).
    pub fn wait(&mut self) {
        while self.scanning {
            self.take();
            std::thread::sleep(std::time::Duration::from_millis(2));
        }
    }
}

/// The songs by id, for what finds a song's files without the shell
/// (the preview player's lookup, a texture's loader).
fn registry() -> &'static RwLock<HashMap<String, LibSong>> {
    static SONGS: OnceLock<RwLock<HashMap<String, LibSong>>> = OnceLock::new();
    SONGS.get_or_init(Default::default)
}

fn register(songs: &[LibSong]) {
    if let Ok(mut r) = registry().write() {
        r.clear();
        r.extend(songs.iter().map(|s| (s.entry.id.clone(), s.clone())));
    }
}

/// The songs tests added ([`register_for_tests`]), apart from the
/// scans' registry, which each scan replaces: tests run side by side,
/// and one's scan must not take another's songs away.
fn test_songs() -> &'static RwLock<HashMap<String, LibSong>> {
    static SONGS: OnceLock<RwLock<HashMap<String, LibSong>>> = OnceLock::new();
    SONGS.get_or_init(Default::default)
}

/// Adds `songs` to the library (tests that load songs without a scan's
/// thread), for the rest of the process: a song of the same id replaces
/// the one added before, and no scan removes them.
pub fn register_for_tests(songs: &[LibSong]) {
    if let Ok(mut r) = test_songs().write() {
        r.extend(songs.iter().map(|s| (s.entry.id.clone(), s.clone())));
    }
}

/// The library's song `id` (a test's first).
pub fn song(id: &str) -> Option<LibSong> {
    let tests = test_songs().read().ok().and_then(|r| r.get(id).cloned());
    tests.or_else(|| registry().read().ok()?.get(id).cloned())
}

/// A song with a chart of content `digest` (a replay's chart moved to
/// another song id): a bundled one, else the library's.
pub fn find_by_digest(digest: ddi_chart::ChartDigest) -> Option<String> {
    let has = |e: &ManifestEntry| e.charts.iter().any(|c| c.digest == Some(digest));
    if let Some(e) = bundled().into_iter().find(|e| has(e)) {
        return Some(e.id);
    }
    let reg = registry().read().ok()?;
    let tests = test_songs().read().ok()?;
    let mut ids: Vec<&String> = reg
        .iter()
        .chain(tests.iter())
        .filter(|(_, s)| has(&s.entry))
        .map(|(id, _)| id)
        .collect();
    ids.sort();
    ids.first().map(|id| (*id).clone())
}

/// The bundled songs' entries: the manifest `xtask gen-songs` writes
/// (`target/songs/index.json`, beside the assets as Trunk builds it), or
/// none when it was never generated. Read once a run once it is there
/// (a test may write it after starting).
pub fn bundled() -> Vec<ManifestEntry> {
    static READ: std::sync::OnceLock<Vec<ManifestEntry>> = std::sync::OnceLock::new();
    if let Some(songs) = READ.get() {
        return songs.clone();
    }
    let assets = crate::images::assets();
    let candidates = [
        assets.join("../target/songs/index.json"),
        assets.join("songs/index.json"),
    ];
    candidates
        .iter()
        .find_map(|p| std::fs::read_to_string(p).ok())
        .and_then(|t| serde_json::from_str::<Manifest>(&t).ok())
        .map(|m| READ.get_or_init(|| m.songs).clone())
        .unwrap_or_default()
}

/// A library song's preview made ready on a thread: its music's path (a
/// zip's extracted into the cache folder), its window and timing.
struct Prepared {
    music: PathBuf,
    from: f64,
    length: f64,
    /// A separate preview file (`#PREVIEW`), played from its start.
    separate: bool,
    timing: ddi_chart::TimingMap,
    target: ddi_platform::mp3::Target,
}

enum Prep {
    Running,
    Ready(Box<Prepared>),
    Failed,
}

/// Previews made ready, by song id (a few dozen kept).
fn preps() -> &'static Mutex<Vec<(String, Prep)>> {
    static PREPS: OnceLock<Mutex<Vec<(String, Prep)>>> = OnceLock::new();
    PREPS.get_or_init(Default::default)
}

fn invalidated_previews() -> &'static Mutex<HashSet<String>> {
    static INVALIDATED: OnceLock<Mutex<HashSet<String>>> = OnceLock::new();
    INVALIDATED.get_or_init(Default::default)
}

/// Changed song revisions whose already-decoded previews must be discarded.
pub fn take_invalidated_previews() -> Vec<String> {
    invalidated_previews()
        .lock()
        .map(|mut ids| ids.drain().collect())
        .unwrap_or_default()
}

/// Previews kept ready at most.
const PREPS_KEPT: usize = 48;

/// What wakes the event loop when a preview is ready (set by the shell).
static PREVIEW_WAKE: OnceLock<Arc<dyn Fn() + Send + Sync>> = OnceLock::new();
/// A preview became ready since [`take_prepared`] was last asked.
static PREPARED: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

/// Sets what wakes the loop when a preview is ready.
pub fn set_preview_wake(wake: Arc<dyn Fn() + Send + Sync>) {
    let _ = PREVIEW_WAKE.set(wake);
}

/// Whether a preview became ready since last asked (the player looks
/// for its song again).
pub fn take_prepared() -> bool {
    PREPARED.swap(false, Ordering::SeqCst)
}

/// The preview of song `id`: a bundled one's at once; a library song's
/// once a thread has read its simfile (and extracted a zip's music):
/// `None` meanwhile ([`take_prepared`] says when to ask again).
pub fn preview_song(id: &str) -> Option<crate::preview::PreviewSong> {
    let Some(s) = song(id) else {
        return crate::preview::PreviewSong::bundled(id);
    };
    let mut list = preps().lock().ok()?;
    match list.iter().find(|(k, _)| k == id).map(|(_, p)| p) {
        Some(Prep::Ready(p)) => {
            let mut song = crate::preview::PreviewSong::of(
                id,
                p.music.clone(),
                p.from,
                p.length,
                p.timing.clone(),
            );
            song.target = p.target;
            if p.separate {
                song.length = ddi_session::preview::file_length(p.length);
            }
            return Some(song);
        }
        Some(Prep::Running | Prep::Failed) => return None,
        None => {}
    }
    list.push((id.to_string(), Prep::Running));
    if list.len() > PREPS_KEPT {
        list.remove(0);
    }
    drop(list);
    let id = id.to_string();
    let _ = std::thread::Builder::new()
        .name("ddi-preview".into())
        .spawn(move || {
            let done = prepare_preview(&s).map_or(Prep::Failed, |p| Prep::Ready(Box::new(p)));
            if let Ok(mut preps) = preps().lock()
                && song(&id).as_ref() == Some(&s)
                && let Some(slot) = preps.iter_mut().find(|(k, _)| *k == id)
            {
                slot.1 = done;
            }
            PREPARED.store(true, Ordering::SeqCst);
            if let Some(w) = PREVIEW_WAKE.get() {
                w();
            }
        });
    None
}

/// A song's preview: its simfile read again, a zip's music extracted
/// once (kept by the zip's size and time, written whole then renamed).
fn prepare_preview(s: &LibSong) -> Option<Prepared> {
    if let Some(w) = &s.work {
        return prepare_work_preview(s, w);
    }
    let reader: Box<dyn Reader> = if s.zip {
        Box::new(ZipReader::open(&s.root, &legacy_text).ok()?)
    } else {
        Box::new(DirReader {
            root: s.root.clone(),
        })
    };
    // A separate preview file (`#PREVIEW`), else the music.
    let file = s.entry.preview.as_ref().unwrap_or(&s.entry.music);
    // The timing for the beat pulse, from the chart as it is now; an FFR
    // level SWF's music read with it.
    let chart = reader.read(&s.rooted(&s.entry.chart)).ok()?;
    let (parsed, level_music) = if ddi_library::ffr::is_level_chart(&s.entry.chart) {
        let (song, mp3) =
            ddi_library::ffr::chart_song_and_music(&s.entry.chart, &chart, &s.entry.title).ok()?;
        (song, mp3.filter(|_| file == &s.entry.chart))
    } else {
        let song = crate::play::load::parse_simfile(&s.entry.chart, &chart, false).ok()?;
        (song, None)
    };
    // An FFR level's music is inside it: taken out once, as a zip's.
    let in_level = ddi_library::ffr::is_swf_name(file);
    let music = if s.zip || in_level {
        let dir = cache_dir()?.join("previews");
        let ext = ddi_library::ffr::music_extension(file).unwrap_or("ogg");
        let stamp = if s.zip {
            zip_stamp(&s.root)?.replace(':', "-")
        } else {
            let meta = std::fs::metadata(s.rooted(file).to_path(&s.root)?).ok()?;
            format!("{}-{}", meta.len(), files::modified(&meta))
        };
        let key = format!("{}-", s.entry.id);
        let out = dir.join(format!("{key}{stamp}.{ext}"));
        if !out.is_file() {
            let bytes = match level_music {
                Some(mp3) => mp3,
                None => {
                    let bytes = reader.read(&s.rooted(file)).ok()?;
                    ddi_library::ffr::music(file, bytes).ok()?.0
                }
            };
            cache_preview(&dir, &key, &out, &bytes)?;
        }
        out
    } else {
        s.rooted(file).to_path(&s.root)?
    };
    Some(Prepared {
        music,
        from: if s.entry.preview.is_some() {
            0.0
        } else {
            s.entry.preview_start
        },
        length: s.entry.preview_length,
        separate: s.entry.preview.is_some(),
        timing: parsed.timing,
        target: Default::default(),
    })
}

/// A work's song's preview: from its start (works give no window), its
/// music where it is, or (in a zip, or a script holding it) written once
/// into the previews folder, lined up with Chrome's decode.
fn prepare_work_preview(s: &LibSong, w: &WorkRef) -> Option<Prepared> {
    let (files, _) = crate::play::load::Files::of(&s.entry.id)?;
    let song = crate::play::load::read_work(&files, w).ok()?;
    let music = if s.zip || w.encoded {
        let mut bytes = files.read_at_root(&w.music).ok()?;
        if w.encoded {
            bytes = ddi_library::danoni::decode_music(&pack::decode_text(&bytes))?;
        }
        let (ext, _) = ddi_library::works::sniff_audio(&bytes);
        let dir = cache_dir()?.join("previews");
        let stamp = if s.zip {
            zip_stamp(&s.root)?.replace(':', "-")
        } else {
            let meta = std::fs::metadata(w.music.to_path(&s.root)?).ok()?;
            format!("{}-{}", meta.len(), files::modified(&meta))
        };
        let key = format!("{}-", s.entry.id);
        let out = dir.join(format!("{key}{stamp}.{ext}"));
        if !out.is_file() {
            cache_preview(&dir, &key, &out, &bytes)?;
        }
        out
    } else {
        w.music.to_path(&s.root)?
    };
    Some(Prepared {
        music,
        from: 0.0,
        length: 0.0,
        separate: false,
        target: ddi_session::music_target(&song),
        timing: song.timing,
    })
}

/// Writes `bytes` as the preview `out` in `dir` (whole, then renamed)
/// and cleans the folder ([`clean_previews`]).
fn cache_preview(dir: &Path, key: &str, out: &Path, bytes: &[u8]) -> Option<()> {
    std::fs::create_dir_all(dir).ok()?;
    let tmp = dir.join(format!(".{key}{}.part", std::process::id()));
    std::fs::write(&tmp, bytes).ok()?;
    std::fs::rename(&tmp, out).ok()?;
    clean_previews(dir, key, out);
    Some(())
}

/// Extracted previews kept at most.
const PREVIEWS_KEPT: usize = 64;

/// The previews folder without the song's older extractions (`key`, its
/// id's prefix) and with only the newest [`PREVIEWS_KEPT`] files.
fn clean_previews(dir: &Path, key: &str, keep: &Path) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    let mut files: Vec<(u64, PathBuf)> = Vec::new();
    for e in entries.flatten() {
        let path = e.path();
        if path == keep {
            continue;
        }
        let name = e.file_name().to_string_lossy().into_owned();
        if name.starts_with(key) {
            let _ = std::fs::remove_file(&path);
            continue;
        }
        if let Ok(meta) = e.metadata() {
            files.push((files::modified(&meta), path));
        }
    }
    files.sort();
    let over = (files.len() + 1).saturating_sub(PREVIEWS_KEPT);
    for (_, path) in files.into_iter().take(over) {
        let _ = std::fs::remove_file(path);
    }
}

#[cfg(test)]
pub(crate) mod tests;

#[cfg(test)]
mod managed_merge_tests {
    use super::*;

    fn song(id: &str, title: &str) -> LibSong {
        LibSong {
            root: PathBuf::from("/synthetic"),
            zip: false,
            dir: Default::default(),
            pack: "Test".into(),
            pack_banner: None,
            entry: serde_json::from_value(serde_json::json!({
                "id": id, "title": title, "artist": "Test", "chart": "chart.ssc", "music": "music.opus"
            })).unwrap(),
            warnings: Vec::new(),
            notes: Vec::new(),
            work: None,
        }
    }

    #[test]
    fn named_pack_restore_keeps_unselected_and_folder_rows_hidden() {
        let mut rows = vec![
            song("restored-a", "A"),
            song("other-a", "B"),
            song("folder-a", "C"),
            song("restored-b", "D"),
        ];
        for row in &mut rows[..3] {
            row.pack = "Collection A".into();
        }
        rows[3].pack = "Collection B".into();
        let mut hidden = Hidden {
            packs: vec![
                "Collection A".into(),
                "Collection B".into(),
                "Unrelated".into(),
            ],
            songs: vec![],
        };
        assert!(hidden.show_selected(&rows, &["restored-a", "restored-b"]));
        assert!(!hidden.hides(&rows[0]));
        assert!(hidden.hides(&rows[1]));
        assert!(hidden.hides(&rows[2]));
        assert!(!hidden.hides(&rows[3]));
        assert_eq!(hidden.packs, ["Unrelated"]);
    }
    #[test]
    fn managed_import_restore_preserves_other_hidden_rows() {
        let mut hidden = Hidden {
            packs: vec!["DanOni imports".into(), "Other pack".into()],
            songs: vec!["separately-hidden".into()],
        };
        assert!(hidden.show_only(
            "DanOni imports",
            &["reimported", "unrelated", "separately-hidden"],
            &["reimported"]
        ));
        assert!(!hidden.packs.contains(&"DanOni imports".into()));
        assert!(hidden.packs.contains(&"Other pack".into()));
        assert!(!hidden.songs.contains(&"reimported".into()));
        assert!(hidden.songs.contains(&"unrelated".into()));
        assert!(hidden.songs.contains(&"separately-hidden".into()));
        assert!(!hidden.show_only(
            "DanOni imports",
            &["reimported", "unrelated"],
            &["reimported"]
        ));
    }

    #[test]
    fn hidden_packs_ignore_unicode_case_as_song_select_does() {
        let mut rows = vec![song("a", "A"), song("b", "B"), song("c", "C")];
        rows[0].pack = "Études Ω".into();
        rows[1].pack = "ÉTUDES ω".into();
        rows[2].pack = "Etudes".into();
        // Hidden under one casing, as stored before: the other casing,
        // which song select shows as the same pack, is hidden too.
        let mut hidden: Hidden =
            serde_json::from_str(r#"{"packs":["ÉTUDES Ω"],"songs":[]}"#).unwrap();
        assert!(hidden.hides(&rows[0]) && hidden.hides(&rows[1]));
        assert!(!hidden.hides(&rows[2]));
        // Once, under whichever casing.
        hidden.hide_pack("études ω");
        assert_eq!(hidden.packs, ["ÉTUDES Ω"]);
        // Shown under the other casing, back with its songs; stored as it was.
        assert!(hidden.show_pack(&rows, "Études Ω"));
        assert!(!hidden.hides(&rows[0]) && !hidden.hides(&rows[1]));
        assert_eq!(
            serde_json::to_string(&hidden).unwrap(),
            r#"{"packs":[],"songs":[]}"#
        );
        assert!(same_pack("ΟΔΟΣ", "οδος") && !same_pack("Étude", "Etude"));
    }

    #[test]
    fn imports_survive_cached_and_scanned_news_with_folder_priority() {
        let (tx, rx) = mpsc::channel();
        let mut library = Library {
            songs: Vec::new(),
            folder_songs: Vec::new(),
            managed_songs: Vec::new(),
            report: Report::default(),
            scanning: true,
            from_cache: false,
            rx: Some(rx),
            cached_ms: None,
            generation: Arc::new(AtomicU64::new(1)),
            wake: Arc::new(|| {}),
            cache: None,
        };
        let mut imported = song("managed-test-import", "Imported");
        imported.pack = "Named collection".into();
        let folder = song("managed-test-folder", "Folder");
        assert!(library.set_managed(vec![imported.clone(), imported.clone()]));
        assert_eq!(library.songs, vec![imported.clone()]);
        assert!(!library.rereadable_packs().contains("named collection"));
        tx.send((1, News::Cached(vec![folder.clone()], 1.0)))
            .unwrap();
        assert!(library.take());
        assert_eq!(library.songs, vec![folder.clone(), imported.clone()]);
        assert!(
            library
                .rereadable_packs()
                .contains(&folder.pack.to_lowercase())
        );
        assert!(!library.rereadable_packs().contains("named collection"));
        let mut collision = imported.clone();
        collision.entry.title = "Folder copy wins".into();
        tx.send((
            1,
            News::Scanned(vec![folder.clone(), collision.clone()], Report::default()),
        ))
        .unwrap();
        assert!(library.take());
        assert_eq!(library.songs, vec![folder.clone(), collision]);
        tx.send((1, News::Scanned(vec![folder.clone()], Report::default())))
            .unwrap();
        assert!(library.take());
        assert_eq!(library.songs, vec![folder.clone(), imported.clone()]);
        assert!(!library.set_managed(vec![imported.clone()]));
        preps()
            .lock()
            .unwrap()
            .push((imported.entry.id.clone(), Prep::Failed));
        let mut replaced = imported;
        replaced.root = PathBuf::from("/replacement");
        assert!(library.set_managed(vec![replaced.clone()]));
        assert!(
            !preps()
                .lock()
                .unwrap()
                .iter()
                .any(|(id, _)| id == &replaced.entry.id)
        );
        assert_eq!(library.songs, vec![folder.clone(), replaced]);
        assert!(library.set_managed(Vec::new()));
        assert_eq!(library.songs, vec![folder]);
    }
}
