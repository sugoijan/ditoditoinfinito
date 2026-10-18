#[path = "../../rust/tests/support/import_fixture.rs"]
mod fixture;
use ddi_danoni_desktop_probe::{Job, audit, resume_job, snapshot};
use ddi_danoni_export::{
    import_host as host,
    import_job::{self, Candidate, Progress, Resume},
    managed_store::Store,
};
use std::{
    fs,
    path::{Path, PathBuf},
    time::{SystemTime, UNIX_EPOCH},
};

fn candidate(root: &Path, title: &str) -> Candidate {
    let (memory, binding) = fixture::fixture(&[0, 1], false, title);
    // Also exercise the shared admission fixture's typed validation.
    let _ = fixture::validated(&memory, &binding);
    for ((namespace, path), bytes) in memory.files {
        let p = root.join(namespace).join(path);
        fs::create_dir_all(p.parent().unwrap()).unwrap();
        fs::write(p, bytes).unwrap();
    }
    Candidate::bind(&root.join("candidate")).unwrap()
}
fn scratch() -> PathBuf {
    let root = std::env::temp_dir().join(format!(
        "ddi-desktop-handoff-{}-{}",
        std::process::id(),
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    fs::create_dir(&root).unwrap();
    root.canonicalize().unwrap()
}
#[test]
fn cancelled_commit_reopen_duplicate_and_replacement_keep_desktop_identity() {
    let root = scratch();
    let library = root.join("library");
    drop(Store::create(&library).unwrap());
    let first = candidate(&root.join("first"), "Synthetic desktop handoff");
    let cancelled = first.clone();
    let store_path = library.clone();
    let (ready_tx, ready_rx) = std::sync::mpsc::channel();
    let (release_tx, release_rx) = std::sync::mpsc::channel();
    let job = Job::spawn(move |cancel, progress| {
        let mut store = Store::open(&store_path)?;
        import_job::resume(&Resume::new(cancelled), &mut store, cancel, &mut |p| {
            progress(p)?;
            if matches!(p, Progress::Installing { detail } if detail.stage == host::Stage::ReadyToCommit) {
                ready_tx.send(()).unwrap();
                release_rx.recv_timeout(std::time::Duration::from_secs(30)).unwrap();
            }
            Ok(())
        })
    }).unwrap();
    ready_rx
        .recv_timeout(std::time::Duration::from_secs(30))
        .unwrap();
    assert!(
        !job.is_finished(),
        "Host retains control while import is pending"
    );
    assert!(job.take_progress().is_some());
    job.cancel.cancel();
    release_tx.send(()).unwrap();
    let failure = job.finish().unwrap_err();
    assert_eq!(failure.code, "request-cancelled", "{failure}");
    assert!(
        snapshot(&library).unwrap().1.is_empty(),
        "Uncommitted songs must be invisible"
    );
    let installed = resume_job(first.clone(), library.clone())
        .unwrap()
        .finish()
        .unwrap();
    assert!(matches!(
        installed.installation,
        host::Outcome::Installed { added: 1, .. }
    ));
    let (revision, songs) = snapshot(&library).unwrap();
    assert_eq!(songs.len(), 1);
    let id = songs[0].entry.id.clone();
    assert_eq!(id, installed.review.import.songs[0].entry.id);
    let before = audit(&songs).unwrap();
    assert_eq!(before["songs"][0]["charts"].as_array().unwrap().len(), 2);
    let duplicate = resume_job(first, library.clone())
        .unwrap()
        .finish()
        .unwrap();
    assert!(matches!(
        duplicate.installation,
        host::Outcome::Unchanged { .. }
    ));
    assert_eq!(snapshot(&library).unwrap().0, revision);

    let next = candidate(&root.join("next"), "Revised title");
    let path = library.clone();
    let replaced = Job::spawn(move |cancel, progress| {
        let mut request = Resume::new(next);
        request.duplicate_policy = host::DuplicatePolicy::Replace;
        import_job::resume(&request, &mut Store::open(&path)?, cancel, &mut |p| {
            progress(p)
        })
    })
    .unwrap()
    .finish()
    .unwrap();
    assert!(matches!(
        replaced.installation,
        host::Outcome::Installed { replaced: 1, .. }
    ));
    let (new_revision, updated) = snapshot(&library).unwrap();
    assert_ne!(revision, new_revision);
    assert_eq!(
        updated.len(),
        1,
        "Old transactions must not duplicate songs"
    );
    assert_eq!(updated[0].entry.id, id);
    assert_eq!(updated[0].entry.title, "Revised title");
    assert_ne!(updated[0].root, songs[0].root);
    audit(&updated).unwrap();
    audit(&songs).unwrap(); // Already-loaded snapshots stay playable after replacement.
    fs::write(updated[0].root.join(&updated[0].entry.chart), b"corrupted").unwrap();
    assert!(
        snapshot(&library).is_err(),
        "Do not register corrupt committed payloads"
    );
    fs::remove_dir_all(root).unwrap();
}
