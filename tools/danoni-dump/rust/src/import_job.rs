//! Blocking native URL → candidate → admission → library job coordinator.
//! No runtime/thread is hidden here; the desktop host queues work and owns I/O.
use crate::{
    Error, Result,
    assets::{self, AssetSource, Binding, JSON_LIMIT, sha256},
    import_host as host,
    managed_store::Store,
    native,
    network::NetworkCancel,
    prepared_import as admission,
    supervisor::WorkerSpec,
    url_prepare,
};
use serde::{Deserialize, Serialize};
use std::{
    collections::BTreeMap,
    path::{Path, PathBuf},
    time::Instant,
};
fn invalid(message: impl Into<String>) -> Error {
    Error::new("invalid-import-job", message)
}
fn wall() -> u64 {
    600000
}
const MAX_WALL_MS: u64 = 900000;
/// A hash-bound locator, not an admission proof. Resume always revalidates all
/// inputs; neither this object nor a saved outcome can instantiate an import.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Candidate {
    pub schema: String,
    pub directory: PathBuf,
    pub ready_sha256: String,
}
impl Candidate {
    /// Bind the current outer manifest. This does not approve its contents.
    pub fn bind(directory: &Path) -> Result<Self> {
        if directory.to_str().is_none() {
            return Err(invalid("Candidate path must be UTF-8"));
        }
        let directory = directory.canonicalize().map_err(native::io)?;
        let source = native::DirectorySource::new(BTreeMap::from([(
            "candidate".into(),
            directory.clone(),
        )]))?;
        let bytes = source.read("candidate", "manifest.json", JSON_LIMIT)?;
        let candidate = Self {
            schema: "ddi-danoni-import-candidate/1".into(),
            directory,
            ready_sha256: sha256(&bytes),
        };
        candidate.check()?;
        Ok(candidate)
    }
    pub fn read(path: &Path) -> Result<Self> {
        let path = path.canonicalize().map_err(native::io)?;
        let parent = path
            .parent()
            .ok_or_else(|| invalid("Candidate file parent"))?;
        let source =
            native::DirectorySource::new(BTreeMap::from([("token".into(), parent.to_path_buf())]))?;
        let name = path
            .file_name()
            .and_then(|n| n.to_str())
            .ok_or_else(|| invalid("Candidate file name"))?;
        let candidate: Self = assets::json(&source.read("token", name, JSON_LIMIT)?)?;
        candidate.check()?;
        Ok(candidate)
    }
    fn check(&self) -> Result<()> {
        if self.schema != "ddi-danoni-import-candidate/1"
            || !self.directory.is_absolute()
            || self.directory.to_str().is_none()
            || !assets::valid_hash(&self.ready_sha256)
        {
            return Err(invalid(
                "Need absolute UTF-8 candidate directory and exact Ready hash",
            ));
        }
        Ok(())
    }
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Request {
    pub preparation: url_prepare::Request,
    #[serde(default)]
    pub duplicate_policy: host::DuplicatePolicy,
    #[serde(default = "wall")]
    pub wall_ms: u64,
}
impl Request {
    pub fn new(url: impl Into<String>) -> Self {
        Self {
            preparation: url_prepare::Request::new(url),
            duplicate_policy: host::DuplicatePolicy::default(),
            wall_ms: wall(),
        }
    }
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Resume {
    pub candidate: Candidate,
    #[serde(default)]
    pub duplicate_policy: host::DuplicatePolicy,
    #[serde(default = "wall")]
    pub wall_ms: u64,
}
impl Resume {
    pub fn new(candidate: Candidate) -> Self {
        Self {
            candidate,
            duplicate_policy: host::DuplicatePolicy::default(),
            wall_ms: wall(),
        }
    }
}
/// All services are trusted host configuration, never selected by work data.
pub struct Services<'a, A: AssetSource> {
    pub cache: &'a Path,
    pub worker: &'a WorkerSpec,
    pub assets: &'a A,
}
/// Hosts with a database can supply this small extension to the transaction
/// contract. Snapshot/revision checks still belong to that same target store.
pub trait CatalogStore: host::LibraryStore {
    fn snapshot(&mut self) -> Result<host::Snapshot>;
}
impl CatalogStore for Store {
    fn snapshot(&mut self) -> Result<host::Snapshot> {
        Ok(self.catalog()?.snapshot())
    }
}
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Review {
    pub candidate: Candidate,
    pub import: admission::Summary,
    pub plan: host::PlanSummary,
}
#[derive(Debug, Serialize)]
#[serde(
    tag = "stage",
    rename_all = "kebab-case",
    rename_all_fields = "camelCase"
)]
pub enum Progress {
    Preparing {
        detail: Option<url_prepare::Progress>,
    },
    Validating {
        detail: Option<admission::Progress>,
    },
    Planned {
        review: Box<Review>,
    },
    Installing {
        detail: host::Progress,
    },
}
/// The final result is returned, never sent to a fallible progress callback.
/// A host may persist/notify this receipt without relabeling an installed import
/// if its own post-commit notification or result-file write fails.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Outcome {
    pub schema: &'static str,
    pub review: Review,
    pub installation: host::Outcome,
}
struct Gate<'a, F> {
    cancel: &'a NetworkCancel,
    started: Instant,
    wall_ms: u64,
    progress: &'a mut F,
}
impl<'a, F: FnMut(&Progress) -> Result<()>> Gate<'a, F> {
    fn new(cancel: &'a NetworkCancel, wall_ms: u64, progress: &'a mut F) -> Result<Self> {
        if tokio::runtime::Handle::try_current().is_ok() {
            return Err(invalid("Call on a blocking job thread"));
        }
        if wall_ms == 0 || wall_ms > MAX_WALL_MS {
            return Err(invalid("Job wallMs must be 1–900000"));
        }
        let g = Self {
            cancel,
            started: Instant::now(),
            wall_ms,
            progress,
        };
        g.check()?;
        Ok(g)
    }
    fn check(&self) -> Result<()> {
        if self.cancel.is_cancelled() {
            return Err(Error::new("request-cancelled", "Import job cancelled"));
        }
        if self.started.elapsed().as_millis() >= u128::from(self.wall_ms) {
            return Err(Error::new(
                "import-job-timeout",
                "Import job deadline exceeded before commit",
            ));
        }
        Ok(())
    }
    fn emit(&mut self, p: Progress) -> Result<()> {
        self.check()?;
        (self.progress)(&p)?;
        self.check()
    }
}
fn install(
    candidate: &Candidate,
    policy: host::DuplicatePolicy,
    store: &mut impl CatalogStore,
    gate: &mut Gate<'_, impl FnMut(&Progress) -> Result<()>>,
) -> Result<Outcome> {
    candidate.check()?;
    gate.emit(Progress::Validating { detail: None })?;
    let source = native::DirectorySource::new(BTreeMap::from([(
        "candidate".into(),
        candidate.directory.clone(),
    )]))?;
    let import = admission::validate(
        &Binding {
            namespace: "candidate".into(),
            path: "manifest.json".into(),
            sha256: candidate.ready_sha256.clone(),
        },
        &source,
        &mut |p| gate.emit(Progress::Validating { detail: Some(p) }),
    )?;
    gate.check()?;
    let snapshot = store.snapshot()?;
    gate.check()?;
    let plan = host::plan(&import, &snapshot, policy)?;
    // Allocate all result/review data before the publication boundary.
    let review = Review {
        candidate: candidate.clone(),
        import: import.summary().clone(),
        plan: plan.summary().clone(),
    };
    gate.emit(Progress::Planned {
        review: Box::new(review.clone()),
    })?;
    let installation = host::install(&import, &plan, store, &mut |p| {
        gate.emit(Progress::Installing { detail: p })
    })?;
    // Deliberately no cancellation, deadline, filesystem write or user callback
    // here: commit/uncertain commit/no-op is the terminal result.
    Ok(Outcome {
        schema: "ddi-danoni-import-job-result/1",
        review,
        installation,
    })
}
/// Resume only preparation-complete candidates. No fetch, worker installation,
/// font or source execution is needed. An uncertain prior install requires the
/// host's explicit store recovery first; this function never auto-recovers.
pub fn resume(
    request: &Resume,
    store: &mut impl CatalogStore,
    cancel: &NetworkCancel,
    progress: &mut impl FnMut(&Progress) -> Result<()>,
) -> Result<Outcome> {
    let mut gate = Gate::new(cancel, request.wall_ms, progress)?;
    install(
        &request.candidate,
        request.duplicate_policy,
        store,
        &mut gate,
    )
}
/// One native job from a URL to a managed installation. Output must be a fresh
/// external candidate directory; the target store must be initialized separately.
/// Retains the Ready candidate on installation conflict/cancellation/failure.
#[allow(clippy::too_many_arguments)]
pub fn execute(
    request: &Request,
    services: &Services<'_, impl AssetSource>,
    output: &Path,
    store: &mut impl CatalogStore,
    cancel: &NetworkCancel,
    site: &mut impl FnMut(&str) -> Result<String>,
    progress: &mut impl FnMut(&Progress) -> Result<()>,
) -> Result<Outcome> {
    let mut gate = Gate::new(cancel, request.wall_ms, progress)?;
    gate.emit(Progress::Preparing { detail: None })?;
    let prepared = url_prepare::execute(
        &request.preparation,
        services.cache,
        services.worker,
        services.assets,
        output,
        cancel,
        site,
        &mut |p| {
            gate.emit(Progress::Preparing {
                detail: Some(p.clone()),
            })
        },
    )?;
    if !prepared.ready {
        return Err(prepared
            .failure
            .unwrap_or_else(|| invalid("Preparation failed without a reason")));
    }
    let candidate = Candidate::bind(&prepared.output)?;
    // Persist a restart locator before touching the library. Failure leaves a
    // Ready directory the host can bind explicitly; it cannot mean installation.
    persist_candidate(&candidate)?;
    gate.check()?;
    install(&candidate, request.duplicate_policy, store, &mut gate)
}
fn persist_candidate(candidate: &Candidate) -> Result<()> {
    use std::io::Write;
    let bytes = serde_json::to_vec_pretty(candidate).map_err(|e| invalid(e.to_string()))?;
    let mut file = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(candidate.directory.join("import-candidate.json"))
        .map_err(native::io)?;
    file.write_all(&bytes).map_err(native::io)?;
    file.sync_all().map_err(native::io)?;
    std::fs::File::open(&candidate.directory)
        .map_err(native::io)?
        .sync_all()
        .map_err(native::io)
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Job {
    schema: String,
    #[serde(default)]
    roots: BTreeMap<String, PathBuf>,
    cache: PathBuf,
    origins: BTreeMap<String, String>,
    request: Request,
}
/// CLI configuration adapter. Store initialization and access decisions stay
/// explicit. The same coordinator executes the operation; no shell tools run.
pub fn execute_job(bundle: &Path, job: &Path, output: &Path, store: &Path) -> Result<Outcome> {
    let path = job.canonicalize().map_err(native::io)?;
    let dir = path.parent().ok_or_else(|| invalid("Job parent"))?;
    let input = native::DirectorySource::new(BTreeMap::from([("job".into(), dir.to_path_buf())]))?;
    let job: Job = assets::json(
        &input.read(
            "job",
            path.file_name()
                .and_then(|n| n.to_str())
                .ok_or_else(|| invalid("Job filename"))?,
            JSON_LIMIT,
        )?,
    )?;
    if job.schema != "ddi-danoni-import-job/1" {
        return Err(invalid("Unsupported job schema"));
    }
    let assets = native::DirectorySource::new(
        job.roots
            .into_iter()
            .map(|(k, p)| (k, if p.is_absolute() { p } else { dir.join(p) }))
            .collect(),
    )?;
    let worker = crate::worker_build::load_bundle(bundle, &mut || Ok(()))?;
    let mut store = Store::open(store)?;
    execute(
        &job.request,
        &Services {
            cache: &if job.cache.is_absolute() {
                job.cache
            } else {
                dir.join(job.cache)
            },
            worker: &worker,
            assets: &assets,
        },
        output,
        &mut store,
        &NetworkCancel::default(),
        &mut |url| {
            let origin = url::Url::parse(url)
                .map_err(|e| invalid(e.to_string()))?
                .origin()
                .ascii_serialization();
            job.origins.get(&origin).cloned().ok_or_else(|| {
                Error::new(
                    "site-terms-denied",
                    "Origin has no host-reviewed public-access decision",
                )
            })
        },
        &mut |_| Ok(()),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        import_fixture::fixture,
        import_host::{LibraryStore, Transaction},
    };
    use std::{
        fs,
        sync::atomic::{AtomicU64, Ordering},
    };
    struct Local(PathBuf);
    impl Local {
        fn new(label: &str) -> Self {
            static N: AtomicU64 = AtomicU64::new(0);
            let p = std::env::temp_dir().canonicalize().unwrap().join(format!(
                "ddi-import-job-{label}-{}-{}",
                std::process::id(),
                N.fetch_add(1, Ordering::Relaxed)
            ));
            fs::create_dir(&p).unwrap();
            Self(p)
        }
        fn candidate(
            &self,
            name: &str,
            selectors: &[usize],
            split: bool,
            title: &str,
        ) -> Candidate {
            let (m, _) = fixture(selectors, split, title);
            let root = self.0.join(name);
            fs::create_dir(&root).unwrap();
            for ((ns, path), bytes) in m.files {
                if ns == "candidate" {
                    let p = root.join(path);
                    fs::create_dir_all(p.parent().unwrap()).unwrap();
                    fs::write(p, bytes).unwrap();
                }
            }
            Candidate::bind(&root).unwrap()
        }
        fn store(&self) -> Store {
            Store::create(&self.0.join("library")).unwrap()
        }
    }
    impl Drop for Local {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }
    #[test]
    fn restart_token_installs_and_reopens_without_preparation_services() {
        let local = Local::new("resume");
        let c = local.candidate("ready", &[0, 1], true, "Restart");
        persist_candidate(&c).unwrap();
        let c = Candidate::read(&c.directory.join("import-candidate.json")).unwrap();
        let mut store = local.store();
        let mut events = vec![];
        let out = resume(
            &Resume::new(c.clone()),
            &mut store,
            &NetworkCancel::default(),
            &mut |p| {
                events.push(match p {
                    Progress::Preparing { .. } => "prepare",
                    Progress::Validating { .. } => "validate",
                    Progress::Planned { review } => {
                        assert_eq!(review.import.songs.len(), 2);
                        "plan"
                    }
                    Progress::Installing { .. } => "install",
                });
                Ok(())
            },
        )
        .unwrap();
        assert!(matches!(
            out.installation,
            host::Outcome::Installed { added: 2, .. }
        ));
        assert_eq!(out.review.import.musical_quality, "provisional");
        assert_eq!(events.first(), Some(&"validate"));
        assert_eq!(events.last(), Some(&"install"));
        assert!(!events.contains(&"prepare"));
        let serialized = serde_json::to_value(&out).unwrap();
        assert_eq!(serialized["schema"], "ddi-danoni-import-job-result/1");
        drop(store);
        let mut store = Store::open(&local.0.join("library")).unwrap();
        assert_eq!(store.verify().unwrap().songs.len(), 2);
        let out = resume(
            &Resume::new(c),
            &mut store,
            &NetworkCancel::default(),
            &mut |_| Ok(()),
        )
        .unwrap();
        assert!(matches!(out.installation, host::Outcome::Unchanged { .. }));
    }
    #[test]
    fn cancellation_at_each_phase_leaves_a_reusable_candidate() {
        for n in 0..8 {
            let local = Local::new("cancel");
            let c = local.candidate("ready", &[0, 1], true, "Cancel");
            let mut store = local.store();
            let cancel = NetworkCancel::default();
            let mut reached = false;
            let result = resume(&Resume::new(c.clone()), &mut store, &cancel, &mut |p| {
                let stop = match (n, p) {
                    (0, Progress::Validating { detail: None }) => true,
                    (1, Progress::Validating { detail: Some(p) }) => {
                        matches!(p.stage, admission::Stage::Inventory)
                    }
                    (2, Progress::Planned { .. }) => true,
                    (3, Progress::Installing { detail: p }) => p.stage == host::Stage::Begin,
                    (4, Progress::Installing { detail: p }) => p.stage == host::Stage::Evidence,
                    (5, Progress::Installing { detail: p }) => {
                        p.stage == host::Stage::Songs && p.completed == 0
                    }
                    (6, Progress::Installing { detail: p }) => {
                        p.stage == host::Stage::Songs && p.completed == 1
                    }
                    (7, Progress::Installing { detail: p }) => {
                        p.stage == host::Stage::ReadyToCommit
                    }
                    _ => false,
                };
                if stop {
                    reached = true;
                    cancel.cancel();
                }
                Ok(())
            });
            assert!(reached, "{n}");
            assert_eq!(result.unwrap_err().code, "request-cancelled");
            assert!(store.verify().unwrap().songs.is_empty());
            assert!(matches!(
                resume(
                    &Resume::new(c),
                    &mut store,
                    &NetworkCancel::default(),
                    &mut |_| Ok(())
                )
                .unwrap()
                .installation,
                host::Outcome::Installed { added: 2, .. }
            ));
        }
    }
    #[test]
    fn callback_errors_timeouts_and_entered_async_runtime_stop_before_writes() {
        let local = Local::new("control");
        let c = local.candidate("ready", &[0], false, "Control");
        let mut store = local.store();
        let out = resume(
            &Resume::new(c.clone()),
            &mut store,
            &NetworkCancel::default(),
            &mut |p| {
                if matches!(p, Progress::Planned { .. }) {
                    Err(Error::new("host-control", "stop"))
                } else {
                    Ok(())
                }
            },
        );
        assert_eq!(out.unwrap_err().code, "host-control");
        let cancel = NetworkCancel::default();
        cancel.cancel();
        let mut calls = 0;
        assert_eq!(
            resume(&Resume::new(c.clone()), &mut store, &cancel, &mut |_| {
                calls += 1;
                Ok(())
            })
            .unwrap_err()
            .code,
            "request-cancelled"
        );
        assert_eq!(calls, 0);
        let mut request = Resume::new(c.clone());
        request.wall_ms = 1;
        assert_eq!(
            resume(&request, &mut store, &NetworkCancel::default(), &mut |_| {
                std::thread::sleep(std::time::Duration::from_millis(3));
                Ok(())
            })
            .unwrap_err()
            .code,
            "import-job-timeout"
        );
        request.wall_ms = MAX_WALL_MS + 1;
        assert_eq!(
            resume(
                &request,
                &mut store,
                &NetworkCancel::default(),
                &mut |_| Ok(())
            )
            .unwrap_err()
            .code,
            "invalid-import-job"
        );
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();
        let error = runtime.block_on(async {
            resume(
                &Resume::new(c),
                &mut store,
                &NetworkCancel::default(),
                &mut |_| Ok(()),
            )
            .unwrap_err()
        });
        assert_eq!(error.code, "invalid-import-job");
        assert!(store.verify().unwrap().songs.is_empty());
    }
    #[test]
    fn changed_ready_failed_parents_and_malformed_tokens_never_install() {
        let local = Local::new("bindings");
        let c = local.candidate("ready", &[0], false, "Bound");
        let mut store = local.store();
        let manifest = c.directory.join("manifest.json");
        let original = fs::read(&manifest).unwrap();
        let mut changed = original.clone();
        changed.push(b' ');
        fs::write(&manifest, changed).unwrap();
        assert!(
            resume(
                &Resume::new(c.clone()),
                &mut store,
                &NetworkCancel::default(),
                &mut |_| Ok(())
            )
            .is_err()
        );
        let mut failed: serde_json::Value = serde_json::from_slice(&original).unwrap();
        failed["ready"] = serde_json::json!(false);
        failed["failure"] = serde_json::json!({"code":"cancelled"});
        fs::write(&manifest, serde_json::to_vec(&failed).unwrap()).unwrap();
        let failed = Candidate::bind(&c.directory).unwrap();
        assert!(
            resume(
                &Resume::new(failed),
                &mut store,
                &NetworkCancel::default(),
                &mut |_| Ok(())
            )
            .is_err()
        );
        let mut invalid = c;
        invalid.ready_sha256 = "bad".into();
        assert!(
            resume(
                &Resume::new(invalid),
                &mut store,
                &NetworkCancel::default(),
                &mut |_| Ok(())
            )
            .is_err()
        );
        assert!(store.verify().unwrap().songs.is_empty());
    }
    #[cfg(unix)]
    #[test]
    fn non_utf8_candidate_paths_are_rejected_before_installation() {
        use std::{ffi::OsString, os::unix::ffi::OsStringExt};
        let local = Local::new("encoding");
        let mut c = local.candidate("ready", &[0], false, "Encoding");
        let mut store = local.store();
        let path = local.0.join(OsString::from_vec(b"non-utf8-\xff".to_vec()));
        // macOS rejects such names at the filesystem level; exercise the
        // API's early argument check without needing to create that filename.
        assert_eq!(
            Candidate::bind(&path).unwrap_err().code,
            "invalid-import-job"
        );
        c.directory = path;
        assert_eq!(
            resume(
                &Resume::new(c),
                &mut store,
                &NetworkCancel::default(),
                &mut |_| Ok(())
            )
            .unwrap_err()
            .code,
            "invalid-import-job"
        );
        assert!(store.verify().unwrap().songs.is_empty());
    }
    #[test]
    fn duplicate_policy_and_selector_coverage_survive_resume() {
        let local = Local::new("policy");
        let a = local.candidate("a", &[0, 1], false, "A");
        let b = local.candidate("b", &[0, 1], false, "B");
        let partial = local.candidate("partial", &[0], false, "Partial");
        let mut store = local.store();
        resume(
            &Resume::new(a),
            &mut store,
            &NetworkCancel::default(),
            &mut |_| Ok(()),
        )
        .unwrap();
        let mut request = Resume::new(b);
        assert_eq!(
            resume(
                &request,
                &mut store,
                &NetworkCancel::default(),
                &mut |_| Ok(())
            )
            .unwrap_err()
            .code,
            "library-import-conflict"
        );
        request.duplicate_policy = host::DuplicatePolicy::KeepExisting;
        assert!(matches!(
            resume(
                &request,
                &mut store,
                &NetworkCancel::default(),
                &mut |_| Ok(())
            )
            .unwrap()
            .installation,
            host::Outcome::Unchanged { .. }
        ));
        request.duplicate_policy = host::DuplicatePolicy::Replace;
        assert!(matches!(
            resume(
                &request,
                &mut store,
                &NetworkCancel::default(),
                &mut |_| Ok(())
            )
            .unwrap()
            .installation,
            host::Outcome::Installed { replaced: 1, .. }
        ));
        request.candidate = partial;
        assert_eq!(
            resume(
                &request,
                &mut store,
                &NetworkCancel::default(),
                &mut |_| Ok(())
            )
            .unwrap_err()
            .code,
            "library-chart-removal"
        );
        assert_eq!(store.verify().unwrap().songs[0].entry.title, "B");
    }
    struct Fake {
        songs: Vec<host::InstalledSong>,
        revision: usize,
        mode: usize,
        commits: usize,
        rollbacks: usize,
        cancel: NetworkCancel,
    }
    struct Tx<'a> {
        store: &'a mut Fake,
        songs: Vec<host::InstalledSong>,
    }
    impl LibraryStore for Fake {
        type Transaction<'a> = Tx<'a>;
        fn check_revision(&mut self, e: &str) -> Result<()> {
            if self.revision.to_string() != e {
                Err(invalid("stale"))
            } else {
                Ok(())
            }
        }
        fn begin<'a>(&'a mut self, p: &host::PlanSummary) -> Result<Tx<'a>> {
            self.check_revision(&p.expected_catalog_revision)?;
            Ok(Tx {
                store: self,
                songs: vec![],
            })
        }
    }
    impl CatalogStore for Fake {
        fn snapshot(&mut self) -> Result<host::Snapshot> {
            Ok(host::Snapshot {
                revision: self.revision.to_string(),
                songs: self.songs.clone(),
            })
        }
    }
    impl Transaction for Tx<'_> {
        fn stage_evidence(&mut self, _: &admission::PreparedImport) -> Result<()> {
            Ok(())
        }
        fn stage_song(&mut self, s: &admission::PreparedSong, _: &host::Operation) -> Result<()> {
            let s = s.summary();
            self.songs.push(host::InstalledSong {
                id: s.entry.id.clone(),
                identity_sha256: s.identity_sha256.clone(),
                source_identity_sha256: s.source_identity_sha256.clone(),
                revision_sha256: s.revision_sha256.clone(),
                selectors: s.selectors.clone(),
            });
            Ok(())
        }
        fn commit(&mut self) -> host::CommitOutcome {
            self.store.commits += 1;
            self.store.cancel.cancel();
            if self.store.mode == 0 {
                return host::CommitOutcome::Rejected {
                    error: Error::new("backend-rejected", "test"),
                };
            }
            if self.store.mode >= 2 {
                self.store.songs = self.songs.clone();
                self.store.revision += 1;
            }
            if self.store.mode <= 2 {
                host::CommitOutcome::NeedsRecovery {
                    transaction_id: "fake-tx".into(),
                    reason: "uncertain".into(),
                }
            } else {
                host::CommitOutcome::Committed {
                    catalog_revision: self.store.revision.to_string(),
                }
            }
        }
        fn rollback(&mut self) -> Result<()> {
            self.store.rollbacks += 1;
            Ok(())
        }
    }
    #[test]
    fn uncertain_commit_is_never_retried_rolled_back_or_cancelled_afterward() {
        let local = Local::new("terminal");
        let c = local.candidate("ready", &[0], false, "Terminal");
        for mode in 0..4 {
            let cancel = NetworkCancel::default();
            let mut store = Fake {
                songs: vec![],
                revision: 0,
                mode,
                commits: 0,
                rollbacks: 0,
                cancel: cancel.clone(),
            };
            let out = resume(&Resume::new(c.clone()), &mut store, &cancel, &mut |_| {
                assert!(!cancel.is_cancelled());
                Ok(())
            });
            if mode == 0 {
                assert_eq!(out.unwrap_err().code, "backend-rejected");
                assert_eq!(store.rollbacks, 1);
            } else {
                let out = out.unwrap();
                assert_eq!(store.rollbacks, 0);
                if mode <= 2 {
                    assert!(matches!(
                        out.installation,
                        host::Outcome::NeedsRecovery { .. }
                    ));
                } else {
                    assert!(matches!(out.installation, host::Outcome::Installed { .. }));
                }
                serde_json::to_vec(&out).unwrap();
            }
            assert_eq!(store.commits, 1);
            assert_eq!(store.songs.len(), usize::from(mode >= 2));
        }
    }
    #[test]
    fn url_preparation_failure_does_not_touch_the_library() {
        let local = Local::new("source-fail");
        let mut store = local.store();
        let mut request = Request::new("https://example.org/work.html");
        request.preparation.wall_ms = 0;
        let worker = WorkerSpec {
            executable: local.0.join("not-executed"),
            sha256: "".into(),
            build_identity: local.0.join("not-read"),
            build_identity_sha256: "".into(),
        };
        let source = native::DirectorySource::new(BTreeMap::new()).unwrap();
        let output = local.0.join("output");
        let result = execute(
            &request,
            &Services {
                cache: &local.0.join("cache"),
                worker: &worker,
                assets: &source,
            },
            &output,
            &mut store,
            &NetworkCancel::default(),
            &mut |_| panic!("No fetch"),
            &mut |_| Ok(()),
        );
        assert_eq!(result.unwrap_err().code, "invalid-url-preparation");
        assert!(!output.exists());
        assert!(store.verify().unwrap().songs.is_empty());
    }
}
