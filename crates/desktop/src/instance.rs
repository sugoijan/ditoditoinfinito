//! One app a data folder (`docs/plans/desktop.md`, "One app a data
//! folder"). The app loads its settings at start and writes the whole
//! file on every change, so two apps on one folder undo each other's
//! changes (the online session and the announcer studio's draft too).
//! The app therefore takes an advisory OS lock on `instance.lock` in its
//! configuration folder ([`crate::paths::config_dir`]: a `DDI_HOME` of
//! its own is a folder of its own, a second, independent app) before it
//! reads its settings, and keeps it by keeping the file open until the
//! process ends: the system releases it then, however the process ended.
//! An app started on a folder another has does not start ([`start`]).
//!
//! Only the lock decides. The file's existence and contents never do:
//! what is written into it (the process and when it started) is for the
//! message alone, and the file is never deleted (deleting a lock file
//! races). The command-line utilities, which open no window, take no
//! lock ([`utility`]) and write no settings (`crate::store::install_reading`).
//!
//! The lock is the standard library's (`File::try_lock`: `flock` on
//! Unix, `LockFileEx` on Windows), on a file the standard library opened:
//! close-on-exec on Unix, not inheritable on Windows, so the processes
//! the app starts (the video decoder, the importer's helper, the
//! system's browser) never hold it.

use std::fs::{File, TryLockError};
use std::io::{Read as _, Seek as _, Write as _};
use std::path::Path;
use std::sync::Mutex;

/// The lock's file, in the configuration folder.
pub const FILE: &str = "instance.lock";

/// The exit status of an app that did not start because another has its
/// folder.
pub const EXIT_RUNNING: i32 = 3;

/// The runs that open no window, by the flag that makes them one and
/// whether it takes a value (as `crate::run` reads them, in its order).
const UTILITIES: [(&str, bool); 7] = [
    ("--danoni-bundle-info", false),
    ("--danoni-importer-check", false),
    ("--mp-bots", false),
    ("--video-check", false),
    ("--share", true),
    ("--shares", false),
    ("--delete-share", true),
];

/// The flag that makes this run a command-line utility (no window, no
/// lock, nothing saved to the settings), if it is one.
pub fn utility(args: &[String]) -> Option<&'static str> {
    UTILITIES.iter().find_map(|&(flag, valued)| {
        let at = args.iter().position(|a| a == flag)?;
        (!valued || at + 1 < args.len()).then_some(flag)
    })
}

/// A folder's lock, held until this is dropped.
pub struct Lock {
    /// Open for as long as the lock is held: closing it releases it.
    _file: File,
}

impl Lock {
    /// Keeps the lock until the process ends.
    pub fn keep(self) {
        *HELD.lock().unwrap_or_else(|e| e.into_inner()) = Some(self);
    }

    #[cfg(all(test, unix))]
    fn raw_fd(&self) -> std::os::fd::RawFd {
        std::os::fd::AsRawFd::as_raw_fd(&self._file)
    }
}

/// The app's lock, kept for the life of the process ([`Lock::keep`]).
static HELD: Mutex<Option<Lock>> = Mutex::new(None);

/// This process keeps a folder's lock.
pub fn held() -> bool {
    HELD.lock().unwrap_or_else(|e| e.into_inner()).is_some()
}

/// The process that has a folder, as its lock file says (for the message
/// alone; unknown where the file cannot be read: Windows refuses reads of
/// a locked file).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Other {
    pub pid: Option<u32>,
    /// When it started, in seconds since the epoch.
    pub started: Option<u64>,
}

impl Other {
    fn read(text: &str) -> Other {
        let field = |name: &str| {
            text.lines()
                .find_map(|l| l.strip_prefix(name)?.trim().parse::<u64>().ok())
        };
        Other {
            pid: field("pid ").and_then(|p| u32::try_from(p).ok()),
            started: field("started "),
        }
    }
}

/// What asking for a folder's lock came to.
pub enum Claim {
    /// This process has the folder.
    Ours(Lock),
    /// Another process has it.
    Theirs(Other),
    /// No lock could be asked for (the folder cannot be written, the
    /// file system has no locks): why.
    Unavailable(std::io::Error),
}

/// Asks for the lock of the configuration folder `dir`, without waiting.
pub fn claim(dir: &Path) -> Claim {
    let open = || {
        std::fs::create_dir_all(dir)?;
        File::options()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .open(dir.join(FILE))
    };
    let mut file = match open() {
        Ok(f) => f,
        Err(e) => return Claim::Unavailable(e),
    };
    match file.try_lock() {
        Ok(()) => {
            // For the other's message alone: failing to say it is nothing.
            let started = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map_or(0, |d| d.as_secs());
            let _ = file
                .set_len(0)
                .and_then(|()| file.rewind())
                .and_then(|()| {
                    write!(file, "pid {}\nstarted {started}\n", std::process::id())?;
                    file.flush()
                });
            Claim::Ours(Lock { _file: file })
        }
        Err(TryLockError::WouldBlock) => {
            let mut text = String::new();
            let _ = file.read_to_string(&mut text);
            Claim::Theirs(Other::read(&text))
        }
        Err(TryLockError::Error(e)) => Claim::Unavailable(e),
    }
}

/// The line on stderr of an app that does not start (English, as logs
/// are), `now` in seconds since the epoch.
pub fn refusal(dir: &Path, other: &Other, now: u64) -> String {
    let who = match (other.pid, other.started) {
        (Some(pid), Some(started)) => {
            let s = now.saturating_sub(started);
            let ago = match s {
                0..120 => format!("{s} s"),
                120..7200 => format!("{} min", s / 60),
                _ => format!("{} h", s / 3600),
            };
            format!("process {pid}, started {ago} ago")
        }
        (Some(pid), None) => format!("process {pid}"),
        _ => "another process".to_string(),
    };
    format!(
        "Dito Dito Infinito is already running on {} ({who}): use its window, or set DDI_HOME to another folder for a second, independent game",
        dir.display()
    )
}

/// Whether an app that does not start says so in a system dialog too: a
/// player's own run on a system with a display. A run with `DDI_HOME`
/// (an agent's, a test's, a second game started from a terminal) says it
/// on stderr alone, as one with no display.
fn shows_dialog() -> bool {
    let display = cfg!(any(target_os = "macos", windows))
        || ["WAYLAND_DISPLAY", "DISPLAY"]
            .iter()
            .any(|k| std::env::var_os(k).is_some_and(|v| !v.is_empty()));
    display && crate::paths::home().is_none()
}

/// The dialog of an app that does not start, in the player's language:
/// the settings' (read, never written), else the system's, or `--lang`'s.
fn dialog(args: &[String]) {
    crate::clock::epoch();
    crate::store::install_reading();
    ddi_l10n::set_system_languages(sys_locale::get_locales());
    ddi_ui::state::settings::Settings::load().choose_language();
    if let Some(tag) = args
        .iter()
        .position(|a| a == "--lang")
        .and_then(|i| args.get(i + 1))
    {
        ddi_l10n::force(tag);
    }
    rfd::MessageDialog::new()
        .set_level(rfd::MessageLevel::Info)
        .set_title(ddi_l10n::msg::shell::running_title().as_str())
        .set_description(ddi_l10n::msg::shell::running().as_str())
        .set_buttons(rfd::MessageButtons::Ok)
        .show();
}

/// What a run does about its folder's lock, before it reads its
/// settings. A utility ([`utility`]) takes none. The app takes it and
/// keeps it for the life of the process; where another app has the
/// folder it does not start: a line on stderr naming the folder and the
/// other process, a system dialog for a player's own run, and the
/// process ends with [`EXIT_RUNNING`]. Where no lock can be asked for,
/// the app starts as it did without one, saying so in one line.
pub fn start(args: &[String]) {
    if utility(args).is_some() {
        return;
    }
    let Some(dir) = crate::paths::config_dir() else {
        eprintln!("no configuration folder, so no instance lock: starting without it");
        return;
    };
    match claim(&dir) {
        Claim::Ours(lock) => lock.keep(),
        Claim::Theirs(other) => {
            let now = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map_or(0, |d| d.as_secs());
            eprintln!("{}", refusal(&dir, &other, now));
            if shows_dialog() {
                dialog(args);
            }
            std::process::exit(EXIT_RUNNING);
        }
        Claim::Unavailable(e) => eprintln!(
            "no instance lock on {} ({e}): starting without it",
            dir.display()
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    /// A scratch configuration folder of a test's own, emptied.
    fn scratch(name: &str) -> PathBuf {
        let dir = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../target/scratch/lock/unit")
            .join(name);
        let _ = std::fs::remove_dir_all(&dir);
        dir
    }

    fn ours(dir: &Path) -> Lock {
        match claim(dir) {
            Claim::Ours(lock) => lock,
            Claim::Theirs(other) => panic!("held by {other:?}"),
            Claim::Unavailable(e) => panic!("no lock: {e}"),
        }
    }

    #[test]
    fn a_folder_is_locked_once_until_the_lock_goes() {
        let dir = scratch("twice/config");
        let first = ours(&dir);
        // A second handle in this process is refused as another process's
        // would be, and reads who has the folder.
        let Claim::Theirs(other) = claim(&dir) else {
            panic!("locked twice");
        };
        assert_eq!(other.pid, Some(std::process::id()));
        assert!(other.started.is_some_and(|s| s > 1_700_000_000));
        // Still refused: asking changed nothing.
        assert!(matches!(claim(&dir), Claim::Theirs(_)));
        drop(first);
        // Released by the drop alone; the file stays, with the last
        // holder's words, which decide nothing.
        assert!(dir.join(FILE).is_file());
        let again = ours(&dir);
        drop(again);
        std::fs::write(dir.join(FILE), "pid 1\nstarted 5\n").unwrap();
        let _stale = ours(&dir);
        assert_eq!(
            Other::read(&std::fs::read_to_string(dir.join(FILE)).unwrap()).pid,
            Some(std::process::id())
        );
    }

    #[test]
    fn a_folder_of_its_own_does_not_collide() {
        let one = scratch("homes/one/config");
        let two = scratch("homes/two/config");
        let _one = ours(&one);
        let _two = ours(&two);
        assert!(matches!(claim(&one), Claim::Theirs(_)));
        assert!(matches!(claim(&two), Claim::Theirs(_)));
    }

    #[test]
    fn a_folder_that_cannot_be_written_has_no_lock() {
        let dir = scratch("blocked");
        std::fs::create_dir_all(&dir).unwrap();
        // A file where the configuration folder should be.
        std::fs::write(dir.join("config"), "x").unwrap();
        assert!(matches!(
            claim(&dir.join("config")),
            Claim::Unavailable(_)
        ));
    }

    /// The lock's descriptor closes in the processes this one starts.
    #[cfg(unix)]
    #[test]
    fn the_lock_is_closed_on_exec() {
        let lock = ours(&scratch("cloexec/config"));
        // SAFETY: a descriptor this test owns, its flags read.
        let flags = unsafe { libc::fcntl(lock.raw_fd(), libc::F_GETFD) };
        assert!(flags >= 0 && flags & libc::FD_CLOEXEC != 0);
    }

    #[test]
    fn the_runs_without_a_window_are_utilities() {
        let args = |s: &str| -> Vec<String> { s.split_whitespace().map(String::from).collect() };
        for (line, flag) in [
            ("--danoni-bundle-info", "--danoni-bundle-info"),
            ("--danoni-importer-check", "--danoni-importer-check"),
            ("--mp-bots --host --guests 2", "--mp-bots"),
            ("--mp-bots --help", "--mp-bots"),
            ("--video-check a.mp4 --frames", "--video-check"),
            ("--songs dir --share pack:Mine --expires 1d", "--share"),
            ("--sharing on --share a,b", "--share"),
            ("--shares", "--shares"),
            ("--delete-share abc", "--delete-share"),
        ] {
            assert_eq!(utility(&args(line)), Some(flag), "{line}");
        }
        for line in [
            "",
            "--silent --seconds 5",
            "--play song --chart 2 --auto --start",
            "--screen options",
            "--shared abc",
            "--sharing on",
            "--multiplayer on --screen room",
            "--join abc",
            "--calibrate audio",
            "--replay 12",
            "--edit song",
            "--help",
            // Without their value these open the window, as `run` reads them.
            "--share",
            "--delete-share",
        ] {
            assert_eq!(utility(&args(line)), None, "{line}");
        }
    }

    #[test]
    fn the_refusal_names_the_folder_and_the_process() {
        let dir = Path::new("/x/config");
        let line = refusal(
            dir,
            &Other {
                pid: Some(77),
                started: Some(1000),
            },
            1042,
        );
        assert!(line.contains("already running on /x/config"), "{line}");
        assert!(line.contains("process 77, started 42 s ago"), "{line}");
        assert!(line.contains("DDI_HOME"), "{line}");
        let old = Other {
            pid: Some(77),
            started: Some(1000),
        };
        assert!(refusal(dir, &old, 1000 + 600).contains("started 10 min ago"));
        assert!(refusal(dir, &old, 1000 + 3 * 3600).contains("started 3 h ago"));
        // A clock set back, a file that could not be read.
        assert!(refusal(dir, &old, 5).contains("started 0 s ago"));
        assert!(refusal(dir, &Other::default(), 5).contains("(another process)"));
        assert_eq!(Other::read("garbage\npid x\n"), Other::default());
    }

    /// The dialog's words, in English and in Japanese.
    #[test]
    fn the_dialog_has_its_words_in_each_language() {
        for tag in ["en", "ja"] {
            ddi_l10n::set_language(tag);
            let title = ddi_l10n::msg::shell::running_title();
            let body = ddi_l10n::msg::shell::running();
            assert!(!title.as_str().is_empty() && !body.as_str().is_empty());
            assert_ne!(title.as_str(), body.as_str());
        }
        ddi_l10n::set_language("en");
    }
}
