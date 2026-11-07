//! Bounded pipe ownership and the app side of the importer's protocol. The UI
//! never waits for filesystem work, a pipe write, or a child process to exit.
use super::report::{Problem, Report, Step, Summary, SystemError};
use super::{Finished, Progress, Prompt};
use crate::library::managed::{PlaybackSnapshot, PlaybackSong};
use ddi_importer_protocol::{Event, Failure, Outcome, Paths, Request, VERSION};
use std::{
    collections::{BTreeMap, BTreeSet, VecDeque},
    io::Read,
    path::Path,
    process::{Child, Command, Stdio},
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
        mpsc,
    },
    thread::JoinHandle,
    time::{Duration, Instant},
};

type Wire = Result<Option<Event>, Problem>;
struct Pipe {
    child: Child,
    requests: Option<mpsc::SyncSender<Request>>,
    events: Option<mpsc::Receiver<Wire>>,
    threads: Vec<JoinHandle<()>>,
    stderr: Arc<Mutex<VecDeque<u8>>>,
    /// Set once the helper's diagnostics reach their end.
    stderr_done: Arc<AtomicBool>,
}
impl Pipe {
    fn spawn(executable: &Path) -> Result<Self, Problem> {
        let start = |e: std::io::Error| Problem::Start(SystemError::from(e));
        let child = crate::os::spawn(
            Command::new(executable)
                .stdin(Stdio::piped())
                .stdout(Stdio::piped())
                .stderr(Stdio::piped()),
        )
        .map_err(start)?;
        // Establish ownership before fallible pipe/thread setup. A partial setup
        // failure must kill and reap the child, rather than merely dropping it.
        let stderr = Arc::new(Mutex::new(VecDeque::new()));
        let mut pipe = Self {
            child,
            requests: None,
            events: None,
            threads: Vec::new(),
            stderr,
            stderr_done: Arc::new(AtomicBool::new(false)),
        };
        let missing = |pipe| Problem::Start(SystemError::other(pipe));
        let mut input = pipe
            .child
            .stdin
            .take()
            .ok_or_else(|| missing("Missing importer input pipe"))?;
        let mut output = pipe
            .child
            .stdout
            .take()
            .ok_or_else(|| missing("Missing importer output pipe"))?;
        let mut errors = pipe
            .child
            .stderr
            .take()
            .ok_or_else(|| missing("Missing importer diagnostic pipe"))?;
        let (requests, rx) = mpsc::sync_channel(4);
        let (tx, events) = mpsc::sync_channel(1);
        let tail = pipe.stderr.clone();
        let done = pipe.stderr_done.clone();
        pipe.requests = Some(requests);
        pipe.events = Some(events);
        let writer = std::thread::Builder::new()
            .name("ddi-importer-write".into())
            .spawn(move || {
                while let Ok(message) = rx.recv() {
                    if ddi_importer_protocol::write(&mut input, &message).is_err() {
                        break;
                    }
                }
            })
            .map_err(start)?;
        pipe.threads.push(writer);
        let reader = std::thread::Builder::new()
            .name("ddi-importer-read".into())
            .spawn(move || {
                loop {
                    let message = ddi_importer_protocol::read::<Event>(&mut output).map_err(|e| {
                        if e.kind() == std::io::ErrorKind::InvalidData {
                            Problem::Protocol(e.to_string())
                        } else {
                            Problem::Connection(SystemError::from(e))
                        }
                    });
                    let end = !matches!(message, Ok(Some(_)));
                    if tx.send(message).is_err() || end {
                        break;
                    }
                }
            })
            .map_err(start)?;
        pipe.threads.push(reader);
        let diagnostics = std::thread::Builder::new()
            .name("ddi-importer-stderr".into())
            .spawn(move || {
                let mut buffer = [0; 4096];
                loop {
                    match errors.read(&mut buffer) {
                        Ok(0) => break,
                        Ok(n) => {
                            if let Ok(mut tail) = tail.lock() {
                                tail.extend(&buffer[..n]);
                                while tail.len() > 32 * 1024 {
                                    tail.pop_front();
                                }
                            }
                        }
                        Err(e) if e.kind() == std::io::ErrorKind::Interrupted => continue,
                        Err(_) => break,
                    }
                }
                done.store(true, Ordering::Release);
            })
            .map_err(start)?;
        pipe.threads.push(diagnostics);
        Ok(pipe)
    }
    fn send(&self, message: Request) -> Result<(), Problem> {
        self.requests
            .as_ref()
            .ok_or_else(|| lost("Importer input closed"))?
            .try_send(message)
            .map_err(|_| lost("Importer stopped accepting commands"))
    }
    /// What the helper wrote to its diagnostics, after waiting up to
    /// `settle` for their end: a helper that reports a failure and exits
    /// has written them before, but their thread may not have read them
    /// all yet.
    fn diagnostics(&self, settle: Duration) -> String {
        let until = Instant::now() + settle;
        while !self.stderr_done.load(Ordering::Acquire) && Instant::now() < until {
            std::thread::sleep(Duration::from_millis(5));
        }
        self.stderr
            .lock()
            .map(|s| {
                String::from_utf8_lossy(&s.iter().copied().collect::<Vec<_>>())
                    .trim()
                    .to_owned()
            })
            .unwrap_or_default()
    }
}
impl Drop for Pipe {
    fn drop(&mut self) {
        // Kill/wait closes a blocked pipe write. Drop the bounded receiver BEFORE
        // joining its producer, so an unread terminal cannot deadlock teardown.
        let _ = self.child.kill();
        let _ = self.child.wait();
        self.events.take();
        self.requests.take();
        for thread in self.threads.drain(..) {
            let _ = thread.join();
        }
    }
}

/// The pipes to the importer closed or failed: how (technical, data).
fn lost(how: &str) -> Problem {
    Problem::Connection(SystemError::other(how))
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Phase {
    Hello,
    Initialize,
    Import,
    Closing,
}
struct Snapshot {
    id: Option<u64>,
    rows: Vec<PlaybackSong>,
    bytes: usize,
    ids: BTreeSet<String>,
}
impl Snapshot {
    fn push(
        &mut self,
        id: Option<u64>,
        value: serde_json::Value,
        pack: Option<String>,
        directory: std::path::PathBuf,
        library: &Path,
    ) -> Result<(), String> {
        if self.id != id || self.rows.len() >= 100_000 {
            return Err("Importer snapshot order/count differs".into());
        }
        if pack.as_ref().is_some_and(|s| {
            s.trim().is_empty() || s.chars().count() > 512 || s.chars().any(char::is_control)
        }) {
            return Err("Invalid catalog pack name".into());
        }
        self.bytes = self
            .bytes
            .checked_add(
                serde_json::to_vec(&value).map_err(|e| e.to_string())?.len()
                    + pack.as_ref().map_or(0, String::len),
            )
            .ok_or("Snapshot size overflow")?;
        if self.bytes > 128 * 1024 * 1024 {
            return Err("Importer snapshot exceeds metadata limit".into());
        }
        // Never reconstruct transaction directories from a URL or an entry id.
        // Only the helper's verified immutable publication may enter playback.
        if !directory.is_absolute()
            || directory == library
            || !directory.starts_with(library)
            || directory.canonicalize().map_err(|e| e.to_string())? != directory
        {
            return Err("Importer returned a directory outside its managed library".into());
        }
        let entry: ddi_library::manifest::ManifestEntry =
            serde_json::from_value(value).map_err(|e| format!("Invalid catalog entry: {e}"))?;
        if entry.id.is_empty() || !self.ids.insert(entry.id.clone()) {
            return Err("Importer snapshot repeats a song identity".into());
        }
        self.rows.push(PlaybackSong {
            entry,
            pack,
            directory,
        });
        Ok(())
    }
}
fn publish(
    progress: &Mutex<Progress>,
    stage: Option<Step>,
    origin: Option<Prompt>,
    wake: &dyn Fn(),
) {
    if let Ok(mut p) = progress.lock() {
        p.stage = stage;
        p.work = None;
        p.origin = origin;
    }
    wake();
}

#[allow(clippy::too_many_arguments)]
pub(super) fn run(
    executable: &Path,
    paths: Paths,
    url: Option<String>,
    offline: bool,
    origins: BTreeMap<String, String>,
    cancel: &AtomicBool,
    progress: &Mutex<Progress>,
    controls: mpsc::Receiver<Request>,
    wake: &dyn Fn(),
) -> Result<Finished, Problem> {
    let mut pipe = Pipe::spawn(executable)?;
    let mut finished = Finished::default();
    let mut committed = false;
    let result = (|| -> Result<(), Problem> {
        pipe.send(Request::Hello { version: VERSION })?;
        let mut phase = Phase::Hello;
        let mut snapshot: Option<Snapshot> = None;
        let mut snapshot_error: Option<String> = None;
        let mut initialized_snapshot = false;
        // The initial catalog is not proof that this import's refresh completed.
        // A malformed/truncated refresh must never restore hidden rows from it.
        let mut imported_snapshot = false;
        let mut cancel_at = None;
        let mut closing_at = None;
        let started = Instant::now();
        loop {
            if started.elapsed() > Duration::from_secs(if url.is_some() { 30 * 60 } else { 5 * 60 })
            {
                return Err(Problem::TimedOut);
            }
            if cancel.load(Ordering::Acquire) && cancel_at.is_none() && phase != Phase::Closing {
                cancel_at = Some(Instant::now());
                publish(progress, Some(Step::Cancelling), None, wake);
                if phase == Phase::Import {
                    pipe.send(Request::Cancel { id: 1 })?;
                } else {
                    pipe.send(Request::Shutdown)?;
                    phase = Phase::Closing;
                    closing_at = Some(Instant::now());
                }
            }
            if cancel_at.is_some_and(|at| at.elapsed() > Duration::from_secs(10))
                || closing_at.is_some_and(|at| at.elapsed() > Duration::from_secs(10))
            {
                return Err(Problem::NotStopped);
            }
            while let Ok(control) = controls.try_recv() {
                if phase == Phase::Import && cancel_at.is_none() {
                    pipe.send(control)?;
                }
            }
            let event = match pipe
                .events
                .as_ref()
                .ok_or_else(|| lost("Importer output closed"))?
                .recv_timeout(Duration::from_millis(50))
            {
                Ok(result) => result?.ok_or_else(|| {
                    lost("The importer closed its output before completing the protocol")
                })?,
                Err(mpsc::RecvTimeoutError::Timeout) => continue,
                Err(mpsc::RecvTimeoutError::Disconnected) => {
                    return Err(lost("The importer response thread stopped"));
                }
            };
            match event {
                Event::Hello {
                    version,
                    capabilities,
                } if phase == Phase::Hello => {
                    if version != VERSION
                        || !capabilities.url_import
                        || !capabilities.offline
                        || !capabilities.origin_approval
                    {
                        return Err(Problem::Incompatible);
                    }
                    pipe.send(Request::Initialize {
                        paths: paths.clone(),
                        origins: origins.clone(),
                    })?;
                    phase = Phase::Initialize;
                }
                Event::Progress {
                    id,
                    stage,
                    progress: work,
                } if (phase == Phase::Initialize && id.is_none())
                    || (phase == Phase::Import && id == Some(1)) =>
                {
                    // Publish stage-local counts independently of stage changes.
                    let work = work.filter(|(done, total)| *total > 0 && done <= total);
                    let stage = Step::Importer(stage);
                    if let Ok(mut p) = progress.lock()
                        && (p.stage != Some(stage) || p.work != work)
                    {
                        p.stage = Some(stage);
                        p.work = work;
                        drop(p);
                        wake();
                    }
                }
                Event::OriginRequired { id: 1, origin }
                    if phase == Phase::Import && cancel_at.is_none() =>
                {
                    let invalid =
                        || Problem::Protocol("Importer requested an invalid origin".into());
                    let parsed = url::Url::parse(&origin).map_err(|_| invalid())?;
                    if !matches!(parsed.scheme(), "http" | "https")
                        || parsed.origin().ascii_serialization() != origin
                    {
                        return Err(invalid());
                    }
                    pipe.send(Request::OriginDecision { id: 1, origin,
                        decision: Some("User-triggered single-work desktop import; no site-terms review claimed".into()) })?;
                }
                Event::OriginRequired { .. } if phase == Phase::Import && cancel_at.is_some() => {}
                Event::SnapshotBegin { id, revision }
                    if (phase == Phase::Initialize && id.is_none())
                        || (phase == Phase::Import && id == Some(1)) =>
                {
                    if snapshot.is_some() || revision.is_empty() || revision.len() > 256 {
                        return Err(Problem::Protocol(
                            "Invalid importer snapshot boundary".into(),
                        ));
                    }
                    if phase == Phase::Import {
                        imported_snapshot = false;
                    }
                    snapshot = Some(Snapshot {
                        id,
                        rows: Vec::new(),
                        bytes: 0,
                        ids: BTreeSet::new(),
                    });
                    snapshot_error = None;
                }
                Event::SnapshotSong {
                    pack,
                    id,
                    entry,
                    directory,
                } if snapshot.is_some() => {
                    if snapshot_error.is_none() {
                        snapshot_error = snapshot
                            .as_mut()
                            .unwrap()
                            .push(id, entry, pack, directory, &paths.library)
                            .err();
                    }
                }
                Event::SnapshotEnd { id, count } if snapshot.is_some() => {
                    let rows = snapshot.take().unwrap();
                    if rows.id != id || (snapshot_error.is_none() && rows.rows.len() != count) {
                        return Err(Problem::Protocol("Incomplete importer snapshot".into()));
                    }
                    if snapshot_error.is_none() {
                        match crate::library::managed::songs(PlaybackSnapshot { songs: rows.rows })
                        {
                            Ok(songs) => {
                                finished.songs = Some(songs);
                                if phase == Phase::Initialize {
                                    initialized_snapshot = true;
                                } else if phase == Phase::Import {
                                    imported_snapshot = true;
                                }
                            }
                            Err(e) => snapshot_error = Some(e),
                        }
                    }
                }
                Event::Initialized { ready, error } if phase == Phase::Initialize => {
                    if !ready {
                        return Err(Problem::CatalogUnopened(error));
                    }
                    if !initialized_snapshot || snapshot.is_some() {
                        return Err(snapshot_error.map_or_else(
                            || {
                                Problem::Protocol(
                                    "Importer did not complete its initial catalog snapshot".into(),
                                )
                            },
                            Problem::CatalogRejected,
                        ));
                    }
                    if let Some(url) = &url {
                        pipe.send(Request::Import {
                            id: 1,
                            url: url.clone(),
                            offline,
                        })?;
                        phase = Phase::Import;
                    } else {
                        pipe.send(Request::Shutdown)?;
                        phase = Phase::Closing;
                        closing_at = Some(Instant::now());
                    }
                }
                Event::Terminal {
                    id: 1,
                    status,
                    songs,
                    restore_ids,
                    failure,
                    refresh,
                } if phase == Phase::Import => {
                    committed = matches!(status, Outcome::Installed | Outcome::Unchanged);
                    let mut report = Report::ended(status, songs, failure, refresh);
                    if let Some(e) = snapshot_error.take() {
                        report.details.push(Problem::CatalogRejected(e));
                    }
                    if snapshot.is_some() {
                        report.details.push(Problem::CatalogIncomplete);
                    }
                    if committed && imported_snapshot {
                        let published: BTreeSet<_> = finished
                            .songs
                            .as_ref()
                            .into_iter()
                            .flatten()
                            .map(|s| s.entry.id.as_str())
                            .collect();
                        finished.restore = restore_ids
                            .into_iter()
                            .filter(|id| published.contains(id.as_str()))
                            .collect();
                    }
                    finished.report = Some(report);
                    publish(progress, Some(Step::Finishing), None, wake);
                    pipe.send(Request::Shutdown)?;
                    phase = Phase::Closing;
                    closing_at = Some(Instant::now());
                }
                Event::Fatal { code, message } => {
                    return Err(Problem::Importer(Failure { code, message }));
                }
                Event::Bye if phase == Phase::Closing => {
                    if cancel_at.is_some() && finished.report.is_none() {
                        finished.report = Some(Report::new(Summary::CancelledEarly));
                    }
                    let until = Instant::now() + Duration::from_secs(5);
                    loop {
                        if let Some(status) = pipe
                            .child
                            .try_wait()
                            .map_err(|e| Problem::Connection(SystemError::from(e)))?
                        {
                            if !status.success() {
                                return Err(Problem::Exited(status.to_string()));
                            }
                            return Ok(());
                        }
                        if Instant::now() >= until {
                            return Err(Problem::NoExit);
                        }
                        std::thread::sleep(Duration::from_millis(10));
                    }
                }
                _ if cancel_at.is_some() && phase == Phase::Closing => {}
                _ => {
                    return Err(Problem::Protocol(
                        "Importer sent a response out of protocol order".into(),
                    ));
                }
            }
        }
    })();
    if let Err(problem) = result {
        let mut problems = vec![problem];
        let diagnostics = pipe.diagnostics(Duration::from_millis(500));
        if !diagnostics.is_empty() {
            problems.push(Problem::Diagnostics(diagnostics));
        }
        if committed {
            // A completed commit is durable even if later IPC or child teardown
            // fails. Never turn it into a cancellation or a claimed rollback.
            if let Some(report) = &mut finished.report {
                report.after.extend(problems);
            }
        } else {
            finished.report = Some(Report {
                details: problems,
                ..Report::new(Summary::Failed)
            });
        }
    }
    Ok(finished)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn snapshot_rejects_outside_paths_duplicate_ids_and_truncation() {
        let root = std::env::temp_dir().canonicalize().unwrap();
        let mut snapshot = Snapshot {
            id: Some(7),
            rows: Vec::new(),
            bytes: 0,
            ids: BTreeSet::new(),
        };
        assert!(
            snapshot
                .push(Some(8), serde_json::json!({}), None, root.clone(), &root)
                .is_err()
        );
        assert!(
            snapshot
                .push(Some(7), serde_json::json!({}), None, root.clone(), &root)
                .is_err()
        );
        let song = root.join(format!("ddi-snapshot-{}", std::process::id()));
        std::fs::create_dir_all(&song).unwrap();
        let source = "#TITLE:Test;#BPMS:0=120;#NOTEDATA:;#STEPSTYPE:dance-single;#DIFFICULTY:Hard;#METER:1;#NOTES:1000;";
        let parsed = ddi_chart::formats::sm::parse_ssc(source).unwrap();
        let mut entry = ddi_library::manifest::summarize(Default::default(), &parsed);
        entry.id = "admitted-id".into();
        let value = serde_json::to_value(entry).unwrap();
        snapshot
            .push(
                Some(7),
                value.clone(),
                Some("Collection".into()),
                song.clone(),
                &root,
            )
            .unwrap();
        assert_eq!(snapshot.rows[0].pack.as_deref(), Some("Collection"));
        assert!(
            snapshot
                .push(Some(7), value, None, song.clone(), &root)
                .is_err()
        );
        std::fs::remove_dir_all(song).unwrap();
    }

    #[derive(Clone, Copy)]
    enum FixtureMode {
        Truncated,
        CommittedIncomplete,
        StderrFailure,
    }
    struct Fixture {
        directory: std::path::PathBuf,
        executable: std::path::PathBuf,
        paths: Paths,
    }
    impl Drop for Fixture {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.directory);
        }
    }
    fn emit(event: Event) -> String {
        let mut frame = Vec::new();
        ddi_importer_protocol::write(&mut frame, &event).unwrap();
        let escaped: String = frame.iter().map(|byte| format!("\\{byte:03o}")).collect();
        format!("printf '{escaped}'\n")
    }
    fn consume(request: Request) -> String {
        let mut frame = Vec::new();
        ddi_importer_protocol::write(&mut frame, &request).unwrap();
        format!(
            "/bin/dd bs=1 count={} of=/dev/null 2>/dev/null\n",
            frame.len()
        )
    }
    fn fixture(mode: FixtureMode) -> Fixture {
        use std::os::unix::fs::PermissionsExt;
        use std::sync::atomic::AtomicUsize;
        // The clock ticks in microseconds on macOS: tests starting together
        // in one process are told apart by the count.
        static FIXTURES: AtomicUsize = AtomicUsize::new(0);
        let directory = std::env::temp_dir().join(format!(
            "ddi-client-pipes-{}-{}-{}",
            std::process::id(),
            FIXTURES.fetch_add(1, Ordering::Relaxed),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir(&directory).unwrap();
        let directory = directory.canonicalize().unwrap();
        let paths = Paths {
            cache: directory.join("cache"),
            staging: directory.join("staging"),
            library: directory.join("library"),
        };
        let executable = directory.join("helper.sh");
        // Synthetic executable, not a JSON IPC file. It exchanges actual framed
        // bytes over its anonymous pipes and never reads/imports any real songs.
        let mut script = String::from("#!/bin/sh\nset -e\n");
        script.push_str(&consume(Request::Hello { version: VERSION }));
        script.push_str(&emit(Event::Hello {
            version: VERSION,
            capabilities: ddi_importer_protocol::Capabilities {
                url_import: true,
                offline: true,
                origin_approval: true,
            },
        }));
        script.push_str(&consume(Request::Initialize {
            paths: paths.clone(),
            origins: BTreeMap::new(),
        }));
        script.push_str(&emit(Event::SnapshotBegin {
            id: None,
            revision: if matches!(mode, FixtureMode::CommittedIncomplete) {
                "a".repeat(64)
            } else {
                "empty".into()
            },
        }));
        let count = if matches!(mode, FixtureMode::CommittedIncomplete) {
            let directory = paths.library.join("immutable-song");
            std::fs::create_dir_all(&directory).unwrap();
            let source = "#TITLE:Original;#BPMS:0=120;#NOTEDATA:;#STEPSTYPE:dance-single;#DIFFICULTY:Hard;#METER:1;#NOTES:1000;";
            std::fs::write(directory.join("song.ssc"), source).unwrap();
            let parsed = ddi_chart::formats::sm::parse_ssc(source).unwrap();
            let mut entry = ddi_library::manifest::summarize(Default::default(), &parsed);
            entry.id = "admitted-unchanged".into();
            entry.chart = "song.ssc".into();
            entry.music = "music.opus".into();
            script.push_str(&emit(Event::SnapshotSong {
                pack: Some("Collection".into()),
                id: None,
                entry: serde_json::to_value(entry).unwrap(),
                directory,
            }));
            1
        } else {
            0
        };
        script.push_str(&emit(Event::SnapshotEnd { id: None, count }));
        script.push_str(&emit(Event::Initialized {
            ready: true,
            error: None,
        }));
        script.push_str(&consume(Request::Import {
            id: 1,
            url: "https://example.invalid/work".into(),
            offline: false,
        }));
        script.push_str(&emit(Event::OriginRequired {
            id: 1,
            origin: "https://music.example.invalid".into(),
        }));
        script.push_str(&consume(Request::OriginDecision {
            id: 1,
            origin: "https://music.example.invalid".into(),
            decision: Some(
                "User-triggered single-work desktop import; no site-terms review claimed".into(),
            ),
        }));
        script.push_str(&emit(Event::Progress {
            id: Some(1),
            stage: ddi_importer_protocol::Stage::ConvertingMusic,
            progress: Some((2, 5)),
        }));
        match mode {
            FixtureMode::Truncated => {
                script.push_str(&emit(Event::SnapshotBegin {
                    id: Some(1),
                    revision: "new".into(),
                }));
                script.push_str("printf '\\000\\000'\n");
            }
            FixtureMode::CommittedIncomplete => {
                script.push_str(&emit(Event::SnapshotBegin {
                    id: Some(1),
                    revision: "new".into(),
                }));
                script.push_str(&emit(Event::Terminal {
                    id: 1,
                    status: Outcome::Installed,
                    songs: Some(ddi_importer_protocol::Counts {
                        added: 1,
                        replaced: 0,
                        skipped: 0,
                    }),
                    restore_ids: vec!["admitted-unchanged".into()],
                    failure: None,
                    refresh: None,
                }));
                script.push_str(&consume(Request::Shutdown));
                script.push_str(&emit(Event::Bye));
            }
            FixtureMode::StderrFailure => {
                script.push_str(&format!("printf '%s' '{}' >&2\n", "X".repeat(64 * 1024)));
                script.push_str(&emit(Event::Fatal {
                    code: "fixture-error".into(),
                    message: "synthetic failure".into(),
                }));
            }
        }
        std::fs::write(&executable, script).unwrap();
        std::fs::set_permissions(&executable, std::fs::Permissions::from_mode(0o700)).unwrap();
        Fixture {
            directory,
            executable,
            paths,
        }
    }
    fn run_fixture(mode: FixtureMode) -> Finished {
        let fixture = fixture(mode);
        let (_controls, requests) = mpsc::channel();
        let progress = Mutex::<Progress>::default();
        let counted = AtomicBool::new(false);
        let finished = run(
            &fixture.executable,
            fixture.paths.clone(),
            Some("https://example.invalid/work".into()),
            false,
            BTreeMap::new(),
            &AtomicBool::new(false),
            &progress,
            requests,
            &|| {
                let progress = progress.lock().unwrap();
                assert!(
                    progress.origin.is_none(),
                    "desktop must not ask for site approval"
                );
                if progress.work == Some((2, 5)) {
                    assert_eq!(
                        progress.stage,
                        Some(Step::Importer(
                            ddi_importer_protocol::Stage::ConvertingMusic
                        ))
                    );
                    counted.store(true, Ordering::Relaxed);
                }
            },
        )
        .unwrap();
        assert!(counted.load(Ordering::Relaxed));
        finished
    }
    #[test]
    fn truncated_import_pipe_retains_complete_initial_catalog() {
        let finished = run_fixture(FixtureMode::Truncated);
        assert!(finished.songs.is_some_and(|songs| songs.is_empty()));
        assert!(finished.restore.is_empty());
        assert!(finished.report.unwrap().error());
    }
    #[test]
    fn committed_terminal_without_snapshot_end_keeps_success_but_never_restores_rows() {
        let finished = run_fixture(FixtureMode::CommittedIncomplete);
        assert!(
            finished
                .songs
                .is_some_and(|songs| songs.len() == 1 && songs[0].entry.id == "admitted-unchanged")
        );
        assert!(finished.restore.is_empty());
        let report = finished.report.unwrap();
        assert!(!report.error());
        assert_eq!(
            report.summary,
            Summary::Installed(ddi_importer_protocol::Counts {
                added: 1,
                replaced: 0,
                skipped: 0
            })
        );
        assert_eq!(report.details, [Problem::CatalogIncomplete]);
    }
    #[test]
    fn helper_stderr_flood_keeps_only_bounded_diagnostic_tail() {
        let finished = run_fixture(FixtureMode::StderrFailure);
        let report = finished.report.unwrap();
        assert!(report.error());
        let [
            Problem::Importer(failure),
            Problem::Diagnostics(diagnostics),
        ] = &report.details[..]
        else {
            panic!("{:?}", report.details);
        };
        assert_eq!(failure.code, "fixture-error");
        assert!(!diagnostics.is_empty());
        assert!(diagnostics.len() <= 32 * 1024);
        assert!(diagnostics.bytes().all(|byte| byte == b'X'));
    }
}
