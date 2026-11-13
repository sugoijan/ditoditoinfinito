//! One app a data folder, between processes (`ddi_desktop::instance`):
//! each "app" here is this test binary started again as [`child`], which
//! does what `ddi_desktop::run` does first about the folder's lock
//! (`instance::start`, with the run's flags) and then waits, with no
//! window, no sound and no settings read. Every home is a scratch folder
//! (`target/scratch/lock/`), and with `DDI_HOME` set an app that does not
//! start says so on stderr alone, never in a dialog.

use std::io::{BufRead as _, BufReader};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::mpsc::{self, Receiver};
use std::time::Duration;

use ddi_desktop::instance;

/// An app's flags, for [`child`].
const ARGS: &str = "DDI_INSTANCE_ARGS";
/// Set for [`child`]: it starts `sleep` before it reports, as the app
/// starts its helpers.
const SPAWNS: &str = "DDI_INSTANCE_SPAWNS";

/// Not a test of its own: the process the tests below start. It does
/// nothing unless [`ARGS`] is set.
#[test]
fn child() {
    let Ok(args) = std::env::var(ARGS) else {
        return;
    };
    let args: Vec<String> = args.split_whitespace().map(String::from).collect();
    // Ends the process with `EXIT_RUNNING` where another has the folder.
    instance::start(&args);
    if std::env::var_os(SPAWNS).is_some() {
        let mut helper = Command::new("sleep")
            .arg("30")
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .expect("sleep");
        println!("spawned {}", helper.id());
        // Reaped if it ends first, as the app's are (`os::open_url`).
        std::thread::spawn(move || {
            let _ = helper.wait();
        });
    }
    println!("started {}", instance::held());
    // Until it is killed or told to go.
    let mut line = String::new();
    let _ = std::io::stdin().read_line(&mut line);
    std::process::exit(0);
}

/// A scratch home of a test's own, emptied.
fn home(name: &str) -> PathBuf {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../target/scratch/lock/process")
        .join(name);
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir.canonicalize().unwrap()
}

/// A started [`child`] and what it prints.
struct App {
    child: Child,
    lines: Receiver<String>,
}

impl App {
    fn start(home: &Path, args: &str, spawns: bool) -> App {
        let mut command = Command::new(std::env::current_exe().unwrap());
        command
            .args(["child", "--exact", "--nocapture", "--test-threads=1"])
            .env("DDI_HOME", home)
            .env(ARGS, args)
            .env_remove(SPAWNS)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        if spawns {
            command.env(SPAWNS, "1");
        }
        let mut child = command.spawn().unwrap();
        let out = child.stdout.take().unwrap();
        let (tx, lines) = mpsc::channel();
        std::thread::spawn(move || {
            for line in BufReader::new(out).lines().map_while(Result::ok) {
                if tx.send(line).is_err() {
                    break;
                }
            }
        });
        App { child, lines }
    }

    /// What follows `word` on the next line saying it (the test harness
    /// writes its own words before the child's first), within the time a
    /// loaded machine may take to start a process.
    fn line(&self, word: &str) -> Option<String> {
        loop {
            let line = self.lines.recv_timeout(Duration::from_secs(60)).ok()?;
            if let Some((_, rest)) = line.split_once(word) {
                return Some(rest.trim().to_string());
            }
        }
    }

    /// Started: whether it has its folder's lock.
    fn started(&self) -> Option<bool> {
        self.line("started ").map(|held| held == "true")
    }

    /// Killed outright, as a crash (SIGKILL on Unix): nothing of its own
    /// runs to release anything.
    fn kill(mut self) {
        self.child.kill().unwrap();
        self.child.wait().unwrap();
    }

    /// Its exit status and what it said on stderr, once it ended by
    /// itself.
    fn ended(self) -> (Option<i32>, String) {
        let out = self.child.wait_with_output().unwrap();
        (
            out.status.code(),
            String::from_utf8_lossy(&out.stderr).into_owned(),
        )
    }
}

#[test]
fn a_second_app_on_one_folder_does_not_start_until_the_first_is_gone() {
    let home = home("second");
    let first = App::start(&home, "--silent --seconds 5", false);
    assert_eq!(first.started(), Some(true));
    let lock = home.join("config").join(instance::FILE);
    assert!(lock.is_file());

    // The same folder: no start, the status, and the line naming the
    // folder and the process that has it.
    let second = App::start(&home, "", false);
    let pid = first.child.id();
    let (status, said) = second.ended();
    assert_eq!(status, Some(instance::EXIT_RUNNING), "{said}");
    let line = said
        .lines()
        .find(|l| l.contains("already running"))
        .unwrap_or_else(|| panic!("no line: {said}"));
    assert!(
        line.contains(&home.join("config").display().to_string()),
        "{line}"
    );
    if cfg!(windows) {
        // Windows refuses reads of a locked file: the line cannot name
        // the process there.
        let _ = pid;
        assert!(line.contains("(another process)"), "{line}");
    } else {
        assert!(line.contains(&format!("process {pid},")), "{line}");
    }
    // Still the first's: asking changed nothing.
    let third = App::start(&home, "--play song --auto", false);
    assert_eq!(third.ended().0, Some(instance::EXIT_RUNNING));

    // The first killed outright: the system released its lock, the file
    // stays, and a new app starts.
    first.kill();
    assert!(lock.is_file());
    let next = App::start(&home, "", false);
    assert_eq!(next.started(), Some(true));
    next.kill();
}

#[test]
fn utilities_and_other_folders_run_beside_the_app() {
    let home = home("beside");
    let app = App::start(&home, "", false);
    assert_eq!(app.started(), Some(true));
    // The runs that open no window take no lock and are not refused.
    for args in [
        "--shares",
        "--mp-bots --host",
        "--share pack:Mine",
        "--delete-share abc",
        "--video-check movie.mp4",
        "--danoni-bundle-info",
        "--danoni-importer-check",
    ] {
        let utility = App::start(&home, args, false);
        assert_eq!(utility.started(), Some(false), "{args}");
        utility.kill();
    }
    // A home of its own is a second, independent app.
    let other = App::start(&self::home("beside-other"), "", false);
    assert_eq!(other.started(), Some(true));
    // And the first still has its folder.
    let second = App::start(&home, "", false);
    assert_eq!(second.ended().0, Some(instance::EXIT_RUNNING));
    other.kill();
    app.kill();
}

/// A process the app started (the video decoder, the importer's helper,
/// the browser's command) does not inherit the lock: it may outlive the
/// app without keeping the folder.
#[cfg(unix)]
#[test]
fn a_process_the_app_started_does_not_keep_the_lock() {
    let home = home("inherit");
    let app = App::start(&home, "", true);
    let helper: i32 = app.line("spawned ").unwrap().parse().unwrap();
    assert_eq!(app.started(), Some(true));
    app.kill();
    // SAFETY: signal 0 only asks whether the process is there.
    let alive = |pid: i32| unsafe { libc::kill(pid, 0) } == 0;
    assert!(alive(helper), "the helper went with the app");
    let next = App::start(&home, "", false);
    let started = next.started();
    let outlived = alive(helper);
    // SAFETY: the `sleep` this test's app started.
    unsafe { libc::kill(helper, libc::SIGKILL) };
    assert_eq!(started, Some(true));
    assert!(outlived, "the helper ended before the next app started");
    next.kill();
}
