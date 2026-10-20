//! The play history on desktop in the data folder (`docs/plans/desktop.md`,
//! D10): `plays/` (two files a play, `crate::plays`) in the platform's
//! data directory, `~/Library/Application Support/DitoDitoInfinito` on
//! macOS (beside the settings), `%APPDATA%\DitoDitoInfinito` on Windows,
//! `~/.local/share/ditoditoinfinito` on Linux, where plays kept in the
//! config folder before (`~/.config/ditoditoinfinito/plays`) are moved
//! the first time. Plays judged by an older judge are worked out again
//! ([`Rework`]) as on the web.

use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::sync::Arc;
use std::sync::mpsc::{self, Receiver, Sender};
use std::time::{Duration, Instant};

use ddi_chart::{ChartDigest, Song};
use ddi_platform::history::{PlayStore, Reply};
use ddi_ui::state::history::{
    PASS_STEP_MS, Plays, REWORK_BATCH, ReworkStep, Reworked, Reworker, rework_entries,
};

use crate::plays::FilePlayStore;

/// The app's folder's name in the platform's folders.
pub const APP_FOLDER: &str = if cfg!(target_os = "linux") {
    "ditoditoinfinito"
} else {
    "DitoDitoInfinito"
};

/// `plays/` in the data folder.
pub fn plays_dir() -> Option<PathBuf> {
    Some(crate::paths::data_dir()?.join("plays"))
}

/// What moving the plays did.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Moved {
    /// Files moved.
    pub moved: usize,
    /// Files left in the old folder: the new one has a file of that
    /// name, or the copy failed.
    pub left: usize,
    /// Both folders held plays.
    pub both: bool,
}

/// Plays kept before in `old` go to `new`: the folder renamed when `new`
/// is not there, else file by file (each removed from `old` once copied,
/// so an interruption strands nothing and records nothing twice), never
/// over a file `new` has. `old` goes once it is empty.
pub fn move_plays(old: &Path, new: &Path) -> std::io::Result<Moved> {
    let mut m = Moved::default();
    if !old.is_dir() {
        return Ok(m);
    }
    if !new.exists() {
        if let Some(parent) = new.parent() {
            std::fs::create_dir_all(parent)?;
        }
        if std::fs::rename(old, new).is_ok() {
            m.moved = std::fs::read_dir(new)?.count();
            return Ok(m);
        }
    } else {
        m.both = true;
    }
    std::fs::create_dir_all(new)?;
    for e in std::fs::read_dir(old)?.flatten() {
        let to = new.join(e.file_name());
        if to.exists() || !e.path().is_file() {
            m.left += 1;
            continue;
        }
        // A rename where it can; across disks a copy, then the source
        // removed.
        let done = std::fs::rename(e.path(), &to).is_ok()
            || (std::fs::copy(e.path(), &to).is_ok() && std::fs::remove_file(e.path()).is_ok());
        if done {
            m.moved += 1;
        } else {
            m.left += 1;
        }
    }
    if m.left == 0 {
        let _ = std::fs::remove_dir(old);
    }
    Ok(m)
}

/// The history, loaded (the file store answers at once), and what works
/// its stale plays out again.
pub fn open() -> Option<(Rc<Plays>, Rework)> {
    let dir = plays_dir()?;
    // Linux's config and data folders differ: plays move before anything
    // records.
    if let Some(old) = crate::plays::default_dir()
        && old != dir
    {
        match move_plays(&old, &dir) {
            Ok(m) if m.left > 0 => eprintln!(
                "history: {} plays moved from {} to {}; {} left there (the same names are in \
                 both, or they could not be moved)",
                m.moved,
                old.display(),
                dir.display(),
                m.left
            ),
            Ok(_) => {}
            Err(e) => eprintln!(
                "history: moving {} to {}: {e}",
                old.display(),
                dir.display()
            ),
        }
    }
    let store = FilePlayStore::new(dir);
    let rework = Rework::new(store.clone());
    let plays = Plays::new(Rc::new(store));
    plays.load(|r| {
        if let Err(e) = r {
            eprintln!("history: {e}");
        }
    });
    Some((plays, rework))
}

/// Song `id`'s chart alone (no music); where that song is gone, a song's
/// with a chart of one of `digests` (the bundled songs first, then the
/// library's).
pub fn read_chart(id: &str, digests: &[ChartDigest]) -> Result<Song, String> {
    crate::play::load::read_chart(id).or_else(|e| {
        digests
            .iter()
            .find_map(|d| crate::library::find_by_digest(*d))
            .ok_or(e)
            .and_then(|other| crate::play::load::read_chart(&other))
    })
}

/// Stale plays worked out again on desktop
/// (`ddi_ui::state::history::Reworker`, as the web's `history.rs` runs
/// it): charts read, plays judged and their summaries written on a
/// thread of the history's own, so song select never waits for a chart,
/// a replay or a synced file; the summaries in memory changed on the
/// event loop's (song select's bests follow, `crate::songs`). Driven by
/// [`Rework::poll`] from the event loop, which sleeps until
/// [`Rework::due`].
pub struct Rework {
    reworker: Reworker<Arc<Song>>,
    /// The plays' folder, for the thread (a clone: it removes nothing).
    store: FilePlayStore,
    /// When the next step is due (`None`: none is scheduled).
    next: Option<Instant>,
    worker: Option<Worker>,
    playing: bool,
}

struct Worker {
    jobs: Sender<Job>,
    answers: Receiver<Answer>,
}

/// What the thread does.
enum Job {
    Read {
        song: String,
        digests: Vec<ChartDigest>,
    },
    Rework {
        ids: Vec<String>,
        chart: Arc<Song>,
        next_ms: u32,
    },
    Write {
        changes: Vec<(String, String)>,
        next_ms: u32,
    },
}

/// What the thread answers.
enum Answer {
    Read {
        song: String,
        chart: Result<Arc<Song>, String>,
    },
    Worked {
        worked: Result<Reworked, String>,
        next_ms: u32,
    },
    Written {
        result: Result<(), String>,
        next_ms: u32,
    },
}

/// The file store's answer (it answers at once).
fn at_once<T: 'static>(f: impl FnOnce(Reply<T>)) -> Result<T, String> {
    let out = Rc::new(std::cell::RefCell::new(None));
    let o = out.clone();
    f(Box::new(move |r| *o.borrow_mut() = Some(r)));
    out.borrow_mut()
        .take()
        .unwrap_or_else(|| Err("the play history did not answer".into()))
}

/// Runs `job` (on the thread; on the event loop's where no thread could
/// be started). A panic is an error, never a step left hanging.
fn run(store: &FilePlayStore, job: Job) -> Answer {
    let failed = |_| "the play history's work stopped".to_string();
    match job {
        Job::Read { song, digests } => {
            let chart = std::panic::catch_unwind(|| read_chart(&song, &digests).map(Arc::new))
                .unwrap_or_else(|p| Err(failed(p)));
            Answer::Read { song, chart }
        }
        Job::Rework {
            ids,
            chart,
            next_ms,
        } => {
            let worked = std::panic::catch_unwind(|| {
                let entries = at_once(|d| store.records(ids.clone(), d))?;
                Ok(rework_entries(&ids, &entries, &chart))
            })
            .unwrap_or_else(|p| Err(failed(p)));
            Answer::Worked { worked, next_ms }
        }
        Job::Write { changes, next_ms } => {
            let result =
                std::panic::catch_unwind(|| at_once(|d| store.replace_summaries(changes, d)))
                    .unwrap_or_else(|p| Err(failed(p)));
            Answer::Written { result, next_ms }
        }
    }
}

impl Rework {
    /// Over the plays in `store`'s folder; the first step once the
    /// background pass's wait has gone by (as the web's, once the
    /// summaries are in).
    pub fn new(store: FilePlayStore) -> Rework {
        Rework {
            reworker: Reworker::default(),
            store,
            next: Some(Instant::now() + Duration::from_millis(PASS_STEP_MS.into())),
            worker: None,
            playing: false,
        }
    }

    fn schedule(&mut self, ms: u32) {
        self.next = Some(Instant::now() + Duration::from_millis(ms.into()));
    }

    /// Song `id`'s chart is at hand (read for its play): its stale plays
    /// are worked out first, a few at a time.
    pub fn chart_loaded(&mut self, plays: &Plays, id: &str, song: &Song) {
        if let Some(ms) = self
            .reworker
            .chart_loaded(plays, id, || Arc::new(song.clone()))
        {
            self.schedule(ms);
        }
    }

    /// When [`Rework::poll`] has a step to take (`None`: nothing to wake
    /// for; the thread wakes the loop when it answers).
    pub fn due(&self) -> Option<Instant> {
        if self.playing { None } else { self.next }
    }

    /// The thread's answers taken, and the next step taken when due.
    /// `playing`: a song's play is on (nothing is worked out); `shown`:
    /// its prompt, play or results are up (else the song's chart is no
    /// longer at hand); `ready`: the library knows its songs (its scan is
    /// done: a chart is not looked for before). `wake` wakes the loop
    /// when the thread answers.
    pub fn poll(
        &mut self,
        plays: &Plays,
        playing: bool,
        shown: bool,
        ready: bool,
        wake: Option<&Arc<dyn Fn() + Send + Sync>>,
    ) {
        if playing != self.playing {
            self.playing = playing;
            if let Some(ms) = self.reworker.set_playing(playing) {
                self.schedule(ms);
            }
        }
        if !shown {
            self.reworker.clear_focus();
        }
        while let Some(answer) = self.worker.as_ref().and_then(|w| w.answers.try_recv().ok()) {
            self.answered(plays, answer, wake);
        }
        if !ready || self.next.is_none_or(|t| t > Instant::now()) {
            return;
        }
        self.next = None;
        match self.reworker.step(plays) {
            ReworkStep::Wait | ReworkStep::Done => {}
            ReworkStep::Read { song, digests } => {
                self.send(plays, Job::Read { song, digests }, wake)
            }
            ReworkStep::Rework {
                song,
                chart,
                next_ms,
            } => {
                let ids = plays.stale_batch(&song, REWORK_BATCH);
                if ids.is_empty() {
                    self.reworker.worked();
                    self.schedule(next_ms);
                } else {
                    self.send(
                        plays,
                        Job::Rework {
                            ids,
                            chart,
                            next_ms,
                        },
                        wake,
                    );
                }
            }
        }
    }

    /// Songs whose chart could not be read are tried again (the library
    /// was read again).
    pub fn library_changed(&mut self) {
        self.reworker.retry_skipped();
        // A pass that was done looks again (a step under way schedules
        // its own next one).
        if self.next.is_none() {
            self.schedule(PASS_STEP_MS);
        }
    }

    fn answered(
        &mut self,
        plays: &Plays,
        answer: Answer,
        wake: Option<&Arc<dyn Fn() + Send + Sync>>,
    ) {
        match answer {
            Answer::Read { song, chart } => {
                let ms = self.reworker.read(song, chart);
                self.schedule(ms);
            }
            Answer::Worked {
                worked: Ok(worked),
                next_ms,
            } => {
                let changes = plays.set_worked(worked);
                if changes.is_empty() {
                    self.reworker.worked();
                    self.schedule(next_ms);
                } else {
                    self.send(plays, Job::Write { changes, next_ms }, wake);
                }
            }
            Answer::Worked {
                worked: Err(e),
                next_ms,
            }
            | Answer::Written {
                result: Err(e),
                next_ms,
            } => {
                eprintln!("play history: {e}");
                self.reworker.worked();
                self.schedule(next_ms);
            }
            Answer::Written {
                result: Ok(()),
                next_ms,
            } => {
                self.reworker.worked();
                self.schedule(next_ms);
            }
        }
    }

    /// `job` to the thread (started when there is none, or again when it
    /// went); where none can be started, run here.
    fn send(&mut self, plays: &Plays, job: Job, wake: Option<&Arc<dyn Fn() + Send + Sync>>) {
        let mut job = job;
        if let Some(w) = &self.worker {
            match w.jobs.send(job) {
                Ok(()) => return,
                Err(mpsc::SendError(j)) => job = j,
            }
        }
        self.worker = None;
        let (jobs, rx) = mpsc::channel::<Job>();
        let (tx, answers) = mpsc::channel();
        let store = self.store.clone();
        let woken = wake.cloned();
        let started = std::thread::Builder::new()
            .name("ddi-history".into())
            .spawn(move || {
                for job in rx {
                    if tx.send(run(&store, job)).is_err() {
                        return;
                    }
                    if let Some(w) = &woken {
                        w();
                    }
                }
            });
        if started.is_ok() {
            match jobs.send(job) {
                Ok(()) => {
                    self.worker = Some(Worker { jobs, answers });
                    return;
                }
                Err(mpsc::SendError(j)) => job = j,
            }
        }
        // No thread: here, at once.
        let answer = run(&self.store, job);
        self.answered(plays, answer, wake);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn plays_move_and_merge_never_over_newer_ones() {
        let base = std::env::temp_dir().join(format!("ddi-history-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&base);
        let (old, new) = (base.join("config/plays"), base.join("data/plays"));
        std::fs::create_dir_all(&old).unwrap();
        std::fs::write(old.join("1.json"), b"{}").unwrap();
        assert_eq!(move_plays(&old, &new).unwrap().moved, 1);
        assert!(new.join("1.json").is_file() && !old.exists());
        // Both hold plays: merged file by file, never over a newer one.
        std::fs::create_dir_all(&old).unwrap();
        std::fs::write(old.join("1.json"), b"old").unwrap();
        std::fs::write(old.join("2.json"), b"{}").unwrap();
        let m = move_plays(&old, &new).unwrap();
        assert_eq!((m.moved, m.left, m.both), (1, 1, true));
        assert_eq!(std::fs::read(new.join("1.json")).unwrap(), b"{}");
        assert!(new.join("2.json").is_file() && !old.join("2.json").exists());
        assert!(old.join("1.json").exists());
        let _ = std::fs::remove_dir_all(&base);
    }
}
