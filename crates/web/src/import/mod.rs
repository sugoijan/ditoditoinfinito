//! Importing user songs: picked or dropped files (folders, loose files, zips)
//! become songs in the IndexedDB library ([`crate::songs`]).
//!
//! Zips are read by slicing the `File`: the central directory first, then
//! only the entries a song uses, one at a time, so a pack of several hundred
//! megabytes never sits in wasm memory. Stored (uncompressed) entries, which
//! is how most packs keep their audio, go into storage as a slice of the zip
//! without being read at all.

mod danoni;
mod ffr;

use ddi_library::path::{RelPath, RelPathBuf};

use std::cell::RefCell;
use std::collections::{HashMap, HashSet};
use std::rc::Rc;

use crate::component::Callback;
use ddi_library::image::{IMAGE_HEADER_LEN, image_size};
use ddi_library::pack::{self, LOOSE_PACK, SongCandidate};
pub(crate) use ddi_library::report::ImportReport;
use ddi_library::report::{Finding, ImportFailure, ImportStep};
use ddi_library::scan::{self, Movie, Naming};
use ddi_library::zip::{self, Locate, Method, ZipEntry};
use wasm_bindgen_futures::spawn_local;
use web_sys::{Blob, File};

use crate::songs::{ImportedSong, file_key};
use crate::web::files::{PickedFile, blob_bytes, bytes_blob};
use crate::web::idb::{self, Db, Write};
use crate::web::js_err;

/// Import state shared by every song list instance: an import keeps running
/// when the list unmounts (the player opens a song meanwhile), and the next
/// list must show its progress and result.
#[derive(Default)]
struct Shared {
    /// What an import does now, while it runs.
    running: Option<ImportStep>,
    report: Option<ImportReport>,
    /// Incremented when an import finishes (and when the stills it queued
    /// are taken).
    generation: u64,
    /// The opening stills' progress, taken after the import.
    stills: Option<ImportStep>,
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

/// What an import does while it runs, the last report, and the number
/// of finished imports (to tell when the library needs reloading).
pub(crate) fn status() -> (Option<ImportStep>, Option<ImportReport>, u64) {
    SHARED.with(|s| {
        let s = s.borrow();
        (s.running.clone(), s.report.clone(), s.generation)
    })
}

/// The song list currently shown; `None` when it unmounts.
pub(crate) fn watch(watcher: Option<Callback<()>>) {
    SHARED.with(|s| s.borrow_mut().watcher = watcher);
}

/// Shows an error in the import panel without importing (the system's
/// words).
pub(crate) fn report_error(e: String) {
    notify(|s| s.report = Some(ImportReport::failed(ImportFailure::System(e))));
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
        s.running = Some(ImportStep::ReadingFiles);
        s.report = None;
    });
    spawn_local(async move {
        let progress = Callback::from(|step: ImportStep| notify(|s| s.running = Some(step)));
        let report = run(files, progress, origin).await;
        notify(|s| {
            s.running = None;
            s.report = Some(report);
            s.generation += 1;
        });
        take_stills();
    });
    true
}

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
    fn link(&self, path: impl AsRef<str>) -> Option<crate::songs::Link> {
        let path = path.as_ref();
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
    // The import kept the last of two entries with one name (encrypted
    // ones never listed).
    let entry = entries
        .iter()
        .rev()
        .find(|e| e.name == name && !e.encrypted)
        .cloned()
        .ok_or_else(|| format!("{name} is no longer in {}", archive.name()))?;
    let source = Source::Zip {
        archive: archive.clone(),
        archive_path: archive.name(),
        entry,
    };
    read_blob(&source, mime_type(name))
        .await
        .map_err(|f| ddi_ui::screens::import_report::finding_text(&f).into())
}

async fn run(
    files: Vec<PickedFile>,
    progress: Callback<ImportStep>,
    origin: Option<Origin>,
) -> ImportReport {
    let rescan = origin.as_ref().is_some_and(|o| o.rescan);
    // Zips that could not be read: their songs are not gone.
    let mut unreadable: Vec<String> = Vec::new();
    let mut report = ImportReport::default();
    // A single picked zip names the pack for songs at its root.
    let zips = files.iter().filter(|f| is_zip(&f.path)).count();
    let fallback = match (zips, files.as_slice()) {
        (1, [only]) => RelPath::new(&only.path).file_stem().to_string(),
        _ => LOOSE_PACK.to_string(),
    };
    let mut sources: HashMap<String, Source> = HashMap::new();
    for f in files {
        if !is_zip(&f.path) {
            sources.insert(f.path, Source::File(f.file));
            continue;
        }
        progress.emit(ImportStep::ReadingZip {
            path: f.path.clone(),
        });
        // `Packs/Foo.zip` → `Packs/Foo/<entry>`: the zip acts as a folder.
        let root = zip::archive_stem(&f.path).unwrap_or(&f.path).to_string();
        match zip_directory(&f.file).await {
            Ok(entries) => {
                for entry in entries.into_iter().filter(|e| !e.is_dir) {
                    let path = format!("{root}/{}", entry.name);
                    // Not read, as on desktop: said so, not listed (an
                    // earlier entry of the name stays).
                    if entry.encrypted {
                        report.skipped.push((path, Finding::Encrypted));
                        continue;
                    }
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
                unreadable.push(root);
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
    let song_dirs: Vec<String> = candidates
        .iter()
        .filter(|c| !c.dir.is_empty())
        .map(|c| format!("{}/", c.dir))
        .collect();
    let in_song = |p: &str| song_dirs.iter().any(|d| p.starts_with(d.as_str()));
    let shared_files: Vec<(String, String)> = paths
        .iter()
        .filter_map(|p| Some((pack::shared_path(p)?.into_string(), p)))
        .filter(|(_, p)| !in_song(p))
        .map(|(shared, p)| (shared, p.clone()))
        .collect();
    let maybe_works = !ddi_library::danoni::work_sources(&paths).is_empty();
    let level_candidates = ddi_library::ffr::find_levels(&paths, &candidates, &fallback);
    let maybe_levels = !level_candidates.is_empty();
    let skins = ddi_noteskin::import::find_skins(&paths);
    let sized: Vec<(&str, u64)> = paths
        .iter()
        .map(|p| (p.as_str(), sources.get(p).map_or(0, source_size)))
        .collect();
    let announcers = ddi_library::announcer::find_announcers(&sized, &fallback);
    // A folder read again with nothing left in it still drops its songs.
    if candidates.is_empty()
        && shared_files.is_empty()
        && !maybe_works
        && !maybe_levels
        && skins.is_empty()
        && announcers.announcers.is_empty()
        && !rescan
    {
        report_music_without_simfile(&mut report, &paths, &HashSet::new());
        if report.skipped.is_empty() {
            report.error = Some(ImportFailure::NothingFound);
        }
        return report;
    }
    let db = match Db::open().await {
        Ok(db) => db,
        Err(e) => {
            report.error = Some(ImportFailure::System(e));
            return report;
        }
    };
    if !skins.is_empty() {
        store_noteskins(&db, &skins, &sources, &paths, &mut report, &progress).await;
    }
    if !announcers.announcers.is_empty() {
        store_announcers(&db, &announcers, &sources, &mut report, &progress).await;
    }
    let keep_videos = crate::settings::Settings::load().import_videos;
    let images: Vec<&(String, String)> = shared_files
        .iter()
        .filter(|(s, _)| ddi_library::backgrounds::is_image(s))
        .collect();
    let movies: Vec<&(String, String)> = shared_files
        .iter()
        .filter(|(s, _)| ddi_library::video::is_movie(s))
        .collect();
    // A pack's genre mapping for its random movies (small text, always
    // copied; `ddi_library::backgrounds::random`).
    let mappings: Vec<&(String, String)> = shared_files
        .iter()
        .filter(|(s, _)| ddi_library::backgrounds::random::is_mapping(s))
        .collect();
    report.shared_skipped = shared_files.len() - images.len() - movies.len() - mappings.len();
    if !mappings.is_empty() {
        let (_, failed) = store_shared(&db, &mappings, &sources).await;
        report.skipped.extend(failed);
    }
    if !movies.is_empty() {
        let linked = origin.as_ref().filter(|o| o.link);
        let (kept, failed) =
            keep_shared_movies(&db, &movies, &sources, linked, keep_videos, &progress).await;
        report.shared_videos = kept;
        report.videos_left_out += movies.len().saturating_sub(kept + failed.len());
        report.skipped.extend(failed);
    }
    if !images.is_empty() {
        progress.emit(ImportStep::StoringSharedImages {
            count: images.len(),
        });
        let (stored, failed) = store_shared(&db, &images, &sources).await;
        report.shared_images = stored;
        report.skipped.extend(failed);
    }
    // What shared folders hold: this import's files and the images already
    // in the library.
    let mut shared: Vec<String> = shared_files.iter().map(|(s, _)| s.clone()).collect();
    shared.extend(crate::songs::shared_paths(&db).await.unwrap_or_default());
    // Each song's files come from its pack's paths, as desktop lists them.
    let mut packs = scan::PackIndexes::new(&paths);
    let scan_shared = pack::PathIndex::new(&shared);
    let n = candidates.len();
    // Two folders with the same pack and song folder names share an id.
    let mut seen = scan::Duplicates::default();
    for (i, c) in candidates.iter().enumerate() {
        let (id, name) = c.identity();
        let label = if c.dir.is_empty() {
            c.chart.to_string()
        } else {
            c.dir.to_string()
        };
        progress.emit(ImportStep::ImportingSong {
            index: i + 1,
            total: n,
            name: name.to_string(),
        });
        if let Some(why) = seen.check(c) {
            report.skipped.push((label, why));
            continue;
        }
        let inputs = Inputs {
            sources: &sources,
            paths: packs.of(c),
            shared: &scan_shared,
            folder: origin.as_ref().map(|o| o.folder.as_str()),
            link: origin.as_ref().is_some_and(|o| o.link),
            keep_videos,
        };
        match import_song(&db, c, &inputs).await {
            Ok(song) => {
                if song.still {
                    STILLS.with(|q| q.borrow_mut().push_back((id.clone(), label.clone())));
                }
                report.imported.push(song.title);
                report.videos += song.videos;
                report.videos_left_out += song.videos_left_out;
                report
                    .warnings
                    .extend(song.warnings.into_iter().map(|w| (label.clone(), w)));
                report
                    .notes
                    .extend(song.notes.into_iter().map(|n| (label.clone(), n)));
            }
            Err(e) => report.skipped.push((label, e)),
        }
    }
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
    let levels = ffr::import_levels(
        &db,
        &sources,
        &level_candidates,
        origin.as_ref().map(|o| o.folder.as_str()),
        &mut report,
        &progress,
    )
    .await;
    // Pack covers: the simfile packs' folders', the levels' own folders.
    let mut packs: Vec<(String, RelPathBuf)> = candidates
        .iter()
        .filter_map(|c| Some((c.pack.clone(), c.pack_dir()?.to_rel_path_buf())))
        .collect();
    packs.extend(levels.packs.iter().cloned());
    store_pack_art(&db, &packs, &sources, &paths, &mut report).await;
    let music_used: HashSet<String> = works.music.union(&levels.music).cloned().collect();
    report_music_without_simfile(&mut report, &paths, &music_used);
    if let Some(origin) = origin.as_ref().filter(|o| o.rescan) {
        match remove_gone(&db, &origin.folder, &paths, &unreadable).await {
            Ok(n) => report.removed = n,
            Err(e) => report
                .skipped
                .push((String::new(), Finding::RemovingGoneFailed { error: e })),
        }
    }
    // Once every song is written: shared records nothing refers to any more
    // (works imported again under other fields, unused folders).
    if (works.found || !report.imported.is_empty() || rescan)
        && let Err(e) = crate::songs::drop_unused_shared(&db).await
    {
        report
            .skipped
            .push((String::new(), Finding::TidyingFailed { error: e }));
    }
    if candidates.is_empty()
        && shared_files.is_empty()
        && !works.found
        && !levels.found
        && report.skipped.is_empty()
        && report.noteskins.is_empty()
        && report.announcers.is_empty()
    {
        report.error = Some(ImportFailure::NothingFound);
    }
    report
}

thread_local! {
    /// Songs whose opening still is still to take: `(id, label)`, queued
    /// by imports, taken after them (`take_stills`).
    static STILLS: RefCell<std::collections::VecDeque<(String, String)>> = RefCell::default();
    static STILLS_RUNNING: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
}

/// Takes the opening stills of the songs imports stored (`crate::stills`),
/// one after another, after the import has finished: its report and its
/// songs show at once, the stills fill in ("taking pictures… N of M"), and
/// the next import never waits for them. What could not be taken is a line
/// in the last report's details (the song keeps its background image); the
/// song list reloads when they are done.
fn take_stills() {
    if STILLS_RUNNING.with(|r| r.replace(true)) {
        return;
    }
    spawn_local(async {
        let mut taker = crate::stills::Taker::default();
        let mut done = 0;
        while let Some((id, label)) = STILLS.with(|q| q.borrow_mut().pop_front()) {
            done += 1;
            let left = STILLS.with(|q| q.borrow().len());
            notify(|s| {
                s.stills = Some(ImportStep::TakingStills {
                    done,
                    total: done + left,
                })
            });
            let note = match crate::stills::fill(&id, &mut taker).await {
                crate::stills::Filled::Failed { movie, reason } => {
                    Some(Finding::StillFailed { movie, reason })
                }
                crate::stills::Filled::Later(reason) => Some(Finding::StillLater { reason }),
                _ => None,
            };
            if let Some(note) = note {
                notify(|s| {
                    if let Some(r) = s.report.as_mut() {
                        r.notes.push((label.clone(), note.clone()));
                    }
                });
            }
        }
        STILLS_RUNNING.with(|r| r.set(false));
        // The song list reads the stills again.
        notify(|s| {
            s.stills = None;
            s.generation += 1;
        });
    });
}

/// The opening stills' progress while they are taken after an import.
pub(crate) fn stills_status() -> Option<ImportStep> {
    SHARED.with(|s| s.borrow().stills.clone())
}

/// A file's size in the import (a zip entry's uncompressed).
fn source_size(source: &Source) -> u64 {
    match source {
        Source::File(f) => f.size() as u64,
        Source::Zip { entry, .. } => entry.uncompressed_size,
    }
}

/// Stores the noteskins found among the import's files (dance, pump and
/// common ones; other games' are reported as skipped), then sums up what
/// of each draws.
async fn store_noteskins(
    db: &Db,
    skins: &[ddi_noteskin::import::Found],
    sources: &HashMap<String, Source>,
    paths: &[String],
    report: &mut ImportReport,
    progress: &Callback<ImportStep>,
) {
    use crate::noteskins::{
        MAX_FILES, MAX_SKINS, MAX_TEXT, MAX_TEXTS, StoredSkin, kept_file, store,
    };
    let mut imported: Vec<StoredSkin> = Vec::new();
    // Other games' skins (a whole StepMania install has dozens): one line.
    let other: Vec<&str> = skins
        .iter()
        .filter(|f| !ddi_noteskin::import::kept_game(&f.game))
        .map(|f| f.game.as_str())
        .collect();
    if !other.is_empty() {
        let mut games: Vec<String> = other.iter().map(|g| g.to_string()).collect();
        games.sort();
        games.dedup();
        report.skipped.push((
            String::new(),
            Finding::SkinsForOtherGames {
                count: other.len(),
                games,
            },
        ));
    }
    // Dance and pump skins, and the shared `common` ones.
    let kept: Vec<&ddi_noteskin::import::Found> = skins
        .iter()
        .filter(|f| ddi_noteskin::import::kept_game(&f.game))
        .collect();
    if kept.len() > MAX_SKINS {
        report.warnings.push((
            String::new(),
            Finding::TooManySkins {
                count: kept.len(),
                kept: MAX_SKINS,
            },
        ));
    }
    for found in kept.into_iter().take(MAX_SKINS) {
        let key = crate::noteskins::skin_key(&found.game, &found.name);
        if imported.iter().any(|s| s.key() == key) {
            report
                .skipped
                .push((found.dir.clone(), Finding::SameSkinName));
            continue;
        }
        progress.emit(ImportStep::StoringSkin {
            name: found.name.clone(),
        });
        let prefix = format!("{}/", found.dir);
        let files: Vec<String> = paths
            .iter()
            .filter_map(|p| p.strip_prefix(&prefix))
            .map(str::to_string)
            .collect();
        let kept = files.iter().filter(|f| kept_file(f)).count();
        if kept > MAX_FILES {
            report.warnings.push((
                found.dir.clone(),
                Finding::TooManySkinFiles {
                    count: kept,
                    kept: MAX_FILES,
                },
            ));
        }
        let mut texts = Vec::new();
        let mut blobs = Vec::new();
        let mut stored = Vec::new();
        let mut text_bytes = 0;
        for f in &files {
            let Some(source) = sources.get(&format!("{prefix}{f}")) else {
                continue;
            };
            let mut is_text = crate::noteskins::kept_text(f);
            if is_text && source_size(source) > MAX_TEXT {
                report.warnings.push((
                    found.dir.clone(),
                    Finding::SkinFileTooLarge { file: f.clone() },
                ));
            } else if is_text && text_bytes + source_size(source) > MAX_TEXTS {
                if text_bytes <= MAX_TEXTS {
                    report.warnings.push((
                        found.dir.clone(),
                        Finding::SkinTextsTooLarge {
                            megabytes: MAX_TEXTS >> 20,
                        },
                    ));
                }
                text_bytes = MAX_TEXTS + 1;
                is_text = false;
            } else if is_text {
                text_bytes += source_size(source);
                match read_bytes(source).await {
                    Ok(bytes) => {
                        texts.push((f.clone(), String::from_utf8_lossy(&bytes).into_owned()))
                    }
                    Err(e) => report.warnings.push((found.dir.clone(), file_failed(f, e))),
                }
            }
            // The texts always; the rest up to the limit.
            if kept_file(f) && (is_text || stored.len() < MAX_FILES) {
                match read_blob(source, mime_type(f)).await {
                    Ok(blob) => {
                        blobs.push((f.clone(), blob));
                        stored.push(f.clone());
                    }
                    Err(e) => report.warnings.push((found.dir.clone(), file_failed(f, e))),
                }
            }
        }
        let skin = StoredSkin {
            game: found.game.clone(),
            name: found.name.clone(),
            files,
            texts,
            origin: found.dir.clone(),
            stored,
            lua_broken: None,
            bundled: None,
            variant_of: None,
        };
        match store(&skin, blobs) {
            Ok(writes) => match db.write(writes).await {
                Ok(()) => {
                    // A new import gets its Lua's chances back.
                    crate::noteskins::forget_stops(&skin.key());
                    imported.push(skin);
                }
                Err(e) => report.skipped.push((found.dir.clone(), e.into())),
            },
            Err(e) => report.skipped.push((found.dir.clone(), e.into())),
        }
    }
    // What draws, with every skin in the library as fallbacks (older
    // records given their `.lua` texts).
    let mut all = crate::noteskins::all(db).await.unwrap_or_default();
    for s in &mut all {
        crate::noteskins::add_missing_texts(db, s).await;
    }
    all.extend(crate::noteskins::bundled_or_none().await);
    let folders: Vec<ddi_noteskin::Folder> = all
        .iter()
        .map(|s| ddi_noteskin::Folder {
            game: s.game.clone(),
            name: s.name.clone(),
            files: s.files.clone(),
            texts: s.texts.clone(),
        })
        .collect();
    for skin in imported {
        if skin.game != "common" {
            progress.emit(ImportStep::RunningSkinLua {
                name: skin.name.clone(),
            });
        }
        let line = crate::noteskins::summary(&folders, &skin.game, &skin.name).await;
        report.noteskins.push((skin.name, line));
    }
}

/// Stores the announcers found among the import's files: each one's
/// lines' sounds (`crate::announcers`), what it speaks in the report, what
/// it lost in the warnings, what makes no difference in the details.
async fn store_announcers(
    db: &Db,
    found: &ddi_library::announcer::Found,
    sources: &HashMap<String, Source>,
    report: &mut ImportReport,
    progress: &Callback<ImportStep>,
) {
    use ddi_library::announcer::MAX_ANNOUNCERS;
    if found.left_out > 0 {
        report.warnings.push((
            String::new(),
            Finding::TooManyAnnouncers {
                count: found.announcers.len() + found.left_out,
                kept: MAX_ANNOUNCERS,
            },
        ));
    }
    let mut keys = HashSet::new();
    for a in &found.announcers {
        let label = if a.dir.is_empty() {
            a.name.clone()
        } else {
            a.dir.to_string()
        };
        let mut kept = a.kept();
        if kept.lines.is_empty() {
            report.skipped.push((label, Finding::AnnouncerWithoutLines));
            continue;
        }
        if !keys.insert(kept.key()) {
            report.skipped.push((label, Finding::SameAnnouncerName));
            continue;
        }
        progress.emit(ImportStep::StoringAnnouncer {
            name: a.name.clone(),
        });
        let mut sounds = Vec::new();
        let mut bytes = 0;
        for files in kept.lines.values_mut() {
            let mut stored = Vec::new();
            for f in files.drain(..) {
                let at = a.dir.join(&f);
                let Some(source) = sources.get(at.as_str()) else {
                    continue;
                };
                match read_blob(source, mime_type(&f)).await {
                    Ok(blob) => {
                        bytes += source_size(source);
                        sounds.push((f.to_string(), blob));
                        stored.push(f);
                    }
                    Err(e) => report.warnings.push((label.clone(), file_failed(&f, e))),
                }
            }
            *files = stored;
        }
        kept.lines.retain(|_, files| !files.is_empty());
        kept.files = sounds.len();
        kept.bytes = bytes;
        let said = a.said();
        let writes = match crate::announcers::store(&kept, sounds) {
            Ok(writes) => writes,
            Err(e) => {
                report.skipped.push((label, e.into()));
                continue;
            }
        };
        if let Err(e) = db.write(writes).await {
            report.skipped.push((label, e.into()));
            continue;
        }
        report.announcers.push((a.name.clone(), said.summary));
        report
            .warnings
            .extend(said.warnings.into_iter().map(|w| (label.clone(), w)));
        report
            .notes
            .extend(said.details.into_iter().map(|d| (label.clone(), d)));
    }
}

/// Folders with music but no simfile, except the folders of music that
/// Dancing☆Onigiri works used.
fn report_music_without_simfile(
    report: &mut ImportReport,
    paths: &[String],
    used: &HashSet<String>,
) {
    let used_dirs: HashSet<&str> = used
        .iter()
        .map(|p| RelPath::new(p).parent().as_str())
        .collect();
    for (dir, music) in pack::folders_without_simfile(paths) {
        if used_dirs.contains(dir.as_str()) {
            continue;
        }
        report.skipped.push((
            dir.to_string(),
            Finding::MusicWithoutSimfile {
                music: music.file_name().to_string(),
            },
        ));
    }
}

/// What an import reads songs from.
struct Inputs<'a> {
    sources: &'a HashMap<String, Source>,
    paths: &'a pack::PathIndex<'a>,
    /// Files in the library's shared background folders.
    shared: &'a pack::PathIndex<'a>,
    /// The kept folder the files come from, recorded on each song (so it
    /// can be re-read).
    folder: Option<&'a str>,
    /// Link files in that folder rather than copy them.
    link: bool,
    /// Copy background-change movies too when copying.
    keep_videos: bool,
}

/// What importing one song gave, for the report.
struct Imported {
    title: String,
    /// The song may have an opening movie, whose still is taken once the
    /// songs are stored (`crate::stills`).
    still: bool,
    warnings: Vec<Finding>,
    notes: Vec<Finding>,
    /// Background-change movies that will play.
    videos: usize,
    /// Movies left out (copied without "also store background videos").
    videos_left_out: usize,
}

/// Imports one song: the library's import core (`ddi_library::scan`, as
/// desktop lists songs) on the files read here from `Blob`s, then its
/// files copied into storage or linked in their kept folder.
async fn import_song(db: &Db, c: &SongCandidate, inputs: &Inputs<'_>) -> Result<Imported, Finding> {
    let Inputs {
        sources,
        paths,
        shared,
        folder,
        link,
        keep_videos,
    } = *inputs;
    let linked = link && folder.is_some();
    let chart_bytes = read_bytes(source(sources, &c.chart)?).await?;
    let mut reading = scan::read_song(c, &chart_bytes, paths, &legacy_text)?;
    drop(chart_bytes);
    if !reading.images_to_size().is_empty() {
        // StepMania's last resort: tell banners from backgrounds by size.
        let mut sizes = Vec::new();
        for path in reading.images_to_size().to_vec() {
            let Ok(src) = source(sources, &path) else {
                continue;
            };
            if let Ok(head) = read_head(src, IMAGE_HEADER_LEN as u64).await
                && let Some(size) = image_size(&head)
            {
                sizes.push((path, size));
            }
        }
        reading.sized(&sizes);
    }
    // Movies: what each is; kept when linked, or copied with "also store
    // background videos"; never when the game cannot play them.
    let mut movies = Vec::new();
    for (relative, path) in reading.movies().to_vec() {
        let src = source(sources, &path)?;
        let format = sniff_movie(src).await;
        let linkable = src.link(&path).filter(|_| linked).is_some();
        movies.push(Movie {
            kept: linkable || keep_videos,
            relative,
            path,
            format,
        });
    }
    let built = reading.finish(paths, shared, movies, Naming::Stored)?;
    let scan::Built {
        scanned,
        text,
        music,
        banner,
        background,
        jacket,
        cd_title,
        lyrics,
        preview,
        chart_music,
        bg_images,
        script_files,
        videos: video_paths,
        videos_playing,
        videos_left_out,
    } = built;
    let entry = scanned.entry;
    let id = entry.id.clone();
    let title = entry.title.clone();
    // Charts are stored as decoded UTF-8 so loading never guesses again.
    let mut files: Vec<(String, Blob)> = vec![(
        entry.chart.to_string(),
        bytes_blob(text.as_bytes(), "text/plain;charset=utf-8")?,
    )];
    // Linked: music and backgrounds stay in the folder (the banner is
    // copied, since the song list shows it before any folder is read).
    let mut links = std::collections::BTreeMap::new();
    let music_source = source(sources, &music)?;
    match music_source.link(&music).filter(|_| linked) {
        Some(l) => {
            links.insert(entry.music.to_string(), l);
        }
        None => files.push((
            entry.music.to_string(),
            read_blob(music_source, mime_type(&music)).await?,
        )),
    }
    let bg_files = bg_images
        .into_iter()
        .map(|(relative, path)| (RelPathBuf::from(format!("bg/{relative}")), path));
    let video_files = video_paths
        .into_iter()
        .map(|(relative, path)| (RelPathBuf::from(format!("bg/{relative}")), path));
    // The scripted backgrounds' files, read by their scripts.
    let script_files = script_files
        .into_iter()
        .map(|(relative, path)| (RelPathBuf::from(format!("bga/{relative}")), path));
    // The song list's images and the lyrics are copied (small, and
    // wanted before any folder is read); a separate preview and charts'
    // own music are linked or copied as the music is.
    for (name, path) in banner
        .into_iter()
        .chain(jacket)
        .chain(cd_title)
        .chain(lyrics)
    {
        // A broken image is not worth losing the song over.
        if let Ok(blob) = read_blob(source(sources, &path)?, mime_type(&path)).await {
            files.push((name.to_string(), blob));
        }
    }
    for (name, path) in background
        .into_iter()
        .chain(preview)
        .chain(chart_music)
        .chain(bg_files)
        .chain(video_files)
        .chain(script_files)
    {
        let src = source(sources, &path)?;
        link_or_copy(src, &path, name.to_string(), linked, &mut links, &mut files).await;
    }
    let record = ImportedSong {
        entry,
        pack: scanned.pack,
        dir: scanned.dir.into_string(),
        imported: js_sys::Date::now(),
        bytes: files.iter().map(|(_, b)| b.size()).sum(),
        work: None,
        folder: folder.map(str::to_string),
        links,
        still: None,
    };
    let json = serde_json::to_string(&record).map_err(|e| e.to_string())?;
    write_song(db, &id, files, None, json).await?;
    Ok(Imported {
        title,
        still: crate::stills::may_have(&record.entry),
        warnings: scanned.warnings,
        notes: scanned.notes,
        videos: videos_playing,
        videos_left_out,
    })
}

/// Keeps the movies of shared background folders: linked imports record
/// where each is in the kept folder ([`crate::songs::SharedLink`]), copying
/// ones store it only with "also store background videos". Returns how
/// many were kept and the ones that failed.
async fn keep_shared_movies(
    db: &Db,
    movies: &[&(String, String)],
    sources: &HashMap<String, Source>,
    linked: Option<&Origin>,
    keep_videos: bool,
    progress: &Callback<ImportStep>,
) -> (usize, Vec<(String, Finding)>) {
    let mut links = Vec::new();
    let mut copies = Vec::new();
    for movie in movies {
        let (shared, path) = *movie;
        let link = linked.and_then(|o| Some((o, source(sources, path).ok()?.link(path)?)));
        match link {
            Some((origin, link)) => {
                let record = crate::songs::SharedLink {
                    folder: origin.folder.clone(),
                    link,
                };
                if let Ok(json) = serde_json::to_string(&record) {
                    links.push(Write::Put {
                        store: idb::FILES,
                        key: crate::songs::shared_key(shared),
                        value: json.into(),
                    });
                }
            }
            None if keep_videos => copies.push(*movie),
            None => {}
        }
    }
    let mut kept = 0;
    let mut failed = Vec::new();
    if !links.is_empty() {
        let n = links.len();
        match db.write(links).await {
            Ok(()) => kept += n,
            Err(e) => failed.push((String::new(), Finding::SharedVideosFailed { error: e })),
        }
    }
    if !copies.is_empty() {
        progress.emit(ImportStep::StoringSharedVideos {
            count: copies.len(),
        });
        let (stored, more) = store_shared(db, &copies, sources).await;
        kept += stored;
        failed.extend(more);
    }
    (kept, failed)
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
) -> (usize, Vec<(String, Finding)>) {
    let mut stored = 0;
    let mut failed: Vec<(String, Finding)> = Vec::new();
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
                Err(e) => failed.extend(paths.into_iter().map(|p| (p, e.clone().into()))),
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
    packs: &[(String, RelPathBuf)],
    sources: &HashMap<String, Source>,
    paths: &[String],
    report: &mut ImportReport,
) {
    let mut done: HashSet<String> = HashSet::new();
    let mut writes = Vec::new();
    for (pack_name, dir) in packs {
        if !done.insert(pack_name.to_lowercase()) {
            continue;
        }
        let Some(path) = pack::pack_banner(dir, paths) else {
            continue;
        };
        match async { read_blob(source(sources, &path)?, mime_type(&path)).await }.await {
            Ok(blob) => writes.push(Write::Put {
                store: idb::FILES,
                key: crate::songs::pack_art_key(pack_name),
                value: blob.into(),
            }),
            Err(e) => report
                .warnings
                .push((path.to_string(), Finding::PackBannerFailed(Box::new(e)))),
        }
    }
    if !writes.is_empty()
        && let Err(e) = db.write(writes).await
    {
        report
            .warnings
            .push((String::new(), Finding::PackBannersFailed { error: e }));
    }
}

fn source(sources: &HashMap<String, Source>, path: impl AsRef<str>) -> Result<&Source, Finding> {
    let path = path.as_ref();
    sources.get(path).ok_or_else(|| Finding::FileMissing {
        file: path.to_string(),
    })
}

/// A file of the import that failed to read, and why.
fn file_failed(file: impl AsRef<str>, e: Finding) -> Finding {
    match e {
        Finding::System(error) => Finding::FileFailed {
            file: file.as_ref().to_string(),
            error,
        },
        other => other,
    }
}

fn is_zip(path: &str) -> bool {
    zip::archive_stem(path).is_some()
}

/// Lowercase extension of the last path component.
pub(crate) fn extension(path: &str) -> Option<String> {
    RelPath::new(path).lower_extension()
}

async fn read_range(blob: &Blob, start: u64, end: u64) -> Result<Vec<u8>, String> {
    let slice = blob
        .slice_with_f64_and_f64(start as f64, end as f64)
        .map_err(|e| js_err("Blob.slice", e))?;
    blob_bytes(&slice).await
}

/// The entry list of a zip, from its tail and central directory.
pub(crate) async fn zip_entries(file: &File) -> Result<Vec<ZipEntry>, String> {
    zip_directory(file)
        .await
        .map_err(|f| ddi_ui::screens::import_report::finding_text(&f).into())
}

/// [`zip_entries`], why not as a value.
async fn zip_directory(file: &File) -> Result<Vec<ZipEntry>, Finding> {
    let len = file.size() as u64;
    let tail = read_range(file, len - zip::tail_len(len), len).await?;
    let cd = match zip::locate_central_directory(&tail, len)? {
        Locate::Found(cd) => cd,
        Locate::NeedZip64Record { offset, len } => {
            let record = read_range(file, offset, offset + len).await?;
            zip::parse_zip64_end_record(&record)?
        }
    };
    let bytes = read_range(file, cd.offset, cd.offset + cd.size).await?;
    let mut entries = zip::parse_central_directory(&bytes, &cd)?;
    pack::apply_legacy_names(&mut entries, legacy_text);
    Ok(entries)
}

/// Text in a legacy code page through the browser's `TextDecoder`
/// (`fatal`), `None` on any invalid sequence (`ddi_library::legacy`).
pub(crate) fn legacy_text(code_page: ddi_library::legacy::Legacy, bytes: &[u8]) -> Option<String> {
    let opts = web_sys::TextDecoderOptions::new();
    opts.set_fatal(true);
    web_sys::TextDecoder::new_with_label_and_options(code_page.label(), &opts)
        .and_then(|d| d.decode_with_buffer_source(&js_sys::Uint8Array::from(bytes)))
        .ok()
}

/// Start of an entry's data, after its local header.
async fn zip_data_start(archive: &File, entry: &ZipEntry) -> Result<u64, Finding> {
    let at = entry.local_header_offset;
    let header = read_range(archive, at, at + zip::LOCAL_HEADER_LEN).await?;
    Ok(zip::local_data_offset(&header, entry)?)
}

/// The first `len` bytes of a file (all of it if shorter). Of a compressed
/// zip entry only the start of the stream is decompressed (a movie is tens
/// of megabytes), the whole entry when that was not enough.
async fn read_head(source: &Source, len: u64) -> Result<Vec<u8>, Finding> {
    match source {
        Source::File(f) => Ok(read_range(f, 0, len.min(f.size() as u64)).await?),
        Source::Zip { archive, entry, .. }
            if entry.method == Method::Stored && !entry.encrypted =>
        {
            let start = zip_data_start(archive, entry).await?;
            Ok(read_range(archive, start, start + len.min(entry.compressed_size)).await?)
        }
        Source::Zip { archive, entry, .. }
            if !matches!(entry.method, Method::Stored | Method::Other(_)) && !entry.encrypted =>
        {
            let start = zip_data_start(archive, entry).await?;
            let want = entry.compressed_size.min((len * 16).max(1 << 20));
            let compressed = read_range(archive, start, start + want).await?;
            let head = zip::extract_prefix(entry, &compressed, len as usize);
            if !head.is_empty() || want == entry.compressed_size {
                return Ok(head);
            }
            let mut bytes = read_bytes(source).await?;
            bytes.truncate(len as usize);
            Ok(bytes)
        }
        Source::Zip { .. } => {
            let mut bytes = read_bytes(source).await?;
            bytes.truncate(len as usize);
            Ok(bytes)
        }
    }
}

/// `len` bytes of a file from `start`: for a file or a stored zip entry
/// (a compressed one would have to be decompressed up to there).
async fn read_at(source: &Source, start: u64, len: u64) -> Result<Vec<u8>, Finding> {
    match source {
        Source::File(f) => {
            let size = f.size() as u64;
            Ok(read_range(f, start.min(size), start.saturating_add(len).min(size)).await?)
        }
        Source::Zip { archive, entry, .. }
            if entry.method == Method::Stored && !entry.encrypted =>
        {
            let data = zip_data_start(archive, entry).await?;
            let end = start.saturating_add(len).min(entry.compressed_size);
            Ok(read_range(archive, data + start.min(end), data + end).await?)
        }
        // Never asked of one (only the index of a movie read in place).
        Source::Zip { .. } => Err(Finding::Other("a compressed zip entry".into())),
    }
}

/// What a movie of the import is ([`ddi_library::video::sniff`], with the
/// MP4 index where the head points to it). Never fails: a file that
/// cannot be read is of an unrecognised format, which the decoder still
/// tries.
async fn sniff_movie(source: &Source) -> ddi_platform::video::VideoFormat {
    use ddi_library::video::{MP4_INDEX_LEN, SNIFF_LEN, mp4_index_at, mp4_index_codec, sniff};
    let head = read_head(source, SNIFF_LEN as u64)
        .await
        .unwrap_or_default();
    let mut format = sniff(&head);
    if let Some(at) = mp4_index_at(&head)
        && let Ok(index) = read_at(source, at, MP4_INDEX_LEN as u64).await
    {
        format.codec = mp4_index_codec(&index);
    }
    format
}

async fn read_bytes(source: &Source) -> Result<Vec<u8>, Finding> {
    match source {
        Source::File(f) => Ok(blob_bytes(f).await?),
        Source::Zip { archive, entry, .. } => {
            let start = zip_data_start(archive, entry).await?;
            let data = read_range(archive, start, start + entry.compressed_size).await?;
            Ok(zip::extract(entry, &data)?)
        }
    }
}

/// The file as a blob for storage. Stored zip entries are sliced out of the
/// archive without being read (and so without a CRC check, which the
/// decoder or image loader effectively does instead).
/// A song's file `name`, from `path`: linked to its place in the kept
/// folder when `link` and it has one, else copied into `files` (left out
/// when it cannot be read).
async fn link_or_copy(
    src: &Source,
    path: impl AsRef<str>,
    name: String,
    link: bool,
    links: &mut std::collections::BTreeMap<String, crate::songs::Link>,
    files: &mut Vec<(String, Blob)>,
) {
    let path = path.as_ref();
    match src.link(path).filter(|_| link) {
        Some(l) => {
            links.insert(name, l);
        }
        None => {
            if let Ok(blob) = read_blob(src, mime_type(path)).await {
                files.push((name, blob));
            }
        }
    }
}

/// Stores imported song `id`: its `files`, in place of those it had,
/// `extra`, then its record (`record`, its JSON), in one transaction.
async fn write_song(
    db: &Db,
    id: &str,
    files: Vec<(String, Blob)>,
    extra: Option<Write>,
    record: String,
) -> Result<(), Finding> {
    let mut writes = vec![Write::DeletePrefix {
        store: idb::FILES,
        prefix: file_key(id, ""),
    }];
    writes.extend(files.into_iter().map(|(name, blob)| Write::Put {
        store: idb::FILES,
        key: file_key(id, &name),
        value: blob.into(),
    }));
    writes.extend(extra);
    writes.push(Write::Put {
        store: idb::SONGS,
        key: id.to_string(),
        value: record.into(),
    });
    Ok(db.write(writes).await?)
}

async fn read_blob(source: &Source, mime: &str) -> Result<Blob, Finding> {
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
                return Err(Finding::Truncated {
                    file: entry.name.clone(),
                });
            }
            Ok(archive
                .slice_with_f64_and_f64_and_content_type(start as f64, end as f64, mime)
                .map_err(|e| js_err("Blob.slice", e))?)
        }
        Source::Zip { .. } => Ok(bytes_blob(&read_bytes(source).await?, mime)?),
    }
}

/// Content type for a stored file, so blob URLs (`<audio>` previews,
/// banners) are served with one. Zip entries have none of their own.
fn mime_type(path: impl AsRef<str>) -> &'static str {
    let path = path.as_ref();
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
        Some("lrc") => "text/plain",
        _ => "",
    }
}
