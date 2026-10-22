//! The play history on desktop (`docs/plans/play-history.md`, step 6):
//! two files a play in `plays/` next to `storage.json`. `<id>.json` holds
//! the play as played, written once and never again (the export's line
//! without the summary: the record as JSON, the log and the timing in
//! base64); `<id>.summary.json` holds its summary, rewritten when the play
//! is worked out again. The start reads the summaries alone. Files are
//! written whole or not at all (a temporary file, synced, then renamed);
//! one that does not read is left where it is and listed, as the web lists
//! its unreadable plays; temporary files left by a crash are removed.

use std::path::{Path, PathBuf};

use ddi_platform::history::{PlayEntry, PlayStore, Reply};

/// `plays/` in the configuration folder (beside `storage.json`).
pub fn default_dir() -> Option<PathBuf> {
    Some(crate::store::default_path()?.parent()?.join("plays"))
}

const SUMMARY: &str = ".summary.json";
const RAW: &str = ".json";
const TMP: &str = ".tmp";

/// Plays as files in a folder. A clone is the same folder (the history's
/// thread reads and writes through one; only [`FilePlayStore::new`]
/// removes a crash's temporary files).
#[derive(Clone)]
pub struct FilePlayStore {
    dir: PathBuf,
}

/// Held while a summary is rewritten or a play removed: the history's
/// thread rewriting a summary checks its play is there and writes in one
/// step, so a play removed meanwhile on the event loop's thread never
/// gets its summary back.
static CHANGING: std::sync::Mutex<()> = std::sync::Mutex::new(());

fn changing() -> std::sync::MutexGuard<'static, ()> {
    CHANGING
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
}

/// Whether `id` is a play id this store writes: nothing that could leave
/// the folder or be read as another file's name.
fn is_id(id: &str) -> bool {
    !id.is_empty()
        && id
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
}

impl FilePlayStore {
    /// The store in `dir`; temporary files a crash left there are removed.
    pub fn new(dir: PathBuf) -> FilePlayStore {
        if let Ok(entries) = std::fs::read_dir(&dir) {
            for e in entries.filter_map(Result::ok) {
                if e.file_name().to_string_lossy().ends_with(TMP) {
                    let _ = std::fs::remove_file(e.path());
                }
            }
        }
        FilePlayStore { dir }
    }

    pub fn dir(&self) -> &Path {
        &self.dir
    }

    fn raw_path(&self, id: &str) -> Option<PathBuf> {
        is_id(id).then(|| self.dir.join(format!("{id}{RAW}")))
    }

    fn summary_path(&self, id: &str) -> Option<PathBuf> {
        is_id(id).then(|| self.dir.join(format!("{id}{SUMMARY}")))
    }

    fn summary(&self, id: &str) -> Option<String> {
        std::fs::read_to_string(self.summary_path(id)?).ok()
    }

    /// The play as played, with its summary.
    fn read(&self, id: &str) -> Option<PlayEntry> {
        let text = std::fs::read_to_string(self.raw_path(id)?).ok()?;
        let mut e = PlayEntry::from_json(&text).ok().filter(|e| e.id == id)?;
        e.summary = self.summary(id);
        Some(e)
    }

    /// Writes `path` whole (`crate::paths::write_whole`).
    fn write_file(&self, path: &Path, text: &str) -> Result<(), String> {
        crate::paths::write_whole(path, text.as_bytes()).map_err(|e| e.to_string())
    }

    /// The ids of the plays in the folder (a play file or a summary
    /// file), in order.
    fn ids(&self) -> Result<Vec<String>, String> {
        let dir = match std::fs::read_dir(&self.dir) {
            Ok(d) => d,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
            Err(e) => return Err(e.to_string()),
        };
        let mut ids: Vec<String> = dir
            .filter_map(Result::ok)
            .filter_map(|e| {
                let name = e.file_name().into_string().ok()?;
                let id = name
                    .strip_suffix(SUMMARY)
                    .or_else(|| name.strip_suffix(RAW))?;
                is_id(id).then(|| id.to_string())
            })
            .collect();
        ids.sort();
        ids.dedup();
        Ok(ids)
    }
}

impl PlayStore for FilePlayStore {
    fn append(&self, entry: PlayEntry, done: Reply<()>) {
        let r = (|| {
            let (Some(raw), Some(sum)) = (self.raw_path(&entry.id), self.summary_path(&entry.id))
            else {
                return Err(format!("not a play id: {:?}", entry.id));
            };
            let summary = entry.summary.clone();
            let played = PlayEntry {
                summary: None,
                ..entry
            };
            self.write_file(&raw, &played.to_json())?;
            match summary {
                Some(s) => self.write_file(&sum, &s),
                None => Ok(()),
            }
        })();
        done(r)
    }

    fn summaries(&self, done: Reply<Vec<(String, Option<String>)>>) {
        done(self.ids().map(|ids| {
            ids.into_iter()
                .map(|id| {
                    let s = self.summary(&id);
                    (id, s)
                })
                .collect()
        }))
    }

    fn records(&self, ids: Vec<String>, done: Reply<Vec<PlayEntry>>) {
        done(Ok(ids.iter().filter_map(|id| self.read(id)).collect()))
    }

    fn replace_summaries(&self, summaries: Vec<(String, String)>, done: Reply<()>) {
        let guard = changing();
        let r = summaries.into_iter().try_for_each(|(id, summary)| {
            match (self.raw_path(&id), self.summary_path(&id)) {
                (Some(raw), Some(sum)) if raw.exists() => self.write_file(&sum, &summary),
                _ => Ok(()),
            }
        });
        // Released before the answer, which may use the store again.
        drop(guard);
        done(r)
    }

    fn remove(&self, ids: Vec<String>, done: Reply<()>) {
        let guard = changing();
        let r = ids.iter().try_for_each(|id| {
            for p in [self.raw_path(id), self.summary_path(id)]
                .into_iter()
                .flatten()
            {
                match std::fs::remove_file(p) {
                    Err(e) if e.kind() != std::io::ErrorKind::NotFound => {
                        return Err(e.to_string());
                    }
                    _ => {}
                }
            }
            Ok(())
        });
        drop(guard);
        done(r)
    }

    fn export(&self, batch: usize, mut each: Box<dyn FnMut(Vec<PlayEntry>)>, done: Reply<usize>) {
        let ids = match self.ids() {
            Ok(ids) => ids,
            Err(e) => return done(Err(e)),
        };
        let mut n = 0;
        for chunk in ids.chunks(batch.max(1)) {
            let entries: Vec<PlayEntry> = chunk
                .iter()
                .map(|id| {
                    self.read(id).unwrap_or_else(|| PlayEntry {
                        // A play whose file does not read: listed with
                        // what there is (its summary), never dropped.
                        id: id.clone(),
                        summary: self.summary(id),
                        record: self
                            .raw_path(id)
                            .and_then(|p| std::fs::read_to_string(p).ok())
                            .unwrap_or_default(),
                        ..PlayEntry::default()
                    })
                })
                .collect();
            n += entries.len();
            each(entries);
        }
        done(Ok(n))
    }
}

#[cfg(test)]
mod tests {
    use std::cell::RefCell;
    use std::rc::Rc;

    use super::*;
    use ddi_platform::history::play_id;

    fn entry(t: f64, summary: &str) -> PlayEntry {
        PlayEntry {
            id: play_id(t, 3),
            summary: Some(summary.into()),
            record: r#"{"time":1}"#.into(),
            log: vec![1, 2, 3, 0, 255],
            timing: vec![7; 20],
        }
    }

    fn get<T: 'static>(f: impl FnOnce(Reply<T>)) -> Result<T, String> {
        let out = Rc::new(RefCell::new(None));
        let o = out.clone();
        f(Box::new(move |r| *o.borrow_mut() = Some(r)));
        out.borrow_mut().take().expect("answered at once")
    }

    fn scratch(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("ddi-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        dir
    }

    #[test]
    fn plays_survive_a_restart_as_files() {
        let dir = scratch("plays");
        let store = FilePlayStore::new(dir.clone());
        // Nothing yet: no folder, no plays.
        assert_eq!(get(|d| store.summaries(d)).unwrap(), vec![]);
        for (t, s) in [(2.0, r#"{"n":2}"#), (1.0, r#"{"n":1}"#)] {
            get(|d| store.append(entry(t, s), d)).unwrap();
        }
        // A stray file, a play whose file does not read, a summary
        // without its play, and a crash's temporary file.
        std::fs::write(dir.join("notes.txt"), "hello").unwrap();
        std::fs::write(dir.join(format!("{}.json", play_id(9.0, 0))), "{nope").unwrap();
        std::fs::write(dir.join(format!("{}.summary.json", play_id(8.0, 0))), "{}").unwrap();
        std::fs::write(dir.join(format!("{}.json.tmp", play_id(7.0, 0))), "half").unwrap();
        let again = FilePlayStore::new(dir.clone());
        assert!(!dir.join(format!("{}.json.tmp", play_id(7.0, 0))).exists());
        assert_eq!(
            get(|d| again.summaries(d)).unwrap(),
            vec![
                (play_id(1.0, 3), Some(r#"{"n":1}"#.to_string())),
                (play_id(2.0, 3), Some(r#"{"n":2}"#.to_string())),
                (play_id(8.0, 0), Some("{}".to_string())),
                (play_id(9.0, 0), None),
            ]
        );
        let whole = get(|d| again.records(vec![play_id(2.0, 3), "missing".into()], d)).unwrap();
        assert_eq!(whole, vec![entry(2.0, r#"{"n":2}"#)]);
        // The play's file: the log in base64, no summary.
        let raw_path = dir.join(format!("{}.json", play_id(2.0, 3)));
        let raw = std::fs::read_to_string(&raw_path).unwrap();
        assert!(raw.contains(r#""log":"AQIDAP8=""#), "{raw}");
        assert!(raw.contains(r#""summary":null"#), "{raw}");
        // A new summary leaves the play's file as it was.
        let before = std::fs::metadata(&raw_path).unwrap().modified().unwrap();
        get(|d| {
            again.replace_summaries(
                vec![
                    (play_id(2.0, 3), r#"{"n":20}"#.into()),
                    (play_id(5.0, 0), "{}".into()),
                ],
                d,
            )
        })
        .unwrap();
        assert_eq!(std::fs::read_to_string(&raw_path).unwrap(), raw);
        assert_eq!(
            std::fs::metadata(&raw_path).unwrap().modified().unwrap(),
            before
        );
        assert_eq!(
            get(|d| again.records(vec![play_id(2.0, 3)], d)).unwrap()[0].summary,
            Some(r#"{"n":20}"#.into())
        );
        assert!(
            !dir.join(format!("{}.summary.json", play_id(5.0, 0)))
                .exists()
        );
        // The export: every play, the unreadable one with what there is.
        let seen = Rc::new(RefCell::new(Vec::new()));
        let s = seen.clone();
        let n = get(|d| again.export(3, Box::new(move |e| s.borrow_mut().push(e)), d)).unwrap();
        assert_eq!(n, 4);
        let batches = seen.borrow();
        assert_eq!(batches.iter().map(Vec::len).collect::<Vec<_>>(), [3, 1]);
        assert_eq!(batches[1][0].record, "{nope");
        get(|d| again.remove(vec![play_id(2.0, 3), "../storage".into()], d)).unwrap();
        assert!(!raw_path.exists());
        assert!(
            !dir.join(format!("{}.summary.json", play_id(2.0, 3)))
                .exists()
        );
        assert!(
            get(|d| again.append(
                PlayEntry {
                    id: "../x".into(),
                    ..entry(1.0, "{}")
                },
                d
            ))
            .is_err()
        );
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn the_model_runs_over_files() {
        use ddi_ui::state::history::Plays;
        let dir = scratch("plays-model");
        let store = Rc::new(FilePlayStore::new(dir.clone()));
        let plays = Plays::new(store.clone());
        plays.load(|r| r.unwrap());
        assert!(plays.history().is_empty());
        get(|d| store.append(entry(1.0, "{}"), d)).unwrap();
        let plays = Plays::new(store);
        plays.load(|r| r.unwrap());
        // Neither `{}` nor the record is a play: listed, not lost.
        assert_eq!(plays.history().unreadable(), [play_id(1.0, 3)]);
        plays.remove_all(|r| r.unwrap());
        assert_eq!(std::fs::read_dir(&dir).unwrap().count(), 0);
        let _ = std::fs::remove_dir_all(dir);
    }
}
