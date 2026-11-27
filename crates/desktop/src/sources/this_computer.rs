//! "This computer" (`docs/plans/source-plugins.md`, section 5 and stage
//! 2): the song source built into the app, searching folders the player
//! names for songs without adding them to the library. TENTATIVE: behind
//! its own switch, off by default (`--sources on|off`), apart from Add
//! songs and changing nothing of it, and easy to take out (this module,
//! its switch and its registration in [`super::Sources`]).
//!
//! What it reads: song folders (a folder with a simfile, `.ssc`, `.sm`
//! or `.dwi`) and zips (`.zip`, `.smzip`) of them, read by the library's
//! own scanner as Add songs' folders are. Not in this stage:
//! Dancing☆Onigiri works and FFR levels, which the library reads by its
//! own way and plays through its records of them.
//!
//! Bounded: a folder is walked at most [`Bounds::depth`] folders deep,
//! [`Bounds::entries`] entries and [`Bounds::songs`] songs, for at most
//! [`Bounds::time`] of work (time waiting while the game is quiet not
//! counted); a cut is said on the page ([`ThisComputer::cut`]). Symbolic
//! links are never followed, so nothing outside a folder named is read.
//! What it found is kept in memory until the page opens again
//! ([`ThisComputer::refresh`]); nothing is written to disk.
//!
//! A song found is fetched `in place`: its files where they are (a zip's
//! read into memory), read by the store's scanner into `src:` songs a play
//! reads as the library's (`super::fetched`).
//!
//! Imported, it is added to the library where it is (the maintainer,
//! 2026-11-27: "folder in place" instead of copying): its zip, else its
//! pack's folder (the folder it is in, where that is within the folder
//! named), else its own folder, as Add songs adds one; nothing where a
//! song folder of the library holds it already ([`ThisComputer::in_place`]).
//!
//! An item's id is a hash of where it is (a song folder's path, or a
//! zip's and the song's folder in it): the same each run while it stays
//! there, so a play's origin finds it again; moved, it is another item,
//! and the old one is gone.

use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use ddi_library::manifest::ManifestEntry;
use ddi_library::pack::{self, ChartFormat, PathIndex};
use ddi_library::path::{RelPath, RelPathBuf};
use ddi_library::scan::{self, Reader as _};
use ddi_library::search::{Query, haystack};
use ddi_source_host::builtin::{
    Answer, Browse, BrowsePage, Builtin, Context, Delivered, Failed, Opened, Page, Search,
};
use ddi_source_host::protocol::check::Accepted;
use ddi_source_host::protocol::{
    Capability, ChartMeta, Choice, Difficulty, FailCode, Filter, FilterKind, FilterValue, Input,
    InputKind, ItemKind, ItemMeta, Node, Step, Text, Unit,
};
use ddi_source_host::store::{ItemFiles, Stored};

use crate::library::files::ZipReader;

/// Its id: never a plugin's.
pub const ID: &str = "this-computer";

/// How far a folder's search goes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Bounds {
    /// Folders deep below the folder named.
    pub depth: usize,
    /// Files and folders looked at.
    pub entries: usize,
    /// Songs read.
    pub songs: usize,
    /// Work on one folder, waiting while the game is quiet not counted.
    pub time: Duration,
    /// A song's files (its folder walked at most 4 deep).
    pub song_files: usize,
}

impl Default for Bounds {
    fn default() -> Bounds {
        Bounds {
            depth: 8,
            entries: 200_000,
            songs: 20_000,
            time: Duration::from_secs(30),
            song_files: ddi_source_host::protocol::limits::MAX_FILES,
        }
    }
}

/// A song folder's own folders walked, at most (`bg/`, `movies/`).
const SONG_DEPTH: usize = 4;

/// What importing a song found adds to the library's song folders
/// ([`ThisComputer::in_place`]).
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum InPlace {
    /// This folder (or zip) is added.
    Add(PathBuf),
    /// This song folder of the library's holds it already: nothing added.
    Covered(PathBuf),
}

/// What importing a song at `place` adds, the player's folders `named`
/// searched and the library's song folders `listed`: its zip; else its
/// pack's folder, the folder its song folder is in where that is within a
/// folder named (that folder itself included), unless a listed folder is
/// inside it (it would be read twice), else its own folder; nothing where a
/// listed folder is it or holds it.
fn in_place_of(place: &Place, named: &[PathBuf], listed: &[PathBuf]) -> InPlace {
    let covered = |p: &Path| listed.iter().find(|f| p.starts_with(f)).cloned();
    let path = match place {
        Place::Zip { zip, .. } => zip.clone(),
        Place::Folder { dir, .. } => {
            dir.parent()
                .filter(|parent| named.iter().any(|n| dir != n && parent.starts_with(n)))
                .filter(|parent| !listed.iter().any(|f| f.starts_with(parent) && f != parent))
                .map_or_else(|| dir.clone(), Path::to_path_buf)
        }
    };
    match covered(&path) {
        Some(f) => InPlace::Covered(f),
        None => InPlace::Add(path),
    }
}

/// Where a song found is.
#[derive(Clone, Debug, PartialEq, Eq)]
enum Place {
    /// A song folder.
    Folder { dir: PathBuf, pack: String },
    /// A song in a zip, its folder inside it.
    Zip { zip: PathBuf, dir: RelPathBuf },
}

/// A song found, as searched and described.
#[derive(Clone, Debug)]
struct Found {
    meta: ItemMeta,
    haystack: String,
    place: Place,
    /// Its original charts' styles and meters.
    charts: Vec<(String, u32)>,
}

/// What a folder holds, as read once.
#[derive(Debug, Default)]
struct Index {
    songs: Vec<Found>,
    /// A bound cut its reading short.
    cut: bool,
}

/// The source.
pub struct ThisComputer {
    folders: Mutex<Vec<PathBuf>>,
    indexes: Mutex<HashMap<PathBuf, Arc<Index>>>,
    bounds: Bounds,
    /// A folder searched since the last refresh was cut short.
    cut: AtomicBool,
}

impl ThisComputer {
    pub fn new(folders: Vec<PathBuf>) -> ThisComputer {
        ThisComputer::with_bounds(folders, Bounds::default())
    }

    pub fn with_bounds(folders: Vec<PathBuf>, bounds: Bounds) -> ThisComputer {
        ThisComputer {
            folders: Mutex::new(folders),
            indexes: Mutex::default(),
            bounds,
            cut: AtomicBool::new(false),
        }
    }

    /// The folders searched when no scope is named.
    pub fn set_folders(&self, folders: Vec<PathBuf>) {
        if let Ok(mut f) = self.folders.lock() {
            *f = folders;
        }
    }

    pub fn folders(&self) -> Vec<PathBuf> {
        self.folders.lock().map(|f| f.clone()).unwrap_or_default()
    }

    /// What was found is forgotten: the next search reads the folders
    /// again (the page opened again).
    pub fn refresh(&self) {
        if let Ok(mut i) = self.indexes.lock() {
            i.clear();
        }
        self.cut.store(false, Ordering::Relaxed);
    }

    /// A folder searched since the last refresh was too large to read
    /// whole.
    pub fn cut(&self) -> bool {
        self.cut.load(Ordering::Relaxed)
    }

    /// The index of `folder`, read now where it was not.
    fn index(&self, cx: &Context, folder: &Path) -> Answer<Arc<Index>> {
        if let Some(i) = self.indexes.lock().ok().and_then(|i| i.get(folder).cloned()) {
            return Ok(i);
        }
        let index = Arc::new(read_folder(cx, folder, &self.bounds)?);
        if index.cut {
            self.cut.store(true, Ordering::Relaxed);
        }
        if let Ok(mut i) = self.indexes.lock() {
            i.insert(folder.to_path_buf(), index.clone());
        }
        Ok(index)
    }

    /// The folders a scope names: one (a folder opened, or one of the
    /// player's), or every one of the player's.
    fn scoped(&self, scope: Option<&str>) -> Vec<PathBuf> {
        match scope {
            Some(s) => vec![PathBuf::from(s)],
            None => self.folders(),
        }
    }

    /// What importing song `item` (found in what was read) adds to the
    /// library's song folders `listed` ([`InPlace`]); `None` where it is not
    /// known now.
    pub fn in_place(&self, item: &str, listed: &[PathBuf]) -> Option<InPlace> {
        let found = self.find(item)?;
        Some(in_place_of(&found.place, &self.folders(), listed))
    }

    /// A song found by its item id, in what was read.
    fn find(&self, item: &str) -> Option<Found> {
        let indexes = self.indexes.lock().ok()?;
        indexes
            .values()
            .flat_map(|i| i.songs.iter())
            .find(|f| f.meta.id == item)
            .cloned()
    }

    /// The song `item` found, reading the player's folders where it is
    /// not known yet (the page opened again since it was found).
    fn find_or_read(&self, cx: &Context, item: &str) -> Answer<Found> {
        if let Some(f) = self.find(item) {
            return Ok(f);
        }
        for folder in self.folders() {
            let _ = self.index(cx, &folder)?;
            if let Some(f) = self.find(item) {
                return Ok(f);
            }
        }
        Err(Failed::new(FailCode::NotFound))
    }
}

/// The style filter's choices: our layouts.
fn style_choices() -> Vec<Choice> {
    ddi_chart::layout::BUILTIN_LAYOUTS
        .iter()
        .map(|id| Choice {
            id: (*id).to_string(),
            label: labelled(|| ddi_ui::screens::data::style_name(id, id)),
        })
        .collect()
}

/// A label in every language the game speaks, from its message.
fn labelled(message: impl Fn() -> ddi_l10n::UiText) -> Text {
    let mut text = Text::default();
    for l in ddi_l10n::all() {
        let words = ddi_l10n::in_language(l, || message().as_str().to_string());
        text.0.insert(l.tag.to_string(), words);
    }
    text
}

impl Builtin for ThisComputer {
    fn id(&self) -> &str {
        ID
    }

    fn accepted(&self) -> Accepted {
        use ddi_l10n::msg::sources as m;
        Accepted {
            protocol: 1,
            capabilities: BTreeSet::from([
                Capability::Input,
                Capability::Search,
                Capability::Filters,
                Capability::Browse,
                Capability::Metadata,
                Capability::Fetch,
            ]),
            inputs: BTreeSet::from([InputKind::Folder]),
            filters: vec![
                Filter {
                    id: "style".into(),
                    label: labelled(m::filter_style),
                    kind: FilterKind::Choice {
                        multiple: true,
                        choices: style_choices(),
                    },
                },
                Filter {
                    id: "level".into(),
                    label: labelled(m::filter_level),
                    kind: FilterKind::Range {
                        min: 1.0,
                        max: 30.0,
                        step: 1.0,
                        unit: Unit::Meter,
                    },
                },
            ],
        }
    }

    fn open(&self, _cx: &Context, input: Input) -> Answer<Opened> {
        if input.kind != InputKind::Folder {
            return Err(Failed::new(FailCode::UnsupportedInput));
        }
        let path = PathBuf::from(&input.value);
        if !path.is_absolute() || !path.is_dir() {
            return Err(Failed::new(FailCode::InvalidInput));
        }
        let name = path
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_else(|| input.value.clone());
        Ok(Opened {
            scope: input.value,
            title: Some(Text::en(name)),
        })
    }

    fn search(&self, cx: &Context, search: Search) -> Answer<Page> {
        cx.progress(Step::Searching, None, None);
        let query = Query::new(&search.query, &crate::Nfkd);
        let mut matching: Vec<ItemMeta> = Vec::new();
        for folder in self.scoped(search.scope.as_deref()) {
            let index = self.index(cx, &folder)?;
            matching.extend(
                index
                    .songs
                    .iter()
                    .filter(|f| query.matches(&f.haystack) && passes(f, &search.filters))
                    .map(|f| f.meta.clone()),
            );
        }
        Ok(paged(matching, search.cursor.as_deref(), search.page_size))
    }

    fn browse(&self, cx: &Context, browse: Browse) -> Answer<BrowsePage> {
        let node = browse.node.or(browse.scope);
        let Some(node) = node else {
            return Ok(BrowsePage {
                nodes: self
                    .folders()
                    .iter()
                    .map(|f| Node {
                        id: f.display().to_string(),
                        title: f
                            .file_name()
                            .map(|n| n.to_string_lossy().into_owned())
                            .unwrap_or_else(|| f.display().to_string()),
                        items: None,
                        nodes: None,
                    })
                    .collect(),
                ..BrowsePage::default()
            });
        };
        // A folder's packs, or a pack's songs (`<folder>\u{1f}<pack>`).
        match node.split_once('\u{1f}') {
            None => {
                let folder = PathBuf::from(&node);
                let index = self.index(cx, &folder)?;
                let mut packs: BTreeMap<String, u64> = BTreeMap::new();
                for f in &index.songs {
                    *packs.entry(f.meta.pack.clone().unwrap_or_default()).or_default() += 1;
                }
                Ok(BrowsePage {
                    nodes: packs
                        .into_iter()
                        .map(|(pack, n)| Node {
                            id: format!("{node}\u{1f}{pack}"),
                            title: pack,
                            items: Some(n),
                            nodes: Some(0),
                        })
                        .collect(),
                    ..BrowsePage::default()
                })
            }
            Some((folder, pack)) => {
                let index = self.index(cx, Path::new(folder))?;
                let items = index
                    .songs
                    .iter()
                    .filter(|f| f.meta.pack.as_deref() == Some(pack))
                    .map(|f| f.meta.clone())
                    .collect();
                let page = paged(items, browse.cursor.as_deref(), browse.page_size);
                Ok(BrowsePage {
                    nodes: Vec::new(),
                    items: page.items,
                    cursor: page.cursor,
                    total: page.total,
                })
            }
        }
    }

    fn describe(&self, cx: &Context, item: &str) -> Answer<ItemMeta> {
        self.find_or_read(cx, item).map(|f| f.meta)
    }

    fn fetch(&self, cx: &Context, item: &str) -> Answer<Delivered> {
        let found = self.find_or_read(cx, item)?;
        cx.progress(Step::Reading, None, None);
        match &found.place {
            Place::Folder { dir, pack } => {
                let files = song_files(cx, dir, pack, &self.bounds)?;
                if files.is_empty() {
                    return Err(Failed::new(FailCode::Gone));
                }
                Ok(Delivered {
                    files: files
                        .into_iter()
                        .map(|(rel, (file, _))| (rel, Stored::Disk(file)))
                        .collect(),
                    pack: pack.clone(),
                })
            }
            Place::Zip { zip, dir } => {
                let reader = ZipReader::open(zip, &crate::library::legacy_text)
                    .map_err(|e| Failed::with(FailCode::Gone, e))?;
                let under = |p: &RelPath| {
                    dir.is_empty() || p.as_str().starts_with(&format!("{dir}/"))
                };
                let mut files = BTreeMap::new();
                let mut bytes = 0u64;
                for name in reader.names.iter().filter(|n| under(n)) {
                    cx.check()?;
                    // Movies are read by path: not taken from a zip here.
                    if ddi_library::video::is_movie(name.as_str()) {
                        continue;
                    }
                    bytes += reader.size(name).unwrap_or(0);
                    if bytes > ddi_source_host::StoreLimits::default().item_memory {
                        return Err(Failed::new(FailCode::TooLarge));
                    }
                    let data = reader
                        .read(name)
                        .map_err(|e| Failed::with(FailCode::UnreadableContent, e))?;
                    files.insert(name.clone(), Stored::Memory(data.into()));
                }
                Ok(Delivered {
                    files,
                    pack: found.meta.pack.clone().unwrap_or_default(),
                })
            }
        }
    }
}

/// Whether `f` passes the search's filters.
fn passes(f: &Found, filters: &BTreeMap<String, FilterValue>) -> bool {
    let styles: Option<&Vec<String>> = match filters.get("style") {
        Some(FilterValue::Choice { values }) if !values.is_empty() => Some(values),
        _ => None,
    };
    let range = match filters.get("level") {
        Some(FilterValue::Range { min, max }) => Some((*min, *max)),
        _ => None,
    };
    f.charts.iter().any(|(style, meter)| {
        styles.is_none_or(|s| s.contains(style))
            && range.is_none_or(|(lo, hi)| {
                let m = f64::from(*meter);
                lo.is_none_or(|lo| m >= lo) && hi.is_none_or(|hi| m <= hi)
            })
    })
}

/// A page of `all` from `cursor` (an offset).
fn paged(all: Vec<ItemMeta>, cursor: Option<&str>, size: u32) -> Page {
    let start = cursor
        .and_then(|c| c.parse::<usize>().ok())
        .unwrap_or(0)
        .min(all.len());
    let end = (start + size.max(1) as usize).min(all.len());
    Page {
        total: Some(all.len() as u64),
        cursor: (end < all.len()).then(|| end.to_string()),
        items: all[start..end].to_vec(),
    }
}

/// Counts work against a folder's time, waiting while the game is quiet
/// left out.
struct Clock {
    spent: Duration,
    since: Instant,
    budget: Duration,
}

impl Clock {
    fn new(budget: Duration) -> Clock {
        Clock {
            spent: Duration::ZERO,
            since: Instant::now(),
            budget,
        }
    }

    /// Between steps: cancelled, or waited while quiet; whether the time
    /// is up.
    fn step(&mut self, cx: &Context) -> Answer<bool> {
        self.spent += self.since.elapsed();
        cx.check()?;
        self.since = Instant::now();
        Ok(self.spent >= self.budget)
    }
}

/// `folder` read: its song folders and zips found and read as songs,
/// within `bounds`.
fn read_folder(cx: &Context, folder: &Path, bounds: &Bounds) -> Answer<Index> {
    if !folder.is_dir() {
        return Err(Failed::new(FailCode::NotFound));
    }
    let mut clock = Clock::new(bounds.time);
    let mut index = Index::default();
    let mut song_dirs: Vec<PathBuf> = Vec::new();
    let mut zips: Vec<PathBuf> = Vec::new();
    let mut entries = 0usize;
    let mut stack = vec![(folder.to_path_buf(), 0usize)];
    'walk: while let Some((dir, depth)) = stack.pop() {
        let Ok(read) = std::fs::read_dir(&dir) else {
            continue;
        };
        let mut subdirs = Vec::new();
        let mut song = false;
        for e in read.flatten() {
            entries += 1;
            if entries.is_multiple_of(256) && clock.step(cx)? {
                index.cut = true;
                break 'walk;
            }
            if entries > bounds.entries {
                index.cut = true;
                break 'walk;
            }
            let Some(name) = e.file_name().to_str().map(str::to_string) else {
                continue;
            };
            if pack::is_ignored(&name) {
                continue;
            }
            // Links are never followed: nothing outside the folder named.
            let Ok(kind) = e.file_type() else {
                continue;
            };
            if kind.is_dir() {
                subdirs.push(e.path());
            } else if kind.is_file() {
                if ChartFormat::of(&name).is_some() {
                    song = true;
                } else if ddi_library::zip::archive_stem(&name).is_some() {
                    zips.push(e.path());
                }
            }
        }
        if song {
            // A song folder holds no songs of its own.
            song_dirs.push(dir);
            continue;
        }
        if depth < bounds.depth {
            subdirs.sort();
            stack.extend(subdirs.into_iter().rev().map(|d| (d, depth + 1)));
        } else if !subdirs.is_empty() {
            index.cut = true;
        }
    }
    song_dirs.sort();
    zips.sort();
    let total = (song_dirs.len() + zips.len()) as u64;
    for (n, dir) in song_dirs.iter().enumerate() {
        if index.songs.len() >= bounds.songs {
            index.cut = true;
            break;
        }
        if clock.step(cx)? {
            index.cut = true;
            break;
        }
        if n % 64 == 0 {
            cx.progress(Step::Reading, Some(n as u64), Some(total));
        }
        let pack = pack_of(dir, folder);
        if let Some(found) = read_song_folder(cx, dir, &pack, bounds) {
            index.songs.push(found);
        }
    }
    for zip in &zips {
        if index.songs.len() >= bounds.songs || clock.step(cx)? {
            index.cut = true;
            break;
        }
        index.songs.extend(read_zip(zip));
    }
    index.songs.sort_by_cached_key(|f| {
        (
            f.meta.pack.clone().unwrap_or_default().to_lowercase(),
            f.meta.title.to_lowercase(),
        )
    });
    Ok(index)
}

/// The pack a song folder is in: its parent's name (the folder named's,
/// for a song folder named itself).
fn pack_of(dir: &Path, folder: &Path) -> String {
    let parent = if dir == folder {
        folder.parent()
    } else {
        dir.parent()
    };
    parent
        .and_then(Path::file_name)
        .map(|n| n.to_string_lossy().into_owned())
        .filter(|n| !n.is_empty())
        .unwrap_or_else(|| pack::LOOSE_PACK.to_string())
}

/// A song folder's files as the scanner names them (`Pack/Song/…`),
/// each with its file and size; links never followed.
fn song_files(
    cx: &Context,
    dir: &Path,
    pack: &str,
    bounds: &Bounds,
) -> Answer<BTreeMap<RelPathBuf, (PathBuf, u64)>> {
    let name = dir
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default();
    let base = format!("{pack}/{name}");
    let mut out = BTreeMap::new();
    let mut stack = vec![(dir.to_path_buf(), base, 0usize)];
    while let Some((d, prefix, depth)) = stack.pop() {
        cx.check()?;
        let Ok(read) = std::fs::read_dir(&d) else {
            continue;
        };
        for e in read.flatten() {
            let Some(n) = e.file_name().to_str().map(str::to_string) else {
                continue;
            };
            let rel = format!("{prefix}/{n}");
            if pack::is_ignored(&n) || RelPath::new(&rel).normalize().is_none() {
                continue;
            }
            let Ok(kind) = e.file_type() else {
                continue;
            };
            if kind.is_dir() && depth < SONG_DEPTH {
                stack.push((e.path(), rel, depth + 1));
            } else if kind.is_file() {
                if out.len() >= bounds.song_files {
                    return Err(Failed::new(FailCode::TooLarge));
                }
                let size = e.metadata().map(|m| m.len()).unwrap_or(0);
                out.insert(RelPathBuf::from(rel), (e.path(), size));
            }
        }
    }
    Ok(out)
}

/// A song folder read as the scanner reads it.
fn read_song_folder(cx: &Context, dir: &Path, pack: &str, bounds: &Bounds) -> Option<Found> {
    let files = song_files(cx, dir, pack, bounds).ok()?;
    let size: u64 = files.values().map(|(_, n)| n).sum();
    let paths: Vec<RelPathBuf> = files.keys().cloned().collect();
    let reader = ItemFiles(Arc::new(
        files
            .iter()
            .map(|(rel, (file, _))| (rel.clone(), Stored::Disk(file.clone())))
            .collect(),
    ));
    let c = pack::find_songs(&paths, pack).into_iter().next()?;
    let empty: [RelPathBuf; 0] = [];
    let scanned = scan::scan_song(
        &reader,
        &PathIndex::new(&paths),
        &c,
        &PathIndex::new(&empty),
        &crate::library::legacy_text,
    )
    .ok()?;
    let revision = files
        .get(&c.chart)
        .and_then(|(file, _)| std::fs::metadata(file).ok())
        .map(|m| stamp(&m));
    let id = item_id(&dir.display().to_string());
    Some(found(
        id,
        &scanned.entry,
        &scanned.pack,
        size,
        revision,
        Place::Folder {
            dir: dir.to_path_buf(),
            pack: pack.to_string(),
        },
    ))
}

/// A zip's songs, as the scanner reads them in it.
fn read_zip(zip: &Path) -> Vec<Found> {
    let Ok(reader) = ZipReader::open(zip, &crate::library::legacy_text) else {
        return Vec::new();
    };
    let stem = zip
        .file_name()
        .and_then(|n| n.to_str())
        .and_then(ddi_library::zip::archive_stem)
        .unwrap_or(pack::LOOSE_PACK)
        .to_string();
    let revision = std::fs::metadata(zip).ok().map(|m| stamp(&m));
    let scan = scan::scan_root(
        &reader,
        &reader.names,
        &stem,
        &[],
        &crate::library::legacy_text,
    );
    scan.songs
        .into_iter()
        .map(|s| {
            let prefix = format!("{}/", s.dir);
            let size = reader
                .names
                .iter()
                .filter(|n| s.dir.is_empty() || n.as_str().starts_with(&prefix))
                .filter_map(|n| reader.size(n))
                .sum();
            let id = item_id(&format!("{}!{}", zip.display(), s.dir));
            found(
                id,
                &s.entry,
                &s.pack,
                size,
                revision.clone(),
                Place::Zip {
                    zip: zip.to_path_buf(),
                    dir: s.dir.clone(),
                },
            )
        })
        .collect()
}

/// A file's size and time, as a revision.
fn stamp(m: &std::fs::Metadata) -> String {
    format!("{:x}-{:x}", m.len(), crate::library::files::modified(m))
}

/// An item's id: its place's hash.
fn item_id(place: &str) -> String {
    format!("{:016x}", pack::fnv1a64(place.as_bytes()))
}

/// A difficulty's name as ours.
fn difficulty(name: &str) -> Difficulty {
    match name.to_ascii_lowercase().as_str() {
        "beginner" => Difficulty::Beginner,
        "easy" => Difficulty::Easy,
        "medium" => Difficulty::Medium,
        "hard" => Difficulty::Hard,
        "challenge" => Difficulty::Challenge,
        _ => Difficulty::Edit,
    }
}

fn found(
    id: String,
    entry: &ManifestEntry,
    pack: &str,
    size: u64,
    revision: Option<String>,
    place: Place,
) -> Found {
    let originals: Vec<_> = entry
        .charts
        .iter()
        .filter(|c| c.derivation.is_none())
        .collect();
    let digests: Option<Vec<String>> = originals
        .iter()
        .map(|c| c.digest.map(|d| d.to_string()))
        .collect();
    let bpm: Vec<f64> = entry
        .bpm
        .split('-')
        .filter_map(|b| b.trim().parse::<f64>().ok())
        .collect();
    let meta = ItemMeta {
        id,
        kind: Some(ItemKind::Song),
        title: entry.title.clone(),
        artist: (!entry.artist.is_empty()).then(|| entry.artist.clone()),
        title_translit: (!entry.title_translit.is_empty()).then(|| entry.title_translit.clone()),
        artist_translit: (!entry.artist_translit.is_empty())
            .then(|| entry.artist_translit.clone()),
        pack: Some(pack.to_string()),
        charts: originals
            .iter()
            .take(ddi_source_host::protocol::limits::MAX_CHARTS)
            .map(|c| ChartMeta {
                style: c.layout.clone(),
                difficulty: difficulty(&c.difficulty),
                meter: Some(c.meter),
                name: (!c.name.is_empty()).then(|| c.name.clone()),
                author: (!c.credit.is_empty()).then(|| c.credit.clone()),
            })
            .collect(),
        length: (entry.length > 0.0).then_some(entry.length),
        bpm: match bpm.as_slice() {
            [b] => Some([*b, *b]),
            [lo, .., hi] => Some([*lo, *hi]),
            [] => None,
        },
        size: Some(size),
        revision,
        song_key: digests
            .filter(|d| !d.is_empty())
            .map(|d| ddi_library::share::song_key(d.iter().map(String::as_str))),
        ..ItemMeta::default()
    };
    let haystack = haystack(
        [entry.title.as_str(), entry.artist.as_str(), pack],
        [entry.title_translit.as_str(), entry.artist_translit.as_str()],
        &crate::Nfkd,
    );
    Found {
        charts: originals
            .iter()
            .map(|c| (c.layout.clone(), c.meter))
            .collect(),
        meta,
        haystack,
        place,
    }
}

#[cfg(test)]
mod tests;
