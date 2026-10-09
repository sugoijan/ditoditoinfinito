//! Importing user songs: picked or dropped files (folders, loose files, zips)
//! become songs in the IndexedDB library ([`crate::songs`]).
//!
//! Zips are read by slicing the `File`: the central directory first, then
//! only the entries a song uses, one at a time, so a pack of several hundred
//! megabytes never sits in wasm memory. Stored (uncompressed) entries, which
//! is how most packs keep their audio, go into storage as a slice of the zip
//! without being read at all.

mod danoni;

use std::cell::RefCell;
use std::collections::{HashMap, HashSet};
use std::rc::Rc;

use ddi_chart::formats::sm::parse_simfile;
use ddi_library::image::{IMAGE_HEADER_LEN, image_size};
use ddi_library::manifest::{EntryMeta, simfile_names, summarize};
use ddi_library::pack::{self, SongCandidate};
use ddi_library::zip::{self, Locate, Method, ZipEntry};
use wasm_bindgen_futures::spawn_local;
use web_sys::{Blob, File};
use yew::Callback;

use crate::songs::{ImportedSong, MAX_BG_IMAGES, file_key};
use crate::web::files::{PickedFile, blob_bytes, bytes_blob};
use crate::web::idb::{self, Db, Write};
use crate::web::js_err;

/// What an import did, for the panel under the pickers.
#[derive(Clone, Debug, Default, PartialEq)]
pub(crate) struct ImportReport {
    /// Titles of the songs stored.
    pub(crate) imported: Vec<String>,
    /// (song folder or file, reason) for everything left out: simfiles
    /// that could not be imported, folders with music but no simfile.
    pub(crate) skipped: Vec<(String, String)>,
    /// (song folder, problem) for songs imported with something missing.
    pub(crate) warnings: Vec<(String, String)>,
    /// (song folder, detail) for inconsistencies that make no difference
    /// in play (a renamed file that a stand-in replaces, background videos):
    /// shown to pack authors who ask for details.
    pub(crate) notes: Vec<(String, String)>,
    /// Images stored from shared background folders (`RandomMovies`,
    /// `SongMovies`).
    pub(crate) shared_images: usize,
    /// Other files in shared folders, left out (videos are not played).
    pub(crate) shared_skipped: usize,
    /// Songs removed because their folder is gone from a re-read folder.
    pub(crate) removed: usize,
    /// Set when the import could not run at all.
    pub(crate) error: Option<String>,
}

impl ImportReport {
    pub(crate) fn failed(e: String) -> ImportReport {
        ImportReport {
            error: Some(e),
            ..ImportReport::default()
        }
    }
}

/// Import state shared by every song list instance: an import keeps running
/// when the list unmounts (the player opens a song meanwhile), and the next
/// list must show its progress and result.
#[derive(Default)]
struct Shared {
    /// Progress text while an import runs.
    running: Option<String>,
    report: Option<ImportReport>,
    /// Incremented when an import finishes.
    generation: u64,
    watcher: Option<Callback<()>>,
}

thread_local! {
    static SHARED: RefCell<Shared> = RefCell::default();
}

fn notify(update: impl FnOnce(&mut Shared)) {
    let watcher = SHARED.with(|s| {
        let mut s = s.borrow_mut();
        update(&mut s);
        s.watcher.clone()
    });
    if let Some(w) = watcher {
        w.emit(());
    }
}

/// Progress text while an import runs, the last report, and the number of
/// finished imports (to tell when the library needs reloading).
pub(crate) fn status() -> (Option<String>, Option<ImportReport>, u64) {
    SHARED.with(|s| {
        let s = s.borrow();
        (s.running.clone(), s.report.clone(), s.generation)
    })
}

/// The song list currently shown; `None` when it unmounts.
pub(crate) fn watch(watcher: Option<Callback<()>>) {
    SHARED.with(|s| s.borrow_mut().watcher = watcher);
}

/// Shows an error in the import panel without importing.
pub(crate) fn report_error(e: String) {
    notify(|s| s.report = Some(ImportReport::failed(e)));
}

/// Starts importing unless an import is already running.
pub(crate) fn start(files: Vec<PickedFile>) -> bool {
    start_with(files, None)
}

/// Where an import's files come from when that is a kept folder
/// ([`crate::web::folders`]).
pub(crate) struct Origin {
    /// The folder's storage key ([`crate::songs::keep_folder`]), recorded on
    /// every song imported, so the folder can be re-read.
    pub(crate) folder: String,
    /// The folder is read again: its songs whose folders are gone are
    /// removed.
    pub(crate) rescan: bool,
    /// Leave music and backgrounds in the folder and read them from it
    /// when played, instead of copying them into the browser.
    pub(crate) link: bool,
}

/// [`start`] for the files of a kept folder.
pub(crate) fn start_from(files: Vec<PickedFile>, origin: Origin) -> bool {
    start_with(files, Some(origin))
}

fn start_with(files: Vec<PickedFile>, origin: Option<Origin>) -> bool {
    let busy = SHARED.with(|s| s.borrow().running.is_some());
    if busy || files.is_empty() {
        return false;
    }
    notify(|s| {
        s.running = Some("reading files…".into());
        s.report = None;
    });
    spawn_local(async move {
        let progress = Callback::from(|text: String| notify(|s| s.running = Some(text)));
        let report = run(files, progress, origin).await;
        notify(|s| {
            s.running = None;
            s.report = Some(report);
            s.generation += 1;
        });
    });
    true
}

/// Pack name for songs picked without a surrounding folder.
const LOOSE_PACK: &str = "Imported";

/// One file of the import, wherever its bytes live.
enum Source {
    File(File),
    /// An entry inside a picked zip.
    Zip {
        archive: File,
        /// The zip's own path in the import.
        archive_path: String,
        entry: ZipEntry,
    },
}

impl Source {
    /// Where the file stays in its kept folder, for a linked import whose
    /// paths start with the folder's own name.
    /// `None` when the folder handle could not open it again: Chromium
    /// lists names containing `\` but refuses to open them, so such files
    /// are copied.
    fn link(&self, path: &str) -> Option<crate::songs::Link> {
        let inside = |p: &str| p.split_once('/').map_or(p, |(_, rest)| rest).to_string();
        let link = match self {
            Source::File(_) => crate::songs::Link {
                path: inside(path),
                entry: None,
            },
            Source::Zip {
                archive_path,
                entry,
                ..
            } => crate::songs::Link {
                path: inside(archive_path),
                entry: Some(entry.name.clone()),
            },
        };
        (!link.path.contains('\\')).then_some(link)
    }
}

/// Zip directories read for linked songs: `(name, size, modified)` of the
/// archive, and its entries.
type ZipDirKey = (String, f64, f64);

thread_local! {
    static ZIP_DIRS: std::cell::RefCell<Vec<(ZipDirKey, Rc<Vec<ZipEntry>>)>> =
        const { std::cell::RefCell::new(Vec::new()) };
}

/// Zip directories kept (a song's files come from one or two archives).
const ZIP_DIRS_KEPT: usize = 4;

/// A zip entry by name, as a blob ([`crate::songs::Link`] to a file in a
/// zip): the archive's directory is read again, so a zip rewritten since
/// the import still works while the entry exists.
pub(crate) async fn zip_entry_blob(archive: &File, name: &str) -> Result<Blob, String> {
    // A song reads several files of one zip (music, background images): its
    // directory is read once while the file is unchanged.
    let key = (archive.name(), archive.size(), archive.last_modified());
    let cached = ZIP_DIRS.with(|c| {
        c.borrow()
            .iter()
            .find(|(k, _)| *k == key)
            .map(|(_, e)| e.clone())
    });
    let entries = match cached {
        Some(e) => e,
        None => {
            let e = Rc::new(zip_entries(archive).await?);
            ZIP_DIRS.with(|c| {
                let mut c = c.borrow_mut();
                c.retain(|(k, _)| *k != key);
                c.push((key, e.clone()));
                if c.len() > ZIP_DIRS_KEPT {
                    c.remove(0);
                }
            });
            e
        }
    };
    // The import kept the last of two entries with one name.
    let entry = entries
        .iter()
        .rev()
        .find(|e| e.name == name)
        .cloned()
        .ok_or_else(|| format!("{name} is no longer in {}", archive.name()))?;
    let source = Source::Zip {
        archive: archive.clone(),
        archive_path: archive.name(),
        entry,
    };
    read_blob(&source, mime_type(name)).await
}

async fn run(
    files: Vec<PickedFile>,
    progress: Callback<String>,
    origin: Option<Origin>,
) -> ImportReport {
    let rescan = origin.as_ref().is_some_and(|o| o.rescan);
    // Zips that could not be read: their songs are not gone.
    let mut unreadable: Vec<String> = Vec::new();
    let mut report = ImportReport::default();
    // A single picked zip names the pack for songs at its root.
    let zips = files.iter().filter(|f| is_zip(&f.path)).count();
    let fallback = match (zips, files.as_slice()) {
        (1, [only]) => stem(&only.path).to_string(),
        _ => LOOSE_PACK.to_string(),
    };
    let mut sources: HashMap<String, Source> = HashMap::new();
    for f in files {
        if !is_zip(&f.path) {
            sources.insert(f.path, Source::File(f.file));
            continue;
        }
        progress.emit(format!("reading {}…", f.path));
        match zip_entries(&f.file).await {
            Ok(entries) => {
                // `Packs/Foo.zip` → `Packs/Foo/<entry>`: the zip acts as a folder.
                let root = f.path[..f.path.len() - ".zip".len()].to_string();
                for entry in entries.into_iter().filter(|e| !e.is_dir) {
                    let path = format!("{root}/{}", entry.name);
                    sources.insert(
                        path,
                        Source::Zip {
                            archive: f.file.clone(),
                            archive_path: f.path.clone(),
                            entry,
                        },
                    );
                }
            }
            Err(e) => {
                unreadable.push(f.path[..f.path.len() - ".zip".len()].to_string());
                report.skipped.push((f.path, e));
            }
        }
    }
    let mut paths: Vec<String> = sources.keys().cloned().collect();
    paths.sort();
    let candidates = pack::find_songs(&paths, &fallback);
    // Shared background folders: every file, for telling what exists, and
    // the images, which are stored for the whole library.
    // Nothing inside a song folder is shared, even under a shared name.
    let in_song = |p: &str| {
        candidates
            .iter()
            .any(|c| !c.dir.is_empty() && p.starts_with(&format!("{}/", c.dir)))
    };
    let shared_files: Vec<(String, String)> = paths
        .iter()
        .filter(|p| !in_song(p))
        .filter_map(|p| Some((pack::shared_path(p)?, p.clone())))
        .collect();
    let maybe_works = !ddi_library::danoni::work_sources(&paths).is_empty();
    // A folder read again with nothing left in it still drops its songs.
    if candidates.is_empty() && shared_files.is_empty() && !maybe_works && !rescan {
        report_music_without_simfile(&mut report, &paths, &HashSet::new());
        if report.skipped.is_empty() {
            report.error = Some(NOTHING_FOUND.into());
        }
        return report;
    }
    let db = match Db::open().await {
        Ok(db) => db,
        Err(e) => {
            report.error = Some(e);
            return report;
        }
    };
    let images: Vec<&(String, String)> = shared_files.iter().filter(|(s, _)| is_image(s)).collect();
    report.shared_skipped = shared_files.len() - images.len();
    if !images.is_empty() {
        progress.emit(format!(
            "storing {} shared background images…",
            images.len()
        ));
        let (stored, failed) = store_shared(&db, &images, &sources).await;
        report.shared_images = stored;
        report.skipped.extend(failed);
    }
    // What shared folders hold: this import's files and the images already
    // in the library.
    let mut shared: Vec<String> = shared_files.iter().map(|(s, _)| s.clone()).collect();
    shared.extend(crate::songs::shared_paths(&db).await.unwrap_or_default());
    let n = candidates.len();
    // Two folders with the same pack and song folder names share an id.
    let mut seen: HashMap<String, String> = HashMap::new();
    for (i, c) in candidates.iter().enumerate() {
        let (id, name) = identity(c);
        let label = if c.dir.is_empty() {
            c.chart.clone()
        } else {
            c.dir.clone()
        };
        progress.emit(format!("importing {}/{n}: {name}", i + 1));
        if let Some(first) = seen.get(&id) {
            report
                .skipped
                .push((label, format!("same pack and folder name as {first}")));
            continue;
        }
        seen.insert(id.clone(), label.clone());
        let inputs = Inputs {
            sources: &sources,
            paths: &paths,
            shared: &shared,
            folder: origin.as_ref().map(|o| o.folder.as_str()),
            link: origin.as_ref().is_some_and(|o| o.link),
        };
        match import_song(&db, c, &id, &name, &inputs).await {
            Ok((title, warnings, notes)) => {
                report.imported.push(title);
                report
                    .warnings
                    .extend(warnings.into_iter().map(|w| (label.clone(), w)));
                report
                    .notes
                    .extend(notes.into_iter().map(|n| (label.clone(), n)));
            }
            Err(e) => report.skipped.push((label, e)),
        }
    }
    store_pack_art(&db, &candidates, &sources, &paths, &mut report).await;
    let works = danoni::import_works(
        &db,
        &sources,
        &paths,
        &fallback,
        &mut report,
        &progress,
        origin.as_ref(),
    )
    .await;
    report_music_without_simfile(&mut report, &paths, &works.music);
    if let Some(origin) = origin.as_ref().filter(|o| o.rescan) {
        match remove_gone(&db, &origin.folder, &paths, &unreadable).await {
            Ok(n) => report.removed = n,
            Err(e) => report
                .skipped
                .push(("removing songs gone from the folder".into(), e)),
        }
    }
    // Once every song is written: shared records nothing refers to any more
    // (works imported again under other fields, unused folders).
    if (works.found || !report.imported.is_empty() || rescan)
        && let Err(e) = crate::songs::drop_unused_shared(&db).await
    {
        report.skipped.push(("stored works and folders".into(), e));
    }
    if candidates.is_empty() && shared_files.is_empty() && !works.found && report.skipped.is_empty()
    {
        report.error = Some(NOTHING_FOUND.into());
    }
    report
}

const NOTHING_FOUND: &str =
    "No .sm, .ssc or .dwi files or Dancing☆Onigiri works were found in what was picked.";

/// Folders with music but no simfile, except the folders of music that
/// Dancing☆Onigiri works used.
fn report_music_without_simfile(
    report: &mut ImportReport,
    paths: &[String],
    used: &HashSet<String>,
) {
    let used_dirs: HashSet<&str> = used
        .iter()
        .map(|p| p.rsplit_once('/').map_or("", |(d, _)| d))
        .collect();
    for (dir, music) in pack::folders_without_simfile(paths) {
        if used_dirs.contains(dir.as_str()) {
            continue;
        }
        report.skipped.push((
            dir,
            format!(
                "has music ({}) but no .sm, .ssc or .dwi simfile",
                file_name(&music)
            ),
        ));
    }
}

/// Id and fallback name of a song. Loose files picked without a folder all
/// sit at the root, so they are told apart by the simfile's name instead.
fn identity(c: &SongCandidate) -> (String, String) {
    if c.dir.is_empty() {
        let name = stem(&c.chart);
        (pack::song_id(&c.pack, name), name.to_string())
    } else {
        (c.id(), c.dir_name().to_string())
    }
}

/// What an import reads songs from.
struct Inputs<'a> {
    sources: &'a HashMap<String, Source>,
    paths: &'a [String],
    /// Files in the library's shared background folders.
    shared: &'a [String],
    /// The kept folder the files come from, recorded on each song (so it
    /// can be re-read).
    folder: Option<&'a str>,
    /// Link files in that folder rather than copy them.
    link: bool,
}

async fn import_song(
    db: &Db,
    c: &SongCandidate,
    id: &str,
    name: &str,
    inputs: &Inputs<'_>,
) -> Result<(String, Vec<String>, Vec<String>), String> {
    let Inputs {
        sources,
        paths,
        shared,
        folder,
        link,
    } = *inputs;
    let linked = link && folder.is_some();
    let mut warnings = Vec::new();
    let mut notes = Vec::new();
    let chart_bytes = read_bytes(source(sources, &c.chart)?).await?;
    let text = pack::decode_text_with(&chart_bytes, shift_jis);
    drop(chart_bytes);
    let song = parse_simfile(&text, c.format.extension()).map_err(|e| e.to_string())?;
    let mut unplayed: Vec<&str> = song
        .charts
        .iter()
        .filter(|ch| song.layout_of(ch).is_none())
        .map(|ch| ch.layout.as_str())
        .collect();
    unplayed.sort_unstable();
    unplayed.dedup();
    if song.playable_charts().next().is_none() {
        return Err(if unplayed.is_empty() {
            "the simfile has no charts".into()
        } else {
            format!(
                "no charts in a layout this game plays; found {}",
                unplayed.join(", ")
            )
        });
    }
    if !unplayed.is_empty() {
        notes.push(format!(
            "charts in layouts this game does not play were left out: {}",
            unplayed.join(", ")
        ));
    }
    let mut assets = pack::resolve_assets(&song, &c.dir, paths);
    if (assets.banner.is_none() || assets.background.is_none()) && !assets.unclassified.is_empty() {
        // StepMania's last resort: tell banners from backgrounds by size.
        let mut sizes = Vec::new();
        for path in &assets.unclassified {
            let Ok(src) = source(sources, path) else {
                continue;
            };
            if let Ok(head) = read_head(src, IMAGE_HEADER_LEN as u64).await
                && let Some(size) = image_size(&head)
            {
                sizes.push((path.clone(), size));
            }
        }
        pack::classify_images(&mut assets, &sizes);
    }
    let music = assets
        .music
        .ok_or("no audio file found next to the simfile")?;
    // A file the simfile names but the folder lacks is only worth a warning
    // when nothing stands in for it: StepMania quietly uses the folder's
    // audio or images instead (`Song::TidyUpData`), and packs often rename
    // files without updating the simfile. Missing music with no stand-in
    // already stops the import above.
    for (tag, reference) in &assets.missing {
        let stand_in = match *tag {
            "#BANNER" => assets.banner.is_some(),
            "#BACKGROUND" => assets.background.is_some(),
            _ => true,
        };
        if !stand_in {
            warnings.push(format!(
                "{tag} names {reference}, which is missing; the song has no {}",
                tag.trim_start_matches('#').to_lowercase()
            ));
        } else {
            let used = match *tag {
                "#MUSIC" => Some(&music),
                "#BANNER" => assets.banner.as_ref(),
                _ => assets.background.as_ref(),
            };
            if let Some(used) = used {
                notes.push(format!(
                    "{tag} names {reference}, which is missing; {} is used instead",
                    file_name(used)
                ));
            }
        }
    }
    let id = id.to_string();
    let (title, artist) = simfile_names(&song, name);
    let named = |role: &str, path: &str| match extension(path) {
        Some(ext) => format!("{role}.{ext}"),
        None => role.to_string(),
    };
    let chart_name = format!("chart.{}", c.format.extension());
    let music_name = named("music", &music);
    let banner = assets.banner.map(|p| (named("banner", &p), p));
    let background = assets.background.map(|p| (named("background", &p), p));
    // Images shown by background changes, stored under `bg/<path>`.
    let mut bg_images = ddi_library::backgrounds::referenced_images(
        &song,
        &c.dir,
        paths,
        background.as_ref().map(|(_, path)| path.as_str()),
    );
    if bg_images.len() > MAX_BG_IMAGES {
        warnings.push(format!(
            "{} background-change images; only the first {MAX_BG_IMAGES} are kept",
            bg_images.len()
        ));
        bg_images.truncate(MAX_BG_IMAGES);
    }
    let missing = ddi_library::backgrounds::missing_images(&song, &c.dir, paths, &c.pack, shared);
    let unshown = ddi_library::backgrounds::unshown_files(&song, &c.dir, paths, &c.pack, shared);
    let (present, absent): (Vec<_>, Vec<_>) = unshown.into_iter().partition(|(_, exists)| *exists);
    let names = |list: Vec<(String, bool)>| {
        list.into_iter()
            .map(|(n, _)| n)
            .collect::<Vec<_>>()
            .join(", ")
    };
    if !present.is_empty() {
        notes.push(format!(
            "background changes use videos or animations, which are not played (the song background shows instead): {}",
            names(present)
        ));
    }
    if !absent.is_empty() {
        notes.push(format!(
            "background changes name videos or animations that are missing (they would not be played anyway): {}",
            names(absent)
        ));
    }
    if !missing.is_empty() {
        warnings.push(format!(
            "background changes name images that were not found (the song background shows instead): {}",
            missing.join(", ")
        ));
    }
    let mut entry = summarize(
        EntryMeta {
            id: id.clone(),
            title: title.clone(),
            artist,
            chart: chart_name.clone(),
            music: music_name.clone(),
            banner: banner.as_ref().map(|(n, _)| n.clone()),
            background: background.as_ref().map(|(n, _)| n.clone()),
            bg_images: bg_images.iter().map(|(r, _)| r.clone()).collect(),
            bg_shared: ddi_library::backgrounds::absent_images(&song, &c.dir, paths),
            credit: song.credit.trim().to_string(),
        },
        &song,
    );
    // serde_json writes a non-finite float as `null`, which would not read
    // back and would hide the song (`#SAMPLESTART:1e999` parses to inf).
    for v in [&mut entry.preview_start, &mut entry.preview_length] {
        if !v.is_finite() {
            *v = 0.0;
        }
    }
    // Charts are stored as decoded UTF-8 so loading never guesses again.
    let mut files: Vec<(String, Blob)> = vec![(
        chart_name,
        bytes_blob(text.as_bytes(), "text/plain;charset=utf-8")?,
    )];
    // Linked: music and backgrounds stay in the folder (the banner is
    // copied, since the song list shows it before any folder is read).
    let mut links = std::collections::BTreeMap::new();
    let music_source = source(sources, &music)?;
    match music_source.link(&music).filter(|_| linked) {
        Some(l) => {
            links.insert(music_name, l);
        }
        None => files.push((
            music_name,
            read_blob(music_source, mime_type(&music)).await?,
        )),
    }
    let bg_files = bg_images
        .into_iter()
        .map(|(relative, path)| (format!("bg/{relative}"), path));
    for (name, path) in banner.into_iter() {
        // A broken image is not worth losing the song over.
        if let Ok(blob) = read_blob(source(sources, &path)?, mime_type(&path)).await {
            files.push((name, blob));
        }
    }
    for (name, path) in background.into_iter().chain(bg_files) {
        let src = source(sources, &path)?;
        match src.link(&path).filter(|_| linked) {
            Some(l) => {
                links.insert(name, l);
            }
            None => {
                if let Ok(blob) = read_blob(src, mime_type(&path)).await {
                    files.push((name, blob));
                }
            }
        }
    }
    let record = ImportedSong {
        entry,
        pack: c.pack.clone(),
        dir: c.dir.clone(),
        imported: js_sys::Date::now(),
        bytes: files.iter().map(|(_, b)| b.size()).sum(),
        work: None,
        folder: folder.map(str::to_string),
        links,
    };
    let json = serde_json::to_string(&record).map_err(|e| e.to_string())?;
    let mut writes = vec![Write::DeletePrefix {
        store: idb::FILES,
        prefix: file_key(&id, ""),
    }];
    writes.extend(files.into_iter().map(|(name, blob)| Write::Put {
        store: idb::FILES,
        key: file_key(&id, &name),
        value: blob.into(),
    }));
    writes.push(Write::Put {
        store: idb::SONGS,
        key: id,
        value: json.into(),
    });
    db.write(writes).await?;
    Ok((title, warnings, notes))
}

/// Shared images written per storage transaction at most: a large shared
/// folder neither sits in memory whole nor fails as a whole.
const SHARED_BATCH_BYTES: f64 = 32.0 * 1024.0 * 1024.0;

/// Stores the images of shared background folders, keyed by their path in
/// the folder ([`crate::songs::shared_key`]), replacing earlier copies, in
/// batches. Returns how many were stored and the ones that were not.
async fn store_shared(
    db: &Db,
    images: &[&(String, String)],
    sources: &HashMap<String, Source>,
) -> (usize, Vec<(String, String)>) {
    let mut stored = 0;
    let mut failed: Vec<(String, String)> = Vec::new();
    let mut batch: Vec<(String, Write)> = Vec::new();
    let mut batch_bytes = 0.0;
    for (i, (shared, path)) in images.iter().enumerate() {
        match async { read_blob(source(sources, path)?, mime_type(path)).await }.await {
            Ok(blob) => {
                batch_bytes += blob.size();
                batch.push((
                    path.clone(),
                    Write::Put {
                        store: idb::FILES,
                        key: crate::songs::shared_key(shared),
                        value: blob.into(),
                    },
                ));
            }
            Err(e) => failed.push((path.clone(), e)),
        }
        let last = i + 1 == images.len();
        if !batch.is_empty() && (batch_bytes >= SHARED_BATCH_BYTES || last) {
            let (paths, writes): (Vec<String>, Vec<Write>) = batch.drain(..).unzip();
            let n = writes.len();
            match db.write(writes).await {
                Ok(()) => stored += n,
                Err(e) => failed.extend(paths.into_iter().map(|p| (p, e.clone()))),
            }
            batch_bytes = 0.0;
        }
    }
    (stored, failed)
}

/// Removes the songs imported from the kept folder `folder` whose song
/// folder (or Dancing☆Onigiri page) is no longer in it. Songs inside a zip
/// that could not be read (`unreadable`, zip paths without `.zip`) stay.
async fn remove_gone(
    db: &Db,
    folder: &str,
    paths: &[String],
    unreadable: &[String],
) -> Result<usize, String> {
    let lower: Vec<String> = paths.iter().map(|p| p.to_lowercase()).collect();
    let present = |dir: &str| {
        let dir = dir.to_lowercase();
        let inside = format!("{dir}/");
        let under = |root: &String| {
            let root = root.to_lowercase();
            dir == root || dir.starts_with(&format!("{root}/"))
        };
        lower.iter().any(|p| *p == dir || p.starts_with(&inside)) || unreadable.iter().any(under)
    };
    let gone: Vec<String> = db
        .entries(idb::SONGS)
        .await?
        .into_iter()
        .filter_map(|(id, v)| {
            let song: ImportedSong = serde_json::from_str(&v.as_string()?).ok()?;
            let from_here = song.folder.as_deref() == Some(folder);
            (from_here && !song.dir.is_empty() && !present(&song.dir)).then_some(id)
        })
        .collect();
    if !gone.is_empty() {
        crate::songs::delete_imported(&gone).await?;
    }
    Ok(gone.len())
}

/// Stores the banner of each pack this import holds songs of
/// ([`pack::pack_banner`]), replacing an earlier one; packs without one
/// keep what they had.
async fn store_pack_art(
    db: &Db,
    candidates: &[SongCandidate],
    sources: &HashMap<String, Source>,
    paths: &[String],
    report: &mut ImportReport,
) {
    let mut done: HashSet<String> = HashSet::new();
    let mut writes = Vec::new();
    for c in candidates {
        let Some(dir) = c.pack_dir() else { continue };
        if !done.insert(c.pack.to_lowercase()) {
            continue;
        }
        let Some(path) = pack::pack_banner(dir, paths) else {
            continue;
        };
        match async { read_blob(source(sources, &path)?, mime_type(&path)).await }.await {
            Ok(blob) => writes.push(Write::Put {
                store: idb::FILES,
                key: crate::songs::pack_art_key(&c.pack),
                value: blob.into(),
            }),
            Err(e) => report.warnings.push((path, format!("pack banner: {e}"))),
        }
    }
    if !writes.is_empty()
        && let Err(e) = db.write(writes).await
    {
        report.warnings.push(("pack banners".into(), e));
    }
}

fn is_image(path: &str) -> bool {
    extension(path).is_some_and(|e| {
        pack::IMAGE_EXTENSIONS
            .iter()
            .any(|x| e.eq_ignore_ascii_case(x))
    })
}

fn file_name(path: &str) -> &str {
    path.rsplit('/').next().unwrap_or(path)
}

fn source<'a>(sources: &'a HashMap<String, Source>, path: &str) -> Result<&'a Source, String> {
    sources
        .get(path)
        .ok_or_else(|| format!("{path} is missing"))
}

fn is_zip(path: &str) -> bool {
    extension(path).as_deref() == Some("zip")
}

/// Lowercase extension of the last path component.
fn extension(path: &str) -> Option<String> {
    let name = path.rsplit('/').next().unwrap_or(path);
    let (_, ext) = name.rsplit_once('.')?;
    (!ext.is_empty()).then(|| ext.to_ascii_lowercase())
}

/// File name without directories or extension.
fn stem(path: &str) -> &str {
    let name = path.rsplit('/').next().unwrap_or(path);
    name.rsplit_once('.').map_or(name, |(s, _)| s)
}

async fn read_range(blob: &Blob, start: u64, end: u64) -> Result<Vec<u8>, String> {
    let slice = blob
        .slice_with_f64_and_f64(start as f64, end as f64)
        .map_err(|e| js_err("Blob.slice", e))?;
    blob_bytes(&slice).await
}

/// The entry list of a zip, from its tail and central directory.
async fn zip_entries(file: &File) -> Result<Vec<ZipEntry>, String> {
    let len = file.size() as u64;
    let tail = read_range(file, len - zip::tail_len(len), len).await?;
    let cd = match zip::locate_central_directory(&tail, len).map_err(|e| e.to_string())? {
        Locate::Found(cd) => cd,
        Locate::NeedZip64Record { offset, len } => {
            let record = read_range(file, offset, offset + len).await?;
            zip::parse_zip64_end_record(&record).map_err(|e| e.to_string())?
        }
    };
    let bytes = read_range(file, cd.offset, cd.offset + cd.size).await?;
    let mut entries = zip::parse_central_directory(&bytes, &cd).map_err(|e| e.to_string())?;
    // Legacy names may be UTF-8 or Shift-JIS rather than CP437, decided
    // for the whole archive.
    let legacy: Vec<(usize, Vec<u8>)> = entries
        .iter_mut()
        .enumerate()
        .filter_map(|(i, e)| Some((i, e.legacy_name.take()?)))
        .collect();
    let raws: Vec<&[u8]> = legacy.iter().map(|(_, r)| r.as_slice()).collect();
    for ((i, _), name) in legacy
        .iter()
        .zip(pack::decode_legacy_names(&raws, shift_jis))
    {
        entries[*i].name = name;
    }
    Ok(entries)
}

/// Strict Shift-JIS through the browser's `TextDecoder` (`fatal`), `None`
/// on any invalid sequence.
pub(crate) fn shift_jis(bytes: &[u8]) -> Option<String> {
    let opts = web_sys::TextDecoderOptions::new();
    opts.set_fatal(true);
    web_sys::TextDecoder::new_with_label_and_options("shift_jis", &opts)
        .and_then(|d| d.decode_with_buffer_source(&js_sys::Uint8Array::from(bytes)))
        .ok()
}

/// Start of an entry's data, after its local header.
async fn zip_data_start(archive: &File, entry: &ZipEntry) -> Result<u64, String> {
    let at = entry.local_header_offset;
    let header = read_range(archive, at, at + zip::LOCAL_HEADER_LEN).await?;
    zip::local_data_offset(&header, entry).map_err(|e| e.to_string())
}

/// The first `len` bytes of a file (all of it if shorter). Deflated zip
/// entries are inflated whole; images are small enough for that.
async fn read_head(source: &Source, len: u64) -> Result<Vec<u8>, String> {
    match source {
        Source::File(f) => read_range(f, 0, len.min(f.size() as u64)).await,
        Source::Zip { archive, entry, .. }
            if entry.method == Method::Stored && !entry.encrypted =>
        {
            let start = zip_data_start(archive, entry).await?;
            read_range(archive, start, start + len.min(entry.compressed_size)).await
        }
        Source::Zip { .. } => {
            let mut bytes = read_bytes(source).await?;
            bytes.truncate(len as usize);
            Ok(bytes)
        }
    }
}

async fn read_bytes(source: &Source) -> Result<Vec<u8>, String> {
    match source {
        Source::File(f) => blob_bytes(f).await,
        Source::Zip { archive, entry, .. } => {
            let start = zip_data_start(archive, entry).await?;
            let data = read_range(archive, start, start + entry.compressed_size).await?;
            zip::extract(entry, &data).map_err(|e| e.to_string())
        }
    }
}

/// The file as a blob for storage. Stored zip entries are sliced out of the
/// archive without being read (and so without a CRC check, which the
/// decoder or image loader effectively does instead).
async fn read_blob(source: &Source, mime: &str) -> Result<Blob, String> {
    match source {
        Source::File(f) => Ok(f.clone().into()),
        Source::Zip { archive, entry, .. }
            if entry.method == Method::Stored && !entry.encrypted =>
        {
            let start = zip_data_start(archive, entry).await?;
            let end = start + entry.compressed_size;
            // `slice` clamps silently; a cut-off archive must not store a
            // cut-off song.
            if end > archive.size() as u64 {
                return Err(format!("{} is cut off (truncated zip)", entry.name));
            }
            archive
                .slice_with_f64_and_f64_and_content_type(start as f64, end as f64, mime)
                .map_err(|e| js_err("Blob.slice", e))
        }
        Source::Zip { .. } => bytes_blob(&read_bytes(source).await?, mime),
    }
}

/// Content type for a stored file, so blob URLs (`<audio>` previews,
/// banners) are served with one. Zip entries have none of their own.
fn mime_type(path: &str) -> &'static str {
    match extension(path).as_deref() {
        Some("ogg" | "oga" | "opus") => "audio/ogg",
        Some("mp3") => "audio/mpeg",
        Some("wav") => "audio/wav",
        Some("flac") => "audio/flac",
        Some("png") => "image/png",
        Some("jpg" | "jpeg") => "image/jpeg",
        Some("gif") => "image/gif",
        Some("bmp") => "image/bmp",
        Some("webp") => "image/webp",
        _ => "",
    }
}
