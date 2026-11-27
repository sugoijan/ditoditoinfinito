//! A fetched item imported (`docs/plans/source-plugins.md`, "Identity"):
//! its songs written into the app's own import store, the data folder's
//! `sources/library/<source>/<pack>/<song>/`, which the library reads as
//! one more folder (`crate::songs::Songs::scanned`), so that afterwards
//! each is an ordinary song of the library (its id `pack::song_id`, its
//! bests and plays by its charts). Each song is written into a staging
//! folder first and renamed into place, replacing an import of the same
//! song. What was imported from where is kept in `sources/catalog.json`.
//! The fetched copy is let go by its caller.

use std::path::{Path, PathBuf};

use ddi_library::path::RelPath;
use ddi_source_host::store::{FetchedSong, ItemFiles, Stored};
use serde::{Deserialize, Serialize};

/// The import store: the data folder's `sources/library/`.
pub fn root() -> Option<PathBuf> {
    crate::paths::data_dir().map(|d| d.join("sources").join("library"))
}

/// The folders the library reads of it: one a source, where there is
/// one.
pub fn roots() -> Vec<PathBuf> {
    let Some(root) = root() else {
        return Vec::new();
    };
    let Ok(read) = std::fs::read_dir(&root) else {
        return Vec::new();
    };
    let mut out: Vec<PathBuf> = read
        .flatten()
        .filter(|e| e.file_type().is_ok_and(|t| t.is_dir()))
        .filter(|e| !e.file_name().to_string_lossy().starts_with('.'))
        .map(|e| e.path())
        .collect();
    out.sort();
    out
}

/// What the catalog keeps of an import.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Imported {
    pub source: String,
    pub item: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub revision: Option<String>,
    /// Its songs' folders, relative to the import store.
    pub songs: Vec<String>,
    /// When, milliseconds since the epoch.
    pub at: f64,
}

fn catalog_path() -> Option<PathBuf> {
    crate::paths::data_dir().map(|d| d.join("sources").join("catalog.json"))
}

/// What was imported, as the catalog keeps it.
pub fn catalog() -> Vec<Imported> {
    catalog_path()
        .and_then(|p| std::fs::read(p).ok())
        .and_then(|b| serde_json::from_slice(&b).ok())
        .unwrap_or_default()
}

/// A folder or file name made from a song's or pack's (data): no
/// separators or control characters, never empty or a dot's.
fn name_of(text: &str, fallback: &str) -> String {
    let cleaned: String = text
        .chars()
        .map(|c| {
            if c.is_control() || matches!(c, '/' | '\\' | ':' | '*' | '?' | '"' | '<' | '>' | '|')
            {
                '_'
            } else {
                c
            }
        })
        .collect();
    let cleaned = cleaned.trim().trim_matches('.').trim().to_string();
    let cleaned: String = cleaned.chars().take(96).collect();
    if cleaned.is_empty() {
        fallback.to_string()
    } else {
        cleaned
    }
}

/// `songs` of an item (its `files`) written into the import store as
/// source `source`'s: how many were. The item's own `item` and
/// `revision` go into the catalog. Refused, nothing written, where the
/// level the store keeps for any of their items does not allow keeping
/// them ([`may_keep`]): checked here, whatever offered the import.
pub fn import(
    source: &str,
    item: &str,
    revision: Option<&str>,
    songs: &[FetchedSong],
    files: &ItemFiles,
) -> Result<usize, String> {
    if let Some(song) = songs.iter().find(|s| !may_keep(s)) {
        return Err(format!(
            "{}: its source does not allow importing it",
            song.entry.title
        ));
    }
    let root = root().ok_or("no data folder")?;
    import_into(&root, source, item, revision, songs, files)
}

/// Whether fetched song `song` may be imported: its item's level, as
/// the store keeps it, is `keep` (an item the store no longer has is
/// not).
pub fn may_keep(song: &FetchedSong) -> bool {
    super::fetched::item_reach(song.key).is_some_and(super::Reach::keeps)
}

/// [`import`] into `root`.
pub fn import_into(
    root: &Path,
    source: &str,
    item: &str,
    revision: Option<&str>,
    songs: &[FetchedSong],
    files: &ItemFiles,
) -> Result<usize, String> {
    let source_dir = root.join(name_of(source, "source"));
    static STAGINGS: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    let n = STAGINGS.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let staging = root.join(format!(".staging-{}-{n}", std::process::id()));
    let result = (|| {
        let mut written = Vec::new();
        for song in songs {
            let pack = name_of(&song.pack, ddi_library::pack::LOOSE_PACK);
            let folder = name_of(
                song.dir.file_name(),
                &name_of(&song.entry.title, "Song"),
            );
            let rel = format!("{pack}/{folder}");
            let staged = staging.join(&pack).join(&folder);
            std::fs::create_dir_all(&staged).map_err(|e| e.to_string())?;
            let prefix = if song.dir.is_empty() {
                String::new()
            } else {
                format!("{}/", song.dir)
            };
            for (path, stored) in files.0.iter() {
                let Some(inner) = path.as_str().strip_prefix(&prefix) else {
                    continue;
                };
                let to = RelPath::new(inner)
                    .to_path(&staged)
                    .ok_or_else(|| format!("{path}: leaves its song"))?;
                if let Some(parent) = to.parent() {
                    std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
                }
                match stored {
                    Stored::Memory(bytes) => std::fs::write(&to, bytes),
                    Stored::Disk(file) => std::fs::copy(file, &to).map(|_| ()),
                }
                .map_err(|e| format!("{path}: {e}"))?;
            }
            // Into place, an import of the same song replaced.
            let dest = source_dir.join(&pack).join(&folder);
            std::fs::create_dir_all(dest.parent().ok_or("no parent")?)
                .map_err(|e| e.to_string())?;
            if dest.exists() {
                std::fs::remove_dir_all(&dest).map_err(|e| e.to_string())?;
            }
            std::fs::rename(&staged, &dest).map_err(|e| e.to_string())?;
            written.push(format!("{}/{rel}", name_of(source, "source")));
        }
        Ok::<_, String>(written)
    })();
    let _ = std::fs::remove_dir_all(&staging);
    let written = result?;
    // The catalog: this item's record replaced.
    if let Some(path) = catalog_path().filter(|_| root == self::root().unwrap_or_default()) {
        let mut list = catalog();
        list.retain(|r| !(r.source == source && r.item == item));
        list.push(Imported {
            source: source.to_string(),
            item: item.to_string(),
            revision: revision.map(str::to_string),
            songs: written.clone(),
            at: crate::clock::epoch_ms(),
        });
        if let Ok(json) = serde_json::to_vec_pretty(&list) {
            let tmp = path.with_extension("json.part");
            if std::fs::write(&tmp, json).is_ok() {
                let _ = std::fs::rename(&tmp, &path);
            }
        }
    }
    Ok(written.len())
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use ddi_library::path::RelPathBuf;
    use ddi_source_host::store::{Assembled, ItemKey, Origin, Store, StoreLimits};

    use super::*;
    use crate::sources::Reach;

    #[test]
    fn an_import_below_keep_is_refused_by_the_import_itself() {
        let dir = std::env::temp_dir().join(format!("ddi-import-reach-{}", std::process::id()));
        let store = Store::open(dir.join("sources"), StoreLimits::default()).shared();
        let song = |key: ItemKey| FetchedSong {
            id: key.song_id(0),
            key,
            entry: serde_json::from_str(
                r#"{"id":"x","title":"T","artist":"A","chart":"song.ssc","music":"song.wav"}"#,
            )
            .unwrap(),
            pack: "Pack".into(),
            dir: RelPathBuf::from("song"),
            song_key: None,
            warnings: Vec::new(),
        };
        let mut songs = Vec::new();
        for (n, reach) in [Reach::Play, Reach::Rooms, Reach::Keep]
            .into_iter()
            .enumerate()
        {
            let key = ItemKey::of("p", &n.to_string(), None);
            store.lock().unwrap().insert(
                key,
                Origin {
                    plugin: "p".into(),
                    item: n.to_string(),
                    revision: None,
                    reach,
                },
                Assembled {
                    files: BTreeMap::new(),
                    memory: 0,
                    disk: 0,
                    owned: vec![],
                },
                vec![song(key)],
            );
            songs.push(song(key));
        }
        crate::sources::fetched::set_store(Some(store));
        let files = ItemFiles(std::sync::Arc::new(BTreeMap::new()));
        // Played only, or added to rooms only: refused before anything is
        // written (no data folder is even asked for).
        for s in &songs[..2] {
            assert!(!may_keep(s));
            let refused = import(
                "p",
                &s.key.0.to_string(),
                None,
                std::slice::from_ref(s),
                &files,
            );
            assert!(refused.is_err_and(|e| e.contains("does not allow importing")));
        }
        // An item's songs with one that may not be kept: none written.
        assert!(import("p", "0", None, &songs, &files).is_err());
        assert!(may_keep(&songs[2]));
        // A song the store does not have: not kept.
        assert!(!may_keep(&song(ItemKey::of("p", "gone", None))));
        crate::sources::fetched::set_store(None);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
