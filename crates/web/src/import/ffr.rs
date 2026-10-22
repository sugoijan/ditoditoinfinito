//! Importing FFR levels (`docs/plans/phase-10.md`, step 2): each level
//! among the import's files (`ddi_library::ffr`, as desktop's library
//! finds them: the same ids and packs) read here and stored as a song:
//! its `beatBox` as JSON ([`ffr::STORED_CHART`]) and its music, taken out
//! of the SWF (or the audio file beside a text level), copied.

use std::collections::{HashMap, HashSet};

use ddi_library::ffr::{self, LevelCandidate};
use ddi_library::path::RelPathBuf;

use super::{ImportReport, Source, mime_type, read_blob, source, write_song};
use crate::songs::ImportedSong;
use crate::web::files::bytes_blob;
use crate::web::idb::Db;

/// What the levels of an import left behind for the rest of the report.
#[derive(Default)]
pub(super) struct LevelsOutcome {
    /// Music files the text levels used (their folders are not "music
    /// without a simfile").
    pub(super) music: HashSet<String>,
    /// Whether any file was a level.
    pub(super) found: bool,
    /// The packs of the levels stored, with their folders (their covers).
    pub(super) packs: Vec<(String, RelPathBuf)>,
}

/// Imports every level of `candidates` (`ffr::find_levels`).
pub(super) async fn import_levels(
    db: &Db,
    sources: &HashMap<String, Source>,
    candidates: &[LevelCandidate],
    folder: Option<&str>,
    report: &mut ImportReport,
    progress: &crate::component::Callback<String>,
) -> LevelsOutcome {
    let mut out = LevelsOutcome::default();
    let n = candidates.len();
    for (i, c) in candidates.iter().enumerate() {
        let label = c.path.to_string();
        progress.emit(format!("reading level {}/{n}: {}", i + 1, c.title));
        match import_level(db, sources, c, folder).await {
            Ok(None) => {}
            Ok(Some(title)) => {
                out.found = true;
                report.imported.push(title);
                if let Some(m) = &c.music {
                    out.music.insert(m.to_string());
                }
                out.packs.push((c.pack.clone(), c.dir.clone()));
            }
            Err(e) => {
                out.found = true;
                report.skipped.push((label, e));
            }
        }
    }
    out
}

/// One level stored: its title, or `None` when the file is no level.
async fn import_level(
    db: &Db,
    sources: &HashMap<String, Source>,
    c: &LevelCandidate,
    folder: Option<&str>,
) -> Result<Option<String>, String> {
    let src = source(sources, &c.path)?;
    if super::source_size(src) > ffr::MAX_LEVEL {
        return Ok(None);
    }
    let bytes = super::read_bytes(src).await?;
    let Some(level) = ffr::read_level(c, &bytes)? else {
        return Ok(None);
    };
    drop(bytes);
    // The music: a SWF's MP3, else the text level's file, copied.
    let (music_name, music) = match (&level.mp3, &c.music) {
        (Some(mp3), _) => ("music.mp3".to_string(), bytes_blob(mp3, "audio/mpeg")?),
        (None, Some(path)) => {
            let ext = path
                .extension()
                .map_or("ogg".to_string(), str::to_ascii_lowercase);
            let name = format!("music.{ext}");
            let blob = read_blob(source(sources, path)?, mime_type(&name)).await?;
            (name, blob)
        }
        (None, None) => return Err("no audio file beside the beatBox".into()),
    };
    let entry = ffr::entry(c, &level.song, ffr::STORED_CHART, &music_name);
    let title = entry.title.clone();
    let id = entry.id.clone();
    let files = vec![
        (
            ffr::STORED_CHART.to_string(),
            bytes_blob(level.beatbox.as_bytes(), "application/json")?,
        ),
        (music_name, music),
    ];
    let record = ImportedSong {
        entry,
        pack: c.pack.clone(),
        // The level itself: a folder read again without it drops it.
        dir: c.path.to_string(),
        imported: js_sys::Date::now(),
        bytes: files.iter().map(|(_, b)| b.size()).sum(),
        work: None,
        folder: folder.map(str::to_string),
        links: Default::default(),
        still: None,
    };
    let json = serde_json::to_string(&record).map_err(|e| e.to_string())?;
    write_song(db, &id, files, None, json).await?;
    Ok(Some(title))
}
