//! Values kept between runs on desktop (P12): one JSON file of keys and
//! values (the same keys and JSON the web keeps in `localStorage`) in the
//! user's configuration folder; the session's in memory.

use std::cell::RefCell;
use std::collections::BTreeMap;
use std::path::PathBuf;
use std::rc::Rc;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Receiver, Sender};
use std::thread::JoinHandle;
use std::time::Duration;

use ddi_platform::storage::{self, Area, MemoryStore, Store};

/// `storage.json` in the configuration folder ([`crate::paths`]:
/// `$DDI_HOME/config` for a run that is not the player's).
pub fn default_path() -> Option<PathBuf> {
    Some(crate::paths::config_dir()?.join("storage.json"))
}

/// What the writing thread is given.
enum Job {
    /// The file's new contents.
    Write(String),
    /// Answered, once what came before is written, with how that went.
    Flush(Sender<Result<(), String>>),
}

/// Local values in a file, written whole after every change on a thread
/// of its own, which only ever writes the newest: reads see the newest
/// value from memory, and a change never waits for the disk. What is
/// still to be written is written before the store goes ([`Drop`]) and
/// on [`Store::flush`], which the shell calls as it quits.
pub struct FileStore {
    values: RefCell<BTreeMap<String, String>>,
    jobs: Option<Sender<Job>>,
    thread: Option<JoinHandle<()>>,
    /// The last write failed: the file may not have `values`.
    unsaved: Arc<AtomicBool>,
    session: MemoryStore,
}

impl FileStore {
    /// The values in `path`. A file that cannot be read as values is
    /// kept aside (`storage.json.bad`, numbered if one is there) and
    /// reported, never overwritten: the store starts empty.
    pub fn open(path: PathBuf) -> FileStore {
        let values = match std::fs::read_to_string(&path) {
            Ok(s) => match serde_json::from_str(&s) {
                Ok(v) => v,
                Err(e) => {
                    let aside = set_aside(&path);
                    eprintln!(
                        "{} is unreadable ({e}); kept as {}, starting afresh",
                        path.display(),
                        aside.map_or("nothing".into(), |p| p.display().to_string())
                    );
                    BTreeMap::new()
                }
            },
            Err(_) => BTreeMap::new(),
        };
        let (jobs, received) = mpsc::channel();
        let unsaved = Arc::new(AtomicBool::new(false));
        let flag = unsaved.clone();
        let thread = std::thread::Builder::new()
            .name("ddi-store".into())
            .spawn(move || write_jobs(&path, &received, &flag))
            .ok();
        FileStore {
            values: RefCell::new(values),
            jobs: Some(jobs),
            thread,
            unsaved,
            session: MemoryStore::default(),
        }
    }

    /// The values to the writing thread.
    fn write(&self) -> Result<(), String> {
        let json =
            serde_json::to_string_pretty(&*self.values.borrow()).map_err(|e| e.to_string())?;
        self.send(Job::Write(json))
    }

    fn send(&self, job: Job) -> Result<(), String> {
        self.jobs
            .as_ref()
            .and_then(|jobs| jobs.send(job).ok())
            .ok_or_else(|| "the store's thread is gone".to_string())
    }
}

impl Drop for FileStore {
    fn drop(&mut self) {
        self.jobs = None;
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

/// The writing thread: each turn takes all that is waiting and writes
/// the newest of it alone.
fn write_jobs(path: &std::path::Path, jobs: &Receiver<Job>, unsaved: &AtomicBool) {
    let mut last = Ok(());
    while let Ok(job) = jobs.recv() {
        let mut newest = None;
        let mut waiting = Vec::new();
        for job in std::iter::once(job).chain(jobs.try_iter()) {
            match job {
                Job::Write(json) => newest = Some(json),
                Job::Flush(answer) => waiting.push(answer),
            }
        }
        if let Some(json) = newest {
            last = crate::paths::write_whole(path, json.as_bytes()).map_err(|e| e.to_string());
            unsaved.store(last.is_err(), Ordering::Relaxed);
        }
        for answer in waiting {
            let _ = answer.send(last.clone());
        }
    }
}

/// Moves an unreadable `path` to `<path>.bad`, or `<path>.bad.<n>` when
/// that is taken; where it went.
fn set_aside(path: &std::path::Path) -> Option<PathBuf> {
    let base = path.with_extension("json.bad");
    let target = std::iter::once(base.clone())
        .chain((1..1000).map(|n| PathBuf::from(format!("{}.{n}", base.display()))))
        .find(|p| !p.exists())?;
    std::fs::rename(path, &target).ok()?;
    Some(target)
}

impl Store for FileStore {
    fn get(&self, area: Area, key: &str) -> Option<String> {
        match area {
            Area::Local => self.values.borrow().get(key).cloned(),
            Area::Session => self.session.get(area, key),
        }
    }

    fn set(&self, area: Area, key: &str, value: &str) -> Result<(), String> {
        match area {
            Area::Local => {
                let old = self.values.borrow_mut().insert(key.into(), value.into());
                // The file has it already.
                if old.as_deref() == Some(value) && !self.unsaved.load(Ordering::Relaxed) {
                    return Ok(());
                }
                self.write()
            }
            Area::Session => self.session.set(area, key, value),
        }
    }

    fn remove(&self, area: Area, key: &str) {
        match area {
            Area::Local => {
                if self.values.borrow_mut().remove(key).is_some() {
                    let _ = self.write();
                }
            }
            Area::Session => self.session.remove(area, key),
        }
    }

    fn flush(&self) -> Result<(), String> {
        let (answer, answered) = mpsc::channel();
        self.send(Job::Flush(answer))?;
        answered
            .recv_timeout(Duration::from_secs(10))
            .map_err(|e| e.to_string())?
    }
}

/// Installs the file store (or memory, with no configuration folder) and
/// the system clock.
pub fn install() {
    match default_path() {
        Some(path) => storage::install(Rc::new(FileStore::open(path))),
        None => storage::install(Rc::new(MemoryStore::default())),
    }
    storage::install_clock(crate::clock::epoch_ms);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn values_survive_a_restart() {
        let dir = std::env::temp_dir().join(format!("ddi-store-{}", std::process::id()));
        let path = dir.join("storage.json");
        let s = FileStore::open(path.clone());
        s.set(Area::Local, "ddi.settings.v1", r#"{"speed":3}"#)
            .unwrap();
        s.set(Area::Session, "ddi.menu.v1", "{}").unwrap();
        s.flush().unwrap();
        let again = FileStore::open(path);
        assert_eq!(
            again.get(Area::Local, "ddi.settings.v1").as_deref(),
            Some(r#"{"speed":3}"#)
        );
        assert_eq!(again.get(Area::Session, "ddi.menu.v1"), None);
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn menu_position_survives_reopening_the_desktop_store() {
        use ddi_ui::state::place::MenuPlace;
        let path = std::env::temp_dir().join(format!("ddi-menu-{}.json", std::process::id()));
        storage::install(Rc::new(FileStore::open(path.clone())));
        let place = MenuPlace {
            song: Some("u-last-song".into()),
            chart: Some(("u-last-song".into(), 4)),
            closed: Some(vec![ddi_ui::state::place::CLOSED_BUNDLED.into()]),
            ..Default::default()
        };
        place.save();
        storage::flush().unwrap();
        storage::install(Rc::new(FileStore::open(path.clone())));
        assert_eq!(MenuPlace::load(), place);
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn the_newest_value_is_read_at_once_and_written_by_the_flush_or_the_drop() {
        let dir = std::env::temp_dir().join(format!("ddi-store-late-{}", std::process::id()));
        let path = dir.join("storage.json");
        let s = FileStore::open(path.clone());
        for n in 0..200 {
            s.set(Area::Local, "k", &n.to_string()).unwrap();
            assert_eq!(s.get(Area::Local, "k"), Some(n.to_string()));
        }
        s.flush().unwrap();
        assert_eq!(
            FileStore::open(path.clone())
                .get(Area::Local, "k")
                .as_deref(),
            Some("199")
        );
        s.set(Area::Local, "k", "last").unwrap();
        s.remove(Area::Local, "gone");
        drop(s);
        assert_eq!(
            FileStore::open(path).get(Area::Local, "k").as_deref(),
            Some("last")
        );
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn a_failed_write_is_told_by_the_flush_and_tried_again() {
        let dir = std::env::temp_dir().join(format!("ddi-store-fail-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        // A file where the store's folder should be.
        let blocker = dir.join("blocker");
        std::fs::write(&blocker, "x").unwrap();
        let path = blocker.join("storage.json");
        let s = FileStore::open(path.clone());
        s.set(Area::Local, "k", "v").unwrap();
        assert!(s.flush().is_err());
        // The same value again goes to the file, the last write having failed.
        std::fs::remove_file(&blocker).unwrap();
        s.set(Area::Local, "k", "v").unwrap();
        s.flush().unwrap();
        assert!(std::fs::read_to_string(&path).unwrap().contains("\"k\""));
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn an_unreadable_file_is_kept_aside() {
        let dir = std::env::temp_dir().join(format!("ddi-store-bad-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("storage.json");
        for round in 0..2 {
            std::fs::write(&path, "{not json").unwrap();
            let s = FileStore::open(path.clone());
            assert_eq!(s.get(Area::Local, "x"), None);
            assert!(!path.exists());
            let aside = if round == 0 {
                dir.join("storage.json.bad")
            } else {
                dir.join("storage.json.bad.1")
            };
            assert_eq!(std::fs::read_to_string(aside).unwrap(), "{not json");
        }
        let _ = std::fs::remove_dir_all(dir);
    }
}
