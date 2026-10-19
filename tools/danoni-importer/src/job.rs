use super::{Emitter, Pending, PendingOrigin, Report, Session, failure, sdk_failure};
use ddi_danoni_export::{
    Error, import_host, import_job,
    managed_store::{PlaybackSnapshot, Store},
    native,
    network::NetworkCancel,
    runtime::Installation,
};
use ddi_importer_protocol::{Event, Failure, Outcome, Paths};
use std::{
    collections::BTreeMap,
    fs, io,
    path::{Component, Path},
    sync::{Arc, Mutex, mpsc},
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

fn cancelled() -> Failure {
    failure("request-cancelled", "Import cancelled before publication")
}
fn check(cancel: &NetworkCancel) -> Result<(), Failure> {
    if cancel.is_cancelled() {
        Err(cancelled())
    } else {
        Ok(())
    }
}
fn ipc(error: io::Error) -> Failure {
    failure("ipc-closed", error)
}

fn directory(path: &Path) -> Result<(), Failure> {
    if !path.is_absolute()
        || path
            .components()
            .any(|c| matches!(c, Component::ParentDir | Component::CurDir))
    {
        return Err(failure(
            "invalid-storage-path",
            "Importer storage paths must be absolute, without dot components",
        ));
    }
    for parent in path.ancestors() {
        if parent.join(".git").exists() {
            return Err(failure(
                "checkout-output",
                "Third-party inputs and importer storage must stay outside Git checkouts",
            ));
        }
        match fs::symlink_metadata(parent) {
            Ok(meta) if meta.file_type().is_symlink() || !meta.is_dir() => {
                return Err(failure(
                    "invalid-storage-path",
                    "Storage ancestors must be real directories, without symlinks",
                ));
            }
            Ok(_) => {}
            Err(e) if e.kind() == io::ErrorKind::NotFound => {}
            Err(e) => return Err(failure("filesystem", e)),
        }
    }
    let mut builder = fs::DirBuilder::new();
    builder.recursive(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::DirBuilderExt;
        builder.mode(0o700);
    }
    builder.create(path).map_err(|e| failure("filesystem", e))
}
fn validate_origins(origins: &BTreeMap<String, String>) -> Result<(), Failure> {
    if origins.len() > 2048 {
        return Err(failure("origin-limit", "Too many stored site decisions"));
    }
    for (origin, decision) in origins {
        let parsed = url::Url::parse(origin).map_err(|e| failure("origin-decision", e))?;
        if !matches!(parsed.scheme(), "http" | "https")
            || parsed.origin().ascii_serialization() != *origin
            || decision.trim().is_empty()
            || decision.len() > 4096
        {
            return Err(failure(
                "origin-decision",
                "Stored decisions require an exact HTTP(S) origin and a bounded nonempty note",
            ));
        }
    }
    Ok(())
}

pub(super) fn initialize(
    paths: Paths,
    origins: BTreeMap<String, String>,
    runtime: &Path,
    cancel: &NetworkCancel,
    output: &Emitter,
) -> Result<Session, Failure> {
    check(cancel)?;
    validate_origins(&origins)?;
    let installation = Installation::load(runtime).map_err(|e| {
        failure(
            e.code,
            format!(
                "{}; reinstall the complete matching application bundle",
                e.message
            ),
        )
    })?;
    check(cancel)?;
    for path in [
        &paths.cache,
        &paths.staging,
        paths.library.parent().ok_or_else(|| {
            failure(
                "invalid-storage-path",
                "Managed library needs a parent directory",
            )
        })?,
    ] {
        directory(path)?;
    }
    // No JSON control/result files: only the SDK's own durable catalog/evidence.
    let mut store = match fs::symlink_metadata(&paths.library) {
        Ok(_) => Store::open(&paths.library),
        Err(e) if e.kind() == io::ErrorKind::NotFound => Store::create(&paths.library),
        Err(e) => return Err(failure("filesystem", e)),
    }
    .map_err(sdk_failure)?;
    store.recover().map_err(sdk_failure)?;
    snapshot(&store, None, cancel, output)?;
    Ok(Session {
        paths,
        installation: Arc::new(installation),
        origins: Arc::new(Mutex::new(origins)),
    })
}

fn snapshot(
    store: &Store,
    id: Option<u64>,
    cancel: &NetworkCancel,
    output: &Emitter,
) -> Result<(), Failure> {
    let snapshot = store.playback_snapshot().map_err(sdk_failure)?;
    stream_snapshot(snapshot, id, cancel, output)
}
fn stream_snapshot(
    snapshot: PlaybackSnapshot,
    id: Option<u64>,
    cancel: &NetworkCancel,
    output: &Emitter,
) -> Result<(), Failure> {
    // A completed commit must still be reported even after late cancellation;
    // snapshot publication itself is all-or-nothing through its End marker.
    if id.is_none() {
        check(cancel)?;
    }
    output
        .send(Event::SnapshotBegin {
            id,
            revision: snapshot.revision,
        })
        .map_err(ipc)?;
    let count = snapshot.songs.len();
    for song in snapshot.songs {
        if id.is_none() {
            check(cancel)?;
        }
        let entry =
            serde_json::to_value(song.entry).map_err(|e| failure("snapshot-serialization", e))?;
        output
            .send(Event::SnapshotSong {
                pack: song.pack,
                id,
                entry,
                directory: song.directory,
            })
            .map_err(ipc)?;
    }
    output.send(Event::SnapshotEnd { id, count }).map_err(ipc)
}

fn origin(
    session: &Session,
    id: u64,
    requested: &str,
    cancel: &NetworkCancel,
    pending: &Pending,
    output: &Emitter,
) -> Result<String, Error> {
    let origin = url::Url::parse(requested)
        .map_err(|e| Error {
            code: "invalid-url",
            message: e.to_string(),
        })?
        .origin()
        .ascii_serialization();
    if let Ok(origins) = session.origins.lock()
        && let Some(note) = origins.get(&origin)
    {
        return Ok(note.clone());
    }
    wait_origin(id, origin, cancel, pending, output).inspect(|note| {
        if let Ok(mut origins) = session.origins.lock() {
            origins.insert(
                url::Url::parse(requested)
                    .unwrap()
                    .origin()
                    .ascii_serialization(),
                note.clone(),
            );
        }
    })
}
fn wait_origin(
    id: u64,
    origin: String,
    cancel: &NetworkCancel,
    pending: &Pending,
    output: &Emitter,
) -> Result<String, Error> {
    let (tx, rx) = mpsc::sync_channel(1);
    {
        let mut waiting = pending.lock().map_err(|_| Error {
            code: "origin-decision",
            message: "Origin response lock poisoned".into(),
        })?;
        *waiting = Some(PendingOrigin {
            id,
            origin: origin.clone(),
            reply: tx,
        });
    }
    if let Err(e) = output.send(Event::OriginRequired {
        id,
        origin: origin.clone(),
    }) {
        if let Ok(mut waiting) = pending.lock() {
            waiting.take();
        }
        return Err(Error {
            code: "ipc-closed",
            message: e.to_string(),
        });
    }
    let deadline = Instant::now() + Duration::from_secs(300);
    let result = loop {
        if cancel.is_cancelled() {
            break Err(Error {
                code: "request-cancelled",
                message: "Import cancelled while awaiting site decision".into(),
            });
        }
        if Instant::now() >= deadline {
            break Err(Error {
                code: "origin-timeout",
                message: "No site decision received within five minutes".into(),
            });
        }
        match rx.recv_timeout(Duration::from_millis(25)) {
            Ok(Some(note)) if !note.trim().is_empty() && note.len() <= 4096 => break Ok(note),
            Ok(_) => {
                break Err(Error {
                    code: "site-terms-denied",
                    message: format!("Public-access approval denied for {origin}"),
                });
            }
            Err(mpsc::RecvTimeoutError::Timeout) => {}
            Err(mpsc::RecvTimeoutError::Disconnected) => {
                break Err(Error {
                    code: "site-terms-denied",
                    message: "Site approval channel closed".into(),
                });
            }
        }
    };
    if let Ok(mut waiting) = pending.lock() {
        waiting.take();
    }
    result
}

type ProgressView = (String, Option<(u64, u64)>);

fn progress_view(progress: &import_job::Progress) -> ProgressView {
    use ddi_danoni_export::{preparation, prepared_import};
    let counted = |label: &str, done: usize, total: usize| {
        (
            label.into(),
            (total > 0 && done <= total).then_some((done as u64, total as u64)),
        )
    };
    match progress {
        import_job::Progress::Preparing { detail: Some(p) } => {
            if let Some(p) = &p.preparation {
                let label = match p.stage {
                    preparation::Stage::Validate => "Checking extracted charts…",
                    preparation::Stage::Audio => "Converting music…",
                    preparation::Stage::Tempo => "Mapping chart timing…",
                    preparation::Stage::Artwork => "Making song artwork…",
                    preparation::Stage::Songs => "Preparing songs…",
                    preparation::Stage::Complete => "Finishing preparation…",
                };
                counted(label, p.completed, p.total)
            } else if p.stage == "writing" {
                ("Saving prepared songs…".into(), None)
            } else if p.source.as_ref().is_some_and(|s| s.stage == "native") {
                ("Reading chart data…".into(), None)
            } else {
                ("Downloading the work…".into(), None)
            }
        }
        import_job::Progress::Preparing { .. } => ("Opening the work…".into(), None),
        import_job::Progress::Validating { detail: Some(p) } => {
            let label = match p.stage {
                prepared_import::Stage::Receipts => "Checking the import…",
                prepared_import::Stage::Inventory => "Checking song files…",
                prepared_import::Stage::Replay => "Verifying converted charts…",
                prepared_import::Stage::Library => "Adding song details…",
                prepared_import::Stage::Complete => "Finishing checks…",
            };
            counted(label, p.completed, p.total)
        }
        import_job::Progress::Validating { .. } => ("Checking prepared songs…".into(), None),
        import_job::Progress::Planned { .. } => ("Checking existing imports…".into(), None),
        import_job::Progress::Installing { detail: p } => {
            let label = match p.stage {
                import_host::Stage::Begin => "Preparing library update…",
                import_host::Stage::Evidence => "Saving import details…",
                import_host::Stage::Songs => "Adding songs to your library…",
                import_host::Stage::ReadyToCommit => "Finishing import…",
            };
            counted(label, p.completed, p.total)
        }
    }
}

pub(super) fn import(
    session: Session,
    id: u64,
    url: String,
    offline: bool,
    cancel: &NetworkCancel,
    pending: &Pending,
    output: &Emitter,
) -> Report {
    let result = run_import(&session, id, &url, offline, cancel, pending, output);
    match result {
        Ok(report) => report,
        Err(error) => Report::failed(id, error),
    }
}
fn run_import(
    session: &Session,
    id: u64,
    url: &str,
    offline: bool,
    cancel: &NetworkCancel,
    pending: &Pending,
    output: &Emitter,
) -> Result<Report, Failure> {
    let parsed = url::Url::parse(url).map_err(|e| failure("invalid-url", e))?;
    if url.len() > 8192
        || !matches!(parsed.scheme(), "http" | "https")
        || parsed.host_str().is_none()
        || !parsed.username().is_empty()
        || parsed.password().is_some()
    {
        return Err(failure(
            "invalid-url",
            "Enter a complete HTTP(S) work URL without embedded credentials",
        ));
    }
    let mut store = Store::open(&session.paths.library).map_err(sdk_failure)?;
    store.recover().map_err(sdk_failure)?;
    let staging = session.paths.staging.join(format!(
        "ipc-{}-{id}-{}",
        std::process::id(),
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_err(|e| failure("clock", e))?
            .as_nanos()
    ));
    let assets = native::DirectorySource::new(BTreeMap::new()).map_err(sdk_failure)?;
    // Use the installed runtime, never paths selected by acquired content.
    // Desktop imports are explicit single-work downloads; research callers keep
    // the SDK's default robots policy. Installation was checked on Initialize.
    let mut request = session.installation.request(url, offline);
    request.preparation.source.fetch.robots_policy =
        ddi_danoni_export::network::RobotsPolicy::UserInitiated;
    let mut last_progress: Option<ProgressView> = None;
    let mut last_sent = Instant::now();
    let outcome = import_job::execute(
        &request,
        &import_job::Services {
            cache: &session.paths.cache,
            worker: &session.installation.worker,
            assets: &assets,
        },
        &staging,
        &mut store,
        cancel,
        &mut |url| origin(session, id, url, cancel, pending, output),
        &mut |progress| {
            let current = progress_view(progress);
            let stage_changed = last_progress
                .as_ref()
                .is_none_or(|(stage, _)| stage != &current.0);
            let complete = current.1.is_some_and(|(done, total)| done == total);
            if last_progress.as_ref() != Some(&current)
                && (stage_changed || complete || last_sent.elapsed() >= Duration::from_millis(100))
            {
                output
                    .send(Event::Progress {
                        id: Some(id),
                        stage: current.0.clone(),
                        progress: current.1,
                    })
                    .map_err(|e| Error {
                        code: "ipc-closed",
                        message: e.to_string(),
                    })?;
                last_progress = Some(current);
                last_sent = Instant::now();
            }
            Ok(())
        },
    );
    let mut report = match outcome {
        Ok(outcome) => {
            let restore_ids = outcome
                .review
                .plan
                .operations
                .iter()
                .map(|op| op.id.clone())
                .collect();
            match outcome.installation {
                import_host::Outcome::Installed {
                    added,
                    replaced,
                    skipped,
                    ..
                } => Report {
                    id,
                    status: Outcome::Installed,
                    summary: format!(
                        "DanOni import complete: {added} added, {replaced} replaced, {skipped} skipped."
                    ),
                    restore_ids,
                    details: Vec::new(),
                },
                import_host::Outcome::Unchanged { .. } => Report {
                    id,
                    status: Outcome::Unchanged,
                    summary: "These songs are already in your DanOni library.".into(),
                    restore_ids,
                    details: Vec::new(),
                },
                import_host::Outcome::NeedsRecovery { reason, .. } => Report {
                    id,
                    status: Outcome::NeedsRecovery,
                    summary: "Import needs library recovery.".into(),
                    restore_ids: Vec::new(),
                    details: vec![failure("recovery", reason).message],
                },
            }
        }
        Err(error) if error.code == "request-cancelled" => Report {
            id,
            status: Outcome::Cancelled,
            summary: "DanOni import cancelled before publication.".into(),
            restore_ids: Vec::new(),
            details: Vec::new(),
        },
        Err(error) => Report::failed(id, sdk_failure(error)),
    };
    // A post-commit refresh/notification error cannot undo or relabel publication.
    // Client retains the old snapshot unless the entire new stream completes.
    if let Err(error) = snapshot(&store, Some(id), cancel, output) {
        report.restore_ids.clear();
        report.details.push(format!(
            "Library refresh failed ({}): {}",
            error.code, error.message
        ));
    }
    Ok(report)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tests::output;
    type Prompt = (
        NetworkCancel,
        Pending,
        std::thread::JoinHandle<Result<String, Error>>,
        Arc<Mutex<Vec<u8>>>,
    );
    fn prompt() -> Prompt {
        let cancel = NetworkCancel::default();
        let token = cancel.clone();
        let pending = Arc::new(Mutex::new(None));
        let waiting = pending.clone();
        let (emitter, bytes) = output();
        let thread = std::thread::spawn(move || {
            wait_origin(
                4,
                "https://example.invalid".into(),
                &token,
                &waiting,
                &emitter,
            )
        });
        let end = Instant::now() + Duration::from_secs(5);
        while pending.lock().unwrap().is_none() {
            assert!(Instant::now() < end);
            std::thread::yield_now();
        }
        (cancel, pending, thread, bytes)
    }
    #[test]
    fn origin_prompt_accepts_explicit_reply_and_cancellation_unblocks_wait() {
        let (_, pending, thread, bytes) = prompt();
        pending
            .lock()
            .unwrap()
            .take()
            .unwrap()
            .reply
            .send(Some("User reviewed public-access terms".into()))
            .unwrap();
        assert_eq!(
            thread.join().unwrap().unwrap(),
            "User reviewed public-access terms"
        );
        let raw = bytes.lock().unwrap().clone();
        assert!(matches!(
            ddi_importer_protocol::read::<Event>(&mut raw.as_slice()).unwrap(),
            Some(Event::OriginRequired { id: 4, .. })
        ));
        let (cancel, pending, thread, _) = prompt();
        cancel.cancel();
        assert_eq!(
            thread.join().unwrap().unwrap_err().code,
            "request-cancelled"
        );
        assert!(pending.lock().unwrap().is_none());
    }
    #[test]
    fn origin_denial_is_not_an_allow_decision() {
        let (_, pending, thread, _) = prompt();
        pending
            .lock()
            .unwrap()
            .take()
            .unwrap()
            .reply
            .send(None)
            .unwrap();
        assert_eq!(
            thread.join().unwrap().unwrap_err().code,
            "site-terms-denied"
        );
    }
    #[test]
    fn progress_reports_real_stage_counts_and_leaves_unknown_work_indeterminate() {
        use ddi_danoni_export::{preparation, url_prepare};
        assert_eq!(
            progress_view(&import_job::Progress::Preparing { detail: None }).1,
            None
        );
        let progress = import_job::Progress::Preparing {
            detail: Some(url_prepare::Progress {
                stage: "preparation",
                source: None,
                preparation: Some(preparation::Progress {
                    stage: preparation::Stage::Audio,
                    completed: 2,
                    total: 5,
                }),
            }),
        };
        assert_eq!(
            progress_view(&progress),
            ("Converting music…".into(), Some((2, 5)))
        );
    }
    #[test]
    fn cancelled_late_snapshot_still_completes_and_keeps_original_metadata() {
        let entry = serde_json::json!({"id":"u-test", "title":"Original", "artist":"Test", "chart":"song.ssc", "music":"music.opus"});
        let original = serde_json::from_value(entry).unwrap();
        let before = serde_json::to_value(&original).unwrap();
        let snapshot = PlaybackSnapshot {
            revision: "revision".into(),
            songs: vec![ddi_danoni_export::managed_store::PlaybackSong {
                pack: Some("Collection".into()),
                entry: original,
                directory: "/immutable/revision/song".into(),
            }],
        };
        let cancel = NetworkCancel::default();
        cancel.cancel();
        let (emitter, bytes) = output();
        stream_snapshot(snapshot, Some(3), &cancel, &emitter).unwrap();
        let raw = bytes.lock().unwrap().clone();
        let mut reader = raw.as_slice();
        assert!(matches!(
            ddi_importer_protocol::read::<Event>(&mut reader).unwrap(),
            Some(Event::SnapshotBegin { id: Some(3), .. })
        ));
        assert!(
            matches!(ddi_importer_protocol::read::<Event>(&mut reader).unwrap(), Some(Event::SnapshotSong {entry,pack,..}) if entry == before && pack.as_deref() == Some("Collection"))
        );
        assert!(matches!(
            ddi_importer_protocol::read::<Event>(&mut reader).unwrap(),
            Some(Event::SnapshotEnd {
                id: Some(3),
                count: 1
            })
        ));
    }
    #[test]
    fn storage_and_saved_decisions_are_host_owned_and_bounded() {
        assert!(directory(Path::new("relative/output")).is_err());
        assert!(
            validate_origins(&BTreeMap::from([(
                "https://example.invalid/path".into(),
                "allow".into()
            )]))
            .is_err()
        );
        assert!(
            validate_origins(&BTreeMap::from([(
                "https://example.invalid".into(),
                "User reviewed public-access terms".into()
            )]))
            .is_ok()
        );
    }
}
